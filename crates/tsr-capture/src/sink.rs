//! The store writer (plan §4.1 T4, §8.1 steps 1–3, §9.3).
//!
//! One thread owns the SQLCipher connection. Finished sessions arrive over a channel and are
//! written as a `sources` upsert plus an `items` insert in one transaction (the FTS index follows
//! by trigger); block decisions become content-free audit events. Embedding and extraction jobs
//! (§8.1 steps 4–5) attach here in Stage 2.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use rusqlite::Connection;
use tsr_guard::blocklist::Decision;
use tsr_store::items::{Acquisition, NewItem, NewSource, SourceKind, insert_item, upsert_source};

use crate::engine::Sink;
use crate::session::{Flushed, Rung};

enum Op {
    Item(Flushed),
    Blocked(Decision, i64),
}

#[derive(Debug, Default)]
pub struct WriterStats {
    pub items: AtomicU64,
    pub blocked: AtomicU64,
    pub errors: AtomicU64,
}

pub struct StoreSink {
    tx: Option<Sender<Op>>,
    join: Option<JoinHandle<Connection>>,
    pub stats: Arc<WriterStats>,
}

fn write_item(conn: &mut Connection, f: &Flushed) -> Result<(), tsr_store::StoreError> {
    let tx = conn.transaction()?;
    let url = f.source.url.as_ref().map(|u| u.expose().as_str());
    let source_id = upsert_source(
        &tx,
        &NewSource {
            kind: if url.is_some() {
                SourceKind::Url
            } else {
                SourceKind::App
            },
            app_exe: &f.source.exe,
            app_name: f.source.app_name.as_deref(),
            window_title: Some(f.source.title.expose()),
            url,
            file_path: None,
            seen_at: f.ended_at,
        },
    )?;
    insert_item(
        &tx,
        &NewItem {
            source_id,
            started_at: f.started_at,
            ended_at: f.ended_at,
            acquisition: match f.rung {
                Rung::Uia => Acquisition::Uia,
                Rung::Ocr => Acquisition::Ocr,
                Rung::Vlm => Acquisition::Vlm,
            },
            text: &f.text,
            simhash: f.simhash,
            lang: None,
            redaction_count: f.redactions,
        },
    )?;
    tx.commit()?;
    Ok(())
}

impl StoreSink {
    /// Start the writer on `conn` (already opened and migrated).
    pub fn spawn(mut conn: Connection) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Op>();
        let stats = Arc::new(WriterStats::default());
        let st = stats.clone();
        let join = std::thread::Builder::new()
            .name("tsr-store-writer".into())
            .spawn(move || {
                for op in rx {
                    let r = match &op {
                        Op::Item(f) => write_item(&mut conn, f).map(|_| st.items.fetch_add(1, Ordering::Relaxed)),
                        Op::Blocked(d, at) => tsr_store::blocklist::record_block(&conn, d, *at)
                            .map(|_| st.blocked.fetch_add(1, Ordering::Relaxed)),
                    };
                    if let Err(e) = r {
                        st.errors.fetch_add(1, Ordering::Relaxed);
                        tracing::error!(error = %e, "store write failed");
                    }
                }
                conn
            })?;
        Ok(StoreSink {
            tx: Some(tx),
            join: Some(join),
            stats,
        })
    }

    /// Stop after all queued writes are done; returns the connection.
    pub fn finish(mut self) -> Connection {
        drop(self.tx.take());
        self.join.take().expect("writer").join().expect("writer thread")
    }
}

impl Sink for StoreSink {
    fn store(&mut self, item: Flushed) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Op::Item(item));
        }
    }

    fn blocked(&mut self, decision: &Decision, at_ms: i64) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Op::Blocked(decision.clone(), at_ms));
        }
    }
}

impl Drop for StoreSink {
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
