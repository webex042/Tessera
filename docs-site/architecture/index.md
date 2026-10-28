# System Architecture

Tessera is engineered around strict boundaries separating the user interface, host orchestration, hardware execution, and persistent storage.

---

## Architecture Chapters

- **[High-Level Architecture](/architecture/high-level)** — Layered subsystem diagram from the desktop UI to physical hardware backends.
- **[Component Breakdown](/architecture/components)** — Detailed responsibility matrix across the application shell, supervisor, and storage engines.
- **[Startup & Runtime Lifecycle](/architecture/lifecycle)** — End-to-end execution flow from cold boot and key unwrapping to graceful shutdown.
- **[Process Isolation Model](/architecture/process-model)** — Win32 Job Objects, environment stripping, and loopback TCP authentication.
- **[Data Flow](/architecture/data-flow)** — The path from desktop window changes through acquisition, redaction, and database persistence.
- **[IPC & Event Dispatch](/architecture/ipc)** — Tauri command protocols, event channels, and content-free error serialization.
- **[Failure & Recovery](/architecture/failure-recovery)** — Fault tolerance models, supervisor restart budgets, and crash handling.
- **[Dependency Boundaries](/architecture/dependency-boundaries)** — Strict crate dependency graph and compile-time network isolation rules.

---

## Architectural Principles at a Glance

```
┌────────────────────────────────────────────────────────────────────────┐
│                        ARCHITECTURAL SUMMARY                           │
├──────────────────────────┬─────────────────────────────────────────────┤
│ Decoupled Compute        │ Host binary compiles with zero GPU SDK      │
│                          │ dependencies; models run in pinned sidecars │
├──────────────────────────┼─────────────────────────────────────────────┤
│ Crash Resilient          │ Inference worker crashes are trapped in Job │
│                          │ Objects; host app never goes down on OOM    │
├──────────────────────────┼─────────────────────────────────────────────┤
│ Deterministic Validation │ Small models propose structured JSON;       │
│                          │ Rust verifies evidence quotes before save   │
├──────────────────────────┼─────────────────────────────────────────────┤
│ Cryptographic Air-Gap    │ Entire memory lives in SQLCipher (DPAPI);   │
│                          │ network code isolated to single crate       │
└──────────────────────────┴─────────────────────────────────────────────┘
```
