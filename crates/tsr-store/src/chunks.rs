//! Embedding chunks (plan §9.4): int8 vectors plus one f32 scale per chunk, tagged with the
//! embedding model and chunker version so vectors from different models are never mixed.

use rusqlite::{Connection, params};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq)]
pub struct NewChunk {
    pub ord: u32,
    pub start_ch: i64,
    pub end_ch: i64,
    pub scale: f32,
    pub vec: Vec<i8>,
}

/// Replace all chunks of `item_id` for `model` in one statement group (caller's transaction).
pub fn replace_for_item(
    conn: &Connection,
    item_id: i64,
    model: &str,
    chunker_ver: &str,
    chunks: &[NewChunk],
) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM chunks WHERE item_id = ?1 AND model = ?2",
        params![item_id, model],
    )?;
    let mut st = conn.prepare(
        "INSERT INTO chunks (item_id, ord, start_ch, end_ch, chunker_ver, model, dims, scale, vec)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for c in chunks {
        if !(c.scale.is_finite() && c.scale > 0.0) || c.vec.is_empty() {
            return Err(StoreError::Invalid("chunk vector"));
        }
        let bytes: Vec<u8> = c.vec.iter().map(|&x| x as u8).collect();
        st.execute(params![
            item_id,
            c.ord,
            c.start_ch,
            c.end_ch,
            chunker_ver,
            model,
            c.vec.len() as i64,
            c.scale as f64,
            bytes
        ])?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub id: i64,
    pub item_id: i64,
    pub ord: u32,
    pub start_ch: i64,
    pub end_ch: i64,
    pub scale: f32,
    pub vec: Vec<i8>,
}

/// Every chunk of one model, for loading the in-memory index.
pub fn all_for_model(conn: &Connection, model: &str) -> Result<Vec<ChunkRow>, StoreError> {
    let mut st =
        conn.prepare("SELECT id, item_id, ord, start_ch, end_ch, scale, vec FROM chunks WHERE model = ?1 ORDER BY id")?;
    let rows = st
        .query_map([model], |r| {
            let b: Vec<u8> = r.get(6)?;
            Ok(ChunkRow {
                id: r.get(0)?,
                item_id: r.get(1)?,
                ord: r.get(2)?,
                start_ch: r.get(3)?,
                end_ch: r.get(4)?,
                scale: r.get::<_, f64>(5)? as f32,
                vec: b.into_iter().map(|x| x as i8).collect(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Models that have vectors stored, with counts (consistency checks, re-index decisions).
pub fn models(conn: &Connection) -> Result<Vec<(String, u64)>, StoreError> {
    let mut st = conn.prepare("SELECT model, count(*) FROM chunks GROUP BY model ORDER BY model")?;
    let rows = st
        .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn delete_model(conn: &Connection, model: &str) -> Result<u64, StoreError> {
    Ok(conn.execute("DELETE FROM chunks WHERE model = ?1", [model])? as u64)
}
