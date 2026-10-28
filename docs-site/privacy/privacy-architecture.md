# Privacy Architecture

Tessera defines strict data boundaries governing what is persisted, what leaves the machine, and what is scrubbed.

---

## 1. What Tessera Keeps (All Stored Locally in `%LOCALAPPDATA%\Tessara`)

| Data Category | Storage Location | Protection Mechanism | Empirical Evidence |
|---|---|---|---|
| **Captured Screen Text** | `tessara.db` (`items` table) | Whole-database SQLCipher AES-256 encryption; DPAPI key wrapping | Canary disk scan: 0 plaintext hits after 60s run. |
| **Extracted Facts & Loops** | `tessara.db` (`facts` table) | Same (SQLCipher encrypted) | Verified in unit tests. |
| **Summaries & Rollups** | `tessara.db` (`summaries` table) | Same (SQLCipher encrypted) | Verified in unit tests. |
| **Diagnostic Events & Logs**| `tessara.db` (`audit_events`), `logs\` | Content-free allowlist (No text, only IDs & counts) | Tracing allowlist automated scan test. |
| **Approved Model Weights** | `models\` directory | Read-only; SHA-256 verified against `tools/models.lock` | Startup digest validation check. |

### Data That is Never Stored
- **Zero Screenshots on Disk:** Screen captures exist exclusively as BGRA8 bitmaps in volatile RAM for milliseconds while WinRT OCR reads lines, after which buffers are zeroized.
- **Zero Keystrokes or Audio:** Tessera installs no low-level keyboard or mouse hooks; it registers standard Win32 `RegisterHotKey` chords and UIA focus events.
- **Zero Raw Drafts:** Draft text lives in RAM until accepted, edited, or discarded. The audit log records only `"inserted"`, `"copied"`, or `"discarded"` alongside character counts.

---

## 2. What Tessera Transmits

| Subsystem / Process | Outbound Network Behavior | Verification Status |
|---|---|---|
| **Tessera Rust Core** | Zero outbound network connections. Models execute over loopback (`127.0.0.1`). | <StatusBadge status="pass" text="Verified" /> Tested via Sysmon network monitoring. |
| **Rust Dependencies** | Zero network-capable crates reachable outside `tsr-net`. | <StatusBadge status="pass" text="Verified" /> Verified by `check_network_deps.py` and `cargo-deny`. |
| **WebView2 UI Runtime** | Telemetry and SmartScreen disabled; **makes 1 outbound HTTPS connection per run to Microsoft IP space**. | <StatusBadge status="fail" text="Open Finding (ADR-0002)" /> Upstream Chromium/WebView2 runtime connection. |
| **Auto-Updater** | Feature compiled out of default builds due to upstream zip advisories. | <StatusBadge status="pass" text="Verified" /> Gated behind inactive Cargo feature ([ADR-0003](https://github.com/webex042/Tessera/blob/main/docs/adr/0003-updater-blocked-by-zip-advisories.md)). |

::: warning The "Nothing Leaves Your PC" Boundary
Until the upstream WebView2 outbound telemetry request is eliminated or sandboxed ([ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md)), Tessera does **not** claim complete zero-network silence for the UI process.
:::
