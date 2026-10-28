<script setup lang="ts">
import { computed } from 'vue'

// Implementation status label. The word is always shown, so status never depends on colour.
const props = defineProps<{
  status: 'implemented' | 'partial' | 'planned' | 'unverified' | 'pass' | 'fail' | 'blocked'
}>()

const label = computed(
  () =>
    ({
      implemented: 'implemented',
      partial: 'partial',
      planned: 'planned',
      unverified: 'unverified',
      pass: 'pass',
      fail: 'fail',
      blocked: 'blocked',
    })[props.status],
)
</script>

<template>
  <span class="t-badge" :class="`is-${status}`">{{ label }}</span>
</template>

<style scoped>
/* Solid border = done, dashed = partial / planned / unverified, inverted = fail / blocked. */
.t-badge {
  display: inline-block;
  padding: 0 0.4rem;
  border: 1px solid var(--p-ink);
  border-radius: 0;
  background: var(--p-white);
  font-family: var(--t-font-mono);
  font-size: 0.6875rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  line-height: 1.7;
  text-transform: uppercase;
  white-space: nowrap;
  color: var(--p-ink);
}

.t-badge.is-partial,
.t-badge.is-planned,
.t-badge.is-unverified {
  border-style: dashed;
  background: var(--p-paper);
}

.t-badge.is-fail,
.t-badge.is-blocked {
  background: var(--p-ink);
  color: var(--p-white);
}
</style>
