//! REAL DPAPI: `open_store` on this Windows user (plan §9.1, Stage 1 task 7).
#![cfg(windows)]

use tsr_store::keystore::{KEY_FILE, open_store};
use tsr_store::{KEY_LEN, StoreError, open_encrypted};

#[test]
fn first_open_creates_key_and_schema_and_reopen_reads_data() {
    let d = tempfile::tempdir().unwrap();
    {
        let c = open_store(d.path()).unwrap();
        c.execute("INSERT INTO settings (key, value) VALUES ('k', '\"v\"')", [])
            .unwrap();
    }
    let raw = std::fs::read(d.path().join(KEY_FILE)).unwrap();
    assert_eq!(&raw[..5], b"TSRK\x01");
    let c = open_store(d.path()).unwrap();
    let v: String = c
        .query_row("SELECT value FROM settings WHERE key = 'k'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, "\"v\"");
}

#[test]
fn missing_key_with_existing_db_is_an_error_not_a_new_key() {
    let d = tempfile::tempdir().unwrap();
    drop(open_store(d.path()).unwrap());
    std::fs::remove_file(d.path().join(KEY_FILE)).unwrap();
    assert!(matches!(open_store(d.path()), Err(StoreError::KeyMissing)));
    assert!(!d.path().join(KEY_FILE).exists(), "no replacement key may be created");
}

#[test]
fn corrupt_or_foreign_key_files_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    drop(open_store(d.path()).unwrap());
    let kp = d.path().join(KEY_FILE);
    let mut raw = std::fs::read(&kp).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 0xff;
    std::fs::write(&kp, &raw).unwrap();
    assert!(matches!(open_store(d.path()), Err(StoreError::Dpapi(_))));
    std::fs::write(&kp, b"PK\x03\x04 not a key").unwrap();
    assert!(matches!(open_store(d.path()), Err(StoreError::KeyFile(_))));
}

#[test]
fn database_is_not_readable_with_another_key() {
    let d = tempfile::tempdir().unwrap();
    drop(open_store(d.path()).unwrap());
    assert!(matches!(
        open_encrypted(&d.path().join("tessara.db"), &[0u8; KEY_LEN]),
        Err(StoreError::WrongKeyOrNotDatabase)
    ));
}
