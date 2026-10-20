<script setup lang="ts">
import type { Tone } from '../lib/status'

withDefaults(
  defineProps<{
    label: string
    tone?: Tone
    dot?: boolean
    title?: string
  }>(),
  { tone: 'neutral', dot: true, title: undefined },
)
</script>

<template>
  <span class="badge" :class="`badge-${tone}`" :title="title">
    <span v-if="dot" class="badge-dot" aria-hidden="true" />
    <span>{{ label }}</span>
  </span>
</template>
