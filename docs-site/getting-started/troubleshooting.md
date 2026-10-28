# Troubleshooting

Common build, runtime, and hardware detection issues and their solutions.

---

## Build Errors

### 1. `perl is not recognized as an internal or external command`
- **Symptom:** Compilation fails during the `rusqlite-sys` or `openssl-src` build step.
- **Cause:** SQLCipher is compiled from source with vendored OpenSSL, which requires Strawberry Perl to generate build configuration files.
- **Fix:** Install Strawberry Perl, ensure `perl.exe` is present on your system `PATH`, and restart your terminal:
```powershell
where.exe perl
# Should print: C:\strawberry\perl\bin\perl.exe
```

### 2. `failed to run custom build command for 'tauri-build'`
- **Symptom:** `tauri-build` fails with an error indicating `ui/dist` or `index.html` is missing.
- **Cause:** The desktop UI has not been compiled yet.
- **Fix:** Run the frontend compilation script before invoking `cargo`:
```powershell
pnpm build:ui
```

---

## Runtime & Detection Issues

### 3. GPU is Listed as `detected` but not `installed`
- **Symptom:** `tsr-models check` lists your graphics adapter as `detected`, but reports no active GPU backend.
- **Cause:** You have not downloaded the corresponding verified `llama.cpp` sidecar variant for that backend.
- **Fix:** Download the required backend using the fetch script:
```powershell
python tools/fetch_sidecar.py llama_cpp vulkan-x64
# Or for NVIDIA:
python tools/fetch_sidecar.py llama_cpp cuda-12.4-x64
```

### 4. Sidecar Fails with `ProbeError` or `Timeout`
- **Symptom:** Startup reports that backend probing timed out after 20 seconds.
- **Cause:** GPU driver crash, outdated driver runtime, or third-party overlay hook interference (e.g., RivaTuner, ASUS GPU Tweak).
- **Fix:** 
  1. Update your GPU driver to the latest release.
  2. If the GPU continues to hang, force CPU execution in `%LOCALAPPDATA%\Tessara\config.json`:
  ```json
  {
    "inference": {
      "backend": "cpu"
    }
  }
  ```

### 5. `Model unavailable (restart budget exceeded)`
- **Symptom:** The floating notch displays a persistent "Model Unavailable" error.
- **Cause:** The sidecar process crashed or hung 5 times within a single 60-minute window, exhausting the supervisor's safety budget.
- **Fix:**
  - Check whether your machine ran out of RAM or VRAM using Task Manager.
  - Review the content-free event logs in `%LOCALAPPDATA%\Tessara\logs\`.
  - Restart the application to reset the hourly restart counter.
