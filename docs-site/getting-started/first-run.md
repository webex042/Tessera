# First-Run Workflow

When Tessera launches for the first time, it guides the user through hardware verification and model onboarding.

---

## 1. Onboarding Hardware Detection

Tessera runs an initial inspection without downloading weights or loading vendor drivers:
```powershell
cargo run -p tsr-net --bin tsr-models -- check
```
The check evaluates:
1. **OS Graphics Adapters:** Lists discrete and integrated GPUs.
2. **Verified Builds:** Checks installed llama.cpp sidecar builds against `tools/sidecar.lock`.
3. **Backend Devices:** Probes `--list-devices` on available builds.
4. **Verdict:**
   - `GPU (Vulkan)` / `GPU (CUDA)`: Sufficient VRAM detected.
   - `CPU only (slow mode)`: No GPU detected; background extraction will wait for AC power and idle time.

---

## 2. In-App AI Setup & Model File Options

End users configure inference directly inside the desktop **AI Setup** view:

- **Option 1 — Local Sidecar (`Qwen3-VL-4B-Instruct`):** Fully offline inference on GPU (Vulkan) or CPU. Requires downloading the model weights (`2.33 GiB` LLM + `0.78 GiB` mmproj, or text-only embedder `0.46 GiB`):
  - **Language & Vision Core:** `Qwen3VL-4B-Instruct-Q4_K_M.gguf` (2.33 GiB).
  - **Vision Projector:** `mmproj-Qwen3VL-4B-Instruct-F16.gguf` (0.78 GiB).
  - **Embedding Model:** `multilingual-e5-small` ONNX format (0.46 GiB) for semantic `VectorDb` search.
- **Option 2 — Groq BYOK (`openai/gpt-oss-20b`):** Skip the 3.1 GiB local GGUF download by pasting your own Groq API key inside **AI Setup** (stored encrypted in Windows DPAPI `groq_key.bin`). Outbound prompts are scrubbed via `tsr-guard` PII masking before leaving your PC.

Users can also inspect and pre-stage models via CLI:

### Option A: Use Files You Already Have
If you have already downloaded these models, point Tessera to your directory:
```powershell
cargo run -p tsr-net --bin tsr-models -- locate "D:\models" --verify
```
Tessera hashes each file (SHA-256) against `tools/models.lock`. It never copies, renames, or modifies your local files.

### Option B: Download with Explicit Approval
Tessera will never initiate a background download without user consent. You can view the download plan and approve the fetch:
```powershell
# Review host, files, and sizes
cargo run -p tsr-net --bin tsr-models -- plan qwen3-vl-4b-instruct-q4_k_m

# Execute approved download to destination
cargo run -p tsr-net --bin tsr-models -- fetch qwen3-vl-4b-instruct-q4_k_m --dest "D:\models" --approve
```

---

## 3. The Smoke Test

Once model files are verified, Tessera executes a lightweight smoke-test prompt to verify device execution:
- Spawns `llama-server` in a suspended Job Object.
- Sends a minimal single-token prompt via loopback HTTP (`127.0.0.1`).
- Upon success, flags the backend as **`inference-tested`**.

---

## 4. Unpausing Capture

The notch interface remains in a **Paused** state until you click **Start** or press `Alt + Shift + P`. Once unpaused:
- The Gatekeeper monitors foreground window changes.
- Sessions aggregate active typing and document reading.
- Passwords, PII, and secrets are masked (`tsr-guard`) before persistence in SQLCipher and AES-256-GCM `VectorDb` (`tessera_vectors.enc`).
- Open **Privacy Controls** at any time to adjust retention pruning (`7`–`365` days), edit app blocklists, delete individual captured items, or run a token-confirmed wipe.
