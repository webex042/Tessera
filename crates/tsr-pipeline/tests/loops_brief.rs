//! Open loops and the morning brief (plan §11.2–11.3) on an encrypted store, with a SIMULATED
//! extraction model. Stage 4 exit criteria: no fact is auto-closed without user confirmation;
//! the brief is ready within 30 s.

mod common;

use std::time::Instant;

use common::*;
use serde_json::json;
use tsr_infer::cancel::CancelToken;
use tsr_pipeline::brief::{self, DEFAULT_BRIEF_MINUTE};
use tsr_pipeline::extract::run;
use tsr_pipeline::loops::{self, completion_signal};
use tsr_store::facts::{self, NewEvidence, NewFact};

const ASK: &str = "Re: Q3 budget sheet\nRavi Iyer <ravi.iyer@northwind.example>\nHi Asha, can you send the revised budget sheet?\nAsha Verma\nSure Ravi, I'll send the revised budget sheet by Friday.";
const DONE: &str =
    "Re: Q3 budget sheet\nAsha Verma\nHi Ravi, sent the revised budget sheet just now. Let me know if anything is off.";
const LATER: &str = "Re: Q3 budget sheet\nAsha Verma\nRavi, I will send the revised budget sheet tomorrow morning.";

fn commitment_out() -> String {
    let mut c = fact(
        "commitment",
        "Send Ravi the revised budget sheet",
        "me",
        "I'll send the revised budget sheet by Friday",
    );
    c["counterparty"] = json!("Ravi Iyer");
    c["due_text"] = json!("by Friday");
    out(
        "",
        &[c],
        &[json!({"name": "Ravi Iyer", "handle": "ravi.iyer@northwind.example"})],
    )
}

fn completion_out(evidence: &str, counterparty: &str) -> String {
    let mut e = fact("event", "Sent the revised budget sheet to Ravi", "me", evidence);
    e["counterparty"] = json!(counterparty);
    out("", &[e], &[])
}

#[test]
fn completion_language_is_recognised_conservatively() {
    assert!(completion_signal("Sent the revised budget sheet to Ravi"));
    assert!(completion_signal("Shared the deck with Lena"));
    assert!(!completion_signal("Send the revised budget sheet to Ravi"));
    assert!(!completion_signal("Will send the sheet tomorrow"));
    assert!(!completion_signal("Haven't sent the sheet yet"));
    assert!(!completion_signal("Can you send the sheet?"));
}

#[test]
fn completion_evidence_proposes_done_and_never_closes() {
    let (_d, mut c) = db();
    let bl = blocklist();
    let a = item(&c, T0, ASK);
    let m = Scripted::json(&[&commitment_out()]);
    run(&mut c, a, &m, &ctx(&bl), &CancelToken::new()).unwrap();
    let all = facts::all(&c).unwrap();
    assert_eq!(all.len(), 1);
    let loop_id = all[0].id;
    assert_eq!(all[0].status, "open");

    let b = item(&c, T0 + 3_600_000, DONE);
    let m = Scripted::json(&[&completion_out("sent the revised budget sheet just now", "Ravi Iyer")]);
    let mut cx = ctx(&bl);
    cx.now_ms = T0 + 3_600_000;
    let r = run(&mut c, b, &m, &cx, &CancelToken::new()).unwrap();
    assert_eq!(r.proposed_done, 1);
    let all = facts::all(&c).unwrap();
    assert_eq!(
        all.len(),
        1,
        "the completion is attached to the loop, not stored as another fact"
    );
    assert_eq!(all[0].status, "proposed_done", "proposed, not closed");
    let ev = facts::evidence(&c, loop_id).unwrap();
    assert_eq!(ev.len(), 2);
    assert!(ev.iter().any(|e| e.item_id == b));

    // Nothing in the pipeline ever writes done or dropped.
    assert!(
        facts::all(&c)
            .unwrap()
            .iter()
            .all(|f| f.status != "done" && f.status != "dropped")
    );
}

#[test]
fn plans_and_other_people_do_not_propose_done() {
    let (_d, mut c) = db();
    let bl = blocklist();
    let a = item(&c, T0, ASK);
    run(
        &mut c,
        a,
        &Scripted::json(&[&commitment_out()]),
        &ctx(&bl),
        &CancelToken::new(),
    )
    .unwrap();

    // "I will send …" is a plan, not a completion.
    let b = item(&c, T0 + 60_000, LATER);
    let mut plan = fact(
        "commitment",
        "Send Ravi the revised budget sheet tomorrow",
        "me",
        "I will send the revised budget sheet tomorrow morning",
    );
    plan["counterparty"] = json!("Ravi Iyer");
    let r = run(
        &mut c,
        b,
        &Scripted::json(&[&out("", &[plan], &[])]),
        &ctx(&bl),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(r.proposed_done, 0);

    // A completion for someone else does not touch Ravi's loop.
    let d = item(
        &c,
        T0 + 120_000,
        "Re: budget\nAsha Verma\nLena, sent the revised budget sheet to you.",
    );
    let mut other = fact(
        "event",
        "Sent the revised budget sheet to Lena",
        "me",
        "sent the revised budget sheet to you",
    );
    other["counterparty"] = json!("Lena Park");
    let _ = run(
        &mut c,
        d,
        &Scripted::json(&[&out(
            "",
            &[other],
            &[json!({"name": "Lena Park", "handle": "lena.park@northwind.example"})],
        )]),
        &ctx(&bl),
        &CancelToken::new(),
    )
    .unwrap();
    assert!(
        facts::all(&c)
            .unwrap()
            .iter()
            .filter(|f| f.kind == "commitment")
            .all(|f| f.status == "open"),
        "Ravi's loop stays open"
    );
}

#[test]
fn loops_group_into_proposed_open_and_still_open() {
    let (_d, c) = db();
    let it = item(&c, T0, "Synthetic: send the agenda; book the room; reply to Lena.");
    let add = |text: &str, status: &str, seen: i64| {
        let q = "Synthetic";
        facts::insert_fact(
            &c,
            &NewFact {
                kind: "task",
                text,
                text_norm: text,
                owner_id: None,
                owner_text: Some("me"),
                counterparty_id: None,
                counterparty_text: None,
                due_text: None,
                due: None,
                status,
                confidence: "high",
                prompt_ver: "test",
                model: "test",
                schema_ver: 1,
                redactions: 0,
                redact_ver: "test",
                seen_at: seen,
            },
            &[NewEvidence {
                item_id: it,
                quote: q,
                start_ch: 0,
                end_ch: 9,
                matched: "exact",
            }],
        )
        .unwrap()
    };
    let fresh = add("Send the agenda", "open", T0);
    let old = add("Book the room", "open", T0 - 30 * 86_400_000);
    let maybe = add("Reply to Lena", "proposed_done", T0);
    let g = loops::groups(&c, T0).unwrap();
    assert_eq!(g.open.iter().map(|f| f.id).collect::<Vec<_>>(), vec![fresh]);
    assert_eq!(g.still_open.iter().map(|f| f.id).collect::<Vec<_>>(), vec![old]);
    assert_eq!(g.proposed_done.iter().map(|f| f.id).collect::<Vec<_>>(), vec![maybe]);
}

#[test]
fn brief_is_due_once_per_day_after_brief_time_and_lists_loops() {
    let (_d, mut c) = db();
    let bl = blocklist();
    let tz = jiff::tz::db().get("Asia/Kolkata").unwrap();
    let a = item(&c, T0, ASK);
    run(
        &mut c,
        a,
        &Scripted::json(&[&commitment_out()]),
        &ctx(&bl),
        &CancelToken::new(),
    )
    .unwrap();

    // T0 is Wednesday 2026-10-21 10:05 IST; "by Friday" resolves to Friday 23 Oct.
    let early = T0 - 6 * 3_600_000; // 04:05
    assert_eq!(
        brief::due_day(&c, early, &tz, DEFAULT_BRIEF_MINUTE).unwrap(),
        None,
        "before 05:00"
    );
    let friday = T0 + 2 * 86_400_000;
    assert_eq!(
        brief::due_day(&c, friday, &tz, DEFAULT_BRIEF_MINUTE)
            .unwrap()
            .as_deref(),
        Some("2026-10-23")
    );
    let b = brief::generate(&c, friday, &tz).unwrap();
    assert_eq!(b.due_today.len(), 1);
    assert_eq!(b.commitments_by_me.len(), 1);
    assert_eq!(b.commitments_by_me[0].owner.as_deref(), Some("me"));
    assert!(b.opener.contains("1 item due today"), "{}", b.opener);
    assert_eq!(
        brief::due_day(&c, friday + 60_000, &tz, DEFAULT_BRIEF_MINUTE).unwrap(),
        None,
        "once per day"
    );
    assert_eq!(brief::load(&c, "2026-10-23").unwrap(), Some(b));

    let saturday = friday + 86_400_000;
    let b2 = brief::assemble(&c, saturday, &tz).unwrap();
    assert_eq!(b2.overdue.len(), 1);
    assert!(b2.due_today.is_empty());
}

#[test]
fn brief_on_a_large_store_is_ready_well_within_30_seconds() {
    let (_d, c) = db();
    let tz = jiff::tz::db().get("Asia/Kolkata").unwrap();
    let it = item(&c, T0, "Synthetic source for many facts.");
    for i in 0..3_000 {
        let text = format!("Synthetic task number {i}");
        facts::insert_fact(
            &c,
            &NewFact {
                kind: ["task", "commitment", "question"][i % 3],
                text: &text,
                text_norm: &text,
                owner_id: None,
                owner_text: Some("me"),
                counterparty_id: None,
                counterparty_text: None,
                due_text: None,
                due: (i % 2 == 0).then_some((T0 + (i as i64 - 1500) * 3_600_000, "datetime", false)),
                status: "open",
                confidence: "high",
                prompt_ver: "test",
                model: "test",
                schema_ver: 1,
                redactions: 0,
                redact_ver: "test",
                seen_at: T0,
            },
            &[NewEvidence {
                item_id: it,
                quote: "Synthetic",
                start_ch: 0,
                end_ch: 9,
                matched: "exact",
            }],
        )
        .unwrap();
    }
    let t = Instant::now();
    let b = brief::generate(&c, T0, &tz).unwrap();
    let ms = t.elapsed().as_millis();
    println!("brief over 3000 facts: {ms} ms");
    assert!(ms < 30_000);
    assert_eq!(b.overdue.len(), brief::LIST_LIMIT);
}
