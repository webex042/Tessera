# Privacy: what Tessara keeps, sends and cannot control

Plain-language inventory of the code as of 2026-10-28 (Stage 7 review). Every statement points
to the test or measurement behind it. Where something is not yet tested, it says so.

## What Tessara keeps (all on your PC, in `%LOCALAPPDATA%\Tessara`)

| Data | Where | Protection | Evidence |
|---|---|---|---|
| Text read from windows you use (after redaction), with app name, window title, time | `tessara.db` | whole-database encryption (SQLCipher, AES-256); key in `key.bin`, wrapped with your Windows login (DPAPI) | store tests; canary scan: no plaintext on disk |
| Facts (tasks, commitments, decisions, questions…) with the quote they came from | `tessara.db` | same | — |
| Hour/day/week/month summaries, morning briefs | `tessara.db` | same | — |
| Content-free logs and audit events (counts, codes, lengths — never text) | `tessara.db`, `logs\` | no content to protect | tracing allowlist test; diagnostics privacy test |
| Model files you approved | `models\` | checked against pinned SHA-256 digests | `tsr-models locate --verify`; start-up digest check |
| Diagnostics you choose to save | `diagnostics\` | content-free (counts, states, versions) | `src-tauri/tests/diagnostics.rs` |

**Never kept:** screenshots, audio, keystrokes, clipboard contents, drafts (only "inserted / copied
/ discarded" with a length). Screen images exist in memory only, for reading text.

**Removed before storage or model input:** passwords and password fields, OTPs, card numbers,
Aadhaar (incl. VID), PAN, passport and voter ID numbers, bank account numbers and IFSC, UPI IDs,
API keys, tokens and private keys (355 test cases, all redacted; 0.4 % false positives).

**Never captured:** windows on your blocklist (password managers, banking and login pages by
default), private/incognito browser windows (and any browser window whose private state can't be
read), and everything while capture is paused. Capture starts paused.

## What Tessara sends

| Component | Behaviour | Evidence |
|---|---|---|
| Tessara's own code | no outbound connections; the model runs as a local `llama-server` on 127.0.0.1 with a port-owner check | `tools/netmon.ps1` (see `docs/stage7/verification.md` §D) |
| Rust dependencies | no network-capable crate in the build except `tsr-net` (model downloads you approve) | `tools/check_network_deps.py`, `cargo deny` |
| WebView2 (the UI engine) | background networking, SmartScreen and proxy discovery switched off; **Stage 0 found one HTTPS connection to a Microsoft address per run** from the WebView2 runtime (ADR-0002); still open | `docs/stage0/network-test.md` |
| WebView2 runtime updater (`msedgeupdate`) | a Windows service shared by every app that uses WebView2; Tessara does not control it | — |
| Updater | compiled out of every build (dependency advisories, ADR-0003) | `cargo deny --features tessara/updater` |

Until ADR-0002 is closed, "nothing leaves your PC" is **not** claimed for the UI process.

## What Tessara does with your screen and keyboard

- The notch is hidden from screenshots and screen sharing (verified on pixels with Windows
  Graphics Capture and GDI; not yet checked inside Teams/Zoom/OBS themselves).
- Drafts are pasted only after you press Insert, only into the field that was focused when you
  asked, and never into a password field; Tessara never presses Enter or Send. The clipboard is
  restored afterwards and the draft is marked to stay out of clipboard history and cloud sync
  (restore verified; history exclusion untested here because clipboard history is off).
- Nothing is marked done or deleted from your open loops without your confirmation.

## Outside Tessara's control

The Windows pagefile and hibernation file (memory may be written there by Windows; not scanned:
needs elevation), malware running as your user, physical access to an unlocked PC, and the
system-wide WebView2 updater. See plan §15.2.

## Not yet tested

The 24-hour network and canary runs (S1, S2) — shorter runs passed; the named apps for capture
exclusion and draft insertion; clipboard history and cloud-clipboard exclusion.
