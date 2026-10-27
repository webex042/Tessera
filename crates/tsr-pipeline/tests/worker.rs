//! Durable jobs → scheduler → runners, with a SIMULATED model and the SIMULATED embedder.

mod common;

use common::*;
use serde_json::json;
use tsr_infer::cancel::{CancelReason, CancelToken};
use tsr_infer::runtime::Runner;
use tsr_infer::sched::{Config, Gates, Outcome, Resource, SchedulerCore};
use tsr_pipeline::dates::DateOrder;
use tsr_pipeline::worker::{CtxOwned, EmbedRunner, ExtractRunner, submit_pending};
use tsr_retrieval::embed::HashEmbedder;
use tsr_store::jobs::{self, JobState};

fn ctx() -> CtxOwned {
    CtxOwned {
        user_name: "Asha Verma".into(),
        user_aliases: vec!["Asha".into()],
        tz: "Asia/Kolkata".into(),
        date_order: DateOrder::Dmy,
        blocklist: blocklist(),
        redact: tsr_guard::redact::Options::default(),
        model_id: "test-scripted".into(),
        now_ms: T0,
    }
}

#[test]
fn queued_jobs_flow_through_the_scheduler_once() {
    let (d, c) = db();
    let text = "Ravi: Asha, can you send the revised sheet by Friday?\nAsha: sure, I'll send it by Friday.";
    let it = item(&c, T0, text);
    jobs::enqueue(&c, "embed", it, T0).unwrap();
    jobs::enqueue(&c, "extract", it, T0).unwrap();

    let mut core = SchedulerCore::new(Config::default());
    let now = std::time::Instant::now();
    let mut submit = |s| core.submit(s, now);
    assert_eq!(submit_pending(&c, &["embed", "extract"], 100, &mut submit).unwrap(), 2);
    assert_eq!(
        submit_pending(&c, &["embed", "extract"], 100, &mut submit).unwrap(),
        0,
        "already queued: not submitted twice"
    );

    let path = d.path().join("t.db");
    let open = || {
        let c = tsr_store::open_encrypted(&path, &[9u8; tsr_store::KEY_LEN]).unwrap();
        c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        c
    };
    let mut ex = ExtractRunner {
        conn: open(),
        make_ctx: ctx,
    };
    let mut em = EmbedRunner {
        conn: open(),
        embedder: HashEmbedder::default(),
        index: None,
        clock: || T0,
    };
    let mut f = fact(
        "task",
        "Send Ravi the revised sheet",
        "me",
        "can you send the revised sheet by Friday?",
    );
    f["due_text"] = json!("by Friday");
    let model = Scripted::json(&[&out("", &[f], &[])]);
    let g = Gates::default();

    let d1 = core.next(Resource::Embedder, now, &g).unwrap();
    assert_eq!(em.run(&d1, None, &CancelToken::new()), Outcome::Done);
    core.finish(d1.id, Outcome::Done, now);
    let d2 = core.next(Resource::Model, now, &g).unwrap();
    assert_eq!(ex.run(&d2, Some(&model), &CancelToken::new()), Outcome::Done);
    core.finish(d2.id, Outcome::Done, now);

    assert_eq!(jobs::get(&c, "extract", it).unwrap().unwrap().state, JobState::Done);
    assert_eq!(jobs::get(&c, "embed", it).unwrap().unwrap().state, JobState::Done);
    let chunks: i64 = c.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0)).unwrap();
    assert_eq!((tsr_store::facts::all(&c).unwrap().len(), chunks), (1, 1));
    assert_eq!(
        submit_pending(&c, &["embed", "extract"], 100, &mut |s| core.submit(s, now)).unwrap(),
        0,
        "done jobs are not resubmitted"
    );
}

#[test]
fn preempted_or_missing_model_requeues_without_writing() {
    let (d, c) = db();
    let it = item(&c, T0, "Ravi: Asha, can you send the revised sheet by Friday please?");
    jobs::enqueue(&c, "extract", it, T0).unwrap();
    let mut core = SchedulerCore::new(Config::default());
    let now = std::time::Instant::now();
    submit_pending(&c, &["embed", "extract"], 10, &mut |s| core.submit(s, now)).unwrap();
    let path = d.path().join("t.db");
    let mut ex = ExtractRunner {
        conn: tsr_store::open_encrypted(&path, &[9u8; tsr_store::KEY_LEN]).unwrap(),
        make_ctx: ctx,
    };
    let dsp = core.next(Resource::Model, now, &Gates::default()).unwrap();
    assert!(
        matches!(ex.run(&dsp, None, &CancelToken::new()), Outcome::Deferred { .. }),
        "no sidecar → deferred"
    );
    let model = Scripted::new(vec![Reply::Cancel(CancelReason::Preempted)]);
    assert_eq!(ex.run(&dsp, Some(&model), &CancelToken::new()), Outcome::Preempted);
    assert!(tsr_store::facts::all(&c).unwrap().is_empty());
    assert_eq!(jobs::get(&c, "extract", it).unwrap().unwrap().state, JobState::Queued);
}
