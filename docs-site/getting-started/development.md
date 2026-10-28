# Development Environment

This document describes how to configure your IDE and development workflow when contributing to or debugging Tessera.

---

## Workspace Structure

Tessera is organized as a Cargo workspace with members linked at the repository root:

```toml
# Cargo.toml
[workspace]
resolver = "3"
members = [
    "crates/tsr-core",
    "crates/tsr-win",
    "crates/tsr-store",
    "crates/tsr-retrieval",
    "crates/tsr-guard",
    "crates/tsr-infer",
    "crates/tsr-capture",
    "crates/tsr-net",
    "crates/tsr-pipeline",
    "crates/tsr-draft",
    "src-tauri",
    "bench/uia-probe",
    "evals/runner",
]
```

---

## Recommended Editor Configuration

### VS Code / Cursor Settings
Install the following extensions:
- **rust-analyzer:** Core Rust language intelligence.
- **Tauri:** Tauri command and IPC inspection.
- **Tailwind CSS IntelliSense:** Class completion in `ui/`.

Recommended `.vscode/settings.json`:
```json
{
  "rust-analyzer.cargo.features": ["custom-protocol"],
  "rust-analyzer.cargo.buildScripts.enable": true,
  "rust-analyzer.check.command": "clippy",
  "editor.formatOnSave": true
}
```

---

## Development Mode Workflows

### 1. Standalone Frontend Preview (Fast UI Iteration)
When designing panels, views, and animations in `ui/`, you can run Vite independently without compiling the Rust backend. A mock IPC bridge simulates Tauri commands:
```powershell
cd ui
pnpm dev
```
Open `http://localhost:5173` to test the floating notch interface with synthetic state data.

### 2. Full Desktop Application in Development
To run the native desktop shell with hot-reloading:
```powershell
# From repository root:
pnpm build:ui
cargo run -p tessara --features custom-protocol
```

### 3. Inspecting Hardware Detection
To check how Tessera evaluates your machine's graphics adapters without launching the full desktop shell:
```powershell
cargo run -p tsr-infer --bin tsr-hw
# Or for structured JSON output:
cargo run -p tsr-infer --bin tsr-hw -- --json
```

### 4. Running the Model Manager CLI
Test model verification, existing file indexing, or download plans:
```powershell
cargo run -p tsr-net --bin tsr-models -- check
```
