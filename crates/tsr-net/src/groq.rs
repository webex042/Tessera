//! Groq OpenAI-compatible client (`openai/gpt-oss-20b`) isolated inside `tsr-net`.
//!
//! Security & privacy guarantees:
//! - Reuses `ureq` with Windows OS SChannel TLS (`NativeTls` + `PlatformVerifier`), proxies disabled,
//!   and zero redirects so Bearer credentials are never forwarded off-host.
//! - `GROQ_API_KEY` stays inside [`Zeroizing`] and is redacted from [`Debug`] / logs / errors.
//! - Defense-in-depth PII & secret masking (`tsr_guard::redact::mask_pii`) runs on system/user
//!   messages prior to serialization so names, emails, phones, addresses, DOBs, and credentials
//!   never leave the device.
//! - Free-tier rate limits (`20 RPM`, `700 RPD`, `6,000 TPM`, `150,000 TPD`) are tracked and
//!   enforced via [`tsr_store::quota::QuotaTracker`], reserving capacity for interactive `P0` calls.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tsr_core::Sensitive;
use tsr_guard::redact::mask_pii;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{CallPriority, ChatModel, ChatRequest, ChatResponse, ClientError};
use tsr_infer::http::HttpError;
use tsr_store::quota::{QuotaPriority, QuotaReject, QuotaTracker, estimate_request_tokens};
use zeroize::Zeroizing;

pub const DEFAULT_ENDPOINT: &str = "https://api.groq.com/openai/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "openai/gpt-oss-20b";
pub const DEFAULT_REASONING_EFFORT: &str = "low";

const MAX_RESPONSE_BYTES: u64 = 4 << 20;
const MAX_ERROR_BODY_BYTES: u64 = 64 << 10;
const MAX_CACHE_ENTRIES: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub struct GroqConfig {
    pub api_key: Zeroizing<String>,
    pub model: String,
    pub endpoint: String,
    pub reasoning_effort: String,
    pub max_completion_cap: u32,
    pub max_retries_p0: u32,
    pub max_sync_wait_ms: u64,
    pub cache_ttl_ms: i64,
    pub connect_timeout: Duration,
    pub recv_timeout: Duration,
    pub read_timeout: Duration,
    pub allow_loopback_http: bool,
}

impl std::fmt::Debug for GroqConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroqConfig")
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("max_completion_cap", &self.max_completion_cap)
            .finish()
    }
}

impl GroqConfig {
    pub fn from_env_or(api_key: Zeroizing<String>) -> Self {
        let model = std::env::var("GROQ_MODEL")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        let endpoint = std::env::var("TSR_GROQ_BASE_URL")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .map(|b| {
                let b = b.trim_end_matches('/');
                if b.ends_with("/chat/completions") {
                    b.to_owned()
                } else {
                    format!("{b}/chat/completions")
                }
            })
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_owned());
        let effort = std::env::var("GROQ_REASONING_EFFORT")
            .ok()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| matches!(s.as_str(), "low" | "medium" | "high"))
            .unwrap_or_else(|| DEFAULT_REASONING_EFFORT.to_owned());
        GroqConfig {
            api_key,
            model,
            endpoint,
            reasoning_effort: effort,
            max_completion_cap: 1_536,
            max_retries_p0: 2,
            max_sync_wait_ms: 4_500,
            cache_ttl_ms: 60_000,
            connect_timeout: Duration::from_secs(10),
            recv_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(30),
            allow_loopback_http: false,
        }
    }
}

struct CachedEntry {
    stored_at_ms: i64,
    content: Sensitive<String>,
    finish_reason: Option<String>,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

pub struct GroqClient {
    cfg: GroqConfig,
    agent: ureq::Agent,
    auth: Zeroizing<String>,
    quota: Arc<QuotaTracker>,
    cache: Mutex<HashMap<[u8; 32], CachedEntry>>,
}

impl std::fmt::Debug for GroqClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroqClient")
            .field("cfg", &self.cfg)
            .field("auth", &"[REDACTED]")
            .finish()
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn split_url(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = if authority.starts_with('[') {
        authority.split(']').next().map(|h| &h[1..])?
    } else {
        authority.rsplit_once(':').map_or(authority, |(h, _)| h)
    };
    Some((scheme, host))
}

fn is_loopback_url(url: &str) -> bool {
    matches!(split_url(url), Some(("http", "127.0.0.1" | "localhost" | "::1")))
}

fn validate_endpoint(cfg: &GroqConfig) -> Result<(), ClientError> {
    let Some((scheme, host)) = split_url(&cfg.endpoint) else {
        return Err(ClientError::Http(HttpError::BadHeader));
    };
    match scheme {
        "https" if !host.is_empty() => Ok(()),
        "http" if cfg.allow_loopback_http && is_loopback_url(&cfg.endpoint) => Ok(()),
        _ => Err(ClientError::Http(HttpError::BadHeader)),
    }
}

/// Adapt tessera strict JSON schemas into Groq OpenAI-compatible strict schemas:
/// - Remove sizing constraints (`minLength`, `maxLength`, `minItems`, `maxItems`, `minimum`, `maximum`)
///   rejected by Groq strict constrained decoding (Tessera's Rust parsers enforce them in code).
/// - Convert `"type": ["T", "null"]` into `"anyOf": [{"type": "T"}, {"type": "null"}]`.
/// - Guarantee `"additionalProperties": false` and complete `"required"` lists on every object.
pub fn normalize_groq_schema(v: &Value) -> Value {
    match v {
        Value::Object(obj) => {
            let mut out = Map::new();
            let mut has_props = false;
            for (k, val) in obj {
                if matches!(
                    k.as_str(),
                    "minLength" | "maxLength" | "minItems" | "maxItems" | "minimum" | "maximum"
                ) {
                    continue;
                }
                if k == "type"
                    && let Value::Array(arr) = val
                {
                    let variants: Vec<Value> = arr
                        .iter()
                        .filter_map(|t| t.as_str())
                        .map(|t| json!({ "type": t }))
                        .collect();
                    out.insert("anyOf".to_owned(), Value::Array(variants));
                    continue;
                }
                if k == "properties"
                    && let Value::Object(props) = val
                {
                    has_props = true;
                    let mut clean_props = Map::new();
                    for (pk, pv) in props {
                        clean_props.insert(pk.clone(), normalize_groq_schema(pv));
                    }
                    out.insert("properties".to_owned(), Value::Object(clean_props));
                    continue;
                }
                if matches!(k.as_str(), "items" | "additionalProperties") {
                    out.insert(k.clone(), normalize_groq_schema(val));
                    continue;
                }
                if matches!(k.as_str(), "anyOf" | "oneOf" | "allOf")
                    && let Value::Array(arr) = val
                {
                    out.insert(k.clone(), Value::Array(arr.iter().map(normalize_groq_schema).collect()));
                    continue;
                }
                out.insert(k.clone(), val.clone());
            }
            if has_props {
                out.insert("additionalProperties".to_owned(), Value::Bool(false));
                if let Some(Value::Object(props)) = out.get("properties") {
                    let req: Vec<Value> = props.keys().cloned().map(Value::String).collect();
                    out.insert("required".to_owned(), Value::Array(req));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(normalize_groq_schema).collect()),
        other => other.clone(),
    }
}

/// Parse durations such as `"7.66s"`, `"500ms"`, `"2m30.5s"`, or bare integer seconds `"3"`.
pub fn parse_groq_duration_ms(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(secs) = s.parse::<f64>()
        && secs.is_finite()
        && secs >= 0.0
    {
        return Some((secs * 1_000.0).ceil() as u64);
    }
    let mut total_ms = 0.0f64;
    let mut num = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        let val: f64 = num.parse().ok()?;
        num.clear();
        let mult = if c == 'm' && chars.peek() == Some(&'s') {
            chars.next();
            1.0
        } else {
            match c {
                'h' => 3_600_000.0,
                'm' => 60_000.0,
                's' => 1_000.0,
                _ => return None,
            }
        };
        total_ms += val * mult;
    }
    if !num.is_empty() {
        let val: f64 = num.parse().ok()?;
        total_ms += val * 1_000.0;
    }
    (total_ms >= 0.0).then_some(total_ms.ceil() as u64)
}

#[derive(Default)]
struct RateHeaders {
    retry_after_ms: Option<u64>,
    reset_tokens_ms: Option<u64>,
    reset_requests_ms: Option<u64>,
    remaining_requests: Option<u64>,
    remaining_tokens: Option<u64>,
}

fn read_rate_headers(headers: &ureq::http::HeaderMap) -> RateHeaders {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    RateHeaders {
        retry_after_ms: get("retry-after").and_then(parse_groq_duration_ms),
        reset_tokens_ms: get("x-ratelimit-reset-tokens").and_then(parse_groq_duration_ms),
        reset_requests_ms: get("x-ratelimit-reset-requests").and_then(parse_groq_duration_ms),
        remaining_requests: get("x-ratelimit-remaining-requests").and_then(|v| v.trim().parse().ok()),
        remaining_tokens: get("x-ratelimit-remaining-tokens").and_then(|v| v.trim().parse().ok()),
    }
}

fn extract_usage(v: &Value) -> (Option<u32>, Option<u32>, Option<u64>) {
    let u = v.get("usage").or_else(|| v.get("x_groq").and_then(|g| g.get("usage")));
    let get = |k: &str| {
        u.and_then(|u| u.get(k))
            .and_then(Value::as_u64)
            .map(|n| n.min(u32::MAX as u64) as u32)
    };
    let pt = get("prompt_tokens");
    let ct = get("completion_tokens");
    let total = u
        .and_then(|u| u.get("total_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| match (pt, ct) {
            (Some(p), Some(c)) => Some(p as u64 + c as u64),
            _ => None,
        });
    (pt, ct, total)
}

fn map_ureq_err(e: ureq::Error, cancel: &CancelToken) -> ClientError {
    if let Some(reason) = cancel.reason() {
        return ClientError::Http(HttpError::Cancelled(reason));
    }
    match e {
        ureq::Error::Timeout(_) => ClientError::Http(HttpError::Timeout),
        ureq::Error::Io(e) => ClientError::Http(HttpError::Io(e.kind())),
        _ => ClientError::Http(HttpError::Connect(io::ErrorKind::Other)),
    }
}

fn cancel_sleep(ms: u64, cancel: &CancelToken) -> Result<(), ClientError> {
    let deadline = Instant::now() + Duration::from_millis(ms);
    loop {
        if let Some(reason) = cancel.reason() {
            return Err(ClientError::Http(HttpError::Cancelled(reason)));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        std::thread::sleep((deadline - now).min(POLL_INTERVAL));
    }
}

impl GroqClient {
    pub fn new(cfg: GroqConfig, quota: Arc<QuotaTracker>) -> Result<Self, ClientError> {
        if cfg.api_key.trim().is_empty() {
            return Err(ClientError::Unauthorized);
        }
        validate_endpoint(&cfg)?;
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
        let agent = ureq::Agent::config_builder()
            .https_only(!cfg.allow_loopback_http)
            .max_redirects(0)
            .http_status_as_error(false)
            .proxy(None)
            .timeout_connect(Some(cfg.connect_timeout))
            .timeout_recv_response(Some(cfg.recv_timeout))
            .timeout_per_read(Some(cfg.read_timeout))
            .user_agent("Tessara-groq-client/1.0")
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into();
        let auth = Zeroizing::new(format!("Bearer {}", cfg.api_key.trim()));
        Ok(GroqClient {
            cfg,
            agent,
            auth,
            quota,
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn config(&self) -> &GroqConfig {
        &self.cfg
    }

    pub fn quota(&self) -> &Arc<QuotaTracker> {
        &self.quota
    }

    fn effective_max_completion_tokens(&self, req: &ChatRequest) -> u32 {
        // gpt-oss reasoning tokens count against `max_completion_tokens`, so add headroom while
        // respecting the application safety cap.
        if self.cfg.model.contains("gpt-oss") {
            req.max_tokens.saturating_add(512).min(self.cfg.max_completion_cap)
        } else {
            req.max_tokens.min(self.cfg.max_completion_cap)
        }
    }

    fn sanitize_and_build_body(&self, req: &ChatRequest) -> (String, String, u32, Zeroizing<Vec<u8>>, [u8; 32]) {
        // Guarantee full PII (names, emails, phones, addresses, DOBs) and secrets (passwords, cards,
        // API keys, JWTs, Aadhaar, PAN, OTPs) are masked before leaving the device across all
        // cloud workflows (ask, draft, extract, rollup).
        let sys_clean = mask_pii(req.system.expose()).text;
        let usr_clean = mask_pii(req.user.expose()).text;
        let max_completion = self.effective_max_completion_tokens(req);

        let mut body = json!({
            "model": self.cfg.model,
            "messages": [
                { "role": "system", "content": sys_clean },
                { "role": "user", "content": usr_clean },
            ],
            "temperature": req.temperature,
            "max_completion_tokens": max_completion,
            "stream": req.stream,
        });
        if self.cfg.model.contains("gpt-oss") {
            body["reasoning_effort"] = json!(self.cfg.reasoning_effort);
            body["include_reasoning"] = json!(false);
        }
        if req.stream {
            body["stream_options"] = json!({ "include_usage": true });
        }
        if let Some(seed) = req.seed {
            body["seed"] = json!(seed);
        }
        if let Some(schema) = &req.json_schema {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "out",
                    "strict": true,
                    "schema": normalize_groq_schema(schema),
                }
            });
        }

        let bytes = Zeroizing::new(serde_json::to_vec(&body).expect("groq json"));
        let mut h = Sha256::new();
        h.update(self.cfg.model.as_bytes());
        h.update(b"\0");
        h.update(sys_clean.as_bytes());
        h.update(b"\0");
        h.update(usr_clean.as_bytes());
        h.update(b"\0");
        h.update(req.temperature.to_bits().to_le_bytes());
        h.update(max_completion.to_le_bytes());
        if let Some(schema) = &req.json_schema {
            h.update(serde_json::to_string(schema).unwrap_or_default().as_bytes());
        }
        let key: [u8; 32] = h.finalize().into();
        (sys_clean, usr_clean, max_completion, bytes, key)
    }

    fn cache_get(&self, key: &[u8; 32], now: i64) -> Option<ChatResponse> {
        let mut g = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|_, v| now - v.stored_at_ms <= self.cfg.cache_ttl_ms);
        g.get(key).map(|e| ChatResponse {
            content: e.content.clone(),
            finish_reason: e.finish_reason.clone(),
            prompt_tokens: e.prompt_tokens,
            completion_tokens: e.completion_tokens,
            elapsed_ms: 0,
        })
    }

    fn cache_put(&self, key: [u8; 32], resp: &ChatResponse, now: i64) {
        if self.cfg.cache_ttl_ms <= 0 {
            return;
        }
        let mut g = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|_, v| now - v.stored_at_ms <= self.cfg.cache_ttl_ms);
        if g.len() >= MAX_CACHE_ENTRIES
            && let Some(oldest_k) = g.iter().min_by_key(|(_, v)| v.stored_at_ms).map(|(k, _)| *k)
        {
            g.remove(&oldest_k);
        }
        g.insert(
            key,
            CachedEntry {
                stored_at_ms: now,
                content: resp.content.clone(),
                finish_reason: resp.finish_reason.clone(),
                prompt_tokens: resp.prompt_tokens,
                completion_tokens: resp.completion_tokens,
            },
        );
    }

    fn complete(
        &self,
        req: &ChatRequest,
        priority: CallPriority,
        cancel: &CancelToken,
        mut on_delta: Option<&mut dyn FnMut(&str)>,
    ) -> Result<ChatResponse, ClientError> {
        if req.image_png.is_some() {
            return Err(ClientError::UnsupportedModality);
        }
        if let Some(reason) = cancel.reason() {
            return Err(ClientError::Http(HttpError::Cancelled(reason)));
        }

        let (sys_clean, usr_clean, max_completion, body_bytes, cache_key) = self.sanitize_and_build_body(req);
        let now = now_ms();
        if req.temperature <= 0.15
            && let Some(cached) = self.cache_get(&cache_key, now)
        {
            if let Some(cb) = on_delta.as_mut() {
                cb(cached.content.expose());
            }
            return Ok(cached);
        }

        let est_tokens = estimate_request_tokens(&sys_clean, &usr_clean, max_completion);
        let q_prio = match priority {
            CallPriority::Interactive => QuotaPriority::Interactive,
            CallPriority::Background => QuotaPriority::Background,
        };
        let max_attempts = if priority == CallPriority::Interactive {
            1 + self.cfg.max_retries_p0
        } else {
            1
        };

        for attempt in 0..max_attempts {
            if let Some(reason) = cancel.reason() {
                return Err(ClientError::Http(HttpError::Cancelled(reason)));
            }
            let t_now = now_ms();
            let reservation = match self.quota.try_reserve(q_prio, est_tokens, t_now) {
                Ok(r) => r,
                Err(QuotaReject::DailyExhausted) => return Err(ClientError::QuotaExhausted),
                Err(QuotaReject::BackgroundDeferred { retry_after_ms }) => {
                    return Err(ClientError::BackgroundDeferred { retry_after_ms });
                }
                Err(QuotaReject::RateLimited { retry_after_ms }) => {
                    if attempt + 1 < max_attempts && retry_after_ms <= self.cfg.max_sync_wait_ms {
                        cancel_sleep(retry_after_ms.max(100), cancel)?;
                        continue;
                    }
                    return Err(ClientError::RateLimited { retry_after_ms });
                }
            };

            let started = Instant::now();
            let resp = match self
                .agent
                .post(&self.cfg.endpoint)
                .header("Authorization", self.auth.as_str())
                .header("Content-Type", "application/json")
                .send(&body_bytes[..])
            {
                Ok(r) => r,
                Err(e) => {
                    self.quota.reconcile(reservation, None, false, None, false, now_ms());
                    let ce = map_ureq_err(e, cancel);
                    if attempt + 1 < max_attempts && !matches!(ce, ClientError::Http(HttpError::Cancelled(_))) {
                        let backoff = 400u64 * (1u64 << attempt.min(3));
                        cancel_sleep(backoff, cancel)?;
                        continue;
                    }
                    return Err(ce);
                }
            };

            let status = resp.status().as_u16();
            let rh = read_rate_headers(resp.headers());

            if status == 401 || status == 403 {
                self.quota.reconcile(reservation, None, false, None, false, now_ms());
                return Err(ClientError::Unauthorized);
            }

            if status == 429 {
                let mut err_buf = Vec::new();
                let _ = resp
                    .into_body()
                    .with_config()
                    .limit(MAX_ERROR_BODY_BYTES)
                    .reader()
                    .read_to_end(&mut err_buf);
                let err_txt = String::from_utf8_lossy(&err_buf).to_ascii_lowercase();
                let daily_exhausted = err_txt.contains("per day")
                    || err_txt.contains("tokens per day")
                    || err_txt.contains("requests per day")
                    || (rh.remaining_requests == Some(0) && rh.reset_requests_ms.is_some_and(|ms| ms > 3_600_000));
                let wait_ms = rh
                    .retry_after_ms
                    .or(rh.reset_tokens_ms)
                    .or(rh.reset_requests_ms)
                    .unwrap_or(2_000);
                self.quota
                    .reconcile(reservation, Some(0), true, Some(wait_ms), daily_exhausted, now_ms());
                if daily_exhausted {
                    return Err(ClientError::QuotaExhausted);
                }
                if attempt + 1 < max_attempts && wait_ms <= self.cfg.max_sync_wait_ms {
                    let jittered = wait_ms.saturating_add(150 * (attempt as u64 + 1));
                    cancel_sleep(jittered.min(self.cfg.max_sync_wait_ms), cancel)?;
                    continue;
                }
                return Err(ClientError::RateLimited {
                    retry_after_ms: wait_ms,
                });
            }

            if (500..600).contains(&status) {
                self.quota.reconcile(reservation, None, true, None, false, now_ms());
                if attempt + 1 < max_attempts {
                    let backoff = 500u64 * (1u64 << attempt.min(3));
                    cancel_sleep(backoff, cancel)?;
                    continue;
                }
                return Err(ClientError::Status(status));
            }

            if status != 200 {
                self.quota.reconcile(reservation, None, false, None, false, now_ms());
                return Err(ClientError::Status(status));
            }

            let cooldown_from_hdr = if rh.remaining_tokens == Some(0) {
                rh.reset_tokens_ms
            } else if rh.remaining_requests == Some(0) {
                rh.reset_requests_ms
            } else {
                None
            };

            if !req.stream {
                let mut buf = Zeroizing::new(Vec::new());
                let mut body = resp.into_body();
                let mut reader = body.with_config().limit(MAX_RESPONSE_BYTES).reader();
                if let Err(e) = reader.read_to_end(&mut buf) {
                    self.quota
                        .reconcile(reservation, None, true, cooldown_from_hdr, false, now_ms());
                    return Err(ClientError::Http(HttpError::Io(e.kind())));
                }
                if let Some(reason) = cancel.reason() {
                    self.quota
                        .reconcile(reservation, None, true, cooldown_from_hdr, false, now_ms());
                    return Err(ClientError::Http(HttpError::Cancelled(reason)));
                }
                let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
                    self.quota
                        .reconcile(reservation, None, true, cooldown_from_hdr, false, now_ms());
                    return Err(ClientError::BadResponse("json"));
                };
                let (pt, ct, total_u) = extract_usage(&v);
                self.quota
                    .reconcile(reservation, total_u, true, cooldown_from_hdr, false, now_ms());
                let choice = v
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .ok_or(ClientError::BadResponse("choices"))?;
                let content = choice
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(Value::as_str)
                    .ok_or(ClientError::BadResponse("message.content"))?;
                let finish_reason = choice.get("finish_reason").and_then(Value::as_str).map(str::to_owned);
                if let Some(cb) = on_delta.as_mut() {
                    cb(content);
                }
                let out = ChatResponse {
                    content: Sensitive::new(content.to_owned()),
                    finish_reason,
                    prompt_tokens: pt,
                    completion_tokens: ct,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                };
                if req.temperature <= 0.15 {
                    self.cache_put(cache_key, &out, now_ms());
                }
                return Ok(out);
            }

            // Streaming SSE response (`data: {json}\n\n` ... `data: [DONE]`).
            let mut body = resp.into_body();
            let reader = body.with_config().limit(MAX_RESPONSE_BYTES).reader();
            let mut br = BufReader::new(reader);
            let mut line_buf = Zeroizing::new(Vec::<u8>::new());
            let mut content = Zeroizing::new(String::new());
            let mut finish_reason: Option<String> = None;
            let mut tokens = (None, None, None);
            let mut done = false;
            let mut err_event = false;

            loop {
                if let Some(reason) = cancel.reason() {
                    let approx = tokens.2.or_else(|| Some((content.len() as u64).div_ceil(4)));
                    self.quota
                        .reconcile(reservation, approx, true, cooldown_from_hdr, false, now_ms());
                    return Err(ClientError::Http(HttpError::Cancelled(reason)));
                }
                line_buf.clear();
                match br.read_until(b'\n', &mut line_buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        let approx = tokens.2.or_else(|| Some((content.len() as u64).div_ceil(4)));
                        self.quota
                            .reconcile(reservation, approx, true, cooldown_from_hdr, false, now_ms());
                        return Err(ClientError::Http(HttpError::Io(e.kind())));
                    }
                }
                let Ok(text) = std::str::from_utf8(&line_buf) else {
                    self.quota
                        .reconcile(reservation, tokens.2, true, cooldown_from_hdr, false, now_ms());
                    return Err(ClientError::Http(HttpError::Malformed("sse utf-8")));
                };
                let text = text.trim_end_matches(['\r', '\n']);
                let Some(payload) = text.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim_start();
                if payload == "[DONE]" {
                    done = true;
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(payload) else {
                    self.quota
                        .reconcile(reservation, tokens.2, true, cooldown_from_hdr, false, now_ms());
                    return Err(ClientError::Http(HttpError::Malformed("sse json")));
                };
                if v.get("error").is_some() {
                    err_event = true;
                    continue;
                }
                if let Some(choice) = v.get("choices").and_then(Value::as_array).and_then(|a| a.first()) {
                    // Explicitly read ONLY `delta.content`; ignore any internal reasoning field.
                    if let Some(d) = choice
                        .get("delta")
                        .and_then(|d| d.get("content"))
                        .and_then(Value::as_str)
                        && !d.is_empty()
                    {
                        content.push_str(d);
                        if let Some(cb) = on_delta.as_mut() {
                            cb(d);
                        }
                    }
                    if let Some(fr) = choice.get("finish_reason").and_then(Value::as_str) {
                        finish_reason = Some(fr.to_owned());
                    }
                }
                let u = extract_usage(&v);
                if u.0.is_some() || u.2.is_some() {
                    tokens = u;
                }
            }

            self.quota
                .reconcile(reservation, tokens.2, true, cooldown_from_hdr, false, now_ms());
            if err_event {
                return Err(ClientError::BadResponse("error event"));
            }
            if !done && finish_reason.is_none() {
                return Err(ClientError::BadResponse("stream ended early"));
            }
            let out = ChatResponse {
                content: Sensitive::new(std::mem::take(&mut *content)),
                finish_reason,
                prompt_tokens: tokens.0,
                completion_tokens: tokens.1,
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
            if req.temperature <= 0.15 {
                self.cache_put(cache_key, &out, now_ms());
            }
            return Ok(out);
        }

        Err(ClientError::RateLimited { retry_after_ms: 5_000 })
    }
}

impl ChatModel for GroqClient {
    fn chat(&self, req: &ChatRequest, cancel: &CancelToken) -> Result<ChatResponse, ClientError> {
        self.complete(req, CallPriority::Interactive, cancel, None)
    }

    fn chat_streaming(
        &self,
        req: &ChatRequest,
        cancel: &CancelToken,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatResponse, ClientError> {
        self.complete(req, CallPriority::Interactive, cancel, Some(on_delta))
    }

    fn chat_with_priority(
        &self,
        req: &ChatRequest,
        priority: CallPriority,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ClientError> {
        self.complete(req, priority, cancel, None)
    }

    fn chat_streaming_with_priority(
        &self,
        req: &ChatRequest,
        priority: CallPriority,
        cancel: &CancelToken,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatResponse, ClientError> {
        self.complete(req, priority, cancel, Some(on_delta))
    }
}
