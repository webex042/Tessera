//! Privacy settings, single-item deletion, retention pruning, and full memory wipe.
//!
//! All operations synchronize SQLCipher and the encrypted `VectorDb` while preserving user
//! configuration (`settings`, `blocklist`) and DPAPI key material (`key.bin`, `groq_key.bin`).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::StoreError;
use crate::vectordb::VectorDb;

pub const SETTINGS_KEY: &str = "privacy_settings_v1";
pub const MAX_RETENTION_DAYS: u32 = 3650;
pub const DEFAULT_GROQ_MODEL: &str = "openai/gpt-oss-20b";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    /// Days to retain captured items (`0` means keep indefinitely until deleted manually).
    pub retention_days: u32,
    /// Active LLM runtime provider: `"local"` or `"groq"`.
    pub ai_provider: String,
    /// Model identifier when using Groq BYOK.
    pub groq_model: String,
    /// Explicit user consent to send PII-masked text to Groq Cloud when `"groq"` is active.
    pub cloud_consent: bool,
    /// Last automatic retention prune timestamp (UTC Unix ms), content-free.
    pub last_pruned_at: Option<i64>,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            retention_days: 0,
            ai_provider: "local".to_owned(),
            groq_model: DEFAULT_GROQ_MODEL.to_owned(),
            cloud_consent: false,
            last_pruned_at: None,
        }
    }
}

impl PrivacySettings {
    pub fn validate(&self) -> Result<(), StoreError> {
        if self.retention_days > MAX_RETENTION_DAYS {
            return Err(StoreError::Invalid("retention_days exceeds maximum"));
        }
        if !matches!(self.ai_provider.as_str(), "local" | "groq") {
            return Err(StoreError::Invalid("ai_provider must be local or groq"));
        }
        let m = self.groq_model.trim();
        if m.is_empty()
            || m.len() > 120
            || !m
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'))
        {
            return Err(StoreError::Invalid("invalid groq model identifier"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ClearReport {
    pub items_deleted: usize,
    pub sources_deleted: usize,
    pub facts_deleted: usize,
    pub vectors_deleted: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PruneReport {
    pub expired_items: usize,
    pub orphan_sources: usize,
    pub vectors_removed: usize,
}

pub fn load_privacy_settings(conn: &Connection) -> Result<PrivacySettings, StoreError> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [SETTINGS_KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(json) = raw else {
        return Ok(PrivacySettings::default());
    };
    let mut parsed: PrivacySettings = serde_json::from_str(&json).unwrap_or_default();
    if parsed.validate().is_err() {
        parsed = PrivacySettings::default();
    }
    Ok(parsed)
}

pub fn save_privacy_settings(conn: &Connection, settings: &PrivacySettings) -> Result<(), StoreError> {
    let mut clean = settings.clone();
    clean.groq_model = clean.groq_model.trim().to_owned();
    clean.validate()?;
    let json = serde_json::to_string(&clean).map_err(|e| StoreError::KeyFile(format!("settings json: {e}")))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SETTINGS_KEY, json],
    )?;
    Ok(())
}

/// Delete a single captured memory item and cascade cleanly across facts/FTS/VectorDb.
pub fn delete_single_item(
    conn: &mut Connection,
    vdb: Option<&mut VectorDb>,
    item_id: i64,
    now_ms: i64,
) -> Result<bool, StoreError> {
    if item_id <= 0 {
        return Err(StoreError::Invalid("item_id must be positive"));
    }
    let deleted = crate::items::delete_items(conn, &[item_id])?;
    if deleted == 0 {
        return Ok(false);
    }
    if let Some(v) = vdb {
        let removed = v.delete_by_item(item_id);
        if removed > 0 {
            v.save()?;
        }
    }
    crate::audit::record(conn, now_ms, "item_deleted", &[("items", deleted as i64)])?;
    Ok(true)
}

/// Wipe all captured & derived memory (items, sources, facts, summaries, briefs, people, jobs,
/// chunks, and encrypted VectorDb records) while keeping settings, blocklist rules, and keys.
pub fn clear_all_memories(
    conn: &mut Connection,
    vdb: Option<&mut VectorDb>,
    now_ms: i64,
) -> Result<ClearReport, StoreError> {
    let mut report = ClearReport::default();
    {
        let tx = conn.transaction()?;
        report.items_deleted = tx.query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))? as usize;
        report.sources_deleted = tx.query_row("SELECT count(*) FROM sources", [], |r| r.get::<_, i64>(0))? as usize;
        report.facts_deleted = tx.query_row("SELECT count(*) FROM facts", [], |r| r.get::<_, i64>(0))? as usize;

        tx.execute_batch(
            "DELETE FROM summary_sources;
             DELETE FROM summaries;
             DELETE FROM briefs;
             DELETE FROM fact_evidence;
             DELETE FROM facts;
             DELETE FROM person_merge_suggestions;
             DELETE FROM person_aliases;
             DELETE FROM suppressed_aliases;
             DELETE FROM people;
             DELETE FROM jobs;
             DELETE FROM chunks;
             DELETE FROM items;
             DELETE FROM sources;",
        )?;
        tx.commit()?;
    }

    if let Some(v) = vdb {
        report.vectors_deleted = v.len();
        if !v.is_empty() {
            v.clear();
            v.save()?;
        }
    }

    crate::audit::record(
        conn,
        now_ms,
        "memories_cleared",
        &[
            ("items", report.items_deleted as i64),
            ("sources", report.sources_deleted as i64),
            ("facts", report.facts_deleted as i64),
            ("vectors", report.vectors_deleted as i64),
        ],
    )?;
    // Scrub freed SQLCipher pages where possible.
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    Ok(report)
}

/// Prune items whose `ended_at < cutoff_ms`, cascading orphan facts/sources and syncing VectorDb.
pub fn prune_expired(
    conn: &mut Connection,
    vdb: Option<&mut VectorDb>,
    cutoff_ms: i64,
    now_ms: i64,
) -> Result<PruneReport, StoreError> {
    let ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT id FROM items WHERE ended_at < ?1 ORDER BY id")?;
        let rows = st.query_map([cutoff_ms], |r| r.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    if ids.is_empty() {
        return Ok(PruneReport::default());
    }

    let sources_before: i64 = conn.query_row("SELECT count(*) FROM sources", [], |r| r.get(0))?;
    let expired_items = crate::items::delete_items(conn, &ids)? as usize;
    let sources_after: i64 = conn.query_row("SELECT count(*) FROM sources", [], |r| r.get(0))?;
    let orphan_sources = sources_before.saturating_sub(sources_after) as usize;

    let mut vectors_removed = 0usize;
    if let Some(v) = vdb {
        vectors_removed = v.delete_by_items(&ids);
        if vectors_removed > 0 {
            v.save()?;
        }
    }

    let mut settings = load_privacy_settings(conn)?;
    settings.last_pruned_at = Some(now_ms);
    let _ = save_privacy_settings(conn, &settings);

    crate::audit::record(
        conn,
        now_ms,
        "retention_pruned",
        &[
            ("items", expired_items as i64),
            ("sources", orphan_sources as i64),
            ("vectors", vectors_removed as i64),
        ],
    )?;

    Ok(PruneReport {
        expired_items,
        orphan_sources,
        vectors_removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocklist::{add_user_rule, load_rules, sync_builtin_rules};
    use crate::facts::{NewEvidence, NewFact, insert_fact};
    use crate::items::{
        Acquisition, NewItem, NewSource, SourceKind, fts_query, insert_item, search_any, upsert_source,
    };
    use crate::migrate::migrate;
    use crate::vectordb::{VectorMetadata, VectorRecord};
    use crate::{KEY_LEN, open_encrypted};
    use tsr_core::Sensitive;
    use tsr_guard::blocklist::{RuleKind, Scope, builtin_rules};

    fn setup() -> (tempfile::TempDir, Connection, VectorDb) {
        let dir = tempfile::tempdir().unwrap();
        let key = [17u8; KEY_LEN];
        let mut conn = open_encrypted(&dir.path().join("t.db"), &key).unwrap();
        migrate(&mut conn).unwrap();
        let vdb = VectorDb::new(dir.path().join("vec.enc"), &key);
        (dir, conn, vdb)
    }

    fn seed_item(conn: &Connection, vdb: &mut VectorDb, url: &str, at: i64, text: &str) -> i64 {
        let src = upsert_source(
            conn,
            &NewSource {
                kind: SourceKind::Url,
                app_exe: "msedge.exe",
                app_name: Some("Edge"),
                window_title: Some("Work"),
                url: Some(url),
                file_path: None,
                seen_at: at,
            },
        )
        .unwrap();
        let id = insert_item(
            conn,
            &NewItem {
                source_id: src,
                started_at: at,
                ended_at: at + 1_000,
                acquisition: Acquisition::Uia,
                text: &Sensitive::new(text.into()),
                simhash: at as u64,
                lang: Some("en"),
                redaction_count: 0,
            },
        )
        .unwrap();
        vdb.insert(VectorRecord {
            id: format!("chunk:{id}:0"),
            model: "test-embed".into(),
            dims: 2,
            scale: 1.0,
            vec_int8: vec![64, 64],
            vec_f32: vec![0.707, 0.707],
            text: text.into(),
            metadata: VectorMetadata {
                item_id: id,
                source_id: Some(src),
                app_name: Some("Edge".into()),
                window_title: Some("Work".into()),
                url: Some(url.into()),
                memory_type: "passage".into(),
                timestamp_ms: at,
                ord: 0,
                start_ch: 0,
                end_ch: text.len() as i64,
                provenance: None,
            },
        })
        .unwrap();
        vdb.save().unwrap();
        id
    }

    #[test]
    fn settings_persistence_and_validation() {
        let (_d, conn, _v) = setup();
        assert_eq!(load_privacy_settings(&conn).unwrap(), PrivacySettings::default());

        let custom = PrivacySettings {
            retention_days: 30,
            ai_provider: "groq".into(),
            groq_model: "openai/gpt-oss-20b".into(),
            cloud_consent: true,
            last_pruned_at: Some(123_456),
        };
        save_privacy_settings(&conn, &custom).unwrap();
        assert_eq!(load_privacy_settings(&conn).unwrap(), custom);

        let mut bad = custom.clone();
        bad.retention_days = 9999;
        assert!(save_privacy_settings(&conn, &bad).is_err());

        bad.retention_days = 7;
        bad.ai_provider = "invalid".into();
        assert!(save_privacy_settings(&conn, &bad).is_err());
    }

    #[test]
    fn single_item_delete_cascades_facts_fts_sources_and_vectors() {
        let (_d, mut conn, mut vdb) = setup();
        let id1 = seed_item(&conn, &mut vdb, "https://example.com/a", 1_000, "Alpha canary note");
        let id2 = seed_item(&conn, &mut vdb, "https://example.com/b", 2_000, "Beta surviving note");

        insert_fact(
            &conn,
            &NewFact {
                kind: "task",
                text: "Finish alpha report",
                text_norm: "finish alpha report",
                owner_id: None,
                owner_text: None,
                counterparty_id: None,
                counterparty_text: None,
                due_text: None,
                due: None,
                status: "open",
                confidence: "high",
                prompt_ver: "v1",
                model: "test",
                schema_ver: 1,
                redactions: 0,
                redact_ver: "v3",
                seen_at: 1_500,
            },
            &[NewEvidence {
                item_id: id1,
                quote: "Alpha canary",
                start_ch: 0,
                end_ch: 12,
                matched: "exact",
            }],
        )
        .unwrap();

        assert!(delete_single_item(&mut conn, Some(&mut vdb), id1, 3_000).unwrap());
        assert!(!delete_single_item(&mut conn, Some(&mut vdb), id1, 3_001).unwrap());
        assert!(search_any(&conn, &fts_query("canary").unwrap(), 10).unwrap().is_empty());
        assert_eq!(search_any(&conn, &fts_query("Beta").unwrap(), 10).unwrap().len(), 1);
        assert_eq!(vdb.len(), 1);
        assert!(vdb.get(&format!("chunk:{id1}:0")).is_none());
        assert!(vdb.get(&format!("chunk:{id2}:0")).is_some());
        let facts_left: i64 = conn.query_row("SELECT count(*) FROM facts", [], |r| r.get(0)).unwrap();
        assert_eq!(facts_left, 0);
    }

    #[test]
    fn clear_all_wipes_memories_and_preserves_settings_and_blocklist() {
        let (dir, mut conn, mut vdb) = setup();
        sync_builtin_rules(&mut conn, &builtin_rules()).unwrap();
        let user_rule = add_user_rule(&mut conn, RuleKind::Process, Scope::All, "SecretBank.exe", false).unwrap();
        let prefs = PrivacySettings {
            retention_days: 90,
            ..Default::default()
        };
        save_privacy_settings(&conn, &prefs).unwrap();

        seed_item(&conn, &mut vdb, "https://example.com/1", 1_000, "First item");
        seed_item(&conn, &mut vdb, "https://example.com/2", 2_000, "Second item");

        let report = clear_all_memories(&mut conn, Some(&mut vdb), 5_000).unwrap();
        assert_eq!(report.items_deleted, 2);
        assert_eq!(report.sources_deleted, 2);
        assert_eq!(report.vectors_deleted, 2);
        assert!(vdb.is_empty());
        let reopened = VectorDb::open_or_create(&dir.path().join("vec.enc"), &[17u8; KEY_LEN]).unwrap();
        assert!(reopened.is_empty());

        let items_left: i64 = conn.query_row("SELECT count(*) FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(items_left, 0);
        assert_eq!(load_privacy_settings(&conn).unwrap(), prefs);
        assert!(load_rules(&conn).unwrap().iter().any(|r| r.key == user_rule));
    }

    #[test]
    fn retention_prune_deletes_only_expired_items() {
        let (_d, mut conn, mut vdb) = setup();
        let old_id = seed_item(&conn, &mut vdb, "https://example.com/old", 10_000, "Expired memory");
        let fresh_id = seed_item(&conn, &mut vdb, "https://example.com/new", 50_000, "Fresh memory");

        let report = prune_expired(&mut conn, Some(&mut vdb), 30_000, 60_000).unwrap();
        assert_eq!(report.expired_items, 1);
        assert_eq!(report.orphan_sources, 1);
        assert_eq!(report.vectors_removed, 1);
        assert!(vdb.get(&format!("chunk:{old_id}:0")).is_none());
        assert!(vdb.get(&format!("chunk:{fresh_id}:0")).is_some());

        let rem: i64 = conn.query_row("SELECT id FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(rem, fresh_id);
        assert_eq!(load_privacy_settings(&conn).unwrap().last_pruned_at, Some(60_000));
    }
}
