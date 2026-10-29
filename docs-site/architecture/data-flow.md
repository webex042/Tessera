# Data Flow

This document details how screen events flow through the capture rungs, change detection, session aggregation, redaction, database persistence, and extraction.

---

## End-to-End Data Processing Pipeline

```mermaid
flowchart TD
    subgraph CAPTURE["1. Acquisition & Normalization"]
        WIN_EVENT["WinEvent Hook: Focus or Title Change"]
        GATE_CHECK{"Gatekeeper: Paused, Locked or Fullscreen?"}
        BLOCK_CHECK{"Blocklist: Blacklisted App, URL or Incognito?"}
        
        RUNG1["Rung 1: UI Automation (285ms Wall-Clock Cap)"]
        RUNG2["Rung 2: WGC + WinRT OCR (In-Memory Bitmap)"]
        RUNG3["Rung 3: Multimodal Crop (Downscale <= 1280px)"]
        
        SIMHASH_CHECK{"SimHash / dHash Hamming Distance <= 3?"}
        DIFF_EXTRACT["Line-Level Delta Extraction"]
    end

    subgraph SESSION["2. Session Aggregation (In RAM)"]
        SESS_BUFFER["Window Session Buffer (Max 6,000 Chars)"]
        FLUSH_COND{"Flush Trigger: 30s Inactive, 5m Active or 6k Chars?"}
    end

    subgraph STORAGE["3. Safety & Persistence"]
        REDACT_ENGINE["PII Masking Engine: Typed Tokens & Pseudonyms"]
        SQL_WRITE["Atomic Transaction: items + sources Tables"]
        VEC_INGEST["Local Embedding (multilingual-e5-small)"]
        VEC_WRITE[("Encrypted VectorDb: Vectors, Chunks & Metadata")]
        FTS_SYNC["SQLite FTS5 Full-Text Index Triggers"]
    end

    subgraph PROCESSING["4. Ingestion & Extraction"]
        JOB_SCHED["Enqueue Extraction Job (Priority P1)"]
        LLM_CALL["llama-server Execution (JSON Schema Grammar)"]
        FACT_VAL["Deterministic Code Validation (Evidence Match Dice >= 0.9)"]
        REDACT_PASS2["Second Redaction Pass"]
        FACTS_WRITE["Atomic Write: facts + evidence Tables"]
    end

    subgraph RETRIEVAL["5. Semantic Retrieval & Ask"]
        QUERY_IN["User Query / Search"]
        QUERY_MASK["Query PII Sanitization"]
        HYBRID_SEARCH["Hybrid Retrieval: VectorDb Cosine + FTS5 + RRF"]
        SNIPPET_GEN["Verified Snippets Assembly"]
    end

    %% Flow
    WIN_EVENT --> GATE_CHECK
    GATE_CHECK -- "No" --> BLOCK_CHECK
    GATE_CHECK -- "Yes" --> DROP_DISCARD["Discard Event"]
    BLOCK_CHECK -- "Allowed" --> RUNG1
    BLOCK_CHECK -- "Blocked" --> DROP_DISCARD
    
    RUNG1 -.-> |"Timeout / Empty"| RUNG2
    RUNG2 -.-> |"Quality < 0.55"| RUNG3
    
    RUNG1 & RUNG2 & RUNG3 --> SIMHASH_CHECK
    SIMHASH_CHECK -- "Unchanged" --> EXTEND_TS["Extend Session Timestamp"]
    SIMHASH_CHECK -- "Changed" --> DIFF_EXTRACT
    
    DIFF_EXTRACT --> SESS_BUFFER
    SESS_BUFFER --> FLUSH_COND
    FLUSH_COND -- "Flush" --> REDACT_ENGINE
    
    REDACT_ENGINE --> SQL_WRITE
    REDACT_ENGINE --> VEC_INGEST
    VEC_INGEST --> VEC_WRITE
    SQL_WRITE --> FTS_SYNC
    SQL_WRITE --> JOB_SCHED
    
    JOB_SCHED --> LLM_CALL
    LLM_CALL --> FACT_VAL
    FACT_VAL --> REDACT_PASS2
    REDACT_PASS2 --> FACTS_WRITE

    QUERY_IN --> QUERY_MASK
    QUERY_MASK --> HYBRID_SEARCH
    VEC_WRITE -.-> HYBRID_SEARCH
    FTS_SYNC -.-> HYBRID_SEARCH
    HYBRID_SEARCH --> SNIPPET_GEN
```

---

## Detailed Pipeline Stages

### 1. Event Ingestion & Gates
- **WinEvent Hooks:** The host thread listens for `EVENT_SYSTEM_FOREGROUND` and `EVENT_OBJECT_NAMECHANGE`.
- **State Gates:** If the screen is locked, fullscreen presentation is active, or the user has toggled **Pause**, the event is dropped immediately.
- **Blocklist Evaluation:** Applications matching known process names (`1password.exe`, `bitwarden.exe`) or window titles indicating banking portals are skipped. If a browser window's private/incognito state cannot be determined, it defaults to blocked.

### 2. The Acquisition Ladder
- **Rung 1 (UIA):** Attempts a structured tree walk. Skips controls with `IsPassword = true`. Aborted at exactly 285ms to prevent hung applications from blocking the pipeline.
- **Rung 2 (WGC + OCR):** Grabs a frame to a BGRA8 `SoftwareBitmap` in RAM. Password control rectangles are blacked out before running the WinRT `OcrEngine`.
- **Rung 3 (VLM):** Invoked if OCR quality score $< 0.55$. Downsamples the window crop to $\le 1280\text{ px}$ and passes it in memory to the multimodal model.

### 3. Change Detection & Session Buffering
- **SimHash:** Computes a 64-bit SimHash over word 3-shingles. If the Hamming distance is $\le 3$, text is treated as unchanged.
- **Window Sessions:** Groups interactions by `(exe, HWND, URL/title)`. Buffers text in memory until focus leaves for $> 30\text{s}$, continuous use reaches 5 minutes, or the buffer exceeds 6,000 characters.

### 4. PII Masking & Persistent Storage
- **Pre-Embedding & Pre-Storage Scrubbing:** Screen text is processed through `tsr_guard::redact::mask_pii`. Personal names (with deterministic document-scoped pseudonyms `[PERSON_01]`), contact details (`[EMAIL]`, `[PHONE]`), national IDs (`[AADHAAR]`, `[PAN]`), financial identifiers (`[CARD]`, `[BANK_ACCOUNT]`, `[IFSC]`), physical addresses (`[ADDRESS]`), dates of birth (`[DATE_OF_BIRTH]`), credentials (`[PASSWORD]`, `[SECRET]`, `[API_KEY]`, `[JWT]`, `[PRIVATE_KEY]`), and OTPs (`[OTP]`) are replaced with typed placeholders.
- **VectorDb Ingestion:** Text chunks are embedded locally using the ONNX Runtime `multilingual-e5-small` model into 384-dimensional vectors and stored encrypted at rest in `tessera_vectors.enc` (`VectorDb`) alongside metadata.
- **Relational Persistence:** Redacted session items and sources are written in atomic transactions to SQLCipher. Database triggers automatically update the SQLite FTS5 search index.

### 5. Structured Extraction & Evidence Verification
- **Model Ingestion:** An extraction job is enqueued in the P1 priority scheduler.
- **Evidence Verification:** Model output must match a strict JSON schema. Every extracted fact must supply an evidence quote that matches a substring ($Dice \ge 0.9$) within the original captured screen item. Fabricated facts are discarded.
- **Second Redaction Pass:** A second redaction pass is executed over model outputs before writing to `facts` and `evidence`.

### 6. Semantic Retrieval & Ask with Sources
- **Query Sanitization:** User queries are sanitized with `mask_pii` before generating embeddings or executing keyword searches.
- **Hybrid Retrieval:** Fuses full-text keyword retrieval (FTS5 BM25) and `VectorDb` cosine similarity search with metadata pre-filtering using Reciprocal Rank Fusion ($k=60$) and an exponential recency decay multiplier.
- **Snippet Assembly:** The top 8 candidate snippets within a 1,500 token budget are presented as grounded context.
- **Deterministic Verification:** Generated sentences cite authenticated snippet IDs and are verified against cited snippet text before display.
