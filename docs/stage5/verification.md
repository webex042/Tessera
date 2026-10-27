# Stage 5 verification report: draft assistant

Date: 2026-10-27. Decisions: [ADR-0008](../adr/0008-stages-4-6-and-real-model-guard.md).

## A. What was built

| task | where |
|---|---|
| Focus-target capture, editability and password checks | `tsr-capture::uia::{FocusTarget, UiaReader::focus}` (UIA RuntimeId, IsPassword, ValuePattern/ControlType) |
| Thread context (redacted, last ~2 000 chars), blocklist check, keyword memory | `src-tauri/src/drafts.rs::begin`, `retrieve` |
| P0 streaming draft, strict output, one retry | `tsr-draft::draft`, `ChatModel::chat_streaming`, `DraftRunner` |
| Code fact check (dates, times, weekdays, amounts, numbers, e-mails, URLs, file names, names), new commitments, injected text, secrets | `tsr-guard::claims`, `tsr-draft::factcheck` |
| Insertion: approval token, focus back, same-target re-check, private clipboard (history + cloud exclusion), Ctrl+V, restore after 500 ms; Copy fallback | `tsr-win::clipboard`, `Drafts::{insert, copy}` |
| Draft panel | `ui/src/notch/Draft.tsx` |
| Audit: `draft_inserted` / `draft_copied` / `draft_discarded` with length only | `tsr-store::audit` |

## B. Exit criteria

| criterion | measured | evidence | status |
|---|---|---|---|
| Works in Gmail web, Outlook (new), Teams, Slack, WhatsApp Desktop and a plain text field | REAL plain Win32 text field: target found, draft pasted, text verified | `crates/tsr-capture/tests/insert_real.rs` | **PARTIAL**: the five named apps need the owner (accounts); compat table not updated |
| Never inserts into a password field or a changed target (tested) | REAL: password field detected (`is_password=true`) and refused; a different focused element is detected (`same_as=false`); unknown password state counts as password | `insert_real.rs` | **PASS** |
| Clipboard restored; draft never in Win+V history or cloud clipboard | REAL: all memory formats restored with identical sizes; the three exclusion formats present during the paste. **History check skipped: clipboard history is off on this machine.** Cloud upload cannot be observed without the owner's account | `insert_real.rs`; `clipboard_history_real.rs` (SKIPPED) | **PARTIAL** |
| First visible text ≤ 3 s p50; complete ≤ 8 s p50 | real model: running at time of writing (`evals/results/draft_real_all.md`) | `tsr-eval draft --model DIR` | see results |
| 0 unsupported dates/amounts/names shown without a flag (20 scenarios) | SIMULATED drafter with invented claims: **0 unflagged** (independent detector); real model: running | `tsr-eval draft` | checker **PASS**; real model see results |

## C. Findings fixed during the stage

- **The clipboard was not restored** after the first real test: the sequence number was read
  while the clipboard was still open, and closing it advances the number, so `restore` always
  thought someone else had copied since. Fixed (read after closing). **During that run the
  developer machine's clipboard content was replaced by the test text and could not be recovered.**
- Text-only clipboard saving would refuse almost every real clipboard (HTML/RTF next to text):
  now every memory format is saved and restored.
- Foreground: a background test process cannot raise its own window; the test now receives its
  own registered hotkey first (what Windows accepts), and it refuses to paste when its window is
  not in front, so Ctrl+V can never land in another app.
- "I've" was flagged as a name.
