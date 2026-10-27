//! Retrieval for Tessara (plan §10).
//!
//! - [`runtime`] — load a pinned, hash-checked ONNX Runtime so embeddings can never fall back to
//!   another copy of `onnxruntime.dll`.
//! - [`chunk`] — deterministic chunks that are spans of the item text.
//! - [`embed`] — the local embedder (and a SIMULATED one for tests), vector checks, int8.
//! - [`index`] — in-memory vector index rebuilt from the `chunks` table.
//! - [`hybrid`] — keyword + vector + fact search fused with Reciprocal Rank Fusion.
//! - [`ingest`] — the embedding job.

pub mod ask;
pub mod chunk;
pub mod embed;
pub mod hybrid;
pub mod index;
pub mod ingest;
pub mod runtime;
