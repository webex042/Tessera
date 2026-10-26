# Execution log

Newest first. One entry per working session: stage, what changed, what was run and what it showed.

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
