//! Loopback client for llama-server's OpenAI-compatible API (plan §7.3).
//!
//! - Refuses to exist for any address other than `127.0.0.1` / `::1`; there is no host name, so no
//!   DNS and no proxy can be involved.
//! - Sends the per-run API key as a Bearer token (the sidecar is started with `--api-key`).
//! - One request per connection, so a response can only belong to the request that opened it.
//! - Request and response sizes are capped; every field we use is type-checked; error values never
//!   carry response text.
//! - Streaming requests can be dropped mid-generation (preemption): closing the socket makes
//!   llama-server stop generating.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tsr_core::Sensitive;
use zeroize::Zeroizing;

use crate::cancel::CancelToken;
use crate::http::{self, HttpError, Limits, Request};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("refusing a non-loopback address")]
    NotLoopback,
    #[error("transport: {0}")]
    Http(#[from] HttpError),
    #[error("sidecar rejected the API key")]
    Unauthorized,
    #[error("model is still loading")]
    Loading,
    #[error("sidecar returned HTTP {0}")]
    Status(u16),
    #[error("unexpected response shape: {0}")]
    BadResponse(&'static str),
}

impl ClientError {
    /// Errors that mean "the process is gone or wedged" rather than "this request was bad".
    pub fn is_connection_problem(&self) -> bool {
        matches!(
            self,
            ClientError::Http(HttpError::Connect(_) | HttpError::Io(_) | HttpError::Timeout)
        )
    }
}

/// One chat request. Content fields are [`Sensitive`] so they cannot reach a log.
pub struct ChatRequest {
    pub system: Sensitive<String>,
    pub user: Sensitive<String>,
    /// PNG bytes, sent as a `data:` URL (plan §5.4 rung 3). Never written to disk.
    pub image_png: Option<Sensitive<Vec<u8>>>,
    pub temperature: f32,
    pub max_tokens: u32,
    /// JSON Schema for `response_format` (llama-server turns it into a grammar; code still validates).
    pub json_schema: Option<Value>,
    /// Stream the reply so the request can be preempted (P1/P2 jobs, plan §7.4).
    pub stream: bool,
    pub seed: Option<u64>,
}

impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRequest")
            .field("image", &self.image_png.is_some())
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("stream", &self.stream)
            .finish()
    }
}

#[derive(Debug)]
pub struct ChatResponse {
    pub content: Sensitive<String>,
    /// `stop`, `length`, ... (llama-server's own values; `length` means the output was cut).
    pub finish_reason: Option<String>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub elapsed_ms: u64,
}

/// What a caller of the model needs; implemented by [`LoopbackClient`] and by test doubles.
pub trait ChatModel: Send + Sync {
    fn chat(&self, req: &ChatRequest, cancel: &CancelToken) -> Result<ChatResponse, ClientError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Ready,
    Loading,
}

#[derive(Clone)]
pub struct LoopbackClient {
    addr: SocketAddr,
    auth: std::sync::Arc<Zeroizing<String>>,
    pub limits: Limits,
}

impl std::fmt::Debug for LoopbackClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopbackClient").field("addr", &self.addr).finish()
    }
}

pub fn is_allowed_addr(addr: &SocketAddr) -> bool {
    matches!(addr.ip(), IpAddr::V4(ip) if ip == Ipv4Addr::LOCALHOST)
        || matches!(addr.ip(), IpAddr::V6(ip) if ip == Ipv6Addr::LOCALHOST)
}

impl LoopbackClient {
    pub fn new(addr: SocketAddr, api_key: &str) -> Result<Self, ClientError> {
        if !is_allowed_addr(&addr) || addr.port() == 0 {
            return Err(ClientError::NotLoopback);
        }
        Ok(LoopbackClient {
            addr,
            auth: std::sync::Arc::new(Zeroizing::new(format!("Bearer {api_key}"))),
            limits: Limits::default(),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn health(&self, timeout: Duration) -> Result<Health, ClientError> {
        let limits = Limits {
            connect_timeout: timeout,
            idle_timeout: timeout,
            total_timeout: timeout,
            ..self.limits
        };
        let auth = self.auth.as_str();
        let r = http::send(
            self.addr,
            &Request {
                method: "GET",
                path: "/health",
                headers: &[("Authorization", auth)],
                body: None,
            },
            &limits,
            None,
        )?;
        match r.status {
            200 => {
                let v: Value = serde_json::from_slice(&r.body).map_err(|_| ClientError::BadResponse("health json"))?;
                match v.get("status").and_then(Value::as_str) {
                    Some("ok") => Ok(Health::Ready),
                    _ => Err(ClientError::BadResponse("health status")),
                }
            }
            503 => Ok(Health::Loading),
            401 | 403 => Err(ClientError::Unauthorized),
            s => Err(ClientError::Status(s)),
        }
    }

    fn body(req: &ChatRequest) -> Zeroizing<Vec<u8>> {
        let user = match &req.image_png {
            None => json!(req.user.expose()),
            Some(png) => {
                let url = Zeroizing::new(format!("data:image/png;base64,{}", base64(png.expose())));
                json!([
                    {"type": "image_url", "image_url": {"url": url.as_str()}},
                    {"type": "text", "text": req.user.expose()},
                ])
            }
        };
        let mut v = json!({
            "messages": [
                {"role": "system", "content": req.system.expose()},
                {"role": "user", "content": user},
            ],
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "stream": req.stream,
            "cache_prompt": true,
        });
        if let Some(seed) = req.seed {
            v["seed"] = json!(seed);
        }
        if let Some(schema) = &req.json_schema {
            v["response_format"] =
                json!({"type": "json_schema", "json_schema": {"name": "out", "strict": true, "schema": schema}});
        }
        Zeroizing::new(serde_json::to_vec(&v).expect("json"))
    }
}

fn status_error(status: u16) -> ClientError {
    match status {
        401 | 403 => ClientError::Unauthorized,
        503 => ClientError::Loading,
        s => ClientError::Status(s),
    }
}

fn usage(v: &Value) -> (Option<u32>, Option<u32>) {
    let u = v.get("usage");
    let get = |k: &str| {
        u.and_then(|u| u.get(k))
            .and_then(Value::as_u64)
            .map(|n| n.min(u32::MAX as u64) as u32)
    };
    (get("prompt_tokens"), get("completion_tokens"))
}

impl ChatModel for LoopbackClient {
    fn chat(&self, req: &ChatRequest, cancel: &CancelToken) -> Result<ChatResponse, ClientError> {
        let started = Instant::now();
        let body = Self::body(req);
        let auth = self.auth.as_str();
        let headers = [("Authorization", auth), ("Content-Type", "application/json")];
        let request = Request {
            method: "POST",
            path: "/v1/chat/completions",
            headers: &headers,
            body: Some(&body),
        };

        if !req.stream {
            let r = http::send(self.addr, &request, &self.limits, Some(cancel))?;
            if r.status != 200 {
                return Err(status_error(r.status));
            }
            let v: Value = serde_json::from_slice(&r.body).map_err(|_| ClientError::BadResponse("json"))?;
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
            let (prompt_tokens, completion_tokens) = usage(&v);
            return Ok(ChatResponse {
                content: Sensitive::new(content.to_owned()),
                finish_reason,
                prompt_tokens,
                completion_tokens,
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
        }

        // Server-sent events: `data: {json}\n\n` ... `data: [DONE]`.
        let mut line = Zeroizing::new(Vec::<u8>::new());
        let mut content = Zeroizing::new(String::new());
        let mut finish_reason = None;
        let mut tokens = (None, None);
        let mut done = false;
        let mut err_body = false;
        let (status, _) = http::send_stream(self.addr, &request, &self.limits, Some(cancel), &mut |data| {
            // A non-200 reply carries a JSON error body without `data:` lines, which is skipped below.
            for &b in data {
                if b != b'\n' {
                    line.push(b);
                    continue;
                }
                let l = std::mem::take(&mut *line);
                let l = Zeroizing::new(l);
                let text = std::str::from_utf8(&l).map_err(|_| HttpError::Malformed("sse utf-8"))?;
                let text = text.trim_end_matches('\r');
                let Some(payload) = text.strip_prefix("data:") else {
                    continue; // comments, `event:` lines, blank separators
                };
                let payload = payload.trim_start();
                if payload == "[DONE]" {
                    done = true;
                    continue;
                }
                let v: Value = serde_json::from_str(payload).map_err(|_| HttpError::Malformed("sse json"))?;
                if v.get("error").is_some() {
                    err_body = true;
                    continue;
                }
                if let Some(choice) = v.get("choices").and_then(Value::as_array).and_then(|a| a.first()) {
                    if let Some(d) = choice
                        .get("delta")
                        .and_then(|d| d.get("content"))
                        .and_then(Value::as_str)
                    {
                        content.push_str(d);
                    }
                    if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
                        finish_reason = Some(f.to_owned());
                    }
                }
                let u = usage(&v);
                if u.0.is_some() {
                    tokens = u;
                }
            }
            Ok(())
        })?;
        if status != 200 {
            return Err(status_error(status));
        }
        if err_body {
            return Err(ClientError::BadResponse("error event"));
        }
        if !done && finish_reason.is_none() {
            return Err(ClientError::BadResponse("stream ended early"));
        }
        Ok(ChatResponse {
            content: Sensitive::new(std::mem::take(&mut *content)),
            finish_reason,
            prompt_tokens: tokens.0,
            completion_tokens: tokens.1,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// Standard base64 (RFC 4648) for the image data URL.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_addresses_are_accepted() {
        for ok in ["127.0.0.1:8080", "[::1]:8080"] {
            assert!(LoopbackClient::new(ok.parse().unwrap(), "k").is_ok(), "{ok}");
        }
        for bad in [
            "0.0.0.0:8080",
            "192.168.1.5:8080",
            "10.0.0.1:80",
            "127.0.0.2:8080",
            "[::]:8080",
            "127.0.0.1:0",
        ] {
            assert_eq!(
                LoopbackClient::new(bad.parse().unwrap(), "k").unwrap_err(),
                ClientError::NotLoopback,
                "{bad}"
            );
        }
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }

    #[test]
    fn request_debug_hides_content() {
        let r = ChatRequest {
            system: Sensitive::new("sys secret".into()),
            user: Sensitive::new("user secret".into()),
            image_png: None,
            temperature: 0.0,
            max_tokens: 10,
            json_schema: None,
            stream: false,
            seed: None,
        };
        assert!(!format!("{r:?}").contains("secret"));
    }
}
