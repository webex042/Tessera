# Stage 3 verification report: notch UI

Date: 2026-10-27. Machine: Windows 11 (10.0.26300), AMD Radeon integrated GPU, one 1920 × 1080
monitor at 150 % scaling. Builds: debug Rust, production UI bundle (`ui/dist`). All
measurements ran on a **throwaway synthetic store** (`tsr-eval seed`, run with `TSR_DATA_DIR`),
never the real profile.

Legend: **REAL** = the real app, OS and hardware; **SIMULATED** = a stand-in;
**UNVERIFIED** = not measured.

## A. What was built

| task (plan §17 Stage 3) | where | summary |
|---|---|---|
| 1. Window mechanics | `src-tauri/src/window.rs`, `tsr-win::display`, `tsr-win::events` | one transparent, always-on-top, tool window at a fixed 760 × 520 (logical); each panel is a **window region** centred at the top (`SetWindowRgn`), so switching panels never resizes the WebView. Monitor under the cursor, physical-pixel placement (mixed DPI), re-place on `WM_DISPLAYCHANGE` and scale change, topmost re-asserted on display change. Focus only for user-opened panels; focus returns to the previous app on close. Hidden while a fullscreen game or presentation is in front |
| 2. Capture exclusion + verifier | `tsr-win::display::{exclude_from_capture, exclude_own_windows}`, `tools/verify-capture-exclusion.ps1`, `tsr-capture` bin `tsr-verify-exclusion` | `WDA_EXCLUDEFROMCAPTURE` on every visible window of the process, read back; the UI warns if it did not take. Verifier: magenta backdrop under the notch, WGC monitor capture **and** GDI BitBlt, two controls |
| 3. Shells and views | `ui/src/App.tsx`, `ui/src/notch/*`, `src-tauri/src/views.rs` | pill, peek, expanded, draft shells; Today, Open loops, Search, Timeline (day) on persisted data through typed commands; fact status changes use a single-use Rust approval token (two-step Done/Drop) |
| 4. Hotkeys, tray, a11y, themes | `src-tauri/src/lib.rs`, `ui/src/index.css`, `ui/src/notch/Expanded.tsx` | `RegisterHotKey` (no hooks, D11) with fallback chords; tray menu (open / pause / quit); tablist with Arrow/Home/End, Esc to close; reduced motion; forced-colors (high contrast); light/dark from the system |
| 5. Typed IPC and states | `ui/src/ipc/{types,api}.ts`, `src-tauri/src/views.rs` | hand-written types mirroring Rust; errors are `{code, message}` codes only; every view has loading, empty and error states |
| dev tooling | `ui/src/dev/mock.ts`, `.claude/launch.json`, `tsr-eval seed`, `tools/measure-notch.ps1` | browser preview with synthetic data (dev builds only, absent from `ui/dist`); synthetic store seeding; perf and focus harness |

Decisions: [ADR-0007](../adr/0007-stage3-notch-ui.md).

## B. Exit-criteria scorecard

| criterion | threshold | measured | evidence | status |
|---|---|---|---|---|
| Notch absent from captures | absent from Teams, Zoom, OBS, Snipping Tool, Xbox Game Bar | **REAL, API level:** pill and expanded panel are 100 % absent from Windows.Graphics.Capture monitor frames (the API Snipping Tool, Game Bar, Teams on Windows 11 and OBS "Windows 10 1903+" capture use) and from GDI BitBlt frames (OBS "BitBlt", older recorders). Controls: an unexcluded test window is 100 % visible in the same frames; the same window with exclusion is 100 % hidden. Affinity reads back `0x11` on both visible windows | `evals/results/capture_exclusion_{pill,expanded}.json` via `tools/verify-capture-exclusion.ps1` | **PARTIAL**: the five named apps were not run (Teams/Zoom need accounts and a live share; OBS is not installed). Checklist in §D |
| Expand/collapse frame rate | 60 fps | 1,229 animation frames over 20 open/close cycles: p50 16.7 ms, p95 16.8 ms, max 33.3 ms; 0.16 % of frames over 17.7 ms | `evals/results/notch_perf.json` via `tools/measure-notch.ps1 -Cycles 20` | **PASS** |
| Input-to-visual | ≤ 100 ms | 20 hotkey opens: p50 33 ms, p95 64 ms, max 64 ms (later of "region applied" and "first animation frame painted", from when `WM_HOTKEY` reached Rust) | same | **PASS** (the keyboard → `WM_HOTKEY` hop is not measurable from user mode and is not included) |
| No focus stealing during peeks | typing elsewhere continues | at start-up a peek was showing (another app owns Ctrl+Alt+Space, so the UI says so) and the foreground window was unchanged; hotkey opens took focus 20/20 (intended: user action); after closing, focus was back in the previous app 20/20 | same | **PASS** |
| Placement after monitor plug/unplug and DPI change | correct placement | handlers wired (`WM_DISPLAYCHANGE` → re-place + re-assert topmost; `ScaleFactorChanged` → re-place); placement maths unit-tested for negative origins, monitors above the primary, 100/125/150 % | `src-tauri/src/window.rs` tests | **PARTIAL**: one monitor here; plugging a monitor or changing the scale needs the owner (§D) |

## C. Bugs found and fixed while measuring

1. **Second WGC capture crashed the process** (access violation). Every rung-3 VLM job would have
   taken the app down after the first. Cause: a fresh MTA thread with COM init/uninit per
   capture. Fix: one persistent WinRT worker thread (`tsr-capture::ocr::on_mta`). Regression
   test: `crates/tsr-capture/tests/wgc_repeat_real.rs` (3 captures in one process, REAL).
2. **Default hotkey taken on this machine.** Ctrl+Alt+Space was already registered by another
   app (`ERROR_HOTKEY_ALREADY_REGISTERED`), so the toggle never worked. Fix: alternates per
   hotkey (toggle falls back to Ctrl+Alt+N); the UI shows which chord is active. REAL test:
   `crates/tsr-win/tests/hotkey_real.rs`.
3. **Focus stayed in the hidden notch after closing**, so typing went nowhere. Fix: remember the
   other app's window on open and return focus on close (20/20 measured).
4. **Input latency p95 292 ms** in the first measurement. Fixes, each measured: fixed window +
   regions instead of WebView resizes (p95 118 ms); keep the expanded panel mounted (125 ms,
   frames 0 late); send the UI event *before* the window work instead of after, since the event
   was queued behind it on the main thread (event delivery 49 → 8 ms; p95 31–64 ms over four
   runs). A Chromium flag (`CalculateNativeWinOcclusion`) was tried, showed no gain, and was
   reverted.
5. A toolkit helper window ("Tao Thread Event Target") was visible and not excluded. It draws
   nothing, but all visible windows are now excluded so the guarantee is simple.
6. Capture-exclusion warning could fire before the UI listened; the UI now asks on load.
7. `GetDIBits` colour-table overrun guarded in the verifier (full table reserved).

## D. Owner checklist (needs a person or accounts)

1. **App captures.** With Tessara running: start a Teams or Zoom screen share, an OBS display
   capture, a Snipping Tool screenshot and a Game Bar recording; confirm the notch is absent
   from each result. Expected from the API-level evidence above: absent.
2. **Monitor and DPI.** Plug or unplug a second monitor and change the scale (Settings → Display)
   while the pill is visible; confirm it re-centres at the top of the monitor under the cursor
   at the right size.

## E. Not done in Stage 3 (and where it goes)

- Ask tab: Stage 6. Draft panel content: Stage 5 (shell only, honest empty state).
- Timeline week/month views: Stage 4 (rollups).
- Settings window for user-chosen hotkeys: later; fallback chords cover collisions now.
- Native notch fallback (plan D1): not needed; no WebView2 transparency glitches seen.
