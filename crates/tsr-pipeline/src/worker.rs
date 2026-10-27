//! Connects the durable `jobs` table to the in-memory scheduler (`tsr_infer::runtime`).
//!
//! - [`submit_pending`] — on start (after [`tsr_store::jobs::requeue_interrupted`]) and then
//!   periodically: every queued `extract` / `embed` job becomes a scheduler job with a stable key
//!   (`extract:<item>`), so it can be queued only once and completes only once.
//! - [`ExtractRunner`] / [`EmbedRunner`] — run one job on their own database connection and
//!   translate the result into a scheduler outcome. A preempted extraction stores nothing and is
//!   re-queued; the database state (`jobs`, `items.extraction`) is always written by the job
//!   itself in its own transaction.

use rusqlite::Connection;
use tsr_infer::cancel::CancelToken;
use tsr_infer::client::ChatModel;
use tsr_infer::runtime::Runner;
use tsr_infer::sched::{Dispatch, JobSpec, Outcome, Priority, Resource, SubmitError};
use tsr_retrieval::embed::Embedder;
use tsr_store::StoreError;

use crate::extract::{self, Ctx, ExtractError};

pub fn spec(kind: &str, item_id: i64) -> Option<JobSpec> {
    let (k, resource): (&'static str, Resource) = match kind {
        "extract" => ("extract", Resource::Model),
        "embed" => ("embed", Resource::Embedder),
        _ => return None,
    };
    Some(JobSpec {
        key: format!("{k}:{item_id}"),
        kind: k,
        priority: Priority::P1,
        resource,
        payload: item_id,
    })
}

/// Submit every queued job; duplicates and already-done keys are skipped by the scheduler.
/// Returns how many were newly accepted.
pub fn submit_pending(
    conn: &Connection,
    kinds: &[&str],
    limit: u32,
    submit: &mut dyn FnMut(JobSpec) -> Result<u64, SubmitError>,
) -> Result<usize, StoreError> {
    let mut n = 0;
    for &kind in kinds {
        for (_, item) in tsr_store::jobs::queued(conn, kind, limit)? {
            match submit(spec(kind, item).expect("known kind")) {
                Ok(_) => n += 1,
                Err(SubmitError::QueueFull) => return Ok(n), // back-pressure: try again later
                Err(_) => {}
            }
        }
    }
    Ok(n)
}

/// Extraction jobs. `ctx` is rebuilt per job by `make_ctx` (clock, settings, blocklist).
pub struct ExtractRunner<F: FnMut() -> CtxOwned + Send> {
    pub conn: Connection,
    pub make_ctx: F,
}

/// Owned form of [`Ctx`] for long-lived runners.
pub struct CtxOwned {
    pub user_name: String,
    pub user_aliases: Vec<String>,
    pub tz: String,
    pub date_order: crate::dates::DateOrder,
    pub blocklist: tsr_guard::blocklist::Blocklist,
    pub redact: tsr_guard::redact::Options,
    pub model_id: String,
    pub now_ms: i64,
}

impl<F: FnMut() -> CtxOwned + Send> Runner for ExtractRunner<F> {
    fn run(&mut self, job: &Dispatch, model: Option<&dyn ChatModel>, cancel: &CancelToken) -> Outcome {
        let Some(model) = model else {
            return Outcome::Deferred { retry_in_ms: 5_000 };
        };
        let o = (self.make_ctx)();
        let aliases: Vec<&str> = o.user_aliases.iter().map(String::as_str).collect();
        let ctx = Ctx {
            user_name: &o.user_name,
            user_aliases: &aliases,
            tz: &o.tz,
            date_order: o.date_order,
            blocklist: &o.blocklist,
            redact: o.redact,
            model_id: &o.model_id,
            now_ms: o.now_ms,
        };
        match extract::run(&mut self.conn, job.spec.payload, model, &ctx, cancel) {
            Ok(r) => match r.outcome {
                Some(extract::Outcome::Preempted) => Outcome::Preempted,
                Some(extract::Outcome::Cancelled) => Outcome::Cancelled,
                // Schema-invalid twice is final for this prompt version; recorded in the database.
                Some(extract::Outcome::Failed(_)) => Outcome::Failed { retryable: false },
                _ => Outcome::Done,
            },
            Err(ExtractError::NotFound) => Outcome::Done, // deleted meanwhile: nothing to do
            Err(_) => Outcome::Failed { retryable: true },
        }
    }
}

/// Embedding jobs (CPU; never touches the model).
pub struct EmbedRunner<E: Embedder> {
    pub conn: Connection,
    pub embedder: E,
    pub index: Option<tsr_retrieval::index::VectorIndex>,
    pub clock: fn() -> i64,
}

impl<E: Embedder> Runner for EmbedRunner<E> {
    fn run(&mut self, job: &Dispatch, _: Option<&dyn ChatModel>, cancel: &CancelToken) -> Outcome {
        if cancel.is_cancelled() {
            return Outcome::Cancelled;
        }
        match tsr_retrieval::ingest::embed_item(
            &mut self.conn,
            job.spec.payload,
            &self.embedder,
            self.index.as_mut(),
            (self.clock)(),
        ) {
            Ok(_) => Outcome::Done,
            Err(_) => Outcome::Failed { retryable: true },
        }
    }
}
