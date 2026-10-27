//! Hybrid search: keyword (FTS5 on items and facts) + vectors, fused with Reciprocal Rank Fusion
//! (plan §10.3).
//!
//! - Each channel returns up to `per_channel` items, ranked 1..n. Fact hits count for the items
//!   their evidence points into.
//! - `score = Σ 1 / (rrf_k + rank)` over channels, then × `(1 + recency_weight · 0.5^(age / half_life))`
//!   (the plan's recency prior applied as a multiplier, so it can reorder near-ties but not
//!   outweigh a channel). Ties break by newer item, then lower id — fully deterministic.
//! - Filters run **before** fusion and apply to both channels identically: the source must pass
//!   the *current* blocklist, and the item must not mention a suppressed identity.
//! - If the vector channel is unavailable (no model, index empty), keyword results are returned
//!   alone and the diagnostics say so. Diagnostics are counts only.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, params};
use tsr_guard::blocklist::{Blocklist, PrivateState, WindowFacts};
use tsr_store::StoreError;

use crate::embed::{Embedder, Kind, embed};
use crate::index::VectorIndex;

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub k: usize,
    pub per_channel: usize,
    pub rrf_k: f64,
    pub recency_weight: f64,
    pub half_life_days: f64,
    pub now_ms: i64,
    pub use_lexical: bool,
    pub use_vector: bool,
    pub use_facts: bool,
}

impl Opts {
    pub fn new(now_ms: i64) -> Self {
        Opts {
            k: 8,
            per_channel: 50,
            rrf_k: 60.0,
            recency_weight: 0.15,
            half_life_days: 14.0,
            now_ms,
            use_lexical: true,
            use_vector: true,
            use_facts: true,
        }
    }
}

pub struct Filter<'a> {
    pub blocklist: Option<&'a Blocklist>,
    /// `true` if text mentions a suppressed identity (tsr-pipeline's `Suppression::mentions`).
    pub suppressed: Option<&'a dyn Fn(&str) -> bool>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Hit {
    pub item_id: i64,
    pub score: f64,
    pub lexical_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub fact_rank: Option<usize>,
    /// Best-matching chunk span in the item, from the vector channel.
    pub span: Option<(i64, i64)>,
    pub started_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostics {
    pub lexical: usize,
    pub vector: usize,
    pub facts: usize,
    pub blocked: usize,
    pub suppressed: usize,
    pub vector_unavailable: bool,
}

struct ItemMeta {
    started_at: i64,
    allowed: bool,
}

fn meta(conn: &Connection, id: i64, f: &Filter<'_>, d: &mut Diagnostics) -> Result<ItemMeta, StoreError> {
    let (started_at, text, exe, title, url): (i64, String, String, Option<String>, Option<String>) = conn.query_row(
        "SELECT items.started_at, items.text, sources.app_exe, sources.window_title, sources.url
         FROM items JOIN sources ON sources.id = items.source_id WHERE items.id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    let mut allowed = true;
    if let Some(bl) = f.blocklist {
        let dec = bl.check(&WindowFacts {
            exe: &exe,
            exe_path: None,
            title: title.as_deref().unwrap_or(""),
            url: url.as_deref(),
            private: PrivateState::NotPrivate,
        });
        if dec.is_blocked() {
            d.blocked += 1;
            allowed = false;
        }
    }
    if allowed
        && let Some(s) = f.suppressed
        && s(&text)
    {
        d.suppressed += 1;
        allowed = false;
    }
    Ok(ItemMeta { started_at, allowed })
}

pub fn search(
    conn: &Connection,
    index: Option<&VectorIndex>,
    embedder: Option<&dyn Embedder>,
    query: &str,
    filter: &Filter<'_>,
    o: &Opts,
) -> Result<(Vec<Hit>, Diagnostics), StoreError> {
    let mut d = Diagnostics::default();
    let mut metas: HashMap<i64, ItemMeta> = HashMap::new();
    let mut allowed = |conn: &Connection, id: i64, d: &mut Diagnostics| -> Result<bool, StoreError> {
        if let Some(m) = metas.get(&id) {
            return Ok(m.allowed);
        }
        let m = meta(conn, id, filter, d)?;
        let a = m.allowed;
        metas.insert(id, m);
        Ok(a)
    };

    // Keyword channel over items.
    let mut lexical: Vec<i64> = Vec::new();
    if o.use_lexical {
        for h in tsr_store::items::search_any(conn, query, (o.per_channel * 2) as u32)? {
            if lexical.len() < o.per_channel && !lexical.contains(&h.item_id) && allowed(conn, h.item_id, &mut d)? {
                lexical.push(h.item_id);
            }
        }
    }
    d.lexical = lexical.len();

    // Facts channel: rank of the best fact per evidence item.
    let mut fact_items: Vec<i64> = Vec::new();
    if o.use_facts {
        for (fid, _) in tsr_store::facts::search(conn, query, (o.per_channel * 2) as u32)? {
            for e in tsr_store::facts::evidence(conn, fid)? {
                if fact_items.len() < o.per_channel
                    && !fact_items.contains(&e.item_id)
                    && allowed(conn, e.item_id, &mut d)?
                {
                    fact_items.push(e.item_id);
                }
            }
        }
    }
    d.facts = fact_items.len();

    // Vector channel.
    let mut vector: Vec<(i64, (i64, i64))> = Vec::new();
    match (index, embedder) {
        (Some(ix), Some(e)) if o.use_vector && !ix.is_empty() && ix.model == e.model_id() => {
            match embed(e, &[query.to_owned()], Kind::Query) {
                Ok(q) => {
                    // Over-fetch, then filter: the filter needs the database.
                    for h in ix.search(&q[0], o.per_channel * 3, &|_| true) {
                        if vector.len() < o.per_channel && allowed(conn, h.item_id, &mut d)? {
                            vector.push((h.item_id, (h.start_ch, h.end_ch)));
                        }
                    }
                }
                Err(_) => d.vector_unavailable = true,
            }
        }
        _ => d.vector_unavailable = o.use_vector,
    }
    d.vector = vector.len();

    // Reciprocal Rank Fusion.
    let mut fused: BTreeMap<i64, Hit> = BTreeMap::new();
    let mut add = |id: i64, rank: usize, ch: u8, span: Option<(i64, i64)>| {
        let h = fused.entry(id).or_insert(Hit {
            item_id: id,
            score: 0.0,
            lexical_rank: None,
            vector_rank: None,
            fact_rank: None,
            span: None,
            started_at: 0,
        });
        h.score += 1.0 / (o.rrf_k + rank as f64);
        match ch {
            0 => h.lexical_rank = Some(rank),
            1 => {
                h.vector_rank = Some(rank);
                h.span = span;
            }
            _ => h.fact_rank = Some(rank),
        }
    };
    for (i, id) in lexical.iter().enumerate() {
        add(*id, i + 1, 0, None);
    }
    for (i, (id, span)) in vector.iter().enumerate() {
        add(*id, i + 1, 1, Some(*span));
    }
    for (i, id) in fact_items.iter().enumerate() {
        add(*id, i + 1, 2, None);
    }
    let mut hits: Vec<Hit> = fused
        .into_values()
        .map(|mut h| {
            let started = metas.get(&h.item_id).map(|m| m.started_at).unwrap_or(0);
            h.started_at = started;
            let age_days = ((o.now_ms - started).max(0) as f64) / 86_400_000.0;
            h.score *= 1.0 + o.recency_weight * 0.5f64.powf(age_days / o.half_life_days);
            h
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.started_at.cmp(&a.started_at))
            .then(a.item_id.cmp(&b.item_id))
    });
    hits.truncate(o.k);
    Ok((hits, d))
}
