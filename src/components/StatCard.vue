<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink } from 'vue-router'

import type { Tone } from '../lib/status'
import Icon from './Icon.vue'

const props = withDefaults(
  defineProps<{
    label: string
    value: string | number
    hint?: string
    icon?: string
    tone?: Tone
    to?: string
  }>(),
  { hint: undefined, icon: undefined, tone: 'neutral', to: undefined },
)

const toneClass = computed(() =>
  props.tone === 'neutral' ? '' : `stat-tone-${props.tone}`,
)
</script>

<template>
  <component :is="to ? RouterLink : 'div'" :to="to" class="stat-card" :class="toneClass">
    <div class="stat-head">
      <Icon v-if="icon" :name="icon" :size="13" />
      <span>{{ label }}</span>
    </div>
    <div class="stat-value">{{ value }}</div>
    <div v-if="hint" class="stat-hint">{{ hint }}</div>
  </component>
</template>
