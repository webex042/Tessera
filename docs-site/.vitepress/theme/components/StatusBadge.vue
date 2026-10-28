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
.t-badge {
  display: inline-block;
  padding: 0.05rem 0.45rem;
  border: 1px solid var(--p-blue);
  border-radius: 3px;
  font-family: var(--t-font-mono);
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.05em;
  line-height: 1.6;
  white-space: nowrap;
  color: var(--p-rose);
}

.t-badge.is-implemented,
.t-badge.is-pass {
  border-style: solid;
}

.t-badge.is-partial,
.t-badge.is-planned,
.t-badge.is-unverified {
  border-style: dashed;
}

.t-badge.is-fail,
.t-badge.is-blocked {
  border-color: var(--p-rose);
  background: var(--p-rose);
  color: var(--p-black);
}
</style>
