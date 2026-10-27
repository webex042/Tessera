//! Opening the store: DPAPI-wrapped key file + SQLCipher database + migrations (plan §9.1).
//!
//! Data dir layout: `tessara.db` (SQLCipher) and `key.bin` = `"TSRK"` + version byte + DPAPI blob.
//! A key is created only when **neither** file exists. If the database exists but the key file
//! is missing or unreadable, opening fails: inventing a new key would make the old database
//! permanently unreadable and hide the real problem.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::{KEY_LEN, StoreError, migrate, open_encrypted};

const MAGIC: &[u8; 4] = b"TSRK";
const VERSION: u8 = 1;
pub const DB_FILE: &str = "tessara.db";
pub const KEY_FILE: &str = "key.bin";

pub struct StorePaths {
    pub db: PathBuf,
    pub key: PathBuf,
}

pub fn paths(data_dir: &Path) -> StorePaths {
    StorePaths {
        db: data_dir.join(DB_FILE),
        key: data_dir.join(KEY_FILE),
    }
}

fn read_key(path: &Path) -> Result<Zeroizing<[u8; KEY_LEN]>, StoreError> {
    let raw = fs::read(path).map_err(|e| StoreError::KeyFile(format!("read: {:?}", e.kind())))?;
    if raw.len() < 6 || &raw[..4] != MAGIC {
        return Err(StoreError::KeyFile("not a Tessara key file".into()));
    }
    if raw[4] != VERSION {
        return Err(StoreError::KeyFile(format!("unsupported key file version {}", raw[4])));
    }
    let key = tsr_win::dpapi::unprotect(&raw[5..]).map_err(|e| StoreError::Dpapi(e.code))?;
    if key.len() != KEY_LEN {
        return Err(StoreError::KeyFile("unwrapped key has the wrong length".into()));
    }
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    out.copy_from_slice(&key);
    Ok(out)
}

fn create_key(path: &Path) -> Result<Zeroizing<[u8; KEY_LEN]>, StoreError> {
    let key = tsr_win::dpapi::random_bytes::<KEY_LEN>().map_err(|e| StoreError::Dpapi(e.code))?;
    let wrapped = tsr_win::dpapi::protect(&key[..]).map_err(|e| StoreError::Dpapi(e.code))?;
    // Write to a temp file, flush to disk, then rename: a crash never leaves a half-written key.
    let tmp = path.with_extension("bin.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| StoreError::KeyFile(format!("create: {:?}", e.kind())))?;
        f.write_all(MAGIC)
            .and_then(|_| f.write_all(&[VERSION]))
            .and_then(|_| f.write_all(&wrapped))
            .and_then(|_| f.sync_all())
            .map_err(|e| StoreError::KeyFile(format!("write: {:?}", e.kind())))?;
    }
    if path.exists() {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::KeyFile(
            "key file appeared concurrently; refusing to overwrite".into(),
        ));
    }
    fs::rename(&tmp, path).map_err(|e| StoreError::KeyFile(format!("rename: {:?}", e.kind())))?;
    tracing::info!(bytes = (5 + wrapped.len()) as u64, "store key created");
    Ok(key)
}

/// Open (creating on first run) the encrypted store in `data_dir` and bring its schema up to date.
pub fn open_store(data_dir: &Path) -> Result<Connection, StoreError> {
    fs::create_dir_all(data_dir).map_err(|e| StoreError::KeyFile(format!("data dir: {:?}", e.kind())))?;
    let p = paths(data_dir);
    let key = match (p.key.exists(), p.db.exists()) {
        (true, _) => read_key(&p.key)?,
        (false, true) => return Err(StoreError::KeyMissing),
        (false, false) => create_key(&p.key)?,
    };
    let mut conn = open_encrypted(&p.db, &key)?;
    drop(key);
    let v = migrate::migrate(&mut conn)?;
    tracing::info!(schema_version = v, "store opened");
    Ok(conn)
}
