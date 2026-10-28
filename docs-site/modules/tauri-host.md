# Desktop Shell (`src-tauri`)

The desktop application shell is built on Tauri 2 (`tauri = "=2.12.5"`). It owns the native Win32 window message loop, system tray integration, global hotkey registration, and IPC command routing.

---

## 1. Native Window Management

Tessera renders its primary user interface inside a borderless, transparent floating window positioned at the top center of the user's primary monitor.

### Panel State Machine
The window smoothly transitions between distinct visual configurations defined in `src-tauri/src/window.rs`:

```rust
pub enum Panel {
    Pill,      // Width: 120px, Height: 32px (Resting top glance)
    Peek,      // Width: 380px, Height: 180px (Hover preview)
    Expanded,  // Width: 580px, Height: 640px (Main interactive panel)
    Draft,     // Width: 520px, Height: 360px (Focused ghost reply assistant)
}
```

### Multi-Monitor & DPI Awareness
Window positioning dynamically recalculates bounds across display events:
- Listens for `WM_DISPLAYCHANGE` and `WM_DPICHANGED`.
- Identifies the primary monitor via `MonitorFromWindow`.
- Normalizes coordinates to physical device pixels, ensuring crisp borders without scaling artifacts.

---

## 2. Capture Exclusion (`WDA_EXCLUDEFROMCAPTURE`)

To prevent Tessera's floating interface from recording itself or appearing in third-party screen sharing applications:
```rust
// Implemented via tsr-win::display
SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
```
- **Windows Graphics Capture (WGC):** The window is rendered as transparent pixels.
- **GDI BitBlt:** Screen grabs capture the underlying desktop rather than the notch.
- **Verified Result:** Automated pixel testing (`tools/verify-capture-exclusion.ps1`) confirms 100% absence in both expanded and pill modes.

---

## 3. Focus Restoration (`return_focus`)

When the user activates an interactive panel (e.g., Draft Assistant via hotkey), keyboard focus temporarily transfers to the notch. To prevent typing from vanishing into a hidden window when the notch is dismissed:
- The shell records the previously focused window handle (`return_focus: Mutex<Option<isize>>`).
- When the panel closes, the shell explicitly restores focus via Win32 `SetForegroundWindow`.
- Verified in 20/20 test trials (`docs/stage3/verification.md`).
