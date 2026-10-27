//! Facts, evidence, people, suppression, jobs and chunks on a real encrypted database.

use rusqlite::Connection;
use tsr_core::Sensitive;
use tsr_store::facts::{self, NewEvidence, NewFact};
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, delete_range, insert_item, upsert_source};
use tsr_store::jobs::{self, JobState};
use tsr_store::migrate::migrate;
use tsr_store::people::{self, AliasKind};
use tsr_store::{KEY_LEN, StoreError, chunks, open_encrypted};

fn db() -> (tempfile::TempDir, Connection) {
    let d = tempfile::tempdir().unwrap();
    let mut c = open_encrypted(&d.path().join("t.db"), &[7u8; KEY_LEN]).unwrap();
    migrate(&mut c).unwrap();
    (d, c)
}

fn item(c: &Connection, at: i64, text: &str) -> i64 {
    let s = upsert_source(
        c,
        &NewSource {
            kind: SourceKind::App,
            app_exe: "slack.exe",
            app_name: Some("Slack"),
            window_title: Some("#design"),
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
            ended_at: at,
            acquisition: Acquisition::Uia,
            text: &Sensitive::new(text.into()),
            simhash: 1,
            lang: None,
            redaction_count: 0,
        },
    )
    .unwrap()
}

fn fact<'a>(text: &'a str, owner: Option<i64>, at: i64) -> NewFact<'a> {
    NewFact {
        kind: "commitment",
        text,
        text_norm: text,
        owner_id: owner,
        owner_text: None,
        counterparty_id: None,
        counterparty_text: None,
        due_text: None,
        due: None,
        status: "open",
        confidence: "high",
        prompt_ver: "extraction/v1",
        model: "test",
        schema_ver: 1,
        redactions: 0,
        redact_ver: "v1",
        seen_at: at,
    }
}

fn ev(item_id: i64, quote: &str, start: i64) -> NewEvidence<'_> {
    NewEvidence {
        item_id,
        quote,
        start_ch: start,
        end_ch: start + quote.chars().count() as i64,
        matched: "exact",
    }
}

#[test]
fn fact_requires_evidence_and_valid_references() {
    let (_d, c) = db();
    assert!(matches!(
        facts::insert_fact(&c, &fact("x", None, 1), &[]),
        Err(StoreError::Invalid(_))
    ));
    // Evidence pointing at an item that does not exist is refused by the foreign key.
    assert!(facts::insert_fact(&c, &fact("x", None, 1), &[ev(999, "q", 0)]).is_err());
}

#[test]
fn check_constraints_reject_schema_invalid_rows() {
    let (_d, c) = db();
    let it = item(&c, 1, "send it");
    let long_due = "x".repeat(61);
    let cases: Vec<(&str, NewFact)> = vec![
        (
            "bad kind",
            NewFact {
                kind: "reminder",
                ..fact("a", None, 1)
            },
        ),
        ("empty text", fact("", None, 1)),
        (
            "bad status",
            NewFact {
                status: "closed",
                ..fact("a", None, 1)
            },
        ),
        (
            "bad confidence",
            NewFact {
                confidence: "sure",
                ..fact("a", None, 1)
            },
        ),
        (
            "due without precision",
            NewFact {
                due: Some((5, "", false)),
                ..fact("a", None, 1)
            },
        ),
        (
            "bad precision",
            NewFact {
                due: Some((5, "year", false)),
                ..fact("a", None, 1)
            },
        ),
        (
            "long due text",
            NewFact {
                due_text: Some(&long_due),
                ..fact("a", None, 1)
            },
        ),
    ];
    let long = "x".repeat(201);
    for (why, f) in cases.iter().chain([("long text", fact(&long, None, 1))].iter()) {
        assert!(facts::insert_fact(&c, f, &[ev(it, "send it", 0)]).is_err(), "{why}");
    }
    let n: i64 = c.query_row("SELECT count(*) FROM facts", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn evidence_is_idempotent_and_merges_count_once() {
    let (_d, c) = db();
    let it = item(&c, 1, "I'll send the sheet");
    let it2 = item(&c, 2, "I'll send the sheet by Friday");
    let id = facts::insert_fact(
        &c,
        &fact("Send the sheet", None, 1),
        &[ev(it, "I'll send the sheet", 0)],
    )
    .unwrap();
    // Retrying the same job adds nothing.
    assert!(!facts::merge_into(&c, id, &[ev(it, "I'll send the sheet", 0)], None, 1).unwrap());
    // A new sighting adds evidence, advances last_seen, fills the missing due date once.
    assert!(
        facts::merge_into(
            &c,
            id,
            &[ev(it2, "I'll send the sheet by Friday", 0)],
            Some((Some("by Friday"), 99, "date", false)),
            5
        )
        .unwrap()
    );
    assert!(
        !facts::merge_into(
            &c,
            id,
            &[ev(it2, "I'll send the sheet by Friday", 0)],
            Some((Some("x"), 1, "date", false)),
            6
        )
        .unwrap()
    );
    let f = facts::get(&c, id).unwrap().unwrap();
    assert_eq!((f.merged_count, f.last_seen, f.due_at), (1, 5, Some(99)));
    assert_eq!(facts::evidence(&c, id).unwrap().len(), 2);
}

#[test]
fn fact_disappears_with_its_last_evidence() {
    let (_d, mut c) = db();
    let a = item(&c, 1_000, "I'll send the sheet");
    let b = item(&c, 5_000, "sending the sheet now");
    let id = facts::insert_fact(&c, &fact("Send the sheet", None, 1), &[ev(a, "I'll send the sheet", 0)]).unwrap();
    facts::merge_into(&c, id, &[ev(b, "sending the sheet", 0)], None, 5).unwrap();
    // Deleting one source item keeps the fact (other evidence remains)...
    delete_range(&mut c, 0, 2_000, false).unwrap();
    assert!(facts::get(&c, id).unwrap().is_some());
    assert_eq!(facts::evidence(&c, id).unwrap().len(), 1);
    // ...deleting the last one removes the fact and its search entry.
    delete_range(&mut c, 0, 10_000, false).unwrap();
    assert!(facts::get(&c, id).unwrap().is_none());
    assert!(facts::search(&c, "sheet", 10).unwrap().is_empty());
}

#[test]
fn failed_transaction_leaves_no_partial_fact() {
    let (_d, mut c) = db();
    let it = item(&c, 1, "text");
    let tx = c.transaction().unwrap();
    facts::insert_fact(&tx, &fact("First", None, 1), &[ev(it, "text", 0)]).unwrap();
    // Second fact fails (bad evidence span) → the caller rolls everything back.
    let bad = NewEvidence {
        end_ch: 0,
        ..ev(it, "text", 0)
    };
    assert!(facts::insert_fact(&tx, &fact("Second", None, 1), &[bad]).is_err());
    drop(tx);
    assert!(facts::all(&c).unwrap().is_empty());
}

#[test]
fn conflicting_facts_from_different_sources_are_both_kept() {
    let (_d, c) = db();
    let a = item(&c, 1, "deadline is Friday");
    let b = item(&c, 2, "deadline moved to Monday");
    let f1 = facts::insert_fact(
        &c,
        &NewFact {
            due_text: Some("Friday"),
            due: Some((10, "date", false)),
            ..fact("Report deadline", None, 1)
        },
        &[ev(a, "deadline is Friday", 0)],
    )
    .unwrap();
    let f2 = facts::insert_fact(
        &c,
        &NewFact {
            due_text: Some("Monday"),
            due: Some((20, "date", false)),
            ..fact("Report deadline", None, 2)
        },
        &[ev(b, "deadline moved to Monday", 0)],
    )
    .unwrap();
    assert_ne!(f1, f2);
    assert_eq!(facts::all(&c).unwrap().len(), 2);
}

#[test]
fn concurrent_writers_do_not_lose_or_duplicate_evidence() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("t.db");
    let mut c = open_encrypted(&path, &[7u8; KEY_LEN]).unwrap();
    migrate(&mut c).unwrap();
    let it = item(&c, 1, "shared text");
    let id = facts::insert_fact(&c, &fact("Shared", None, 1), &[ev(it, "shared text", 0)]).unwrap();
    let mut hs = vec![];
    for t in 0..4 {
        let p = path.clone();
        hs.push(std::thread::spawn(move || {
            let c = open_encrypted(&p, &[7u8; KEY_LEN]).unwrap();
            c.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
            for i in 0..10 {
                // Everyone adds the same 10 spans; only 10 rows may exist afterwards.
                let q = format!("span {i}");
                let _ = t;
                facts::merge_into(
                    &c,
                    id,
                    &[NewEvidence {
                        item_id: it,
                        quote: &q,
                        start_ch: i + 1,
                        end_ch: i + 2,
                        matched: "fuzzy",
                    }],
                    None,
                    2,
                )
                .unwrap();
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(facts::evidence(&c, id).unwrap().len(), 11);
    assert_eq!(facts::get(&c, id).unwrap().unwrap().merged_count, 10);
}

#[test]
fn names_may_repeat_but_emails_identify() {
    let (_d, c) = db();
    let a = people::create_person(
        &c,
        "Ravi Iyer",
        &[
            (AliasKind::Name, "ravi iyer"),
            (AliasKind::Email, "ravi@northwind.example"),
        ],
        1,
    )
    .unwrap();
    let b = people::create_person(&c, "Ravi Iyer", &[(AliasKind::Name, "ravi iyer")], 1).unwrap();
    assert_eq!(
        people::find_by_alias(&c, AliasKind::Name, "ravi iyer").unwrap(),
        vec![a, b]
    );
    assert!(
        !people::add_alias(&c, b, AliasKind::Email, "ravi@northwind.example").unwrap(),
        "e-mail belongs to a"
    );
    let me = people::ensure_self(&c, "Asha Verma", &[(AliasKind::Name, "asha verma")], 1).unwrap();
    assert_eq!(
        people::ensure_self(&c, "Asha", &[(AliasKind::Name, "asha")], 2).unwrap(),
        me,
        "one self person"
    );
    people::suggest_merge(&c, b, a, "same_name", 3).unwrap();
    people::suggest_merge(&c, a, b, "same_name", 3).unwrap();
    assert_eq!(
        people::merge_suggestions(&c).unwrap(),
        vec![(a, b, "same_name".to_string())]
    );
}

#[test]
fn user_confirmed_merge_moves_aliases_and_facts() {
    let (_d, mut c) = db();
    let it = item(&c, 1, "x");
    let a = people::create_person(&c, "Meera Nair", &[(AliasKind::Name, "meera nair")], 1).unwrap();
    let b = people::create_person(
        &c,
        "Meera",
        &[(AliasKind::Name, "meera"), (AliasKind::Handle, "@meera")],
        1,
    )
    .unwrap();
    let f = facts::insert_fact(&c, &fact("Owner b", Some(b), 1), &[ev(it, "x", 0)]).unwrap();
    people::merge_people(&mut c, a, b).unwrap();
    assert_eq!(facts::get(&c, f).unwrap().unwrap().owner_id, Some(a));
    assert_eq!(people::find_by_alias(&c, AliasKind::Handle, "@meera").unwrap(), vec![a]);
    assert!(people::person(&c, b).unwrap().is_none());
}

#[test]
fn forgetting_a_person_suppresses_without_keeping_names() {
    let (_d, mut c) = db();
    let it = item(&c, 1, "Kenji will fix it");
    let k = people::create_person(
        &c,
        "Kenji Watanabe",
        &[(AliasKind::Name, "kenji watanabe"), (AliasKind::Name, "kenji")],
        1,
    )
    .unwrap();
    facts::insert_fact(
        &c,
        &fact("Kenji fixes export", Some(k), 1),
        &[ev(it, "Kenji will fix it", 0)],
    )
    .unwrap();
    let dry = people::forget_person(&mut c, k, true, 2).unwrap();
    assert_eq!((dry.facts, dry.aliases, dry.people), (1, 2, 0));
    assert!(people::person(&c, k).unwrap().is_some(), "dry run changes nothing");
    let r = people::forget_person(&mut c, k, false, 2).unwrap();
    assert_eq!((r.facts, r.aliases, r.people), (1, 2, 1));
    assert!(facts::all(&c).unwrap().is_empty());
    let salt = people::suppression_salt(&c).unwrap();
    assert_eq!(salt.len(), 64);
    let set = people::suppressed_digests(&c).unwrap();
    assert!(set.contains(&(AliasKind::Name, people::alias_digest(&salt, AliasKind::Name, "kenji"))));
    // The item itself stays (the user deletes mentions explicitly), and no table holds the name.
    let n: i64 = c.query_row("SELECT count(*) FROM items", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
    for t in ["people", "person_aliases", "suppressed_aliases", "facts", "settings"] {
        let dump: String = c
            .query_row(
                &format!("SELECT coalesce(group_concat(quote(x.*)), '') FROM (SELECT * FROM {t}) x"),
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
        assert!(!dump.to_lowercase().contains("kenji"), "{t} still holds the name");
    }
    // The self person cannot be forgotten through this path.
    let me = people::ensure_self(&c, "Asha", &[], 3).unwrap();
    assert_eq!(people::forget_person(&mut c, me, false, 3).unwrap().people, 0);
    assert!(people::person(&c, me).unwrap().is_some());
}

#[test]
fn salts_differ_per_database() {
    let (_d1, a) = db();
    let (_d2, b) = db();
    assert_ne!(
        people::suppression_salt(&a).unwrap(),
        people::suppression_salt(&b).unwrap()
    );
}

#[test]
fn jobs_are_unique_resumable_and_content_free() {
    let (_d, c) = db();
    let it = item(&c, 1, "x");
    let (id, new) = jobs::enqueue(&c, "extract", it, 1).unwrap();
    assert!(new);
    assert_eq!(jobs::enqueue(&c, "extract", it, 2).unwrap(), (id, false));
    assert!(jobs::start(&c, id, 3).unwrap());
    jobs::set_stage(&c, id, "validate", 3).unwrap();
    // Crash: running goes back to queued on the next start.
    assert_eq!(jobs::requeue_interrupted(&c, 4).unwrap(), 1);
    assert_eq!(jobs::queued(&c, "extract", 10).unwrap(), vec![(id, it)]);
    jobs::finish(&c, id, JobState::Done, None, 5).unwrap();
    assert!(!jobs::start(&c, id, 6).unwrap(), "finished jobs do not run again");
    assert!(jobs::finish(&c, id, JobState::Failed, Some("Ravi's text leaked"), 7).is_err());
    let j = jobs::get(&c, "extract", it).unwrap().unwrap();
    assert_eq!((j.state, j.attempts), (JobState::Done, 1));
    assert!(jobs::enqueue(&c, "summarise", it, 1).is_err());
}

#[test]
fn chunks_are_per_model_and_replaced_atomically() {
    let (_d, c) = db();
    let it = item(&c, 1, "hello world");
    let ch = |ord, v: Vec<i8>| chunks::NewChunk {
        ord,
        start_ch: 0,
        end_ch: 5,
        scale: 0.01,
        vec: v,
    };
    chunks::replace_for_item(&c, it, "e5-small", "c1", &[ch(0, vec![1, 2, 3]), ch(1, vec![4, 5, 6])]).unwrap();
    chunks::replace_for_item(&c, it, "other-model", "c1", &[ch(0, vec![9; 5])]).unwrap();
    chunks::replace_for_item(&c, it, "e5-small", "c1", &[ch(0, vec![7, 8, 9])]).unwrap();
    let rows = chunks::all_for_model(&c, "e5-small").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].vec, vec![7, 8, 9]);
    assert_eq!(
        chunks::models(&c).unwrap(),
        vec![("e5-small".into(), 1), ("other-model".into(), 1)]
    );
    let bad = chunks::NewChunk {
        scale: f32::NAN,
        ..ch(0, vec![1])
    };
    assert!(chunks::replace_for_item(&c, it, "e5-small", "c1", &[bad]).is_err());
}
