//! Migration utility: safely migrate SQLCipher storage to the encrypted VectorDb.
//!
//! Preserves data integrity, creates backups, applies comprehensive PII masking with
//! typed placeholders, and migrates embeddings and metadata into the local vector database.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tsr_guard::redact::mask_pii;

use crate::StoreError;
use crate::vectordb::{VectorDb, VectorMetadata, VectorRecord};

/// Detailed migration report documenting processed records and masking statistics.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigrationReport {
    pub items_scanned: usize,
    pub chunks_migrated: usize,
    pub pii_redactions_applied: usize,
    pub backup_path: Option<String>,
}

/// Backup the existing database file before performing migration.
pub fn backup_database(db_path: &Path) -> Result<PathBuf, StoreError> {
    if !db_path.exists() {
        return Err(StoreError::KeyMissing);
    }
    let backup_path = db_path.with_extension("db.bak");
    fs::copy(db_path, &backup_path)
        .map_err(|e| StoreError::KeyFile(format!("database backup failed: {e:?}")))?;
    tracing::info!(backup = %backup_path.display(), "database backed up prior to vector migration");
    Ok(backup_path)
}

/// Migrate all items and chunks from the SQLCipher connection into `VectorDb`.
///
/// Idempotent: re-running overwrites/refreshes existing item chunks in the VectorDb.
/// Non-destructive: the original database is preserved intact.
pub fn migrate_sqlcipher_to_vectordb(
    conn: &Connection,
    vector_db: &mut VectorDb,
) -> Result<MigrationReport, StoreError> {
    let mut report = MigrationReport::default();

    // Query items with source metadata
    let mut item_stmt = conn.prepare(
        "SELECT items.id, items.source_id, items.started_at, items.text,
                COALESCE(sources.app_name, sources.app_exe), sources.window_title, sources.url
         FROM items
         JOIN sources ON sources.id = items.source_id
         ORDER BY items.id",
    )?;

    struct ItemData {
        id: i64,
        source_id: i64,
        started_at: i64,
        text: String,
        app_name: Option<String>,
        window_title: Option<String>,
        url: Option<String>,
    }

    let items = item_stmt
        .query_map([], |row| {
            Ok(ItemData {
                id: row.get(0)?,
                source_id: row.get(1)?,
                started_at: row.get(2)?,
                text: row.get(3)?,
                app_name: row.get(4)?,
                window_title: row.get(5)?,
                url: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut chunk_stmt = conn.prepare(
        "SELECT ord, start_ch, end_ch, model, dims, scale, vec
         FROM chunks
         WHERE item_id = ?1
         ORDER BY ord",
    )?;

    for item in items {
        report.items_scanned += 1;

        // Apply robust PII masking to text and window title before transferring
        let masked_text_res = mask_pii(&item.text);
        report.pii_redactions_applied += masked_text_res.total() as usize;
        let masked_text = masked_text_res.text;

        let masked_title = item.window_title.as_ref().map(|t| {
            let res = mask_pii(t);
            report.pii_redactions_applied += res.total() as usize;
            res.text
        });

        // Read chunks for this item
        struct ChunkData {
            ord: u32,
            start_ch: i64,
            end_ch: i64,
            model: String,
            dims: usize,
            scale: f32,
            vec_int8: Vec<i8>,
        }

        let chunks = chunk_stmt
            .query_map([item.id], |r| {
                let bytes: Vec<u8> = r.get(6)?;
                Ok(ChunkData {
                    ord: r.get(0)?,
                    start_ch: r.get(1)?,
                    end_ch: r.get(2)?,
                    model: r.get(3)?,
                    dims: r.get::<_, i64>(4)? as usize,
                    scale: r.get::<_, f64>(5)? as f32,
                    vec_int8: bytes.into_iter().map(|b| b as i8).collect(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for c in chunks {
            // Compute reconstructed f32 vectors from scaled int8
            let vec_f32: Vec<f32> = c.vec_int8.iter().map(|&x| (x as f32) * c.scale).collect();

            // Extract the chunk span from the PII-masked text safely
            let chunk_text: String = if c.start_ch >= 0 && c.end_ch >= c.start_ch {
                let start = c.start_ch as usize;
                let len = (c.end_ch - c.start_ch) as usize;
                masked_text.chars().skip(start).take(len).collect()
            } else {
                masked_text.clone()
            };

            let record_id = format!("chunk-{}-{}", item.id, c.ord);
            let record = VectorRecord {
                id: record_id,
                model: c.model,
                dims: c.dims,
                scale: c.scale,
                vec_int8: c.vec_int8,
                vec_f32,
                text: chunk_text,
                metadata: VectorMetadata {
                    item_id: item.id,
                    source_id: Some(item.source_id),
                    app_name: item.app_name.clone(),
                    window_title: masked_title.clone(),
                    url: item.url.clone(),
                    memory_type: "passage".to_string(),
                    timestamp_ms: item.started_at,
                    ord: c.ord,
                    start_ch: c.start_ch,
                    end_ch: c.end_ch,
                    provenance: Some(format!("item:{}", item.id)),
                },
            };

            vector_db.insert(record)?;
            report.chunks_migrated += 1;
        }
    }

    // Persist migrated records to encrypted disk storage
    vector_db.save()?;
    tracing::info!(
        items = report.items_scanned,
        chunks = report.chunks_migrated,
        pii_masked = report.pii_redactions_applied,
        "migration to vector database completed successfully"
    );

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KEY_LEN;
    use tempfile::tempdir;

    fn setup_test_sqlite() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sources (
                id INTEGER PRIMARY KEY,
                app_exe TEXT NOT NULL,
                app_name TEXT,
                window_title TEXT,
                url TEXT
            );
            CREATE TABLE items (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL REFERENCES sources(id),
                started_at INTEGER NOT NULL,
                text TEXT NOT NULL
            );
            CREATE TABLE chunks (
                id INTEGER PRIMARY KEY,
                item_id INTEGER NOT NULL REFERENCES items(id),
                ord INTEGER NOT NULL,
                start_ch INTEGER NOT NULL,
                end_ch INTEGER NOT NULL,
                model TEXT NOT NULL,
                chunker_ver TEXT NOT NULL,
                dims INTEGER NOT NULL,
                scale REAL NOT NULL,
                vec BLOB NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_migration_masks_pii_and_populates_vectordb() {
        let conn = setup_test_sqlite();
        conn.execute(
            "INSERT INTO sources (id, app_exe, app_name, window_title, url)
             VALUES (1, 'mail.exe', 'Mail', 'Inbox: Meeting with Alice', 'https://mail.example.com')",
            [],
        )
        .unwrap();

        let raw_text = "Contact Alice Smith at alice@example.com or phone +1-555-0199 for key AKIAIOSFODNN7EXAMPLE";
        conn.execute(
            "INSERT INTO items (id, source_id, started_at, text) VALUES (1, 1, 1000, ?1)",
            [raw_text],
        )
        .unwrap();

        let vec_bytes = vec![10u8, 20u8, 30u8];
        conn.execute(
            "INSERT INTO chunks (item_id, ord, start_ch, end_ch, model, chunker_ver, dims, scale, vec)
             VALUES (1, 0, 0, 50, 'test-model', 'v1', 3, 0.5, ?1)",
            [&vec_bytes],
        )
        .unwrap();

        let dir = tempdir().unwrap();
        let vec_path = dir.path().join("migrated.enc");
        let key = [99u8; KEY_LEN];
        let mut vdb = VectorDb::new(vec_path, &key);

        let report = migrate_sqlcipher_to_vectordb(&conn, &mut vdb).unwrap();
        assert_eq!(report.items_scanned, 1);
        assert_eq!(report.chunks_migrated, 1);
        assert!(report.pii_redactions_applied >= 2); // email + API key + phone

        assert_eq!(vdb.len(), 1);
        let rec = vdb.get("chunk-1-0").unwrap();
        // Sensitive email or api key must NOT be in rec.text
        assert!(!rec.text.contains("alice@example.com"));
        assert!(!rec.text.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(rec.text.contains("[EMAIL]") || rec.text.contains("[API_KEY]"));
    }
}
