//! Embedded local encrypted vector database for Tessera memory and semantic retrieval.
//!
//! Stores redacted text chunks, vector embeddings, and rich metadata encrypted at rest
//! using DPAPI-backed keys. Fully offline, zero external daemons, zero cloud services.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{KEY_LEN, StoreError};

const MAGIC: &[u8; 4] = b"TSRV";
const VERSION_V1: u8 = 1;
const VERSION: u8 = 2;
const V1_NONCE_LEN: usize = 16;
const V1_TAG_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

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
            && meta.source_id.is_some_and(|s| !sids.contains(&s))
        {
            return false;
        }
        if let Some(apps) = &self.app_names
            && meta.app_name.as_ref().is_some_and(|a| !apps.contains(a))
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
                self.by_item
                    .entry(record.metadata.item_id)
                    .or_default()
                    .push(existing_idx);
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

    /// Delete all records for a set of `item_id`s. Returns number of deleted records.
    pub fn delete_by_items(&mut self, item_ids: &[i64]) -> usize {
        if item_ids.is_empty() || self.records.is_empty() {
            return 0;
        }
        let ids: HashSet<i64> = item_ids.iter().copied().collect();
        let initial_len = self.records.len();
        self.records.retain(|r| !ids.contains(&r.metadata.item_id));
        let removed = initial_len - self.records.len();
        if removed > 0 {
            self.rebuild_indices();
        }
        removed
    }

    /// Remove all records from memory. Call [`Self::save`] to persist the empty store.
    pub fn clear(&mut self) {
        self.records.clear();
        self.by_id.clear();
        self.by_item.clear();
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

    /// Persist the database to disk encrypted with AES-256-GCM (v2) using the DPAPI-derived key.
    ///
    /// Wire layout (`v2`):
    /// `MAGIC (4: "TSRV") || VERSION (1: 0x02) || NONCE (12 CSPRNG bytes) || TAG (16 GCM bytes) || CIPHERTEXT`
    /// where `AAD = MAGIC || VERSION || NONCE`.
    pub fn save(&self) -> Result<(), StoreError> {
        let mut in_out = Zeroizing::new(
            serde_json::to_vec(&self.records).map_err(|e| StoreError::KeyFile(format!("json serialization: {e}")))?,
        );
        let nonce = generate_nonce()?;
        let header = build_v2_header(&nonce);
        let unbound = UnboundKey::new(&AES_256_GCM, &*self.key)
            .map_err(|_| StoreError::KeyFile("aes-256-gcm key init failed".into()))?;
        let aead = LessSafeKey::new(unbound);
        let tag = aead
            .seal_in_place_separate_tag(Nonce::assume_unique_for_key(nonce), Aad::from(&header), &mut in_out)
            .map_err(|_| StoreError::KeyFile("aes-256-gcm encryption failed".into()))?;

        let mut output = Zeroizing::new(Vec::with_capacity(header.len() + TAG_LEN + in_out.len()));
        output.extend_from_slice(&header);
        output.extend_from_slice(tag.as_ref());
        output.extend_from_slice(&in_out);

        atomic_write(&self.path, &output)
    }

    /// Load the database from disk, decrypting AES-256-GCM (v2) or migrating authenticated legacy v1.
    pub fn load(&mut self) -> Result<(), StoreError> {
        let raw = fs::read(&self.path)
            .map_err(|e| StoreError::KeyFile(format!("read {}: {:?}", self.path.display(), e.kind())))?;

        if raw.len() < 5 || &raw[..4] != MAGIC {
            return Err(StoreError::WrongKeyOrNotDatabase);
        }

        match raw[4] {
            VERSION => self.load_v2(&raw),
            VERSION_V1 => self.load_and_migrate_v1(&raw),
            other if other > VERSION => Err(StoreError::SchemaTooNew {
                found: other as u32,
                supported: VERSION as u32,
            }),
            _ => Err(StoreError::WrongKeyOrNotDatabase),
        }
    }

    fn load_v2(&mut self, raw: &[u8]) -> Result<(), StoreError> {
        let min_len = 5 + NONCE_LEN + TAG_LEN;
        if raw.len() < min_len {
            return Err(StoreError::WrongKeyOrNotDatabase);
        }
        let header = &raw[..5 + NONCE_LEN];
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&raw[5..5 + NONCE_LEN]);
        let tag = &raw[5 + NONCE_LEN..5 + NONCE_LEN + TAG_LEN];
        let ciphertext = &raw[5 + NONCE_LEN + TAG_LEN..];

        let unbound = UnboundKey::new(&AES_256_GCM, &*self.key)
            .map_err(|_| StoreError::KeyFile("aes-256-gcm key init failed".into()))?;
        let aead = LessSafeKey::new(unbound);

        let mut in_out = Zeroizing::new(Vec::with_capacity(ciphertext.len() + TAG_LEN));
        in_out.extend_from_slice(ciphertext);
        in_out.extend_from_slice(tag);

        let plaintext = aead
            .open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(header), &mut in_out)
            .map_err(|_| StoreError::WrongKeyOrNotDatabase)?;

        let records: Vec<VectorRecord> =
            serde_json::from_slice(plaintext).map_err(|_| StoreError::WrongKeyOrNotDatabase)?;
        self.records = records;
        self.rebuild_indices();
        Ok(())
    }

    /// Authenticate and decrypt a legacy v1 file (SHA-256 CTR + SHA-256 MAC), save a `.v1.bak`
    /// backup before touching the primary file, then re-encrypt and save as AES-256-GCM v2.
    fn load_and_migrate_v1(&mut self, raw: &[u8]) -> Result<(), StoreError> {
        let min_len = 5 + V1_NONCE_LEN + V1_TAG_LEN;
        if raw.len() < min_len {
            return Err(StoreError::WrongKeyOrNotDatabase);
        }
        let nonce = &raw[5..5 + V1_NONCE_LEN];
        let tag = &raw[5 + V1_NONCE_LEN..5 + V1_NONCE_LEN + V1_TAG_LEN];
        let ciphertext = &raw[5 + V1_NONCE_LEN + V1_TAG_LEN..];

        let expected_tag = compute_v1_tag(&self.key, nonce, ciphertext);
        if !ct_eq(tag, &expected_tag) {
            return Err(StoreError::WrongKeyOrNotDatabase);
        }

        let plaintext = Zeroizing::new(crypt_v1_ctr(ciphertext, &self.key, nonce));
        let records: Vec<VectorRecord> =
            serde_json::from_slice(&plaintext).map_err(|_| StoreError::WrongKeyOrNotDatabase)?;

        let bak_path = v1_backup_path(&self.path);
        if !bak_path.exists() {
            atomic_write(&bak_path, raw)?;
        }

        self.records = records;
        self.rebuild_indices();
        self.save()
    }
}

fn build_v2_header(nonce: &[u8; NONCE_LEN]) -> [u8; 4 + 1 + NONCE_LEN] {
    let mut header = [0u8; 4 + 1 + NONCE_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4] = VERSION;
    header[5..].copy_from_slice(nonce);
    header
}

fn generate_nonce() -> Result<[u8; NONCE_LEN], StoreError> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| StoreError::KeyFile("csprng nonce generation failed".into()))?;
    Ok(nonce)
}

fn v1_backup_path(path: &Path) -> PathBuf {
    let mut os: OsString = path.as_os_str().to_os_string();
    os.push(".v1.bak");
    PathBuf::from(os)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let mut tmp_os: OsString = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp = PathBuf::from(tmp_os);
    {
        let mut f = fs::File::create(&tmp).map_err(|e| StoreError::KeyFile(format!("create tmp: {:?}", e.kind())))?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| StoreError::KeyFile(format!("write/sync: {:?}", e.kind())))?;
    }
    fs::rename(&tmp, path).map_err(|e| StoreError::KeyFile(format!("rename tmp: {:?}", e.kind())))?;
    Ok(())
}

/// Constant-time byte-slice equality comparison for legacy v1 MAC verification.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

/// Legacy v1 authenticated tag: SHA256(key || b"TSRV_AUTH" || nonce || ciphertext).
/// Retained strictly for migrating verified legacy v1 stores to AES-256-GCM v2.
fn compute_v1_tag(key: &[u8; KEY_LEN], nonce: &[u8], ciphertext: &[u8]) -> [u8; V1_TAG_LEN] {
    let mut h = Sha256::new();
    h.update(key);
    h.update(b"TSRV_AUTH");
    h.update(nonce);
    h.update(ciphertext);
    let mut tag = [0u8; V1_TAG_LEN];
    tag.copy_from_slice(&h.finalize());
    tag
}

/// Legacy v1 stream cipher helper retained strictly for decrypting legacy v1 stores during migration.
fn crypt_v1_ctr(input: &[u8], key: &[u8; KEY_LEN], nonce: &[u8]) -> Vec<u8> {
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
            db.insert(test_record("v1", 10, vec![0.6, 0.8], "Masked [PERSON_01]"))
                .unwrap();
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

    fn write_legacy_v1_fixture(path: &Path, key: &[u8; KEY_LEN], records: &[VectorRecord]) -> Vec<u8> {
        let plaintext = serde_json::to_vec(records).unwrap();
        let nonce = [0xABu8; V1_NONCE_LEN];
        let ciphertext = crypt_v1_ctr(&plaintext, key, &nonce);
        let tag = compute_v1_tag(key, &nonce, &ciphertext);
        let mut out = Vec::with_capacity(5 + V1_NONCE_LEN + V1_TAG_LEN + ciphertext.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION_V1);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&tag);
        out.extend_from_slice(&ciphertext);
        fs::write(path, &out).unwrap();
        out
    }

    #[test]
    fn v2_csprng_nonces_are_unique_across_rapid_saves() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonces_vec.enc");
        let key = [11u8; KEY_LEN];
        let mut db = VectorDb::new(path.clone(), &key);
        db.insert(test_record("n1", 1, vec![1.0, 0.0], "same plaintext"))
            .unwrap();

        db.save().unwrap();
        let first = fs::read(&path).unwrap();
        assert_eq!(first[4], VERSION);

        db.save().unwrap();
        let second = fs::read(&path).unwrap();
        assert_eq!(second[4], VERSION);

        let nonce1 = &first[5..5 + NONCE_LEN];
        let nonce2 = &second[5..5 + NONCE_LEN];
        assert_ne!(nonce1, nonce2, "CSPRNG nonces must never repeat across rapid saves");
        assert_ne!(first, second, "ciphertexts must differ cleanly");
    }

    #[test]
    fn v2_tampering_and_truncation_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tamper_vec.enc");
        let key = [19u8; KEY_LEN];

        let mut db = VectorDb::new(path.clone(), &key);
        db.insert(test_record("t1", 7, vec![0.5, 0.5], "Sensitive masked [EMAIL]"))
            .unwrap();
        db.save().unwrap();
        let valid = fs::read(&path).unwrap();

        // Flip ciphertext byte
        let mut flipped_ct = valid.clone();
        let last = flipped_ct.len() - 1;
        flipped_ct[last] ^= 0x01;
        fs::write(&path, &flipped_ct).unwrap();
        assert!(matches!(
            VectorDb::open_or_create(&path, &key).err().unwrap(),
            StoreError::WrongKeyOrNotDatabase
        ));

        // Flip tag byte
        let mut flipped_tag = valid.clone();
        flipped_tag[5 + NONCE_LEN] ^= 0x80;
        fs::write(&path, &flipped_tag).unwrap();
        assert!(matches!(
            VectorDb::open_or_create(&path, &key).err().unwrap(),
            StoreError::WrongKeyOrNotDatabase
        ));

        // Flip nonce (also AAD)
        let mut flipped_nonce = valid.clone();
        flipped_nonce[5] ^= 0x01;
        fs::write(&path, &flipped_nonce).unwrap();
        assert!(matches!(
            VectorDb::open_or_create(&path, &key).err().unwrap(),
            StoreError::WrongKeyOrNotDatabase
        ));

        // Truncated header
        fs::write(&path, &valid[..10]).unwrap();
        assert!(matches!(
            VectorDb::open_or_create(&path, &key).err().unwrap(),
            StoreError::WrongKeyOrNotDatabase
        ));

        // Future version rejected as SchemaTooNew
        let mut future_ver = valid;
        future_ver[4] = 99;
        fs::write(&path, &future_ver).unwrap();
        assert!(matches!(
            VectorDb::open_or_create(&path, &key).err().unwrap(),
            StoreError::SchemaTooNew {
                found: 99,
                supported: 2
            }
        ));
    }

    #[test]
    fn v1_migration_creates_backup_and_upgrades_to_v2() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tessera_vectors.enc");
        let key = [31u8; KEY_LEN];
        let rec = test_record("m1", 42, vec![0.3, 0.4, 0.5], "Migrated [PERSON_01]");

        let v1_bytes = write_legacy_v1_fixture(&path, &key, std::slice::from_ref(&rec));
        assert_eq!(v1_bytes[4], VERSION_V1);

        let migrated = VectorDb::open_or_create(&path, &key).unwrap();
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated.get("m1").unwrap().text, "Migrated [PERSON_01]");

        let upgraded_bytes = fs::read(&path).unwrap();
        assert_eq!(upgraded_bytes[4], VERSION);

        let bak_path = v1_backup_path(&path);
        assert!(bak_path.exists());
        assert_eq!(fs::read(&bak_path).unwrap(), v1_bytes);

        // Reloading now uses v2 directly
        let reloaded = VectorDb::open_or_create(&path, &key).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.get("m1").unwrap().metadata.item_id, 42);
    }

    #[test]
    fn corrupted_v1_rejected_without_creating_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad_v1.enc");
        let key = [5u8; KEY_LEN];
        let rec = test_record("bad", 1, vec![1.0, 0.0], "corrupt me");

        let mut v1_bytes = write_legacy_v1_fixture(&path, &key, &[rec]);
        let idx = v1_bytes.len() - 1;
        v1_bytes[idx] ^= 0xFF;
        fs::write(&path, &v1_bytes).unwrap();

        let err = VectorDb::open_or_create(&path, &key).err().unwrap();
        assert!(matches!(err, StoreError::WrongKeyOrNotDatabase));
        assert!(!v1_backup_path(&path).exists());
        assert_eq!(fs::read(&path).unwrap(), v1_bytes);
    }

    #[test]
    fn batch_delete_and_clear() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("batch_vec.enc");
        let key = [2u8; KEY_LEN];
        let mut db = VectorDb::new(path, &key);
        db.insert(test_record("x1", 10, vec![1.0, 0.0], "x1")).unwrap();
        db.insert(test_record("x2", 20, vec![0.0, 1.0], "x2")).unwrap();
        db.insert(test_record("x3", 30, vec![1.0, 1.0], "x3")).unwrap();

        assert_eq!(db.delete_by_items(&[10, 30]), 2);
        assert_eq!(db.len(), 1);
        assert!(db.get("x2").is_some());

        db.clear();
        assert!(db.is_empty());
    }
}
