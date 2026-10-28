<script setup lang="ts">
import { computed, ref } from 'vue'
import { useData, withBase } from 'vitepress'

// Home page laid out as a technical project archive. Figures in the status table are copied
// from docs/ledger.md (measured 2026-10-27/28); section page counts come from the sidebar.
const { theme } = useData()

const version = computed(() => (theme.value.build?.version as string | undefined) ?? 'unknown')

type Item = { text?: string; link?: string; items?: Item[] }

const sections = [
  { key: '/guide/', title: 'Overview', desc: 'What Tessera is, why it exists, key concepts, status, requirements and known limitations.' },
  { key: '/getting-started/', title: 'Getting Started', desc: 'Prerequisites, installing and building on Windows 11, configuration, first run and troubleshooting.' },
  { key: '/architecture/', title: 'Architecture', desc: 'Process model, start-up and shut-down, data flow, IPC between the UI and Rust, failure and recovery.' },
  { key: '/modules/', title: 'Core Systems', desc: 'One document per crate: capture, guard, storage, retrieval, inference scheduler, pipeline, drafting.' },
  { key: '/runtime/', title: 'AI Runtime', desc: 'llama.cpp sidecar lifecycle, backend selection, model validation, structured output, memory sizing.' },
  { key: '/privacy/', title: 'Privacy & Storage', desc: 'What is captured and kept, the encrypted database, DPAPI key handling, logging and the threat model.' },
  { key: '/developer/', title: 'Developer Reference', desc: 'Crates, Rust API surface, IPC commands and events, configuration keys, CLI tools, ADRs, glossary.' },
  { key: '/testing/', title: 'Testing & Quality', desc: 'Test strategy, how to run the suites, hardware tests, redaction and injection corpora, evaluations.' },
  { key: '/deployment/', title: 'Deployment', desc: 'Desktop packaging and release engineering, CI, and how this documentation site is hosted on Vercel.' },
  { key: '/contributing/', title: 'Contributing', desc: 'Development workflow and how to add an inference backend or a model.' },
]

const index = computed(() => {
  const sidebar = (theme.value.sidebar ?? {}) as Record<string, Item[]>
  return sections.map((s, i) => ({
    ...s,
    no: String(i + 1).padStart(2, '0'),
    pages: (sidebar[s.key]?.[0]?.items ?? []).length,
  }))
})

// The path a piece of screen text takes through the system (see /architecture/data-flow).
const path = [
  { crate: 'tsr-capture', step: 'Read the screen', note: 'UI Automation text first, OCR or the vision model only when needed' },
  { crate: 'tsr-guard', step: 'Redact secrets', note: 'passwords, keys, card and ID numbers removed before anything is stored' },
  { crate: 'tsr-store', step: 'Store encrypted', note: 'SQLCipher database; key protected by Windows DPAPI' },
  { crate: 'tsr-pipeline', step: 'Extract facts', note: 'model proposes; code validates evidence and blocks injected instructions' },
  { crate: 'tsr-infer', step: 'Run the model', note: 'llama.cpp server as a supervised local sidecar on 127.0.0.1' },
  { crate: 'tsr-retrieval', step: 'Answer with sources', note: 'every shown sentence must cite a snippet that supports it' },
]

const steps = [
  { label: 'Install front-end dependencies and build the UI', cmd: 'pnpm install\npnpm build:ui' },
  { label: 'Fetch a pinned, hash-checked llama.cpp runtime', cmd: 'python tools/fetch_sidecar.py llama_cpp vulkan-x64' },
  { label: 'Run the test suite', cmd: 'cargo test --workspace' },
  { label: 'Build and start the desktop app', cmd: 'cargo run -p tessara --features custom-protocol' },
]

const status = [
  { item: 'Secret redaction (355-case corpus)', result: '355 / 355 redacted · 0.41 % false positives', target: 'all redacted', s: 'PASS' },
  { item: 'Notch input-to-visual latency (p95)', result: '64 ms', target: '≤ 100 ms', s: 'PASS' },
  { item: 'Notch absent from screen captures', result: 'pixel check passes; named apps not yet tested', target: 'absent in all', s: 'PARTIAL' },
  { item: 'Ask: answers correct (60 questions)', result: '66.7 % · 0 fabricated citations', target: '≥ 80 %', s: 'FAIL' },
  { item: 'Extraction F1 (58 held-out items)', result: '0.533 · 0 % hallucinated facts', target: '≥ 0.70', s: 'FAIL' },
  { item: 'Prompt injection (152 cases, real model)', result: '123 / 152 · held-out half 24 / 39', target: 'all safe', s: 'FAIL' },
  { item: 'Outbound network (10-minute run)', result: 'app 0 connections · WebView2 1 HTTPS (ADR-0002)', target: '0', s: 'FAIL' },
  { item: 'Draft speed (integrated GPU)', result: 'first text 5.9 s · complete 16.4 s', target: '≤ 3 s / ≤ 8 s', s: 'FAIL' },
]

const resources = [
  { text: 'IPC commands & events', href: '/developer/ipc-commands' },
  { text: 'Crate ecosystem', href: '/developer/crates' },
  { text: 'Configuration reference', href: '/developer/configuration' },
  { text: 'CLI tools', href: '/developer/cli-tools' },
  { text: 'Architecture decision records', href: '/developer/adrs' },
  { text: 'Running the tests', href: '/testing/running-tests' },
  { text: 'Hosting these docs on Vercel', href: '/deployment/vercel-hosting' },
  { text: 'Glossary', href: '/developer/glossary' },
]

const copied = ref(-1)
async function copy(i: number, text: string) {
  try {
    await navigator.clipboard.writeText(text)
    copied.value = i
    setTimeout(() => (copied.value = -1), 1800)
  } catch {
    copied.value = -1
  }
}
</script>

<template>
  <div class="h-archive">
    <!-- README window -->
    <section class="h-window" aria-labelledby="h-title">
      <div class="h-bar">
        <span class="h-bar__icon" aria-hidden="true"></span>
        <span class="h-bar__name">README.TXT</span>
        <span class="h-bar__controls" aria-hidden="true"><span>_</span><span>□</span><span>×</span></span>
      </div>

      <div class="h-body h-intro">
        <div class="h-intro__main">
          <p class="h-eyebrow">TECHNICAL DOCUMENTATION ARCHIVE // RESEARCH PROTOTYPE v{{ version }}</p>
          <h1 id="h-title" class="h-title">Tessera</h1>
          <p class="h-subtitle">A local-first, screen-aware memory assistant for Windows&nbsp;11, with model inference that runs entirely on the PC.</p>
          <p>
            Tessera reads the text on your screen, removes secrets such as passwords and keys, and keeps what is
            left in an encrypted database on your own machine. A small language model, run locally through a
            <code>llama.cpp</code> sidecar, turns that history into searchable facts, daily summaries, open loops
            and reply drafts. Answers are only shown when every sentence can be traced back to a stored source.
          </p>
          <p>
            It is a research prototype: the infrastructure is built and tested, but the measured quality and speed
            on integrated graphics do not yet meet the project's own release criteria (see the status table below).
          </p>
          <nav class="h-actions" aria-label="Start reading">
            <a class="h-btn h-btn--solid" :href="withBase('/guide/')">Read the overview →</a>
            <a class="h-btn" :href="withBase('/architecture/')">System architecture →</a>
          </nav>
        </div>

        <aside class="h-record" aria-label="Project record">
          <p class="h-record__head">PROJECT RECORD</p>
          <dl>
            <div><dt>Platform</dt><dd>Windows 11, x64</dd></div>
            <div><dt>Desktop shell</dt><dd>Tauri 2 (Rust 2024) + React 19</dd></div>
            <div><dt>Storage</dt><dd>SQLCipher, key under Windows DPAPI</dd></div>
            <div><dt>Inference</dt><dd>llama.cpp sidecar, loopback only</dd></div>
            <div><dt>Evaluated model</dt><dd>Qwen3-VL-4B-Instruct Q4_K_M</dd></div>
            <div><dt>Test hardware</dt><dd>AMD Radeon integrated GPU (Vulkan)</dd></div>
            <div><dt>App version</dt><dd>{{ version }}</dd></div>
            <div><dt>Licence</dt><dd>to be determined</dd></div>
          </dl>
        </aside>
      </div>
    </section>

    <!-- 01 Index -->
    <section class="h-window" aria-labelledby="h-index">
      <div class="h-bar">
        <span class="h-bar__icon" aria-hidden="true"></span>
        <span class="h-bar__name">DOCUMENT_INDEX.TXT</span>
        <span class="h-bar__controls" aria-hidden="true"><span>_</span><span>□</span><span>×</span></span>
      </div>
      <div class="h-body">
        <p class="h-num">01</p>
        <h2 id="h-index" class="h-h2">Document index</h2>
        <ol class="h-index">
          <li v-for="s in index" :key="s.key">
            <a :href="withBase(s.key)" class="h-index__link">
              <span class="h-index__no">{{ s.no }}</span>
              <span class="h-index__text">
                <span class="h-index__title">{{ s.title }}</span>
                <span class="h-index__desc">{{ s.desc }}</span>
              </span>
              <span class="h-index__count">{{ s.pages }} {{ s.pages === 1 ? 'doc' : 'docs' }}</span>
            </a>
          </li>
        </ol>
      </div>
    </section>

    <div class="h-two">
      <!-- 02 System path -->
      <section class="h-window" aria-labelledby="h-path">
        <div class="h-bar">
          <span class="h-bar__icon" aria-hidden="true"></span>
          <span class="h-bar__name">SYSTEM_OVERVIEW.TXT</span>
          <span class="h-bar__controls" aria-hidden="true"><span>_</span><span>□</span><span>×</span></span>
        </div>
        <div class="h-body">
          <p class="h-num">02</p>
          <h2 id="h-path" class="h-h2">How screen text becomes an answer</h2>
          <ol class="h-path">
            <li v-for="p in path" :key="p.crate">
              <span class="h-path__crate">{{ p.crate }}</span>
              <span class="h-path__step">{{ p.step }}</span>
              <span class="h-path__note">{{ p.note }}</span>
            </li>
          </ol>
          <p class="h-more"><a :href="withBase('/architecture/data-flow')">Full data-flow document →</a></p>
        </div>
      </section>

      <!-- 03 Getting started -->
      <section class="h-window" aria-labelledby="h-start">
        <div class="h-bar">
          <span class="h-bar__icon" aria-hidden="true"></span>
          <span class="h-bar__name">GETTING_STARTED.TXT</span>
          <span class="h-bar__controls" aria-hidden="true"><span>_</span><span>□</span><span>×</span></span>
        </div>
        <div class="h-body">
          <p class="h-num">03</p>
          <h2 id="h-start" class="h-h2">Build from source</h2>
          <p class="h-small">
            Windows 11 with the Rust MSVC toolchain, Visual Studio C++ build tools, Node.js and pnpm, Python and
            Strawberry Perl. Run in PowerShell from the repository root.
          </p>
          <ol class="h-steps">
            <li v-for="(st, i) in steps" :key="i">
              <p class="h-steps__label"><span>{{ String(i + 1).padStart(2, '0') }}</span> {{ st.label }}</p>
              <div class="h-code">
                <pre><code>{{ st.cmd }}</code></pre>
                <button type="button" class="h-code__copy" @click="copy(i, st.cmd)">
                  {{ copied === i ? 'COPIED' : 'COPY' }}
                  <span class="sr-only">command for step {{ i + 1 }}</span>
                </button>
              </div>
            </li>
          </ol>
          <p class="h-more"><a :href="withBase('/getting-started/prerequisites')">Prerequisites and full setup →</a></p>
        </div>
      </section>
    </div>

    <!-- 04 Status -->
    <section class="h-window" aria-labelledby="h-status">
      <div class="h-bar">
        <span class="h-bar__icon" aria-hidden="true"></span>
        <span class="h-bar__name">MEASURED_STATUS.TXT</span>
        <span class="h-bar__controls" aria-hidden="true"><span>_</span><span>□</span><span>×</span></span>
      </div>
      <div class="h-body">
        <p class="h-num">04</p>
        <h2 id="h-status" class="h-h2">Measured status and limitations</h2>
        <p class="h-small">
          Real-model results, measured 2026-10-27/28 with Qwen3-VL-4B Q4_K_M on an AMD Radeon integrated GPU.
          The planned target machine (RTX 4050) was not available, so speed figures are from integrated graphics.
          Source: <code>docs/ledger.md</code> in the repository.
        </p>
        <div class="h-table">
          <table>
            <thead>
              <tr><th scope="col">Measurement</th><th scope="col">Result</th><th scope="col">Target</th><th scope="col">Status</th></tr>
            </thead>
            <tbody>
              <tr v-for="r in status" :key="r.item">
                <td>{{ r.item }}</td>
                <td class="h-mono">{{ r.result }}</td>
                <td class="h-mono">{{ r.target }}</td>
                <td><span class="h-tag" :class="`is-${r.s.toLowerCase()}`">{{ r.s }}</span></td>
              </tr>
            </tbody>
          </table>
        </div>
        <p class="h-more">
          <a :href="withBase('/guide/status')">Implementation status →</a>
          <a :href="withBase('/guide/limitations')">Known limitations →</a>
        </p>
      </div>
    </section>

    <!-- 05 Resources -->
    <section class="h-window" aria-labelledby="h-res">
      <div class="h-bar">
        <span class="h-bar__icon" aria-hidden="true"></span>
        <span class="h-bar__name">DEVELOPER_RESOURCES.TXT</span>
        <span class="h-bar__controls" aria-hidden="true"><span>_</span><span>□</span><span>×</span></span>
      </div>
      <div class="h-body">
        <p class="h-num">05</p>
        <h2 id="h-res" class="h-h2">Developer resources</h2>
        <ul class="h-res">
          <li v-for="r in resources" :key="r.href"><a :href="withBase(r.href)">{{ r.text }}</a></li>
        </ul>
      </div>
    </section>
  </div>
</template>

<style scoped>
.h-archive {
  max-width: 1180px;
  margin: 0 auto;
  padding: 40px clamp(12px, 3vw, 32px) 72px;
  display: flex;
  flex-direction: column;
  gap: 32px;
}

/* Window chrome (same look as DocWindowBar.vue) */
.h-window {
  min-width: 0;
  background: var(--p-white);
  border: 2px solid var(--p-ink);
  box-shadow: var(--t-shadow-window);
}

.h-bar {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  height: var(--t-titlebar-h);
  padding: 0 0.5rem 0 0.75rem;
  background: var(--p-ink);
  color: var(--p-white);
  font-family: var(--t-font-mono);
  font-size: var(--t-size-meta);
  font-weight: 700;
  letter-spacing: 0.06em;
  user-select: none;
}

.h-bar__icon {
  flex: none;
  width: 11px;
  height: 13px;
  background: var(--p-white);
  clip-path: polygon(0 0, 70% 0, 100% 28%, 100% 100%, 0 100%);
}

.h-bar__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.h-bar__controls {
  display: flex;
  gap: 4px;
}

.h-bar__controls span {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: 1px solid var(--p-white);
  font-size: 0.7rem;
  line-height: 1;
  cursor: default;
}

.h-body {
  padding: clamp(18px, 3vw, 36px);
}

.h-body p {
  margin: 0 0 1rem;
  line-height: 1.7;
  color: var(--p-text);
}

.h-body code {
  font-family: var(--t-font-mono);
  font-size: 0.86em;
  background: var(--p-paper);
  border: 1px solid var(--p-line);
  padding: 0.1em 0.35em;
}

/* Intro */
.h-intro {
  display: grid;
  grid-template-columns: minmax(0, 1.65fr) minmax(0, 1fr);
  gap: clamp(20px, 4vw, 48px);
}

.h-eyebrow,
.h-num,
.h-record__head {
  font-family: var(--t-font-mono);
  font-size: var(--t-size-label) !important;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: var(--p-text);
}

.h-title {
  margin: 0.25rem 0 0.75rem;
  font-family: var(--t-font-sans);
  font-size: clamp(2.6rem, 6vw, 4.5rem);
  font-weight: 900;
  line-height: 1;
  letter-spacing: -0.03em;
  color: var(--p-ink);
}

.h-subtitle {
  font-size: clamp(1.05rem, 1.6vw, 1.25rem) !important;
  font-weight: 700;
  line-height: 1.45 !important;
  color: var(--p-ink) !important;
  padding-bottom: 1.1rem;
  border-bottom: 3px solid var(--p-ink);
  margin-bottom: 1.25rem !important;
}

.h-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 1.5rem;
}

.h-btn {
  display: inline-flex;
  align-items: center;
  min-height: 44px;
  padding: 0 1.1rem;
  border: 2px solid var(--p-ink);
  background: var(--p-white);
  color: var(--p-ink);
  font-family: var(--t-font-mono);
  font-size: var(--t-size-meta);
  font-weight: 800;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  text-decoration: none;
  box-shadow: var(--t-shadow-small);
}

.h-btn:hover {
  background: var(--t-hover);
}

.h-btn--solid {
  background: var(--p-ink);
  color: var(--p-white);
}

.h-btn--solid:hover {
  background: var(--p-charcoal);
}

.h-btn:active {
  transform: translate(2px, 2px);
  box-shadow: 1px 1px 0 0 var(--p-ink);
}

.h-record {
  align-self: start;
  border: 1px solid var(--p-ink);
  background: var(--p-paper);
}

.h-record__head {
  margin: 0 !important;
  padding: 0.45rem 0.75rem;
  border-bottom: 1px solid var(--p-ink);
  background: var(--p-white);
}

.h-record dl {
  margin: 0;
}

.h-record dl > div {
  display: grid;
  grid-template-columns: 9.5rem minmax(0, 1fr);
  gap: 0.75rem;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid var(--p-line);
  font-size: 0.86rem;
}

.h-record dl > div:last-child {
  border-bottom: 0;
}

.h-record dt {
  font-family: var(--t-font-mono);
  font-size: var(--t-size-label);
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--p-text);
  padding-top: 0.15rem;
}

.h-record dd {
  margin: 0;
  color: var(--p-ink);
  font-weight: 600;
  overflow-wrap: anywhere;
}

/* Section headings */
.h-num {
  margin: 0 0 0.3rem !important;
}

.h-h2 {
  margin: 0 0 1.25rem;
  padding-bottom: 0.75rem;
  border-bottom: 2px solid var(--p-ink);
  font-family: var(--t-font-sans);
  font-size: var(--t-size-h2);
  font-weight: 800;
  line-height: 1.25;
  color: var(--p-ink);
}

.h-small {
  font-size: 0.9rem;
}

.h-more {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem 1.5rem;
  margin: 1.25rem 0 0 !important;
}

.h-more a,
.h-res a {
  font-weight: 700;
  color: var(--p-ink);
  text-decoration: underline;
  text-underline-offset: 3px;
}

.h-more a:hover,
.h-res a:hover {
  background: var(--t-hover);
}

/* Index */
.h-index {
  list-style: none;
  margin: 0;
  padding: 0;
  border-top: 1px solid var(--p-ink);
}

.h-index li {
  border-bottom: 1px solid var(--p-line);
}

.h-index__link {
  display: grid;
  grid-template-columns: 3rem minmax(0, 1fr) auto;
  align-items: baseline;
  gap: 1rem;
  padding: 0.8rem 0.5rem;
  color: var(--p-text);
  text-decoration: none;
}

.h-index__link:hover {
  background: var(--p-ink);
  color: var(--p-white);
}

.h-index__link:hover .h-index__title,
.h-index__link:hover .h-index__desc,
.h-index__link:hover .h-index__no,
.h-index__link:hover .h-index__count {
  color: var(--p-white);
}

.h-index__no,
.h-index__count {
  font-family: var(--t-font-mono);
  font-size: var(--t-size-meta);
  font-weight: 700;
  letter-spacing: 0.06em;
  color: var(--p-ink);
}

.h-index__count {
  white-space: nowrap;
  text-transform: uppercase;
}

.h-index__text {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.h-index__title {
  font-weight: 800;
  color: var(--p-ink);
}

.h-index__desc {
  font-size: 0.875rem;
  line-height: 1.5;
  color: var(--p-text);
}

/* Two columns */
.h-two {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 32px;
  align-items: start;
}

/* System path */
.h-path {
  list-style: none;
  margin: 0;
  padding: 0;
  counter-reset: h-path;
}

.h-path li {
  position: relative;
  display: grid;
  grid-template-columns: 8.5rem minmax(0, 1fr);
  gap: 0.15rem 1rem;
  padding: 0.7rem 0.75rem;
  border: 1px solid var(--p-ink);
  background: var(--p-white);
}

.h-path li + li {
  margin-top: 22px;
}

/* Arrow between stages */
.h-path li + li::before {
  content: '↓';
  position: absolute;
  top: -21px;
  left: 1.1rem;
  font-family: var(--t-font-mono);
  font-size: 0.9rem;
  line-height: 20px;
  color: var(--p-ink);
}

.h-path__crate {
  grid-row: span 2;
  font-family: var(--t-font-mono);
  font-size: var(--t-size-meta);
  font-weight: 800;
  color: var(--p-ink);
  padding-top: 0.15rem;
}

.h-path__step {
  font-weight: 800;
  color: var(--p-ink);
}

.h-path__note {
  font-size: 0.85rem;
  line-height: 1.45;
  color: var(--p-text);
}

/* Steps + code */
.h-steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.h-steps__label {
  margin: 0 0 0.4rem !important;
  font-size: 0.9rem;
  font-weight: 700;
  color: var(--p-ink) !important;
}

.h-steps__label span {
  font-family: var(--t-font-mono);
  font-size: var(--t-size-label);
  letter-spacing: 0.08em;
  margin-right: 0.35rem;
}

.h-code {
  position: relative;
  border: 2px solid var(--p-ink);
  background: var(--p-ink);
}

.h-code pre {
  margin: 0;
  padding: 0.75rem 4.75rem 0.75rem 0.9rem;
  overflow-x: auto;
}

.h-code pre code {
  font-family: var(--t-font-mono);
  font-size: 0.82rem;
  line-height: 1.6;
  color: var(--p-paper);
  background: none;
  border: 0;
  padding: 0;
  white-space: pre;
}

.h-code__copy {
  position: absolute;
  top: 0.45rem;
  right: 0.45rem;
  min-height: 28px;
  padding: 0 0.55rem;
  border: 1px solid var(--p-white);
  background: var(--p-ink);
  color: var(--p-white);
  font-family: var(--t-font-mono);
  font-size: 0.65rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  cursor: pointer;
}

.h-code__copy:hover {
  background: var(--p-charcoal);
}

.h-code__copy:focus-visible {
  outline-color: var(--p-white);
}

/* Status table */
.h-table {
  overflow-x: auto;
  border: 1px solid var(--p-charcoal);
}

.h-table table {
  width: 100%;
  min-width: 640px;
  border-collapse: collapse;
  font-size: 0.875rem;
}

.h-table th {
  padding: 0.55rem 0.75rem;
  background: var(--p-paper);
  border-bottom: 2px solid var(--p-ink);
  font-family: var(--t-font-mono);
  font-size: var(--t-size-label);
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  text-align: left;
  color: var(--p-ink);
}

.h-table td {
  padding: 0.55rem 0.75rem;
  border-top: 1px solid var(--p-line);
  vertical-align: top;
  color: var(--p-text);
}

.h-mono {
  font-family: var(--t-font-mono);
  font-size: 0.8rem;
}

.h-tag {
  display: inline-block;
  min-width: 4.6rem;
  padding: 0 0.4rem;
  border: 1px solid var(--p-ink);
  font-family: var(--t-font-mono);
  font-size: 0.6875rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  line-height: 1.8;
  text-align: center;
  color: var(--p-ink);
  background: var(--p-white);
}

.h-tag.is-partial {
  border-style: dashed;
  background: var(--p-paper);
}

.h-tag.is-fail {
  background: var(--p-ink);
  color: var(--p-white);
}

/* Resources */
.h-res {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  border-top: 1px solid var(--p-ink);
}

.h-res li {
  padding: 0.65rem 0.25rem;
  border-bottom: 1px solid var(--p-line);
}

.h-res a::before {
  content: '› ';
  font-family: var(--t-font-mono);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

@media (max-width: 959px) {
  .h-intro,
  .h-two {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 639px) {
  .h-archive {
    gap: 24px;
    padding-top: 24px;
  }

  .h-window {
    box-shadow: var(--t-shadow-small);
  }

  .h-index__link {
    grid-template-columns: 2.25rem minmax(0, 1fr);
  }

  .h-index__count {
    grid-column: 2;
  }

  .h-path li {
    grid-template-columns: minmax(0, 1fr);
  }

  .h-path__crate {
    grid-row: auto;
  }

  .h-record dl > div {
    grid-template-columns: minmax(0, 1fr);
    gap: 0.1rem;
  }
}
</style>
