# Prerequisites

Before compiling Tessera, ensure your host environment meets the necessary hardware and software prerequisites.

---

## Operating System

- **Windows 11 (64-bit, Build 22000+)**
  - Tessera's desktop capture hooks rely on Windows 11 WinRT APIs:
    - `Windows.Graphics.Capture.GraphicsCaptureItem` with borderless capture (`IsBorderRequired = false`).
    - `Windows.Media.Ocr.OcrEngine` using user-installed language profiles.
    - `Windows.Security.Cryptography.DataProtection.CryptProtectData` (DPAPI).
  - *Note:* macOS and Linux builds of inference crates (`tsr-infer`, `tsr-store`, `tsr-pipeline`) exist, but desktop capture rungs (`tsr-capture`, `tsr-win`) currently target Windows 11.

---

## Compiler Toolchains

### 1. Rust Stable Toolchain (MSVC)
Tessera requires the `x86_64-pc-windows-msvc` target using Rust edition 2024:
```powershell
rustup default stable-x86_64-pc-windows-msvc
rustup update stable
```
Verify the installation:
```powershell
rustc --version
# Expected: rustc 1.85+ (or 1.99+ preview)
```

### 2. Visual Studio 2022 Build Tools
Install the Visual Studio C++ Build Tools workload to provide MSVC linker tools (`link.exe`) and Windows SDK headers:
- Workload: **Desktop development with C++**
- Individual components:
  - MSVC v143 - VS 2022 C++ x64/x86 build tools
  - Windows 11 SDK (10.0.22621.0 or newer)

### 3. Node.js & pnpm
The frontend UI is built using React 19, TypeScript, and Vite 8:
- **Node.js:** v24.x LTS recommended (minimum v20.x).
- **pnpm:** v12.15.0 (managed via `corepack` or standalone).
```powershell
corepack enable
corepack prepare pnpm@12.15.0 --activate
pnpm --version
```

### 4. Python 3.11+
Required for evaluation runners, benchmark harnesses, and compile-time network dependency validation:
```powershell
python --version
# Expected: Python 3.11.x or newer
```

### 5. Strawberry Perl (Critical for SQLCipher)
`rusqlite` compiles SQLCipher 4 from source with a vendored OpenSSL bundle (`bundled-sqlcipher-vendored-openssl`). OpenSSL's build configuration scripts require Perl on the system `PATH`:
- Download [Strawberry Perl (Portable Edition)](https://strawberryperl.com/)
- Extract and add `C:\strawberry-perl\perl\bin` to your system `PATH`.
- Verify in terminal:
```powershell
perl -v
```

::: warning First Build Latency
The initial compilation of vendored OpenSSL and SQLCipher takes approximately 20–25 minutes. Subsequent incremental builds take only a few seconds.
:::
