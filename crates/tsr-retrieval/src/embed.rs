//! Local embedders (plan §10.1).
//!
//! - [`OnnxEmbedder`] — the real one: `multilingual-e5-small` from **local, digest-checked
//!   files** (tools/models.lock, verified by `tsr_infer::models`) on the pinned ONNX Runtime
//!   ([`crate::runtime::load_pinned`]). CPU execution (the pinned runtime is the CPU build), two
//!   intra-op threads. Never touches the network: fastembed is built without its hub client.
//! - [`HashEmbedder`] — SIMULATED: deterministic character-trigram hashing, for tests and for
//!   evals on machines without the model. It captures spelling overlap, not meaning, and is
//!   labelled `sim-hash-v1` so its vectors can never be mistaken for, or mixed with, real ones.
//!
//! Every vector is checked (dimension, finite values) and L2-normalised before it is stored.

use std::path::Path;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Query,
    Passage,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EmbedError {
    #[error("model files: {0}")]
    Files(String),
    #[error("runtime: {0}")]
    Runtime(String),
    #[error("wrong vector size {got}, expected {want}")]
    Dims { got: usize, want: usize },
    #[error("vector has non-finite or all-zero values")]
    BadVector,
    #[error("embedder returned {got} vectors for {want} inputs")]
    Count { got: usize, want: usize },
}

pub trait Embedder: Send + Sync {
    /// Stored with every vector; vectors of different ids are never compared.
    fn model_id(&self) -> &str;
    fn dims(&self) -> usize;
    fn embed_raw(&self, texts: &[String], kind: Kind) -> Result<Vec<Vec<f32>>, EmbedError>;
}

/// Embed and validate: right count, right size, finite, unit length.
pub fn embed(e: &dyn Embedder, texts: &[String], kind: Kind) -> Result<Vec<Vec<f32>>, EmbedError> {
    let v = e.embed_raw(texts, kind)?;
    if v.len() != texts.len() {
        return Err(EmbedError::Count {
            got: v.len(),
            want: texts.len(),
        });
    }
    v.into_iter().map(|x| check(x, e.dims())).collect()
}

pub fn check(mut v: Vec<f32>, dims: usize) -> Result<Vec<f32>, EmbedError> {
    if v.len() != dims {
        return Err(EmbedError::Dims {
            got: v.len(),
            want: dims,
        });
    }
    if v.iter().any(|x| !x.is_finite()) {
        return Err(EmbedError::BadVector);
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n < 1e-6 {
        return Err(EmbedError::BadVector);
    }
    v.iter_mut().for_each(|x| *x /= n);
    Ok(v)
}

/// int8 quantisation with one scale per vector (plan §9.4).
pub fn quantize(v: &[f32]) -> (Vec<i8>, f32) {
    let m = v.iter().fold(0f32, |a, x| a.max(x.abs())).max(1e-12);
    let scale = m / 127.0;
    (
        v.iter()
            .map(|x| (x / scale).round().clamp(-127.0, 127.0) as i8)
            .collect(),
        scale,
    )
}

// ---------------------------------------------------------------------------------------------

/// SIMULATED embedder (see module docs).
pub struct HashEmbedder {
    pub dims: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        HashEmbedder { dims: 384 }
    }
}

fn fnv(b: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

impl Embedder for HashEmbedder {
    fn model_id(&self) -> &str {
        "sim-hash-v1"
    }
    fn dims(&self) -> usize {
        self.dims
    }
    fn embed_raw(&self, texts: &[String], _: Kind) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0f32; self.dims];
                for w in t
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.len() > 1)
                {
                    let padded = format!("#{w}#");
                    let b = padded.as_bytes();
                    for g in b.windows(3.min(b.len())) {
                        let h = fnv(g);
                        let i = (h % self.dims as u64) as usize;
                        v[i] += if h >> 63 == 0 { 1.0 } else { -1.0 };
                    }
                    // Whole-word feature, weighted higher.
                    let h = fnv(w.as_bytes());
                    v[(h % self.dims as u64) as usize] += if h >> 63 == 0 { 2.0 } else { -2.0 };
                }
                if v.iter().all(|x| *x == 0.0) {
                    v[0] = 1.0;
                }
                v
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------------------------

/// The real embedder: ONNX model + tokenizer from a local folder, on the pinned runtime.
pub struct OnnxEmbedder {
    id: String,
    dims: usize,
    inner: Mutex<fastembed::TextEmbedding>,
}

impl OnnxEmbedder {
    /// `dir` holds the manifest's files (`onnx/model.onnx`, `onnx/tokenizer.json`, ...). The
    /// caller verifies them first (`tsr_infer::models::inspect_dir` with digests).
    pub fn from_dir(
        dir: &Path,
        model_id: &str,
        dims: usize,
        runtime: &crate::runtime::PinnedRuntime,
    ) -> Result<Self, EmbedError> {
        let model = load_files(dir)?;
        let opts = fastembed::InitOptionsUserDefined::new()
            .with_environment(runtime.environment.clone())
            .with_max_length(512)
            .with_intra_threads(2);
        let te = fastembed::TextEmbedding::try_new_from_user_defined(model, opts)
            .map_err(|e| EmbedError::Runtime(e.to_string().chars().take(120).collect()))?;
        Ok(OnnxEmbedder {
            id: model_id.to_owned(),
            dims,
            inner: Mutex::new(te),
        })
    }
}

/// Read the model folder into memory (no runtime needed; testable without a model).
pub fn load_files(dir: &Path) -> Result<fastembed::UserDefinedEmbeddingModel, EmbedError> {
    let read = |p: &str| std::fs::read(dir.join(p)).map_err(|e| EmbedError::Files(format!("{p}: {:?}", e.kind())));
    Ok(fastembed::UserDefinedEmbeddingModel::new(
        read("onnx/model.onnx")?,
        fastembed::TokenizerFiles {
            tokenizer_file: read("onnx/tokenizer.json")?,
            config_file: read("onnx/config.json")?,
            special_tokens_map_file: read("onnx/special_tokens_map.json")?,
            tokenizer_config_file: read("onnx/tokenizer_config.json")?,
        },
    )
    .with_pooling(fastembed::Pooling::Mean))
}

impl Embedder for OnnxEmbedder {
    fn model_id(&self) -> &str {
        &self.id
    }
    fn dims(&self) -> usize {
        self.dims
    }
    fn embed_raw(&self, texts: &[String], kind: Kind) -> Result<Vec<Vec<f32>>, EmbedError> {
        // E5 expects these prefixes (plan §10.1).
        let prefix = match kind {
            Kind::Query => "query: ",
            Kind::Passage => "passage: ",
        };
        let input: Vec<String> = texts.iter().map(|t| format!("{prefix}{t}")).collect();
        self.inner
            .lock()
            .map_err(|_| EmbedError::Runtime("poisoned".into()))?
            .embed(&input, Some(16))
            .map_err(|e| EmbedError::Runtime(e.to_string().chars().take(120).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Broken(Vec<f32>);
    impl Embedder for Broken {
        fn model_id(&self) -> &str {
            "broken"
        }
        fn dims(&self) -> usize {
            4
        }
        fn embed_raw(&self, t: &[String], _: Kind) -> Result<Vec<Vec<f32>>, EmbedError> {
            Ok(t.iter().map(|_| self.0.clone()).collect())
        }
    }

    #[test]
    fn invalid_vectors_are_rejected() {
        let t = vec!["x".to_string()];
        assert_eq!(
            embed(&Broken(vec![1.0, 2.0]), &t, Kind::Query),
            Err(EmbedError::Dims { got: 2, want: 4 })
        );
        assert_eq!(
            embed(&Broken(vec![1.0, f32::NAN, 0.0, 0.0]), &t, Kind::Query),
            Err(EmbedError::BadVector)
        );
        assert_eq!(
            embed(&Broken(vec![0.0; 4]), &t, Kind::Query),
            Err(EmbedError::BadVector)
        );
        let ok = embed(&Broken(vec![3.0, 4.0, 0.0, 0.0]), &t, Kind::Query).unwrap();
        assert!((ok[0][0] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn quantisation_round_trips_closely() {
        let v = check(vec![0.1, -0.5, 0.3, 0.8], 4).unwrap();
        let (q, s) = quantize(&v);
        for (a, b) in v.iter().zip(q.iter().map(|x| *x as f32 * s)) {
            assert!((a - b).abs() < 0.01);
        }
    }

    #[test]
    fn hash_embedder_is_deterministic_and_spelling_sensitive() {
        let e = HashEmbedder::default();
        let v = embed(
            &e,
            &[
                "budget sheet travel".into(),
                "budget sheet travel".into(),
                "dentist appointment".into(),
            ],
            Kind::Passage,
        )
        .unwrap();
        assert_eq!(v[0], v[1]);
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let q = embed(&e, &["travel budget".into()], Kind::Query).unwrap();
        assert!(dot(&q[0], &v[0]) > dot(&q[0], &v[2]));
    }

    #[test]
    fn real_embedder_reports_missing_files_without_touching_the_network() {
        let d = tempfile::tempdir().unwrap();
        let e = load_files(d.path()).err().unwrap();
        assert!(matches!(e, EmbedError::Files(ref s) if s.starts_with("onnx/model.onnx")));
    }
}
