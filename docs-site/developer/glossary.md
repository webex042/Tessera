# Glossary

Standard systems, inference, and architectural terminology used across Tessera.

---

### Capture Ladder
A multi-rung text acquisition hierarchy (Rung 1 UI Automation $\rightarrow$ Rung 2 WGC + WinRT OCR $\rightarrow$ Rung 3 Multimodal VLM Crop) that attempts fast, structured, lightweight methods before falling back to compute-heavy vision.

### DPAPI (Data Protection API)
A Windows cryptographic subsystem that allows applications to encrypt data using keys derived from the logged-in user's domain or local machine credentials (`CryptProtectData`).

### GGUF
The binary file format introduced by `llama.cpp` for storing quantized neural network weights and metadata in a single, memory-mappable container.

### Job Object
A Windows operating system kernel object that groups processes together, enabling atomic resource management, CPU/memory limits, and unconditional termination upon handle closure (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).

### Preemption (Stream-Drop)
Halting a running background language model generation by deliberately closing the client-side loopback TCP socket, causing `llama-server` to immediately abort tensor calculations and free memory context.

### Reciprocal Rank Fusion (RRF)
An algorithmic technique for fusing rankings from multiple information retrieval systems (e.g., lexical BM25 and semantic cosine similarity) without requiring score normalization.

### SimHash
A locality-sensitive hash algorithm that maps variable-length text documents into a 64-bit integer such that minor wording changes result in small bitwise Hamming distances.

### SQLCipher
An open-source extension to SQLite that provides transparent, 256-bit AES encryption of database files, journal files, and temporary storage.
