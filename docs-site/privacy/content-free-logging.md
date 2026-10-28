# Content-Free Logging (`tsr-core::logging`)

Logging frameworks in desktop applications represent a major privacy leak risk: unhandled exceptions, debug print statements, or trace events frequently capture sensitive user text, passwords, or URLs.

Tessera solves this at the compiler and runtime levels through **content-free logging**.

---

## 1. The `Sensitive<T>` Type

All strings originating from desktop windows, user inputs, or model outputs are wrapped in `Sensitive<T>`:

```rust
pub struct Sensitive<T>(T);

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted>")
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted>")
    }
}
```

Even if a developer writes `tracing::info!("Failed: {:?}", user_input)`, the output printed to the terminal or log file is strictly:
```text
Failed: <redacted>
```

---

## 2. The Allowlist Subscriber Layer

In `crates/tsr-core/src/logging.rs`, Tessera implements a custom `tracing` subscriber layer that discards any event or field that does not appear on a strict compile-time allowlist:

### Allowed Messages (`ALLOWED_MESSAGES`)
Only explicit, content-free event names are permitted:
- `"gpu adapter"`
- `"sidecar started"`
- `"sidecar health check"`
- `"item stored"`
- `"capture paused"`
- `"job preempted"`

### Allowed Fields (`ALLOWED_FIELDS`)
Only non-content identifiers and numeric metrics are retained:
- `index`, `vendor`, `dedicated_mib`, `shared_mib`
- `duration_ms`, `tokens_in`, `tokens_out`
- `error_code`, `status`, `attempt`

Any field containing arbitrary strings or unlisted field names is dropped and counted in an internal drop metric.

---

## 3. Automated Compile-Time Verification

Tessera includes an automated regression test in `crates/tsr-core/tests/logging_allowlist.rs` that scans every source file in the Cargo workspace for `tracing::` invocations, failing the build if an unlisted message string or field name is introduced.
