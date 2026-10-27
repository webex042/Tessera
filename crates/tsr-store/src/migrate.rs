//! Forward-only schema migrations tracked in `PRAGMA user_version` (plan §9.2).
//!
//! The plan names `rusqlite_migration`; at Stage 1 it did not yet support rusqlite 0.41, and the
//! job is small, so this module does it directly. Each migration runs in its own transaction
//! together with the version bump, so a failure leaves the database at the previous version.

use rusqlite::Connection;

use crate::StoreError;

pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "blocklist_audit_settings",
        sql: include_str!("../migrations/0001_blocklist_audit_settings.sql"),
    },
    Migration {
        version: 2,
        name: "sources_items_fts",
        sql: include_str!("../migrations/0002_sources_items_fts.sql"),
    },
];

pub fn current_version(conn: &Connection) -> Result<u32, StoreError> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Bring the database to the latest schema. Returns the resulting version.
pub fn migrate(conn: &mut Connection) -> Result<u32, StoreError> {
    migrate_with(conn, MIGRATIONS)
}

/// Same as [`migrate`] with an explicit list; tests use it to simulate older databases.
pub fn migrate_with(conn: &mut Connection, list: &[Migration]) -> Result<u32, StoreError> {
    for (i, m) in list.iter().enumerate() {
        assert_eq!(
            m.version as usize,
            i + 1,
            "migrations must be numbered 1, 2, 3, ... ({})",
            m.name
        );
    }
    let latest = list.len() as u32;
    let current = current_version(conn)?;
    if current > latest {
        // Opened by an older build than the one that wrote it: refuse instead of guessing.
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported: latest,
        });
    }
    for m in &list[current as usize..] {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql)?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
    }
    Ok(latest)
}
