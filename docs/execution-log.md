# Execution log

Newest first. One entry per working session: stage, what changed, what was run and what it showed.

## 2026-10-26 (evening) — Stage 1: logging, DPAPI key, items/FTS, redaction

- `tsr-core`: `Sensitive<T>` (formats as `<redacted>`, zeroized on drop); `logging::ContentFreeLayer`
  with reviewed message/field allowlists; workspace-wide test that audits every `tracing::` call
  (it immediately caught a generic `name` field in tsr-infer, renamed `adapter_name`).
- `tsr-win`: DPAPI `protect`/`unprotect` + `BCryptGenRandom` (real tests on this machine).
- `tsr-store`: `open_store` (key file, refuse-to-replace-key rule), migration 0002 (sources, items,
  items_fts with triggers and FTS5 secure-delete), repository: source upsert, URL credential
  stripping, insert, injection-safe search (incl. Hindi/Hinglish), range delete with dry run.
- `tsr-guard`: redaction v1 (11 secret kinds + optional phone/e-mail), generated corpora
  (`tools/gen_redaction_corpus.py`), random-input idempotence test. ADR-0005.

| Command | Result |
|---|---|
| `cargo test -p tsr-core` | 5 passed (incl. canary-never-logged, workspace tracing audit) |
| `cargo test -p tsr-win` | 3 passed (REAL DPAPI) |
| `cargo test -p tsr-store` | 27 passed (REAL DPAPI open_store ×4, items/FTS ×6, migrations ×8, SQLCipher ×5, …) |
| `cargo test -p tsr-guard` | 19 passed; corpus: 290/290 secrets, 0.43 % false positives |
| `cargo test --workspace` | 102 passed, 0 failed; clippy `-D warnings` and rustfmt clean |

## 2026-10-26 (later still) — Stage 1: UIA wall-clock deadline

- `crates/tsr-capture`: `DeadlineWorker` (dedicated thread, caller waits at most the deadline,
  `Busy` instead of queueing behind a stuck call, init excluded from the first call, panics contained),
  `UiaFallback` (two timeouts in a row → OCR only for 10 min; bounded memory), `UiaReader`
  (TextPattern only when the document has no password field; otherwise one cached subtree round
  trip, password subtrees skipped structurally; `Debug` hides text).
- `bench/uia-probe` now uses the production reader and reports abandoned-read times.
- Bugs caught: the first read paid for UIA initialisation (fixed: spawn waits for readiness);
  a 300 ms wait returned after up to 314 ms because of Windows' timer tick (deadline set to 285 ms).

| Command | Result |
|---|---|
| `cargo test -p tsr-capture` | 12 passed (incl. REAL taskbar read: 300.07 ms cut-off cold, 125–145 ms warm) |
| `tools/run-capture-probe.ps1` | 16/16 windows usable; max caller-observed UIA 294 ms; Word 285 ms (abandoned read 290 ms), PowerPoint 285 (647), Explorer 294 (732) |
| `cargo test --workspace` | 75 passed |

## 2026-10-26 (later) — Stage 1 started; GPU-vendor independence

**Stage 1 (part 1, commit 825f6aa):** `tsr-guard` scoped blocklist (10 tests incl. the
non-browser regression test); `tsr-store` migration runner, 0001 schema (blocklist with `scope` and
content-free `key`, settings, audit log), blocklist repository with per-rule block counts (8 tests);
`cipher_log_level = NONE` with a child-process stderr test (written first, seen failing, then fixed).

**GPU portability (owner request, ADR-0004):**
- Audit: no Rust dependency on CUDA; NVIDIA-only pieces were the benchmark's `nvidia-smi` sampler and
  docs. Fixed both.
- `crates/tsr-infer`: OS adapter enumeration (DXGI / sysfs / Apple), pinned-build discovery with full
  manifest verification, real `--list-devices` probe with cleared environment, GGUF header reader,
  backend support table, ranked selection with override and CPU fallback, start-with-fallback,
  `tsr-hw` diagnostic CLI.
- `tools/sidecar.lock`: 19 llama.cpp builds (all backends/OSes) with GitHub digests; full per-file
  manifests for `cpu-x64`, `vulkan-x64` and ONNX Runtime. `tools/fetch_sidecar.py` replaces the
  PowerShell logic (wrapper kept).
- Bench: vendor-neutral per-process GPU memory (Windows counters), `--device/--ngl/--no-mmproj-offload`.
- Download with owner's consent: llama.cpp b11900 Vulkan build (37 MB, hash-checked). No models.

| Command | Result |
|---|---|
| `cargo test --workspace` | 63 passed (incl. 18 simulated hardware scenarios) |
| `cargo test -p tsr-infer --test real_hardware -- --ignored --test-threads=1` | 4 passed on AMD Radeon iGPU (Vulkan) |
| `tsr-hw` | AMD Radeon via DXGI; Vulkan probe `Vulkan0 … 8096 MiB, 7691 MiB free`; selected Vulkan, CPU fallback |
| `tsr-hw` timing | 0.6–0.7 s release, 10.4 s debug (SHA-256 of 105 files + probe) |
| bench sampler on `dwm.exe` | 130 MiB dedicated / 136–138 MiB shared, ~0.5 samples/s |
| network deps / cargo deny / integrity / evals / harness / UI | all pass |

**Bugs caught by tests:** device-list parser took a trailing log line for a GPU; GGUF reader accepted
a file truncated inside the last tensor descriptor (seek past EOF does not fail on Windows).

**Security findings:** backend DLLs were unpinned (exe identical across builds) — now full manifests;
`GGML_*` env vars change device visibility — probe/sidecar env cleared and tested; `ggml-rpc.dll`
present in every build (inert without `--rpc`).

**Still open:** Stage 1 UIA wall-clock deadline (next); real tests on NVIDIA/Intel/Apple/Qualcomm;
manifests for builds other than CPU/Vulkan; no model run on any backend.

## 2026-10-26 — Stage 0 (in progress)

**Constraints for this session (owner's decisions):** the dev machine has no NVIDIA GPU (AMD Radeon
iGPU, 512 MB VRAM); build the tooling only — no model downloads, no model runs.

**Done**
- Toolchain: Rust 1.99.1 (rustup, user scope, checksum-verified installer), portable Strawberry Perl
  5.42.3 (checksum-verified), pnpm 12.15, cargo-deny. VS Build Tools 2019 already present.
- Workspace: `crates/tsr-store`, `crates/tsr-retrieval`, `src-tauri` (app shell), `bench/uia-probe`,
  `ui/` (React 19 + Vite 8 + Tailwind 4 + TS 7). Lockfiles committed.
- `tsr-store`: SQLCipher open with raw key + §9.1 PRAGMAs; build-info check (SQLCipher + FTS5).
- `tsr-retrieval`: pinned ONNX Runtime loader (hash, `init_from`, module check, `api-31`).
- `src-tauri`: transparent pill window; WebView2 policy (wry defaults kept, background-networking
  switches, `--no-proxy-server`, SmartScreen off, navigation lock, `WEBVIEW2_*` cleared);
  `--stage0-report` read-back; debug-only NetLog switch.
- `tools/`: `sidecar.lock` + `fetch-sidecar.ps1` (llama.cpp b11900, ONNX Runtime 1.31.0),
  `check_network_deps.py` (+ self-test fixtures), `netmon.ps1`, `run-capture-probe.ps1`.
- `deny.toml` with network bans, crate-scoped licence exception, documented advisory ignore.
- `evals/`: 80 extraction items (126 gold facts, incl. 8 Hindi/Hinglish, 6 prompt-injection),
  60 Ask questions (8 unanswerable), 20 draft scenarios; `validate.py`.
- `bench/`: stdlib-only harness (`run_bench.py`) with hardened server launch, VRAM sampling, TTFT,
  scoring; `fake_server.py` (SIMULATED) for self-tests.
- Prompts/schemas from §16 as versioned files in `crates/tsr-infer/prompts/`.
- CI: `.github/workflows/ci.yml` (Rust, dependency audit both builds, Python/evals, frontend).
- Docs: `docs/stage0/{findings,results,network-test}.md`, `docs/app-compatibility.md`,
  ADR-0001/0002/0003, `docs/privacy.md` (draft); plan v0.3 and `stages.md` updated.

**Commands and results** (all run on the dev machine)

| Command | Result |
|---|---|
| `cargo test -p tsr-store` | 4 passed |
| `cargo test -p tsr-retrieval -- --include-ignored` | 4 passed (incl. real DLL load) |
| `cargo test -p tessara`, `cargo test -p uia-probe` | 2 + 4 passed |
| `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `python tools/check_network_deps.py --locked [--features tessara/updater]` | PASS / PASS |
| checker on `tests/fixtures/netdeps/*` | both FAIL as intended |
| `cargo deny check` | ok (advisories, bans, licenses, sources) |
| `cargo deny --features tessara/updater check` | FAIL: zip RUSTSEC-2026-0367/0368/0369 (ADR-0003) |
| `python evals/validate.py` | OK |
| `python -m unittest discover -s bench/tests` | 11 passed (SIMULATED server) |
| `pnpm build:ui` | clean |
| `tessara.exe --stage0-report` | WebView2 155.0.4283.62; SmartScreen off; override var cleared |
| `tools/netmon.ps1` ×3 + NetLog ×2 | WPAD lookups found → removed; one Microsoft 443 connection per run remains |
| `tools/run-capture-probe.ps1` (+ PowerPoint via COM) | 7 apps usable; UIA timeouts not wall-clock |

**Bugs found and fixed during the session:** draft fact-checker missed times like "5pm";
`fetch-sidecar.ps1` corrupted UTF-8 under PowerShell 5.1; `netmon.ps1` failed on an empty
argument list; windows-rs 0.62 API changes (`join()`, `Win32_System_Ole`, VARIANT handling).

**Privacy slips caught:** public IPv6 in raw network CSVs; Office user name in a generated
`.pptx` and a `~$` lock file. Removed; scripts changed so they cannot recur.

**Unresolved / blockers**
1. ADR-0002: WebView2 outbound connection — needs an elevated packet capture and an owner decision.
2. All model gates (VRAM, F1, JSON validity, latency, TTFT, drafts): need the RTX 4050 machine and
   model files.
3. App coverage gate: 7/20 measured; the rest need other apps or the owner's accounts.
4. Not yet built: `bench/embed-eval` (embedding recall), OCR-vs-VLM CER runner, 2 h/24 h network
   runs with Sysmon/pktmon.
5. ADR-0003: updater blocked by zip advisories (Stage 10).

**Known security limitations right now:** UIA reads have no wall-clock cap yet (Stage 1); TextPattern
path trusts the app to mask password fields (Stage 1); SQLCipher logs to stderr until
`cipher_log_level = NONE` lands (Stage 1); hash-then-load window for `onnxruntime.dll`
(same-user malware, out of scope per §15.2).

**Next stage prerequisites:** Stage 1 can start on this machine (it needs no model). Stage 0 stays
open until items 1–3 above are resolved or explicitly accepted by the owner in an ADR.
