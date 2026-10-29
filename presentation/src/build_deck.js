#!/usr/bin/env node
/* Tessera — technical project presentation.
   Zen in appearance, precise in engineering. 15 slides, 16:9.
   Every factual claim traces to the repository: README.md, stages.md,
   docs/final-report.md, docs/stage{0,2,3,4,5,6,7}/*, evals/results/*, Cargo.toml. */

const pptxgen = require("pptxgenjs");
const { THEME, HEX } = require("./theme");
const H = require("./helpers");
const { F } = H;

const pres = new pptxgen();
pres.layout = "LAYOUT_WIDE";            // 13.3 x 7.5
pres.theme = { headFontFace: THEME.headFontFace, bodyFontFace: THEME.bodyFontFace };
pres.author = "Pranshu Ranjan";
pres.company = "Birla Institute of Technology, Mesra";
pres.subject = "Tessera — technical project presentation";
pres.title = "Tessera";

const C = pres.SchemeColor;
const M = 0.75;                 // page margin
const W = 13.3 - 2 * M;         // content width = 11.8
const FOOT = "Tessera  ·  B.Tech Project  ·  BIT Mesra, Off-Campus Patna";

/* ------------------------------------------------------------------ layouts */

pres.defineSlideMaster({
  title: "TITLE_DARK",
  background: { color: HEX.ink },
  objects: [
    { placeholder: { options: { name: "kicker", type: "body", x: M, y: 2.05, w: 7.6, h: 0.3,
        fontFace: F.body, fontSize: 10, bold: true, charSpacing: 3, color: HEX.stoneLight,
        align: "left", valign: "middle", margin: 0 }, text: "" } },
    { placeholder: { options: { name: "title", type: "title", x: M, y: 2.42, w: 7.6, h: 1.25,
        fontFace: F.head, fontSize: 66, bold: false, color: HEX.washi,
        align: "left", valign: "middle", margin: 0 }, text: "" } },
    { placeholder: { options: { name: "body", type: "body", x: M, y: 3.78, w: 7.0, h: 0.95,
        fontFace: F.body, fontSize: 14.5, color: HEX.dryStone,
        align: "left", valign: "top", margin: 0 }, text: "" } },
  ],
});

pres.defineSlideMaster({
  title: "CONTENT",
  background: { color: HEX.washi },
  objects: [
    { placeholder: { options: { name: "kicker", type: "body", x: M, y: 0.40, w: W, h: 0.26,
        fontFace: F.body, fontSize: 9.5, bold: true, charSpacing: 2.5, color: HEX.stoneLight,
        align: "left", valign: "middle", margin: 0 }, text: "" } },
    { placeholder: { options: { name: "title", type: "title", x: M, y: 0.68, w: W, h: 0.62,
        fontFace: F.head, fontSize: 30, bold: false, color: HEX.ink,
        align: "left", valign: "middle", margin: 0 }, text: "" } },
    { text: { text: FOOT, options: { x: M, y: 6.95, w: 7.5, h: 0.28,
        fontFace: F.body, fontSize: 8, color: HEX.dryStone,
        align: "left", valign: "middle", margin: 0, isTextBox: true } } },
  ],
  slideNumber: { x: 12.3, y: 6.95, w: 0.4, h: 0.28, fontFace: F.body, fontSize: 9,
                 color: HEX.stoneLight, align: "right" },
});

pres.defineSlideMaster({
  title: "CLOSE_DARK",
  background: { color: HEX.ink },
  objects: [
    { placeholder: { options: { name: "kicker", type: "body", x: M, y: 0.40, w: W, h: 0.26,
        fontFace: F.body, fontSize: 9.5, bold: true, charSpacing: 2.5, color: HEX.stoneLight,
        align: "left", valign: "middle", margin: 0 }, text: "" } },
    { placeholder: { options: { name: "title", type: "title", x: M, y: 0.68, w: W, h: 0.62,
        fontFace: F.head, fontSize: 30, bold: false, color: HEX.washi,
        align: "left", valign: "middle", margin: 0 }, text: "" } },
    { text: { text: FOOT, options: { x: M, y: 6.95, w: 7.5, h: 0.28,
        fontFace: F.body, fontSize: 8, color: HEX.stone,
        align: "left", valign: "middle", margin: 0, isTextBox: true } } },
  ],
  slideNumber: { x: 12.3, y: 6.95, w: 0.4, h: 0.28, fontFace: F.body, fontSize: 9,
                 color: HEX.stone, align: "right" },
});

/* helper: content slide with kicker + title already filled */
function content(sectionTitle, kicker, title) {
  const s = pres.addSlide({ masterName: "CONTENT", sectionTitle });
  s.addText(kicker.toUpperCase(), { placeholder: "kicker" });
  s.addText(title, { placeholder: "title" });
  return s;
}

/* ============================================================== 1. TITLE */
pres.addSection({ title: "Opening" });
{
  const s = pres.addSlide({ masterName: "TITLE_DARK", sectionTitle: "Opening" });

  // ensō motif: two thin concentric circles, one small filled dot off-centre
  H.enso(s, { x: 8.95, y: 1.72, d: 3.55, color: "3A3733", w: 1.25, name: "enso-outer" });
  H.enso(s, { x: 9.52, y: 2.29, d: 2.41, color: HEX.shu, w: 1, name: "enso-inner" });
  H.dot(s, { x: 10.63, y: 3.40, d: 0.19, color: HEX.shu, name: "enso-centre" });

  s.addText("B.TECH PROJECT  ·  COMPUTER SCIENCE AND ENGINEERING", { placeholder: "kicker" });
  s.addText("Tessera", { placeholder: "title" });
  s.addText(
    "A privacy-preserving, hardware-independent local inference and screen-aware memory system for Windows",
    { placeholder: "body" });

  s.addText([
    { text: "Pranshu Ranjan", options: { bold: true, color: HEX.washi } },
    { text: "   ·   BTECH/15191/24   ·   Guide: Dr. Ayan Kumar Das", options: { color: HEX.stoneLight } },
  ], { x: M, y: 5.25, w: 7.6, h: 0.28, fontFace: F.body, fontSize: 12,
       align: "left", valign: "middle", margin: 0, isTextBox: true, objectName: "author" });

  s.addText("Birla Institute of Technology, Mesra — Off-Campus Patna", {
    x: M, y: 5.56, w: 7.6, h: 0.28, fontFace: F.body, fontSize: 11, color: HEX.dryStone,
    align: "left", valign: "middle", margin: 0, isTextBox: true, objectName: "institute" });

  H.dot(s, { x: M, y: 6.26, d: 0.11, color: HEX.shu, name: "status-dot" });
  s.addText("Stages 0–7 · v0.0.1 · research prototype under evaluation, not a released product", {
    x: M + 0.22, y: 6.14, w: 8.0, h: 0.3, fontFace: F.body, fontSize: 10, color: HEX.stone,
    align: "left", valign: "middle", margin: 0, isTextBox: true, objectName: "status" });

  s.addNotes(
    "Open with the one-line description, then set expectations honestly: this is a working research " +
    "prototype at version 0.0.1, evaluated against criteria fixed in advance, and several of those " +
    "criteria are not met. Say that up front — the honesty is part of the contribution.\n\n" +
    "Tessera keeps a searchable memory of what you worked on, using a language model that runs on " +
    "your own machine. Two engineering problems shape it: running a 4-billion-parameter " +
    "vision-language model across unrelated GPU vendors, and making sure screen text that may " +
    "contain hostile instructions cannot become instructions to the system.");
}

/* ============================================================== 2. PROBLEM */
pres.addSection({ title: "Problem and idea" });
{
  const s = content("Problem and idea", "The problem", "Useful context is private context");

  s.addText(
    "An assistant that knows what you are working on is only useful because it can see what you are " +
    "working on: your mail, your chats, your documents. Almost every product that does this sends " +
    "that text to a remote server, because the strongest models are too large to run on a laptop.",
    { x: M, y: 1.58, w: 5.55, h: 1.25, fontFace: F.body, fontSize: 13, color: HEX.ink,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.22, isTextBox: true,
      objectName: "problem-lede" });

  s.addText(
    "Running the model locally removes that exposure — and replaces it with four engineering problems " +
    "that the cloud design hides.",
    { x: M, y: 2.95, w: 5.55, h: 0.75, fontFace: F.body, fontSize: 13, color: HEX.ink, bold: true,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.22, isTextBox: true,
      objectName: "problem-turn" });

  H.caption(s, { x: M, y: 3.98, w: 5.55, name: "problem-who",
    text: "Who is affected: anyone whose screen holds work they cannot send to a third party — " +
          "students, clinicians, lawyers, engineers under NDA." });

  const items = [
    ["Hardware fragmentation",
     "CUDA, ROCm, SYCL, Metal and OpenCL are mutually incompatible. Ship one and you serve one vendor; ship all and the CUDA runtime alone is ~390 MB."],
    ["Process fragility",
     "A 2–6 GB model pushes consumer GPUs to their limit. In-process inference means a driver reset takes the application, and possibly the database, with it."],
    ["Exposure of captured text",
     "A system that reads the screen will eventually read a password manager, a banking page or an identity number. By the time the model sees it, the exposure has happened."],
    ["Misplaced authority",
     "A web page can contain a sentence addressed to the model, not the user. A model cannot reliably tell its operator's instructions from text it was asked to read."],
  ];
  let y = 1.58;
  items.forEach(([t, b], i) => {
    H.stepMark(s, { x: 6.68, y: y + 0.03, n: i + 1 });
    H.cardText(s, { x: 7.26, y, w: 5.3, title: t, body: b, titleSize: 12.5, bodySize: 10,
                    gap: 0.24, name: `prob-${i}` });
    y += 1.33;
  });

  s.addNotes(
    "Lead with the tension, not the technology: the context that makes such a system useful is " +
    "exactly the context a person would least like to transmit.\n\n" +
    "Then make the pivot explicit — moving the model on-device solves the privacy problem and buys " +
    "four new ones. The audience should leave this slide knowing that the rest of the talk is about " +
    "those four.\n\n" +
    "Point 4 is the one people underestimate. Indirect prompt injection (Greshake et al., AISec 2023) " +
    "is the first entry in the OWASP Top 10 for LLM Applications, and it is the hardest of the four " +
    "to solve — slide 12 shows how far we actually got.");
}

/* ============================================================== 3. THE IDEA */
{
  const s = content("Problem and idea", "The idea", "The model proposes. Code disposes");

  s.addText(
    "No guarantee offered to the user may depend on the language model behaving correctly. Wherever " +
    "the model produces something, deterministic code checks it before it has any effect.",
    { x: M, y: 1.55, w: 11.8, h: 0.52, fontFace: F.body, fontSize: 13, color: HEX.ink,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.2, isTextBox: true,
      objectName: "idea-lede" });

  const rows = [
    ["Where the text goes",
     "To a provider's servers",
     "Never leaves the machine; inference on 127.0.0.1"],
    ["What the model is trusted with",
     "Reading, judging and often acting",
     "Proposing JSON only — it has no tools and cannot act"],
    ["What you can check afterwards",
     "The answer, and whether it sounds right",
     "Every fact carries the verbatim span it came from"],
    ["What an injected instruction achieves",
     "Whatever the model's tools allow",
     "At most a misleading line of text you read"],
  ];

  const xA = M, xB = 4.45, xC = 8.65, cardW = 3.9;

  s.addText("CONVENTIONAL ASSISTANT", {
    x: xB, y: 2.2, w: 3.7, h: 0.26, fontFace: F.body, fontSize: 9, bold: true,
    charSpacing: 1.6, color: HEX.stoneLight, align: "left", valign: "middle",
    margin: 0, isTextBox: true, objectName: "hdr-conv" });
  s.addText("TESSERA", {
    x: xC + 0.22, y: 2.2, w: 2.4, h: 0.26, fontFace: F.body, fontSize: 9, bold: true,
    charSpacing: 1.6, color: HEX.shu, align: "left", valign: "middle",
    margin: 0, isTextBox: true, objectName: "hdr-tess" });

  let ry = 2.5;
  rows.forEach((r, i) => {
    const h = 0.86;
    H.card(s, { x: xC, y: ry, w: cardW, h, fill: HEX.mist, name: `cmp-hi-${i}` });
    s.addText(r[0], {
      x: xA, y: ry, w: 3.5, h, fontFace: F.body, fontSize: 11.5, bold: true, color: HEX.ink,
      align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
      objectName: `cmp-k-${i}` });
    s.addText(r[1], {
      x: xB, y: ry, w: 3.9, h, fontFace: F.body, fontSize: 11, color: HEX.stone,
      align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
      objectName: `cmp-a-${i}` });
    s.addText(r[2], {
      x: xC + 0.22, y: ry, w: cardW - 0.44, h, fontFace: F.body, fontSize: 11, color: HEX.ink,
      align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
      objectName: `cmp-b-${i}` });
    ry += h + 0.08;
  });

  H.caption(s, { x: M, y: 6.42, w: 11.8, name: "idea-cap",
    text: "The right-hand column is enforced by code paths and database constraints, not by prompt wording — which is why slide 12 can report a hallucination rate of zero while the model's accuracy misses its target." });

  s.addNotes(
    "This is the thesis of the whole project, so spend time here.\n\n" +
    "Intuitive version: don't ask the model to be trustworthy; arrange things so it does not need to be.\n\n" +
    "Engineering version: the model is given no tools, its output is constrained by a JSON grammar, " +
    "and deterministic Rust then checks every proposed fact against the source text before anything " +
    "is stored. A fact whose quotation cannot be located in the source is discarded, never repaired.\n\n" +
    "Row 4 is the honest one. The architecture bounds what a successful injection can do, but it does " +
    "not eliminate it — an injected line can still be stored as text the user reads. Slide 12 quantifies that.");
}

/* ============================================================== 4. HOW IT WORKS */
pres.addSection({ title: "How it works" });
{
  const s = content("How it works", "How it works", "From a window changing to an answer you can check");

  const steps = [
    ["Window changes", "Focus or title event from the OS hook"],
    ["Gates and blocklist", "Paused, locked, fullscreen, private window → stop"],
    ["Read the text", "UI Automation → screen capture + OCR → vision model"],
    ["Redact", "Checksum-validated: cards, Aadhaar, PAN, keys, OTPs"],
    ["Store encrypted", "Session written as one item to SQLCipher"],
  ];

  const bw = 2.16, bh = 1.0, gapx = 0.26;
  let x = M;
  steps.forEach((st, i) => {
    H.stepMark(s, { x, y: 1.62, n: i + 1, d: 0.38 });
    H.box(s, { x, y: 2.12, w: bw, h: bh, label: st[0], sub: st[1], name: `flow-${i}`,
               size: 11, subSize: 8.5 });
    if (i < steps.length - 1) H.arrow(s, { x: x + bw + 0.04, y: 2.62, w: gapx - 0.08, name: `flow-arr-${i}` });
    x += bw + gapx;
  });

  // store → extraction: down out of the store box, left across, then down into Extraction
  const storeX = M + 4 * (bw + gapx) + bw / 2;
  s.addShape("line", { x: storeX, y: 3.12, w: 0, h: 0.26,
    line: { color: HEX.dryStone, width: 1 }, objectName: "elbow-down" });
  s.addShape("line", { x: 3.25, y: 3.38, w: storeX - 3.25, h: 0,
    line: { color: HEX.dryStone, width: 1 }, objectName: "elbow-across" });
  H.arrowDown(s, { x: 3.25, y: 3.38, h: 0.22, name: "elbow-in" });
  s.addText("the stored item queues an extraction job", {
    x: 5.9, y: 3.12, w: 3.4, h: 0.22, fontFace: F.body, fontSize: 8, color: HEX.stone,
    align: "center", valign: "middle", margin: 0, isTextBox: true, objectName: "elbow-label" });

  H.box(s, { x: M, y: 3.62, w: 5.0, h: 0.86, fill: HEX.mist, border: HEX.dryStone,
             label: "Extraction (background, preemptible)",
             sub: "Model proposes facts in JSON · code verifies the quote, classifies injection, redacts again, resolves dates and people",
             size: 11, subSize: 8.5, name: "extract-box" });

  H.arrow(s, { x: 5.82, y: 4.05, w: 0.3, name: "extract-arrow" });

  const outs = [
    ["Ask with sources", "Every sentence carries a citation; unverified sentences are hidden, not flagged"],
    ["Open loops and brief", "Commitments surfaced; nothing is ever closed without your confirmation"],
    ["Contextual draft", "Checked claim by claim, inserted only after you press Insert"],
  ];
  let ox = 6.2;
  outs.forEach((o, i) => {
    H.box(s, { x: ox, y: 3.62, w: 2.08, h: 0.86, label: o[0], sub: "", size: 10.5, name: `out-${i}` });
    s.addText(o[1], {
      x: ox, y: 4.56, w: 2.08, h: 0.95, fontFace: F.body, fontSize: 8.8, color: HEX.stone,
      align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
      objectName: `out-sub-${i}` });
    ox += 2.3;
  });

  H.hairline(s, { x: M, y: 5.72, w: 11.8, color: HEX.dryStone, width: 0.5, name: "sep" });

  const notes = [
    ["Capture starts paused", "Nothing is read until you switch it on; pausing discards buffered text."],
    ["No screen image is written", "Frames exist in memory only."],
    ["Redaction runs before storage", "And again on the model's own output."],
  ];
  let nx = M;
  notes.forEach((n, i) => {
    H.dot(s, { x: nx, y: 6.0, d: 0.11, color: HEX.sage, name: `fact-dot-${i}` });
    s.addText([
      { text: n[0] + "  ", options: { bold: true, color: HEX.ink } },
      { text: n[1], options: { color: HEX.stone } },
    ], { x: nx + 0.2, y: 5.88, w: 3.68, h: 0.6, fontFace: F.body, fontSize: 9.5,
         align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
         objectName: `fact-${i}` });
    nx += 3.93;
  });

  s.addNotes(
    "Walk the top row left to right — this is the capture pipeline, and every stage can only reduce " +
    "what is kept.\n\n" +
    "Step 2 fails closed: a browser window whose private-browsing state cannot be read is treated as " +
    "private and skipped. Step 3 is a ladder, not a choice: accessibility first because it is " +
    "structured and cheap, OCR when that times out, the vision model only when OCR quality is poor.\n\n" +
    "Step 4 is the one to dwell on. Redaction validates checksums — Luhn for payment cards, Verhoeff " +
    "for Aadhaar — rather than matching digit patterns. That is what keeps the false-positive rate " +
    "under half a per cent while removing every seeded secret.\n\n" +
    "The bottom row is what the user actually sees. Note that extraction is background work and gets " +
    "preempted by anything interactive — slide 8 explains how.");
}

/* ============================================================== 5. ARCHITECTURE */
pres.addSection({ title: "Architecture" });
{
  const s = content("Architecture", "Architecture at a glance", "Three processes, one of which is allowed to fail");

  // --- Interface process
  H.boundary(s, { x: M, y: 1.66, w: 3.25, h: 1.62, label: "INTERFACE PROCESS  ·  WEBVIEW2", name: "bnd-ui" });
  H.box(s, { x: M + 0.18, y: 2.04, w: 2.89, h: 1.06, label: "Notch UI",
             sub: "pill · peek · expanded\nToday · Loops · Search · Timeline · Ask · Diagnostics",
             size: 11, subSize: 8, name: "ui-box" });

  // --- Host process
  H.boundary(s, { x: 4.52, y: 1.66, w: 5.35, h: 4.22, label: "HOST PROCESS  ·  tessara.exe", name: "bnd-host" });

  const bands = [
    ["Acquisition", "tsr-capture · tsr-win", HEX.white],
    ["Policy", "tsr-guard — blocklist, redaction, claims", HEX.white],
    ["Memory", "tsr-store (SQLCipher) · tsr-retrieval", HEX.white],
    ["Reasoning", "tsr-pipeline · tsr-draft", HEX.white],
    ["Orchestration", "tsr-infer (scheduler, supervisor) · tsr-net", HEX.white],
  ];
  let by = 2.06;
  bands.forEach((b, i) => {
    H.box(s, { x: 4.72, y: by, w: 4.95, h: 0.63, fill: b[2],
               label: b[0], sub: b[1], size: 10.5, subSize: 8.5, name: `band-${i}` });
    by += 0.71;
  });
  s.addText("tsr-core underlies all five: Sensitive<T> and content-free logging", {
    x: 4.72, y: 5.6, w: 4.95, h: 0.22, fontFace: F.body, fontSize: 8.2, italic: true,
    color: HEX.stone, align: "center", valign: "middle", margin: 0, isTextBox: true,
    objectName: "core-note" });

  // --- Worker process
  H.boundary(s, { x: 10.1, y: 1.66, w: 2.45, h: 1.44, label: "WORKER  ·  JOB OBJECT", name: "bnd-worker" });
  H.box(s, { x: 10.26, y: 2.04, w: 2.13, h: 0.86, label: "llama-server.exe",
             sub: "Qwen3-VL-4B Q4_K_M", size: 10.5, subSize: 8.5, name: "worker-box" });
  H.arrowDown(s, { x: 11.32, y: 2.92, h: 0.34, name: "worker-hw" });
  H.box(s, { x: 10.26, y: 3.3, w: 2.13, h: 0.82, fill: HEX.mist, label: "Hardware backend",
             sub: "CUDA · ROCm · Vulkan\nSYCL · Metal · OpenCL · CPU", size: 10, subSize: 7.8,
             name: "backend-box" });

  // --- connectors
  H.arrow(s, { x: 4.02, y: 2.57, w: 0.46, name: "ui-host" });
  s.addText("typed IPC", { x: 3.82, y: 2.22, w: 0.9, h: 0.26, fontFace: F.body, fontSize: 8,
    color: HEX.stone, align: "center", valign: "middle", margin: 0, isTextBox: true, objectName: "lbl-ipc" });

  H.arrow(s, { x: 9.72, y: 2.47, w: 0.34, name: "host-worker" });

  // --- the point of the separation
  H.card(s, { x: 10.1, y: 4.46, w: 2.45, h: 1.42, fill: HEX.mist, name: "why-card" });
  s.addText([
    { text: "Why separate?", options: { bold: true, color: HEX.ink, breakLine: true } },
    { text: "The worker is created suspended, stripped of its environment and tied to a job that kills it when the host exits. A driver reset or out-of-memory crash takes down inference — never the application or the database.",
      options: { color: HEX.stone } },
  ], { x: 10.28, y: 4.6, w: 2.1, h: 1.16, fontFace: F.body, fontSize: 8.4,
       align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
       objectName: "why-text" });

  H.caption(s, { x: M, y: 6.06, w: 9.2, name: "arch-cap",
    text: "Dashed outlines are operating-system process boundaries. The host reaches the worker only over 127.0.0.1, and only after confirming that the process owning the listening socket is the child it created. It trusts neither of the other two: everything from the interface is validated, everything from the worker is an unverified proposal." });

  s.addNotes(
    "Read the diagram outside-in: the user touches only the left box; the host does all the work; " +
    "the worker is deliberately expendable.\n\n" +
    "The asymmetry of trust is the design. The host validates every command from the interface " +
    "(lengths, enumerations, identifiers, single-use approval tokens) and treats every byte from the " +
    "worker as a proposal to be checked.\n\n" +
    "Three details worth naming if asked. The worker is created with CREATE_SUSPENDED so it cannot " +
    "execute an instruction before its environment is cleared and the job object is attached. The " +
    "environment is stripped because llama-server reads LLAMA_ARG_* variables — an inherited one " +
    "could change its behaviour; this was tested. And before the host trusts the health endpoint it " +
    "queries the OS TCP table to confirm the listening socket belongs to the child it created, so " +
    "another local process cannot race to bind the port and impersonate the worker.");
}

/* ============================================================== 6. INSIDE */
{
  const s = content("Architecture", "Inside the architecture", "Boundaries drawn where the risk is, not where it is convenient");

  const mods = [
    ["tsr-capture", "5,101", "Three-rung acquisition ladder, the 285 ms deadline worker, SimHash change detection, session aggregation.", "Needs Windows"],
    ["tsr-guard", "1,651", "Blocklist, redaction engine, claim checker. Deliberately platform-neutral.", "Platform-neutral"],
    ["tsr-store", "3,081", "SQLCipher database, forward-only migrations, full-text indexes, repositories.", "Platform-neutral"],
    ["tsr-retrieval", "1,621", "Pinned ONNX runtime, chunking, int8 vectors, hybrid search, the Ask pipeline.", "Platform-neutral"],
    ["tsr-infer", "8,664", "Adapter discovery, GGUF parsing, backend selection, supervisor, loopback client, scheduler.", "Mixed"],
    ["tsr-pipeline", "7,254", "Extraction, schema, evidence verification, injection guard, resolvers, rollups.", "Platform-neutral"],
    ["tsr-draft", "587", "Draft generation and the independent fact checker.", "Platform-neutral"],
    ["tsr-net", "1,115", "The only network-capable crate. Approval-gated, digest-verified downloads.", "Isolated"],
    ["tsr-win", "1,771", "DPAPI, job objects, display affinity, window hooks, clipboard, TCP table.", "Needs Windows"],
    ["tsr-core", "475", "Sensitive<T>, content-free logging allowlist.", "Platform-neutral"],
  ];

  const cw = 2.84, ch = 1.1, gx = 0.24, gy = 0.18;
  mods.forEach((m, i) => {
    const col = i % 4, row = Math.floor(i / 4);
    const x = M + col * (cw + gx), y = 1.62 + row * (ch + gy);
    if (i >= 8) return; // first eight in the grid; last two placed in the wide row below
    H.card(s, { x, y, w: cw, h: ch, fill: HEX.white, line: HEX.mist, name: `mod-${i}` });
    s.addText([
      { text: m[0], options: { bold: true, fontSize: 11, color: HEX.ink, fontFace: F.mono } },
      { text: "   " + m[1] + " lines", options: { fontSize: 8, color: HEX.stoneLight, breakLine: true } },
      { text: m[2], options: { fontSize: 8.8, color: HEX.stone } },
    ], { x: x + 0.16, y: y + 0.12, w: cw - 0.32, h: ch - 0.24, fontFace: F.body,
         align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
         objectName: `mod-t-${i}` });
  });

  const wide = [mods[8], mods[9]];
  wide.forEach((m, i) => {
    const x = M + i * (5.94 + 0.24), y = 1.62 + 2 * (ch + gy);
    H.card(s, { x, y, w: 5.78, h: 0.62, fill: HEX.white, line: HEX.mist, name: `modw-${i}` });
    s.addText([
      { text: m[0], options: { bold: true, fontSize: 11, color: HEX.ink, fontFace: F.mono } },
      { text: "   " + m[1] + " lines   ", options: { fontSize: 8, color: HEX.stoneLight } },
      { text: m[2], options: { fontSize: 8.8, color: HEX.stone } },
    ], { x: x + 0.16, y, w: 5.46, h: 0.62, fontFace: F.body,
         align: "left", valign: "middle", margin: 0, lineSpacingMultiple: 1.1, isTextBox: true,
         objectName: `modw-t-${i}` });
  });

  const why = [
    ["Security code compiles without Windows",
     "tsr-guard, tsr-store, tsr-pipeline and tsr-retrieval have no Windows dependency, so the 355-case secrets corpus and the 152-case injection corpus run in ordinary CI."],
    ["Network capability is a single crate",
     "A build-time checker walks the whole dependency graph and fails compilation if any other crate can reach a network-capable dependency. It is itself tested against broken fixtures."],
    ["One crate per failure domain",
     "Acquisition can time out, inference can crash, downloads can be refused — each is contained by the boundary it sits behind rather than by discipline."],
  ];
  let wx = M;
  why.forEach((w, i) => {
    H.dot(s, { x: wx, y: 5.3, d: 0.12, color: HEX.shu, name: `why-dot-${i}` });
    s.addText([
      { text: w[0], options: { bold: true, color: HEX.ink, breakLine: true } },
      { text: w[1], options: { color: HEX.stone } },
    ], { x: wx + 0.22, y: 5.18, w: 3.7, h: 1.6, fontFace: F.body, fontSize: 9.6,
         align: "left", valign: "top", margin: 0, lineSpacingMultiple: 1.14, isTextBox: true,
         objectName: `why-${i}` });
    wx += 3.95;
  });

  s.addNotes(
    "Don't read the ten boxes aloud. The point is the three statements at the bottom: the division " +
    "follows trust and platform boundaries rather than convenience.\n\n" +
    "If asked why tsr-infer is the largest crate: it carries four separable concerns that share one " +
    "lifecycle — hardware discovery, process supervision, transport and scheduling.\n\n" +
    "tsr-net is small on purpose. It is the entire outbound attack surface, so it is kept short " +
    "enough for a person to read in one sitting.\n\n" +
    "Line counts are measured from the repository, not estimated. Roughly 37,000 lines of Rust in " +
    "total, plus about 2,000 lines of TypeScript for the interface.");
}

module.exports = { pres, content, C, M, W, HEX, H, F };
