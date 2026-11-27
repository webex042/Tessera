//! Mocked integration tests for Groq (`openai/gpt-oss-20b`) client over a local loopback
//! `TcpListener` (no network access, no real API key).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tsr_core::Sensitive;
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::client::{CallPriority, ChatModel, ChatRequest, ClientError};
use tsr_infer::http::HttpError;
use tsr_infer::prompts::{ASK_V2, DRAFT_V1, EXTRACTION_V1, ROLLUP_V1};
use tsr_net::groq::{DEFAULT_MODEL, GroqClient, GroqConfig, normalize_groq_schema, parse_groq_duration_ms};
use tsr_store::keystore::open_store;
use tsr_store::quota::{QuotaLimits, QuotaTracker};
use zeroize::Zeroizing;

#[derive(Clone)]
struct MockReply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
    stream_chunks: Option<Vec<(String, u64)>>,
}

impl MockReply {
    fn ok_json(content: &str, prompt: u32, completion: u32) -> Self {
        let body = serde_json::json!({
            "id": "chatcmpl-mock",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": content },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": prompt,
                "completion_tokens": completion,
                "total_tokens": prompt + completion
            }
        })
        .to_string();
        Self {
            status: 200,
            headers: vec![("Content-Type", "application/json".into())],
            body,
            stream_chunks: None,
        }
    }

    fn status(status: u16, extra_headers: Vec<(&'static str, String)>, body: &str) -> Self {
        let mut headers = vec![("Content-Type", "application/json".into())];
        headers.extend(extra_headers);
        Self {
            status,
            headers,
            body: body.to_owned(),
            stream_chunks: None,
        }
    }

    fn sse(chunks: Vec<(String, u64)>) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("Content-Type", "text/event-stream".into()),
                ("Cache-Control", "no-cache".into()),
            ],
            body: String::new(),
            stream_chunks: Some(chunks),
        }
    }
}

struct CapturedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
    body: String,
}

struct MockServer {
    port: u16,
    hits: Arc<AtomicUsize>,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    stop: Arc<AtomicBool>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn spawn_mock<F>(responder: F) -> MockServer
where
    F: Fn(usize, &CapturedRequest) -> MockReply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let h2 = hits.clone();
    let c2 = captured.clone();
    let s2 = stop.clone();
    let resp_fn = Arc::new(responder);

    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if s2.load(Ordering::Relaxed) {
                break;
            }
            let Ok(stream) = conn else { continue };
            let h3 = h2.clone();
            let c3 = c2.clone();
            let f3 = resp_fn.clone();
            std::thread::spawn(move || {
                handle_conn(stream, h3, c3, &*f3);
            });
        }
    });

    MockServer {
        port,
        hits,
        captured,
        stop,
    }
}

fn handle_conn<F>(
    mut stream: TcpStream,
    hits: Arc<AtomicUsize>,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    responder: &F,
) where
    F: Fn(usize, &CapturedRequest) -> MockReply,
{
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(cloned) = stream.try_clone() else { return };
    let mut reader = BufReader::new(cloned);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut headers = Vec::new();
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_owned();
            if key == "content-length" {
                content_len = val.parse().unwrap_or(0);
            }
            headers.push((key, val));
        }
    }
    let mut body_bytes = vec![0u8; content_len];
    if content_len > 0 && reader.read_exact(&mut body_bytes).is_err() {
        return;
    }
    let req = CapturedRequest {
        request_line: request_line.trim().to_owned(),
        headers,
        body: String::from_utf8_lossy(&body_bytes).into_owned(),
    };
    let idx = hits.fetch_add(1, Ordering::SeqCst);
    let reply = responder(idx, &req);
    c3_push(&captured, req);

    let reason = match reply.status {
        200 => "OK",
        401 => "Unauthorized",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let mut head = format!("HTTP/1.1 {} {}\r\nConnection: close\r\n", reply.status, reason);
    for (k, v) in &reply.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    if reply.stream_chunks.is_none() {
        head.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
    }
    head.push_str("\r\n");
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    if let Some(chunks) = reply.stream_chunks {
        for (chunk, delay_ms) in chunks {
            if delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            if stream.write_all(chunk.as_bytes()).is_err() {
                return;
            }
            let _ = stream.flush();
        }
    } else {
        let _ = stream.write_all(reply.body.as_bytes());
        let _ = stream.flush();
    }
}

fn c3_push(captured: &Arc<Mutex<Vec<CapturedRequest>>>, req: CapturedRequest) {
    captured.lock().unwrap().push(req);
}

fn test_cfg(port: u16, key: &str) -> GroqConfig {
    GroqConfig {
        api_key: Zeroizing::new(key.to_owned()),
        model: DEFAULT_MODEL.to_owned(),
        endpoint: format!("http://127.0.0.1:{port}/openai/v1/chat/completions"),
        reasoning_effort: "low".to_owned(),
        max_completion_cap: 1_536,
        max_retries_p0: 2,
        max_sync_wait_ms: 2_000,
        cache_ttl_ms: 60_000,
        connect_timeout: Duration::from_secs(2),
        recv_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(5),
        allow_loopback_http: true,
    }
}

fn sample_req(user: &str) -> ChatRequest {
    ChatRequest {
        system: Sensitive::new("Return concise factual output.".into()),
        user: Sensitive::new(user.into()),
        image_png: None,
        temperature: 0.2,
        max_tokens: 256,
        json_schema: None,
        stream: false,
        seed: Some(7),
    }
}

#[test]
fn successful_non_stream_request_and_body_shape() {
    let srv = spawn_mock(|_, _| MockReply::ok_json("hello from mock", 18, 7));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_test_fake_key_1"), quota.clone()).unwrap();

    let resp = client.chat(&sample_req("Ping"), &CancelToken::new()).unwrap();
    assert_eq!(resp.content.expose(), "hello from mock");
    assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    assert_eq!(resp.prompt_tokens, Some(18));
    assert_eq!(resp.completion_tokens, Some(7));

    let reqs = srv.captured.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].request_line.starts_with("POST /openai/v1/chat/completions "));
    assert!(
        reqs[0]
            .headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer gsk_test_fake_key_1")
    );
    let body: Value = serde_json::from_str(&reqs[0].body).unwrap();
    assert_eq!(body["model"], "openai/gpt-oss-20b");
    assert_eq!(body["reasoning_effort"], "low");
    assert_eq!(body["include_reasoning"], false);
    assert_eq!(body["max_completion_tokens"], 768);
    assert_eq!(body["seed"], 7);
    assert!(body.get("cache_prompt").is_none());
    assert!(body.get("stream_options").is_none());

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let snap = quota.snapshot(now);
    assert_eq!(snap.day_requests, 1);
    assert_eq!(snap.day_tokens, 25);
}

#[test]
fn custom_model_selection_and_https_enforcement() {
    let default_cfg = GroqConfig::from_env_or(Zeroizing::new("gsk_default_check".into()));
    assert!(!default_cfg.allow_loopback_http);
    assert!(!default_cfg.model.is_empty());

    let mut plain_http = test_cfg(9999, "gsk_plain");
    plain_http.allow_loopback_http = false;
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    assert!(matches!(
        GroqClient::new(plain_http, quota.clone()),
        Err(ClientError::Http(HttpError::BadHeader))
    ));

    let srv = spawn_mock(|_, _| MockReply::ok_json("ok", 10, 2));
    let mut custom = test_cfg(srv.port, "gsk_custom");
    custom.model = "llama-3.3-70b-versatile".into();
    let client = GroqClient::new(custom, quota).unwrap();
    let _ = client.chat(&sample_req("Hello"), &CancelToken::new()).unwrap();

    let body: Value = serde_json::from_str(&srv.captured.lock().unwrap()[0].body).unwrap();
    assert_eq!(body["model"], "llama-3.3-70b-versatile");
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("include_reasoning").is_none());
}

#[test]
fn api_key_redacted_in_debug_and_errors() {
    let secret = "gsk_sentinel_key_never_appears_001122";
    let srv = spawn_mock(move |idx, _| {
        if idx == 0 {
            MockReply::status(401, vec![], &format!("{{\"error\":\"bad key {secret}\"}}"))
        } else {
            MockReply::status(500, vec![], &format!("upstream echo {secret}"))
        }
    });
    let cfg = test_cfg(srv.port, secret);
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(cfg.clone(), quota).unwrap();

    let dbg_cfg = format!("{cfg:?}");
    let dbg_cli = format!("{client:?}");
    assert!(!dbg_cfg.contains(secret));
    assert!(!dbg_cli.contains(secret));
    assert!(dbg_cfg.contains("[REDACTED]"));

    let e1 = client.chat(&sample_req("a"), &CancelToken::new()).unwrap_err();
    let e2 = client.chat(&sample_req("b"), &CancelToken::new()).unwrap_err();
    for msg in [format!("{e1}"), format!("{e1:?}"), format!("{e2}"), format!("{e2:?}")] {
        assert!(!msg.contains(secret), "secret leaked into error: {msg}");
    }
}

#[test]
fn sse_streaming_ignores_reasoning_and_reads_x_groq_usage() {
    let chunks = vec![
        ("data: {\"choices\":[{\"delta\":{\"reasoning\":\"hidden chain\"},\"finish_reason\":null}]}\n\n".into(), 0),
        ("data: {\"choices\":[{\"delta\":{\"content\":\"Alpha \"},\"finish_reason\":null}]}\n\n".into(), 0),
        ("data: {\"choices\":[{\"delta\":{\"content\":\"Beta\"},\"finish_reason\":\"stop\"}]}\n\n".into(), 0),
        ("data: {\"choices\":[],\"x_groq\":{\"usage\":{\"prompt_tokens\":14,\"completion_tokens\":3,\"total_tokens\":17}}}\n\n".into(), 0),
        ("data: [DONE]\n\n".into(), 0),
    ];
    let srv = spawn_mock(move |_, _| MockReply::sse(chunks.clone()));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_stream"), quota).unwrap();

    let mut req = sample_req("Stream test");
    req.stream = true;
    let mut deltas = Vec::new();
    let resp = client
        .chat_streaming(&req, &CancelToken::new(), &mut |d| deltas.push(d.to_owned()))
        .unwrap();

    assert_eq!(deltas, vec!["Alpha ", "Beta"]);
    assert_eq!(resp.content.expose(), "Alpha Beta");
    assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    assert_eq!(resp.prompt_tokens, Some(14));
    assert_eq!(resp.completion_tokens, Some(3));

    let body: Value = serde_json::from_str(&srv.captured.lock().unwrap()[0].body).unwrap();
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
}

fn assert_strict_groq_schema(v: &Value) {
    match v {
        Value::Object(map) => {
            for bad in ["minLength", "maxLength", "minItems", "maxItems", "minimum", "maximum"] {
                assert!(!map.contains_key(bad), "disallowed constraint left in schema: {bad}");
            }
            if let Some(Value::Array(_)) = map.get("type") {
                panic!("type array should have been converted to anyOf");
            }
            if let Some(Value::Object(props)) = map.get("properties") {
                assert_eq!(map.get("additionalProperties"), Some(&Value::Bool(false)));
                let req = map
                    .get("required")
                    .and_then(Value::as_array)
                    .expect("required list missing");
                for pk in props.keys() {
                    assert!(
                        req.iter().any(|r| r.as_str() == Some(pk.as_str())),
                        "property `{pk}` missing from required"
                    );
                }
            }
            for child in map.values() {
                assert_strict_groq_schema(child);
            }
        }
        Value::Array(arr) => arr.iter().for_each(assert_strict_groq_schema),
        _ => {}
    }
}

#[test]
fn structured_json_schema_normalization_for_tessera_prompts() {
    for prompt in [&*ASK_V2, &*DRAFT_V1, &*EXTRACTION_V1, &*ROLLUP_V1] {
        let raw = prompt.schema.as_ref().expect("prompt has schema");
        let normalized = normalize_groq_schema(raw);
        assert_strict_groq_schema(&normalized);
    }

    let srv = spawn_mock(|_, _| MockReply::ok_json("{\"answer\":\"yes\",\"citations\":[]}", 30, 8));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_schema"), quota).unwrap();
    let mut req = sample_req("Answer with schema");
    req.json_schema = ASK_V2.schema.clone();

    let _ = client.chat(&req, &CancelToken::new()).unwrap();
    let body: Value = serde_json::from_str(&srv.captured.lock().unwrap()[0].body).unwrap();
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert_strict_groq_schema(&body["response_format"]["json_schema"]["schema"]);
}

#[test]
fn http_401_fails_fast_without_retry_or_quota_burn() {
    let srv = spawn_mock(|_, _| MockReply::status(401, vec![], "{\"error\":\"invalid key\"}"));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_bad"), quota.clone()).unwrap();

    let err = client.chat(&sample_req("test 401"), &CancelToken::new()).unwrap_err();
    assert_eq!(err, ClientError::Unauthorized);
    assert_eq!(srv.hits.load(Ordering::SeqCst), 1);

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    assert_eq!(quota.snapshot(now).day_requests, 0);
}

#[test]
fn http_429_rate_limit_background_fails_without_retry() {
    let srv = spawn_mock(|_, _| {
        MockReply::status(
            429,
            vec![("Retry-After", "2".into())],
            "{\"error\":\"rate_limit_exceeded\"}",
        )
    });
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_429"), quota).unwrap();

    let err = client
        .chat_with_priority(&sample_req("bg 429"), CallPriority::Background, &CancelToken::new())
        .unwrap_err();
    assert!(matches!(err, ClientError::RateLimited { retry_after_ms } if retry_after_ms >= 2_000));
    assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn retry_after_duration_parsing_and_p0_backoff_recovery() {
    assert_eq!(parse_groq_duration_ms("100ms"), Some(100));
    assert_eq!(parse_groq_duration_ms("2m30.5s"), Some(150_500));
    assert_eq!(parse_groq_duration_ms("1h2m3s"), Some(3_723_000));
    assert_eq!(parse_groq_duration_ms("3"), Some(3_000));
    assert_eq!(parse_groq_duration_ms("bogus"), None);

    let srv = spawn_mock(move |idx, _| {
        if idx == 0 {
            MockReply::status(429, vec![("Retry-After", "80ms".into())], "{}")
        } else {
            MockReply::ok_json("recovered", 12, 4)
        }
    });
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_retry"), quota).unwrap();

    let resp = client.chat(&sample_req("recover"), &CancelToken::new()).unwrap();
    assert_eq!(resp.content.expose(), "recovered");
    assert_eq!(srv.hits.load(Ordering::SeqCst), 2);
}

#[test]
fn daily_quota_exhaustion_and_priority_deferral() {
    let srv = spawn_mock(|idx, _| MockReply::ok_json(&format!("ok-{idx}"), 10, 5));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits {
        rpm: 50,
        rpd: 10,
        tpm: 500_000,
        tpd: 500_000,
        background_daily_pct: 70,
        background_minute_pct: 65,
    }));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_quota"), quota).unwrap();

    for i in 0..7 {
        client
            .chat(&sample_req(&format!("p0-{i}")), &CancelToken::new())
            .unwrap();
    }
    let bg_err = client
        .chat_with_priority(
            &sample_req("bg-deferred"),
            CallPriority::Background,
            &CancelToken::new(),
        )
        .unwrap_err();
    assert!(matches!(bg_err, ClientError::BackgroundDeferred { .. }));
    assert_eq!(srv.hits.load(Ordering::SeqCst), 7);

    for i in 7..10 {
        client
            .chat(&sample_req(&format!("p0-{i}")), &CancelToken::new())
            .unwrap();
    }
    let exhausted = client
        .chat(&sample_req("p0-overflow"), &CancelToken::new())
        .unwrap_err();
    assert_eq!(exhausted, ClientError::QuotaExhausted);
    assert_eq!(srv.hits.load(Ordering::SeqCst), 10);
}

#[test]
fn persistent_sqlcipher_quota_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let srv = spawn_mock(|_, _| MockReply::ok_json("persisted", 20, 22));
    {
        let conn = open_store(dir.path()).unwrap();
        let quota = Arc::new(QuotaTracker::open(conn, QuotaLimits::default()));
        let client = GroqClient::new(test_cfg(srv.port, "gsk_sqlcipher"), quota).unwrap();
        let _ = client.chat(&sample_req("save"), &CancelToken::new()).unwrap();
    }

    let conn2 = open_store(dir.path()).unwrap();
    let quota2 = QuotaTracker::open(conn2, QuotaLimits::default());
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let snap = quota2.snapshot(now);
    assert_eq!(snap.day_requests, 1);
    assert_eq!(snap.day_tokens, 42);
}

#[test]
fn concurrent_requests_enforce_rpm_limit() {
    let srv = spawn_mock(|idx, _| MockReply::ok_json(&format!("c-{idx}"), 10, 5));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits {
        rpm: 3,
        rpd: 100,
        tpm: 500_000,
        tpd: 500_000,
        background_daily_pct: 70,
        background_minute_pct: 65,
    }));
    let mut cfg = test_cfg(srv.port, "gsk_conc");
    cfg.max_retries_p0 = 0;
    let client = Arc::new(GroqClient::new(cfg, quota).unwrap());

    let mut handles = Vec::new();
    for i in 0..6 {
        let c = client.clone();
        handles.push(std::thread::spawn(move || {
            c.chat(&sample_req(&format!("worker-{i}")), &CancelToken::new())
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let ok = results.iter().filter(|r| r.is_ok()).count();
    let limited = results
        .iter()
        .filter(|r| matches!(r, Err(ClientError::RateLimited { .. })))
        .count();
    assert_eq!(ok, 3);
    assert_eq!(limited, 3);
    assert_eq!(srv.hits.load(Ordering::SeqCst), 3);
}

#[test]
fn image_input_rejected_with_unsupported_modality() {
    let srv = spawn_mock(|_, _| MockReply::ok_json("should not happen", 1, 1));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_img"), quota).unwrap();

    let mut req = sample_req("describe image");
    req.image_png = Some(Sensitive::new(vec![137, 80, 78, 71]));
    let err = client.chat(&req, &CancelToken::new()).unwrap_err();
    assert_eq!(err, ClientError::UnsupportedModality);
    assert_eq!(srv.hits.load(Ordering::SeqCst), 0);
}

#[test]
fn mid_stream_cancellation_aborts_cleanly() {
    let chunks = vec![
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"part1\"}}]}\n\n".into(),
            0,
        ),
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"part2\"}}]}\n\n".into(),
            800,
        ),
        ("data: [DONE]\n\n".into(), 0),
    ];
    let srv = spawn_mock(move |_, _| MockReply::sse(chunks.clone()));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let mut cfg = test_cfg(srv.port, "gsk_cancel");
    cfg.read_timeout = Duration::from_millis(120);
    let client = GroqClient::new(cfg, quota).unwrap();

    let cancel = CancelToken::new();
    let c2 = cancel.clone();
    let mut req = sample_req("cancel mid stream");
    req.stream = true;

    let err = client
        .chat_streaming(&req, &cancel, &mut |_delta| {
            c2.cancel(CancelReason::Cancelled);
        })
        .unwrap_err();
    assert_eq!(err, ClientError::Http(HttpError::Cancelled(CancelReason::Cancelled)));
}

#[test]
fn pii_secret_redaction_and_ttl_cache_dedup() {
    let fake_gh = ["ghp_", "0123456789abcdefghijklmnopqrstuvwxyz"].concat();
    let srv = spawn_mock(|_, _| MockReply::ok_json("redacted ok", 12, 4));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let client = GroqClient::new(test_cfg(srv.port, "gsk_redact"), quota).unwrap();

    let mut req = sample_req(&format!("token is {fake_gh} on screen"));
    req.temperature = 0.0;

    let r1 = client.chat(&req, &CancelToken::new()).unwrap();
    let r2 = client.chat(&req, &CancelToken::new()).unwrap();
    assert_eq!(r1.content.expose(), "redacted ok");
    assert_eq!(r2.content.expose(), "redacted ok");
    assert_eq!(srv.hits.load(Ordering::SeqCst), 1);

    let sent_body = &srv.captured.lock().unwrap()[0].body;
    assert!(!sent_body.contains(&fake_gh), "secret leaked to outbound Groq payload");
    assert!(sent_body.contains("[API_KEY]"));
}

#[test]
fn full_pii_masking_across_all_groq_workflows() {
    let srv = spawn_mock(|_, _| MockReply::ok_json("{\"status\":\"ok\"}", 24, 8));
    let quota = Arc::new(QuotaTracker::in_memory(QuotaLimits::default()));
    let mut cfg = test_cfg(srv.port, "gsk_pii");
    cfg.cache_ttl_ms = 0;
    let client = GroqClient::new(cfg, quota).unwrap();

    let sensitive_input = concat!(
        "contact Sarah Jenkins (alice.miller@acme-corp.example) from +1-415-555-0192. ",
        "address: 742 Evergreen Terrace Street, DOB 1991-04-12, PAN ABCPK1234F, Aadhaar 2345 6789 0124."
    );

    for (prompt, tag) in [
        (&*ASK_V2, "ask"),
        (&*DRAFT_V1, "draft"),
        (&*EXTRACTION_V1, "extract"),
        (&*ROLLUP_V1, "rollup"),
    ] {
        let mut req = sample_req(&format!("[{tag}] {sensitive_input}"));
        req.system = Sensitive::new(prompt.system.to_string());
        req.temperature = 0.0;
        client.chat(&req, &CancelToken::new()).unwrap();
    }

    let captured = srv.captured.lock().unwrap();
    assert_eq!(captured.len(), 4);
    for (idx, req) in captured.iter().enumerate() {
        let body = &req.body;
        assert!(!body.contains("Sarah Jenkins"), "person name leaked in call {idx}");
        assert!(
            !body.contains("alice.miller@acme-corp.example"),
            "email leaked in call {idx}"
        );
        assert!(!body.contains("415-555-0192"), "phone leaked in call {idx}");
        assert!(!body.contains("742 Evergreen Terrace"), "address leaked in call {idx}");
        assert!(!body.contains("1991-04-12"), "DOB leaked in call {idx}");
        assert!(!body.contains("ABCPK1234F"), "PAN leaked in call {idx}");
        assert!(!body.contains("2345 6789 0124"), "Aadhaar leaked in call {idx}");
        assert!(body.contains("[PERSON_01]"), "person mask token missing in call {idx}");
        assert!(body.contains("[EMAIL]"), "email mask token missing in call {idx}");
        assert!(body.contains("[PHONE]"), "phone mask token missing in call {idx}");
        assert!(body.contains("[ADDRESS]"), "address mask token missing in call {idx}");
        assert!(body.contains("[DATE_OF_BIRTH]"), "DOB mask token missing in call {idx}");
        assert!(body.contains("[PAN]"), "PAN mask token missing in call {idx}");
        assert!(body.contains("[AADHAAR]"), "Aadhaar mask token missing in call {idx}");
    }
}
