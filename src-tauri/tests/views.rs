//! The notch views' data, on a real encrypted store with synthetic items and facts.

use std::time::{Duration, Instant};

use rusqlite::Connection;
use tessara_lib::views::{Core, E_INVALID, E_NOT_FOUND, E_TOKEN, day_range};
use tsr_core::Sensitive;
use tsr_store::facts::{NewEvidence, NewFact, insert_fact};
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, insert_item, upsert_source};
use tsr_store::migrate::migrate;
use tsr_store::{KEY_LEN, open_encrypted};

fn tz() -> jiff::tz::TimeZone {
    jiff::tz::db().get("Asia/Kolkata").unwrap()
}

/// 2026-10-21 10:05 IST.
const NOW: i64 = 1_792_557_300_000;
const H: i64 = 3_600_000;

fn setup() -> (tempfile::TempDir, Core, i64, i64) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("t.db");
    let mut w = open_encrypted(&p, &[5u8; KEY_LEN]).unwrap();
    migrate(&mut w).unwrap();
    let it = |c: &Connection, at: i64, app: &str, text: &str| {
        let s = upsert_source(
            c,
            &NewSource {
                kind: SourceKind::App,
                app_exe: app,
                app_name: Some(app),
                window_title: Some("Window"),
                url: None,
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
                ended_at: at + 60_000,
                acquisition: Acquisition::Uia,
                text: &Sensitive::new(text.into()),
                simhash: 0,
                lang: None,
                redaction_count: 0,
            },
        )
        .unwrap()
    };
    let a = it(
        &w,
        NOW - 2 * H,
        "Outlook",
        "Ravi asked for the revised budget sheet by Friday",
    );
    let b = it(&w, NOW - 26 * H, "Slack", "design review notes from yesterday");
    let fact = |c: &Connection, kind: &str, text: &str, due: Option<i64>, status: &str, item: i64| {
        insert_fact(
            c,
            &NewFact {
                kind,
                text,
                text_norm: text,
                owner_id: None,
                owner_text: None,
                counterparty_id: None,
                counterparty_text: Some("Ravi Iyer"),
                due_text: due.map(|_| "by Friday"),
                due: due.map(|d| (d, "date", false)),
                status,
                confidence: "high",
                prompt_ver: "extraction/v1",
                model: "test",
                schema_ver: 1,
                redactions: 0,
                redact_ver: "v1",
                seen_at: NOW - H,
            },
            &[NewEvidence {
                item_id: item,
                quote: "revised budget sheet",
                start_ch: 15,
                end_ch: 35,
                matched: "exact",
            }],
        )
        .unwrap()
    };
    let due_today = fact(&w, "commitment", "Send revised budget", Some(NOW + 3 * H), "open", a);
    fact(&w, "task", "Overdue thing", Some(NOW - 30 * H), "open", a);
    fact(&w, "task", "Check this", None, "needs_review", b);
    fact(&w, "note", "A note", None, "open", b);
    drop(w);
    let r = open_encrypted(&p, &[5u8; KEY_LEN]).unwrap();
    (d, Core::new(r), due_today, a)
}

#[test]
fn today_groups_due_overdue_review_and_timeline() {
    let (_d, core, due, item) = setup();
    let t = core.today(NOW, &tz()).unwrap();
    assert_eq!(t.due_today.iter().map(|f| f.id).collect::<Vec<_>>(), vec![due]);
    assert_eq!(t.overdue.len(), 1);
    assert_eq!(t.needs_review.len(), 1, "needs_review is shown apart, never as a task");
    assert_eq!(t.due_today[0].counterparty.as_deref(), Some("Ravi Iyer"));
    assert_eq!(t.due_today[0].item_id, Some(item), "drill-down target");
    assert_eq!(t.timeline.len(), 1, "only today's item");
    assert_eq!(t.timeline[0].hour, 8);
    let c = core.counts(NOW, &tz()).unwrap();
    assert_eq!(
        (c.due_today, c.overdue, c.needs_review, c.items_today, c.open),
        (1, 1, 1, 1, 2)
    );
}

#[test]
fn day_boundaries_follow_the_time_zone() {
    let ist = day_range(NOW, &tz()).unwrap();
    let ny = day_range(NOW, &jiff::tz::db().get("America/New_York").unwrap()).unwrap();
    assert_ne!(ist, ny);
    assert_eq!(ist.end - ist.start, 24 * H);
    // A DST-change day in New York is 25 hours long.
    let fall = 1_793_534_400_000; // 2026-11-01 12:00 UTC
    let d = day_range(fall, &jiff::tz::db().get("America/New_York").unwrap()).unwrap();
    assert_eq!(d.end - d.start, 25 * H);
}

#[test]
fn open_loops_and_input_validation() {
    let (_d, core, _, _) = setup();
    let l = core.open_loops(None).unwrap();
    assert_eq!(l.len(), 3, "tasks, commitments and review items; notes excluded");
    assert!(l[0].due_at.is_some(), "dated first");
    assert_eq!(core.open_loops(Some("commitment")).unwrap().len(), 1);
    assert_eq!(core.open_loops(Some("'; DROP TABLE facts; --")).unwrap_err(), E_INVALID);
    assert_eq!(core.search("", None, 10).unwrap_err(), E_INVALID);
    assert_eq!(core.search(&"x".repeat(201), None, 10).unwrap_err(), E_INVALID);
    assert_eq!(core.item(99_999).unwrap_err(), E_NOT_FOUND);
}

#[test]
fn search_finds_items_and_filters_by_app() {
    let (_d, core, _, item) = setup();
    let h = core.search("what about the budget sheet?", None, 10).unwrap();
    assert_eq!(h[0].item.item_id, item);
    assert!(core.search("budget", Some("Slack"), 10).unwrap().is_empty());
    let d = core.item(item).unwrap();
    assert_eq!(d.facts.len(), 2);
}

#[test]
fn status_changes_need_a_matching_single_use_unexpired_token() {
    let (_d, core, due, _) = setup();
    let t0 = Instant::now();
    assert_eq!(
        core.set_fact_status(due, "done", "made-up-token", t0).unwrap_err(),
        E_TOKEN
    );
    assert_eq!(
        core.propose_fact_status(due, "proposed_done", t0).unwrap_err(),
        E_INVALID,
        "code-only status"
    );
    let tok = core.propose_fact_status(due, "done", t0).unwrap();
    assert_eq!(
        core.set_fact_status(due, "dropped", &tok, t0).unwrap_err(),
        E_TOKEN,
        "token is for 'done' only"
    );
    let tok = core.propose_fact_status(due, "done", t0).unwrap();
    let f = core.set_fact_status(due, "done", &tok, t0).unwrap();
    assert_eq!(f.status, "done");
    assert_eq!(
        core.set_fact_status(due, "done", &tok, t0).unwrap_err(),
        E_TOKEN,
        "single use"
    );
    let tok = core.propose_fact_status(due, "open", t0).unwrap();
    assert_eq!(
        core.set_fact_status(due, "open", &tok, t0 + Duration::from_secs(121))
            .unwrap_err(),
        E_TOKEN,
        "expired"
    );
}

#[test]
fn bulk_dismiss_needs_a_token_for_exactly_that_set() {
    let (_d, core, _, _) = setup();
    // A month later every open loop is stale ("Still open?").
    let later = NOW + 30 * 24 * H;
    let g = core.loop_groups(later).unwrap();
    assert!(g.open.is_empty() && g.proposed_done.is_empty());
    let ids: Vec<i64> = g.still_open.iter().map(|f| f.id).collect();
    assert_eq!(
        ids.len(),
        2,
        "task and commitment; review items and notes are not loops"
    );
    let t0 = Instant::now();
    let tok = core.propose_bulk_status(&ids, "dropped", t0).unwrap();
    assert_eq!(
        core.set_bulk_status(&ids[..1], "dropped", &tok, t0).unwrap_err(),
        E_TOKEN,
        "a subset is not the approved set"
    );
    let tok = core.propose_bulk_status(&ids, "dropped", t0).unwrap();
    let rev: Vec<i64> = ids.iter().rev().copied().collect();
    assert_eq!(
        core.set_bulk_status(&rev, "dropped", &tok, t0).unwrap(),
        2,
        "order does not matter"
    );
    assert_eq!(
        core.set_bulk_status(&ids, "dropped", &tok, t0).unwrap_err(),
        E_TOKEN,
        "single use"
    );
    assert!(core.loop_groups(later).unwrap().still_open.is_empty());
    assert_eq!(
        core.propose_bulk_status(&[ids[0], 99_999], "dropped", t0).unwrap_err(),
        E_NOT_FOUND
    );
    assert_eq!(
        core.propose_bulk_status(&ids, "proposed_done", t0).unwrap_err(),
        E_INVALID
    );
    assert_eq!(core.propose_bulk_status(&[], "dropped", t0).unwrap_err(), E_INVALID);
}

#[test]
fn week_and_month_views_drill_down_to_items() {
    let (d, core, _, item) = setup();
    // Roll up (code-only) as the app would a week later.
    let mut w = open_encrypted(&d.path().join("t.db"), &[5u8; KEY_LEN]).unwrap();
    let tz = tz();
    let ctx = tsr_pipeline::rollup::RollupCtx {
        tz: &tz,
        redact: tsr_guard::redact::Options::default(),
        model_id: "extractive",
        now_ms: NOW + 8 * 24 * H,
    };
    tsr_pipeline::rollup::run_all_due(&mut w, None, &ctx, 0, &tsr_infer::cancel::CancelToken::new()).unwrap();
    drop(w);

    let week = core.rollups("week", NOW, &tz).unwrap();
    assert_eq!(week.children.len(), 7);
    assert!(week.period.summary.is_some());
    let days: Vec<_> = week.children.iter().filter(|c| c.summary.is_some()).collect();
    assert_eq!(days.len(), 2, "the two days with items");
    let ids: Vec<i64> = days
        .iter()
        .flat_map(|c| {
            c.summary
                .as_ref()
                .unwrap()
                .highlights
                .iter()
                .flat_map(|h| h.item_ids.clone())
        })
        .collect();
    assert!(ids.contains(&item));
    let cards = core.items_by_ids(&[item, 99_999]).unwrap();
    assert_eq!(cards.len(), 1, "missing ids are skipped");
    assert_eq!(cards[0].item_id, item);

    let month = core.rollups("month", NOW, &tz).unwrap();
    assert!((4..=6).contains(&month.children.len()), "weeks overlapping October");
    assert_eq!(core.rollups("hour", NOW, &tz).unwrap_err(), E_INVALID);
    assert_eq!(core.items_by_ids(&[]).unwrap_err(), E_INVALID);

    let b = core.brief(NOW, &tz).unwrap();
    assert!(!b.opener.is_empty());
    assert_eq!(b.due_today.len(), 1);
}
