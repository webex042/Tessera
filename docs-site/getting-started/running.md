# Running the Application

This guide covers how to launch Tessera, interact with the floating desktop notch, and observe background workers.

---

## Launching Tessera

To start Tessera in development mode:
```powershell
cargo run -p tessara --features custom-protocol
```

Upon startup, the following initialization sequence occurs:
1. **Environment Cleansing:** Native Win32 hooks clear WebView2 override variables before any webview instance is spawned.
2. **Master Key Derivation:** The encrypted key file `%LOCALAPPDATA%\Tessara\key.bin` is read and unwrapped using Windows DPAPI (`CryptUnprotectData`).
3. **Database Initialization:** SQLCipher opens `tessara.db` with PRAGMAs (`cipher_memory_security = ON`, `WAL` journal mode) and runs database schema migrations.
4. **Gatekeeper Startup:** Windows event hooks (`SetWinEventHook`) register for foreground focus changes and title updates.
5. **Notch Display:** The floating transparent notch appears at the top center of the primary display in the compact **Pill** state.

::: info Pause State Default
By design, Tessera starts in the **Paused** state upon fresh installation. No screen captures or background worker threads execute until you complete onboarding and explicitly press **Start** or press the pause hotkey.
:::

---

## Interacting with the Floating Notch

The interface operates as a persistent, hardware-accelerated overlay:

```
[ Pill State ] ──► (Hover) ──► [ Peek State ] ──► (Click) ──► [ Expanded Panel ]
```

- **Pill State (120 × 32 px):** Resting state pinned to the top screen edge. Displays live status indicators (active, paused, background processing, or error).
- **Peek State:** Expands briefly on hover to display the current active session context and immediate open loops without stealing keyboard focus.
- **Expanded Panel (580 × 640 px):** Opens upon click or shortcut activation (`Alt + Space`), exposing the primary navigation tabs:
  - **Today:** Timeline of active work sessions and hour rollups.
  - **Open Loops:** Action items, commitments, and unanswered questions extracted from context.
  - **Search:** Instant hybrid search combining FTS5 keyword matching and vector similarity.
  - **Ask:** Conversational question answering grounded in stored memories with verifiable citations.
  - **Diagnostics:** Content-free system telemetry, memory usage, and sidecar status.

---

## Global Hotkeys

Tessera registers system-wide hotkeys via Win32 `RegisterHotKey`:

| Hotkey | Default Combination | Action |
|---|---|---|
| **Toggle Notch** | `Alt + Space` | Expand or collapse the main floating notch panel. |
| **Instant Pause** | `Alt + Shift + P` | Instantly toggle screen capture pause state. |
| **Draft Assistant** | `Alt + Shift + D` | Request an in-place reply draft for the currently focused field. |

*Note:* If another running application already owns a shortcut, Tessera automatically cascades to secondary fallback chords documented in the Diagnostics view.
