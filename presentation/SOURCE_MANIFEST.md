# Tessera presentation — source manifest and verification summary

**Deliverable:** `Tessera_Technical_Project_Presentation.pptx` — 15 slides, 16:9 (13.33 × 7.5 in),
editable text and native PowerPoint shapes throughout, speaker notes on every slide.
**Preview:** `Tessera_Technical_Project_Presentation.pdf`, exported from that exact file.
**Generator:** `deck.js` (assembled from `build_deck.js` + `part2.js`, with `theme.js` and `helpers.js`).

---

## 1. Project materials examined

Read in full before any slide was designed:

| Source | Used for |
|---|---|
| `README.md` (65 KB) | Project overview, feature-status matrix, technology stack, configuration reference, build instructions |
| `stages.md` | Stage status and exit criteria — treated as authoritative over the README's §22 status list |
| `docs/final-report.md` (2026-10-27/28) | The current scorecard; the source for every measured number on slides 12 and 13 |
| `docs/stage0/results.md`, `stage2`–`stage7/verification.md` | Per-stage measurements, what was REAL vs SIMULATED vs UNVERIFIED |
| `docs/adr/0001`–`0010` | The ten decision records behind slides 3, 5, 8, 9 and 13 |
| `docs/privacy.md`, `SECURITY.md` | Slide 9's control inventory and the three-way status split |
| `docs/hardware-backends.md`, `docs/app-compatibility.md` | Slide 8's backend table and the 285 ms deadline figures |
| `evals/results/extraction_real_heldout.md`, `ask_real_all.md`, `draft_real_all.md`, `injection_real.md` | Slides 11, 12 and 13 — precision, recall, F1, latency, injection pass rates |
| `Cargo.toml` | Pinned dependency versions on slide 7 |
| `tools/models.lock`, `tools/sidecar.lock` | Model identifiers, digests and the pinned llama.cpp build tag |
| `crates/*/src/*.rs` (module documentation and key algorithms) | Slides 4, 5, 6 and 8 — verified that documented features exist in code |
| `crates/tsr-store/migrations/*.sql` | The storage guarantees quoted on slides 8 and 9 |
| `src-tauri/src/*.rs` | The typed command surface and approval tokens on slides 9 and 10 |
| `thesis/` (this project's thesis, written earlier) | Narrative structure and cross-checking |

Measured directly from the repository rather than quoted: source lines per crate (slide 6),
375 Rust test functions, 17 hardware-dependent tests (slide 12).

## 2. External references used

Each was fetched and confirmed to exist before being put on a slide.

| Reference | URL | Status |
|---|---|---|
| Tessera repository | https://github.com/webex042/Tessera | Confirmed public; taken from the repository's own `git remote` |
| llama.cpp | https://github.com/ggml-org/llama.cpp | Confirmed; owner is the `ggml-org` organisation |
| Qwen3-VL Technical Report | https://arxiv.org/abs/2511.21631 | Confirmed |
| Greshake et al., indirect prompt injection (AISec 2023) | https://arxiv.org/abs/2302.12173 | Confirmed |
| OWASP Top 10 for LLM Applications | https://owasp.org/www-project-top-10-for-large-language-model-applications/ | Confirmed |

All five are embedded as working hyperlinks on slide 15.

## 3. Verification summary

**Checks performed and passed**

- Slide count: 15 (the stated maximum). No separate thank-you slide.
- `validate.py`: all schema, relationship, content-type and chart checks pass.
- Content audit: no placeholder text, no lorem ipsum, no `TODO`, no `[insert …]`.
- Speaker notes present on all 15 slides (555–1,146 characters each).
- Hyperlinks: all five present in the slide relationships and resolving to the URLs above.
- Visual audit: every slide rendered at 110 dpi and inspected. Thirteen defects were found and
  fixed — a connector pointing at the wrong box on slide 4, columns overflowing the right margin
  on slide 3, result text running through a rule on slide 8, a table row colliding with its
  caption on slide 13, overlapping resource entries on slide 15, a hardware box drawn inside the
  job-object boundary on slide 5, and seven smaller spacing and contrast issues. All were
  corrected in the generator and re-rendered.
- The chart on slide 12 is a native PowerPoint chart, not an image.
- All diagrams are native shapes and text, editable in PowerPoint.

**Checks not performed**

- The deck was not opened in Microsoft PowerPoint itself; rendering was verified through
  LibreOffice. Fonts used (Cambria, Calibri, Courier New) ship with Office and render
  true-to-width in this previewer, so text fit should hold, but this is not the same as opening
  the file in PowerPoint.
- Hyperlinks were verified as present and well-formed in the file, and the target URLs were
  fetched separately; clicking them from inside PowerPoint was not tested.

## 4. Information that could not be verified

| Item | What the deck does |
|---|---|
| **Tessera vs Tessara** | The repository, README and documentation say *Tessera*; the Cargo package, the compiled binary (`tessara.exe`), the data directory (`%LOCALAPPDATA%\Tessara`) and `productName` say *Tessara*. The deck uses **Tessera** in prose and the exact internal spelling wherever a command, path or process name appears, and slide 10 notes the discrepancy. **Needs a decision before submission.** |
| **Repository licence** | Recorded as "to be determined" in the README. The deck makes no licence claim. |
| **Deployed documentation site** | A Vercel configuration exists for `docs-site/`, but no deployed URL is recorded anywhere in the repository. The deck links only to the repository, not to an invented docs URL. |
| **Screenshots of the interface** | None exist in the repository. The deck uses diagrams only; no interface screenshot was fabricated. |
| **Development timeline** | The git history contains 27 commits across three days, which does not reflect the real development period. The deck makes no claim about duration. |
| **Performance on the intended hardware** | The RTX 4050 reference machine was never available. Every latency figure is labelled as measured on an AMD integrated GPU and described as a lower bound. |

## 5. Inconsistencies found in the repository

These were resolved in favour of the most recent, best-evidenced document. They are worth fixing
in the repository itself, since a reviewer who opens it will notice.

1. `README.md` §22 lists Stages 0–6 as **COMPLETE**; `stages.md` and every stage verification
   report list them as in progress with unmet exit criteria. The deck follows `stages.md`.
2. `README.md` quotes extraction F1 **0.507** and Ask correctness **15 %**; `docs/final-report.md`
   quotes **0.533** and **66.7 %** after the ADR-0010 key-order fix. The deck uses the later figures.
3. `docs/hardware-backends.md` states that no model has been run on any backend; the final report
   and the evaluation results show real-model runs on Vulkan. That document is stale.
4. `README.md` §11.1 lists fact kinds as `action, decision, event, note, question`; the code and
   schema use `task, commitment, decision, event, question, note`. The deck follows the code.

## 6. Rebuilding the deck

Requires Node with `pptxgenjs`, and LibreOffice for the PDF.

```bash
node deck.js Tessera_Technical_Project_Presentation.pptx
soffice --headless --convert-to pdf Tessera_Technical_Project_Presentation.pptx
```

`deck.js` is generated by concatenating `build_deck.js` (slides 1–6) and `part2.js` (slides 7–15);
edit those two files, not `deck.js`. `theme.js` holds the palette and fonts — changing a colour
there restyles the whole deck, because every element uses the shared constants. `helpers.js` holds
the drawing primitives: the outlined-circle motif, cards, boxes, arrows and boundaries.
