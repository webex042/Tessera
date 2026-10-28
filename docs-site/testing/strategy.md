# Testing Strategy

Tessera employs a four-tiered testing pyramid designed to validate systems reliability without requiring constant access to high-end GPU clusters.

---

## The Four Testing Tiers

```
       ▲
      / \     Tier 4: Physical Hardware & Real Models (Manual / Nightly)
     /   \    Tier 3: Evals Runner with Gold Oracles (tsr-eval)
    /     \   Tier 2: Simulated Adapter Suites & Mock Servers
   /_______\  Tier 1: Fast Unit Tests & Pure Algorithmic Verification
```

### Tier 1: Pure Unit Tests (Fast, CI Default)
- **Scope:** State machines, parsers, regex redaction, GGUF binary headers, and mathematical SimHash.
- **Environment:** Pure Rust, no Windows dependencies, runs on any Linux/macOS/Windows runner in seconds.

### Tier 2: Simulated Integration Tests
- **Scope:** Hardware candidate ranking (`selection.rs`), supervisor backoff logic, and prompt injection defenses.
- **Mocking:** Tests provide mocked adapter lists (e.g., simulating discrete NVIDIA RTX 4090 + integrated Intel Xe) and mocked `llama-server --list-devices` outputs to test arbitration logic without hardware.

### Tier 3: Synthetic Evaluation Runner (`tsr-eval`)
- **Scope:** Extraction F1 scoring, search retrieval metrics (MRR, nDCG@10), and backpressure queue draining.
- **Oracle Mode:** Uses `--model oracle` to feed gold facts back into the pipeline, measuring pipeline accuracy and evidence parsing independent of model capabilities.

### Tier 4: Real-Hardware Validation
- **Scope:** Actual token generation and VRAM allocation on physical GPUs.
- **Suite:** `crates/tsr-infer/tests/real_hardware.rs` tests real Vulkan and CPU execution on physical silicon.
