//! Draft assistant in the app (plan §12).
//!
//! 1. **Begin** (Ctrl+Alt+D, before the notch takes focus): the focused element must be an
//!    editable, non-password field in a window the blocklist allows. Its window's text (UIA) is
//!    the thread context, redacted, last ~2 000 characters.
//! 2. **Generate**: keyword memory retrieval (top 5), then a P0 model job that streams the draft
//!    to the UI (`draft_chunk`) and finishes with the fact-checked draft (`draft_ready`) and a
//!    single-use insert token.
//! 3. **Insert**: only with that token; focus goes back to the target, which must be the very
//!    same element (UIA RuntimeId) and still editable and not a password field; the draft is
//!    pasted through a private clipboard (no history, no cloud) that is restored 500 ms later.
//!    Never Enter, never Send.
//!
//! Drafts are never stored; the audit log gets `draft_inserted` / `draft_copied` /
//! `draft_discarded` with the length only.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use tsr_core::Sensitive;
use tsr_guard::blocklist::{Blocklist, PrivateState, WindowFacts, is_browser};
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::ChatModel;
use tsr_infer::runtime::Runner;
use tsr_infer::sched::{Dispatch, JobSpec, Outcome, Priority, Resource};

use crate::views::ApiError;

pub const E_NO_FIELD: ApiError = ApiError::new("draft_no_field", "draft.no_field");
pub const E_PASSWORD: ApiError = ApiError::new("draft_password_field", "draft.password_field");
pub const E_BLOCKED: ApiError = ApiError::new("draft_blocked_app", "draft.blocked_app");
pub const E_NO_SESSION: ApiError = ApiError::new("draft_no_session", "draft.no_session");
pub const E_MODEL: ApiError = ApiError::new("draft_model_unavailable", "draft.model_unavailable");
pub const E_TARGET_CHANGED: ApiError = ApiError::new("draft_target_changed", "draft.target_changed");
pub const E_CLIPBOARD: ApiError = ApiError::new("draft_clipboard_not_text", "draft.clipboard_not_text");
pub const E_INVALID: ApiError = crate::views::E_INVALID;
pub const E_TOKEN: ApiError = crate::views::E_TOKEN;

pub const TOKEN_TTL: Duration = Duration::from_secs(600);
pub const MAX_EDITED_CHARS: usize = 2_000;

/// UI events: `draft_chunk` {text}, `draft_ready` {DraftDto}, `draft_error` {code}.
pub type Emit = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
pub struct DraftStart {
    pub app: String,
    pub thread_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftDto {
    pub text: String,
    pub flags: Vec<tsr_draft::factcheck::Flag>,
    pub placeholders: Vec<String>,
    pub used: Vec<i64>,
    pub first_text_ms: Option<u64>,
    pub total_ms: u64,
    pub token: String,
}

#[cfg(windows)]
type Target = tsr_capture::uia::FocusTarget;
#[cfg(not(windows))]
type Target = ();

struct Session {
    #[cfg_attr(not(windows), allow(dead_code))]
    target: Target,
    app: String,
    thread: Sensitive<String>,
    text: Option<Sensitive<String>>,
    token: Option<(String, Instant)>,
}

struct Job {
    app: String,
    thread: Sensitive<String>,
    memory: Vec<(i64, Sensitive<String>)>,
    instruction: String,
}

pub struct Drafts {
    #[cfg(windows)]
    uia: Mutex<tsr_capture::uia::UiaReader>,
    session: Arc<Mutex<Option<Session>>>,
    pending: Arc<Mutex<Option<Job>>>,
    conn: Mutex<Connection>,
    blocklist: Blocklist,
    /// Queues a P0 job on the model runtime; unset when no model is set up.
    submit: OnceLock<Box<dyn Fn(JobSpec) -> bool + Send + Sync>>,
    emit: Arc<OnceLock<Emit>>,
    seq: Mutex<u64>,
}

fn new_token() -> String {
    #[cfg(windows)]
    let b = tsr_win::dpapi::random_bytes::<16>().expect("BCryptGenRandom");
    #[cfg(not(windows))]
    let b = [7u8; 16];
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Drafts {
    pub fn new(conn: Connection, blocklist: Blocklist) -> std::io::Result<Self> {
        Ok(Drafts {
            #[cfg(windows)]
            uia: Mutex::new(tsr_capture::uia::UiaReader::new()?),
            session: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(None)),
            conn: Mutex::new(conn),
            blocklist,
            submit: OnceLock::new(),
            emit: Arc::new(OnceLock::new()),
            seq: Mutex::new(0),
        })
    }

    pub fn set_emitter(&self, e: Emit) {
        let _ = self.emit.set(e);
    }

    /// Set once the model runtime is running.
    pub fn set_submit(&self, f: Box<dyn Fn(JobSpec) -> bool + Send + Sync>) {
        let _ = self.submit.set(f);
    }

    /// The runner for `draft` jobs (registered with the model runtime).
    pub fn runner(&self) -> DraftRunner {
        DraftRunner {
            pending: self.pending.clone(),
            session: self.session.clone(),
            emit: self.emit.clone(),
        }
    }

    fn lock_session(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Step 1: the focused field. Called on the hotkey, while the target app is still in front.
    #[cfg(windows)]
    pub fn begin(&self) -> Result<DraftStart, ApiError> {
        let uia = self.uia.lock().unwrap_or_else(|p| p.into_inner());
        let t = uia.focus().map_err(|_| E_NO_FIELD)?;
        if t.is_password {
            return Err(E_PASSWORD);
        }
        if !t.insertable() {
            return Err(E_NO_FIELD);
        }
        let facts = tsr_win::query::window_facts(t.hwnd).ok_or(E_NO_FIELD)?;
        let (url, private) = if is_browser(&facts.exe) {
            match uia.browser(windows::Win32::Foundation::HWND(t.hwnd as *mut _)) {
                Ok(b) => (b.url.map(|u| u.expose().clone()), b.private),
                Err(_) => (None, PrivateState::Unknown), // fail closed
            }
        } else {
            (None, PrivateState::NotPrivate)
        };
        let decision = self.blocklist.check(&WindowFacts {
            exe: &facts.exe,
            exe_path: Some(&facts.exe_path),
            title: &facts.title,
            url: url.as_deref(),
            private,
        });
        if decision.is_blocked() {
            return Err(E_BLOCKED);
        }
        let raw = uia
            .read(windows::Win32::Foundation::HWND(t.hwnd as *mut _))
            .map(|r| r.text)
            .unwrap_or_default();
        let redacted = tsr_guard::redact::redact(&raw, tsr_guard::redact::Options::default()).text;
        let thread = tsr_draft::draft::thread_window(&redacted, tsr_draft::draft::THREAD_CHARS).to_string();
        let start = DraftStart {
            app: facts.exe.clone(),
            thread_chars: thread.chars().count(),
        };
        *self.lock_session() = Some(Session {
            target: t,
            app: facts.exe,
            thread: Sensitive::new(thread),
            text: None,
            token: None,
        });
        Ok(start)
    }

    /// Step 2: queue the draft with the user's optional instruction.
    pub fn generate(&self, instruction: &str) -> Result<(), ApiError> {
        if instruction.chars().count() > tsr_draft::draft::MAX_INSTRUCTION_CHARS {
            return Err(E_INVALID);
        }
        let (app, thread) = {
            let s = self.lock_session();
            let s = s.as_ref().ok_or(E_NO_SESSION)?;
            (s.app.clone(), s.thread.clone())
        };
        let submit = self.submit.get().ok_or(E_MODEL)?;
        let memory = retrieve(
            &self.conn.lock().unwrap_or_else(|p| p.into_inner()),
            thread.expose(),
            instruction,
        );
        *self.pending.lock().unwrap_or_else(|p| p.into_inner()) = Some(Job {
            app,
            thread,
            memory,
            instruction: instruction.to_string(),
        });
        let n = {
            let mut s = self.seq.lock().unwrap_or_else(|p| p.into_inner());
            *s += 1;
            *s
        };
        let ok = submit(JobSpec {
            key: format!("draft:{n}"),
            kind: "draft",
            priority: Priority::P0,
            resource: Resource::Model,
            payload: 0,
        });
        if ok { Ok(()) } else { Err(E_MODEL) }
    }

    /// The draft text for this token (consumes the token). `edited`: the user's edited text.
    fn take_text(&self, token: &str, edited: Option<&str>) -> Result<(String, Target), ApiError> {
        let mut s = self.lock_session();
        let sess = s.as_mut().ok_or(E_NO_SESSION)?;
        match sess.token.take() {
            Some((t, at)) if t == token && at.elapsed() < TOKEN_TTL => {}
            _ => return Err(E_TOKEN),
        }
        let text = match edited {
            Some(e) if e.chars().count() <= MAX_EDITED_CHARS && !e.trim().is_empty() => e.to_string(),
            Some(_) => return Err(E_INVALID),
            None => sess.text.as_ref().ok_or(E_NO_SESSION)?.expose().clone(),
        };
        #[cfg(windows)]
        let target = sess.target.clone();
        #[cfg(not(windows))]
        let target = ();
        *s = None;
        Ok((text, target))
    }

    /// Step 3: paste into the original field, only if it is still the same editable field.
    /// The caller has already returned focus to the target window.
    #[cfg(windows)]
    pub fn insert(&self, token: &str, edited: Option<&str>, now_ms: i64) -> Result<(), ApiError> {
        let (text, target) = self.take_text(token, edited)?;
        std::thread::sleep(Duration::from_millis(200)); // let focus settle in the target
        let now = self
            .uia
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .focus()
            .map_err(|_| E_TARGET_CHANGED)?;
        if !now.same_as(&target) {
            return Err(E_TARGET_CHANGED);
        }
        if now.is_password || !now.insertable() {
            return Err(E_PASSWORD);
        }
        let guard = match tsr_win::clipboard::set_private_text(&text) {
            Ok(g) => g,
            Err(tsr_win::clipboard::ClipError::NotText) => return Err(E_CLIPBOARD),
            Err(_) => return Err(E_CLIPBOARD),
        };
        let pasted = tsr_win::clipboard::send_paste();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            let _ = tsr_win::clipboard::restore(guard);
        });
        pasted.map_err(|_| E_TARGET_CHANGED)?;
        let _ = tsr_store::audit::record(
            &self.conn.lock().unwrap_or_else(|p| p.into_inner()),
            now_ms,
            "draft_inserted",
            &[("chars", text.chars().count() as i64)],
        );
        Ok(())
    }

    /// Copy instead of insert: the draft goes on the clipboard privately and stays there.
    #[cfg(windows)]
    pub fn copy(&self, token: &str, edited: Option<&str>, now_ms: i64) -> Result<(), ApiError> {
        let (text, _) = self.take_text(token, edited)?;
        tsr_win::clipboard::set_private_text(&text).map_err(|_| E_CLIPBOARD)?;
        let _ = tsr_store::audit::record(
            &self.conn.lock().unwrap_or_else(|p| p.into_inner()),
            now_ms,
            "draft_copied",
            &[("chars", text.chars().count() as i64)],
        );
        Ok(())
    }

    pub fn discard(&self, now_ms: i64) {
        let had = self.lock_session().take();
        *self.pending.lock().unwrap_or_else(|p| p.into_inner()) = None;
        if let Some(s) = had {
            let n = s.text.map(|t| t.expose().chars().count() as i64).unwrap_or(0);
            let _ = tsr_store::audit::record(
                &self.conn.lock().unwrap_or_else(|p| p.into_inner()),
                now_ms,
                "draft_discarded",
                &[("chars", n)],
            );
        }
    }
}

/// Keyword retrieval for the draft (plan §12.1: top 5). Snippets are item summaries, or the
/// start of the item text.
fn retrieve(conn: &Connection, thread: &str, instruction: &str) -> Vec<(i64, Sensitive<String>)> {
    let tail: String = thread
        .chars()
        .rev()
        .take(300)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let q = format!("{tail} {instruction}");
    let Ok(hits) = tsr_store::items::search_any(conn, &q, 5) else {
        return Vec::new();
    };
    hits.into_iter()
        .filter_map(|h| {
            let it = tsr_store::items::get_item(conn, h.item_id).ok()??;
            let text: String = it.text.expose().chars().take(tsr_draft::draft::SNIPPET_CHARS).collect();
            Some((h.item_id, Sensitive::new(text)))
        })
        .collect()
}

/// Runs `draft` jobs on the model runtime (P0).
pub struct DraftRunner {
    pending: Arc<Mutex<Option<Job>>>,
    session: Arc<Mutex<Option<Session>>>,
    emit: Arc<OnceLock<Emit>>,
}

impl Runner for DraftRunner {
    fn run(&mut self, _: &Dispatch, model: Option<&dyn ChatModel>, cancel: &CancelToken) -> Outcome {
        let emit = |ev: &'static str, v: serde_json::Value| {
            if let Some(e) = self.emit.get() {
                e(ev, v);
            }
        };
        let Some(job) = self.pending.lock().unwrap_or_else(|p| p.into_inner()).take() else {
            return Outcome::Done; // replaced or discarded
        };
        let Some(model) = model else {
            emit("draft_error", json!({"code": E_MODEL.code}));
            return Outcome::Done;
        };
        let memory: Vec<(i64, &str)> = job.memory.iter().map(|(i, t)| (*i, t.expose().as_str())).collect();
        let input = tsr_draft::draft::DraftInput {
            app: &job.app,
            thread: job.thread.expose(),
            memory: &memory,
            instruction: &job.instruction,
            style_examples: &[],
        };
        let r = tsr_draft::draft::run(model, &input, cancel, &mut |t| emit("draft_chunk", json!({"text": t})));
        match r {
            Ok(d) => {
                let token = new_token();
                if let Some(s) = self.session.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
                    s.text = Some(Sensitive::new(d.text.clone()));
                    s.token = Some((token.clone(), Instant::now()));
                }
                let dto = DraftDto {
                    text: d.text,
                    flags: d.flags,
                    placeholders: d.placeholders,
                    used: d.used,
                    first_text_ms: d.first_text_ms,
                    total_ms: d.total_ms,
                    token,
                };
                emit("draft_ready", serde_json::to_value(&dto).unwrap_or_default());
                Outcome::Done
            }
            Err(tsr_draft::draft::DraftError::Model(e))
                if cancel.is_cancelled()
                    || matches!(
                        e,
                        tsr_infer::client::ClientError::Http(tsr_infer::http::HttpError::Cancelled(_))
                    ) =>
            {
                emit("draft_error", json!({"code": "draft_cancelled"}));
                Outcome::Cancelled
            }
            Err(tsr_draft::draft::DraftError::Model(_)) => {
                emit("draft_error", json!({"code": E_MODEL.code}));
                Outcome::Failed { retryable: false }
            }
            Err(_) => {
                emit("draft_error", json!({"code": "draft_invalid_output"}));
                Outcome::Done
            }
        }
    }
}
