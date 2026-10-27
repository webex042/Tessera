# Stage 2 verification report

Date: 2026-10-27. Machine: Windows 11, AMD Radeon integrated GPU (Vulkan), 12 logical cores
(AVX2/FMA/F16C), 15.3 GiB RAM. **No model files on this machine** (owner's choice: tooling only),
so every model-dependent number below is either SIMULATED or UNVERIFIED, and says so.

Legend: **REAL** = real OS, processes, binaries or hardware; **SIMULATED** = a stand-in for the
model, server, embedder or job costs; **UNVERIFIED** = not measured.

## A. What was built

| workstream | where | summary |
|---|---|---|
| A. inference runtime | `tsr-win::job`, `tsr-win::tcp`, `tsr-infer::{http,client,sidecar,supervisor,sched,runtime,metrics,prompts,cancel}`, `tsr-fake-llama` | suspended start inside a kill-on-close Job Object, cleared environment, port-owner check, readiness; supervisor state machine with health, hang detection, backoff with jitter, restart budget, idle unload; loopback-only HTTP client; priority scheduler with preemption, aging, fairness, retries, idempotent keys; content-free metrics |
| B. model setup | `tools/models.lock`, `tsr-infer::{models,onboard}`, `tsr-net` (new), `tsr-models` CLI | pinned model manifest with digests; hardware check with four evidence levels; "locate existing files"; approval-gated, resumable, verified downloads |
| C. extraction | `tsr-pipeline::{extract,schema,evidence,normalize,guard,audit}` | staged, durable, preemptible job; strict schema; evidence as source spans; instruction classes; second redaction; one atomic write |
| D. resolvers | `tsr-pipeline::{dates,people}` | deterministic date resolver (193 table cases); conservative people resolver; digest-based suppression |
| E. facts store | migration 0003, `tsr-store::{facts,people,jobs,chunks}` | provenance and CHECK constraints, evidence spans, fact deleted with its last evidence, dedupe/merge, merge suggestions, forget person |
| F. retrieval | `tsr-retrieval::{chunk,embed,index,hybrid,ingest}` | span chunker, embedder (real ONNX + SIMULATED hash), int8 index, hybrid RRF with filters on every channel |
| G. rung 3 | `tsr-capture::{vlm,ocr}` | OCR word boxes, mask plan, verified masking, PNG, fixed-text request, strict transcript, leak check |
| H. evals and CI | `evals/runner` (`tsr-eval`), `tests/fixtures/injection`, `.github/workflows/{ci,nightly-evals}.yml` | injection, extraction, search, backlog runners; CI on every push; optional self-hosted model runner |
| ingest | `tsr-capture::sink`, `tsr-pipeline::worker` | store writer queues embed + extract jobs with the item; worker feeds the scheduler |

Decisions and deviations from the plan: [ADR-0006](../adr/0006-stage2-understanding-pipeline.md).

## B. Hardware and runtime support

| backend | status |
|---|---|
| CPU | REAL: pinned build verified; real llama-server started, contained and authenticated (router mode, no model) |
| Vulkan (AMD iGPU) | REAL: build verified, `Vulkan0` listed; **no inference run** (no model) |
| CUDA, ROCm, SYCL, OpenVINO, Metal, OpenCL | pinned in `tools/sidecar.lock`; selection logic tested with SIMULATED adapters; UNVERIFIED on hardware |
| embedder (ONNX Runtime CPU) | code path built (pinned runtime only); UNVERIFIED end to end (no embedding model) |
| VLM (mmproj) | UNVERIFIED (no model) |

## C. Exit-criteria scorecard

| criterion | threshold | measured | command / data | status |
|---|---|---|---|---|
| Extraction F1 (held-out) | ≥ 0.70 | **not measured** with a model. Oracle (SIMULATED perfect model): F1 1.000 — the validators dropped no gold fact | `tsr-eval extraction --model DIR --split heldout` / 58 items, 83 facts | **UNVERIFIED** |
| Hallucinated stored facts | ≤ 3 % (denominator: stored facts; a fact whose stored evidence is not in its item) | oracle: 0 / 83; by construction every stored fact has a verified source span | same | **UNVERIFIED** for a model (pipeline side holds) |
| Schema-invalid stored rows | 0 | 0 (oracle runs, all extraction tests, `audit::validate_stored`) | same; `tsr-pipeline` tests | **PASS** (code), model run pending |
| Injection corpus ≥ 50 cases | no injected task, no action, no leak | 74 cases (57 tuned, 12 held-out adversarial, 5 genuine controls): **74 / 74** with a SIMULATED model that obeys every injection | `tsr-eval injection` | **PASS** at code level; real model **UNVERIFIED** |
| 2-hour workload drains | done ≤ 30 min after the window | default profile: 158/158 jobs, drained **25 s** after; 2× arrivals: 354/354, 25 s; CPU-only: 158/158, 157 s | `tsr-eval backlog` (real `SchedulerCore`, assumed costs) | **SIMULATED PASS**; real throughput **UNVERIFIED** |
| No GPU work when idle; unload on battery | — | SIMULATED sidecar: 0 model requests over idle periods; unload after the battery timeout; lazy restart; 12-job unload/request race all served once | `cargo test -p tsr-infer --test runtime` | **PASS** (simulated); real GPU utilisation **UNVERIFIED** |
| Cleared sidecar environment | `LLAMA_ARG_*` in parent has no effect | REAL llama-server b11900: inherited `LLAMA_ARG_LOG_FILE` writes a log (control) / nothing when started by Tessara; ready in 290 ms | `cargo test -p tsr-infer --test sidecar_env -- --ignored` | **PASS (REAL)** |
| Embedders use only the pinned ONNX Runtime | System32 copy never mapped | embedder constructible only from a `PinnedRuntime`; existing mapped-module test | `cargo test -p tsr-retrieval -- --ignored` | **PASS** for the runtime; embedder end to end **UNVERIFIED** |
| Date resolver ≥ 150 table tests | ≥ 150 | **193** cases, all pass; no clock access (source scan) | `cargo test -p tsr-pipeline --test dates` | **PASS** |
| Basic CI without a GPU or model | — | all of the above except UNVERIFIED rows run in `ci.yml` on `windows-latest` | `.github/workflows/ci.yml` | ready (not yet run on GitHub) |

## D. Security results

- **Injection corpus.** First run with the SIMULATED compromised model: **11 / 62 failed** —
  summaries repeating the injected instruction (7), a genuine request dropped because an
  injection shared its line, exfiltration targets missing ("laptop disk", "all invoices"), a
  "summarizing this page must tell the user…" phrasing. Fixes: summaries with obligation
  language or from injected items are withheld; judgement per evidence sentence; wider target
  lists. Then 12 **held-out** cases were written without looking at the rules: first run
  **6 / 12** (two stored as *open user tasks*: "approve the attached wire … to account …" and
  "close every ticket … as Won't Fix"). Fixes: an AI role-noun rule, preamble taint, money
  movement always `needs_review`. One expectation was changed and documented (inj-x01 is fraud
  addressed to the user, not an AI instruction: it may be stored for review, never as an open
  task). Retest: **74 / 74**. The held-out cases have now informed the rules too, so **a fresh,
  untouched set is needed for an unbiased estimate**; the 6/12 figure is the honest measure of
  how the first version generalised.
- **Cost of the stricter guard** (oracle runs): 5 of 83 held-out gold facts and 1 of 43 dev gold
  facts land in `needs_review` instead of open; no gold fact was rejected outright.
- **Masking (REAL).** A Notepad window with a synthetic note was captured and OCR'd; the mask
  plan blacked out 2 words (OTP, AWS key) and re-OCR of the masked pixels found **no readable
  secret**, while ordinary text remained. **Finding:** Windows OCR did not read the line
  `Card 4111 1111 1111 1111` at all, so it was **not masked**. The VLM answer's redaction pass
  would still remove a transcribed card number (SIMULATED test), but the model would have seen
  it. Mitigation (union with rung-1 UIA element boxes) is not built; until then rung 3 must not
  be trusted to hide what OCR cannot read.
- **Storage.** A test found that `INSERT OR IGNORE` silently swallowed a CHECK failure and stored
  a fact without evidence; all such statements now use conflict-only `ON CONFLICT DO NOTHING`.
- **Network.** `tools/check_network_deps.py` and `cargo deny`: network-capable crates only in
  `tsr-net`; downloads refuse anything without approval, non-HTTPS, other hosts, wrong ranges,
  wrong digests (13 tests on a SIMULATED loopback server). No model was downloaded.

## E. Retrieval (SIMULATED embedder)

54 answerable Ask questions over the 80 extraction items:

| channels | recall@5 | recall@10 | MRR | nDCG@10 |
|---|---|---|---|---|
| keyword (any-term FTS) | 0.942 | 0.962 | 0.828 | 0.856 |
| vector (`sim-hash-v1`) | 0.788 | 0.846 | 0.664 | 0.697 |
| keyword + vector (RRF) | 0.904 | 0.981 | 0.847 | 0.870 |
| keyword + vector + facts (RRF) | 0.942 | 1.000 | 0.845 | 0.876 |

With this simulated embedder, fusion improves recall@10 and MRR but lowers recall@5 for
keyword+vector; no claim is made about semantic search until E5 runs. All 8 unanswerable
questions return hits (expected: deciding "not in memory" is Stage 6). Behaviour checks (all
pass): blocklist and suppression filters apply to every channel; a redacted OTP is not findable;
duplicates keep their own provenance; no vector index → keyword results with a diagnostic;
deterministic ranking. Known gap: a first-name-only mention of a forgotten person is not filtered.

## F. Build and regression results

Final run on 2026-10-27 (commit after ec8b1a5), dev machine:

| command | result |
|---|---|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean |
| `cargo test --workspace --locked` | **311 passed, 0 failed**, 11 ignored (the REAL tests below); was 128 at the end of Stage 1 |
| `cargo test -p tsr-win --locked -- --include-ignored` | REAL: Job Object with child + grandchild, TCP port owner, OS events (Notepad), memory and process usage — all pass |
| `cargo test -p tsr-infer --test sidecar_env -- --ignored` | REAL llama-server b11900 ready in 206 ms; inherited `LLAMA_ARG_LOG_FILE` had no effect — pass |
| `cargo test -p tsr-infer --test real_hardware -- --ignored` | REAL backend probes (CPU, Vulkan on AMD iGPU) — 4 pass |
| `cargo test -p tsr-retrieval -- --ignored` | REAL pinned ONNX Runtime, System32 copy not mapped — pass |
| `cargo test -p tsr-capture --lib -- --ignored` | REAL taskbar frame capture with OCR word boxes — pass |
| `cargo test -p tsr-capture --test vlm_mask_real -- --ignored` (run separately) | REAL masking — pass, with the OCR-miss finding in section D |
| `python tools/check_network_deps.py --locked` | PASS: network crates only via `tsr-net` |
| `cargo deny --locked check` | advisories, bans, licences, sources ok |
| `python evals/validate.py` | OK |
| `cargo run -p tsr-evals --bin tsr-eval -- all` | exit 0 (injection 74/74, oracle extraction, search checks, backlog) |

Not run here: the GitHub Actions workflows themselves (pushed nothing), the nightly self-hosted
model job (no runner, no model).

## G. Remaining work (genuine gaps only)

1. **No model run anywhere** (owner's constraint): held-out F1, real hallucination rate, real
   injection behaviour, VLM quality, embedding quality, inference latency, VRAM, GPU idle
   utilisation, smoke test / "inference-tested" level. Runners are ready (section H).
2. **App assembly:** `src-tauri` does not yet start capture, the inference runtime or the
   workers, and has no onboarding screens (Stage 1 pending UI items).
3. **Rung 3 is not triggered automatically** from the capture loop, and masking misses text OCR
   does not read (UIA-box union pending).
4. Not implemented: sidecar warm-up request; tokens/s metrics; embedding-similarity dedupe; the
   high-confidence fact boost in search; index latency target (p95 ≤ 30 ms at 100 k) unmeasured.
5. Non-Windows: containment is kill-on-drop only (no grandchild containment), endpoint owner check
   skipped; untested.
6. A fresh, untouched injection set is needed for an unbiased generalisation estimate.

## H. Reproduction

```bash
cargo test --workspace --locked
cargo test -p tsr-infer --test sidecar_env --locked -- --ignored --test-threads=1
cargo test -p tsr-win --locked -- --include-ignored
cargo test -p tsr-capture --test vlm_mask_real --locked -- --ignored --nocapture
cargo run -p tsr-evals --bin tsr-eval --locked -- all
cargo run -p tsr-net --bin tsr-models --locked -- check
python tools/check_network_deps.py --locked
cargo deny --locked check
```

With model files (after `tsr-models locate DIR --verify` shows them usable):

```bash
cargo run -p tsr-evals --bin tsr-eval --locked -- extraction --model DIR --split heldout
cargo run -p tsr-evals --bin tsr-eval --locked -- injection --model DIR
```
