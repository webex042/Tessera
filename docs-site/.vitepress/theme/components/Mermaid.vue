<script setup lang="ts">
import { onMounted, ref } from 'vue'

// Renders a ```mermaid fence (see config.ts) in the browser as a neutral engineering diagram.
const props = defineProps<{ code: string }>()

const host = ref<HTMLElement | null>(null)
const failed = ref(false)
// The source is rendered on the server and stays until the diagram is drawn, so it is the
// fallback without JavaScript. (A <noscript> fallback caused hydration mismatches.)
const rendered = ref(false)
const source = (() => {
  try {
    return decodeURIComponent(props.code)
  } catch {
    return props.code
  }
})()

// Mermaid needs literal colours (it cannot read CSS variables). Conventional engineering
// diagrams: white boxes, black strokes and arrows, grey groups; values from tokens.css only.
const INK = '#000000'
const WHITE = '#FFFFFF'
const PAPER = '#F5F5F5'
const TEXT = '#111111'
const CHARCOAL = '#1A1A1A'

let counter = 0

onMounted(async () => {
  try {
    const mermaid = (await import('mermaid')).default
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'base',
      fontFamily: "'Inter Variable', system-ui, sans-serif",
      themeVariables: {
        darkMode: false,
        background: WHITE,
        fontSize: '14px',
        primaryColor: WHITE,
        primaryTextColor: TEXT,
        primaryBorderColor: INK,
        secondaryColor: PAPER,
        secondaryTextColor: TEXT,
        secondaryBorderColor: INK,
        tertiaryColor: PAPER,
        tertiaryTextColor: TEXT,
        tertiaryBorderColor: CHARCOAL,
        mainBkg: WHITE,
        nodeBorder: INK,
        nodeTextColor: TEXT,
        textColor: TEXT,
        titleColor: TEXT,
        lineColor: INK,
        clusterBkg: PAPER,
        clusterBorder: CHARCOAL,
        edgeLabelBackground: WHITE,
        noteBkgColor: PAPER,
        noteTextColor: TEXT,
        noteBorderColor: INK,
        actorBkg: WHITE,
        actorBorder: INK,
        actorTextColor: TEXT,
        actorLineColor: INK,
        signalColor: INK,
        signalTextColor: TEXT,
        labelBoxBkgColor: WHITE,
        labelBoxBorderColor: INK,
        labelTextColor: TEXT,
        loopTextColor: TEXT,
        activationBkgColor: PAPER,
        activationBorderColor: INK,
        sequenceNumberColor: WHITE,
        altBackground: PAPER,
        attributeBackgroundColorOdd: WHITE,
        attributeBackgroundColorEven: PAPER,
        errorBkgColor: WHITE,
        errorTextColor: TEXT,
      },
      // Anything Mermaid derives on its own is forced back to the neutral scheme.
      themeCSS: `
        .label, .nodeLabel, .edgeLabel, text, .cluster-label { color: ${TEXT}; fill: ${TEXT}; }
        .edgeLabel, .edgeLabel p, .labelBkg { background-color: ${WHITE}; }
        .cluster rect { fill: ${PAPER}; stroke: ${CHARCOAL}; stroke-dasharray: 4 3; }
        .node rect, .node circle, .node polygon, .node path { fill: ${WHITE}; stroke: ${INK}; stroke-width: 1.25px; }
        .flowchart-link, .messageLine0, .messageLine1, .relation { stroke: ${INK}; stroke-width: 1.1px; }
        marker path, .arrowheadPath { fill: ${INK}; stroke: ${INK}; }
        .sequenceNumber { fill: ${WHITE}; }
        .actor { fill: ${WHITE}; stroke: ${INK}; }
        .note { fill: ${PAPER}; stroke: ${INK}; }
        .er.entityBox, .er.attributeBoxOdd, .er.attributeBoxEven { stroke: ${INK}; }
      `,
    })
    const id = `t-mermaid-${Date.now().toString(36)}-${counter++}`
    const { svg } = await mermaid.render(id, source)
    if (host.value) {
      host.value.innerHTML = svg
      rendered.value = true
    }
  } catch {
    failed.value = true
  }
})
</script>

<template>
  <figure class="t-diagram">
    <div ref="host" class="t-diagram__svg" role="img" :aria-label="'Diagram'" />
    <figcaption v-if="failed" class="t-diagram__error">Diagram could not be rendered. Source:</figcaption>
    <pre v-if="!rendered" class="t-diagram__source"><code>{{ source }}</code></pre>
  </figure>
</template>

<style scoped>
.t-diagram {
  margin: 1.5rem 0;
  padding: 1rem;
  border: 1px solid var(--p-charcoal);
  background: var(--p-white);
  overflow-x: auto;
}

.t-diagram__svg {
  min-width: 560px;
  display: flex;
  justify-content: center;
}

.t-diagram__svg :deep(svg) {
  max-width: 100%;
  height: auto;
}

.t-diagram__error {
  font-family: var(--t-font-mono);
  font-size: var(--t-size-meta);
  color: var(--p-ink);
}

.t-diagram__source {
  font-family: var(--t-font-mono);
  font-size: 0.8125rem;
  color: var(--p-text);
  white-space: pre;
}
</style>
