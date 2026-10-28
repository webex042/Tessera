# Model Validation & GGUF (`tsr-infer::gguf`)

Tessera inspects and validates model files before loading them into memory. The `tsr-infer::gguf` module implements a zero-copy GGUF header parser that reads file metadata without touching weight tensors.

---

## 1. Zero-Copy Header Inspection

Rather than relying on file extensions or external Python tools, Tessera parses the binary GGUF container header directly:

```rust
// Implemented via tsr-infer::gguf
pub struct GgufMetadata {
    pub version: u32,
    pub architecture: String,
    pub tensor_count: u64,
    pub kv_pairs: HashMap<String, GgufValue>,
    pub context_length: u32,
    pub embedding_length: u32,
    pub block_count: u32,
    pub quantization_type: String,
}
```

### Parsed Validation Attributes
- **GGUF Magic & Version:** Confirms the 4-byte `GGUF` magic identifier (`0x46554747`) and validates format version 2 or 3.
- **Model Architecture:** Confirms compatibility with the Qwen3-VL transformer specification (`qwen2vl` / `qwen3vl`).
- **Context Length:** Reads `qwen2vl.context_length` to allocate the KV cache accurately.
- **Quantization Compatibility:** Inspects tensor types (e.g. `Q4_K_M`, `Q8_0`). If a tensor format is known to be unsupported by a candidate backend (such as `IQ2_XXS` on certain Vulkan drivers), that backend is skipped.

---

## 2. Integrity Verification (`tools/models.lock`)

Every model used by Tessera is pinned to an exact commit revision and cryptographic hash in `tools/models.lock`:

```toml
[[model]]
name = "qwen3-vl-4b-instruct-q4_k_m"
filename = "Qwen3VL-4B-Instruct-Q4_K_M.gguf"
size = 2503274496
sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

[[model]]
name = "mmproj-qwen3-vl-4b-instruct-f16"
filename = "mmproj-Qwen3VL-4B-Instruct-F16.gguf"
size = 838860800
sha256 = "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
```

- **Startup Validation:** Before spawning the inference worker, Tessera computes the SHA-256 digest of local model files.
- **Tampering Refusal:** If a byte has been modified or corrupted, Tessera halts with a `DigestMismatch` error, preventing corrupted execution or security vulnerabilities.
