<script setup lang="ts">
import { onMounted, ref } from 'vue'

// Renders a ```mermaid fence (see config.ts) in the browser, themed with Palette A only.
const props = defineProps<{ code: string }>()

const host = ref<HTMLElement | null>(null)
const failed = ref(false)
const source = (() => {
  try {
    return decodeURIComponent(props.code)
  } catch {
    return props.code
  }
})()

// Mermaid needs literal colours (it cannot read CSS variables). Only the three palette values.
const BLACK = '#0A0A0A'
const BLUE = '#007BA7'
const ROSE = '#FF2D95'

let counter = 0

onMounted(async () => {
  try {
    const mermaid = (await import('mermaid')).default
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'base',
      fontFamily: "'JetBrains Mono Variable', ui-monospace, monospace",
      themeVariables: {
        darkMode: true,
        background: BLACK,
        fontSize: '14px',
        primaryColor: BLACK,
        primaryTextColor: ROSE,
        primaryBorderColor: BLUE,
        secondaryColor: BLACK,
        secondaryTextColor: ROSE,
        secondaryBorderColor: BLUE,
        tertiaryColor: BLACK,
        tertiaryTextColor: ROSE,
        tertiaryBorderColor: BLUE,
        mainBkg: BLACK,
        nodeBorder: BLUE,
        nodeTextColor: ROSE,
        textColor: ROSE,
        titleColor: ROSE,
        lineColor: BLUE,
        clusterBkg: BLACK,
        clusterBorder: BLUE,
        edgeLabelBackground: BLACK,
        noteBkgColor: BLACK,
        noteTextColor: ROSE,
        noteBorderColor: ROSE,
        actorBkg: BLACK,
        actorBorder: BLUE,
        actorTextColor: ROSE,
        actorLineColor: BLUE,
        signalColor: BLUE,
        signalTextColor: ROSE,
        labelBoxBkgColor: BLACK,
        labelBoxBorderColor: BLUE,
        labelTextColor: ROSE,
        loopTextColor: ROSE,
        activationBkgColor: BLACK,
        activationBorderColor: ROSE,
        sequenceNumberColor: BLACK,
        altBackground: BLACK,
        attributeBackgroundColorOdd: BLACK,
        attributeBackgroundColorEven: BLACK,
        errorBkgColor: BLACK,
        errorTextColor: ROSE,
      },
      // Belt and braces: anything Mermaid derives on its own is forced back to the palette.
      themeCSS: `
        .label, .nodeLabel, .edgeLabel, text, .cluster-label { color: ${ROSE}; fill: ${ROSE}; }
        .edgeLabel, .edgeLabel p, .labelBkg { background-color: ${BLACK}; }
        .cluster rect { fill: ${BLACK}; stroke: ${BLUE}; stroke-dasharray: 4 3; }
        .node rect, .node circle, .node polygon, .node path { fill: ${BLACK}; stroke: ${BLUE}; }
        .flowchart-link, .messageLine0, .messageLine1, .relation { stroke: ${BLUE}; }
        marker path, .arrowheadPath { fill: ${BLUE}; stroke: ${BLUE}; }
        .sequenceNumber { fill: ${BLACK}; }
        .actor { fill: ${BLACK}; stroke: ${BLUE}; }
        .note { fill: ${BLACK}; stroke: ${ROSE}; }
        .er.entityBox, .er.attributeBoxOdd, .er.attributeBoxEven { fill: ${BLACK}; stroke: ${BLUE}; }
      `,
    })
    const id = `t-mermaid-${Date.now().toString(36)}-${counter++}`
    const { svg } = await mermaid.render(id, source)
    if (host.value) host.value.innerHTML = svg
  } catch {
    failed.value = true
  }
})
</script>

<template>
  <figure class="t-diagram">
    <div ref="host" class="t-diagram__svg" role="img" :aria-label="'Diagram'" />
    <figcaption v-if="failed" class="t-diagram__error">Diagram could not be rendered. Source:</figcaption>
    <pre v-if="failed" class="t-diagram__source"><code>{{ source }}</code></pre>
    <noscript><pre class="t-diagram__source"><code>{{ source }}</code></pre></noscript>
  </figure>
</template>

<style scoped>
.t-diagram {
  margin: 1.5rem 0;
  padding: 1rem;
  border: 1px solid var(--t-border-subtle);
  border-radius: var(--t-radius-lg);
  background: var(--t-bg);
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
  color: var(--p-rose);
}

.t-diagram__source {
  font-family: var(--t-font-mono);
  font-size: 0.8125rem;
  color: var(--p-rose);
  white-space: pre;
}
</style>
