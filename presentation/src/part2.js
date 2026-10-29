/* ============================================================== 7. STACK */
pres.addSection({ title: "Build" });
{
  const s = content("Build", "Technology stack", "Every layer chosen for a constraint, not a preference");

  const layers = [
    ["Interface",
     "React 19 · TypeScript · Vite · Tailwind · Tauri 2.12.5",
     "Tauri hosts the UI in the system WebView2 runtime instead of bundling a browser engine, which keeps the binary small. Trade-off: WebView2 is also the one component whose network behaviour the project does not control."],
    ["Host and orchestration",
     "Rust 2024 (1.99+) · Cargo workspace, 10 crates",
     "Memory and thread safety matter because the capture path handles untrusted text on background threads. The workspace boundary is what makes the network check in slide 6 enforceable at compile time."],
    ["Operating system",
     "UI Automation · Windows Graphics Capture · WinRT OCR · DPAPI · Job Objects · display affinity",
     "There is no single way to read another application's text on Windows, so all three mechanisms are used in sequence. DPAPI ties the database key to the Windows login; job objects guarantee the worker dies with the host."],
    ["Storage and retrieval",
     "SQLCipher via rusqlite 0.41.0 · SQLite FTS5 · fastembed 7.5.0 on ONNX Runtime 1.31",
     "Whole-database AES-256 encryption including indexes and the write-ahead log. Default features of the embedding crates are switched off because they would pull in an HTTP client and a build-time download."],
    ["Inference",
     "llama.cpp b11900 · Qwen3-VL-4B-Instruct Q4_K_M · multilingual-e5-small",
     "The engine is an external, hash-verified binary rather than a linked library — the whole basis of the hardware independence on the next slide. Quantisation brings a 4B vision-language model to about 2.4 GB."],
  ];

  let y = 1.56;
  layers.forEach((l, i) => {
    H.card(s, { x: M, y, w: 11.8, h: 0.94, fill: i % 2 === 0 ? HEX.white : HEX.mist,
                line: i % 2 === 0 ? HEX.mist : undefined, name: `stk-${i}` });
    s.addText(l[0], {
      x: M + 0.2, y: y + 0.02, w: 1.9, h: 0.90, fontFace: F.body, fontSize: 11, bold: true,
      color: HEX.ink, align: "left", valign: "middle", margin: 0, isTextBox: true,
      objectName: `stk-l-${i}` });
    s.addText(l[1], {
      x: M + 2.14, y: y + 0.02, w: 3.28, h: 0.90, fontFace: F.mono, fontSize: 8.6,
      color: HEX.shu, align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.12,
      isTextBox: true, objectName: `stk-t-${i}` });
    s.addText(l[2], {
      x: M + 5.6, y: y + 0.02, w: 5.96, h: 0.90, fontFace: F.body, fontSize: 8.9,
      color: HEX.stone, align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.1,
      isTextBox: true, objectName: `stk-w-${i}` });
    y += 0.99;
  });

  H.caption(s, { x: M, y: 6.58, w: 11.8, size: 9, name: "stk-cap",
    text: "Versions are read from Cargo.toml. Security-sensitive dependencies are pinned to exact patch versions, not compatible ranges." });

  s.addNotes(
    "Do not read the stack as a list — each row answers 'why this, and what did it cost'.\n\n" +
    "The ONNX Runtime pin deserves a mention: the binding demands API level 31, which makes it " +
    "refuse any runtime older than 1.31. Windows ships its own copy at 1.17 in System32, and without " +
    "that requirement it would be silently accepted — the 'pinned runtime' guarantee would simply be " +
    "false. A test confirms the system copy is never mapped into the process.\n\n" +
    "If asked about the model choice: Qwen3-VL-4B was picked because the task — read a paragraph, " +
    "notice a commitment — is far easier than open-ended reasoning, and a 4B model fits in consumer " +
    "GPU memory. Slide 12 shows how that bet actually turned out.");
}

/* ============================================================== 8. IMPLEMENTATION */
{
  const s = content("Build", "Implementation deep dive", "Three decisions that define the system");

  const cols = [
    { title: "Hardware independence",
      problem: "GPU vendors ship incompatible runtimes. Linking one serves one vendor; linking all is unshippable.",
      approach: "Backends live outside the binary as separately pinned builds. At run time: enumerate adapters without loading any vendor runtime, hash-verify every file of each build against a compiled-in manifest, probe each build for real with a device-listing flag, read the model's own GGUF header, then rank candidates with the CPU always last.",
      result: "No vendor library is in the dependency graph; the application builds and runs with no GPU SDK installed. The model is never re-quantised to fit — an unsupported pairing is skipped.",
      caveat: "Two backend families verified on real hardware; five on simulated adapter reports only." },
    { title: "The 285 ms deadline",
      problem: "Accessibility reads are synchronous calls into another process. Microsoft Word blocked for 2.8 seconds in measurement.",
      approach: "Reads run on a dedicated apartment-threaded worker. The caller waits 285 ms and then proceeds without a result; the abandoned read finishes in the background and its answer is discarded.",
      result: "Across 16 windows and 9 applications the capture loop never blocked longer than 294 ms, while the abandoned reads themselves ran on for up to 732 ms.",
      caveat: "Word, PowerPoint and File Explorer hit the deadline every time and are served by OCR instead." },
    { title: "Evidence, or nothing",
      problem: "A model asked to summarise will produce fluent statements the source does not support — and a memory system is believed later.",
      approach: "Every proposed fact must carry a quotation. Code looks for it: exact substring, then after normalisation, then a fuzzy word-set match at Dice ≥ 0.9. A fact whose quote is absent is discarded, never repaired. The schema requires at least one evidence row and a trigger deletes a fact when its last one goes.",
      result: "Of 97 facts stored in the real-model run, zero carried evidence absent from the source, and zero rows violated the schema.",
      caveat: "Provenance is guaranteed; completeness is not — recall is the weak half (slide 12)." },
  ];

  const cw = 3.76, gx = 0.26;
  cols.forEach((c, i) => {
    const x = M + i * (cw + gx);
    H.card(s, { x, y: 1.5, w: cw, h: 4.88, fill: HEX.white, line: HEX.mist, name: `imp-${i}` });
    H.stepMark(s, { x: x + 0.2, y: 1.68, n: i + 1, d: 0.36 });
    s.addText(c.title, {
      x: x + 0.66, y: 1.68, w: cw - 0.86, h: 0.36, fontFace: F.body, fontSize: 13, bold: true,
      color: HEX.ink, align: "left", valign: "middle", margin: 0, isTextBox: true,
      objectName: `imp-t-${i}` });

    const rows = [["PROBLEM", c.problem], ["APPROACH", c.approach], ["RESULT", c.result]];
    let ry = 2.18;
    rows.forEach(([lab, txt], j) => {
      s.addText(lab, {
        x: x + 0.2, y: ry, w: cw - 0.4, h: 0.2, fontFace: F.body, fontSize: 7.6, bold: true,
        charSpacing: 1.4, color: j === 2 ? HEX.sage : HEX.stoneLight,
        align: "left", valign: "middle", margin: 0, isTextBox: true,
        objectName: `imp-lab-${i}-${j}` });
      const h = j === 1 ? 1.70 : (j === 0 ? 0.62 : 0.74);
      s.addText(txt, {
        x: x + 0.2, y: ry + 0.2, w: cw - 0.4, h, fontFace: F.body, fontSize: 9.2,
        color: j === 2 ? HEX.ink : HEX.stone, align: "left", valign: "top", margin: 0,
        lineSpacingMultiple: 1.12, isTextBox: true, objectName: `imp-txt-${i}-${j}` });
      ry += 0.2 + h + 0.1;
    });

    H.hairline(s, { x: x + 0.2, y: 5.86, w: cw - 0.4, color: HEX.mist, width: 0.75,
                    name: `imp-rule-${i}` });
    s.addText(c.caveat, {
      x: x + 0.2, y: 5.93, w: cw - 0.4, h: 0.42, fontFace: F.body, fontSize: 8.4, italic: true,
      color: HEX.stoneLight, align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1,
      isTextBox: true, objectName: `imp-cav-${i}` });
  });

  H.caption(s, { x: M, y: 6.5, w: 11.8, size: 9, name: "imp-cap",
    text: "A fourth decision underlies all three: the scheduler preempts background extraction by dropping its HTTP stream, so an interactive request never waits behind a summary." });

  s.addNotes(
    "Give roughly a minute to each column, in the Problem → Approach → Result shape printed on the slide.\n\n" +
    "Column 1 is the distinctive engineering contribution and the one an examiner is most likely to " +
    "probe. The honest caveat matters: the selection logic is tested against simulated adapter " +
    "reports for five vendors, which proves the arbitration code, not that any given driver works.\n\n" +
    "Column 2 is the smallest idea and the most practical. Without a deadline, one unresponsive " +
    "application freezes capture for everyone.\n\n" +
    "Column 3 is where the slide-3 principle becomes a measurement. Note the direction of the " +
    "guarantee: provenance, not completeness. Nothing stored is unsupported; plenty is missed.\n\n" +
    "If time allows, mention the key-order defect found during evaluation: schema keys were being " +
    "serialised alphabetically, so the grammar forced the model to commit to an 'insufficient' field " +
    "before writing any answer. Fixing it moved answer correctness from 15% to 66.7% with no prompt change.");
}

/* ============================================================== 9. SECURITY */
pres.addSection({ title: "Assurance" });
{
  const s = content("Assurance", "Security and user protection", "What is enforced, what is merely present, and what is out of reach");

  const bnds = [
    ["Interface → host", "Fixed command set; lengths, enumerations and identifiers validated; status changes need a single-use approval token; errors return a code, never content."],
    ["Host → worker", "Loopback only. The listening socket's owning process ID must equal the spawned child's, so no other local process can impersonate the worker."],
    ["Application → disk", "Whole-database AES-256. The key is unwrapped from the Windows login, used, then wiped. A wrong key is rejected and writes nothing to stderr."],
    ["Application → internet", "All network code in one crate, gated on a typed approval value and a pinned SHA-256 digest. Enforced at build time across the dependency graph."],
    ["Screen text → prompt", "Escaped and delimited, and the model has no tools. Its output is grammar-constrained, then re-validated, re-redacted and classified in code."],
  ];
  let y = 1.56;
  bnds.forEach((b, i) => {
    H.dot(s, { x: M + 0.02, y: y + 0.14, d: 0.12, color: HEX.slate, name: `sec-dot-${i}` });
    s.addText([
      { text: b[0], options: { bold: true, color: HEX.ink, breakLine: true } },
      { text: b[1], options: { color: HEX.stone } },
    ], { x: M + 0.26, y, w: 6.05, h: 0.92, fontFace: F.body, fontSize: 9.8,
         align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.14, isTextBox: true,
         objectName: `sec-${i}` });
    y += 0.96;
  });

  const statusX = 7.5, statusW = 5.05;
  const groups = [
    ["VERIFIED BY MEASUREMENT", HEX.sage, [
      "355 / 355 seeded secrets removed, 0.41 % false positives",
      "0 plaintext hits on disk in a 60-second canary scan",
      "0 hallucinated stored facts, 0 fabricated citations",
      "Insertion into a password field or a changed target refused",
      "Interface absent from two independent capture paths",
    ]],
    ["PRESENT BUT NOT ADEQUATELY TESTED", HEX.dryStone, [
      "24-hour network and canary runs — only 10 and 60 seconds run",
      "Password exclusion in native WinForms / WPF applications",
      "Clipboard-history and cloud-clipboard exclusion",
      "Coverage-guided fuzzing — randomised testing only",
    ]],
    ["KNOWN GAPS AND OUT OF SCOPE", HEX.shu, [
      "WebView2 opens one outbound HTTPS connection per run",
      "12 of 39 unseen injections still stored as readable to-dos",
      "Page file and hibernation file not scanned (needs elevation)",
      "An attacker holding the Windows account defeats the design",
    ]],
  ];
  let gy = 1.56;
  groups.forEach((g, i) => {
    H.card(s, { x: statusX, y: gy, w: statusW, h: g[2].length * 0.215 + 0.46,
                fill: i === 0 ? HEX.mist : HEX.white, line: i === 0 ? undefined : HEX.mist,
                name: `grp-${i}` });
    H.dot(s, { x: statusX + 0.18, y: gy + 0.19, d: 0.12, color: g[1], name: `grp-dot-${i}` });
    s.addText(g[0], {
      x: statusX + 0.4, y: gy + 0.1, w: statusW - 0.6, h: 0.28, fontFace: F.body, fontSize: 8.4,
      bold: true, charSpacing: 1.2, color: g[1], align: "left", valign: "middle", margin: 0,
      isTextBox: true, objectName: `grp-h-${i}` });
    s.addText(g[2].map((t, k) => ({ text: t, options: { breakLine: k < g[2].length - 1 } })), {
      x: statusX + 0.4, y: gy + 0.4, w: statusW - 0.6, h: g[2].length * 0.215,
      fontFace: F.body, fontSize: 8.9, color: HEX.stone, align: "left", valign: "top",
      margin: 0, lineSpacingMultiple: 1.12, isTextBox: true, objectName: `grp-b-${i}` });
    gy += g[2].length * 0.215 + 0.46 + 0.16;
  });

  H.caption(s, { x: M, y: 6.56, w: 11.8, size: 9, name: "sec-cap",
    text: "Tessera is not claimed to be fully sandboxed, air-gapped or immune to attack. The architecture bounds what a successful injection can do — no case in a 152-attack corpus caused an action, revealed a secret or wrote an unexpected table — but it does not prevent one." });

  s.addNotes(
    "Structure this as: here are the boundaries, and here is the honest status of each control.\n\n" +
    "The three-way split on the right is the whole point of the slide. Resist the temptation to " +
    "present everything as green.\n\n" +
    "On the WebView2 finding: every mitigation available was applied — framework defaults preserved, " +
    "reputation service disabled and read back, background networking off, proxy discovery off, " +
    "navigation locked, environment cleared. The connection still appears, it is absent from the " +
    "browser engine's own network log, and the host name was not in the resolver cache, so it comes " +
    "from code outside the engine's network service. The project recorded this as an open decision " +
    "rather than narrowing the criterion it violates.\n\n" +
    "If challenged on the injection number, that is the right instinct — slide 12 explains how it " +
    "was measured and why the honest figure is lower than the headline one.");
}

/* ============================================================== 10. INSTALL + USE */
{
  const s = content("Assurance", "Installation and usage", "Build it, approve a model, then switch capture on");

  s.addText("PREREQUISITES", {
    x: M, y: 1.54, w: 5.6, h: 0.24, fontFace: F.body, fontSize: 8.4, bold: true, charSpacing: 1.4,
    color: HEX.stoneLight, align: "left", valign: "middle", margin: 0, isTextBox: true,
    objectName: "pre-h" });
  s.addText(
    "Windows 11 64-bit · Rust stable MSVC (1.99+, edition 2024) · Visual Studio C++ build tools · " +
    "Strawberry Perl on PATH (the vendored OpenSSL build needs it) · Node.js 24+ with pnpm · Python 3.11+",
    { x: M, y: 1.78, w: 5.6, h: 0.72, fontFace: F.body, fontSize: 9.4, color: HEX.stone,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.14, isTextBox: true,
      objectName: "pre-b" });

  H.card(s, { x: M, y: 2.6, w: 5.6, h: 2.46, fill: HEX.ink, name: "cmd-card" });
  const cmd = [
    ["# interface bundle", HEX.stoneLight],
    ["pnpm install && pnpm build:ui", HEX.washi],
    ["", HEX.washi],
    ["# pinned, hash-checked inference runtimes", HEX.stoneLight],
    ["python tools/fetch_sidecar.py llama_cpp cpu-x64", HEX.washi],
    ["python tools/fetch_sidecar.py llama_cpp vulkan-x64", HEX.washi],
    ["", HEX.washi],
    ["# verify the boundaries, then build", HEX.stoneLight],
    ["python tools/check_network_deps.py --locked", HEX.washi],
    ["cargo test --workspace -- --include-ignored", HEX.washi],
    ["cargo build -p tessara --features custom-protocol", HEX.washi],
  ];
  s.addText(cmd.map((c, i) => ({
      text: c[0] || " ",
      options: { color: c[1], breakLine: i < cmd.length - 1 },
    })), {
    x: M + 0.26, y: 2.76, w: 5.08, h: 2.2, fontFace: F.mono, fontSize: 9,
    align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.16, isTextBox: true,
    objectName: "cmd-text" });

  H.caption(s, { x: M, y: 5.18, w: 5.6, size: 8.8, name: "cmd-cap",
    text: "Commands as given in README.md §18.2. The package is named tessara internally while the repository is Tessera — the build command uses the internal name." });

  const steps = [
    ["Approve a model", "Nothing is downloaded automatically. The download API needs a typed approval value that only an explicit user action can construct, and the file is accepted only if its SHA-256 matches tools/models.lock."],
    ["Switch capture on", "Capture starts paused on first run. Until you unpause it, nothing is read at all."],
    ["Work normally", "The notch sits at the top of the screen as a pill. Ctrl+Alt+Space opens it; if another application already owns that chord it falls back to Ctrl+Alt+N and the interface tells you which is active."],
    ["Ask, or draft", "Ctrl+Alt+K opens Ask; each answer sentence carries a citation chip that opens the underlying item. For a draft, put the cursor in the reply field first — insertion re-checks the target and never presses Enter."],
    ["Pause, or forget", "The hotkey or tray toggle halts capture instantly and drops buffered text. A person can be forgotten; only a salted digest of the identifier is kept, so the suppression list cannot be read back into names."],
  ];
  let y = 1.54;
  steps.forEach((st, i) => {
    H.stepMark(s, { x: 6.46, y: y + 0.02, n: i + 1, d: 0.36 });
    s.addText([
      { text: st[0], options: { bold: true, color: HEX.ink, breakLine: true } },
      { text: st[1], options: { color: HEX.stone } },
    ], { x: 6.96, y, w: 5.46, h: 1.0, fontFace: F.body, fontSize: 9.8,
         align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.14, isTextBox: true,
         objectName: `use-${i}` });
    y += 1.03;
  });

  s.addNotes(
    "Show the build, then spend the time on the right-hand column — that is the part a user " +
    "experiences.\n\n" +
    "Two defaults are deliberate and worth stating. Capture starts paused, so an unattended install " +
    "records nothing. And no model is bundled or fetched automatically; the approval value cannot be " +
    "constructed except from an explicit user action, which makes 'it downloaded something quietly' " +
    "structurally impossible rather than merely unlikely.\n\n" +
    "The fallback hotkey exists because of a real bug found by measurement: the default chord was " +
    "already registered by another application on the development machine, so the toggle silently " +
    "never worked.\n\n" +
    "Configuration lives in %LOCALAPPDATA%\\Tessara\\config.json — backend, device, layer count, " +
    "idle unload, capture cadence and a 90-day retention default.");
}

/* ============================================================== 11. USE CASES */
{
  const s = content("Assurance", "Real-world use cases", "Three situations the implementation already supports");

  const cases = [
    { who: "A student across a week of coursework",
      prob: "A submission date agreed in a chat on Monday is gone by Thursday. It was never written anywhere except on screen.",
      how: "Extraction records it as a commitment with the sentence it came from. It appears in Open Loops and in the morning brief.",
      comp: "tsr-pipeline · tsr-store",
      need: "Needs the window to have been readable and not blocklisted at the time." },
    { who: "An engineer returning to an old decision",
      prob: "“Why did we drop the second approach?” — the reasoning is in a thread nobody can find.",
      how: "Ask retrieves the snippets, the model answers from them, and each sentence shown is verified against the snippet it cites. Unverified sentences are hidden, not shown with a caveat.",
      comp: "tsr-retrieval · tsr-pipeline",
      need: "Correct on 66.7 % of a 60-question set with the 4B model, and slow on integrated graphics." },
    { who: "Anyone writing a reply with context",
      prob: "The reply needs a date, an amount and a name that are scattered across the thread and last week's mail.",
      how: "The draft assistant gathers the thread and relevant memory, generates at interactive priority, then an independent checker verifies every date, amount, number and name against the context and flags anything unsupported.",
      comp: "tsr-draft · tsr-guard",
      need: "Verified in a plain text field only; the five named mail and chat applications are untested." },
  ];

  const cw = 3.76, gx = 0.26;
  cases.forEach((c, i) => {
    const x = M + i * (cw + gx);
    H.card(s, { x, y: 1.56, w: cw, h: 4.24, fill: HEX.white, line: HEX.mist, name: `uc-${i}` });
    H.stepMark(s, { x: x + 0.2, y: 1.76, n: i + 1, d: 0.34 });
    s.addText(c.who, {
      x: x + 0.66, y: 1.74, w: cw - 0.86, h: 0.4, fontFace: F.body, fontSize: 11.5, bold: true,
      color: HEX.ink, align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.05,
      isTextBox: true, objectName: `uc-who-${i}` });

    s.addText(c.prob, {
      x: x + 0.2, y: 2.3, w: cw - 0.4, h: 0.78, fontFace: F.body, fontSize: 9.4, italic: true,
      color: HEX.stone, align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.14,
      isTextBox: true, objectName: `uc-p-${i}` });

    s.addText(c.how, {
      x: x + 0.2, y: 3.08, w: cw - 0.4, h: 1.5, fontFace: F.body, fontSize: 9.4, color: HEX.ink,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.14, isTextBox: true,
      objectName: `uc-h-${i}` });

    s.addText(c.comp, {
      x: x + 0.2, y: 4.52, w: cw - 0.4, h: 0.24, fontFace: F.mono, fontSize: 8.4, color: HEX.shu,
      align: "left", valign: "middle", margin: 0, isTextBox: true, objectName: `uc-c-${i}` });

    H.hairline(s, { x: x + 0.2, y: 4.84, w: cw - 0.4, color: HEX.mist, name: `uc-rule-${i}` });
    s.addText(c.need, {
      x: x + 0.2, y: 4.92, w: cw - 0.4, h: 0.72, fontFace: F.body, fontSize: 8.4, color: HEX.stoneLight,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
      objectName: `uc-n-${i}` });
  });

  H.caption(s, { x: M, y: 6.06, w: 11.8, name: "uc-cap",
    text: "Potential applications not yet built: meeting notes and live translation were specified for a later stage and are not implemented. Nothing here claims the system is pleasant to use at current latency." });

  s.addNotes(
    "Keep each case concrete and short. The grey line at the foot of each card is the constraint — " +
    "say it out loud, because an examiner will ask.\n\n" +
    "Case 2 is the one to be careful about. The pipeline works and fabricates nothing, but the 4B " +
    "model answers two in three correctly against a target of four in five, and takes about " +
    "25 seconds on the integrated GPU available for testing. It is a demonstration, not a product.\n\n" +
    "If asked what is deliberately not built: the system takes no autonomous action. It never sends " +
    "a message, never presses send, and never closes a commitment without confirmation — that last " +
    "one is enforced by a test that scans the source for any code path that writes a closed status.");
}

/* ============================================================== 12. TESTING */
{
  const s = content("Assurance", "Testing and validation", "Code-enforced guarantees held. Model-dependent ones did not");

  const chartData = [
    { name: "Target", labels: ["Extraction F1", "Answers correct", "Sentences verified"], values: [70, 80, 90] },
    { name: "Measured", labels: ["Extraction F1", "Answers correct", "Sentences verified"], values: [53.3, 66.7, 83.7] },
  ];
  s.addChart(pres.ChartType.bar, chartData, {
    x: M, y: 1.72, w: 5.9, h: 2.98,
    barDir: "col", barGapWidthPct: 60,
    chartColors: [HEX.dryStone, HEX.shu],
    showTitle: true, title: "Measured against targets fixed before implementation (%)",
    titleFontFace: "+mn-lt", titleFontSize: 10, titleColor: HEX.stone, titleAlign: "left",
    showValue: true, dataLabelPosition: "outEnd", dataLabelFontFace: "+mn-lt",
    dataLabelFontSize: 9, dataLabelColor: HEX.ink, dataLabelFormatCode: "0.0",
    catAxisLabelFontFace: "+mn-lt", catAxisLabelFontSize: 9, catAxisLabelColor: HEX.stone,
    valAxisLabelFontFace: "+mn-lt", valAxisLabelFontSize: 9, valAxisLabelColor: HEX.stone,
    valAxisMaxVal: 100, valAxisMajorUnit: 25,
    valGridLine: { color: HEX.mist, size: 0.75 },
    catGridLine: { style: "none" },
    showLegend: true, legendPos: "b", legendFontFace: "+mn-lt", legendFontSize: 9,
    legendColor: HEX.stone,
    plotArea: { fill: { color: HEX.washi } }, chartArea: { fill: { color: HEX.washi } },
    border: { pt: 0, color: HEX.washi },
  });

  H.caption(s, { x: M, y: 4.74, w: 5.9, size: 8.8, name: "chart-cap",
    text: "Qwen3-VL-4B Q4_K_M on an AMD Radeon integrated GPU via Vulkan. The intended reference machine, an RTX 4050, was never available, so every timing is a lower bound." });

  const met = [
    "355 / 355 seeded secrets removed · 0.41 % false positives",
    "0 plaintext hits across 285 files / 32 MB",
    "0 / 97 stored facts hallucinated · 0 schema violations",
    "0 fabricated citations",
    "0 commitments closed without confirmation",
    "Frame p95 16.8 ms · input-to-visual p95 64 ms",
    "100 % absent from Graphics Capture and GDI frames",
  ];
  const missed = [
    "Extraction F1 0.533 against 0.70",
    "Answers correct 66.7 % against 80 %",
    "Sentences verified 83.7 % against 90 %",
    "First answer sentence 25.2 s against 2.5 s",
    "Complete draft 16.4 s against 8 s",
    "WebView2: one outbound connection per run",
    "Injection: 24 / 39 on the untouched corpus half",
  ];

  const colX = 6.96, colW = 5.58;
  H.card(s, { x: colX, y: 1.72, w: colW, h: 2.26, fill: HEX.mist, name: "met-card" });
  H.dot(s, { x: colX + 0.2, y: 1.92, d: 0.12, color: HEX.sage, name: "met-dot" });
  s.addText("ELEVEN CRITERIA MET — ALL ENFORCED BY CODE", {
    x: colX + 0.42, y: 1.83, w: colW - 0.62, h: 0.28, fontFace: F.body, fontSize: 8.6, bold: true,
    charSpacing: 1.2, color: HEX.sage, align: "left", valign: "middle", margin: 0, isTextBox: true,
    objectName: "met-h" });
  s.addText(met.map((t, i) => ({ text: t, options: { breakLine: i < met.length - 1 } })), {
    x: colX + 0.42, y: 2.14, w: colW - 0.62, h: 1.76, fontFace: F.body, fontSize: 9,
    color: HEX.stone, align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.16,
    isTextBox: true, objectName: "met-b" });

  H.card(s, { x: colX, y: 4.1, w: colW, h: 2.26, fill: HEX.white, line: HEX.mist, name: "miss-card" });
  H.dot(s, { x: colX + 0.2, y: 4.3, d: 0.12, color: HEX.shu, name: "miss-dot" });
  s.addText("SEVEN NOT MET — MODEL, HARDWARE OR THIRD PARTY", {
    x: colX + 0.42, y: 4.21, w: colW - 0.62, h: 0.28, fontFace: F.body, fontSize: 8.6, bold: true,
    charSpacing: 1.2, color: HEX.shu, align: "left", valign: "middle", margin: 0, isTextBox: true,
    objectName: "miss-h" });
  s.addText(missed.map((t, i) => ({ text: t, options: { breakLine: i < missed.length - 1 } })), {
    x: colX + 0.42, y: 4.52, w: colW - 0.62, h: 1.76, fontFace: F.body, fontSize: 9,
    color: HEX.stone, align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.16,
    isTextBox: true, objectName: "miss-b" });

  s.addText(
    "375 Rust test functions · 355-case secrets corpus · 152-case injection corpus, half of it " +
    "held out from tuning · 80 extraction items · 60 questions · 20 draft scenarios · 20,000 " +
    "randomised inputs per robustness target. One criterion — the 8-hour resource and 7-day " +
    "stability run — has not been run at all.",
    { x: M, y: 5.6, w: 5.9, h: 1.2, fontFace: F.body, fontSize: 9, color: HEX.stone,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.16, isTextBox: true,
      objectName: "test-inventory" });

  s.addNotes(
    "This is the slide the deck exists for. The division is not random: every criterion met is one " +
    "enforced by deterministic code, and every criterion missed depends on the model's quality, the " +
    "GPU's throughput, or a third-party component.\n\n" +
    "On the injection figure, explain the method — it is the most defensible thing in the project. " +
    "The 152-case corpus was split in half; one half was used to tune the guard rules and the other " +
    "was never looked at. Against the tuned half the defence is near-perfect, which means nothing. " +
    "Against the untouched half it stops about 62 % with the real model. Those twelve failures have " +
    "deliberately been left unfixed so the number keeps meaning something — fixing them would turn " +
    "the held-out half into a second tuned half.\n\n" +
    "What holds regardless: across all 152 cases, nothing caused an action, revealed a secret in " +
    "clear, or wrote an unexpected table. The failures are confined to misleading text in a to-do list.\n\n" +
    "If asked about threats to validity: synthetic evaluation sets, no held-out split for Ask, only " +
    "7 of 20 target applications tested, and single-run results on a generative model that varies " +
    "between runs.");
}

/* ============================================================== 13. LIMITATIONS */
pres.addSection({ title: "Where it stands" });
{
  const s = content("Where it stands", "Limitations and trade-offs", "What is wrong, why, and what would fix it");

  const rows = [
    ["Extraction misses too much and over-produces trivia",
     "The 4B model generates many low-value note facts; precision on that kind is 0.21 against 23 false positives.",
     "F1 of 0.533 against a 0.70 gate, so the memory is noisier than it should be.",
     "Narrow the schema. Restricted to tasks and commitments the same run already reaches F1 0.68."],
    ["Interactive latency is roughly ten times the target",
     "Memory bandwidth on an integrated GPU, not an algorithmic fault. The intended discrete GPU was never available.",
     "About 25 s to an answer. Usable for background work, not for conversation.",
     "Re-measure on a discrete GPU before concluding anything; speculative decoding trades arithmetic for memory traffic."],
    ["Unseen injections still reach the task list",
     "The guard is pattern-based and conservative; social engineering phrased as ordinary mail evades it.",
     "12 of 39 untouched attacks stored as readable to-dos. None acted, leaked or wrote anything.",
     "A third, unseen corpus is required before any further tuning can be measured honestly."],
    ["The zero-network claim cannot be made for the whole process tree",
     "WebView2 opens one HTTPS connection per run despite every available mitigation; it is outside the application's code.",
     "The honest statement is narrower than the headline one, and the project says so in its own documentation.",
     "Identify the endpoint with an elevated capture; failing that, draw the notch natively and use WebView2 only for windows the user opens."],
    ["Five of seven backend families are unproven on hardware",
     "Only CPU and AMD Vulkan devices were available; the rest are exercised against simulated adapter reports.",
     "The arbitration logic is tested; the vendors' drivers are not.",
     "A closed beta on varied hardware is the only way to close this — simulated adapters cannot."],
  ];

  const colX = [M, M + 3.15, M + 6.05, M + 8.75];
  const colW = [2.95, 2.7, 2.5, 3.05];
  const heads = ["LIMITATION", "WHY IT EXISTS", "PRACTICAL IMPACT", "POSSIBLE IMPROVEMENT"];
  heads.forEach((h, i) => {
    s.addText(h, {
      x: colX[i], y: 1.5, w: colW[i], h: 0.24, fontFace: F.body, fontSize: 8, bold: true,
      charSpacing: 1.3, color: i === 3 ? HEX.sage : HEX.stoneLight,
      align: "left", valign: "middle", margin: 0, isTextBox: true, objectName: `lim-h-${i}` });
  });

  let y = 1.78;
  rows.forEach((r, i) => {
    const h = 0.86;
    if (i % 2 === 0) H.card(s, { x: M - 0.12, y: y - 0.05, w: 12.04, h: h + 0.02, fill: HEX.mist,
                                 name: `lim-bg-${i}` });
    r.forEach((cell, j) => {
      s.addText(cell, {
        x: colX[j], y, w: colW[j] - 0.18, h,
        fontFace: F.body, fontSize: 8.8,
        bold: j === 0, color: j === 0 ? HEX.ink : (j === 3 ? HEX.slate : HEX.stone),
        align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.12, isTextBox: true,
        objectName: `lim-${i}-${j}` });
    });
    y += h + 0.05;
  });

  H.caption(s, { x: M, y: 6.44, w: 11.8, size: 9, name: "lim-cap",
    text: "Not listed because they have not been measured rather than because they are absent: resource use over 8 hours, 7-day stability, and the contribution of the vector retrieval channel, which is implemented but was never run end to end." });

  s.addNotes(
    "Do not soften these. An examiner who finds a limitation you did not mention trusts the rest of " +
    "the deck less.\n\n" +
    "Row 1 carries the most useful insight in the project: the remedy for a model that is too small " +
    "is usually a narrower task, not a larger model. Tasks and commitments — the facts that actually " +
    "matter for a memory system — already reach 0.68.\n\n" +
    "Row 2: be disciplined about what is unknown. The hypothesis that a discrete GPU closes the gap " +
    "is untested and should not be asserted.\n\n" +
    "Row 3: the decision to leave known failures unfixed is deliberate and worth defending. It keeps " +
    "the generalisation estimate meaningful.\n\n" +
    "Row 4: this is the difference between 'our code makes no connections', which is true and " +
    "measured, and 'nothing leaves your PC', which is not currently defensible.");
}

/* ============================================================== 14. ROADMAP */
{
  const s = content("Where it stands", "Roadmap", "Close the measurement gaps before building anything new");

  const bands = [
    { tag: "IMPLEMENTED TODAY", color: HEX.sage, fill: HEX.mist,
      items: ["Capture ladder, redaction and encrypted store",
              "Extraction with evidence verification and the injection guard",
              "Hybrid retrieval, Ask with per-sentence citations",
              "Rollups, morning brief, open loops, draft assistant",
              "Backend detection, selection and fallback; notch interface"] },
    { tag: "NEXT — CLOSE THE GAPS (NOT YET DONE)", color: HEX.shu, fill: HEX.white,
      items: ["Re-run extraction, Ask and drafts on a discrete GPU",
              "Curb low-value note facts to lift precision",
              "Commission a third, unseen injection corpus",
              "24-hour network and canary runs; 8-hour and 7-day stability runs",
              "Resolve or formally document the WebView2 connection"] },
    { tag: "LATER — PLANNED, NOT STARTED", color: HEX.stoneLight, fill: HEX.white,
      items: ["Meeting capture and local translation (specified, Stage 8)",
              "Closed beta on varied hardware — the only route to real backend coverage",
              "Signed installer, SBOM per release, opt-in signed updater",
              "Speculative decoding for memory-bandwidth-limited GPUs",
              "Cross-platform acquisition behind the existing Platform trait"] },
  ];

  const cw = 3.76, gx = 0.26;
  bands.forEach((b, i) => {
    const x = M + i * (cw + gx);
    H.card(s, { x, y: 1.56, w: cw, h: 2.86, fill: b.fill,
                line: b.fill === HEX.white ? HEX.mist : undefined, name: `rm-${i}` });
    H.dot(s, { x: x + 0.2, y: 1.78, d: 0.12, color: b.color, name: `rm-dot-${i}` });
    s.addText(b.tag, {
      x: x + 0.42, y: 1.69, w: cw - 0.62, h: 0.28, fontFace: F.body, fontSize: 8.3, bold: true,
      charSpacing: 1.2, color: b.color, align: "left", valign: "middle", margin: 0,
      isTextBox: true, objectName: `rm-h-${i}` });
    s.addText(b.items.map((t, k) => ({
        text: t, options: { bullet: true, breakLine: k < b.items.length - 1 },
      })), {
      x: x + 0.42, y: 2.04, w: cw - 0.66, h: 2.7, fontFace: F.body, fontSize: 9.6,
      color: HEX.stone, align: "left", valign: "top", margin: 0, paraSpaceAfter: 6,
      isTextBox: true, objectName: `rm-b-${i}` });
  });

  H.card(s, { x: M, y: 4.72, w: 11.8, h: 1.1, fill: HEX.ink, name: "rm-note" });
  s.addText([
    { text: "The ordering is the point.  ", options: { bold: true, color: HEX.washi } },
    { text: "Stages 8 to 10 were gated behind the release criteria before any of them were measured, " +
            "and that gate has not been met. Building meeting capture on top of an extraction pipeline " +
            "that scores 0.533 would add surface area to something not yet known to work.",
      options: { color: HEX.dryStone } },
  ], { x: M + 0.3, y: 4.88, w: 11.2, h: 0.8, fontFace: F.body, fontSize: 10.5,
       align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.16, isTextBox: true,
       objectName: "rm-note-text" });

  s.addNotes(
    "Keep the three columns clearly separated — implemented, not yet done, not started. Conflating " +
    "them is the most common way a project report loses credibility.\n\n" +
    "The dark band at the bottom is the argument: the project's own staging gates further work " +
    "behind criteria that have not been met. That is a deliberate engineering decision, not a delay.\n\n" +
    "If asked what you would do first with one more month: re-measure on a discrete GPU and narrow " +
    "the extraction schema. Those two address the largest gap at the lowest cost, and neither " +
    "requires new architecture.");
}

/* ============================================================== 15. CONCLUSION */
{
  const s = pres.addSlide({ masterName: "CLOSE_DARK", sectionTitle: "Where it stands" });
  s.addText("CONCLUSION", { placeholder: "kicker" });
  s.addText("Put the guarantees where they can be tested", { placeholder: "title" });

  H.enso(s, { x: 11.72, y: 0.52, d: 0.62, color: "4A443D", w: 1, name: "close-enso" });
  H.dot(s, { x: 11.99, y: 0.79, d: 0.08, color: HEX.shu, name: "close-dot" });

  const points = [
    ["The problem", "An assistant is useful because it can see your screen, which is exactly why you would not want it sent away."],
    ["The approach", "Run the model locally, give it no tools, and let deterministic code verify everything it proposes before anything is stored or shown."],
    ["The contribution", "An architecture in which privacy and provenance are enforced by code, and a measurement of which of its guarantees survived a real 4B model on commodity hardware."],
    ["The finding", "Every criterion enforced by code was met. Every criterion depending on the model, the GPU or a third-party runtime was not. That split is the most portable result here."],
  ];
  let y = 1.66;
  points.forEach((p, i) => {
    H.dot(s, { x: M + 0.02, y: y + 0.13, d: 0.11, color: HEX.shu, name: `cl-dot-${i}` });
    s.addText([
      { text: p[0], options: { bold: true, color: HEX.washi, breakLine: true } },
      { text: p[1], options: { color: HEX.dryStone } },
    ], { x: M + 0.26, y, w: 6.0, h: 1.04, fontFace: F.body, fontSize: 10.6,
         align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.18, isTextBox: true,
         objectName: `cl-${i}` });
    y += 1.1;
  });

  s.addText("RESOURCES", {
    x: 7.2, y: 1.66, w: 5.35, h: 0.26, fontFace: F.body, fontSize: 8.6, bold: true,
    charSpacing: 1.4, color: HEX.stoneLight, align: "left", valign: "middle", margin: 0,
    isTextBox: true, objectName: "res-h" });

  const links = [
    ["Tessera repository", "github.com/webex042/Tessera",
     "https://github.com/webex042/Tessera", "Source, README, decision records, stage verification reports and the evaluation results quoted throughout this deck."],
    ["llama.cpp", "github.com/ggml-org/llama.cpp",
     "https://github.com/ggml-org/llama.cpp", "The inference engine, pinned at build b11900."],
    ["Qwen3-VL technical report", "arxiv.org/abs/2511.21631",
     "https://arxiv.org/abs/2511.21631", "The vision-language model family used here."],
    ["Indirect prompt injection", "arxiv.org/abs/2302.12173",
     "https://arxiv.org/abs/2302.12173", "Greshake et al., AISec 2023 — the attack slide 9 defends against."],
    ["OWASP Top 10 for LLM Applications", "owasp.org",
     "https://owasp.org/www-project-top-10-for-large-language-model-applications/",
     "Prompt injection as the first entry; the framing used for the threat model."],
  ];
  let ly = 2.0;
  links.forEach((l, i) => {
    s.addText([
      { text: l[0] + "  ", options: { bold: true, color: HEX.washi } },
      { text: l[1], options: { color: HEX.shu, underline: true, hyperlink: { url: l[2] } } },
    ], { x: 7.2, y: ly, w: 5.35, h: 0.24, fontFace: F.body, fontSize: 9.6,
         align: "left", valign: "middle", margin: 0, isTextBox: true, objectName: `lnk-${i}` });
    s.addText(l[3], {
      x: 7.2, y: ly + 0.24, w: 5.35, h: 0.46, fontFace: F.body, fontSize: 8.2,
      color: HEX.stoneLight, align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.08,
      isTextBox: true, objectName: `lnk-d-${i}` });
    ly += 0.82;
  });

  s.addText("Full thesis — \u201cTessera: A Privacy-Preserving, Hardware-Independent Local Inference and Screen-Aware Memory System for Windows\u201d — is in the repository under thesis/, with the evaluation data behind every number in this deck.", {
    x: M, y: 6.28, w: 11.8, h: 0.44, fontFace: F.body, fontSize: 8.6, italic: true,
    color: HEX.stoneLight, align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1,
    isTextBox: true, objectName: "thesis-ref" });

  s.addNotes(
    "Close on the finding, not on a thank-you. The four lines on the left are the whole talk in " +
    "order: problem, approach, contribution, result.\n\n" +
    "The last line is the one to land. It was not obvious in advance which guarantees would depend " +
    "on the model and which would not; the evaluation answers that empirically, and the answer tells " +
    "anyone building a similar system where to put their effort.\n\n" +
    "Offer the repository for anyone who wants to check a number — the corpora, the harness and the " +
    "raw results are all in it, including the runs that failed.\n\n" +
    "Likely questions: why not a larger model (narrower task is cheaper than more parameters); why " +
    "Rust (untrusted text on background threads); why not use Windows Recall (different guarantee — " +
    "Tessera stores verifiable spans rather than snapshots, and publishes its adversarial results).");
}

/* ============================================================== write */
(async () => {
  const out = process.argv[2] || "Tessera_Technical_Project_Presentation.pptx";
  await pres.writeFile({ fileName: out });
  const { applyTheme } = require(SKILL_DIR + "/scripts/apply_theme.js");
  await applyTheme(out, THEME);
  console.log("written:", out);
})();
