# Installation & Setup

This guide walks through cloning the repository, installing dependencies, fetching pinned sidecars, and verifying system integrity.

---

## 1. Clone Repository

Clone the project to a local directory:
```powershell
git clone https://github.com/webex042/Tessera.git
cd Tessera
```

---

## 2. Install Frontend Dependencies

Tessera utilizes a pnpm monorepo workspace for frontend packages:
```powershell
pnpm install
pnpm build:ui
```
This compiles the React 19 desktop notch application inside `ui/` to static distribution assets (`ui/dist/`), which Tauri packages into the binary.

---

## 3. Fetch Verified Sidecar Runtimes

Tessera isolates GPU compute into discrete `llama.cpp` sidecar builds. Rather than downloading arbitrary binaries, every downloaded file is hash-checked against the compiled-in manifest `tools/sidecar.lock`:

```powershell
# List available sidecar variants
python tools/fetch_sidecar.py --list

# Fetch the universal CPU fallback build
python tools/fetch_sidecar.py llama_cpp cpu-x64

# Fetch the Vulkan backend (supports AMD, Intel, NVIDIA, and Qualcomm)
python tools/fetch_sidecar.py llama_cpp vulkan-x64
```

For machines with dedicated NVIDIA GPUs, download the CUDA 12.4 build and runtime archive:
```powershell
python tools/fetch_sidecar.py llama_cpp cuda-12.4-x64
python tools/fetch_sidecar.py llama_cpp cudart-12.4-x64
```

---

## 4. Fetch Pinned ONNX Runtime (Embeddings)

Local semantic vector search uses `fastembed` dynamically linked against a pinned local build of ONNX Runtime 1.31:
```powershell
./tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64
```
This places `onnxruntime.dll` into `third_party/onnxruntime/1.31.0/win-x64/`. Tessera's initialization explicitly verifies this file's SHA-256 and refuses to load Windows' older built-in `System32\onnxruntime.dll`.

---

## 5. Verify Build Dependencies

Execute the automated network dependency scanner to ensure no unauthorized crate has introduced network connectivity:
```powershell
python tools/check_network_deps.py --locked
```
Expected output:
```text
PASS (features=default): no network-capable crates reachable from tessara, tsr-capture, tsr-core, tsr-draft, tsr-guard, tsr-infer, tsr-pipeline, tsr-retrieval, tsr-store, tsr-win
```

---

## 6. Run Workspace Test Suite

Verify that all unit and integration tests pass:
```powershell
cargo test --workspace -- --include-ignored
```
This executes tests across all 10 crates, including simulated GPU adapter selection, redaction corpora tests, and SQLCipher encrypted database tests.
