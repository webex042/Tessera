# Rust API Reference

This page highlights core public interfaces, data structures, and traits across the primary Rust crates.

---

## 1. `tsr-core`

### `Sensitive<T>`
Wraps private data to prevent accidental string leakage:
```rust
pub struct Sensitive<T>(pub T);

impl<T> Sensitive<T> {
    pub fn new(val: T) -> Self;
    pub fn into_inner(self) -> T;
    pub fn as_inner(&self) -> &T;
}
```

---

## 2. `tsr-store`

### `open_encrypted`
Opens a SQLCipher database with security PRAGMAs:
```rust
pub fn open_encrypted(
    path: &std::path::Path, 
    key: &[u8; 32]
) -> Result<rusqlite::Connection, StoreError>;
```

---

## 3. `tsr-infer`

### `detect`
Enumerates physical adapters, validates sidecar lockfile manifests, and probes devices:
```rust
pub fn detect(
    third_party: &std::path::Path,
    adapters: &dyn AdapterSource,
    probe: &dyn BackendProbe
) -> HardwareReport;
```

### `SchedulerCore`
State machine coordinating priority queues and stream-drop preemption:
```rust
pub struct SchedulerCore { ... }

impl SchedulerCore {
    pub fn new(config: SchedConfig) -> Self;
    pub fn enqueue(&mut self, job: Job) -> Result<JobId, SchedError>;
    pub fn poll_next(&mut self) -> Option<DispatchAction>;
    pub fn on_stream_chunk(&mut self, id: JobId, chunk: &[u8]);
    pub fn on_complete(&mut self, id: JobId, outcome: JobOutcome);
}
```

---

## 4. `tsr-guard`

### `redact_secrets`
Applies regex and algorithmic checksums to clean sensitive data:
```rust
pub fn redact_secrets(input: &str) -> RedactionResult {
    // Returns redacted String and metadata count of scrubbed items
}
```
