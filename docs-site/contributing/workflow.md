---
title: Development & Contribution Workflow
description: Step-by-step developer workflow, coding standards, local verification loops, and pull request procedures for Tessera.
---

# Development & Contribution Workflow

This document outlines the local developer workflow for building, testing, linting, and submitting changes to the Tessera repository.

---

## 1. Prerequisites Checklist

Before creating a branch, ensure your development environment satisfies:

* **OS**: Windows 11 (64-bit) with Developer Mode or standard Win32 debugging capabilities.
* **Rust**: `1.82+` stable toolchain with `rustfmt` and `clippy`.
* **Node.js**: `20.x` or `24.x` with Corepack enabled (`corepack enable`).
* **pnpm**: Version 9 (`pnpm install --frozen-lockfile`).
* **Python**: Python `3.11+` (tested on 3.13) for utility and evaluation scripts.
* **C++ Compiler**: Visual Studio 2022 C++ Build Tools (MSVC v143) for native compilation.
* **Perl**: Strawberry Perl (required to build OpenSSL / SQLCipher from source).

---

## 2. Branch & Commit Conventions

### Branch Naming
Create focused, descriptive feature branches off `main`:
* `feat/<short-description>`: New features, backend adapters, or tools.
* `fix/<bug-identifier>`: Bug fixes, memory leak resolutions, crash recovery.
* `docs/<topic>`: Documentation updates, ADRs, or guides.
* `refactor/<module>`: Internal architectural refactoring without behavioral alterations.

### Commit Messages
Write clear, imperative commit messages:
```text
feat(tsr-infer): add DirectML adapter support for AMD/Intel GPUs

- Implement DirectML execution provider initialization in tsr-infer
- Add adapter probe tests in tests/selection.rs
- Update models.lock with tested DirectML quantization targets
```

---

## 3. The Local Pre-Commit Verification Loop

Before pushing code or opening a pull request, run the following verification pipeline locally:

```powershell
# 1. Format checking
cargo fmt --all --check

# 2. Rust Clippy (zero warnings allowed)
cargo clippy --workspace --all-targets --locked -- -D warnings

# 3. Frontend TypeScript & UI Build
pnpm -r typecheck
pnpm build:ui

# 4. Enforce Network Boundary Invariant
python tools/check_network_deps.py --locked

# 5. Dependency License and Advisory Audits
cargo deny --locked check

# 6. Unit and Simulated Hardware Tests
cargo test --workspace --locked

# 7. Deterministic Evaluation Suite (Stage 2 gates)
cargo run -p tsr-evals --bin tsr-eval --locked -- all
```

---

## 4. Documentation Site Verification

If you modified documentation files in `docs-site/`:

```powershell
# Navigate to docs directory
cd docs-site

# Verify production static build
pnpm build

# Preview documentation locally
pnpm preview
```

---

## 5. Pull Request Checklist

When submitting a pull request on GitHub:

- [ ] Branch is rebased against the latest `main`.
- [ ] No network-capable dependencies were added outside `crates/tsr-net`.
- [ ] No raw screen content, OCR text, or PII is emitted to logs.
- [ ] All unit, simulated hardware, and deterministic evaluation tests pass.
- [ ] If new crate dependencies were added, licenses are compatible with `deny.toml`.
- [ ] If user-facing APIs or CLI tools were modified, corresponding documentation in `docs-site/` is updated.
- [ ] No private keys, local file paths, or credentials are committed.
