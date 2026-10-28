# Architecture Decision Records (ADRs)

Tessera maintains a formal ledger of architectural decisions in `docs/adr/`. Below is a summary of all accepted records.

---

## ADR Index

| ADR ID | Title | Date | Status | Key Decision |
|---|---|---|---|---|
| **[ADR-0001](https://github.com/webex042/Tessera/blob/main/docs/adr/0001-stage0-pins-and-hardening.md)** | Stage 0 Pins and Hardening | 2026-10-24 | Accepted | Pinned exact dependency versions (`=`); dynamic ONNX loading (`api-31`); stripped sidecar environment allowlist. |
| **[ADR-0002](https://github.com/webex042/Tessera/blob/main/docs/adr/0002-webview2-outbound-connection.md)** | WebView2 Outbound Connection | 2026-10-25 | Accepted (Open Defect) | Documented that WebView2 makes 1 outbound HTTPS connection per run to Microsoft; "nothing leaves your PC" claim withheld until resolved. |
| **[ADR-0003](https://github.com/webex042/Tessera/blob/main/docs/adr/0003-updater-blocked-by-zip-advisories.md)** | Updater Blocked by Zip Advisories | 2026-10-25 | Accepted | Auto-updater feature disabled and compiled out of default builds due to unpatched security advisories in transitive `zip` crates. |
| **[ADR-0004](https://github.com/webex042/Tessera/blob/main/docs/adr/0004-gpu-vendor-independence.md)** | GPU-Vendor-Independent Inference | 2026-10-26 | Accepted | Separate pinned llama.cpp sidecar builds (CPU, CUDA, Vulkan, ROCm, SYCL); host binary links zero GPU libraries; dynamic DXGI probing. |
| **[ADR-0005](https://github.com/webex042/Tessera/blob/main/docs/adr/0005-redaction-v1-thresholds.md)** | Redaction V1 Thresholds | 2026-10-26 | Accepted | Target 100% secret recall on known corpus; maximum $2.0\%$ false positive rate on clean text; Verhoeff for Aadhaar, Luhn for cards. |
| **[ADR-0006](https://github.com/webex042/Tessera/blob/main/docs/adr/0006-stage2-understanding-pipeline.md)** | Stage 2 Understanding Pipeline Decisions | 2026-10-27 | Accepted | Sidecar spawned suspended in Job Object; loopback TCP port owner checked via `GetExtendedTcpTable`; minimal `std::net` HTTP client. |
| **[ADR-0007](https://github.com/webex042/Tessera/blob/main/docs/adr/0007-stage3-notch-ui.md)** | Stage 3 Notch UI Decisions | 2026-10-27 | Accepted | Fixed-size transparent window with panel state machine; Win32 capture exclusion affinity; focus return HWND tracking. |
| **[ADR-0008](https://github.com/webex042/Tessera/blob/main/docs/adr/0008-stages-4-6-and-real-model-guard.md)** | Stages 4–6 and Real-Model Guard | 2026-10-27 | Accepted | Hour-to-month map-reduce rollups; prompt injection defense with `needs_review` holding state; streaming sentence citation verifier. |
| **[ADR-0009](https://github.com/webex042/Tessera/blob/main/docs/adr/0009-stage7-security-audit.md)** | Stage 7 Security Audit & Release Gates | 2026-10-28 | Accepted | Formalized MVP exit criteria S1–S7; blocked Stage 8 audio downloads pending resolution of WebView2 finding. |
| **[ADR-0010](https://github.com/webex042/Tessera/blob/main/docs/adr/0010-schema-key-order.md)** | Keep JSON Schema Key Order | 2026-10-28 | Accepted | Enabled `serde_json/preserve_order` so response schemas reach the model in declared order; sorted keys had caused empty Ask answers and invalid extraction JSON. |
