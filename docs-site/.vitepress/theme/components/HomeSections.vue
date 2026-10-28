<script setup lang="ts">
import { withBase } from 'vitepress'

const tracks = [
  {
    title: 'Getting Started & Setup',
    badge: 'TRACK.01',
    desc: 'Hardware prerequisites, Windows 11 environment setup, fetching hash-locked sidecars, and local compilation.',
    links: [
      { text: 'Quickstart Guide', href: '/getting-started/' },
      { text: 'System Prerequisites', href: '/getting-started/prerequisites' },
      { text: 'Step-by-Step Installation', href: '/getting-started/installation' },
      { text: 'Troubleshooting & FAQ', href: '/getting-started/troubleshooting' }
    ]
  },
  {
    title: 'Systems Architecture & IPC',
    badge: 'TRACK.02',
    desc: 'Multi-tiered architecture, Win32 Job Object sandboxing, process isolation, and typed Tauri command routing.',
    links: [
      { text: 'High-Level Architecture', href: '/architecture/high-level' },
      { text: 'Component Breakdown', href: '/architecture/components' },
      { text: 'Process Isolation Model', href: '/architecture/process-model' },
      { text: 'IPC Commands & Events', href: '/developer/ipc-commands' }
    ]
  },
  {
    title: 'AI Runtime & Acceleration',
    badge: 'TRACK.03',
    desc: 'Dynamic Vulkan and AVX2 CPU arbitration, GGUF 4-bit quantization, and zero-dependency loopback clients.',
    links: [
      { text: 'Hardware Backends', href: '/runtime/hardware-backends' },
      { text: 'Backend Selection Logic', href: '/runtime/backend-selection' },
      { text: 'Model Validation & GGUF', href: '/runtime/model-validation' },
      { text: 'Structured Output Grammars', href: '/runtime/structured-outputs' }
    ]
  },
  {
    title: 'Security, Storage & Privacy',
    badge: 'TRACK.04',
    desc: 'Deterministic regex secret redaction, AES-256 DPAPI storage, content-free logging, and threat boundaries.',
    links: [
      { text: 'Privacy Architecture', href: '/privacy/privacy-architecture' },
      { text: 'Guardrails & Redaction', href: '/modules/guardrails' },
      { text: 'Encrypted Storage & SQLite', href: '/modules/storage' },
      { text: 'Security Threat Model', href: '/privacy/threat-model' }
    ]
  }
]

const invariants = [
  {
    code: 'INVARIANT.01',
    title: 'Decoupled Compute',
    desc: 'Zero host-binary linking to CUDA, ROCm, or proprietary GPU SDKs. Models execute exclusively within supervisor-managed sidecars.'
  },
  {
    code: 'INVARIANT.02',
    title: 'Process Sandboxing',
    desc: 'AI crashes and VRAM out-of-memory errors are trapped within Win32 Job Objects. The desktop application never terminates unexpectedly.'
  },
  {
    code: 'INVARIANT.03',
    title: 'Deterministic Redaction',
    desc: 'Untrusted screen text is scanned against 355 regex patterns before disk writes. Raw credentials and tokens never enter model context.'
  },
  {
    code: 'INVARIANT.04',
    title: 'Grounded Citation',
    desc: 'Small language models propose answers, but Rust strictly verifies every cited claim against exact source substrings before display.'
  }
]

const scorecard = [
  { metric: 'Secret Redaction (355-case corpus)', result: '355 / 355, 0.41% FP', status: 'pass' },
  { metric: 'Notch Latency (p95 input-to-visual)', result: '64 ms (target ≤ 100 ms)', status: 'pass' },
  { metric: 'Capture Exclusion (WGC + GDI)', result: 'Absent in pill & expanded', status: 'pass' },
  { metric: 'Fabricated Citations in Ask', result: '0 (100% citation precision)', status: 'pass' },
  { metric: 'Ask Accuracy (60 evaluation queries)', result: '66.7 % (target ≥ 80 %)', status: 'fail' },
  { metric: 'Extraction F1 (held-out corpus)', result: '0.507 (target ≥ 0.70)', status: 'fail' },
  { metric: 'Outbound Network (10-minute audit)', result: '0 code · 1 WebView2 telemetry', status: 'fail' }
]

const dispatches = [
  {
    title: 'Stage 7 Adversarial Injection Resistance Audit',
    summary: 'tsr-guard evaluated against 152-case injection corpus. Deterministic schema delimiting prevents untrusted screen text from hijacking inference.',
    date: '28.10.2026',
    link: '/testing/injection-tests'
  },
  {
    title: '355/355 Secret Redaction Benchmark Passed',
    summary: 'Deterministic regular expressions filter credentials, API keys, and personal identifiers with 0.41% false positive rate before persistent write.',
    date: '28.10.2026',
    link: '/testing/redaction-tests'
  },
  {
    title: 'Vulkan & AVX2 Sidecar Discovery Protocol',
    summary: 'Zero dynamic host linking to CUDA. Pinned llama.cpp sidecars dynamically probe display drivers and arbitrate between GPU and CPU.',
    date: '26.10.2026',
    link: '/runtime/hardware-backends'
  }
]
</script>

<template>
  <div class="c-portal">
    <div class="c-portal__inner">
      <!-- Section 1: Documentation Tracks -->
      <section class="c-section" aria-labelledby="tracks-heading">
        <header class="c-section__header">
          <p class="c-section__kicker">// ARCHITECTURE DOMAINS</p>
          <h2 id="tracks-heading" class="c-section__title">Documentation Tracks</h2>
          <p class="c-section__lead">
            Explore the four core engineering tracks structuring the Tessera platform and codebase.
          </p>
        </header>

        <div class="c-tracks-grid">
          <article v-for="t in tracks" :key="t.badge" class="c-track-card">
            <div class="c-track-card__header">
              <span class="c-track-card__badge">{{ t.badge }}</span>
              <h3 class="c-track-card__title">{{ t.title }}</h3>
            </div>
            <p class="c-track-card__desc">{{ t.desc }}</p>
            <ul class="c-track-card__links">
              <li v-for="l in t.links" :key="l.href">
                <a :href="withBase(l.href)">{{ l.text }} <span aria-hidden="true">»</span></a>
              </li>
            </ul>
          </article>
        </div>
      </section>

      <!-- Section 2: Architectural Invariants -->
      <section class="c-section" aria-labelledby="invariants-heading">
        <header class="c-section__header">
          <p class="c-section__kicker">// SYSTEM GUARANTEES</p>
          <h2 id="invariants-heading" class="c-section__title">Architectural Invariants</h2>
          <p class="c-section__lead">
            Non-negotiable architectural constraints enforced at compile time and verified in CI.
          </p>
        </header>

        <div class="c-invariants-grid">
          <div v-for="inv in invariants" :key="inv.code" class="c-invariant-box">
            <div class="c-invariant__top">
              <span class="c-invariant__code">{{ inv.code }}</span>
              <h3 class="c-invariant__title">{{ inv.title }}</h3>
            </div>
            <p class="c-invariant__desc">{{ inv.desc }}</p>
          </div>
        </div>
      </section>

      <!-- Section 3: Empirical Scorecard & Dispatches (2 Columns) -->
      <div class="c-two-col">
        <!-- Empirical Scorecard Column -->
        <section class="c-section" aria-labelledby="scorecard-heading">
          <header class="c-section__header">
            <p class="c-section__kicker">// EMPIRICAL LEDGER</p>
            <h2 id="scorecard-heading" class="c-section__title">Measured Scorecard</h2>
            <p class="c-section__lead">
              Exact benchmark metrics from <code>docs/ledger.md</code>. Stages 0–7 are built; MVP production gate is not met.
            </p>
          </header>

          <div class="c-table-wrap">
            <table class="c-scorecard-table">
              <thead>
                <tr>
                  <th scope="col">EVALUATION METRIC</th>
                  <th scope="col">RESULT</th>
                  <th scope="col">STATUS</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="row in scorecard" :key="row.metric">
                  <td class="c-scorecard__metric">{{ row.metric }}</td>
                  <td class="c-scorecard__result"><code>{{ row.result }}</code></td>
                  <td class="c-scorecard__status">
                    <span class="c-status-tag" :class="`is-${row.status}`">
                      {{ row.status === 'pass' ? '[PASS]' : '[FAIL]' }}
                    </span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <p class="c-table-note">
            Full audit details and root cause analysis:
            <a :href="withBase('/guide/status')">Implementation Status Matrix »</a>
          </p>
        </section>

        <!-- Technical Dispatches Column -->
        <section class="c-section" aria-labelledby="dispatches-heading">
          <header class="c-section__header">
            <p class="c-section__kicker">// AUDIT LOGS</p>
            <h2 id="dispatches-heading" class="c-section__title">Technical Dispatches</h2>
            <p class="c-section__lead">
              Recent engineering audits, corpora evaluations, and milestone reports.
            </p>
          </header>

          <div class="c-dispatches-list">
            <article v-for="d in dispatches" :key="d.title" class="c-dispatch-card">
              <div class="c-dispatch__meta">
                <span class="c-dispatch__date">[{{ d.date }}]</span>
                <span class="c-dispatch__tag">AUDIT</span>
              </div>
              <h3 class="c-dispatch__title">
                <a :href="withBase(d.link)">{{ d.title }}</a>
              </h3>
              <p class="c-dispatch__summary">{{ d.summary }}</p>
              <a class="c-dispatch__link" :href="withBase(d.link)">Read Audit Report »</a>
            </article>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.c-portal {
  background-color: var(--p-black);
  padding: 3rem 0 5rem;
}

.c-portal__inner {
  max-width: 1440px;
  margin: 0 auto;
  padding: 0 clamp(16px, 4vw, 48px);
  display: flex;
  flex-direction: column;
  gap: 3.5rem;
}

/* Section Headers */
.c-section {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.c-section__header {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  max-width: 72ch;
}

.c-section__kicker {
  margin: 0;
  font-family: var(--t-font-mono);
  font-size: 0.785rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--p-rose);
}

.c-section__title {
  margin: 0;
  font-family: var(--t-font-mono);
  font-size: var(--t-size-h2);
  font-weight: 800;
  color: var(--p-blue);
  letter-spacing: -0.02em;
}

.c-section__lead {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 0.95rem;
  line-height: 1.6;
  color: var(--t-text-secondary);
}

.c-section__lead code {
  font-family: var(--t-font-mono);
  color: var(--p-rose);
  background: var(--t-surface-2);
  padding: 0.1rem 0.35rem;
  border-radius: 3px;
  font-size: 0.85em;
}

/* Documentation Tracks Grid */
.c-tracks-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 1.25rem;
}

.c-track-card {
  background: var(--t-surface);
  border: 1px solid var(--t-border);
  border-radius: var(--t-radius);
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  transition: border-color 0.15s ease;
}

.c-track-card:hover {
  border-color: var(--p-blue);
}

.c-track-card__header {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.c-track-card__badge {
  font-family: var(--t-font-mono);
  font-size: 0.725rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  color: var(--p-rose);
}

.c-track-card__title {
  margin: 0;
  font-family: var(--t-font-mono);
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--p-blue);
}

.c-track-card__desc {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 0.875rem;
  line-height: 1.6;
  color: var(--t-text);
  flex-grow: 1;
}

.c-track-card__links {
  list-style: none;
  margin: 0;
  padding: 0.75rem 0 0;
  border-top: 1px solid var(--t-border-subtle);
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.c-track-card__links li a {
  font-family: var(--t-font-sans);
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--p-blue);
  text-decoration: underline;
  text-decoration-color: var(--t-border-subtle);
  text-underline-offset: 2px;
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  transition: all 0.15s ease;
}

.c-track-card__links li a:hover {
  color: var(--p-rose);
  text-decoration-color: var(--p-rose);
}

/* System Invariants Grid */
.c-invariants-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 1.25rem;
}

.c-invariant-box {
  background: var(--t-surface);
  border: 1px solid var(--t-border);
  border-radius: var(--t-radius);
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.c-invariant__top {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.c-invariant__code {
  font-family: var(--t-font-mono);
  font-size: 0.725rem;
  font-weight: 700;
  color: var(--p-blue);
  letter-spacing: 0.05em;
}

.c-invariant__title {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 0.95rem;
  font-weight: 700;
  color: var(--t-text-strong);
}

.c-invariant__desc {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 0.85rem;
  line-height: 1.55;
  color: var(--t-text);
}

/* Two Column Layout */
.c-two-col {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
  gap: 2rem;
  align-items: start;
}

/* Scorecard Table */
.c-table-wrap {
  border: 1px solid var(--t-border);
  border-radius: var(--t-radius);
  overflow-x: auto;
  background: var(--t-surface);
}

.c-scorecard-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.85rem;
  text-align: left;
}

.c-scorecard-table th {
  background: var(--t-surface-2);
  color: var(--p-blue);
  font-family: var(--t-font-mono);
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.05em;
  padding: 0.65rem 0.85rem;
  border-bottom: 1px solid var(--t-border);
}

.c-scorecard-table td {
  padding: 0.65rem 0.85rem;
  border-bottom: 1px solid var(--t-border-subtle);
  color: var(--t-text);
}

.c-scorecard-table tr:last-child td {
  border-bottom: none;
}

.c-scorecard__metric {
  font-family: var(--t-font-sans);
  font-size: 0.85rem;
}

.c-scorecard__result code {
  font-family: var(--t-font-mono);
  font-size: 0.8125rem;
  color: var(--t-text-strong);
}

.c-status-tag {
  font-family: var(--t-font-mono);
  font-size: 0.725rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  padding: 0.15rem 0.45rem;
  border-radius: 3px;
  white-space: nowrap;
}

.c-status-tag.is-pass {
  color: var(--p-blue);
  background: color-mix(in srgb, var(--p-blue) 12%, var(--p-black));
  border: 1px solid var(--p-blue);
}

.c-status-tag.is-fail {
  color: var(--p-black);
  background: var(--p-rose);
  border: 1px solid var(--p-rose);
}

.c-table-note {
  margin: 0.5rem 0 0;
  font-family: var(--t-font-sans);
  font-size: 0.8125rem;
  color: var(--t-text-muted);
}

.c-table-note a {
  color: var(--p-blue);
  font-weight: 600;
  text-decoration: underline;
}

.c-table-note a:hover {
  color: var(--p-rose);
}

/* Dispatches List */
.c-dispatches-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.c-dispatch-card {
  background: var(--t-surface);
  border: 1px solid var(--t-border);
  border-radius: var(--t-radius);
  padding: 1.15rem;
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.c-dispatch__meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-family: var(--t-font-mono);
  font-size: 0.725rem;
}

.c-dispatch__date {
  color: var(--p-blue);
}

.c-dispatch__tag {
  background: var(--t-surface-2);
  color: var(--p-rose);
  border: 1px solid var(--t-border-subtle);
  padding: 0.05rem 0.35rem;
  border-radius: 2px;
  font-weight: 700;
}

.c-dispatch__title {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 0.95rem;
  font-weight: 700;
  line-height: 1.35;
}

.c-dispatch__title a {
  color: var(--t-text-strong);
  text-decoration: none;
}

.c-dispatch__title a:hover {
  color: var(--p-rose);
  text-decoration: underline;
}

.c-dispatch__summary {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 0.825rem;
  line-height: 1.5;
  color: var(--t-text);
}

.c-dispatch__link {
  font-family: var(--t-font-mono);
  font-size: 0.785rem;
  font-weight: 600;
  color: var(--p-blue);
  text-decoration: underline;
  margin-top: 0.25rem;
}

.c-dispatch__link:hover {
  color: var(--p-rose);
}

@media (max-width: 960px) {
  .c-two-col {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
