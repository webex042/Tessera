//! People, aliases, merge suggestions and the suppression list (plan §8.5, §15.5).
//!
//! Aliases arrive already normalised (`tsr_pipeline::people::normalize_alias`). Suppressed
//! aliases are stored as `sha256(salt ‖ kind ‖ 0x00 ‖ alias)` with a random per-database salt, so the
//! list can answer "is this suppressed?" without keeping the forgotten names.

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use tsr_core::Sensitive;

use crate::StoreError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AliasKind {
    Name,
    Email,
    Handle,
}

impl AliasKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AliasKind::Name => "name",
            AliasKind::Email => "email",
            AliasKind::Handle => "handle",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "name" => AliasKind::Name,
            "email" => AliasKind::Email,
            "handle" => AliasKind::Handle,
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub struct Person {
    pub id: i64,
    pub display_name: Sensitive<String>,
    pub is_self: bool,
}

pub fn self_person(conn: &Connection) -> Result<Option<i64>, StoreError> {
    Ok(conn
        .query_row("SELECT id FROM people WHERE is_self = 1", [], |r| r.get(0))
        .optional()?)
}

/// Create the self person once; later calls add any new aliases and return the same id.
pub fn ensure_self(
    conn: &Connection,
    display_name: &str,
    aliases: &[(AliasKind, &str)],
    now: i64,
) -> Result<i64, StoreError> {
    let id = match self_person(conn)? {
        Some(id) => id,
        None => {
            conn.execute(
                "INSERT INTO people (display_name, is_self, created_at) VALUES (?1, 1, ?2)",
                params![display_name, now],
            )?;
            conn.last_insert_rowid()
        }
    };
    for (k, a) in aliases {
        add_alias(conn, id, *k, a)?;
    }
    Ok(id)
}

pub fn create_person(
    conn: &Connection,
    display_name: &str,
    aliases: &[(AliasKind, &str)],
    now: i64,
) -> Result<i64, StoreError> {
    conn.execute(
        "INSERT INTO people (display_name, created_at) VALUES (?1, ?2)",
        params![display_name, now],
    )?;
    let id = conn.last_insert_rowid();
    for (k, a) in aliases {
        add_alias(conn, id, *k, a)?;
    }
    Ok(id)
}

/// Returns whether the alias was new. An e-mail/handle already owned by someone else is refused.
pub fn add_alias(conn: &Connection, person: i64, kind: AliasKind, alias: &str) -> Result<bool, StoreError> {
    if matches!(kind, AliasKind::Email | AliasKind::Handle)
        && find_by_alias(conn, kind, alias)?.into_iter().any(|p| p != person)
    {
        return Ok(false);
    }
    Ok(conn.execute(
        "INSERT INTO person_aliases (person_id, kind, alias) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
        params![person, kind.as_str(), alias],
    )? == 1)
}

/// People with this exact (normalised) alias. Names can match several people.
pub fn find_by_alias(conn: &Connection, kind: AliasKind, alias: &str) -> Result<Vec<i64>, StoreError> {
    let mut st =
        conn.prepare("SELECT person_id FROM person_aliases WHERE kind = ?1 AND alias = ?2 ORDER BY person_id")?;
    let ids = st
        .query_map(params![kind.as_str(), alias], |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}

pub fn aliases_of(conn: &Connection, person: i64) -> Result<Vec<(AliasKind, Sensitive<String>)>, StoreError> {
    let mut st = conn.prepare("SELECT kind, alias FROM person_aliases WHERE person_id = ?1 ORDER BY kind, alias")?;
    let rows = st
        .query_map([person], |r| {
            let k: String = r.get(0)?;
            Ok((
                AliasKind::parse(&k).unwrap_or(AliasKind::Name),
                Sensitive::new(r.get::<_, String>(1)?),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn person(conn: &Connection, id: i64) -> Result<Option<Person>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT id, display_name, is_self FROM people WHERE id = ?1",
            [id],
            |r| {
                Ok(Person {
                    id: r.get(0)?,
                    display_name: Sensitive::new(r.get(1)?),
                    is_self: r.get(2)?,
                })
            },
        )
        .optional()?)
}

pub fn suggest_merge(conn: &Connection, a: i64, b: i64, reason: &str, now: i64) -> Result<(), StoreError> {
    if a == b {
        return Ok(());
    }
    let (a, b) = (a.min(b), a.max(b));
    conn.execute(
        "INSERT INTO person_merge_suggestions (a, b, reason, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
        params![a, b, reason, now],
    )?;
    Ok(())
}

pub fn merge_suggestions(conn: &Connection) -> Result<Vec<(i64, i64, String)>, StoreError> {
    let mut st = conn.prepare("SELECT a, b, reason FROM person_merge_suggestions ORDER BY a, b")?;
    let rows = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// User-confirmed merge: aliases and fact links of `remove` move to `keep`.
pub fn merge_people(conn: &mut Connection, keep: i64, remove: i64) -> Result<(), StoreError> {
    if keep == remove {
        return Ok(());
    }
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO person_aliases (person_id, kind, alias)
         SELECT ?1, kind, alias FROM person_aliases WHERE person_id = ?2 AND kind = 'name' ON CONFLICT DO NOTHING",
        params![keep, remove],
    )?;
    // E-mail/handle are globally unique: move them by update, not copy.
    tx.execute(
        "UPDATE person_aliases SET person_id = ?1 WHERE person_id = ?2 AND kind IN ('email', 'handle')",
        params![keep, remove],
    )?;
    tx.execute(
        "UPDATE facts SET owner_id = ?1 WHERE owner_id = ?2",
        params![keep, remove],
    )?;
    tx.execute(
        "UPDATE facts SET counterparty_id = ?1 WHERE counterparty_id = ?2",
        params![keep, remove],
    )?;
    tx.execute("DELETE FROM people WHERE id = ?1 AND is_self = 0", [remove])?;
    tx.commit()?;
    Ok(())
}

// --- Suppression -----------------------------------------------------------------------------

pub fn suppression_salt(conn: &Connection) -> Result<String, StoreError> {
    let v: String = conn.query_row(
        "SELECT json_extract(value, '$') FROM settings WHERE key = 'suppression_salt'",
        [],
        |r| r.get(0),
    )?;
    Ok(v)
}

/// Keyed digest of a normalised alias.
pub fn alias_digest(salt: &str, kind: AliasKind, alias: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(kind.as_str().as_bytes());
    h.update([0u8]);
    h.update(alias.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn suppress(conn: &Connection, salt: &str, kind: AliasKind, alias: &str, now: i64) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO suppressed_aliases (kind, digest, created_at) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
        params![kind.as_str(), alias_digest(salt, kind, alias), now],
    )?;
    Ok(())
}

pub fn unsuppress(conn: &Connection, salt: &str, kind: AliasKind, alias: &str) -> Result<bool, StoreError> {
    Ok(conn.execute(
        "DELETE FROM suppressed_aliases WHERE kind = ?1 AND digest = ?2",
        params![kind.as_str(), alias_digest(salt, kind, alias)],
    )? == 1)
}

/// All suppressed digests (small; loaded once per extraction batch).
pub fn suppressed_digests(conn: &Connection) -> Result<std::collections::HashSet<(AliasKind, String)>, StoreError> {
    let mut st = conn.prepare("SELECT kind, digest FROM suppressed_aliases")?;
    let rows = st
        .query_map([], |r| {
            let k: String = r.get(0)?;
            Ok((AliasKind::parse(&k).unwrap_or(AliasKind::Name), r.get::<_, String>(1)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForgetReport {
    pub facts: u64,
    pub aliases: u64,
    pub people: u64,
}

/// "Forget a person" (plan §15.5): suppress every alias, delete the person's facts (as owner or
/// counterparty) and the person. With `dry_run`, only count. Items that merely mention the person
/// are **not** deleted here — the UI lists them (search by alias) and the user deletes them
/// explicitly, so suppression never turns into an unbounded delete.
pub fn forget_person(conn: &mut Connection, id: i64, dry_run: bool, now: i64) -> Result<ForgetReport, StoreError> {
    let salt = suppression_salt(conn)?;
    let tx = conn.transaction()?;
    let facts: i64 = tx.query_row(
        "SELECT count(*) FROM facts WHERE owner_id = ?1 OR counterparty_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    let aliases = {
        let mut st = tx.prepare("SELECT kind, alias FROM person_aliases WHERE person_id = ?1")?;
        st.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let is_self: bool = tx
        .query_row("SELECT is_self FROM people WHERE id = ?1", [id], |r| r.get(0))
        .optional()?
        .unwrap_or(false);
    let report = ForgetReport {
        facts: facts as u64,
        aliases: aliases.len() as u64,
        people: u64::from(!is_self),
    };
    if dry_run || is_self {
        return Ok(ForgetReport { people: 0, ..report });
    }
    for (k, a) in &aliases {
        let kind = AliasKind::parse(k).unwrap_or(AliasKind::Name);
        tx.execute(
            "INSERT INTO suppressed_aliases (kind, digest, created_at) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
            params![kind.as_str(), alias_digest(&salt, kind, a), now],
        )?;
    }
    tx.execute("DELETE FROM facts WHERE owner_id = ?1 OR counterparty_id = ?1", [id])?;
    tx.execute("DELETE FROM people WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(report)
}
