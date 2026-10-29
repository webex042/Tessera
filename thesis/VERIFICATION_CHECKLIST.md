# Verification checklist — Tessera thesis

Read this before submitting anything. It lists, in order of importance: what only you or your guide
can confirm, what the thesis deliberately reports as unproven, inconsistencies found in the
repository itself, and the checks that were run on the document.

---

## A. Must be resolved before submission — needs you or your guide

| # | Item | Where | Why it is open |
|---|---|---|---|
| A1 | **Academic session and submission date** | Title page | Not stated anywhere in the repository or the brief. Not guessed. |
| A2 | **Guide's designation** | Approval of the Guide | Only the name "Dr. Ayan Kumar Das" was supplied. Designation left blank rather than invented. |
| A3 | **Title-page address arrangement** | Title page | The template gives the Mesra main-campus address. The thesis uses that address plus a separate "OFF-CAMPUS PATNA" line. Confirm this is what your department wants. |
| A4 | **Additional acknowledgements** | Acknowledgement | Head of Department, faculty and staff are not named because no names were supplied. |
| A5 | **Signatures** | Approval, Declaration, Certificate of Approval | All three are unsigned and must stay unsigned until the guide signs and you have read the full thesis. **Do not sign the declaration until you have personally verified every statement in the thesis is true.** |
| A6 | **Repository licence** | Appendix B | `README.md` in the repository records the licence as "to be determined". Choose and state one, since the repository is cited as project evidence. |
| A7 | **Tessera vs Tessara** | Throughout | The folder, README and docs say *Tessera*; the Cargo package, the binary and the data directory (`%LOCALAPPDATA%\Tessara`) say *Tessara*. The thesis uses **Tessera** in prose and the exact internal spelling in paths and package names. Confirm which is intended; renaming the internal identifier before submission would remove the inconsistency. |
| A8 | **Proposed title** | Title page | "Tessera: A Privacy-Preserving, Hardware-Independent Local Inference and Screen-Aware Memory System for Windows". Confirm your guide accepts it; it is descriptive and claims no novelty beyond what Chapter 6 measures. |
| A9 | **Thesis length and chapter structure** | Whole document | 99 pages, 8 chapters plus 6 appendices. If your department sets a page limit or a required chapter list, say so and the structure can be adjusted. |

---

## B. Claims the thesis deliberately reports as unverified

These are **not** errors. They are places where the repository's own verification reports say a
measurement was not taken, and the thesis says so rather than inferring a result. If any of these
have since been measured, send the numbers and the relevant section can be updated.

| # | Claim | Status as written |
|---|---|---|
| B1 | 24-hour network monitoring run (release criterion S1) | Only a 10-minute run was performed. Reported as such. |
| B2 | 24-hour canary disk scan (S2) | Only a 60-second run. Reported as such. |
| B3 | 8-hour resource run and 7-day stability run | Not run. Reported as not run (Table 3.2, NFR-11; Section 6.13). |
| B4 | "No image file is ever written to disk" | No image-writing code path exists, but the file-system watcher test was not performed. Reported as such, not as proven. |
| B5 | Password-field exclusion in native Windows Forms / WPF applications | Verified in browsers only. Reported as outstanding. |
| B6 | Capture exclusion inside Teams, Zoom, OBS, Snipping Tool, Game Bar | Verified at the Windows API level (Graphics Capture and GDI) only; the named applications were not run. Reported as partial. |
| B7 | Draft insertion in Gmail, Outlook, Teams, Slack, WhatsApp | Only a plain Win32 text field was exercised. Reported as partial. |
| B8 | Clipboard history and cloud-clipboard exclusion | Clipboard history is off on the test machine; cloud upload is unobservable without an account. Reported as untested. |
| B9 | Placement after a monitor or DPI change | Unit-tested only; one monitor available. Reported as partial. |
| B10 | Embedding (vector) retrieval channel | Implemented but never run end to end; no embedding model on the evaluation machine. The thesis says the vector channel's contribution is unknown. |
| B11 | CUDA, ROCm, SYCL, Metal, OpenCL backends | Selection logic verified against simulated adapter reports only. The thesis states this explicitly wherever the backends are mentioned (Tables 4.2 and 6.9). |
| B12 | Application compatibility gate (16 of 20 applications) | 7 tested, 7 usable. Reported as unverified, not as failed. |
| B13 | Coverage-guided fuzzing | Not run (toolchain unavailable offline). Randomised robustness testing was run and is reported as such. |

---

## C. Inconsistencies found inside the repository

Found while tracing claims back to source. Each one was resolved in favour of the most recent,
best-evidenced document; you may want to fix them in the repository too.

| # | Conflict | Resolution used in the thesis |
|---|---|---|
| C1 | `README.md` §22 lists Stages 0–6 as **COMPLETE**, while `stages.md` and every stage verification report list them as **In progress** with unmet exit criteria. | The thesis follows `stages.md` and the verification reports. §22 of the README overstates progress and should be corrected. |
| C2 | `README.md` reports extraction F1 **0.507** and Ask correctness **15 %**. `docs/final-report.md` (2026-10-27/28) reports **0.533** and **66.7 %** after the ADR-0010 key-order fix. | The thesis uses the post-fix figures and explains the earlier ones. The README predates the fix and should be updated. |
| C3 | `docs/hardware-backends.md` states "**no model has been run on any backend yet**". The final report and the evaluation results show real-model runs on Vulkan. | The thesis follows the final report. `hardware-backends.md` is stale on this point. |
| C4 | `README.md` §11.1 lists fact kinds as `action, decision, event, note, question`. The code and schema use `task, commitment, decision, event, question, note`. | The thesis uses the code's values, and notes where the evaluation groups *task* and *commitment* together as "action". |
| C5 | Redaction false-positive rate is reported as **0.43 % (1/235)** in `stages.md` and **0.41 % (1/242)** in `docs/stage7/verification.md`. | The thesis uses 0.41 % (1/242), from the later Stage 7 report. |
| C6 | Hallucination denominator is **0/63** in `docs/stage2/verification.md` and **0/97** in `evals/results/extraction_real_heldout.md`. | The thesis uses 0/97, from the results file for the final run. |
| C7 | The git history contains only 27 commits over three days (2026-10-26 to 2026-10-28), which does not reflect the real development period. | The thesis makes **no claim** about development duration or timeline. If your report needs one, supply the real dates. |

---

## D. Checks that were run on this document

- Every one of the 28 references is cited at least once in the text, and every in-text citation
  resolves to a reference. Verified programmatically.
- Every one of the 12 figures and 21 tables is placed, numbered sequentially within its chapter,
  captioned, and cross-referenced by number in the body text. Verified programmatically.
- Every `Section N.M` cross-reference resolves to a heading that exists. Verified programmatically.
- The contents, List of Figures and List of Tables carry real page numbers, read back from a
  rendered PDF of this exact document. No entry is unresolved.
- The DOCX and the PDF are the same document: the PDF is generated from the DOCX in the same build.
- Source lines per module (Table 4.1), test counts (Table 6.2) and all measured figures were read
  from the repository, not estimated.
- Reference entries for Qwen3-VL (arXiv:2511.21631), multilingual-E5 (arXiv:2402.05672), Greshake et
  al. on indirect prompt injection (arXiv:2302.12173), the OWASP LLM Top 10, Microsoft's Recall
  documentation and the llama.cpp repository were each confirmed against the live source. The
  remaining entries are standard published works and official vendor documentation.

---

## E. What is deliberately absent

- **No performance claim for the intended reference hardware.** The RTX 4050 was never available.
  Every latency figure is from the AMD integrated GPU and is labelled as such.
- **No screenshots of the running application.** None exist in the repository, and fabricating
  interface screenshots for a thesis would be dishonest. If you can capture real ones, Chapters 4
  and 6 are the natural homes for them and they can be added.
- **No claim that the system is complete or ready for release.** Chapters 6 and 7 state the opposite.
- **No claim of institutional approval.** The thesis is not approved until your guide says it is.

---

## F. Suggested next steps

1. Fill in A1–A4 and A6–A8, then rebuild or edit the DOCX directly.
2. Send the thesis to your guide with the title (A8) flagged for approval.
3. If you can get access to a discrete GPU, re-run `tsr-eval` for extraction, Ask and drafts; the
   latency figures in Section 6.7, 6.8 and Figure 6.3 would change substantially and the discussion
   in Section 7.4 already anticipates it.
4. Fix C1–C4 in the repository so that the README matches the verification reports — a reviewer who
   opens the repository will notice the discrepancy.
5. Add real screenshots if you can take them.
