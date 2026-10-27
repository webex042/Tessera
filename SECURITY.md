# Security policy

Tessara is pre-release software (no public builds yet). This page says how to report a problem
and what the security design is; [`docs/privacy.md`](docs/privacy.md) says what is true of the
current code, with evidence.

## Reporting a vulnerability

Please report privately by e-mail to the repository owner rather than in a public issue. Include
the version or commit, what you did, what happened, and whether real personal data was involved
(please don't send any). We aim to reply within 7 days and to fix high-severity issues before the
next release. There is no bug bounty.

## What Tessara defends against (plan §15)

| Threat | Defence | Evidence |
|---|---|---|
| Screen text instructing the model ("ignore previous instructions…") | content is data, never instructions: escaped `<screen_content>` blocks, strict JSON schemas, a code guard that rejects or holds facts that look injected, quoted, or ungrounded; the model has no tools and its output cannot act | `tests/fixtures/injection` (152 cases), `tsr-eval injection` (SIMULATED and real model) |
| Secrets in captured text | redaction before storage and before any model input (cards, Aadhaar incl. VID, PAN, passport, voter ID, bank accounts, IFSC, UPI, OTPs, passwords, API keys, JWTs, private keys) | `tests/fixtures/redaction` (355 cases, FP 0.4 %) |
| Data at rest | SQLCipher (whole database, AES-256), key wrapped with DPAPI (current user); no screenshots or audio on disk | store tests; `tools/canary-scan.ps1` |
| Data leaving the PC | only `tsr-net` may reach the internet (model download with approval); llama-server is loopback-only with a port-owner check; enforced by `cargo deny` and `tools/check_network_deps.py` | `tools/netmon.ps1`; ADR-0002 (WebView2 exception) |
| Tampered model or sidecar | sidecar files hash-checked against `tools/sidecar.lock` before start; model files against `tools/models.lock` (full digest on first use and after any change) | `tsr-infer` tests |
| Actions on the user's behalf | none without an explicit user step: fact status changes and draft insertion need single-use approval tokens issued by Rust; drafts are never sent; nothing is auto-closed | `no_auto_close.rs`, `insert_real.rs`, `views.rs` |
| Screen sharing showing Tessara | every visible window excluded from capture (`WDA_EXCLUDEFROMCAPTURE`), verified on pixels | `tools/verify-capture-exclusion.ps1` |
| WebView compromise | strict CSP (`'self'` only, no inline/eval), navigation lock, frozen prototypes, one capability (event listen), DevTools not in release builds | `src-tauri/tests/security_config.rs` |

## Out of scope

Malware running as the same Windows user (it can read anything the user can), the Windows
pagefile and hibernation file, physical attacks on an unlocked session, and the system-wide
WebView2 runtime updater. See plan §15.2.

## Known open items

See [`docs/ledger.md`](docs/ledger.md) and [`docs/stage7/verification.md`](docs/stage7/verification.md).
