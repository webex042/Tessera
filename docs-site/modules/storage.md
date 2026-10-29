# Storage Architecture & Vector Database (`tsr-store`)

The `tsr-store` crate manages Tessera's local persistence architecture, combining an embedded, encrypted-at-rest local vector database (`VectorDb`) for semantic memory, chunk embeddings, and retrieval metadata, alongside an encrypted relational store for structured facts, sources, and audit logs.

---

## 1. Local Encrypted Vector Database (`VectorDb`)

To replace dependence on SQL-based chunk retrieval with native semantic vector similarity, Tessera includes a dedicated, embedded vector database engine implemented in `crates/tsr-store/src/vectordb.rs` within the `tsr_store::vectordb` module.

The embedded vector database is intended to support local, in-process semantic retrieval without requiring an external vector database server or cloud service.

### Core Data Structures & Types

| Component | Rust Type | Description |
|---|---|---|
| **Database Instance** | `VectorDb` | Embedded in-process vector store managing encrypted disk persistence, in-memory records, and indexing |
| **Record Unit** | `VectorRecord` | Stored entry containing record ID, model ID, vector dimensions, scale factor, quantized `int8` vector, normalized `f32` vector, redacted text chunk, and metadata |
| **Metadata Record** | `VectorMetadata` | Structured provenance: `item_id`, `source_id`, `app_name`, `window_title`, `url`, `memory_type`, `timestamp_ms`, `ord`, `start_ch`, `end_ch`, `provenance` |
| **Filter Specification** | `VectorFilter` | Pre-filtering criteria matching model, item IDs, source IDs, application names, memory types, and millisecond time ranges (`min_ms..=max_ms`) |
| **Search Hit** | `VectorHit` | Query match returning record ID, item ID, cosine similarity score, redacted chunk text, and associated `VectorMetadata` |

### Architectural Specifications
- **Embedding Model:** Local `multilingual-e5-small` in quantized ONNX format.
- **Vector Dimensionality:** 384 dimensions.
- **Similarity Metric:** Cosine similarity (computed via L2-normalized `f32` dot products and scaled quantized `int8` dot products).
- **Metadata Pre-Filtering:** Filters records before vector scoring by application name, time range (`timestamp_ms`), item ID, source ID, and memory type (`passage`, `fact`, `summary`).
- **Storage Location:** `%LOCALAPPDATA%\Tessara\tessera_vectors.enc`.
- **Encryption at Rest:** 256-bit symmetric key unwrapped from the Windows Data Protection API (`CryptUnprotectData`). Uses an authenticated SHA-256 CTR stream keystream with a random 16-byte nonce and HMAC integrity tag (`TSRV` magic header).

### Cryptographic File Layout (`tessera_vectors.enc`)
```
+----------------+---------------+----------------+----------------+--------------------------+
| Magic (4 bytes)| Version (1 B) | Nonce (16 B)   | Tag (32 B)     | Ciphertext (N bytes)     |
| "TSRV"         | 0x01          | Random Salt    | SHA-256 HMAC   | AES-CTR Keystream Stream |
+----------------+---------------+----------------+----------------+--------------------------+
```

---

## 2. Storage Roles: VectorDb vs. SQLCipher

Tessera maintains a clear architectural division between vector retrieval and relational persistence:

- **`VectorDb` (Vector Retrieval Store):** Serves as the primary database for vector-based semantic retrieval, housing text chunks, 384-dimensional embeddings, and retrieval filters.
- **SQLCipher (Relational Store):** Remains in the application as the transactional store for structured relational facts, evidence spans, window sources, captured items, people/aliases, background jobs, and content-free audit events. SQLCipher is not deleted or replaced for these relational responsibilities.
- **`migrate_sqlcipher_to_vectordb`:** Transfers eligible legacy chunk data and item texts into the encrypted `VectorDb`.

---

## 3. Safe Data Migration Utility (`tsr_store::migration`)

Tessera provides a safe, idempotent, non-destructive migration utility to transition existing relational chunk data to the encrypted vector database:

```rust
pub fn migrate_sqlcipher_to_vectordb(
    conn: &Connection,
    vector_db: &mut VectorDb,
) -> Result<MigrationReport, StoreError>
```

### Migration Lifecycle & Guarantees
1. **Automatic Backup:** Automatically creates a backup of the active database at `tessara.db.bak` before initiating migration.
2. **PII Masking Application:** Reads legacy text and window titles, applying `mask_pii` with typed placeholders (`[PERSON_01]`, `[EMAIL]`, etc.) before vector database insertion.
3. **Metadata & Provenance Preservation:** Reconstructs vector embeddings and binds each record to its originating source metadata (`item_id`, `source_id`, `app_name`, `timestamp_ms`).
4. **Idempotent Execution:** Can be safely re-run without creating duplicate records or corrupting existing indexes.
5. **Non-Destructive Retention:** Retains the original SQLCipher database intact; original records are not dropped during migration.

---

## 4. Relational Storage Configuration

When opening the local database (`%LOCALAPPDATA%\Tessara\tessara.db`), Tessera executes the following sequence:

```rust
pub fn open_encrypted(path: &Path, key: &[u8; KEY_LEN]) -> Result<Connection, StoreError> {
    let conn = Connection::open(path)?;
    // 1. Silence stderr logging to ensure content-free diagnostics
    conn.query_row("PRAGMA cipher_log_level = NONE", [], |_| Ok(()))?;
    // 2. Inject 32-byte raw key (skips PBKDF2 derivation)
    conn.execute_batch(&format!("PRAGMA key = \"x'{hex_key}'\";"))?;
    // 3. Test page decrypt
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
    // 4. Enforce security PRAGMAs
    conn.execute_batch(
        "PRAGMA cipher_memory_security = ON;
         PRAGMA secure_delete = ON;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA synchronous = NORMAL;"
    )?;
    // 5. Enable Write-Ahead Logging
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    Ok(conn)
}
```

### Key Security PRAGMAs
- **`cipher_memory_security = ON`:** Overwrites decrypted cache pages in RAM with zeros when released.
- **`secure_delete = ON`:** Overwrites deleted rows with zeros, preventing raw plaintext forensics from recovery tools.
- **`temp_store = MEMORY`:** Temporary indexes and sorting tables are held in memory; zero plaintext spills to `%TEMP%`.
- **`journal_mode = WAL`:** High-concurrency Write-Ahead Logging allows background extraction threads to write facts while the UI reads timelines.

---

## 2. Database Schema & Tables

Tessera tracks memories and derived intelligence across ten core tables:

```mermaid
erDiagram
    sources ||--o{ items : produces
    items ||--o{ chunks : indexed_as
    items ||--o{ evidence : substantiates
    facts ||--o{ evidence : evidenced_by
    people ||--o{ aliases : has
    facts }o--o| people : assigned_to

    sources {
        INTEGER id PK
        TEXT kind
        TEXT identifier
        TEXT display_name
    }
    items {
        INTEGER id PK
        INTEGER source_id FK
        TEXT content_hash
        TEXT raw_text
        INTEGER created_at
    }
    facts {
        INTEGER id PK
        TEXT kind
        TEXT text
        TEXT status
        INTEGER owner_id FK
        INTEGER due_at
    }
    evidence {
        INTEGER id PK
        INTEGER fact_id FK
        INTEGER item_id FK
        TEXT quote
        INTEGER start_ch
        INTEGER end_ch
    }
```

- **`sources`:** Unique origins (application executable, web domain, or document path).
- **`items`:** Raw captured sessions (redacted text, timestamps, SimHash).
- **`chunks`:** 512-token segments paired with 384-dimensional int8 embedding BLOBs.
- **`facts`:** Atomic intelligence (action, decision, event, question, note).
- **`evidence`:** Character spans (`start_ch..end_ch`) in `items` proving a fact.
- **`audit_events`:** Content-free tracking of system transitions, deletions, and model calls.
