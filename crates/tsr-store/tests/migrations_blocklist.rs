//! Stage 1: schema migrations and the scoped blocklist repository (plan §6.1, §9.2).

use tsr_guard::blocklist::{
    BlockReason, Blocklist, Decision, PrivateState, RuleKind, Scope, WindowFacts, builtin_rules,
};
use tsr_store::blocklist::{
    add_user_rule, blocked_counts, get_rule, load_rules, record_block, remove_user_rule, set_enabled,
    sync_builtin_rules,
};
use tsr_store::migrate::{MIGRATIONS, Migration, current_version, migrate, migrate_with};
use tsr_store::{KEY_LEN, StoreError, open_encrypted};

fn db() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_encrypted(&dir.path().join("t.db"), &[3u8; KEY_LEN]).unwrap();
    (dir, conn)
}

#[test]
fn fresh_database_migrates_to_latest_and_is_idempotent() {
    let (_d, mut c) = db();
    assert_eq!(current_version(&c).unwrap(), 0);
    assert_eq!(migrate(&mut c).unwrap(), MIGRATIONS.len() as u32);
    assert_eq!(migrate(&mut c).unwrap(), MIGRATIONS.len() as u32);
    let scope_col: i64 = c
        .query_row(
            "SELECT count(*) FROM pragma_table_info('blocklist') WHERE name = 'scope'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(scope_col, 1);
}

/// "Existing database" path: a DB created by an older migration list is upgraded in place and
/// keeps its rows. No Tessara database has shipped yet, so the older list is simulated by
/// appending a test-only migration to the real ones.
#[test]
fn existing_database_upgrades_and_keeps_rows() {
    let (_d, mut c) = db();
    migrate(&mut c).unwrap();
    c.execute(
        "INSERT INTO settings (key, value) VALUES ('brief_time', '\"05:00\"')",
        [],
    )
    .unwrap();
    let v1 = current_version(&c).unwrap();

    let extra = Migration {
        version: v1 + 1,
        name: "test_add_column",
        sql: "ALTER TABLE settings ADD COLUMN note TEXT;",
    };
    let mut list: Vec<Migration> = MIGRATIONS
        .iter()
        .map(|m| Migration {
            version: m.version,
            name: m.name,
            sql: m.sql,
        })
        .collect();
    list.push(extra);
    assert_eq!(migrate_with(&mut c, &list).unwrap(), v1 + 1);
    let v: String = c
        .query_row("SELECT value FROM settings WHERE key = 'brief_time'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, "\"05:00\"");
}

#[test]
fn newer_schema_is_refused() {
    let (_d, mut c) = db();
    c.pragma_update(None, "user_version", 999).unwrap();
    assert!(matches!(
        migrate(&mut c),
        Err(StoreError::SchemaTooNew { found: 999, .. })
    ));
}

#[test]
fn failed_migration_rolls_back() {
    let (_d, mut c) = db();
    let bad = [Migration {
        version: 1,
        name: "bad",
        sql: "CREATE TABLE a(x); SELECT * FROM missing_table;",
    }];
    assert!(migrate_with(&mut c, &bad).is_err());
    assert_eq!(current_version(&c).unwrap(), 0);
    let tables: i64 = c
        .query_row("SELECT count(*) FROM sqlite_master WHERE name = 'a'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tables, 0);
}

#[test]
fn builtin_sync_keeps_user_choice_and_updates_patterns() {
    let (_d, mut c) = db();
    migrate(&mut c).unwrap();
    let mut b = builtin_rules();
    sync_builtin_rules(&mut c, &b).unwrap();
    assert_eq!(load_rules(&c).unwrap().len(), b.len());

    assert!(set_enabled(&c, "builtin.title.sign_in", false).unwrap());
    // A later release changes a pattern and drops a rule.
    for s in b.iter_mut().filter(|s| s.key == "builtin.title.sign_in") {
        s.pattern = r"\bsign[ -]?(in|on)\b".into();
    }
    b.retain(|s| s.key != "builtin.url.about");
    sync_builtin_rules(&mut c, &b).unwrap();

    let r = get_rule(&c, "builtin.title.sign_in").unwrap().unwrap();
    assert!(!r.enabled, "user's disable survives a sync");
    assert!(r.pattern.contains("on"));
    assert_eq!(r.scope, Scope::Browser);
    assert!(get_rule(&c, "builtin.url.about").unwrap().is_none());
}

#[test]
fn user_rules_get_content_free_keys_and_are_validated() {
    let (_d, mut c) = db();
    migrate(&mut c).unwrap();
    let k = add_user_rule(&mut c, RuleKind::Process, Scope::All, "SecretBankClient.exe", false).unwrap();
    assert!(k.starts_with("user.") && !k.contains("Secret"), "{k}");
    assert!(matches!(
        add_user_rule(&mut c, RuleKind::Title, Scope::All, "(unclosed", true),
        Err(StoreError::InvalidRule)
    ));
    assert!(matches!(
        add_user_rule(&mut c, RuleKind::Title, Scope::All, "   ", false),
        Err(StoreError::InvalidRule)
    ));
    assert!(
        !remove_user_rule(&c, "builtin.title.otp").unwrap(),
        "built-ins cannot be deleted"
    );
    assert!(remove_user_rule(&c, &k).unwrap());
}

#[test]
fn blocks_are_counted_per_rule_without_content() {
    let (_d, mut c) = db();
    migrate(&mut c).unwrap();
    sync_builtin_rules(&mut c, &builtin_rules()).unwrap();
    let (bl, _) = Blocklist::compile(&load_rules(&c).unwrap());

    let title = "Enter OTP for SECRET-CANARY-TITLE";
    let w = WindowFacts {
        exe: "app.exe",
        exe_path: None,
        title,
        url: None,
        private: PrivateState::NotPrivate,
    };
    let d = bl.check(&w);
    assert!(matches!(
        d,
        Decision::Block {
            reason: BlockReason::Title,
            ..
        }
    ));
    for t in [1_000, 2_000, 3_000] {
        record_block(&c, &d, t).unwrap();
    }
    let b = WindowFacts {
        exe: "chrome.exe",
        private: PrivateState::Unknown,
        ..w
    };
    record_block(&c, &bl.check(&b), 2_500).unwrap();
    record_block(&c, &Decision::Allow, 2_600).unwrap();

    let counts = blocked_counts(&c, 1_500).unwrap();
    // Most blocks first; the t=1000 OTP block is before `since` and not counted.
    assert_eq!(
        counts,
        vec![
            ("builtin.title.otp".to_string(), 2),
            ("builtin.private.undetermined_or_private".to_string(), 1)
        ]
    );
    let dump: String = c
        .query_row("SELECT group_concat(detail, '|') FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert!(!dump.contains("CANARY") && !dump.contains("OTP for"), "{dump}");
}

#[test]
fn record_block_refuses_unexpected_keys() {
    let (_d, mut c) = db();
    migrate(&mut c).unwrap();
    let d = Decision::Block {
        reason: BlockReason::Title,
        rule_key: Some("My Bank \"title\"".into()),
    };
    assert!(matches!(record_block(&c, &d, 1), Err(StoreError::InvalidRule)));
}
