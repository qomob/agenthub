<script setup lang="ts">
import type { Tone } from '../lib/status'
import Icon from './Icon.vue'

withDefaults(
  defineProps<{
    label: string
    tone?: Tone
    dot?: boolean
    title?: string
    icon?: string
  }>(),
  { tone: 'neutral', dot: true, title: undefined, icon: undefined },
)
</script>

<template>
  <span class="badge" :class="`badge-${tone}`" :title="title">
    <Icon v-if="icon" :name="icon" :size="10" />
    <span v-else-if="dot" class="badge-dot" aria-hidden="true" />
    <span>{{ label }}</span>
  </span>
</template>
