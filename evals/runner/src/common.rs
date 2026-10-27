//! Shared pieces: an encrypted temp store, the eval datasets, models, and report files.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Deserialize;
use tsr_core::Sensitive;
use tsr_guard::redact::{Options, redact};
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{ChatModel, ChatRequest, ChatResponse, ClientError};
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, insert_item, upsert_source};
use tsr_store::migrate::migrate;
use tsr_store::{KEY_LEN, open_encrypted};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

pub fn db() -> (tempfile::TempDir, Connection) {
    let d = tempfile::tempdir().expect("temp dir");
    let mut c = open_encrypted(&d.path().join("eval.db"), &[3u8; KEY_LEN]).expect("store");
    migrate(&mut c).expect("migrate");
    (d, c)
}

/// Parse `2026-10-21T15:12:00+05:30` to UTC ms.
pub fn ts(s: &str) -> i64 {
    let t: jiff::Timestamp = s.parse().expect("timestamp");
    t.as_millisecond()
}

/// Store an item the way capture would: redacted first.
pub fn add_item(c: &Connection, app: &str, title: &str, source: &str, at: i64, text: &str, acq: Acquisition) -> i64 {
    let (kind, exe, url) = if source.starts_with("http") {
        (SourceKind::Url, app.to_string(), Some(source))
    } else {
        (
            SourceKind::App,
            source.strip_prefix("app:").unwrap_or(app).to_string(),
            None,
        )
    };
    let s = upsert_source(
        c,
        &NewSource {
            kind,
            app_exe: &exe,
            app_name: Some(app),
            window_title: Some(&redact(title, Options::default()).text),
            url,
            file_path: None,
            seen_at: at,
        },
    )
    .expect("source");
    let r = redact(text, Options::default());
    insert_item(
        c,
        &NewItem {
            source_id: s,
            started_at: at,
            ended_at: at,
            acquisition: acq,
            text: &Sensitive::new(r.text.clone()),
            simhash: 0,
            lang: None,
            redaction_count: r.total(),
        },
    )
    .expect("item")
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoldFact {
    pub kind: String,
    pub owner: String,
    #[serde(default)]
    pub counterparty: Option<String>,
    pub text: String,
    #[serde(default)]
    pub due_text: Option<String>,
    pub evidence: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Expected {
    pub facts: Vec<GoldFact>,
    #[serde(default)]
    pub people: Vec<serde_json::Value>,
    #[serde(default)]
    pub forbidden_substrings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalItem {
    pub id: String,
    pub split: String,
    pub app: String,
    pub title: String,
    pub source: String,
    pub captured_at: String,
    pub text: String,
    pub expected: Expected,
}

pub fn load_items() -> Vec<EvalItem> {
    let mut v = Vec::new();
    let dir = root().join("evals/extraction/items");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("items dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for f in files {
        let items: Vec<EvalItem> = serde_json::from_str(&std::fs::read_to_string(&f).expect("read")).expect("parse");
        v.extend(items);
    }
    v
}

/// SIMULATED model: replies in order.
pub struct Scripted(pub Mutex<VecDeque<String>>, pub Mutex<Vec<String>>);

impl Scripted {
    pub fn one(reply: String) -> Self {
        Scripted(Mutex::new(VecDeque::from([reply])), Mutex::new(Vec::new()))
    }
}

impl ChatModel for Scripted {
    fn chat(&self, req: &ChatRequest, _: &CancelToken) -> Result<ChatResponse, ClientError> {
        self.1.lock().unwrap().push(req.user.expose().clone());
        let t = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(ClientError::BadResponse("script exhausted"))?;
        Ok(ChatResponse {
            content: Sensitive::new(t),
            finish_reason: Some("stop".into()),
            prompt_tokens: None,
            completion_tokens: None,
            elapsed_ms: 0,
        })
    }
}

/// A real model behind a started sidecar (kept alive as long as this value).
pub struct RealModel {
    pub process: tsr_infer::sidecar::SidecarProcess,
    pub label: String,
}

impl ChatModel for RealModel {
    fn chat(&self, req: &ChatRequest, c: &CancelToken) -> Result<ChatResponse, ClientError> {
        self.process.client().chat(req, c)
    }
}

/// Start llama-server with the verified model files in `models_dir` on the best backend.
pub fn real_model(models_dir: &Path) -> Result<RealModel, String> {
    use tsr_infer::models::{FileStatus, MANIFEST, inspect_dir};
    let r = inspect_dir(models_dir, &MANIFEST, true, &CancelToken::new(), &mut |_, _| {});
    let usable = |id: &str| {
        r.entries
            .iter()
            .any(|e| e.id == id && e.usable && e.files.iter().all(|f| matches!(f.status, FileStatus::Ok { .. })))
    };
    if !usable("qwen3-vl-4b-instruct-q4_k_m") {
        return Err("model files missing or not verified (tsr-models locate <dir> --verify)".into());
    }
    let model = models_dir.join("Qwen3VL-4B-Instruct-Q4_K_M.gguf");
    let mmproj =
        usable("qwen3-vl-4b-instruct-mmproj-f16").then(|| models_dir.join("mmproj-Qwen3VL-4B-Instruct-F16.gguf"));
    let report = tsr_infer::detect(
        &root().join("third_party"),
        &tsr_infer::hardware::SystemAdapters,
        &tsr_infer::backend::LlamaServerProbe::default(),
    );
    let g = tsr_infer::gguf::read_gguf(&model).map_err(|e| e.to_string())?;
    let mm = mmproj.as_ref().and_then(|p| tsr_infer::gguf::read_gguf(p).ok());
    let needs = tsr_infer::select::ModelNeeds::from_gguf(&g, mm.as_ref(), 8192, 1.0625);
    let plan = tsr_infer::select::plan(
        &report.adapters_result(),
        &report.builds,
        &report.probes,
        &needs,
        &Default::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut last = String::from("no candidates");
    for c in &plan.candidates {
        let spec = tsr_infer::sidecar::SidecarSpec::from_candidate(c, model.clone(), mmproj.clone(), 8192, None);
        match tsr_infer::sidecar::start(&spec, &CancelToken::new()) {
            Ok(p) => {
                return Ok(RealModel {
                    process: p,
                    label: format!("qwen3-vl-4b-instruct-q4_k_m on {}", c.label()),
                });
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// Write `<name>.json` and `<name>.md` under `evals/results/`.
pub fn write_report(name: &str, json: &serde_json::Value, md: &str) -> PathBuf {
    let dir = root().join("evals/results");
    std::fs::create_dir_all(&dir).expect("results dir");
    std::fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_string_pretty(json).expect("json") + "\n",
    )
    .expect("write");
    let p = dir.join(format!("{name}.md"));
    std::fs::write(&p, md).expect("write");
    p
}

pub fn pct(x: f64) -> String {
    format!("{:.1} %", x * 100.0)
}
