# CLI Tools Reference

Tessera includes standalone command-line tools for hardware diagnostics, model management, and offline evaluation.

---

## 1. Hardware Inspection (`tsr-hw`)

Evaluates the local host machine, detects GPU adapters via DXGI, and tests candidate backend arbitration without launching the full application:

```powershell
# Standard human-readable summary
cargo run -p tsr-infer --bin tsr-hw

# Machine-readable JSON output for automated scripting
cargo run -p tsr-infer --bin tsr-hw -- --json

# Force a specific backend and test probing
cargo run -p tsr-infer --bin tsr-hw -- --backend vulkan --device 0

# Test compatibility against a specific local GGUF file
cargo run -p tsr-infer --bin tsr-hw -- --model D:\models\Qwen3VL-4B-Instruct-Q4_K_M.gguf
```

---

## 2. Model Onboarding Manager (`tsr-models`)

Manages model verification, local directory discovery, and approved downloads:

```powershell
# Check current hardware suitability and verification status
cargo run -p tsr-net --bin tsr-models -- check

# Verify existing model files in a local directory
cargo run -p tsr-net --bin tsr-models -- locate "D:\models" --verify

# Review download plan (host, file sizes, licenses)
cargo run -p tsr-net --bin tsr-models -- plan qwen3-vl-4b-instruct-q4_k_m

# Execute approved download to destination directory
cargo run -p tsr-net --bin tsr-models -- fetch qwen3-vl-4b-instruct-q4_k_m --dest "D:\models" --approve
```

---

## 3. Pipeline Evaluation Runner (`tsr-eval`)

Runs automated evaluation suites against synthetic benchmark datasets:

```powershell
# Run all model-free tests (executed in CI)
cargo run -p tsr-evals --bin tsr-eval -- all

# Evaluate prompt injection defense against a simulated compromised model
cargo run -p tsr-evals --bin tsr-eval -- injection

# Evaluate extraction pipeline using gold oracle data
cargo run -p tsr-evals --bin tsr-eval -- extraction --model oracle --split heldout

# Evaluate extraction pipeline against real local model weights
cargo run -p tsr-evals --bin tsr-eval -- extraction --model D:\models --split heldout
```
