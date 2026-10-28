<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vitepress'

// Slim black strip above the header. Every value is real build metadata (config.ts reads it
// from the repository at build time); nothing here is a live metric or a measured result.
const { theme } = useData()

const build = computed(
  () => (theme.value.build ?? { version: 'unknown', commit: 'unknown', date: 'unknown' }) as {
    version: string
    commit: string
    date: string
  },
)
</script>

<template>
  <div class="t-ticker" role="note" aria-label="Documentation build information">
    <p class="t-ticker__line">
      <span class="t-ticker__item t-ticker__item--strong">TESSERA DOCUMENTATION ARCHIVE</span>
      <span class="t-ticker__sep" aria-hidden="true">//</span>
      <span class="t-ticker__item">APP v{{ build.version }}</span>
      <span class="t-ticker__sep t-ticker__opt3" aria-hidden="true">|</span>
      <span class="t-ticker__item t-ticker__opt3">STATUS: RESEARCH PROTOTYPE</span>
      <span class="t-ticker__sep t-ticker__opt" aria-hidden="true">|</span>
      <span class="t-ticker__item t-ticker__opt">PLATFORM: WINDOWS 11 x64</span>
      <span class="t-ticker__sep t-ticker__opt2" aria-hidden="true">//</span>
      <span class="t-ticker__item t-ticker__opt2">DOCS BUILD {{ build.date }} · COMMIT {{ build.commit }}</span>
    </p>
  </div>
</template>

<style scoped>
.t-ticker {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: calc(var(--vp-z-index-nav) + 1);
  height: var(--t-ticker-h);
  background: var(--p-ink);
  color: var(--p-white);
  border-bottom: 1px solid var(--p-ink);
  overflow: hidden;
}

.t-ticker__line {
  margin: 0 auto;
  max-width: 1440px;
  height: 100%;
  padding: 0 clamp(12px, 3vw, 32px);
  display: flex;
  align-items: center;
  gap: 0.6rem;
  font-family: var(--t-font-mono);
  font-size: var(--t-size-label);
  font-weight: 500;
  letter-spacing: 0.08em;
  white-space: nowrap;
  overflow: hidden;
}

.t-ticker__item--strong {
  font-weight: 800;
}

.t-ticker__sep {
  color: var(--p-slate);
}

@media (max-width: 1023px) {
  .t-ticker__opt2 {
    display: none;
  }
}

@media (max-width: 899px) {
  .t-ticker__opt {
    display: none;
  }
}

@media (max-width: 519px) {
  .t-ticker__opt3 {
    display: none;
  }
}
</style>
