# Stage 0 findings: what the plan assumed vs. what the pinned builds do

Every item here was checked against source code, `--help` output or a live run on 2026-10-26 on the
dev machine (Windows 11 build 26300, AMD Ryzen 5 7530U, Radeon iGPU 512 MB, 16 GB RAM). Items marked
**plan updated** were folded into `implementation_plan.md` / `stages.md` in the same commit.

## Pinned versions

| Component | Version | How pinned | Verified by |
|---|---|---|---|
| Rust toolchain | 1.99.1 stable (MSVC) | `rust-version` in Cargo.toml | build |
| tauri / tauri-build | 2.12.5 / 2.7.4 | `=` in Cargo.toml, Cargo.lock | build + run |
| wry (via tauri) | 0.57.1 | Cargo.lock | source read |
| WebView2 runtime | 155.0.4283.62 | system component (not pinned by us) | live read-back |
| windows crate | 0.62.2 | Cargo.lock | build |
| rusqlite / libsqlite3-sys | 0.41.0 / 0.39.1 | `=` + Cargo.lock | tests |
| SQLCipher / SQLite | 4.19.0 community / 3.53.4 | via sqlcipher-src 419.0.6 | `PRAGMA cipher_version` |
| fastembed / ort | 7.5.0 / 2.0.0-rc.15 | `=` + Cargo.lock | source read + tests |
| ONNX Runtime | 1.31.0 win-x64 | `tools/sidecar.lock` (archive + DLL SHA-256) | fetch script + load test |
| llama.cpp | b11900 (`74b104c`) | `tools/sidecar.lock` (archive + exe SHA-256 for cpu-x64) | `--help` |
| React / Vite / Tailwind / TS | 19.3.1 / 8.3.5 / 4.3.3 / 7.0.2 | exact in package.json + pnpm-lock.yaml | build |

The plan said React 18; React 19 is current and nothing in the plan depends on 18. **Plan updated.**

## 1. Embeddings and ONNX Runtime (D7, §10.1) — plan updated

- `fastembed` 7.5.0 default features are `onnx`, `ort-download-binaries-native-tls`,
  `hf-hub-native-tls`, `image-models`. The first two network paths are real: `hf-hub` (TLS HTTP at
  run time) and `ort/download-binaries` (ureq download inside ort-sys's **build script**).
- Correct declaration: `fastembed = { default-features = false, features = ["ort-load-dynamic"] }`
  and `ort = { default-features = false, features = ["std", "load-dynamic", "api-31"] }`.
  `tools/check_network_deps.py` confirms no network-capable crate is reachable; its self-test
  fixtures (`tests/fixtures/netdeps/`) prove it rejects both default-feature configurations.
- **New risk found:** Windows 11 ships `C:\Windows\System32\onnxruntime.dll` (version 1.17 on this
  machine). With `load-dynamic`, `ort` falls back to `ORT_DYLIB_PATH` or a bare `onnxruntime.dll`
  resolved by the normal DLL search if nobody called `ort::init_from` first, and at the default
  `api-17` level it would accept 1.17. Mitigations implemented in `tsr-retrieval::runtime`:
  hash-check an absolute path, `ort::init_from(path)`, build the one process-wide `Environment`,
  confirm with `GetModuleHandleW(full path)` that our file is mapped and System32's is not, and
  require `api-31` so anything older than 1.31 is refused even on the fallback path.
- `fastembed` creates its own default environment via `ort::init()` unless given one; embedders
  must be built with `InitOptionsUserDefined::with_environment(pinned.environment)`.
- `cargo deny` flags `paste` (RUSTSEC-2024-0436, unmaintained, build-time proc-macro via
  `tokenizers`). Ignored with a written reason; re-check on each fastembed bump.
- Not verified here (no model files by request): end-to-end embedding of text and the
  e5-small vs bge-small recall comparison.

## 2. SQLCipher (D5, §9.1) — plan updated

- `bundled-sqlcipher-vendored-openssl` builds SQLCipher 4.19.0 on SQLite 3.53.4 with the
  OpenSSL provider; FTS5 is compiled in. Strawberry Perl is required (portable zip works; no admin).
  First build of vendored OpenSSL took ~24 min on this laptop (single-threaded nmake); later builds
  are cached.
- `sqlcipher-src` also bundles libtomcrypt sources (unused with our features) and declares
  `blessing` and `WTFPL` licences; allowed for that crate only in `deny.toml`.
- On a wrong key SQLCipher writes its own log lines to stderr (`ERROR CORE sqlite3Codec ...`).
  They contain no content, but Stage 1 must set `PRAGMA cipher_log_level = NONE` so the store has
  no logging channel outside our content-free tracing layer.
- Tests (`crates/tsr-store/tests/sqlcipher.rs`): FTS5 round trip incl. Hinglish, wrong key
  rejected, no plaintext header or canary (UTF-8/UTF-16) in the DB or WAL files. All pass.

## 3. llama-server b11900 (§7.1) — plan updated

From `docs/stage0/llama-server-b11900-help.txt`:

| Plan said | b11900 reality | Action |
|---|---|---|
| `--no-webui` | exists (`--ui, --webui, --no-ui, --no-webui`); web UI **on by default** | keep |
| — | `--offline` exists ("prevents network access") | add |
| — | `--agent` / `--tools` / `--mcp-servers-*` / `--ui-mcp-proxy` (experimental agent tooling) | add `--no-agent --no-ui-mcp-proxy`; never pass the others |
| `-c 8192` | default `-c 0` = model's full trained context | keep explicit `-c 8192` |
| — | `--cache-ram` defaults to **8192 MiB** of host RAM for prompt cache | add `--cache-ram 0` (tune in Stage 2) |
| — | `--fit on` by default silently shrinks unset args to fit memory | add `--fit off` so misconfiguration fails loudly |
| — | CORS default `*` with credentials enabled | add `--no-cors-credentials --cors-origins http://tessara.invalid` |
| — | `--slots` endpoint on by default | add `--no-slots` |
| — | `--reasoning auto` | add `--reasoning off` (Instruct model, schema output) |
| — | **reads `LLAMA_ARG_*` env vars** (e.g. `LLAMA_ARG_AGENT`, `LLAMA_ARG_MCP_SERVERS_JSON`, `LLAMA_ARG_HF_REPO`) and `HF_TOKEN` | start the sidecar with a **cleared environment** (allow-list: SystemRoot, SystemDrive, windir, TEMP, TMP, NUMBER_OF_PROCESSORS, PROCESSOR_ARCHITECTURE) |
| `-fa on`, `-ctk/-ctv q8_0`, `--jinja`, `-np 1`, `-ngl 99`, `--mmproj` | present as written | keep |

Not verified (needs a model): that the server starts with this exact line, that the CORS value is
accepted, and that `--offline` blocks every download path.

## 4. WebView2 (§13.5, §18.6, §21) — plan updated

- wry 0.57.1 default browser args, read from `src/webview2/mod.rs`:
  `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` (plus autoplay/proxy only when
  configured). Setting `additionalBrowserArgs` replaces this string, so `src-tauri` keeps it
  verbatim and appends ours. A unit test guards the prefix.
- `ICoreWebView2Settings8::SetIsReputationCheckingRequired(false)` works on runtime
  155.0.4283.62; read back `false` from the live webview.
- WebView2 honours `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` and other `WEBVIEW2_*` variables over
  app settings. The app clears them before the first webview; verified by planting
  `--remote-debugging-port=9222` and reading back `cleared_env_vars`.
- **NetLog showed WPAD proxy auto-discovery lookups (`wpad:80`).** Added `--no-proxy-server`;
  the next NetLog had none.
- NetLog also shows Chromium's IPv6 reachability probe (a UDP *connect* to a Microsoft address,
  which sends no packets) and speculative pre-connects to `tauri.localhost` on loopback port 80.
- **Open S1 failure:** in three separate runs (10, 5 and 4 minutes) the WebView2 browser process
  opened one TCP connection to port 443 in Microsoft's `2603:1046:c04::/48` range about a minute
  after start. It does **not** appear in the Chromium NetLog and its name was not in the Windows
  DNS cache, so it is made outside Chromium's network service — most likely the Edge/WebView2
  diagnostic-data client. Identifying the host needs an elevated packet capture (TLS SNI); the
  procedure is in `network-test.md`. Decision needed: ADR-0002.

## 5. Capture APIs (§5.4) — plan updated

- Measured on synthetic fixtures (see `app-compatibility.md`). `UIA` connection/transaction
  timeouts of 300 ms are **not a wall-clock cap**: Word failed after 2.8 s and Explorer after
  0.6 s, both with timeout errors. Stage 1 must run UIA on a worker that the caller abandons at the
  deadline (plan §19 budget "≤ 300 ms p95, hard timeout").
- PowerPoint's tree walk took 559 ms; Chromium's first tree build 230–320 ms. Both above the
  300 ms budget on first contact.
- Borderless WGC capture: `GraphicsCaptureAccess::RequestAccessAsync(Borderless)` returned
  *Allowed* for this unpackaged process; `SetIsBorderRequired(false)` succeeded in every capture.
- WGC single frame 150–350 ms, Windows OCR 60–150 ms (window sizes up to 1920×1008): within the
  500 ms p95 budget.
- windows-rs 0.62: blocking waits on WinRT async are `.join()` (not `.get()`); UIA
  `CreatePropertyCondition`/value reads need the `Win32_System_Ole` feature, and the Win32
  `VARIANT` has no string conversion, so values are read through the cached `ValuePattern`.
- Password fixture: the canary password value never appeared in UIA or OCR output in Edge or
  Chrome. The TextPattern path relies on the browser masking the field; Stage 1 must add an
  explicit IsPassword check for TextPattern documents.

## 6. Updater exception (D10, §20.3, Stage 10) — plan updated

- Default build: no `reqwest`/`rustls`/`ureq` anywhere in the graph (`cargo tree -i`).
- `--features updater`: `reqwest` 0.13.5 and `rustls` 0.23.46 come in only through
  `tauri-plugin-updater` 2.14.0 (plus `rustls-platform-verifier`, which uses rustls directly;
  added to the wrapper list).
- **Blocker:** `tauri-plugin-updater` 2.14.0 depends on `zip` 4.6.1, which has
  RUSTSEC-2026-0367, -0368 and -0369 (symlink escape and permission handling during extraction);
  fixed in zip >= 9.0, which the plugin's version range does not allow. The updater build fails
  `cargo deny`; CI keeps that job visible (`continue-on-error`) and Stage 10 cannot pass until it
  is green. Not ignored.

## 7. Documentation errors fixed

- §16.4 example labelled 2026-10-21 as "Tue"; it is a Wednesday.

## 8. Privacy slips caught while testing

- Raw network CSVs recorded the machine's public IPv6 address; `tools/netmon.ps1` now records
  local ports only and `bench/results/` is git-ignored.
- A `.pptx` generated through PowerPoint and Word's `~$` owner file carried the signed-in user's
  name. Both deleted; the probe script now builds Office fixtures in `%TEMP%` and `~$*` is ignored.
