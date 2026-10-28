# Data, Storage & Privacy Architecture

This section documents Tessera's privacy guarantees, data lifecycle, encryption mechanics, and security threat boundaries.

Tessera is built on an uncompromising principle: **your screen activity and private thoughts belong exclusively on your computer.**

---

## 1. What Makes Tessera Privacy-First? (Introduction)

Traditional AI desktop software acts as a "dumb terminal"—it captures your screen, packages it into images or text, and streams it over the internet to remote cloud servers for processing.

**Tessera reverses this relationship completely:**
Instead of sending your private data to the AI model in the cloud, **Tessera brings the AI model down to your data** on your local machine.

```mermaid
flowchart LR
    subgraph CLOUD ["Traditional Cloud AI Tools (Insecure)"]
        SCREEN1["Your Screen Text &\nPrivate Passwords"] --> |Internet Streaming| CLOUD_SERVER["Remote Cloud Servers\n(Third-Party Logs)"]
    end

    subgraph TESSERA ["Tessera Architecture (Zero-Cloud)"]
        SCREEN2["Your Screen Text"] --> REDACT["tsr-guard\n(Secret Redaction)"]
        REDACT --> VAULT[("Encrypted Local SQLite\n(Protected by DPAPI)")]
        VAULT --> LOCAL_MODEL["Local 4B Model\n(On Your Own PC)"]
    end
```

---

## 2. The Four Pillars of Tessera's Privacy Guarantee

### Pillar 1: Zero Outbound Network Requests
Tessera's Rust backend code contains no networking libraries for cloud telemetry or analytics. When network auditing tools (`tools/netmon.ps1`) monitor Tessera for 10 minutes of continuous operation, application code produces **0 outbound network connections**. The only network socket initialized is a private local loopback (`127.0.0.1`) connecting the host to the sidecar.

### Pillar 2: Redaction *Before* Storage (The "Black Marker" Analogy)
Think of `tsr-guard` as an automated security inspector standing with a black permanent marker:
* Before any text captured from your screen is written to disk, it is scanned against a deterministic suite of **355 regular expression patterns**.
* If a credit card number, GitHub personal access token, AWS secret key, or private email is found, it is permanently overwritten with an opaque token (e.g., `[REDACTED_API_KEY]`).
* The raw secret is erased from volatile memory immediately and never touches persistent storage.

### Pillar 3: Encryption at Rest via Windows DPAPI
All indexed text, window titles, session histories, and derived memory vectors are stored in a local SQLite database encrypted with **AES-256** using the Windows Data Protection API (DPAPI):
* The encryption master key is wrapped using Windows `CryptProtectData`.
* The key is tied directly to your Windows user account credentials and TPM security chip.
* If someone steals your laptop's hard drive or copies the database file without knowing your Windows login, the database appears as unreadable cryptographic noise.

### Pillar 4: Content-Free Logging
Debugging application errors must never compromise privacy. Tessera enforces a strict **content-free logging policy**:
* Log files only contain component names, numeric error codes, and microsecond timestamps.
* Window titles, user search queries, and screen text fragments are strictly prohibited from appearing in log outputs.

---

## 3. Privacy & Storage Chapters

Explore the complete technical and cryptographic specifications:

* **[Privacy Architecture](/privacy/privacy-architecture)** — What Tessera keeps, what it redacts, and what remains outside its control.
* **[Data Lifecycle & Sessions](/privacy/data-lifecycle)** — The lifecycle of screen text from volatile RAM buffers to disk storage.
* **[Database Schema & ERD](/privacy/database-schema)** — Entity-relationship diagram and schema definition for encrypted SQLite.
* **[DPAPI Key Management](/privacy/key-management)** — Key derivation, Windows DPAPI wrapping, and memory zeroization.
* **[Content-Free Logging](/privacy/content-free-logging)** — How Tessera prevents accidental data leakage in diagnostics.
* **[Threat Model & Residual Risks](/privacy/threat-model)** — Comprehensive threat analysis, trust boundaries, and open audit findings.
