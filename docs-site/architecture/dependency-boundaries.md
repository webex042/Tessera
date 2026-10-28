# Dependency Boundaries

Tessera enforces an architectural separation between network-capable crates, systems crates, and runtime binaries.

---

## The Network Air-Gap Rule

```mermaid
flowchart TD
    subgraph NETWORK_PERMITTED["Network Permitted (Strictly Controlled)"]
        TSR_NET["crates/tsr-net"]
        APPROVAL_TOKEN["Explicit User Approval Struct"]
        OS_SCHANNEL["Windows SChannel TLS Stack"]
    end

    subgraph NETWORK_BANNED["Network Strictly Banned (Compile-Time Enforced)"]
        TSR_CORE["crates/tsr-core"]
        TSR_WIN["crates/tsr-win"]
        TSR_CAPTURE["crates/tsr-capture"]
        TSR_GUARD["crates/tsr-guard"]
        TSR_STORE["crates/tsr-store"]
        TSR_RETRIEVAL["crates/tsr-retrieval"]
        TSR_INFER["crates/tsr-infer"]
        TSR_PIPELINE["crates/tsr-pipeline"]
        TSR_DRAFT["crates/tsr-draft"]
    end

    APPROVAL_TOKEN --> TSR_NET --> OS_SCHANNEL --> HF["Hugging Face (Models Only)"]
    TSR_INFER -.-> |"Custom std::net HTTP/1.1 (127.0.0.1 Only)"| LOCAL_SOCKET["Loopback Socket"]
```

### Compile-Time Verification
Tessera enforces the network ban via `tools/check_network_deps.py`:
- Walks the complete resolved dependency graph from `cargo metadata`.
- Verifies that banned network crates (`reqwest`, `hyper`, `curl`, `ureq`, `native-tls`, `tungstenite`) are unreachable from any crate other than `tsr-net`.
- Flags transitive dependencies in build scripts (e.g. `ort-sys` binary downloaders).

---

## Pinned Native Runtime Binaries

To guarantee reproducible and safe execution, external native runtimes are version-pinned and hash-verified:

| Runtime / Binary | Pinned Revision / Version | Manifest Reference | Loading & Validation Mechanism |
|---|---|---|---|
| **`llama-server.exe`** | b11900 | `tools/sidecar.lock` | Spawned as child process; all DLLs and executables checked against SHA-256 digests. |
| **`onnxruntime.dll`** | 1.31.0 | `tools/sidecar.lock` | Dynamically linked via `tsr-retrieval::runtime::load_pinned`. System32 DLLs rejected. |
| **`Qwen3-VL-4B-Instruct`** | Q4_K_M (commit-pinned) | `tools/models.lock` | GGUF header checked; SHA-256 digest validated before first launch. |
| **`multilingual-e5-small`** | ONNX format | `tools/models.lock` | Loaded exclusively by local `fastembed` runtime; no online model discovery. |
