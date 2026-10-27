# Stage 7 verification report: security hardening and guardrail audit

Date: 2026-10-28. Machine as before (Windows 11, AMD iGPU, Qwen3-VL-4B Q4_K_M via Vulkan).
Decisions: [ADR-0009](../adr/0009-stage7-security-audit.md).

## A. Task results

| task (plan §17 Stage 7) | result | evidence | status |
|---|---|---|---|
| 1. Threat-model review against the built system | reviewed per threat (§C); gaps found and fixed: draft echoing an injected request (real model), date-resolver panic, non-idempotent redaction, model files not digest-checked at start, clipboard restore never running (Stage 5), injection guard gaps | this report, ADR-0009 | **PASS** |
| 2. Red-team corpora: injection ≥ 150, secrets ≥ 300 incl. Indian identifiers, in CI | injection **152** cases (78 new, written before any run); secrets **355** (Aadhaar incl. VID, PAN, passport, voter ID, accounts + IFSC, UPI); both run in `cargo test` / `tsr-eval all` | `tests/fixtures/{injection,redaction}` | **PASS** (size); results in §B |
| 3. Fuzzing: redaction, dates, incremental JSON, normaliser | **randomised robustness tests** (seeded, 20 000 inputs per target; 3 000 random chunkings per streaming parser); found 2 real bugs (date panic, redaction idempotence), fixed. **Coverage-guided `cargo-fuzz` not run**: needs a nightly toolchain and downloads | `tsr-pipeline/tests/robustness.rs`, `tsr-retrieval/tests/stream_robustness.rs`, `tsr-draft/tests/partial_robustness.rs` | **PARTIAL** (cargo-fuzz BLOCKED on approval) |
| 4. 24-hour network monitor | 10-minute run: `tessara.exe` **no** outbound connections; `msedgewebview2.exe` **one** HTTPS connection to a Microsoft address (2603:1046:…:443) ~1 min after start — the Stage 0 finding (ADR-0002), still open | `tools/netmon.ps1` | **FAIL** for S1 as written (zero connections); 24 h run needs the owner |
| 5. Canary disk scan incl. WebView2 profile, %TEMP%, crash folders | **0 plaintext hits** in 285 files / 32 MB after a 60 s run with the canary in items, titles, a fact, a summary and the open panel; positive control found | `tools/canary-scan.ps1`, `evals/results/canary_scan.json` | **PARTIAL**: S2 asks for 24 h; pagefile/hiberfil not scannable without elevation |
| 6. Dependency audit + licence review → THIRD_PARTY_NOTICES.md | `cargo deny` bans / licences / sources **ok**; advisories **ok** against the cached RustSec DB (2026-10-26, offline); `THIRD_PARTY_NOTICES.md` generated (all permissive, MPL-2.0 the only weak copyleft). **Not run:** `cargo audit` (not installed), `pnpm audit` (network) | `tools/gen_third_party_notices.py` | **PARTIAL** (fresh DB fetch and `pnpm audit` BLOCKED on approval) |
| 7. Tauri review: CSP, capabilities, DevTools, IPC validation | CSP `'self'` only, no inline/eval, navigation lock, frozen prototype, one capability (event listen/unlisten), DevTools feature absent; IPC inputs validated (lengths, enums, ids, tokens) with tests | `src-tauri/tests/{security_config,views}.rs` | **PASS** |
| 8. Sidecar/model integrity at load; signed builds plan | sidecar files hash-checked before start (Stage 2); **model files now digest-checked on first use and after any change** (stamp of size + mtime) — before this stage only names and sizes were checked; signing plan in `docs/release/README.md` | `tsr-infer` stamp test | **PASS** (signing itself: owner, Stage 10) |
| 9. `privacy.md`, `SECURITY.md` | rewritten with evidence per claim | `docs/privacy.md`, `SECURITY.md` | **PASS** |

## B. Measurements

### Injection (152 cases)

| run | result |
|---|---|
| SIMULATED compromised model, before Stage 7 fixes | 126 / 152; new batch 52 / 78 (odd half 27/39, even half 25/39) |
| SIMULATED, after fixes tuned on the **odd** half only | 140 / 152; **odd 39/39 (tuned), even 27/39 (held out)**, original 74/74 |
| REAL model, 152 cases | see `evals/results/injection_real.md` (run in progress at time of writing) |

The even half is the honest estimate of how the code guard generalises to unseen attack styles
when the model obeys every injection: about 69 %. Its misses are social engineering phrased as
ordinary mail (fake IT password reset, payroll account change), memory-poisoning and
delayed-trigger variants, quoted phishing, and window-title injections. They are left unfixed so
the estimate stays meaningful; the next round needs a third unseen set. What holds regardless:
stored facts are never executed — an injected "task" can only appear as text the user reads.

### Secrets (355 cases)

All 355 redacted; false positives 1 / 242 clean lines (0.41 %, limit 2 %). The 65 new cases first
leaked 48 (Aadhaar VID, passport, voter ID had no detectors); detectors added (`REDACT_VER` v2).

### Network

10-minute monitored run (2026-10-28, 300 polls, process tree incl. WebView2): see §D.

## C. Threat review (plan §15) — what changed

| threat | gap found | fix |
|---|---|---|
| Injected instructions reaching drafts | real model copied an injected request into a draft; no flag | draft sentences that repeat an injected thread line are flagged; banner when the thread contains instructions for AI tools (ADR-0009) |
| Adversarial text crashing jobs | date resolver panicked on huge offsets | fallible span construction |
| Secrets surviving redaction | second OTP in the keyword window survived one pass | redaction runs to a fixed point |
| Tampered model files | only size checked at start | digest check with stamp |
| Clipboard left holding a draft | restore never ran (sequence number read too early) | fixed in Stage 5 |

## D. Network result

`tools/netmon.ps1 -Minutes 10` on a synthetic store: processes seen `tessara.exe`,
`msedgewebview2.exe`, `conhost.exe`; 3 TCP endpoints, 0 UDP sockets; 2 non-loopback rows, both
the same WebView2 connection to `2603:1046:c06:c75::2:443` (established 00:39:00, closing
00:40:46). Tessara's own process and llama-server were never seen outside loopback. Polling can
miss a connection that opens and closes between polls; the formal 24 h run uses Sysmon/WFP
(`docs/stage0/network-test.md`) and needs the owner (elevation, 24 h of the machine).
