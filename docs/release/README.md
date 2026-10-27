# Release engineering (Stage 10)

Status 2026-10-28: configured and documented; **no installer has been built or signed** (both
need owner actions below).

## Installer (NSIS)

`src-tauri/tauri.conf.json` → `bundle`: NSIS, per-user install (no admin), WebView2 bootstrapper
embedded, LZMA. `bundle.active` stays **false** until the owner approves the first `tauri build`,
because the Tauri CLI downloads the NSIS toolchain on first use (an external download).

To build once approved:

```
pnpm --dir ui build
pnpm tauri build --bundles nsis
```

Still to add before a public build: the pinned llama.cpp builds and `onnxruntime.dll` as bundle
resources (hash-checked at start by `tsr-infer`), and an uninstaller option to keep or delete
`%LOCALAPPDATA%\Tessara` (the encrypted memory).

## Code signing (owner action)

Unsigned installers trigger SmartScreen warnings. Signing needs the owner's Authenticode
certificate (OV/EV, or Azure Trusted Signing); it never goes into the repository.

1. Put the certificate in the Windows certificate store (or use the HSM/token the CA supplies).
2. Set `bundle.windows.certificateThumbprint`, `digestAlgorithm: "sha256"` and
   `timestampUrl` (the CA's RFC 3161 server) in `tauri.conf.json`, or sign after the build:
   `signtool sign /fd sha256 /tr <timestamp-url> /td sha256 /sha1 <thumbprint> <installer.exe>`.
3. Sign `tessara.exe`, the installer, and the bundled `llama-server.exe` and DLLs.
4. Verify: `signtool verify /pa /v <file>`.

## SBOM

`python tools/gen_sbom.py deny-list.json pnpm-lic.json > sbom.cdx.json` (CycloneDX 1.5, offline;
inputs from `cargo deny --offline list -f json -l crate` and `pnpm licenses list --json --prod`).
This commit's SBOM: [`sbom.cdx.json`](sbom.cdx.json) — 348 components (321 libraries, 20 pinned
binaries, 7 model/data files with hashes). Licence inventory: `THIRD_PARTY_NOTICES.md`.

## Updater gate (kept closed)

`cargo deny --offline --features tessara/updater check advisories` still fails on 2026-10-28:
RUSTSEC-2026-0367, -0368, -0369 (`zip` 4.6.1 via `tauri-plugin-updater` 2.14.0). The `updater`
feature stays off in every build (ADR-0003); no advisory is ignored.
