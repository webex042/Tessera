# Embeddings & Hybrid Retrieval (`tsr-retrieval`)

The `tsr-retrieval` crate implements local vector embedding generation, full-text lexical indexing, and hybrid reciprocal rank fusion.

---

## 1. Pinned ONNX Runtime Execution

To prevent subtle cross-platform numerical discrepancies and avoid security vulnerabilities from uncontrolled system libraries, Tessera binds strictly to a pinned dynamic library:
- **`fastembed`:** Configured with `default-features = false, features = ["ort-load-dynamic"]`.
- **`ort`:** Version pinned to `=2.0.0-rc.15` with `features = ["std", "load-dynamic", "api-31"]`.
- **Dynamic Loader:** `tsr_retrieval::runtime::load_pinned` dynamically loads `third_party/onnxruntime/1.31.0/win-x64/onnxruntime.dll`. The `api-31` flag causes the loader to reject Windows 11's default `System32\onnxruntime.dll` (which is version 1.17).

---

## 2. Chunking & Embeddings

- **Model:** `multilingual-e5-small` in quantized ONNX format (0.46 GiB).
- **Compute Execution:** CPU-only to prevent VRAM contention with the primary vision-language model.
- **Chunk Configuration:**
  - Token size: Up to 512 tokens.
  - Overlap: 64 tokens.
  - Vector dimensionality: 384 dimensions, stored as int8 quantized BLOBs in SQLite.

---

## 3. Hybrid Search Architecture

```mermaid
flowchart LR
    QUERY["User Search Query"] --> FTS["SQLite FTS5 (BM25 Keyword)"]
    QUERY --> EMBED["ONNX Embedder (384-d Vector)"]
    EMBED --> COSINE["In-Memory Cosine Similarity"]
    
    FTS --> CAND_FTS["Top 50 Lexical Hits"]
    COSINE --> CAND_VEC["Top 50 Semantic Hits"]
    
    CAND_FTS & CAND_VEC --> RRF["Reciprocal Rank Fusion (k=60)"]
    RRF --> AGE_DECAY["Recency Multiplier: 1 + 0.15 * 0.5^(age/14d)"]
    AGE_DECAY --> TOP8["Top 8 Snippets (Max 1,500 Tokens)"]
```

### Reciprocal Rank Fusion Formula
Rankings from the lexical and semantic channels are fused:

$$\text{RRF Score}(d) = \sum_{c \in \{\text{FTS5}, \text{Cosine}\}} \frac{1}{k + \text{Rank}_c(d)}$$

where $k = 60$. To ensure fresh work sessions rank higher than month-old notes, an exponential half-life recency multiplier is applied:

$$\text{Final Score}(d) = \text{RRF Score}(d) \times \left(1 + 0.15 \times 0.5^{\frac{\text{Age in Days}}{14}}\right)$$
