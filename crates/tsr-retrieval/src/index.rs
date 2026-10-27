//! In-memory vector index over `chunks` (plan §9.4): int8 vectors + per-vector scale, brute-force
//! dot product, top-k per item. One index = one embedding model; rows of another model or another
//! size are refused, so incompatible vectors are never compared. The database is the source of
//! truth: the index is rebuilt from it on start, updated per item, and can be checked against it.

use std::collections::HashMap;

use rusqlite::Connection;
use tsr_store::StoreError;
use tsr_store::chunks;

/// Above this, switch to an ANN index (plan D6); refuse rather than grow without bound.
pub const MAX_ROWS: usize = 200_000;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    #[error("store: {0}")]
    Store(String),
    #[error("chunk {0} has {1} dimensions, index has {2}")]
    Dims(i64, usize, usize),
    #[error("index full ({0} vectors)")]
    Full(usize),
}

impl From<StoreError> for IndexError {
    fn from(e: StoreError) -> Self {
        IndexError::Store(e.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Row {
    chunk_id: i64,
    item_id: i64,
    start_ch: i64,
    end_ch: i64,
    dead: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VecHit {
    pub item_id: i64,
    pub chunk_id: i64,
    pub start_ch: i64,
    pub end_ch: i64,
    pub score: f32,
}

pub struct VectorIndex {
    pub model: String,
    pub dims: usize,
    rows: Vec<Row>,
    data: Vec<i8>,
    scales: Vec<f32>,
    by_item: HashMap<i64, Vec<usize>>,
    live: usize,
}

impl VectorIndex {
    pub fn empty(model: &str, dims: usize) -> Self {
        VectorIndex {
            model: model.to_owned(),
            dims,
            rows: Vec::new(),
            data: Vec::new(),
            scales: Vec::new(),
            by_item: HashMap::new(),
            live: 0,
        }
    }

    pub fn load(conn: &Connection, model: &str, dims: usize) -> Result<Self, IndexError> {
        let mut ix = Self::empty(model, dims);
        for r in chunks::all_for_model(conn, model)? {
            ix.push(r.id, r.item_id, r.start_ch, r.end_ch, &r.vec, r.scale)?;
        }
        Ok(ix)
    }

    fn push(&mut self, chunk_id: i64, item_id: i64, s: i64, e: i64, v: &[i8], scale: f32) -> Result<(), IndexError> {
        if v.len() != self.dims {
            return Err(IndexError::Dims(chunk_id, v.len(), self.dims));
        }
        if self.live >= MAX_ROWS {
            return Err(IndexError::Full(self.live));
        }
        self.by_item.entry(item_id).or_default().push(self.rows.len());
        self.rows.push(Row {
            chunk_id,
            item_id,
            start_ch: s,
            end_ch: e,
            dead: false,
        });
        self.data.extend_from_slice(v);
        self.scales.push(scale);
        self.live += 1;
        Ok(())
    }

    /// Replace an item's vectors with the rows now stored for it.
    pub fn refresh_item(&mut self, conn: &Connection, item_id: i64) -> Result<(), IndexError> {
        self.remove_item(item_id);
        let mut st = conn
            .prepare(
                "SELECT id, start_ch, end_ch, scale, vec FROM chunks WHERE item_id = ?1 AND model = ?2 ORDER BY ord",
            )
            .map_err(|e| IndexError::Store(StoreError::from(e).to_string()))?;
        let rows: Vec<(i64, i64, i64, f64, Vec<u8>)> = st
            .query_map(rusqlite::params![item_id, self.model], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .and_then(|m| m.collect())
            .map_err(|e| IndexError::Store(StoreError::from(e).to_string()))?;
        for (id, s, e, scale, b) in rows {
            let v: Vec<i8> = b.into_iter().map(|x| x as i8).collect();
            self.push(id, item_id, s, e, &v, scale as f32)?;
        }
        Ok(())
    }

    /// Tombstone an item's rows (deleted item, or before re-embedding).
    pub fn remove_item(&mut self, item_id: i64) {
        if let Some(ix) = self.by_item.remove(&item_id) {
            for i in ix {
                if !self.rows[i].dead {
                    self.rows[i].dead = true;
                    self.live -= 1;
                }
            }
        }
    }

    /// Drop tombstoned rows (on idle).
    pub fn compact(&mut self) {
        let mut n = Self::empty(&self.model, self.dims);
        for (i, r) in self.rows.iter().enumerate() {
            if !r.dead {
                let v = &self.data[i * self.dims..(i + 1) * self.dims];
                let _ = n.push(r.chunk_id, r.item_id, r.start_ch, r.end_ch, v, self.scales[i]);
            }
        }
        *self = n;
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Best chunk per item, top `k` items. `allow(item_id)` filters before ranking.
    pub fn search(&self, q: &[f32], k: usize, allow: &dyn Fn(i64) -> bool) -> Vec<VecHit> {
        if q.len() != self.dims || k == 0 {
            return Vec::new();
        }
        let mut best: HashMap<i64, VecHit> = HashMap::new();
        for (i, r) in self.rows.iter().enumerate() {
            if r.dead || !allow(r.item_id) {
                continue;
            }
            let v = &self.data[i * self.dims..(i + 1) * self.dims];
            let dot: f32 = v.iter().zip(q).map(|(a, b)| *a as f32 * b).sum::<f32>() * self.scales[i];
            let e = best.entry(r.item_id).or_insert(VecHit {
                item_id: r.item_id,
                chunk_id: r.chunk_id,
                start_ch: r.start_ch,
                end_ch: r.end_ch,
                score: f32::MIN,
            });
            if dot > e.score {
                *e = VecHit {
                    item_id: r.item_id,
                    chunk_id: r.chunk_id,
                    start_ch: r.start_ch,
                    end_ch: r.end_ch,
                    score: dot,
                };
            }
        }
        let mut v: Vec<VecHit> = best.into_values().collect();
        v.sort_by(|a, b| b.score.total_cmp(&a.score).then(b.item_id.cmp(&a.item_id)));
        v.truncate(k);
        v
    }

    /// Compare with the database: same number of live vectors for this model.
    pub fn consistent_with(&self, conn: &Connection) -> Result<bool, IndexError> {
        let n: i64 = conn
            .query_row("SELECT count(*) FROM chunks WHERE model = ?1", [&self.model], |r| {
                r.get(0)
            })
            .map_err(|e| IndexError::Store(StoreError::from(e).to_string()))?;
        Ok(n as usize == self.live)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_vectors_are_refused_and_tombstones_compact() {
        let mut ix = VectorIndex::empty("m", 3);
        assert_eq!(ix.push(1, 10, 0, 5, &[1, 2], 0.1), Err(IndexError::Dims(1, 2, 3)));
        ix.push(1, 10, 0, 5, &[127, 0, 0], 1.0 / 127.0).unwrap();
        ix.push(2, 10, 5, 9, &[0, 127, 0], 1.0 / 127.0).unwrap();
        ix.push(3, 11, 0, 4, &[0, 0, 127], 1.0 / 127.0).unwrap();
        let hits = ix.search(&[0.0, 1.0, 0.0], 5, &|_| true);
        assert_eq!((hits[0].item_id, hits[0].chunk_id, hits[0].start_ch), (10, 2, 5), "best chunk per item");
        assert_eq!(hits.len(), 2);
        assert!(ix.search(&[0.0, 1.0], 5, &|_| true).is_empty(), "wrong query size");
        assert_eq!(ix.search(&[0.0, 1.0, 0.0], 5, &|i| i != 10).len(), 1, "filter applies before ranking");
        ix.remove_item(10);
        assert_eq!(ix.len(), 1);
        ix.compact();
        assert_eq!((ix.len(), ix.rows.len()), (1, 1));
    }
}
