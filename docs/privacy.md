# Privacy: what Tessara keeps, sends and cannot control

> Draft started in Stage 0. Plan §15 is the design; this page says what is **true of the code
> today**. Nothing here claims a guarantee that has not been tested. Final review: Stage 7.

## Current state (2026-10-26)

Tessara does not capture anything yet. What exists is a pill window, an encrypted-store module, an
embedding-runtime loader and test tooling.

## Network

| Component | Behaviour today | Evidence |
|---|---|---|
| `tessara.exe` (our Rust code) | no outbound connections observed | `tools/netmon.ps1`, 3 runs |
| Rust dependencies | no network-capable crate in the default build | `tools/check_network_deps.py`, `cargo deny` |
| WebView2 (UI) | SmartScreen, proxy auto-discovery and Chromium background networking switched off; **one HTTPS connection to a Microsoft address about a minute after start is still made by the WebView2 runtime** and is not yet identified | `docs/stage0/network-test.md`, ADR-0002 |
| WebView2 runtime updater (`msedgeupdate`) | a Windows system service shared by all apps that use WebView2; Tessara does not start, control or disable it | — |
| llama-server (sidecar) | not run yet; will start loopback-only with `--offline` and a cleared environment | plan §7.1 |
| Optional updater | compiled out of every build; blocked by dependency advisories | ADR-0003 |

Until ADR-0002 is resolved, the statement "nothing leaves your PC" is **not yet true** for the UI
process and must not be made in user-facing text.

## Disk

- The store module encrypts the whole database (SQLCipher, AES-256 via OpenSSL); tests confirm no
  plaintext header and no test canary in the database or its WAL file.
- Test tooling writes nothing personal into the repository: network logs record local ports only,
  and Office fixtures are generated in `%TEMP%` because Office stamps the signed-in user's name
  into files it saves.

## Outside the app's control

Windows pagefile and hibernation file, malware running as the same user, and the system-wide
WebView2 runtime updater. See plan §15.2.
