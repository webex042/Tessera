# Tauri IPC Commands & Events

The frontend interface communicates with the native host shell via Tauri 2 command bindings registered in `src-tauri/src/lib.rs`.

---

## Command Signatures

### `get_status`
- **Request Payload:** None.
- **Response Struct:**
```typescript
interface StatusDto {
  paused: boolean;
  model_state: "stopped" | "starting" | "ready" | "backoff" | "failed";
  active_panel: "pill" | "peek" | "expanded" | "draft";
  battery: boolean;
  power_saver: boolean;
  pending_jobs: number;
}
```

### `set_panel`
- **Request Payload:** `{ panel: "pill" | "peek" | "expanded" | "draft" }`
- **Response:** `Result<void, ApiError>`
- **Behavior:** Dynamically repositions and resizes the native Win32 window.

### `toggle_pause`
- **Request Payload:** None.
- **Response:** `Result<boolean, ApiError>` (returns new pause state).

### `search`
- **Request Payload:** `{ query: string, limit: number }`
- **Response Struct:**
```typescript
interface SearchHit {
  item_id: number;
  source_title: string;
  source_kind: string;
  snippet: string;
  timestamp: number;
  score: number;
}
```

### `ask_question`
- **Request Payload:** `{ question: string }`
- **Protocol:** Server-Sent Events (SSE) streaming tokens to the UI.
- **Event Names:**
  - `token`: Streaming token text.
  - `citation`: Verifiable evidence pointer (`{ citation_id: number, quote: string }`).
  - `done`: Final generation summary and timing metrics.

---

## Content-Free Error Contract (`ApiError`)

All IPC failures return structured, content-free error objects:
```typescript
interface ApiError {
  code: string;     // e.g. "unavailable", "invalid", "busy"
  message: string;  // e.g. "error.unavailable"
}
```
Errors physically cannot serialize database queries, raw file paths, or private user strings across the IPC bridge.
