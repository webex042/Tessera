//! Stage 1: sources/items/FTS schema and repository (plan §9.2, §10.3, §15.5).

use rusqlite::Connection;
use tsr_core::Sensitive;
use tsr_store::items::{
    Acquisition, NewItem, NewSource, SourceKind, clean_url, delete_range, fts_query, insert_item, search, upsert_source,
};
use tsr_store::migrate::migrate;
use tsr_store::{KEY_LEN, open_encrypted};

fn db() -> (tempfile::TempDir, Connection) {
    let d = tempfile::tempdir().unwrap();
    let mut c = open_encrypted(&d.path().join("t.db"), &[5u8; KEY_LEN]).unwrap();
    migrate(&mut c).unwrap();
    (d, c)
}

fn src(c: &Connection, title: &str, url: Option<&str>, at: i64) -> i64 {
    upsert_source(
        c,
        &NewSource {
            kind: if url.is_some() {
                SourceKind::Url
            } else {
                SourceKind::App
            },
            app_exe: "chrome.exe",
            app_name: Some("Google Chrome"),
            window_title: Some(title),
            url,
            file_path: None,
            seen_at: at,
        },
    )
    .unwrap()
}

fn item(c: &Connection, source: i64, at: i64, text: &str) -> i64 {
    insert_item(
        c,
        &NewItem {
            source_id: source,
            started_at: at,
            ended_at: at + 1000,
            acquisition: Acquisition::Uia,
            text: &Sensitive::new(text.to_owned()),
            simhash: 42,
            lang: Some("en"),
            redaction_count: 0,
        },
    )
    .unwrap()
}

#[test]
fn sources_are_deduplicated_and_last_seen_advances() {
    let (_d, c) = db();
    let a = src(&c, "Inbox", Some("https://mail.example/inbox"), 100);
    let b = src(&c, "Inbox", Some("https://mail.example/inbox"), 500);
    let other = src(&c, "Inbox", Some("https://mail.example/sent"), 600);
    assert_eq!(a, b);
    assert_ne!(a, other);
    let (first, last): (i64, i64) = c
        .query_row("SELECT first_seen, last_seen FROM sources WHERE id = ?1", [a], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((first, last), (100, 500));
}

#[test]
fn urls_lose_fragments_and_credential_parameters() {
    assert_eq!(
        clean_url("https://app.example/cb#access_token=abc&x=1"),
        "https://app.example/cb"
    );
    assert_eq!(
        clean_url("https://d.example/f?id=7&sig=XYZ&X-Amz-Signature=1&page=2"),
        "https://d.example/f?id=7&page=2"
    );
    assert_eq!(
        clean_url("https://login.example/?code=123&state=abc"),
        "https://login.example/"
    );
    assert_eq!(
        clean_url("https://news.example/a?topic=ai"),
        "https://news.example/a?topic=ai"
    );
    let (_d, c) = db();
    let id = src(&c, "x", Some("https://d.example/f?id=7&token=SECRET"), 1);
    let url: String = c
        .query_row("SELECT url FROM sources WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(url, "https://d.example/f?id=7");
}

#[test]
fn search_finds_items_including_hinglish_and_hindi() {
    let (_d, c) = db();
    let s = src(&c, "Gmail", None, 1);
    let a = item(&c, s, 1_000, "Sure Ravi, I'll send the revised budget sheet by Friday.");
    let b = item(&c, s, 2_000, "kal subah 10 baje tak slides bhej dungi");
    let h = item(&c, s, 3_000, "रविवार को मामा जी के घर लंच है");
    let ids = |q: &str| {
        search(&c, q, 10)
            .unwrap()
            .into_iter()
            .map(|h| h.item_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("budget friday"), vec![a]);
    assert_eq!(ids("Budget, FRIDAY!"), vec![a]);
    assert_eq!(ids("slides bhej"), vec![b]);
    assert_eq!(ids("मामा"), vec![h]);
    assert!(ids("budget slides").is_empty(), "terms are ANDed");
    let hit = &search(&c, "budget", 1).unwrap()[0];
    assert!(hit.snippet.expose().contains("[budget]"));
    assert_eq!(format!("{:?}", hit.snippet), "<redacted>");
}

#[test]
fn hostile_queries_are_plain_text() {
    let (_d, c) = db();
    let s = src(&c, "x", None, 1);
    item(&c, s, 1, "alpha beta gamma");
    for q in [
        "\"",
        "alpha OR",
        "NEAR(alpha beta)",
        "text:alpha",
        "alpha*",
        "-alpha",
        "^alpha",
        "\"alpha\" AND \"",
        "' OR 1=1 --",
        "",
    ] {
        assert!(search(&c, q, 10).is_ok(), "query {q:?} must not error");
    }
    assert_eq!(
        fts_query("text:alpha OR beta"),
        Some("\"text\" \"alpha\" \"OR\" \"beta\"".into())
    );
    assert_eq!(fts_query("  ,,  "), None);
}

#[test]
fn delete_range_counts_first_then_removes_items_index_entries_and_orphan_sources() {
    let (_d, mut c) = db();
    let s1 = src(&c, "Old", None, 1);
    let s2 = src(&c, "New", None, 1);
    item(&c, s1, 1_000, "tsrcanaryoldsecretterm one");
    item(&c, s1, 2_000, "tsrcanaryoldsecretterm two");
    item(&c, s2, 9_000, "kept forever");
    assert_eq!(delete_range(&mut c, 0, 5_000, true).unwrap(), 2);
    assert_eq!(
        search(&c, "tsrcanaryoldsecretterm", 10).unwrap().len(),
        2,
        "dry run changes nothing"
    );
    assert_eq!(delete_range(&mut c, 0, 5_000, false).unwrap(), 2);
    assert!(search(&c, "tsrcanaryoldsecretterm", 10).unwrap().is_empty());
    assert_eq!(search(&c, "kept", 10).unwrap().len(), 1);
    // The term is gone from the index vocabulary, not just unreachable.
    c.execute_batch("CREATE VIRTUAL TABLE temp.vocab USING fts5vocab(main, items_fts, 'row');")
        .unwrap();
    let n: i64 = c
        .query_row(
            "SELECT count(*) FROM temp.vocab WHERE term = 'tsrcanaryoldsecretterm'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
    let sources: i64 = c.query_row("SELECT count(*) FROM sources", [], |r| r.get(0)).unwrap();
    assert_eq!(sources, 1, "the source with no items left is removed");
    let secure: i64 = c
        .query_row("SELECT v FROM items_fts_config WHERE k = 'secure-delete'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(secure, 1);
}

#[test]
fn updates_keep_the_index_in_sync() {
    let (_d, c) = db();
    let s = src(&c, "x", None, 1);
    let id = item(&c, s, 1, "draft wording");
    c.execute(
        "UPDATE items SET text = 'final wording', summary = 'summary about pricing' WHERE id = ?1",
        [id],
    )
    .unwrap();
    assert!(search(&c, "draft", 10).unwrap().is_empty());
    assert_eq!(search(&c, "final", 10).unwrap().len(), 1);
    assert_eq!(search(&c, "pricing", 10).unwrap().len(), 1, "summaries are searchable");
}
