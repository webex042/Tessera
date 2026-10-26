//! Minimal GGUF header reader (model format validation, plan D3 / §7.1).
//!
//! Reads only the header, metadata and tensor descriptors — never tensor data — so checking a
//! multi-GB model costs a few milliseconds. Every length read from the file is bounded, because a
//! model file is an untrusted input until its SHA-256 has been checked.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

const MAGIC: &[u8; 4] = b"GGUF";
const MAX_STRING: u64 = 1 << 20;
const MAX_TENSORS: u64 = 1 << 20;
const MAX_KV: u64 = 1 << 16;
const MAX_ARRAY: u64 = 1 << 26;
const MAX_DIMS: u32 = 8;

/// ggml tensor element types (numbering from ggml.h).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum GgmlType {
    F32,
    F16,
    Q4_0,
    Q4_1,
    Q5_0,
    Q5_1,
    Q8_0,
    Q8_1,
    Q2K,
    Q3K,
    Q4K,
    Q5K,
    Q6K,
    Q8K,
    IQ2XXS,
    IQ2XS,
    IQ3XXS,
    IQ1S,
    IQ4NL,
    IQ3S,
    IQ2S,
    IQ4XS,
    I8,
    I16,
    I32,
    I64,
    F64,
    IQ1M,
    BF16,
    TQ1_0,
    TQ2_0,
    MXFP4,
    Unknown(u32),
}

impl GgmlType {
    pub fn from_id(id: u32) -> Self {
        use GgmlType::*;
        match id {
            0 => F32,
            1 => F16,
            2 => Q4_0,
            3 => Q4_1,
            6 => Q5_0,
            7 => Q5_1,
            8 => Q8_0,
            9 => Q8_1,
            10 => Q2K,
            11 => Q3K,
            12 => Q4K,
            13 => Q5K,
            14 => Q6K,
            15 => Q8K,
            16 => IQ2XXS,
            17 => IQ2XS,
            18 => IQ3XXS,
            19 => IQ1S,
            20 => IQ4NL,
            21 => IQ3S,
            22 => IQ2S,
            23 => IQ4XS,
            24 => I8,
            25 => I16,
            26 => I32,
            27 => I64,
            28 => F64,
            29 => IQ1M,
            30 => BF16,
            34 => TQ1_0,
            35 => TQ2_0,
            39 => MXFP4,
            other => Unknown(other),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GgufError {
    #[error("not a GGUF file")]
    BadMagic,
    #[error("unsupported GGUF version {0} (need 2 or 3)")]
    UnsupportedVersion(u32),
    #[error("GGUF header field out of bounds: {0}")]
    OutOfBounds(&'static str),
    #[error("unknown GGUF value type {0}")]
    BadValueType(u32),
    #[error("GGUF file truncated or unreadable")]
    Io(#[from] std::io::Error),
}

/// Metadata values we keep. Arrays are skipped (tokenizer vocabularies are large).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum MetaValue {
    U64(u64),
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    Array { elem_type: u32, len: u64 },
}

#[derive(Clone, Debug, Serialize)]
pub struct GgufInfo {
    pub version: u32,
    pub architecture: Option<String>,
    pub file_size: u64,
    pub tensor_count: u64,
    /// Tensor count per element type, e.g. Q4_K_M files contain Q4K, Q6K and F32 tensors.
    pub tensor_types: BTreeMap<GgmlType, u64>,
    pub metadata: BTreeMap<String, MetaValue>,
}

impl GgufInfo {
    fn meta_u64(&self, key: &str) -> Option<u64> {
        match self.metadata.get(key)? {
            MetaValue::U64(v) => Some(*v),
            MetaValue::I64(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }

    fn arch_u64(&self, suffix: &str) -> Option<u64> {
        self.meta_u64(&format!("{}.{suffix}", self.architecture.as_deref()?))
    }

    pub fn block_count(&self) -> Option<u64> {
        self.arch_u64("block_count")
    }

    /// KV-cache size in MiB for `ctx` tokens with `bytes_per_elem` per element (f16 = 2.0,
    /// q8_0 ≈ 1.0625). `None` if the attention shape is not in the metadata.
    pub fn kv_cache_mib(&self, ctx: u64, bytes_per_elem: f64) -> Option<u64> {
        let layers = self.block_count()?;
        let heads_kv = self
            .arch_u64("attention.head_count_kv")
            .or_else(|| self.arch_u64("attention.head_count"))?;
        let head_dim = self
            .arch_u64("attention.key_length")
            .or_else(|| Some(self.arch_u64("embedding_length")? / self.arch_u64("attention.head_count")?.max(1)))?;
        let bytes = 2.0 * layers as f64 * ctx as f64 * heads_kv as f64 * head_dim as f64 * bytes_per_elem;
        Some((bytes / (1u64 << 20) as f64).ceil() as u64)
    }
}

struct R<T: Read + Seek> {
    inner: T,
}

impl<T: Read + Seek> R<T> {
    fn u32(&mut self) -> Result<u32, GgufError> {
        let mut b = [0u8; 4];
        self.inner.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn u64(&mut self) -> Result<u64, GgufError> {
        let mut b = [0u8; 8];
        self.inner.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }
    fn skip(&mut self, n: u64) -> Result<(), GgufError> {
        self.inner.seek(SeekFrom::Current(
            i64::try_from(n).map_err(|_| GgufError::OutOfBounds("skip"))?,
        ))?;
        Ok(())
    }
    fn string(&mut self) -> Result<String, GgufError> {
        let len = self.u64()?;
        if len > MAX_STRING {
            return Err(GgufError::OutOfBounds("string length"));
        }
        let mut b = vec![0u8; len as usize];
        self.inner.read_exact(&mut b)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
    fn scalar_size(t: u32) -> Option<u64> {
        Some(match t {
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4..=6 => 4,
            10..=12 => 8,
            _ => return None,
        })
    }
    fn value(&mut self, t: u32) -> Result<MetaValue, GgufError> {
        let mut b = [0u8; 8];
        Ok(match t {
            0 => {
                self.inner.read_exact(&mut b[..1])?;
                MetaValue::U64(b[0] as u64)
            }
            1 => {
                self.inner.read_exact(&mut b[..1])?;
                MetaValue::I64(b[0] as i8 as i64)
            }
            2 => {
                self.inner.read_exact(&mut b[..2])?;
                MetaValue::U64(u16::from_le_bytes([b[0], b[1]]) as u64)
            }
            3 => {
                self.inner.read_exact(&mut b[..2])?;
                MetaValue::I64(i16::from_le_bytes([b[0], b[1]]) as i64)
            }
            4 => MetaValue::U64(self.u32()? as u64),
            5 => MetaValue::I64(self.u32()? as i32 as i64),
            6 => MetaValue::F64(f32::from_bits(self.u32()?) as f64),
            7 => {
                self.inner.read_exact(&mut b[..1])?;
                MetaValue::Bool(b[0] != 0)
            }
            8 => MetaValue::Str(self.string()?),
            9 => {
                let elem_type = self.u32()?;
                let len = self.u64()?;
                if len > MAX_ARRAY {
                    return Err(GgufError::OutOfBounds("array length"));
                }
                if let Some(sz) = Self::scalar_size(elem_type) {
                    self.skip(sz * len)?;
                } else if elem_type == 8 {
                    for _ in 0..len {
                        let l = self.u64()?;
                        if l > MAX_STRING {
                            return Err(GgufError::OutOfBounds("array string length"));
                        }
                        self.skip(l)?;
                    }
                } else {
                    // Nested arrays are not used by llama.cpp metadata.
                    return Err(GgufError::BadValueType(elem_type));
                }
                MetaValue::Array { elem_type, len }
            }
            10 => MetaValue::U64(self.u64()?),
            11 => MetaValue::I64(self.u64()? as i64),
            12 => MetaValue::F64(f64::from_bits(self.u64()?)),
            other => return Err(GgufError::BadValueType(other)),
        })
    }
}

pub fn read_gguf(path: &Path) -> Result<GgufInfo, GgufError> {
    let file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let mut r = R {
        inner: BufReader::new(file),
    };
    let mut magic = [0u8; 4];
    r.inner.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(GgufError::BadMagic);
    }
    let version = r.u32()?;
    if !(2..=3).contains(&version) {
        return Err(GgufError::UnsupportedVersion(version));
    }
    let tensor_count = r.u64()?;
    let kv_count = r.u64()?;
    if tensor_count > MAX_TENSORS {
        return Err(GgufError::OutOfBounds("tensor count"));
    }
    if kv_count > MAX_KV {
        return Err(GgufError::OutOfBounds("metadata count"));
    }
    let mut metadata = BTreeMap::new();
    for _ in 0..kv_count {
        let key = r.string()?;
        let t = r.u32()?;
        let v = r.value(t)?;
        metadata.insert(key, v);
    }
    let mut tensor_types = BTreeMap::new();
    for _ in 0..tensor_count {
        let name_len = r.u64()?;
        if name_len > MAX_STRING {
            return Err(GgufError::OutOfBounds("tensor name"));
        }
        r.skip(name_len)?;
        let n_dims = r.u32()?;
        if n_dims > MAX_DIMS {
            return Err(GgufError::OutOfBounds("tensor dims"));
        }
        r.skip(8 * n_dims as u64)?;
        let t = GgmlType::from_id(r.u32()?);
        r.skip(8)?; // data offset
        *tensor_types.entry(t).or_insert(0) += 1;
    }
    // Seeking past the end does not fail on every platform; a header that claims more than the
    // file holds is truncated or hostile.
    if r.inner.stream_position()? > file_size {
        return Err(GgufError::Io(std::io::ErrorKind::UnexpectedEof.into()));
    }
    let architecture = match metadata.get("general.architecture") {
        Some(MetaValue::Str(s)) => Some(s.clone()),
        _ => None,
    };
    Ok(GgufInfo {
        version,
        architecture,
        file_size,
        tensor_count,
        tensor_types,
        metadata,
    })
}

/// Test helper: write a header-only GGUF v3 file (SIMULATED model; no tensor data).
#[doc(hidden)]
pub fn write_test_gguf(path: &Path, arch: &str, meta_u32: &[(&str, u32)], tensor_types: &[u32]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let s = |f: &mut std::fs::File, x: &str| -> std::io::Result<()> {
        f.write_all(&(x.len() as u64).to_le_bytes())?;
        f.write_all(x.as_bytes())
    };
    f.write_all(MAGIC)?;
    f.write_all(&3u32.to_le_bytes())?;
    f.write_all(&(tensor_types.len() as u64).to_le_bytes())?;
    f.write_all(&(2 + meta_u32.len() as u64).to_le_bytes())?;
    s(&mut f, "general.architecture")?;
    f.write_all(&8u32.to_le_bytes())?;
    s(&mut f, arch)?;
    // A string array, like a tokenizer vocabulary, to exercise skipping.
    s(&mut f, "tokenizer.ggml.tokens")?;
    f.write_all(&9u32.to_le_bytes())?;
    f.write_all(&8u32.to_le_bytes())?;
    f.write_all(&3u64.to_le_bytes())?;
    for t in ["a", "bb", "ccc"] {
        s(&mut f, t)?;
    }
    for (k, v) in meta_u32 {
        s(&mut f, k)?;
        f.write_all(&4u32.to_le_bytes())?;
        f.write_all(&v.to_le_bytes())?;
    }
    for (i, t) in tensor_types.iter().enumerate() {
        s(&mut f, &format!("blk.{i}.weight"))?;
        f.write_all(&2u32.to_le_bytes())?;
        f.write_all(&4096u64.to_le_bytes())?;
        f.write_all(&4096u64.to_le_bytes())?;
        f.write_all(&t.to_le_bytes())?;
        f.write_all(&0u64.to_le_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_header_metadata_and_tensor_types() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("m.gguf");
        write_test_gguf(
            &p,
            "qwen3vl",
            &[
                ("qwen3vl.block_count", 36),
                ("qwen3vl.attention.head_count", 32),
                ("qwen3vl.attention.head_count_kv", 8),
                ("qwen3vl.attention.key_length", 128),
            ],
            &[12, 12, 14, 0],
        )
        .unwrap();
        let g = read_gguf(&p).unwrap();
        assert_eq!(g.version, 3);
        assert_eq!(g.architecture.as_deref(), Some("qwen3vl"));
        assert_eq!(g.tensor_types.get(&GgmlType::Q4K), Some(&2));
        assert_eq!(g.tensor_types.get(&GgmlType::Q6K), Some(&1));
        assert_eq!(g.block_count(), Some(36));
        // 2 * 36 layers * 8192 ctx * 8 heads * 128 dim * 2 bytes = 1152 MiB (f16).
        assert_eq!(g.kv_cache_mib(8192, 2.0), Some(1152));
        assert!(matches!(
            g.metadata.get("tokenizer.ggml.tokens"),
            Some(MetaValue::Array { len: 3, .. })
        ));
    }

    #[test]
    fn rejects_non_gguf_and_bad_versions() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.bin");
        std::fs::write(&p, b"PK\x03\x04 not a model").unwrap();
        assert!(matches!(read_gguf(&p), Err(GgufError::BadMagic)));
        std::fs::write(&p, [b"GGUF".as_slice(), &1u32.to_le_bytes()].concat()).unwrap();
        assert!(matches!(read_gguf(&p), Err(GgufError::UnsupportedVersion(1))));
    }

    #[test]
    fn rejects_hostile_sizes_and_truncation() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.gguf");
        let mut b = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&u64::MAX.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        std::fs::write(&p, &b).unwrap();
        assert!(matches!(read_gguf(&p), Err(GgufError::OutOfBounds("tensor count"))));

        let ok = d.path().join("ok.gguf");
        write_test_gguf(&ok, "llama", &[], &[2]).unwrap();
        let full = std::fs::read(&ok).unwrap();
        std::fs::write(&p, &full[..full.len() - 5]).unwrap();
        assert!(matches!(read_gguf(&p), Err(GgufError::Io(_))));
    }
}
