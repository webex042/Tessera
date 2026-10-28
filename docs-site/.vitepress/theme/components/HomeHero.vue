<script setup lang="ts">
import { withBase } from 'vitepress'
import { ref } from 'vue'

const copied = ref(false)
const command = 'git clone https://github.com/webex042/Tessera.git && cd Tessera && cargo run -p tessera'

const copyCommand = async () => {
  try {
    await navigator.clipboard.writeText(command)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch {}
}
</script>

<template>
  <header class="c-hero" aria-labelledby="c-hero-heading">
    <div class="c-hero__container">
      <!-- Technical System Metadata Strip -->
      <div class="c-hero__telemetry" aria-label="System Runtime Specifications">
        <span class="c-badge c-badge--sys">SYS.INFO</span>
        <span class="c-telemetry__item">
          <span class="c-telemetry__key">ARCH:</span>
          <span class="c-telemetry__val">Tauri 2 + Rust</span>
        </span>
        <span class="c-telemetry__sep" aria-hidden="true">/</span>
        <span class="c-telemetry__item">
          <span class="c-telemetry__key">OS:</span>
          <span class="c-telemetry__val">Windows 11 (x86_64)</span>
        </span>
        <span class="c-telemetry__sep" aria-hidden="true">/</span>
        <span class="c-telemetry__item">
          <span class="c-telemetry__key">NETWORK:</span>
          <span class="c-telemetry__val">0 Outbound Sockets</span>
        </span>
        <span class="c-telemetry__sep" aria-hidden="true">/</span>
        <span class="c-telemetry__item">
          <span class="c-telemetry__key">RELEASE:</span>
          <span class="c-telemetry__val c-telemetry__val--accent">v0.0.1 Prototype</span>
        </span>
      </div>

      <!-- Main Headline & Concise Technical Overview -->
      <div class="c-hero__main">
        <h1 id="c-hero-heading" class="c-hero__title">
          <span class="c-hero__brand">tessera_</span>
          <span class="c-hero__headline">Hardware-Aware Local AI & Screen-Aware Memory Architecture</span>
        </h1>
        <p class="c-hero__lead">
          A local-first systems platform and desktop assistant for Windows 11. Tessera indexes visible
          desktop context through a non-invasive capture ladder, scrubs secrets deterministically, and powers
          grounded question answering with isolated <code>llama.cpp</code> sidecars — keeping all private data strictly on your machine.
        </p>
      </div>

      <!-- High-Impact Action CTAs -->
      <nav class="c-hero__actions" aria-label="Primary Documentation Actions">
        <a class="c-btn c-btn--primary" :href="withBase('/getting-started/')">
          Quickstart Guide <span class="c-btn__arrow" aria-hidden="true">→</span>
        </a>
        <a class="c-btn c-btn--secondary" :href="withBase('/architecture/high-level')">
          System Architecture <span class="c-btn__arrow" aria-hidden="true">→</span>
        </a>
        <a class="c-btn c-btn--ghost" :href="withBase('/guide/status')">
          Empirical Status Matrix
        </a>
      </nav>

      <!-- Terminal Command Bar -->
      <div class="c-hero__terminal">
        <div class="c-terminal__header">
          <span class="c-terminal__dot" aria-hidden="true"></span>
          <span class="c-terminal__title">POWERSHELL LOCAL LAUNCH</span>
          <button
            class="c-terminal__copy"
            type="button"
            @click="copyCommand"
            :aria-label="copied ? 'Copied command to clipboard' : 'Copy command to clipboard'"
          >
            {{ copied ? 'COPIED ✓' : 'COPY COMMAND' }}
          </button>
        </div>
        <div class="c-terminal__code">
          <span class="c-terminal__prompt" aria-hidden="true">PS &gt;</span>
          <code>{{ command }}</code>
        </div>
      </div>
    </div>
  </header>
</template>

<style scoped>
.c-hero {
  background-color: var(--p-black);
  border-bottom: 1px solid var(--t-border);
  position: relative;
}

.c-hero__container {
  max-width: 1440px;
  margin: 0 auto;
  padding: 2.25rem clamp(16px, 4vw, 48px) 2.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

/* System Telemetry Metadata Strip */
.c-hero__telemetry {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.5rem 0.85rem;
  font-family: var(--t-font-mono);
  font-size: 0.785rem;
  color: var(--t-text-muted);
}

.c-badge--sys {
  background: var(--t-surface-2);
  color: var(--p-blue);
  border: 1px solid var(--t-border);
  padding: 0.1rem 0.45rem;
  border-radius: var(--t-radius);
  font-weight: 700;
  letter-spacing: 0.06em;
}

.c-telemetry__item {
  display: inline-flex;
  gap: 0.35rem;
}

.c-telemetry__key {
  color: var(--t-text-muted);
}

.c-telemetry__val {
  color: var(--t-text-secondary);
  font-weight: 600;
}

.c-telemetry__val--accent {
  color: var(--p-rose);
  font-weight: 700;
}

.c-telemetry__sep {
  color: var(--t-border);
}

/* Main Headline */
.c-hero__main {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  max-width: 82ch;
}

.c-hero__title {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.c-hero__brand {
  font-family: var(--t-font-mono);
  font-size: 1.65rem;
  font-weight: 800;
  color: var(--p-rose);
  letter-spacing: -0.02em;
}

.c-hero__headline {
  font-family: var(--t-font-mono);
  font-size: var(--t-size-hero);
  font-weight: 800;
  line-height: 1.2;
  letter-spacing: -0.025em;
  color: var(--p-blue);
}

.c-hero__lead {
  margin: 0;
  font-family: var(--t-font-sans);
  font-size: 1.05rem;
  line-height: 1.7;
  color: var(--t-text);
}

.c-hero__lead code {
  font-family: var(--t-font-mono);
  font-size: 0.875em;
  padding: 0.15em 0.4em;
  background: var(--t-surface-2);
  border: 1px solid var(--t-border-subtle);
  border-radius: 3px;
  color: var(--p-rose);
}

/* High-Impact Actions */
.c-hero__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.85rem;
  align-items: center;
}

.c-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1.4rem;
  font-family: var(--t-font-mono);
  font-size: 0.875rem;
  font-weight: 700;
  text-decoration: none;
  border-radius: var(--t-radius);
  transition: all 0.15s ease;
}

.c-btn--primary {
  background: var(--p-rose);
  color: var(--p-black);
  border: 1px solid var(--p-rose);
}

.c-btn--primary:hover {
  background: var(--p-black);
  color: var(--p-rose);
}

.c-btn--secondary {
  background: var(--t-surface);
  color: var(--p-blue);
  border: 1px solid var(--p-blue);
}

.c-btn--secondary:hover {
  background: var(--p-blue);
  color: var(--p-black);
}

.c-btn--ghost {
  background: transparent;
  color: var(--t-text-secondary);
  border: 1px solid var(--t-border);
}

.c-btn--ghost:hover {
  border-color: var(--p-rose);
  color: var(--p-rose);
}

.c-btn__arrow {
  transition: transform 0.15s ease;
}

.c-btn:hover .c-btn__arrow {
  transform: translateX(3px);
}

/* Terminal Box */
.c-hero__terminal {
  max-width: 820px;
  background: var(--t-surface);
  border: 1px solid var(--t-border);
  border-radius: var(--t-radius);
  overflow: hidden;
}

.c-terminal__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.45rem 0.85rem;
  background: var(--t-surface-2);
  border-bottom: 1px solid var(--t-border-subtle);
  font-family: var(--t-font-mono);
  font-size: 0.725rem;
  letter-spacing: 0.05em;
  color: var(--t-text-muted);
}

.c-terminal__dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--p-blue);
}

.c-terminal__copy {
  background: transparent;
  border: 1px solid var(--t-border-subtle);
  border-radius: 3px;
  padding: 0.15rem 0.55rem;
  font-family: var(--t-font-mono);
  font-size: 0.7rem;
  font-weight: 700;
  color: var(--p-blue);
  cursor: pointer;
  transition: all 0.15s ease;
}

.c-terminal__copy:hover {
  border-color: var(--p-rose);
  color: var(--p-rose);
}

.c-terminal__code {
  display: flex;
  align-items: baseline;
  gap: 0.65rem;
  padding: 0.85rem 1rem;
  font-family: var(--t-font-mono);
  font-size: 0.875rem;
  color: var(--t-text);
  overflow-x: auto;
}

.c-terminal__prompt {
  color: var(--p-blue);
  font-weight: 700;
  user-select: none;
}

.c-terminal__code code {
  color: var(--t-text-strong);
  white-space: nowrap;
}

@media (max-width: 768px) {
  .c-hero__actions {
    flex-direction: column;
    align-items: stretch;
  }

  .c-btn {
    justify-content: center;
  }
}
</style>
