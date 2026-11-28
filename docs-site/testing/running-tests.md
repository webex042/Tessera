# Running Tests

Commands for executing unit, integration, hardware, and evaluation suites.

---

## 1. Fast Unit Tests

Run standard unit tests across all workspace crates:
```powershell
cargo test --workspace
```

---

## 2. Full Integration Suite (Including Ignored Tests)

Certain integration tests (e.g. simulated multi-device GPU ranking and full SQLCipher canary scans) are tagged with `#[ignore]` to keep standard compilation fast. To run all tests:
```powershell
cargo test --workspace -- --include-ignored
```

---

## 3. Physical Hardware Test Suite

To run tests against your machine's real physical GPU (requires installed sidecar runtimes):
```powershell
cargo test -p tsr-infer --test real_hardware -- --ignored --nocapture --test-threads=1
```

---

## 4. Compile-Time Dependency & License Checks

```powershell
# 1. Verify zero network-capable crates reachable outside tsr-net
python tools/check_network_deps.py --locked

# 2. Check for dependency security advisories and banned licenses
cargo deny check
```

---

## 5. Synthetic Evaluation Suite (`tsr-eval`)

```powershell
# Run all model-free pipeline checks
cargo run -p tsr-evals --bin tsr-eval -- all

# Run prompt injection defense against a simulated compromised model
cargo run -p tsr-evals --bin tsr-eval -- injection
cargo run -p tsr-evals --bin tsr-eval -- injection --corpus tests/fixtures/injection/unseen_v3.json

# Run search benchmark with real E5 ONNX embedder or fast hash baseline
cargo run -p tsr-evals --bin tsr-eval -- search --model D:\models
cargo run -p tsr-evals --bin tsr-eval -- search

# Run real-model evaluations under hardware-calibrated iGPU beta profile
cargo run -p tsr-evals --bin tsr-eval -- ask --model D:\models --split heldout --profile local-igpu-beta
cargo run -p tsr-evals --bin tsr-eval -- extraction --model D:\models --split heldout --profile local-igpu-beta
cargo run -p tsr-evals --bin tsr-eval -- draft --model D:\models --split all --profile local-igpu-beta
cargo run -p tsr-evals --bin tsr-eval -- scorecard
```
