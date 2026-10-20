<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { healthStateIcon, healthStateKey, healthStateTone } from '../lib/health'
import type { HealthState } from '../lib/types'
import StatusBadge from './StatusBadge.vue'

const props = withDefaults(
  defineProps<{
    state: HealthState
    title?: string
  }>(),
  { title: undefined },
)

const { t } = useI18n()

const label = computed(() => t(healthStateKey[props.state] ?? 'status.health.unknown'))
const tone = computed(() => healthStateTone[props.state] ?? 'neutral')
const icon = computed(() => healthStateIcon[props.state] ?? 'alert-circle')
</script>

<template>
  <StatusBadge :label="label" :tone="tone" :icon="icon" :title="title" />
</template>
