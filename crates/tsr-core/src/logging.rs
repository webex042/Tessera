//! Content-free logging (plan §6.6).
//!
//! A `tracing` layer that writes one JSON line per event and keeps only:
//! - the level and target (module path);
//! - the message, **only if it is one of [`ALLOWED_MESSAGES`]** (fixed, reviewed strings) —
//!   otherwise the line says `"msg":null` and names the call site so it can be fixed;
//! - fields whose **name is in [`ALLOWED_FIELDS`]** and whose value is a number, a bool, or a
//!   string of at most 160 printable ASCII characters.
//!
//! Everything else is dropped and counted in `"dropped"`. Screen text therefore cannot reach a
//! log by accident: it would need an allowlisted name *and* to be short ASCII *and* to bypass
//! `Sensitive<T>`, which formats as `<redacted>`. Adding a message or field name means editing
//! this file, which is a reviewed change; `tests::workspace_tracing_calls_are_allowlisted`
//! checks every `tracing::` call in the workspace against both lists.

use std::fmt::{self, Write as _};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// Fixed event messages that may be written. Keep them free of data; put data in fields.
pub const ALLOWED_MESSAGES: &[&str] = &[
    // tsr-infer
    "gpu adapter",
    "backend probe",
    "backend selection",
    "backend selected",
    "started after fallback",
    "backend initialisation failed; trying next",
    "llama.cpp build failed integrity check",
    "sidecar ready",
    "sidecar launch failed",
    "sidecar exited",
    "sidecar hung",
    "sidecar restart scheduled",
    "sidecar restart budget exhausted",
    "sidecar unloaded",
    "job preempted",
    // tsr-net
    "model file downloaded",
    // tsr-pipeline
    "extraction finished",
    "extraction failed",
    // tsr-capture
    "uia: window falls back to OCR",
    // tsr-store
    "store opened",
    "store key created",
    "store write failed",
];

/// Field names that may be written. Values must still be numbers, bools or short ASCII.
/// These carry hardware facts, counts, durations, enum values and error codes — never content.
pub const ALLOWED_FIELDS: &[&str] = &[
    // hardware / backends
    "index",
    "adapter_name",
    "vendor",
    "dedicated_mib",
    "shared_mib",
    "software",
    "variant",
    "backend",
    "devices",
    "device",
    "fallbacks",
    "candidate",
    "selected",
    "failed",
    "problems",
    "first",
    "note",
    // generic, content-free by convention
    "ms",
    "count",
    "reason",
    "error",
    "error_code",
    "kind",
    "rule",
    "status",
    "version",
    "strikes",
    "cooldown_s",
    "schema_version",
    "items",
    "bytes",
    "stored",
    "merged",
    "rejected",
    "needs_review",
];

const MAX_STR: usize = 160;

#[derive(Default)]
struct Collected {
    msg: Option<String>,
    fields: Vec<(&'static str, String)>,
    dropped: u32,
}

fn safe_str(s: &str) -> bool {
    s.len() <= MAX_STR && s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

impl Visit for Collected {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.text(field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        // Formatting happens here, but the result is only kept if the field is allowlisted.
        if field.name() == "message" || ALLOWED_FIELDS.contains(&field.name()) {
            let mut s = String::new();
            let _ = write!(s, "{value:?}");
            self.text(field, &s);
        } else {
            self.dropped += 1;
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.number(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.number(field, value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.number(
            field,
            if value.is_finite() {
                format!("{value}")
            } else {
                "null".into()
            },
        );
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.number(field, value.to_string());
    }
}

impl Collected {
    fn text(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            if ALLOWED_MESSAGES.contains(&value) {
                self.msg = Some(value.to_owned());
            } else {
                self.dropped += 1;
            }
        } else if ALLOWED_FIELDS.contains(&field.name()) && safe_str(value) {
            self.fields.push((field.name(), json_str(value)));
        } else {
            self.dropped += 1;
        }
    }

    fn number(&mut self, field: &Field, value: String) {
        if ALLOWED_FIELDS.contains(&field.name()) {
            self.fields.push((field.name(), value));
        } else {
            self.dropped += 1;
        }
    }
}

/// The layer. Writes to any `Write` (a log file in the app, a buffer in tests).
pub struct ContentFreeLayer {
    sink: Mutex<Box<dyn Write + Send>>,
}

impl ContentFreeLayer {
    pub fn new(sink: impl Write + Send + 'static) -> Self {
        ContentFreeLayer {
            sink: Mutex::new(Box::new(sink)),
        }
    }
}

impl<S: Subscriber> Layer<S> for ContentFreeLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut c = Collected::default();
        event.record(&mut c);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut line = format!(
            "{{\"ts\":{ts},\"level\":\"{}\",\"target\":{},\"msg\":{}",
            meta.level(),
            json_str(meta.target()),
            c.msg.as_deref().map_or("null".to_string(), json_str)
        );
        if c.msg.is_none() {
            // Point at the call site (static, from the binary) so the message can be fixed.
            let _ = write!(
                line,
                ",\"callsite\":{}",
                json_str(&format!("{}:{}", meta.file().unwrap_or("?"), meta.line().unwrap_or(0)))
            );
        }
        line.push_str(",\"fields\":{");
        for (i, (k, v)) in c.fields.iter().enumerate() {
            if i > 0 {
                line.push(',');
            }
            let _ = write!(line, "\"{k}\":{v}");
        }
        let _ = write!(line, "}},\"dropped\":{}}}", c.dropped);
        if let Ok(mut w) = self.sink.lock() {
            let _ = writeln!(w, "{line}");
        }
    }
}

/// Install the layer as the global subscriber. Call once at start-up.
pub fn init(sink: impl Write + Send + 'static) -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
    use tracing_subscriber::layer::SubscriberExt;
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(ContentFreeLayer::new(sink)))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt;

    use super::*;
    use crate::Sensitive;

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture(f: impl FnOnce()) -> String {
        let buf = Buf::default();
        let sub = tracing_subscriber::registry().with(ContentFreeLayer::new(buf.clone()));
        tracing::subscriber::with_default(sub, f);
        String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
    }

    const CANARY: &str = "TSRCANARY-7f3a91c2-log";

    /// Stage 1 criterion: tests that fail if a content string reaches a log.
    #[test]
    fn content_never_reaches_the_log() {
        let screen_text = format!("Send the budget to Ravi {CANARY}");
        let secret = Sensitive::new(screen_text.clone());
        let out = capture(|| {
            // Content in the message (formatted), in unknown fields, in allowlisted fields
            // via Sensitive, as Debug, and as long or non-ASCII strings.
            tracing::info!("captured {}", screen_text);
            tracing::info!(text = %screen_text, title = ?screen_text, "gpu adapter");
            tracing::warn!(reason = %secret, error = ?secret, "backend selected");
            tracing::info!(note = %format!("{}{}", "x".repeat(200), CANARY), "backend selection");
            tracing::info!(reason = "कल सुबह भेज दूँगी", "backend selection");
            tracing::error!(count = 3, ms = 12u64, backend = "vulkan", "backend selected");
        });
        assert!(!out.contains("TSRCANARY"), "{out}");
        assert!(!out.contains("Ravi"), "{out}");
        assert!(!out.contains("सुबह"), "{out}");
        // Redacted values may appear; that is the point of Sensitive.
        assert!(out.contains("\"reason\":\"<redacted>\""), "{out}");
        // Allowlisted, content-free data still gets through.
        assert!(
            out.contains("\"backend\":\"vulkan\"") && out.contains("\"count\":3") && out.contains("\"ms\":12"),
            "{out}"
        );
        // Unknown message: null plus call site.
        assert!(
            out.lines().next().unwrap().contains("\"msg\":null") && out.contains("logging.rs:"),
            "{out}"
        );
    }

    #[test]
    fn dropped_fields_are_counted() {
        let out = capture(|| tracing::info!(text = "x", title = 5, backend = "cpu", "backend selected"));
        assert!(out.contains("\"dropped\":2"), "{out}");
        assert!(out.contains("\"msg\":\"backend selected\""), "{out}");
    }

    /// Every `tracing::` call in the workspace uses an allowlisted message and field names.
    #[test]
    fn workspace_tracing_calls_are_allowlisted() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut problems = Vec::new();
        let mut calls = 0;
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("logging.rs") {
                    let src = std::fs::read_to_string(&p).unwrap();
                    for (start, _) in src.match_indices("tracing::") {
                        let rest = &src[start + 9..];
                        let Some(bang) = rest.find("!(") else { continue };
                        let mac = &rest[..bang];
                        if !["info", "warn", "error", "debug", "trace"].contains(&mac) {
                            continue;
                        }
                        let body = &rest[bang + 2..rest.find(");").unwrap_or(rest.len())];
                        calls += 1;
                        // Message = last string literal; fields = `name =` outside literals.
                        let mut in_str = false;
                        let mut outside = String::new();
                        let mut last_lit = String::new();
                        let mut cur = String::new();
                        let mut prev = ' ';
                        for ch in body.chars() {
                            if ch == '"' && prev != '\\' {
                                if in_str {
                                    last_lit = cur.clone();
                                    cur.clear();
                                }
                                in_str = !in_str;
                            } else if in_str {
                                cur.push(ch);
                            } else {
                                outside.push(ch);
                            }
                            prev = ch;
                        }
                        if !ALLOWED_MESSAGES.contains(&last_lit.as_str()) {
                            problems.push(format!("{}: message {last_lit:?}", p.display()));
                        }
                        for part in outside.split(',') {
                            if let Some((name, _)) = part.split_once('=') {
                                let name = name.trim();
                                if !name.is_empty()
                                    && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                                    && !ALLOWED_FIELDS.contains(&name)
                                {
                                    problems.push(format!("{}: field {name}", p.display()));
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            calls > 5,
            "scanner found only {calls} calls; is it looking in the right place?"
        );
        assert!(
            problems.is_empty(),
            "non-allowlisted tracing usage:\n{}",
            problems.join("\n")
        );
    }
}
