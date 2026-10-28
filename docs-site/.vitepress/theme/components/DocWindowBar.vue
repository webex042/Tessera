<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vitepress'

// Title bar and document header for every documentation page. The file-style name, section
// and position are derived from the page itself and the sidebar in config.ts.
const { page, frontmatter, theme } = useData()

type Item = { text?: string; link?: string; items?: Item[] }

const title = computed(() => (frontmatter.value.title as string | undefined) ?? page.value.title ?? 'Document')

const fileName = computed(() => {
  const stem = title.value
    .replace(/&/g, ' and ')
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toUpperCase()
  return `${stem || 'DOCUMENT'}.TXT`
})

const route = computed(() => '/' + page.value.relativePath.replace(/(^|\/)index\.md$/, '$1').replace(/\.md$/, ''))

const group = computed(() => {
  const sidebar = theme.value.sidebar as Record<string, Item[]> | undefined
  if (!sidebar) return null
  const key = Object.keys(sidebar)
    .filter((k) => route.value.startsWith(k))
    .sort((a, b) => b.length - a.length)[0]
  if (!key) return null
  const first = sidebar[key][0]
  const items = (first?.items ?? []).filter((i) => i.link)
  const index = items.findIndex((i) => i.link === route.value || i.link === route.value.replace(/\/$/, ''))
  return { name: first?.text ?? '', index, total: items.length }
})

const pad = (n: number) => String(n).padStart(2, '0')
</script>

<template>
  <div class="t-window-bar">
    <span class="t-window-bar__icon" aria-hidden="true"></span>
    <span class="t-window-bar__name">{{ fileName }}</span>
    <span class="t-window-bar__controls" aria-hidden="true">
      <span class="t-window-bar__ctl">_</span>
      <span class="t-window-bar__ctl">□</span>
      <span class="t-window-bar__ctl">×</span>
    </span>
  </div>
  <div class="t-doc-meta">
    <p class="t-doc-meta__eyebrow">
      <span v-if="group">SECTION // {{ group.name.toUpperCase() }}</span>
      <span v-if="group && group.index >= 0"> · DOCUMENT {{ pad(group.index + 1) }} OF {{ pad(group.total) }}</span>
    </p>
    <p class="t-doc-meta__source">SOURCE: docs-site/{{ page.relativePath }}</p>
  </div>
</template>

<style scoped>
.t-window-bar {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  height: var(--t-titlebar-h);
  padding: 0 0.5rem 0 0.75rem;
  background: var(--p-ink);
  color: var(--p-white);
  border-bottom: 2px solid var(--p-ink);
  font-family: var(--t-font-mono);
  font-size: var(--t-size-meta);
  font-weight: 700;
  letter-spacing: 0.06em;
  user-select: none;
}

/* A tiny document glyph: white page with a folded corner. */
.t-window-bar__icon {
  flex: none;
  width: 11px;
  height: 13px;
  background: var(--p-white);
  clip-path: polygon(0 0, 70% 0, 100% 28%, 100% 100%, 0 100%);
}

.t-window-bar__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Decorative only: flat, no hover or pointer state, hidden from assistive technology. */
.t-window-bar__controls {
  flex: none;
  display: flex;
  gap: 4px;
}

.t-window-bar__ctl {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: 1px solid var(--p-white);
  font-size: 0.7rem;
  line-height: 1;
  cursor: default;
}

.t-doc-meta {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 0.25rem 1.5rem;
  padding: 0.6rem clamp(16px, 3.5vw, 40px);
  border-bottom: 1px solid var(--p-line);
  background: var(--p-paper);
  font-family: var(--t-font-mono);
  font-size: var(--t-size-label);
  font-weight: 700;
  letter-spacing: 0.08em;
}

.t-doc-meta p {
  margin: 0;
  color: var(--p-text);
}

.t-doc-meta__source {
  overflow-wrap: anywhere;
  font-weight: 500;
}
</style>
