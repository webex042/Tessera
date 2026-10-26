# App compatibility (capture rungs)

Measured with `tools/run-capture-probe.ps1` (raw numbers: `docs/stage0/app-compat-probe.csv`).
Content is synthetic fixtures from `tests/fixtures/screens/`, not real app data, so this proves the
APIs work against each app's UI framework, not how well real inboxes or chats read.
"Usable" = at least 40 characters at quality ≥ 0.55 from UIA (rung 1) or WGC + Windows OCR (rung 2).

Machine: Windows 11 build 26300, Ryzen 5 7530U, 2026-10-26. Times are single measurements.

**Update (Stage 1, production UIA reader with a 285 ms deadline):** every read returned to the
caller within **294 ms** (p95 294 ms). Word, PowerPoint and Explorer hit the deadline (their
abandoned reads finished in the background at 290, 647 and 732 ms) and are read by OCR. In
both browsers' password fixture, TextPattern is now refused because the document contains a
password field; the cached tree walk skips that control (`uia_pw_skipped = 1`). All 16 windows
usable. The table below is the Stage 0 measurement; the current CSV is `docs/stage0/app-compat-probe.csv`.

| # | Target app | Status | Rung 1 UIA | Rung 2 WGC + OCR | Notes |
|---|---|---|---|---|---|
| 1 | Chrome | usable | TextPattern 62–74 ms; first page tree walk 231 ms | 179–199 + 87–94 ms | isolated profile |
| 2 | Edge | usable | TextPattern 68–108 ms; first page tree walk 319 ms | 154–243 + 64–108 ms | isolated profile |
| 3 | Firefox | not installed | | | |
| 4 | Outlook (new and classic) | not tested | | | would show the owner's mailbox |
| 5 | Gmail web | not tested | | | needs a real Gmail session; browser fixture approximates only |
| 6 | Teams | not tested | | | signed-in account |
| 7 | Slack | not installed | | | |
| 8 | Discord | not installed | | | |
| 9 | WhatsApp Desktop | not tested | | | signed-in account |
| 10 | VS Code | usable (OCR) | tree walk 155 ms, only 54 chars (editor not exposed without screen-reader mode) | 214 + 77 ms, 306 chars | isolated user-data dir |
| 11 | Word | usable (OCR) | **failed: UIA timeout after 2 794 ms** | 346 + 125 ms, 608 chars | |
| 12 | Excel | usable | TextPattern 149 ms | 216 + 150 ms | CSV fixture |
| 13 | PowerPoint | usable | tree walk 559 ms, 1 263 chars | 170 + 70 ms | deck built via COM in %TEMP% |
| 14 | OneNote | not tested | | | shows the owner's notebooks |
| 15 | Notion | not installed | | | |
| 16 | Obsidian | not installed | | | |
| 17 | Figma | not installed | | | |
| 18 | Telegram | not installed | | | |
| 19 | Zoom | not installed | | | |
| 20 | File Explorer | usable (OCR) | **failed: timeout after 628 ms** | 258 + 78 ms, 62 chars | shared explorer.exe; matched by folder title |
| — | Notepad | usable | TextPattern 40 ms | 159 + 71 ms | baseline, not in the 20 |

Tally against the Stage 0 gate (≥ 16 of 20): **7 tested, 7 usable**, 8 not installed, 5 not
testable without the owner's accounts. Gate status: UNVERIFIED.

Password field (Edge, Chrome): the canary value in a `type=password` input never appeared in UIA
or OCR text (`--assert-absent`).
