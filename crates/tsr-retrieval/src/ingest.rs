//! The embedding job (plan §8.1 step 4): chunk an item, embed the chunks, store int8 vectors in
//! one transaction, update the in-memory index. Idempotent: a re-run replaces the item's vectors
//! for that model. Interrupted runs leave the previous state (or nothing) and are re-queued from
//! the `jobs` table.

use rusqlite::Connection;
use tsr_store::chunks::{self, NewChunk};
use tsr_store::items::get_item;
use tsr_store::jobs::{self, JobState};

use crate::chunk::{CHUNKER_VER, chunk};
use crate::embed::{EmbedError, Embedder, Kind, embed, quantize};
use crate::index::{IndexError, VectorIndex};

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("store: {0}")]
    Store(#[from] tsr_store::StoreError),
    #[error("embedder: {0}")]
    Embed(#[from] EmbedError),
    #[error("index: {0}")]
    Index(#[from] IndexError),
    #[error("index model {index} differs from embedder {embedder}")]
    ModelMismatch { index: String, embedder: String },
}

impl From<rusqlite::Error> for IngestError {
    fn from(e: rusqlite::Error) -> Self {
        IngestError::Store(e.into())
    }
}

/// Returns the number of chunks stored.
pub fn embed_item(
    conn: &mut Connection,
    item_id: i64,
    e: &dyn Embedder,
    index: Option<&mut VectorIndex>,
    now: i64,
) -> Result<usize, IngestError> {
    if let Some(ix) = &index
        && ix.model != e.model_id()
    {
        return Err(IngestError::ModelMismatch {
            index: ix.model.clone(),
            embedder: e.model_id().to_owned(),
        });
    }
    let (job, _) = jobs::enqueue(conn, "embed", item_id, now)?;
    jobs::start(conn, job, now)?;
    let Some(item) = get_item(conn, item_id)? else {
        jobs::finish(conn, job, JobState::Skipped, Some("item_missing"), now)?;
        return Ok(0);
    };
    let raw_text = item.text.expose();
    // Mandatory privacy boundary: mask PII before text is chunked or embedded
    let masked_text = tsr_guard::redact::mask_pii(raw_text).text;
    let spans = chunk(&masked_text);
    let texts: Vec<String> = spans
        .iter()
        .map(|c| masked_text.chars().skip(c.start_ch).take(c.end_ch - c.start_ch).collect())
        .collect();
    let vecs = match embed(e, &texts, Kind::Passage) {
        Ok(v) => v,
        Err(err) => {
            jobs::finish(conn, job, JobState::Queued, Some("embed_error"), now)?;
            return Err(err.into());
        }
    };
    let rows: Vec<NewChunk> = spans
        .iter()
        .zip(vecs.iter())
        .map(|(c, v)| {
            let (q, scale) = quantize(v);
            NewChunk {
                ord: c.ord,
                start_ch: c.start_ch as i64,
                end_ch: c.end_ch as i64,
                scale,
                vec: q,
            }
        })
        .collect();
    let tx = conn.transaction()?;
    chunks::replace_for_item(&tx, item_id, e.model_id(), CHUNKER_VER, &rows)?;
    jobs::finish(&tx, job, JobState::Done, None, now)?;
    tx.commit()?;
    if let Some(ix) = index {
        ix.refresh_item(conn, item_id)?;
    }
    Ok(rows.len())
}

/// Embed an item into the local encrypted VectorDb, guaranteeing PII masking before embedding.
pub fn embed_item_to_vectordb(
    conn: &mut Connection,
    vdb: &mut tsr_store::vectordb::VectorDb,
    item_id: i64,
    e: &dyn Embedder,
    index: Option<&mut VectorIndex>,
    now: i64,
) -> Result<usize, IngestError> {
    if let Some(ix) = &index
        && ix.model != e.model_id()
    {
        return Err(IngestError::ModelMismatch {
            index: ix.model.clone(),
            embedder: e.model_id().to_owned(),
        });
    }
    let (job, _) = jobs::enqueue(conn, "embed", item_id, now)?;
    jobs::start(conn, job, now)?;
    let Some(item) = get_item(conn, item_id)? else {
        jobs::finish(conn, job, JobState::Skipped, Some("item_missing"), now)?;
        return Ok(0);
    };
    let src = tsr_store::items::get_source(conn, item.source_id)?;
    let raw_text = item.text.expose();
    // Mandatory privacy boundary: mask PII before text is chunked or embedded
    let masked_text = tsr_guard::redact::mask_pii(raw_text).text;
    let spans = chunk(&masked_text);
    let texts: Vec<String> = spans
        .iter()
        .map(|c| masked_text.chars().skip(c.start_ch).take(c.end_ch - c.start_ch).collect())
        .collect();
    let vecs = match embed(e, &texts, Kind::Passage) {
        Ok(v) => v,
        Err(err) => {
            jobs::finish(conn, job, JobState::Queued, Some("embed_error"), now)?;
            return Err(err.into());
        }
    };

    vdb.delete_by_item(item_id);

    let mut count = 0;
    for (c, v) in spans.iter().zip(vecs.iter()) {
        let (q, scale) = quantize(v);
        let chunk_text: String = masked_text
            .chars()
            .skip(c.start_ch)
            .take(c.end_ch - c.start_ch)
            .collect();

        let rec = tsr_store::vectordb::VectorRecord {
            id: format!("chunk-{item_id}-{}", c.ord),
            model: e.model_id().to_owned(),
            dims: e.dims(),
            scale,
            vec_int8: q,
            vec_f32: v.clone(),
            text: chunk_text,
            metadata: tsr_store::vectordb::VectorMetadata {
                item_id,
                source_id: Some(item.source_id),
                app_name: src.as_ref().and_then(|s| s.app_name.clone().or_else(|| Some(s.app_exe.clone()))),
                window_title: src.as_ref().and_then(|s| s.window_title.as_ref().map(|t| t.expose().clone())),
                url: src.as_ref().and_then(|s| s.url.as_ref().map(|u| u.expose().clone())),
                memory_type: "passage".to_string(),
                timestamp_ms: item.started_at,
                ord: c.ord,
                start_ch: c.start_ch as i64,
                end_ch: c.end_ch as i64,
                provenance: Some(format!("item:{item_id}")),
            },
        };
        vdb.insert(rec)?;
        count += 1;
    }
    vdb.save()?;

    jobs::finish(conn, job, JobState::Done, None, now)?;
    if let Some(ix) = index {
        ix.refresh_item_from_vectordb(vdb, item_id)?;
    }
    Ok(count)
}
