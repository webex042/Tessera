//! Shared test fixtures: an encrypted temp store, items, and a SIMULATED model that returns
//! scripted outputs and records every request (content stays in the test process).

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Mutex;

use rusqlite::Connection;
use tsr_core::Sensitive;
use tsr_guard::blocklist::{Blocklist, builtin_rules};
use tsr_guard::redact::Options;
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::client::{ChatModel, ChatRequest, ChatResponse, ClientError};
use tsr_infer::http::HttpError;
use tsr_pipeline::dates::DateOrder;
use tsr_pipeline::extract::Ctx;
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, insert_item, upsert_source};
use tsr_store::migrate::migrate;
use tsr_store::{KEY_LEN, open_encrypted};

pub fn db() -> (tempfile::TempDir, Connection) {
    let d = tempfile::tempdir().unwrap();
    let mut c = open_encrypted(&d.path().join("t.db"), &[9u8; KEY_LEN]).unwrap();
    migrate(&mut c).unwrap();
    (d, c)
}

/// 2026-10-21 10:05 IST in UTC ms.
pub const T0: i64 = 1_792_557_300_000;

pub fn item_in(c: &Connection, app: &str, title: &str, url: Option<&str>, at: i64, text: &str) -> i64 {
    let s = upsert_source(
        c,
        &NewSource {
            kind: if url.is_some() {
                SourceKind::Url
            } else {
                SourceKind::App
            },
            app_exe: app,
            app_name: Some(app),
            window_title: Some(title),
            url,
            file_path: None,
            seen_at: at,
        },
    )
    .unwrap();
    insert_item(
        c,
        &NewItem {
            source_id: s,
            started_at: at,
            ended_at: at,
            acquisition: Acquisition::Uia,
            text: &Sensitive::new(text.into()),
            simhash: 0,
            lang: None,
            redaction_count: 0,
        },
    )
    .unwrap()
}

pub fn item(c: &Connection, at: i64, text: &str) -> i64 {
    item_in(c, "OUTLOOK.EXE", "Inbox - Outlook", None, at, text)
}

pub enum Reply {
    Text(String),
    Cancel(CancelReason),
    Fail(ClientError),
}

/// SIMULATED model: pops one scripted reply per request.
pub struct Scripted {
    pub replies: Mutex<VecDeque<Reply>>,
    pub requests: Mutex<Vec<(String, String, f32)>>,
}

impl Scripted {
    pub fn new(replies: Vec<Reply>) -> Self {
        Scripted {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn json(outputs: &[&str]) -> Self {
        Self::new(outputs.iter().map(|s| Reply::Text(s.to_string())).collect())
    }

    pub fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl ChatModel for Scripted {
    fn chat(&self, req: &ChatRequest, cancel: &CancelToken) -> Result<ChatResponse, ClientError> {
        self.requests
            .lock()
            .unwrap()
            .push((req.system.expose().clone(), req.user.expose().clone(), req.temperature));
        if let Some(r) = cancel.reason() {
            return Err(ClientError::Http(HttpError::Cancelled(r)));
        }
        match self.replies.lock().unwrap().pop_front() {
            Some(Reply::Text(t)) => Ok(ChatResponse {
                content: Sensitive::new(t),
                finish_reason: Some("stop".into()),
                prompt_tokens: None,
                completion_tokens: None,
                elapsed_ms: 1,
            }),
            Some(Reply::Cancel(r)) => Err(ClientError::Http(HttpError::Cancelled(r))),
            Some(Reply::Fail(e)) => Err(e),
            None => Err(ClientError::BadResponse("script exhausted")),
        }
    }
}

pub fn blocklist() -> Blocklist {
    Blocklist::compile(&builtin_rules()).0
}

pub fn ctx<'a>(bl: &'a Blocklist) -> Ctx<'a> {
    Ctx {
        user_name: "Asha Verma",
        user_aliases: &["Asha", "asha.verma@northwind.example"],
        tz: "Asia/Kolkata",
        date_order: DateOrder::Dmy,
        blocklist: bl,
        redact: Options::default(),
        model_id: "test-scripted",
        now_ms: T0,
    }
}

/// One-fact output helper.
pub fn out(summary: &str, facts: &[serde_json::Value], people: &[serde_json::Value]) -> String {
    serde_json::json!({"summary": summary, "facts": facts, "people": people}).to_string()
}

pub fn fact(kind: &str, text: &str, owner: &str, evidence: &str) -> serde_json::Value {
    serde_json::json!({"kind": kind, "text": text, "owner": owner, "counterparty": null, "due_text": null,
                       "evidence": evidence, "confidence": "high"})
}
