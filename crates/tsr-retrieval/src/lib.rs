//! Retrieval for Tessara (plan §10).
//!
//! Stage 0 scope: load a pinned, hash-checked ONNX Runtime so embeddings can never
//! fall back to another copy of `onnxruntime.dll` (see [`runtime`]).

pub mod runtime;
