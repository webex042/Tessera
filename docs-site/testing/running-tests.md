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

# Run search benchmark (recall@5, precision@5, MRR, nDCG@10)
cargo run -p tsr-evals --bin tsr-eval -- search
```
