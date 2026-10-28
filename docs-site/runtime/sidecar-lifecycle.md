# Sidecar Execution (`llama-server`)

Tessera runs an isolated `llama-server.exe` sidecar process compiled from pinned upstream `llama.cpp` (release b11900).

---

## 1. Hardened Command-Line Flags

When spawning `llama-server`, Tessera passes explicit arguments checked against the b11900 CLI specification to enforce security and resource boundaries:

```bash
llama-server.exe \
  --host 127.0.0.1 \
  --port <random_port> \
  --api-key <random_32byte_token> \
  -m <models>/Qwen3VL-4B-Instruct-Q4_K_M.gguf \
  --mmproj <models>/mmproj-Qwen3VL-4B-Instruct-F16.gguf \
  -ngl 99 \
  -c 8192 \
  -np 1 \
  -fa on \
  -ctk q8_0 \
  -ctv q8_0 \
  --jinja \
  --no-webui \
  --offline \
  --no-agent \
  --no-ui-mcp-proxy \
  --no-slots \
  --no-cors-credentials \
  --cors-origins http://tessara.invalid \
  --fit off \
  --cache-ram 0 \
  --reasoning off \
  -t <physical_cores/2>
```

### Rationale Behind Critical Flags
- **`--no-webui` & `--no-slots`:** Disables the built-in static HTTP web interface and debug slots endpoints.
- **`--offline` & `--no-agent`:** Strips out experimental autonomous web search, Hugging Face repo pulling, and Model Context Protocol (MCP) tooling.
- **`--fit off`:** Prevents `llama.cpp` from silently downsampling context length or quantizations if memory is tight; Tessera requires predictable memory budgeting.
- **`-np 1`:** Constrains concurrency to exactly one slot, preventing multiple requests from dividing VRAM or fragmenting KV caches.
- **`-ctk q8_0 -ctv q8_0`:** Quantizes the key-value cache to 8-bit precision, cutting KV cache VRAM consumption in half while preserving numerical fidelity.

---

## 2. Health Monitoring & Hang Detection

Once spawned, the `tsr-infer::supervisor` state machine monitors sidecar health:
- **Cold Boot Timeout:** Allows up to 90 seconds for large model weights to map into GPU memory before declaring a startup timeout.
- **Periodic Health Polling:** Polls `GET /health` with an `Authorization: Bearer <key>` header.
- **3-Miss Hang Detection:** If three consecutive health checks fail to respond or time out, the supervisor declares the worker hung, issues a Win32 `TerminateProcess`, and initiates backoff recovery.
