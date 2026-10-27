//! SIMULATED llama-server for tests (plan §18.2: "a fake OpenAI-compatible server for
//! deterministic tests"). It is a real, separate process started by the real sidecar code, so
//! containment, cleared environment, port authentication, health, restart, streaming and
//! preemption are exercised end to end — only the model is fake. Never bundled with the app.
//!
//! It accepts the llama-server command line Tessara builds and **refuses to start** if any
//! hardening flag is missing. Test behaviour comes from `--fake-*` flags:
//!
//! | flag | effect |
//! |---|---|
//! | `--fake-load-ms N` | `/health` answers 503 (loading) for N ms |
//! | `--fake-exit-after N` | exit with code 3 after N chat requests |
//! | `--fake-hang-after N` | after N chat requests, accept connections but never answer |
//! | `--fake-crash-on-start` | exit with code 5 immediately |
//! | `--fake-token-ms N` / `--fake-tokens N` | streaming speed / length |
//! | `--fake-reply FILE` | message content (default `{"ok":true}`) |
//! | `--fake-echo` | reply with the user message |
//! | `--fake-env-file FILE` | write the names of all environment variables at start |
//! | `--fake-spawn-child FILE` | start a long-running grandchild and write its pid to FILE |
//!
//! `GET /fake/stats` (authenticated) returns request counters.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const REQUIRED: &[&str] = &[
    "--offline",
    "--no-webui",
    "--no-agent",
    "--no-ui-mcp-proxy",
    "--no-slots",
    "--no-cors-credentials",
];

#[derive(Default)]
struct Stats {
    chat: AtomicU64,
    completed: AtomicU64,
    aborted: AtomicU64,
    health: AtomicU64,
    unauthorized: AtomicU64,
}

struct Cfg {
    key: String,
    load_ms: u64,
    exit_after: Option<u64>,
    hang_after: Option<u64>,
    token_ms: u64,
    tokens: usize,
    reply: String,
    echo: bool,
    started: Instant,
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--fake-crash-on-start") {
        std::process::exit(5);
    }
    for f in REQUIRED {
        if !args.iter().any(|a| a == f) {
            eprintln!("missing hardening flag {f}");
            std::process::exit(2);
        }
    }
    if arg_value(&args, "--host").as_deref() != Some("127.0.0.1") {
        std::process::exit(2);
    }
    if let Some(f) = arg_value(&args, "--fake-env-file") {
        let names: Vec<String> = std::env::vars_os()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        std::fs::write(f, names.join("\n")).ok();
    }
    if let Some(f) = arg_value(&args, "--fake-spawn-child") {
        // Full path: the sidecar's environment has no PATH (it is cleared on purpose).
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let child = std::process::Command::new(format!(r"{root}\System32\PING.EXE"))
            .args(["-n", "600", "127.0.0.1"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("grandchild");
        std::fs::write(f, child.id().to_string()).ok();
        std::mem::forget(child);
    }
    let port: u16 = arg_value(&args, "--port").and_then(|p| p.parse().ok()).expect("--port");
    let num = |n: &str| arg_value(&args, n).and_then(|v| v.parse::<u64>().ok());
    let cfg = Arc::new(Cfg {
        key: arg_value(&args, "--api-key").expect("--api-key"),
        load_ms: num("--fake-load-ms").unwrap_or(0),
        exit_after: num("--fake-exit-after"),
        hang_after: num("--fake-hang-after"),
        token_ms: num("--fake-token-ms").unwrap_or(5),
        tokens: num("--fake-tokens").unwrap_or(20) as usize,
        reply: arg_value(&args, "--fake-reply")
            .map(|f| std::fs::read_to_string(f).expect("reply file"))
            .unwrap_or_else(|| "{\"ok\":true}".into()),
        echo: args.iter().any(|a| a == "--fake-echo"),
        started: Instant::now(),
    });
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(_) => std::process::exit(4), // port taken: what the real server does too
    };
    let stats = Arc::new(Stats::default());
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let (cfg, stats) = (cfg.clone(), stats.clone());
        std::thread::spawn(move || handle(conn, &cfg, &stats));
    }
}

struct Req {
    method: String,
    path: String,
    auth: Option<String>,
    body: Vec<u8>,
}

fn read_req(s: &TcpStream) -> Option<Req> {
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut len = 0usize;
    let mut auth = None;
    loop {
        let mut h = String::new();
        r.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':')?;
        if k.eq_ignore_ascii_case("content-length") {
            len = v.trim().parse().ok()?;
        }
        if k.eq_ignore_ascii_case("authorization") {
            auth = Some(v.trim().to_owned());
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some(Req {
        method,
        path,
        auth,
        body,
    })
}

fn respond(mut s: &TcpStream, status: &str, body: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).expect("json string")
}

fn handle(s: TcpStream, cfg: &Cfg, stats: &Stats) {
    let Some(req) = read_req(&s) else { return };
    if req.method == "GET" && req.path == "/health" {
        stats.health.fetch_add(1, Ordering::Relaxed);
        if let Some(n) = cfg.hang_after
            && stats.chat.load(Ordering::Relaxed) >= n
        {
            std::thread::sleep(Duration::from_secs(3600));
        }
        if cfg.started.elapsed() < Duration::from_millis(cfg.load_ms) {
            respond(
                &s,
                "503 Service Unavailable",
                "{\"error\":{\"message\":\"Loading model\"}}",
            );
        } else {
            respond(&s, "200 OK", "{\"status\":\"ok\"}");
        }
        return;
    }
    if req.auth.as_deref() != Some(&format!("Bearer {}", cfg.key)) {
        stats.unauthorized.fetch_add(1, Ordering::Relaxed);
        respond(&s, "401 Unauthorized", "{\"error\":{\"message\":\"Invalid API Key\"}}");
        return;
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/fake/stats") => {
            let b = format!(
                "{{\"chat\":{},\"completed\":{},\"aborted\":{},\"health\":{},\"unauthorized\":{}}}",
                stats.chat.load(Ordering::Relaxed),
                stats.completed.load(Ordering::Relaxed),
                stats.aborted.load(Ordering::Relaxed),
                stats.health.load(Ordering::Relaxed),
                stats.unauthorized.load(Ordering::Relaxed)
            );
            respond(&s, "200 OK", &b);
        }
        ("POST", "/v1/chat/completions") => chat(s, &req, cfg, stats),
        _ => respond(&s, "404 Not Found", "{\"error\":{\"message\":\"not found\"}}"),
    }
}

fn chat(mut s: TcpStream, req: &Req, cfg: &Cfg, stats: &Stats) {
    let n = stats.chat.fetch_add(1, Ordering::Relaxed) + 1;
    if let Some(h) = cfg.hang_after
        && n > h
    {
        std::thread::sleep(Duration::from_secs(3600));
    }
    let v: serde_json::Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(_) => return respond(&s, "400 Bad Request", "{\"error\":{\"message\":\"bad json\"}}"),
    };
    let stream = v.get("stream").and_then(|x| x.as_bool()).unwrap_or(false);
    let content = if cfg.echo {
        let user = &v["messages"][1]["content"];
        match user {
            serde_json::Value::String(t) => t.clone(),
            serde_json::Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        }
    } else {
        cfg.reply.clone()
    };
    if !stream {
        let body = format!(
            "{{\"id\":\"chatcmpl-fake\",\"choices\":[{{\"index\":0,\"finish_reason\":\"stop\",\"message\":{{\"role\":\"assistant\",\"content\":{}}}}}],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":{}}}}}",
            json_str(&content),
            cfg.tokens
        );
        respond(&s, "200 OK", &body);
        stats.completed.fetch_add(1, Ordering::Relaxed);
    } else {
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
        if s.write_all(head.as_bytes()).is_err() {
            stats.aborted.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // Split the reply into `tokens` pieces on char boundaries.
        let chars: Vec<char> = content.chars().collect();
        let per = chars.len().div_ceil(cfg.tokens.max(1)).max(1);
        let mut ok = true;
        for piece in chars.chunks(per) {
            std::thread::sleep(Duration::from_millis(cfg.token_ms));
            let text: String = piece.iter().collect();
            let ev = format!(
                "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":{}}},\"finish_reason\":null}}]}}\n\n",
                json_str(&text)
            );
            if write!(s, "{:x}\r\n{ev}\r\n", ev.len()).is_err() || s.flush().is_err() {
                ok = false;
                break;
            }
        }
        if ok {
            let last = "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n";
            ok = write!(s, "{:x}\r\n{last}\r\n0\r\n\r\n", last.len()).is_ok() && s.flush().is_ok();
        }
        if ok {
            stats.completed.fetch_add(1, Ordering::Relaxed);
        } else {
            stats.aborted.fetch_add(1, Ordering::Relaxed);
        }
    }
    if let Some(e) = cfg.exit_after
        && n >= e
    {
        std::thread::sleep(Duration::from_millis(50));
        std::process::exit(3);
    }
}
