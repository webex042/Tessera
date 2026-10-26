# ADR-0001: Stage 0 dependency pins and hardening

- Status: Accepted (2026-10-26)
- Revisits: plan D2, D5, D7, D10

## Context

The plan was written before any build. Stage 0 checked its assumptions against the pinned
versions (details and evidence: `docs/stage0/findings.md`).

## Decision

1. Pin exactly (`=`) the crates whose features carry privacy guarantees: rusqlite 0.41.0,
   fastembed 7.5.0, ort 2.0.0-rc.15, tauri 2.12.5, tauri-plugin-updater 2.14.0. Pin external
   binaries by SHA-256 in `tools/sidecar.lock` (llama.cpp b11900, ONNX Runtime 1.31.0), checking both
   the archive and each extracted file the app loads.
2. Embeddings: `fastembed` with `default-features = false, features = ["ort-load-dynamic"]`;
   `ort` with `std, load-dynamic, api-31`. Load ONNX Runtime only through
   `tsr_retrieval::runtime::load_pinned` (absolute path, hash check, `ort::init_from`, module
   check) and pass its `Environment` to every embedder.
3. llama-server: start with a cleared environment and the hardened command line in
   `bench/tessara_bench/llama.py::server_args` (adds `--offline --no-agent --no-ui-mcp-proxy
   --no-slots --no-cors-credentials --cors-origins http://tessara.invalid --fit off --cache-ram 0
   --reasoning off` to the plan's flags).
4. WebView2: keep wry's default argument string verbatim, append background-networking switches
   and `--no-proxy-server`, set `IsReputationCheckingRequired(false)`, lock navigation to the app
   origin, clear `WEBVIEW2_*` override variables at start-up.
5. Network policy enforced two ways: `deny.toml` bans with wrappers, and
   `tools/check_network_deps.py` over the full resolved graph including build dependencies.

## Consequences

- Upgrading any pinned crate or binary needs the Stage 0 checks re-run (findings.md lists them).
- Windows' own System32 `onnxruntime.dll` can never be loaded by mistake.
- A local `LLAMA_ARG_*` or `WEBVIEW2_*` variable cannot change sidecar or webview behaviour.
- The vendored OpenSSL build needs Perl and is slow the first time (~24 min on the dev laptop).
