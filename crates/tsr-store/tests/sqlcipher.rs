//! Stage 0 checks for SQLCipher + FTS5 (plan D5, §9.1; Stage 1 exit criterion
//! "DB file is unreadable without the key").

use tsr_store::{KEY_LEN, StoreError, build_info, open_encrypted};

const CANARY: &str = "TSRCANARY-7f3a91c2-store";

fn key(byte: u8) -> [u8; KEY_LEN] {
    [byte; KEY_LEN]
}

#[test]
fn sqlcipher_and_fts5_are_linked() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_encrypted(&dir.path().join("t.db"), &key(7)).unwrap();
    let info = build_info(&conn).unwrap();
    println!(
        "sqlite {} / sqlcipher {} / provider {}",
        info.sqlite_version, info.cipher_version, info.cipher_provider
    );
    assert!(info.has_fts5);
    assert_eq!(info.cipher_provider, "openssl");
}

#[test]
fn fts5_round_trip_in_encrypted_db() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_encrypted(&dir.path().join("t.db"), &key(1)).unwrap();
    conn.execute_batch(
        "CREATE VIRTUAL TABLE notes USING fts5(text, tokenize = 'unicode61 remove_diacritics 2');
         INSERT INTO notes(text) VALUES ('send the revised budget sheet to Ravi by Friday');
         INSERT INTO notes(text) VALUES ('kal subah standup ke baad call karna');",
    )
    .unwrap();
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM notes WHERE notes MATCH 'budget AND ravi'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
    let hinglish: i64 = conn
        .query_row("SELECT count(*) FROM notes WHERE notes MATCH 'standup'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(hinglish, 1);
}

#[test]
fn wrong_key_is_rejected_and_right_key_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    {
        let conn = open_encrypted(&path, &key(42)).unwrap();
        conn.execute_batch("CREATE TABLE t(x TEXT); INSERT INTO t VALUES ('hello');")
            .unwrap();
    }
    match open_encrypted(&path, &key(43)) {
        Err(StoreError::WrongKeyOrNotDatabase) => {}
        other => panic!("expected wrong-key error, got {:?}", other.map(|_| ())),
    }
    let conn = open_encrypted(&path, &key(42)).unwrap();
    let x: String = conn.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
    assert_eq!(x, "hello");
}

#[test]
fn file_on_disk_has_no_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    {
        let conn = open_encrypted(&path, &key(9)).unwrap();
        conn.execute_batch("CREATE TABLE t(x TEXT);").unwrap();
        conn.execute("INSERT INTO t VALUES (?1)", [CANARY]).unwrap();
        // Force WAL contents into the main file so both are checked in their final state.
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
    }
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let p = entry.unwrap().path();
        let bytes = std::fs::read(&p).unwrap();
        assert!(
            !bytes.starts_with(b"SQLite format 3"),
            "{} has a plaintext SQLite header",
            p.display()
        );
        let utf16: Vec<u8> = CANARY.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        for needle in [CANARY.as_bytes(), utf16.as_slice()] {
            assert!(
                !bytes.windows(needle.len()).any(|w| w == needle),
                "canary found in plaintext in {}",
                p.display()
            );
        }
    }
}
