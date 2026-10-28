# Getting Started with Tessera

Welcome to the **Tessera Quickstart Guide**. This tutorial will guide you through setting up, configuring, and running Tessera on your Windows 11 machine from scratch.

Even if you have never run a local artificial intelligence model before, this guide breaks each step into clear, straightforward actions with verifiable outcomes.

---

## 1. Introduction: What You Will Build

By the end of this guide, you will have a working local instance of Tessera running on your desktop. You will be able to:
* Capture text from your active desktop windows without cloud streaming.
* Redact sensitive credentials automatically before they touch disk.
* Ask natural-language questions about what was on your screen.
* Generate grounded answers powered by a local 4-billion parameter language model running on your GPU or CPU.

---

## 2. Why Getting Started is Simple

Setting up local AI software is traditionally intimidating. Users often face:
* Hours spent installing gigabytes of complex GPU development toolkits (like NVIDIA CUDA or AMD ROCm).
* Confusing compilation errors caused by incompatible C++ compiler versions.
* System crashes caused by software conflicting with display drivers.

**Tessera eliminates this complexity:**
1. **Zero CUDA Installation:** Tessera does not require installing NVIDIA CUDA SDKs. It uses pre-built, hash-verified **Vulkan** and **CPU** sidecar binaries.
2. **Automated Verification:** Simple scripts check that your environment is ready and that all sidecar binaries match pinned SHA-256 checksums.
3. **Graceful Fallbacks:** If your graphics card is busy or unsupported, Tessera automatically falls back to your CPU without crashing.

---

## 3. System Prerequisites Checklist

Before you begin, ensure your machine meets these basic requirements:

| Requirement | Minimum Specification | Recommended Specification |
| :--- | :--- | :--- |
| **Operating System** | Windows 11 (64-bit, build $\ge 19041$) | Windows 11 Pro 64-bit (build $\ge 22621$) |
| **System Memory (RAM)** | 8 GB | 16 GB or higher |
| **Processor (CPU)** | x86_64 CPU with **AVX2** support | Modern multi-core Intel (10th gen+) or AMD Ryzen |
| **Graphics Card (GPU)** | Integrated Intel Iris Xe or AMD Radeon | Dedicated GPU with Vulkan 1.2+ support |
| **Disk Space** | 8 GB free storage | 15 GB free storage (for models and cache) |
| **Software Toolchains** | Rust 1.80+, Node.js 20+, Python 3.10+ | Visual Studio 2022 C++ Build Tools installed |

For detailed software installation instructions, see [System Prerequisites](/getting-started/prerequisites).

---

## 4. Step-by-Step Installation Walkthrough

Follow these sequential steps in an elevated **PowerShell** window:

### Step 1: Clone the Repository
Clone the Tessera repository and enter the project directory:
```powershell
git clone https://github.com/webex042/Tessera.git
cd Tessera
```

### Step 2: Install Frontend Dependencies & Build Desktop UI
The desktop frontend is built using React, TypeScript, and Vite under Tauri 2:
```powershell
cd ui
npm install
npm run build
cd ..
```
*Expected Outcome:* The `ui/dist/` directory is generated containing the compiled HTML, CSS, and JavaScript assets.

### Step 3: Fetch Verified AI Sidecar Binaries
Tessera uses pinned builds of `llama.cpp` to run models independently of your main application. Download the hash-verified binaries:
```powershell
# Fetch CPU-optimized binary with AVX2 support
python tools/fetch_sidecar.py llama_cpp cpu-x64

# Fetch Vulkan GPU acceleration binary
python tools/fetch_sidecar.py llama_cpp vulkan-x64

# Fetch ONNX Runtime sidecar
./tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64
```
*Expected Outcome:* The binaries are placed in `third_party/llama_cpp/b11900/` and verified against the `tools/sidecar.lock` checksums.

### Step 4: Verify Dependency Boundaries (Zero Cloud Leakage)
Run the automated network dependency scanner to ensure no unauthorized network packages have entered the build tree:
```powershell
python tools/check_network_deps.py --locked
```
*Expected Outcome:* The script prints `[PASS] Zero prohibited network crates detected.`

### Step 5: Build & Launch Tessera
Compile and run the desktop application using Cargo:
```powershell
cargo run -p tessera
```
*Expected Outcome:* The Rust compiler links the desktop shell, initializes SQLite storage, tests the sidecar supervisor, and displays the Tessera desktop pill in the upper screen notch.

---

## 5. First-Run Walkthrough: Using Tessera

Once Tessera is running:

1. **System Tray Icon:** Look for the Tessera icon in your Windows notification area (system tray). Right-clicking opens the status menu.
2. **The Top Notch:** Tessera appears as a subtle, unobtrusive pill at the top of your primary display.
3. **Capture Ladder in Action:** As you switch between windows (e.g., your browser or text editor), Tessera periodically samples the active window title and text content.
4. **Testing Redaction:** Open Notepad, type `My secret key is sk-1234567890abcdef`, and save it. When Tessera reads the window, `tsr-guard` immediately replaces `sk-1234567890abcdef` with `[REDACTED_API_KEY]` before saving to SQLite.
5. **Asking a Question:** Click the notch or press `Alt + Space` to expand the query bar. Type a question about something you recently viewed, such as:
   > *"What did I write in Notepad just now?"*
6. **Reviewing Grounded Answers:** Tessera retrieves the redacted note and answers with exact substring citations.

---

## 6. Common Beginner Mistakes

* **Mistake 1: Forgetting to Fetch the Sidecar Binaries:**
  Running `cargo run -p tessera` without running `fetch_sidecar.py` will cause the application to report `Worker binary not found in third_party`. Always run the fetch scripts first.
* **Mistake 2: Missing Visual C++ Redistributable:**
  Pre-compiled `llama.cpp` binaries require the Microsoft Visual C++ Redistributable. If the sidecar immediately exits with code `0xC0000135`, install the free Visual C++ 2015–2022 x64 runtime from Microsoft.
* **Mistake 3: Testing in a Virtual Machine Without GPU Passthrough:**
  Virtual machines often lack Vulkan 1.2 drivers. In a VM, configure Tessera to use the `cpu-x64` backend explicitly in `config.json`.
* **Mistake 4: Running Multiple Heavy LLMs Simultaneously:**
  Running another local AI tool (like Ollama or LM Studio) simultaneously can exhaust your GPU VRAM, causing Tessera to fall back to CPU inference.

---

## 7. Troubleshooting Common Startup Issues

| Symptom / Error | Root Cause | Exact Fix |
| :--- | :--- | :--- |
| **`error: linker 'link.exe' not found`** | Rust cannot locate the MSVC C++ toolchain. | Open Visual Studio Installer and check "Desktop development with C++". |
| **`Sidecar exited with status 3221225781`** | Missing `MSVCP140.dll` or `VCRUNTIME140.dll`. | Download and install the latest `vc_redist.x64.exe` from Microsoft. |
| **`WGC capture failed: 0x80070005`** | App lacks permission to capture desktop window. | Ensure Tessera is running under your standard user account (not an unmapped service). |
| **`Port 127.0.0.1:8080 already in use`** | Another local server is occupying the default port. | Change `server_port` in `config.json` to an open port (e.g., `8085`). |

For a complete catalog of errors, see the [Troubleshooting Guide](/getting-started/troubleshooting).

---

## 8. Next Steps & Further Learning

Now that your local instance is up and running, dive deeper into specific components:

* **[Development Environment](/getting-started/development)** — Set up VS Code or RustRover with recommended debug extensions.
* **[Configuration Matrix](/getting-started/configuration)** — Learn how to tune VRAM offload layers, context limits, and capture sampling intervals.
* **[System Architecture](/architecture/)** — Understand how Rust, Tauri, and `llama.cpp` communicate over loopback IPC.
