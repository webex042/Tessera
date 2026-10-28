# Configuration Reference

This page provides an exhaustive reference for all settings available in `%LOCALAPPDATA%\Tessara\config.json`.

---

## Complete Configuration Key Reference

| Configuration Key | Data Type | Default Value | Accepted Values | Relevant Subsystem | Security & Operational Implications |
|---|---|---|---|---|---|
| `inference.backend` | `string` | `"auto"` | `"auto"`, `"cuda"`, `"vulkan"`, `"rocm"`, `"sycl"`, `"openvino"`, `"metal"`, `"opencl"`, `"cpu"` | `tsr-infer` | Overrides automated backend discovery. Forcing an unverified backend pairing logs a warning. |
| `inference.device` | `string` | `""` | Device ID or name substring (e.g. `"CUDA0"`, `"0"`, `"rtx 4050"`) | `tsr-infer` | Restricts inference execution strictly to matching hardware adapters. |
| `inference.gpu_layers` | `integer` | `-1` | `-1` (all), `0` (CPU only), `> 0` (layer count) | `tsr-infer` | Controls VRAM offloading. Setting too high on memory-constrained GPUs causes OOM crashes. |
| `inference.allow_fallback`| `boolean` | `true` | `true`, `false` | `tsr-infer` | If `false`, the supervisor halts with an error if the primary backend fails instead of falling back to CPU. |
| `inference.idle_unload_min`| `integer` | `10` | `0` (never), `1` to `120` | `tsr-infer` | Minutes of inactivity before evicting the model from VRAM on battery power. Reduces idle battery drain. |
| `capture.cadence_active_s`| `integer` | `5` | `1` to `60` | `tsr-capture` | Polling interval during active user typing. Multiplied by $3\times$ when operating on battery. |
| `capture.quality_threshold`| `float` | `0.55` | `0.0` to `1.0` | `tsr-capture` | OCR quality score below which Rung 3 VLM screen cropping triggers. Higher values trigger VLM more often. |
| `storage.retention_days` | `integer` | `90` | `1` to `365` | `tsr-store` | Retention limit for captured items. Older items are pruned automatically, triggering evidence cascades. |
| `storage.vacuum_interval_d`| `integer` | `7` | `1` to `30` | `tsr-store` | Frequency in days for running database defragmentation and WAL checkpoints. |
