# ADR-0007: Stage 3 notch UI decisions

Status: accepted, 2026-10-27. Evidence: [docs/stage3/verification.md](../stage3/verification.md).

## 1. One window size; panels are window regions

**Plan:** "resize-per-state" (§17 Stage 3 task 1, §13.2).
**Decision:** the notch window keeps one size (760 × 520 logical, the largest panel). Each panel
is a `SetWindowRgn` region centred at the top. Outside the region the window is neither drawn nor
hit-tested, so clicks pass through as before.
**Why:** resizing a WebView2 on every open cost tens of milliseconds (measured p95 292 ms
input-to-visual); a region change does not resize the WebView. The visible behaviour is the same.

## 2. Opens are placed by Rust, and the UI is told first

Hotkey and tray opens: Rust sends `panel_request` to the UI **first**, then applies the region and
focus on its own thread. The UI skips its own `set_panel` call (`placed: true`). Closes stay
UI-first (animate out, then shrink). Measured: event delivery 49 → 8 ms. The harness reports the
later of "region applied" and "first frame painted", so the overlap cannot hide a slow region.

## 3. Hotkey fallback chords

**Plan:** Ctrl+Alt+Space / D / P / K, configurable.
**Decision:** each hotkey may list alternates tried in order when the chord is already owned. The
toggle falls back to **Ctrl+Alt+N**. The UI states which chord is active. On the dev machine
another app owned Ctrl+Alt+Space, so without this the notch could not be opened by keyboard.
User-chosen hotkeys arrive with the settings window.

## 4. Every visible window of the process is excluded from capture

The toolkit creates a visible helper window that draws nothing. It is excluded too, so "every
visible Tessara window has `WDA_EXCLUDEFROMCAPTURE`" holds and the verifier can check exactly that.

## 5. One persistent WinRT (MTA) worker thread for rung-3 capture and OCR

A new thread with COM init/uninit per capture crashed on the second capture in a process. One
long-lived worker thread that never leaves the MTA fixes it (regression test included).

## 6. Test hooks

- `TSR_DATA_DIR`: use another data directory (synthetic stores for UI checks).
- `TSR_START_PANEL=expanded`: open the panel after start (capture-exclusion check).
- `TSR_FRAME_LOG`: frame and latency numbers only.

These change no security behaviour: the store is still encrypted, capture still starts paused,
and exclusion is unaffected.

## 7. Browser preview with a mock IPC (dev builds only)

`ui/src/dev/mock.ts` stands in for Tauri IPC with synthetic data when the UI runs in a plain
browser under `pnpm dev`. The real notch cannot be screenshotted (it is excluded from capture),
so this is how the UI is looked at. Production bundles do not contain it (checked: no mock
strings in `ui/dist`).
