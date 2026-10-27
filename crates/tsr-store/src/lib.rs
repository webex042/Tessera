//! Encrypted store for Tessara (plan §9).
//!
//! Stage 0 scope: open a SQLCipher database with a raw 32-byte key, apply the
//! security PRAGMAs from §9.1, and confirm SQLCipher and FTS5 are really compiled in.
//! Stage 1 adds schema migrations ([`migrate`]) and the blocklist repository ([`blocklist`]).
//! Key management (DPAPI wrapping) follows.

pub mod audit;
pub mod blocklist;
pub mod chunks;
pub mod facts;
pub mod items;
pub mod jobs;
#[cfg(windows)]
pub mod keystore;
pub mod migrate;
pub mod people;
pub mod summaries;

use std::path::Path;

use rusqlite::Connection;
use zeroize::Zeroizing;

/// Length of the raw SQLCipher key in bytes (256-bit).
pub const KEY_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The key did not decrypt the file, or the file is not a SQLCipher database.
    #[error("database could not be opened with this key")]
    WrongKeyOrNotDatabase,
    /// The linked SQLite is not SQLCipher (cipher_version is empty).
    #[error("SQLCipher is not linked")]
    NotSqlcipher,
    /// FTS5 is missing from the SQLite build.
    #[error("FTS5 is not available")]
    NoFts5,
    #[error("sqlite error code {0}")]
    Sqlite(i32),
    /// The database was written by a newer build (schema version above what this build knows).
    #[error("database schema {found} is newer than supported {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
    /// A blocklist rule was rejected (empty, too long, or a regex that does not compile).
    #[error("invalid blocklist rule")]
    InvalidRule,
    /// The database exists but its key file does not (never silently create a new key).
    #[error("database exists but key file is missing")]
    KeyMissing,
    #[error("key file problem: {0}")]
    KeyFile(String),
    #[error("DPAPI error {0:#010x}")]
    Dpapi(i32),
    /// A value the repository refuses before it reaches SQL (content-free reason).
    #[error("invalid value: {0}")]
    Invalid(&'static str),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        // Only the numeric code is kept: error messages can quote SQL, which may contain content.
        match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => StoreError::WrongKeyOrNotDatabase,
            _ => StoreError::Sqlite(e.sqlite_error().map(|s| s.extended_code).unwrap_or(-1)),
        }
    }
}

/// Facts about the linked SQLite build, used by Stage 0 verification.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub sqlite_version: String,
    pub cipher_version: String,
    pub cipher_provider: String,
    pub has_fts5: bool,
}

/// Open (or create) an encrypted database and apply the §9.1 PRAGMAs.
///
/// The key is passed as a raw key (`x'…'`), so SQLCipher skips its own key derivation.
/// The hex copy we build is zeroized on drop. SQLite keeps its own copy of the SQL text
/// internally, which we cannot wipe; `cipher_memory_security` reduces that exposure.
pub fn open_encrypted(path: &Path, key: &[u8; KEY_LEN]) -> Result<Connection, StoreError> {
    let conn = Connection::open(path)?;

    // SQLCipher logs to stderr by default (e.g. on a wrong key). All diagnostics must go through
    // our content-free tracing layer instead, so silence it before the key is even tried.
    let level: String = conn.query_row("PRAGMA cipher_log_level = NONE", [], |r| r.get(0))?;
    debug_assert_eq!(level, "NONE");

    let mut hex = Zeroizing::new(String::with_capacity(KEY_LEN * 2 + 16));
    hex.push_str("PRAGMA key = \"x'");
    for b in key {
        hex.push_str(&format!("{b:02x}"));
    }
    hex.push_str("'\";");
    conn.execute_batch(&hex)?;
    drop(hex);

    // Reading the schema is the first statement that actually decrypts a page,
    // so a wrong key fails here rather than later.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;

    conn.execute_batch(
        "PRAGMA cipher_memory_security = ON;
         PRAGMA secure_delete = ON;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA synchronous = NORMAL;",
    )?;
    // journal_mode returns a row, so it cannot go through execute_batch.
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    debug_assert_eq!(mode.to_ascii_lowercase(), "wal");

    Ok(conn)
}

/// Report what the linked SQLite actually is. Fails if SQLCipher or FTS5 is missing.
pub fn build_info(conn: &Connection) -> Result<BuildInfo, StoreError> {
    let sqlite_version: String = conn.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let cipher_version: String = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .unwrap_or_default();
    if cipher_version.is_empty() {
        return Err(StoreError::NotSqlcipher);
    }
    let cipher_provider: String = conn
        .query_row("PRAGMA cipher_provider", [], |r| r.get(0))
        .unwrap_or_default();

    let mut stmt = conn.prepare("PRAGMA compile_options")?;
    let has_fts5 = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .any(|opt| opt == "ENABLE_FTS5");
    if !has_fts5 {
        return Err(StoreError::NoFts5);
    }

    Ok(BuildInfo {
        sqlite_version,
        cipher_version,
        cipher_provider,
        has_fts5,
    })
}
