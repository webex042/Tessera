//! Embedded local encrypted vector database for Tessera memory and semantic retrieval.
//!
//! Stores redacted text chunks, vector embeddings, and rich metadata encrypted at rest
//! using DPAPI-backed keys. Fully offline, zero external daemons, zero cloud services.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{KEY_LEN, StoreError};

const MAGIC: &[u8; 4] = b"TSRV";
const VERSION: u8 = 1;
const NONCE_LEN: usize = 16;
const TAG_LEN: usize = 32;

/// Structured metadata associated with each vector chunk.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VectorMetadata {
    pub item_id: i64,
    pub source_id: Option<i64>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub memory_type: String, // "passage", "fact", "summary"
    pub timestamp_ms: i64,
    pub ord: u32,
    pub start_ch: i64,
    pub end_ch: i64,
    pub provenance: Option<String>,
}

/// A stored vector record containing embedding, chunk text, and metadata.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VectorRecord {
    pub id: String,
    pub model: String,
    pub dims: usize,
    pub scale: f32,
    pub vec_int8: Vec<i8>,
    pub vec_f32: Vec<f32>,
    /// Redacted chunk text (guaranteed PII-masked).
    pub text: String,
    pub metadata: VectorMetadata,
}

/// Query filter for metadata pre-filtering during vector similarity search.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VectorFilter {
    pub model: Option<String>,
    pub item_ids: Option<Vec<i64>>,
    pub source_ids: Option<Vec<i64>>,
    pub app_names: Option<Vec<String>>,
    pub memory_types: Option<Vec<String>>,
    pub time_range: Option<(i64, i64)>, // min_ms ..= max_ms
}

impl VectorFilter {
    pub fn matches(&self, model: &str, meta: &VectorMetadata) -> bool {
        if let Some(m) = &self.model
            && m != model
        {
            return false;
        }
        if let Some(ids) = &self.item_ids
            && !ids.contains(&meta.item_id)
        {
            return false;
        }
        if let Some(sids) = &self.source_ids
            && meta.source_id.map_or(false, |s| !sids.contains(&s))
        {
            return false;
        }
        if let Some(apps) = &self.app_names
            && meta.app_name.as_ref().map_or(false, |a| !apps.contains(a))
        {
            return false;
        }
        if let Some(types) = &self.memory_types
            && !types.contains(&meta.memory_type)
        {
            return false;
        }
        if let Some((min_t, max_t)) = self.time_range
            && (meta.timestamp_ms < min_t || meta.timestamp_ms > max_t)
        {
            return false;
        }
        true
    }
}

/// A search hit returned from vector similarity search.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VectorHit {
    pub id: String,
    pub item_id: i64,
    pub score: f32,
    pub text: String,
    pub metadata: VectorMetadata,
}

/// An embedded, local, encrypted vector database.
pub struct VectorDb {
    path: PathBuf,
    key: Zeroizing<[u8; KEY_LEN]>,
    records: Vec<VectorRecord>,
    by_id: HashMap<String, usize>,
    by_item: HashMap<i64, Vec<usize>>,
}

impl VectorDb {
    /// Create a new in-memory VectorDb bound to `path` and encrypted with `key`.
    pub fn new(path: PathBuf, key: &[u8; KEY_LEN]) -> Self {
        let mut k = Zeroizing::new([0u8; KEY_LEN]);
        k.copy_from_slice(key);
        VectorDb {
            path,
            key: k,
            records: Vec::new(),
            by_id: HashMap::new(),
            by_item: HashMap::new(),
        }
    }

    /// Open an existing encrypted vector database or create a new one if it does not exist.
    pub fn open_or_create(path: &Path, key: &[u8; KEY_LEN]) -> Result<Self, StoreError> {
        let mut db = Self::new(path.to_path_buf(), key);
        if path.exists() {
            db.load()?;
        }
        Ok(db)
    }

    /// Number of live records in the database.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Retrieve a record by its stable identifier.
    pub fn get(&self, id: &str) -> Option<&VectorRecord> {
        self.by_id.get(id).map(|&idx| &self.records[idx])
    }

    /// Retrieve all records in the database.
    pub fn all_records(&self) -> &[VectorRecord] {
        &self.records
    }

    /// Insert or update a single vector record.
    pub fn insert(&mut self, record: VectorRecord) -> Result<(), StoreError> {
        if record.id.trim().is_empty() {
            return Err(StoreError::Invalid("record id cannot be empty"));
        }
        if record.dims == 0 || (record.vec_f32.is_empty() && record.vec_int8.is_empty()) {
            return Err(StoreError::Invalid("vector data cannot be empty"));
        }

        if let Some(&existing_idx) = self.by_id.get(&record.id) {
            // Update existing
            let old_item = self.records[existing_idx].metadata.item_id;
            if old_item != record.metadata.item_id {
                if let Some(list) = self.by_item.get_mut(&old_item) {
                    list.retain(|&i| i != existing_idx);
                }
                self.by_item.entry(record.metadata.item_id).or_default().push(existing_idx);
            }
            self.records[existing_idx] = record;
        } else {
            // New insert
            let idx = self.records.len();
            self.by_id.insert(record.id.clone(), idx);
            self.by_item.entry(record.metadata.item_id).or_default().push(idx);
            self.records.push(record);
        }
        Ok(())
    }

    /// Batch insert records.
    pub fn insert_batch(&mut self, records: Vec<VectorRecord>) -> Result<usize, StoreError> {
        let count = records.len();
        for r in records {
            self.insert(r)?;
        }
        Ok(count)
    }

    /// Delete a record by ID. Returns true if removed.
    pub fn delete(&mut self, id: &str) -> bool {
        let Some(&idx) = self.by_id.get(id) else {
            return false;
        };
        self.records.remove(idx);
        self.rebuild_indices();
        true
    }

    /// Delete all records for an `item_id`. Returns number of deleted records.
    pub fn delete_by_item(&mut self, item_id: i64) -> usize {
        let initial_len = self.records.len();
        self.records.retain(|r| r.metadata.item_id != item_id);
        let removed = initial_len - self.records.len();
        if removed > 0 {
            self.rebuild_indices();
        }
        removed
    }

    /// Delete all records matching a specific embedding model.
    pub fn delete_by_model(&mut self, model: &str) -> usize {
        let initial_len = self.records.len();
        self.records.retain(|r| r.model != model);
        let removed = initial_len - self.records.len();
        if removed > 0 {
            self.rebuild_indices();
        }
        removed
    }

    /// Rebuild fast hash index mappings after removal.
    fn rebuild_indices(&mut self) {
        self.by_id.clear();
        self.by_item.clear();
        for (idx, r) in self.records.iter().enumerate() {
            self.by_id.insert(r.id.clone(), idx);
            self.by_item.entry(r.metadata.item_id).or_default().push(idx);
        }
    }

    /// Cosine similarity search over stored vectors with optional metadata pre-filtering.
    pub fn search(&self, query_vec: &[f32], k: usize, filter: Option<&VectorFilter>) -> Vec<VectorHit> {
        if query_vec.is_empty() || k == 0 || self.records.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<VectorHit> = Vec::new();

        for r in &self.records {
            if let Some(f) = filter
                && !f.matches(&r.model, &r.metadata)
            {
                continue;
            }

            let sim = if !r.vec_f32.is_empty() && r.vec_f32.len() == query_vec.len() {
                // High-precision f32 dot product (query & stored vector are L2-normalized)
                r.vec_f32.iter().zip(query_vec.iter()).map(|(a, b)| a * b).sum::<f32>()
            } else if !r.vec_int8.is_empty() && r.vec_int8.len() == query_vec.len() {
                // Quantized int8 dot product with scale
                let dot: f32 = r
                    .vec_int8
                    .iter()
                    .zip(query_vec.iter())
                    .map(|(&a, &b)| a as f32 * b)
                    .sum::<f32>();
                dot * r.scale
            } else {
                continue;
            };

            scored.push(VectorHit {
                id: r.id.clone(),
                item_id: r.metadata.item_id,
                score: sim,
                text: r.text.clone(),
                metadata: r.metadata.clone(),
            });
        }

        // Rank by descending score, then deterministic ID order
        scored.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        scored.truncate(k);
        scored
    }

    /// Persist the database to disk encrypted with the DPAPI-derived key.
    pub fn save(&self) -> Result<(), StoreError> {
        let plaintext = serde_json::to_vec(&self.records)
            .map_err(|e| StoreError::KeyFile(format!("json serialization: {e}")))?;

        // Generate 16-byte nonce
        let nonce = generate_nonce();

        // Encrypt with SHA-256 CTR keystream
        let ciphertext = crypt_ctr(&plaintext, &self.key, &nonce);

        // Compute HMAC/integrity authentication tag: SHA256(key || b"TSRV_AUTH" || nonce || ciphertext)
        let tag = compute_tag(&self.key, &nonce, &ciphertext);

        let mut output = Vec::with_capacity(4 + 1 + NONCE_LEN + TAG_LEN + ciphertext.len());
        output.extend_from_slice(MAGIC);
        output.push(VERSION);
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&tag);
        output.extend_from_slice(&ciphertext);

        // Atomic write via temp file
        let tmp = self.path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| StoreError::KeyFile(format!("create tmp: {:?}", e.kind())))?;
            f.write_all(&output)
                .and_then(|_| f.sync_all())
                .map_err(|e| StoreError::KeyFile(format!("write/sync: {:?}", e.kind())))?;
        }
        fs::rename(&tmp, &self.path)
            .map_err(|e| StoreError::KeyFile(format!("rename tmp: {:?}", e.kind())))?;

        Ok(())
    }

    /// Load the database from disk, decrypting and verifying authentication tag.
    pub fn load(&mut self) -> Result<(), StoreError> {
        let raw = fs::read(&self.path)
            .map_err(|e| StoreError::KeyFile(format!("read {}: {:?}", self.path.display(), e.kind())))?;

        if raw.len() < 4 + 1 + NONCE_LEN + TAG_LEN || &raw[..4] != MAGIC {
            return Err(StoreError::WrongKeyOrNotDatabase);
        }
        if raw[4] != VERSION {
            return Err(StoreError::SchemaTooNew {
                found: raw[4] as u32,
                supported: VERSION as u32,
            });
        }

        let nonce = &raw[5..5 + NONCE_LEN];
        let tag = &raw[5 + NONCE_LEN..5 + NONCE_LEN + TAG_LEN];
        let ciphertext = &raw[5 + NONCE_LEN + TAG_LEN..];

        // Verify authentication tag
        let expected_tag = compute_tag(&self.key, nonce, ciphertext);
        if tag != expected_tag {
            return Err(StoreError::WrongKeyOrNotDatabase);
        }

        // Decrypt
        let plaintext = crypt_ctr(ciphertext, &self.key, nonce);

        // Deserialize
        let records: Vec<VectorRecord> = serde_json::from_slice(&plaintext)
            .map_err(|_| StoreError::WrongKeyOrNotDatabase)?;

        self.records = records;
        self.rebuild_indices();
        Ok(())
    }
}

/// Generate a 16-byte nonce using timestamp and hasher entropy.
fn generate_nonce() -> [u8; NONCE_LEN] {
    let mut h = Sha256::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    h.update(now.to_le_bytes());
    h.update(b"tessera-nonce-salt");
    let result = h.finalize();
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&result[..NONCE_LEN]);
    nonce
}

/// Authenticated tag: SHA256(key || b"TSRV_AUTH" || nonce || ciphertext)
fn compute_tag(key: &[u8; KEY_LEN], nonce: &[u8], ciphertext: &[u8]) -> [u8; TAG_LEN] {
    let mut h = Sha256::new();
    h.update(key);
    h.update(b"TSRV_AUTH");
    h.update(nonce);
    h.update(ciphertext);
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(&h.finalize());
    tag
}

/// AES-CTR style stream cipher using SHA-256 block keystream:
/// Block `i`: SHA256(key || nonce || (i as u64).to_le_bytes())
fn crypt_ctr(input: &[u8], key: &[u8; KEY_LEN], nonce: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut block_idx: u64 = 0;

    let mut in_pos = 0;
    while in_pos < input.len() {
        let mut h = Sha256::new();
        h.update(key);
        h.update(nonce);
        h.update(block_idx.to_le_bytes());
        let ks_block = h.finalize();

        let chunk_len = std::cmp::min(ks_block.len(), input.len() - in_pos);
        for i in 0..chunk_len {
            output.push(input[in_pos + i] ^ ks_block[i]);
        }
        in_pos += chunk_len;
        block_idx += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_record(id: &str, item_id: i64, vec: Vec<f32>, text: &str) -> VectorRecord {
        let int8_vec: Vec<i8> = vec.iter().map(|&x| (x * 127.0) as i8).collect();
        VectorRecord {
            id: id.to_string(),
            model: "test-model".to_string(),
            dims: vec.len(),
            scale: 1.0 / 127.0,
            vec_int8: int8_vec,
            vec_f32: vec,
            text: text.to_string(),
            metadata: VectorMetadata {
                item_id,
                source_id: Some(1),
                app_name: Some("code.exe".to_string()),
                window_title: Some("editor".to_string()),
                url: None,
                memory_type: "passage".to_string(),
                timestamp_ms: 1000 + item_id,
                ord: 0,
                start_ch: 0,
                end_ch: text.len() as i64,
                provenance: Some(format!("item:{}", item_id)),
            },
        }
    }

    #[test]
    fn insert_search_and_metadata_filter() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_vec.enc");
        let key = [42u8; KEY_LEN];
        let mut db = VectorDb::new(path, &key);

        let r1 = test_record("r1", 1, vec![1.0, 0.0, 0.0], "First record");
        let mut r2 = test_record("r2", 2, vec![0.0, 1.0, 0.0], "Second record");
        r2.metadata.app_name = Some("browser.exe".to_string());
        r2.metadata.timestamp_ms = 5000;

        let r3 = test_record("r3", 3, vec![0.707, 0.707, 0.0], "Third record");

        db.insert(r1).unwrap();
        db.insert(r2).unwrap();
        db.insert(r3).unwrap();
        assert_eq!(db.len(), 3);

        // Search near [1.0, 0.0, 0.0]
        let hits = db.search(&[1.0, 0.0, 0.0], 2, None);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "r1");
        assert_eq!(hits[1].id, "r3");

        // Search with metadata filter (only browser.exe)
        let filter = VectorFilter {
            app_names: Some(vec!["browser.exe".to_string()]),
            ..Default::default()
        };
        let filtered_hits = db.search(&[0.0, 1.0, 0.0], 5, Some(&filter));
        assert_eq!(filtered_hits.len(), 1);
        assert_eq!(filtered_hits[0].id, "r2");

        // Search with time range filter
        let time_filter = VectorFilter {
            time_range: Some((4000, 6000)),
            ..Default::default()
        };
        let time_hits = db.search(&[0.0, 1.0, 0.0], 5, Some(&time_filter));
        assert_eq!(time_hits.len(), 1);
        assert_eq!(time_hits[0].id, "r2");
    }

    #[test]
    fn persistence_and_encryption_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("encrypted_vec.enc");
        let key = [7u8; KEY_LEN];

        {
            let mut db = VectorDb::new(path.clone(), &key);
            db.insert(test_record("v1", 10, vec![0.6, 0.8], "Masked [PERSON_01]")).unwrap();
            db.save().unwrap();
        }

        // Verify the file on disk is encrypted (starts with TSRV magic and doesn't contain plaintext)
        let raw = fs::read(&path).unwrap();
        assert_eq!(&raw[..4], MAGIC);
        let raw_str = String::from_utf8_lossy(&raw);
        assert!(!raw_str.contains("Masked"));
        assert!(!raw_str.contains("[PERSON_01]"));

        // Successfully decrypt with right key
        {
            let mut db = VectorDb::new(path.clone(), &key);
            db.load().unwrap();
            assert_eq!(db.len(), 1);
            let r = db.get("v1").unwrap();
            assert_eq!(r.text, "Masked [PERSON_01]");
            assert_eq!(r.metadata.item_id, 10);
        }

        // Fails with wrong key
        {
            let wrong_key = [99u8; KEY_LEN];
            let mut db = VectorDb::new(path, &wrong_key);
            let err = db.load().unwrap_err();
            assert!(matches!(err, StoreError::WrongKeyOrNotDatabase));
        }
    }

    #[test]
    fn delete_operations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("del_vec.enc");
        let key = [1u8; KEY_LEN];
        let mut db = VectorDb::new(path, &key);

        db.insert(test_record("a1", 100, vec![1.0, 0.0], "a1")).unwrap();
        db.insert(test_record("a2", 100, vec![0.0, 1.0], "a2")).unwrap();
        db.insert(test_record("b1", 200, vec![1.0, 1.0], "b1")).unwrap();
        assert_eq!(db.len(), 3);

        // Delete single
        assert!(db.delete("b1"));
        assert_eq!(db.len(), 2);
        assert!(db.get("b1").is_none());

        // Delete by item
        let removed = db.delete_by_item(100);
        assert_eq!(removed, 2);
        assert_eq!(db.len(), 0);
    }
}

