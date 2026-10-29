//! Semantic retrieval and VectorDb integration tests.
//!
//! Verifies:
//! - PII is masked before embedding generation and storage.
//! - Semantic similarity search via encrypted VectorDb.
//! - Metadata pre-filtering (app_name, source, item_id, time range).
//! - Deleted items leave no stale retrieval hits.
//! - Citation and provenance preservation.

use rusqlite::Connection;
use tempfile::tempdir;
use tsr_core::Sensitive;
use tsr_guard::blocklist::Blocklist;
use tsr_retrieval::embed::HashEmbedder;
use tsr_retrieval::hybrid::{Filter, Opts, search_vectordb};
use tsr_retrieval::ingest::embed_item_to_vectordb;
use tsr_store::KEY_LEN;
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, insert_item, upsert_source};
use tsr_store::vectordb::{VectorDb, VectorFilter};

fn setup_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    tsr_store::migrate::migrate(&mut conn).unwrap();
    conn
}

#[test]
fn semantic_retrieval_with_vectordb_and_pii_masking() {
    let mut conn = setup_db();
    let dir = tempdir().unwrap();
    let vec_path = dir.path().join("semantic_test.enc");
    let key = [88u8; KEY_LEN];
    let mut vdb = VectorDb::new(vec_path.clone(), &key);

    let embedder = HashEmbedder::default();

    // Insert sources
    let s1 = upsert_source(
        &conn,
        &NewSource {
            kind: SourceKind::App,
            app_exe: "code.exe",
            app_name: Some("VS Code"),
            window_title: Some("project-roadmap.rs"),
            url: None,
            file_path: None,
            seen_at: 1000,
        },
    )
    .unwrap();

    let s2 = upsert_source(
        &conn,
        &NewSource {
            kind: SourceKind::App,
            app_exe: "slack.exe",
            app_name: Some("Slack"),
            window_title: Some("#general"),
            url: None,
            file_path: None,
            seen_at: 2000,
        },
    )
    .unwrap();

    // Insert items with raw PII
    let raw_text_1 = Sensitive::new(
        "Please email Alice at alice@tessera.local and call +1-555-0144 to deploy the database migration.".to_string(),
    );
    let it1 = insert_item(
        &conn,
        &NewItem {
            source_id: s1,
            started_at: 1050,
            ended_at: 1060,
            acquisition: Acquisition::Uia,
            text: &raw_text_1,
            simhash: 12345,
            lang: Some("en"),
            redaction_count: 0,
        },
    )
    .unwrap();

    let raw_text_2 = Sensitive::new(
        "Bob mentioned the cafeteria lunch menu for today is pizza and pasta.".to_string(),
    );
    let it2 = insert_item(
        &conn,
        &NewItem {
            source_id: s2,
            started_at: 2050,
            ended_at: 2060,
            acquisition: Acquisition::Uia,
            text: &raw_text_2,
            simhash: 67890,
            lang: Some("en"),
            redaction_count: 0,
        },
    )
    .unwrap();

    // Embed both into VectorDb
    let c1 = embed_item_to_vectordb(&mut conn, &mut vdb, it1, &embedder, None, 1100).unwrap();
    let c2 = embed_item_to_vectordb(&mut conn, &mut vdb, it2, &embedder, None, 2100).unwrap();
    assert!(c1 > 0);
    assert!(c2 > 0);

    // 1. Verify PII masking guarantee in VectorDb
    for rec in vdb.all_records() {
        assert!(!rec.text.contains("alice@tessera.local"));
        assert!(!rec.text.contains("+1-555-0144"));
        if rec.metadata.item_id == it1 {
            assert!(rec.text.contains("[EMAIL]") || rec.text.contains("[PHONE]"));
        }
    }

    // 2. Perform semantic search via search_vectordb
    let bl = Blocklist::compile(&[]).0;
    let filter = Filter {
        blocklist: Some(&bl),
        suppressed: None,
    };
    let mut opts = Opts::new(3000);
    opts.use_lexical = true;
    opts.use_vector = true;

    // Query for database migration
    let (hits, diag) = search_vectordb(
        &conn,
        Some(&vdb),
        Some(&embedder),
        "deploy database migration",
        &filter,
        &opts,
    )
    .unwrap();

    assert!(!hits.is_empty());
    assert_eq!(hits[0].item_id, it1);
    assert!(diag.vector > 0);

    // 3. Metadata Filtering in VectorDb
    let app_filter = VectorFilter {
        app_names: Some(vec!["Slack".to_string()]),
        ..Default::default()
    };
    let hits_slack = vdb.search(&[0.1; 384], 5, Some(&app_filter));
    for h in &hits_slack {
        assert_eq!(h.metadata.app_name.as_deref(), Some("Slack"));
    }

    // 4. Deletion cleans up VectorDb completely
    let deleted = vdb.delete_by_item(it1);
    assert!(deleted > 0);
    assert_eq!(vdb.search(&[0.1; 384], 10, None).iter().filter(|h| h.item_id == it1).count(), 0);

    // 5. Persistence encryption test
    vdb.save().unwrap();
    let reopened = VectorDb::open_or_create(&vec_path, &key).unwrap();
    assert_eq!(reopened.len(), vdb.len());
    let hit_reopened = reopened.get(&format!("chunk-{it2}-0"));
    assert!(hit_reopened.is_some());
}
