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

## 2. Model File Options

Tessera requires the primary vision-language model weights:
- **Language & Vision Core:** `Qwen3-VL-4B-Instruct-Q4_K_M.gguf` (2.33 GiB).
- **Vision Projector:** `mmproj-Qwen3-VL-4B-Instruct-F16.gguf` (0.78 GiB).
- **Embedding Model:** `multilingual-e5-small` ONNX format (0.46 GiB).

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
- Passwords and secrets are redacted before persistence in SQLCipher.
