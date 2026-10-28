# Frequently Asked Questions

---

### Does Tessera send my screen data or search queries to the cloud?
**No.** All screen capture analysis, text extraction, embedding generation, full-text indexing, and model inferences execute strictly on your own computer. The core crates have zero outbound network connectivity.

### Why does Tessera not link to CUDA directly?
Linking CUDA directly into the main application executable would require users to install hundreds of megabytes of NVIDIA SDKs and would prevent the application from running on AMD, Intel, or CPU-only systems. Decoupling into an isolated sidecar allows Tessera to support any GPU architecture via discrete, hash-checked runtime builds.

### Can Tessera act on my computer autonomously (e.g. click buttons or send emails)?
**No.** Tessera explicitly avoids autonomous agents. It does not send emails, click UI buttons, or invoke external APIs. When generating draft replies, text is pasted only after you review the draft and explicitly click **Insert**.

### How does Tessera handle password managers and private browser windows?
Tessera monitors active process names and window titles. Known password managers (1Password, Bitwarden, KeePass) and banking portals are skipped entirely. Furthermore, UI Automation structurally ignores any control with `IsPassword = true`, and OCR frames have password rectangles blanked out before image processing.

### Why does the initial build take over 20 minutes?
`rusqlite` compiles SQLCipher 4 from source with a vendored OpenSSL bundle (`bundled-sqlcipher-vendored-openssl`). Compiling OpenSSL from C source on Windows is CPU-intensive. Subsequent builds take only a few seconds because Cargo caches the compiled library artifacts.

### What is the open WebView2 network finding (ADR-0002)?
During Stage 0 network monitoring, our security audit detected that Microsoft's WebView2 runtime initiates a single outbound HTTPS request to Microsoft IP space upon startup despite passing flags to disable telemetry and SmartScreen. Tessera's own Rust code makes zero network calls, but true air-gapping currently requires firewall isolation until upstream WebView2 flags resolve this.
