# Production Build

This document covers compiling Tessera for optimized release deployment.

---

## 1. Compile Optimized Frontend UI

Ensure all React assets are compiled to static bundles:
```powershell
pnpm build:ui
```
This runs `tsc --noEmit` and `vite build`, writing the output to `ui/dist/`.

---

## 2. Compile Release Rust Binary

Compile the workspace in release mode with link-time optimization (LTO):
```powershell
cargo build --release -p tessara --features custom-protocol
```

### Compiler Optimization Profile
The workspace `Cargo.toml` specifies aggressive binary hardening and size optimizations:

```toml
[profile.release]
codegen-units = 1
lto = "thin"
panic = "unwind"
strip = "symbols"
```

- **`codegen-units = 1`:** Enables cross-crate inlining across all 10 internal crates.
- **`lto = "thin"`:** Optimizes inter-module code generation while maintaining reasonable build times.
- **`strip = "symbols"`:** Removes debugging symbols, reducing binary size and stripping file paths from release binaries.
- **Output:** The compiled executable is generated at `target\release\tessara.exe`.

---

## 3. Verify Supply-Chain Invariants

Before distributing any compiled artifacts, run the security gates:

```powershell
# 1. Verify no unauthorized crates pull in network dependencies
python tools/check_network_deps.py --locked

# 2. Run cargo deny checks for license compliance and security advisories
cargo deny check
```

---

## 4. Software Bill of Materials (SBOM)

Tessera includes an automated CycloneDX SBOM generator:
```powershell
python tools/gen_sbom.py
```
This generates a standardized machine-readable CycloneDX JSON inventory of every Rust crate and npm package linked into the release build.
