//! Ask with sources in the app (plan §10).
//!
//! `start`: filters from the question (time, app), keyword + facts retrieval (no embedder until
//! the owner approves one), top 8 snippets — returned at once so the sources show immediately.
//! With a model, a P0 job streams sentences (`ask_sentence`), each verified against the snippets
//! it cites, then `ask_done` with the whole answer. Citation chips open the item inside the
//! notch; nothing outside Tessara is opened.

use std::sync::{Arc, Mutex, OnceLock, RwLock};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use tsr_guard::blocklist::Blocklist;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::{ChatModel, ChatRequest};
use tsr_infer::runtime::Runner;
use tsr_infer::sched::{Dispatch, JobSpec, Outcome, Priority, Resource};
use tsr_retrieval::ask::{Snippet, answer, request, snippets};
use tsr_retrieval::embed::Embedder;
use tsr_retrieval::hybrid::{Filter, Opts, search, search_vectordb};
use tsr_store::vectordb::VectorDb;

use crate::drafts::Emit;
use crate::views::{ApiError, E_INVALID, E_STORE};

#[derive(Debug, Clone, Serialize)]
pub struct AskStart {
    pub snippets: Vec<Snippet>,
    /// A model will answer (otherwise the UI shows the sources only).
    pub answering: bool,
    /// True if semantic VectorDb search contributed to retrieval for this query.
    pub vector_used: bool,
}

struct Job {
    req: ChatRequest,
    snippets: Vec<Snippet>,
}

type AskSubmitFn = Arc<dyn Fn(JobSpec) -> bool + Send + Sync>;
type EmbedderSlot = Arc<Mutex<Option<Arc<dyn Embedder>>>>;

pub struct Asks {
    conn: Mutex<Connection>,
    blocklist: Arc<RwLock<Blocklist>>,
    pending: Arc<Mutex<Option<Job>>>,
    submit: Mutex<Option<AskSubmitFn>>,
    emit: Arc<OnceLock<Emit>>,
    vector_db: Option<Arc<Mutex<VectorDb>>>,
    embedder: Option<EmbedderSlot>,
    seq: Mutex<u64>,
}

impl Asks {
    pub fn new(conn: Connection, blocklist: Blocklist) -> Self {
        Asks {
            conn: Mutex::new(conn),
            blocklist: Arc::new(RwLock::new(blocklist)),
            pending: Arc::new(Mutex::new(None)),
            submit: Mutex::new(None),
            emit: Arc::new(OnceLock::new()),
            vector_db: None,
            embedder: None,
            seq: Mutex::new(0),
        }
    }

    pub fn with_shared_blocklist(mut self, blocklist: Arc<RwLock<Blocklist>>) -> Self {
        self.blocklist = blocklist;
        self
    }

    pub fn with_vector_db(mut self, vdb: Arc<Mutex<VectorDb>>) -> Self {
        self.vector_db = Some(vdb);
        self
    }

    pub fn with_embedder(mut self, embedder: Arc<Mutex<Option<Arc<dyn Embedder>>>>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    pub fn set_emitter(&self, e: Emit) {
        let _ = self.emit.set(e);
    }

    pub fn set_submit(&self, f: Box<dyn Fn(JobSpec) -> bool + Send + Sync>) {
        *self.submit.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::from(f));
    }

    pub fn clear_submit(&self) {
        *self.submit.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    pub fn runner(&self) -> AskRunner {
        AskRunner {
            pending: self.pending.clone(),
            emit: self.emit.clone(),
        }
    }

    pub fn start(&self, question: &str, now: i64, tz: &jiff::tz::TimeZone) -> Result<AskStart, ApiError> {
        let q = question.trim();
        if q.is_empty() || q.chars().count() > tsr_retrieval::ask::MAX_QUESTION_CHARS {
            return Err(E_INVALID);
        }
        let c = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let apps: Vec<String> = c
            .prepare("SELECT DISTINCT COALESCE(app_name, app_exe) FROM sources")
            .map_err(|_| E_STORE)?
            .query_map([], |r| r.get(0))
            .map_err(|_| E_STORE)?
            .flatten()
            .collect();
        let aq = tsr_pipeline::askq::parse(q, now, tz, &apps);
        let mut o = Opts::new(now);
        o.k = 40;
        o.use_vector = false;
        let bl_guard = self.blocklist.read().unwrap_or_else(|p| p.into_inner());
        let filter = Filter {
            blocklist: Some(&*bl_guard),
            suppressed: None,
        };
        let active_emb = self
            .embedder
            .as_ref()
            .and_then(|m| m.lock().unwrap_or_else(|p| p.into_inner()).clone());
        let (hits, diag) = if let (Some(vdb_lock), Some(emb)) = (&self.vector_db, active_emb) {
            let vdb = vdb_lock.lock().unwrap_or_else(|p| p.into_inner());
            if !vdb.is_empty() {
                o.use_vector = true;
                search_vectordb(&c, Some(&*vdb), Some(&*emb), q, &filter, &o)?
            } else {
                o.use_vector = false;
                search(&c, None, None, q, &filter, &o)?
            }
        } else {
            o.use_vector = false;
            search(&c, None, None, q, &filter, &o)?
        };
        let vector_used = diag.vector > 0;
        drop(bl_guard);
        let mut ranked = Vec::new();
        for h in hits {
            if let Some((a, b)) = aq.range
                && !(a..b).contains(&h.started_at)
            {
                continue;
            }
            if let Some(app) = &aq.app {
                let src: Option<String> = c
                    .query_row(
                        "SELECT COALESCE(sources.app_name, sources.app_exe) FROM items JOIN sources ON sources.id = items.source_id WHERE items.id = ?1",
                        [h.item_id],
                        |r| r.get(0),
                    )
                    .ok();
                if src.as_deref() != Some(app.as_str()) {
                    continue;
                }
            }
            ranked.push(h.item_id);
        }
        let snips = snippets(&c, &ranked, q)?;
        drop(c);
        let local = jiff::Timestamp::from_millisecond(now)
            .map_err(|_| E_INVALID)?
            .to_zoned(tz.clone());
        let tz2 = tz.clone();
        let label = move |ms: i64| {
            jiff::Timestamp::from_millisecond(ms)
                .map(|t| t.to_zoned(tz2.clone()).strftime("%a %-d %b %H:%M").to_string())
                .unwrap_or_default()
        };
        let req = request(
            q,
            &snips,
            &local.strftime("%Y-%m-%d").to_string(),
            &local.strftime("%A").to_string(),
            &label,
        );
        let submit_fn = self.submit.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let answering = match submit_fn {
            Some(submit) if !snips.is_empty() => {
                *self.pending.lock().unwrap_or_else(|p| p.into_inner()) = Some(Job {
                    req,
                    snippets: snips.clone(),
                });
                let n = {
                    let mut s = self.seq.lock().unwrap_or_else(|p| p.into_inner());
                    *s += 1;
                    *s
                };
                submit(JobSpec {
                    key: format!("ask:{n}"),
                    kind: "ask",
                    priority: Priority::P0,
                    resource: Resource::Model,
                    payload: 0,
                })
            }
            _ => false,
        };
        Ok(AskStart {
            snippets: snips,
            answering,
            vector_used,
        })
    }
}

/// Runs `ask` jobs on the model runtime (P0).
pub struct AskRunner {
    pending: Arc<Mutex<Option<Job>>>,
    emit: Arc<OnceLock<Emit>>,
}

impl Runner for AskRunner {
    fn run(&mut self, _: &Dispatch, model: Option<&dyn ChatModel>, cancel: &CancelToken) -> Outcome {
        let emit = |ev: &'static str, v: serde_json::Value| {
            if let Some(e) = self.emit.get() {
                e(ev, v);
            }
        };
        let Some(job) = self.pending.lock().unwrap_or_else(|p| p.into_inner()).take() else {
            return Outcome::Done;
        };
        let Some(model) = model else {
            emit("ask_error", json!({"code": "ask_model_unavailable"}));
            return Outcome::Done;
        };
        match answer(model, &job.req, &job.snippets, cancel, &mut |s| {
            emit("ask_sentence", serde_json::to_value(s).unwrap_or_default());
        }) {
            Ok(a) => {
                emit("ask_done", serde_json::to_value(&a).unwrap_or_default());
                Outcome::Done
            }
            Err(_) if cancel.is_cancelled() => Outcome::Cancelled,
            Err(tsr_retrieval::ask::AskError::Model(
                tsr_infer::client::ClientError::RateLimited { .. }
                | tsr_infer::client::ClientError::QuotaExhausted
                | tsr_infer::client::ClientError::BackgroundDeferred { .. },
            )) => {
                emit("ask_error", json!({"code": "ask_quota_exceeded"}));
                Outcome::Done
            }
            Err(tsr_retrieval::ask::AskError::Model(tsr_infer::client::ClientError::Unauthorized)) => {
                emit("ask_error", json!({"code": "ask_model_unauthorized"}));
                Outcome::Done
            }
            Err(_) => {
                emit("ask_error", json!({"code": "ask_invalid_output"}));
                Outcome::Done
            }
        }
    }
}
