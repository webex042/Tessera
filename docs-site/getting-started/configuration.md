# Configuration

Tessera stores persistent application and runtime settings locally in `%LOCALAPPDATA%\Tessara\config.json`.

---

## Configuration Schema

Below is an example of the default configuration generated during initialization:

```json
{
  "inference": {
    "backend": "auto",
    "device": "",
    "gpu_layers": -1,
    "allow_fallback": true,
    "idle_unload_min": 10
  },
  "capture": {
    "cadence_active_s": 5,
    "quality_threshold": 0.55
  },
  "storage": {
    "retention_days": 90
  }
}
```

---

## Detailed Setting Definitions

### Inference Configuration (`inference`)

- **`backend` (string, default: `"auto"`)**
  - Specifies the preferred hardware backend.
  - Allowed values: `"auto"`, `"cuda"`, `"rocm"`, `"vulkan"`, `"sycl"`, `"openvino"`, `"metal"`, `"opencl"`, `"cpu"`.
  - When set to `"auto"`, Tessera probes available devices and follows the vendor arbitration hierarchy.
  - *Note:* Specifying an unverified backend pairing logs a warning.

- **`device` (string, default: `""`)**
  - Selects a specific GPU device index or substring match.
  - Examples: `"CUDA0"`, `"Vulkan1"`, `"0"`, `"RTX 4050"`.

- **`gpu_layers` (integer, default: `-1`)**
  - Number of model tensor layers to offload to GPU VRAM.
  - `-1`: Calculate full offload automatically based on reported free memory.
  - `0`: Execute entirely on CPU.
  - `> 0`: Offload exactly this many layers.

- **`allow_fallback` (boolean, default: `true`)**
  - If `true`, Tessera automatically cascades down to alternative backends (and finally CPU) if the primary choice fails to initialize.
  - If `false`, the inference supervisor refuses to start and raises an error if the selected backend is unavailable.

- **`idle_unload_min` (integer, default: `10`)**
  - Minutes of inactivity before the model is evicted from VRAM to preserve battery and free GPU resources.
  - When set to `0`, the model remains resident permanently.
  - *Note:* On AC power, the worker remains resident by default.

### Capture Configuration (`capture`)

- **`cadence_active_s` (integer, default: `5`)**
  - Polling interval in seconds when the user is actively typing or interacting with the focused window.
  - On battery power, this interval is multiplied by $3\times$ (15 seconds).

- **`quality_threshold` (float, default: `0.55`)**
  - The OCR heuristic score below which Tessera drops to Rung 3 (multimodal Vision-Language crop).
  - Evaluates printable character ratios, word dictionary plausibility, and line-density consistency.

### Storage Configuration (`storage`)

- **`retention_days` (integer, default: `90`)**
  - Maximum age of captured items and window sessions.
  - Items older than this limit are pruned during background maintenance, cascading through associated full-text index records and embedding vectors.

---

## LLM Provider Environment Variables (`local` vs `groq`)

By default, Tessera runs strictly offline against the local `llama-server` sidecar (`TSR_LLM_PROVIDER=local`). When compiled with the optional `--features groq` Cargo flag, you can explicitly opt into Groq inference without downloading the local ~2.5 GB GGUF model:

| Variable | Default | Purpose |
| :--- | :--- | :--- |
| `TSR_LLM_PROVIDER` | `local` | Set to `groq` to activate the Groq backend (`local` keeps local sidecar inference as strict default). |
| `GROQ_API_KEY` | *(unset)* | Groq API key loaded from process env, `.env`/`.env.local`, or DPAPI-encrypted `%LOCALAPPDATA%\Tessara\groq_key.bin`. |
| `GROQ_MODEL` | `openai/gpt-oss-20b` | Model sent to Groq Chat Completions (`https://api.groq.com/openai/v1/chat/completions`). |
| `GROQ_REASONING_EFFORT` | `low` | Sets `reasoning_effort` (`low` \| `medium` \| `high`) with `include_reasoning: false` on `gpt-oss` models. |
| `TSR_GROQ_BASE_URL` | `https://api.groq.com/openai/v1` | Optional base URL override (HTTPS required outside loopback tests). |

### Free-Tier Quota Protection

In `groq` mode, `tsr-store::quota::QuotaTracker` persists usage counters in SQLCipher (`settings` key `groq_quota_v1`) and stays under conservative free-tier ceilings (`20 RPM`, `700 RPD`, `6,000 TPM`, `150,000 TPD`):
- Background `extract` and `rollup` jobs yield when daily usage reaches `70%` or minute usage reaches `65%`, falling back to extractive summaries and preserving remaining tokens for interactive `ask` and `draft` calls.
- Image/VLM requests return `UnsupportedModality` on text-only `openai/gpt-oss-20b`.
- End users can store or remove their Groq API key (`groq_key.bin`, Windows DPAPI), toggle Local vs. Groq BYOK, and download local ONNX/GGUF files interactively inside **AI Setup**, plus configure retention windows (`7`–`365` days), capture blocklist rules, and full memory wipes inside **Privacy Controls** (`privacy_settings_v1`).
