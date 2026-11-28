# Download & Release Notes

Tessera distributes signed-manifest, unsigned **Windows 11 (x64)** desktop builds (`v0.0.1-beta`) packaged with Tauri 2, bundled hash-verified `llama.cpp` `b11900` inference sidecars (`cpu-x64` and `vulkan-x64`), and pinned ONNX Runtime `1.31.0`. The Windows bundle identifier in `tauri.conf.json` uses the internal executable/package name `Tessara` (`tessara.exe` and `%LOCALAPPDATA%\Tessara`), while the project and website use **Tessera**.

> [!WARNING] Unsigned Beta Notice & Current Release Scope
> - **SmartScreen Warning:** The published `v0.0.1-beta` Windows binaries are **unsigned**. Running an unsigned `.exe` installer on Windows 11 triggers Microsoft Defender SmartScreen ("Windows protected your PC"). Verify the SHA-256 checksum against `SHA256SUMS.txt` below before selecting **More info → Run anyway**.
> - **Known Prototype Limitations:** Local AMD iGPU testing still fails blind **Extraction F1 (`0.500 < 0.60`)**, leaves **WebView2 Microsoft TLS sockets open (`ADR-0002`)**, and has **no RTX 4050 or live Groq evaluation yet**. Review [Measured Status and Limitations](/guide/limitations) before installing.

---

## Release Downloads (`v0.0.1-beta`)

| Download Package | Format | File Size | SHA-256 Digest |
|---|---|---|---|
| [**Tessara_0.0.1_x64-setup.exe**](https://github.com/webex042/Tessera/releases/download/v0.0.1-beta/Tessara_0.0.1_x64-setup.exe) | NSIS Installer (`currentUser`) | `30,609,607` bytes (~29.19 MiB) | `ff4aa21e694ca4ae451db1bdc1fca475a661e7fc647bdc299c252f46f3e061f5` |
| [**Tessara_0.0.1_x64-portable.zip**](https://github.com/webex042/Tessera/releases/download/v0.0.1-beta/Tessara_0.0.1_x64-portable.zip) | Portable Archive (109 files) | `76,539,597` bytes (~73.00 MiB) | `1230e67a2307d3deb1de4a4ee32600e62417627640308ca950e6de0327e8f1bc` |
| [**SHA256SUMS.txt**](https://github.com/webex042/Tessera/releases/download/v0.0.1-beta/SHA256SUMS.txt) | Checksum Manifest | Manifest | Published with GitHub Release |

View full release information and source code on [GitHub Releases v0.0.1-beta](https://github.com/webex042/Tessera/releases/tag/v0.0.1-beta).

### Verify Checksum in PowerShell Before Launching

```powershell
Get-FileHash .\Tessara_0.0.1_x64-setup.exe -Algorithm SHA256
Get-FileHash .\Tessara_0.0.1_x64-portable.zip -Algorithm SHA256
```

Compare the hexadecimal hash with the table above or `SHA256SUMS.txt`.

---

## Included vs. Separate Model Downloads

To keep installer sizes modest and avoid bundling multi-gigabyte binary blobs:

| Component | Included in Installer / Portable ZIP? | Size | Security Check |
|---|---|---|---|
| `tessara.exe` + React 19 UI | **Yes** | ~18–25 MB | Compiled without DevTools; CSP locked to `'self'` and IPC |
| `llama-server.exe` + `ggml-*.dll` (`cpu-x64`, `vulkan-x64`) | **Yes** | ~90 MB unpacked | Every file verified against compiled-in `tools/sidecar.lock` SHA-256 |
| `onnxruntime.dll` (v1.31.0) | **Yes** | ~17 MB | Pinned SHA-256 checked at load (`tsr-retrieval::runtime::load_pinned`) |
| `Qwen3VL-4B-Instruct` GGUF + projector | **Optional in-app download** | ~3.10 GiB | Consent-gated download from `huggingface.co` verified against `tools/models.lock` |
| `multilingual-e5-small-onnx` embedder | **Optional in-app download** | ~465 MiB | Consent-gated download from `huggingface.co` verified against `tools/models.lock` |
| Groq Cloud API key | **Never bundled** | 0 B | Optional user-provided BYOK stored solely in Windows DPAPI (`groq_key.bin`) |

---

## Choosing an AI Provider After First Launch

Tessera starts with **screen capture paused by default**. When you open the notch (**Ctrl+Alt+Space** or tray icon) and select **AI Setup**:

1. **Option 1 — Local AI (`llama.cpp`, 100% offline):**
   - Runs locally with Vulkan GPU acceleration or CPU fallback.
   - Tessera checks available GPU VRAM, system RAM, and free disk space before offering a consented download of `Qwen3VL-4B-Instruct-Q4_K_M.gguf` (plus optional `multilingual-e5-small-onnx` for vector search).
2. **Option 2 — Groq Cloud BYOK (`openai/gpt-oss-20b`):**
   - Fast inference without large local model downloads (~3.1 GiB saved). Requires your own free-tier or paid Groq API key.
   - Free-tier quotas are subject to Groq's account limits (`20 RPM`, `700 RPD`, `6,000 TPM`, `150,000 TPD` baseline tracking).
   - **Privacy boundary:** Full deterministic PII masking (`mask_pii`) replaces person names, emails, phones, addresses, DOBs, cards, and credentials with typed tokens (`[PERSON_01]`, `[EMAIL]`, etc.) before outbound payloads leave the device. Screen images (`image_png`) are never sent to Groq.

---

## Changelog (`v0.0.1`)

- **AES-256-GCM VectorDb v2 (`tsr-store`):** Replaced custom SHA-256 CTR + timestamp nonces with `ring::aead::AES_256_GCM` and CSPRNG 96-bit nonces (`SystemRandom`), plus explicit authenticated migration from legacy `v1` files with automatic `.v1.bak` backup creation.
- **Outbound Full PII Masking (`tsr-net`):** Applied `tsr_guard::redact::mask_pii` across all outbound Groq prompt paths (Ask, Draft, Extraction, Rollups).
- **Privacy & Deletion:** Single-item deletion across SQLCipher + `VectorDb`, token-confirmed (`CLEAR`) wipes, configurable retention pruning (`0` Forever, UI presets `7 / 30 / 90 / 365` days, backend cap `3650` days), and live blocklist editing in **Privacy Controls**.
- **In-App AI Onboarding & Runtime Switching:** Added first-run setup UI for local GGUF verification/downloads and Groq BYOK configuration (stored via Windows DPAPI) with live runtime hot-swapping.
- **Embedded Vector Retrieval Integration (`tsr-retrieval`, `tsr-pipeline`):** Connected pinned ONNX Runtime `1.31.0` + `multilingual-e5-small-onnx` vector indexing into `tessera_vectors.enc` and Ask/Search hybrid RRF retrieval with automatic FTS5 fallback.
- **Windows Desktop Packaging:** Enabled Tauri 2 `currentUser` NSIS installer and portable ZIP targets bundling verified CPU/Vulkan `llama-server` and pinned `onnxruntime.dll` sidecars.

---

## Upgrade & Uninstall Behavior

- **Per-user install:** Installs into `%LOCALAPPDATA%\Programs\Tessera` without requiring Administrator UAC elevation.
- **Your data stays yours:** Captured SQLCipher database (`tessara.db`), encrypted `VectorDb` (`tessera_vectors.enc`), and DPAPI keys (`key.bin`, `groq_key.bin`) reside in `%LOCALAPPDATA%\Tessara`. Upgrading or uninstalling preserves your local profile unless you run **Privacy → Clear All Memories** or manually delete `%LOCALAPPDATA%\Tessara`.
- **More help:** See [System Requirements](/guide/requirements), [Configuration](/getting-started/configuration), and [Troubleshooting](/getting-started/troubleshooting).
