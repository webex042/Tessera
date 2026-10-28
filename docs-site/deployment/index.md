---
title: Deployment & Distribution Overview
description: Overview of Tessera's dual-track deployment architecture spanning static documentation hosting on Vercel and native Windows desktop application packaging.
---

# Deployment & Distribution Overview

Tessera's deployment model separates the documentation and public web presence from the local-first desktop application runtime. The application itself never runs on the web, does not use a cloud backend, and does not require persistent server infrastructure.

```mermaid
flowchart TD
    subgraph Track1["Track A: Documentation Website (Cloud)"]
        A1["docs-site/ (VitePress)"] -->|Static Site Generation| A2[".vitepress/dist"]
        A2 -->|Deploy via GitHub Integration| A3["Vercel Global Edge CDN"]
        A3 -->|Public HTTPS| A4["Developer & Contributor Browsers"]
    end

    subgraph Track2["Track B: Desktop Application (Local Windows)"]
        B1["Rust Crates + React UI"] -->|tauri build / cargo build| B2["Native Windows Executable"]
        B2 -->|LZMA Compression| B3["CurrentUser NSIS Installer (.exe)"]
        B4["Pinned Sidecars (tools/sidecar.lock)"] --> B3
        B5["CycloneDX 1.5 SBOM (gen_sbom.py)"] --> B6["GitHub Release Artifacts"]
        B3 --> B6
    end
```

---

## The Dual Deployment Tracks

| Dimension | Track A: Documentation Website | Track B: Desktop Application |
| :--- | :--- | :--- |
| **Directory** | `docs-site/` | Root repository (`src-tauri/`, `crates/`, `ui/`) |
| **Platform** | Vercel Edge Network | Microsoft Windows 11 (x64) |
| **Output Type** | Pure Static HTML, JS, CSS, SVG | Native Win32 / WebView2 binary (`.exe`) + NSIS installer |
| **Runtime Backend** | None (Client-side MiniSearch index) | Local Rust engine, SQLCipher, `llama-server` sidecar |
| **Data Boundary** | Zero telemetry, zero external search APIs | 100% loopback, DPAPI-encrypted local database |
| **CI / CD Pipeline** | Vercel GitHub App (preview + production) | `.github/workflows/ci.yml` & `nightly-evals.yml` |

---

## Deployment Subsections

1. **[Vercel Documentation Hosting](/deployment/vercel-hosting)**  
   Detailed guide for deploying this VitePress documentation site to Vercel, including Root Directory overrides, build commands, output paths, clean URLs, and preview branch workflows.
2. **[Desktop Application Packaging](/deployment/packaging)**  
   How the Tauri 2 desktop shell packages the React 19 frontend, bundles WebView2 bootstrappers, and configures NSIS installers with per-user privileges.
3. **[Release Engineering & SBOM](/deployment/release-engineering)**  
   The zero-network software bill of materials (`tools/gen_sbom.py`), cryptographic hash locking (`sidecar.lock`, `models.lock`), code signing prerequisites, and the auto-updater security gate ([ADR-0003](/developer/adrs)).
4. **[CI/CD Workflows](/deployment/ci-cd)**  
   Architecture of GitHub Actions automation: continuous integration checks, simulated hardware matrices, deterministic evaluation gates, and nightly self-hosted GPU evaluations.
