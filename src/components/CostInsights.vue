<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import ErrorBanner from './ErrorBanner.vue'
import StatusBadge from './StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import type {
  OptimizerMoneyDelta,
  OptimizerRecommendation,
  OptimizerReport,
  OptimizerScenarioResult,
  OptimizerSettings,
  ScenarioAction,
} from '../lib/types'
import { formatMoneyMinor } from '../lib/stack'

const { t } = useI18n()

const report = ref<OptimizerReport | null>(null)
const settings = ref<OptimizerSettings | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

// Simulation state — pure arithmetic; nothing here can cancel a subscription.
const simulateTarget = ref<string>('')
const simulating = ref(false)
const simulateError = ref<string | null>(null)
const simulateResult = ref<OptimizerScenarioResult | null>(null)

const recommendations = computed(() => report.value?.recommendations ?? [])

const dataGapCount = computed(
  () => recommendations.value.filter((r) => r.recommendationType === 'R5_DATA_GAP').length,
)

const reviewCount = computed(
  () =>
    recommendations.value.filter((r) => r.recommendationType !== 'R5_DATA_GAP')
      .length,
)

async function load(): Promise<void> {
  loading.value = true
  error.value = null
  try {
    // Both surfaces read the same persisted settings — one config, two views.
    const [rep, cfg] = await Promise.all([api.optimizerReport(), api.optimizerSettings()])
    report.value = rep
    settings.value = cfg
  } catch (err) {
    error.value = normalizeError(err, t('stack.errors.loadFailed'))
  } finally {
    loading.value = false
  }
}

async function runSimulation(): Promise<void> {
  if (!simulateTarget.value) return
  simulating.value = true
  simulateError.value = null
  simulateResult.value = null
  try {
    // A cancel scenario only simulates; the note in the result repeats it.
    const actions: ScenarioAction[] = [
      { kind: 'cancel', subscriptionId: simulateTarget.value },
    ]
    simulateResult.value = await api.optimizerSimulate(actions)
  } catch (err) {
    simulateError.value = normalizeError(err, t('stack.errors.loadFailed'))
  } finally {
    simulating.value = false
  }
}

function formatDelta(delta: OptimizerMoneyDelta): string {
  const before = formatMoneyMinor(delta.minorBefore, delta.currency) ?? `${delta.minorBefore}`
  const after = formatMoneyMinor(delta.minorAfter, delta.currency) ?? `${delta.minorAfter}`
  return `${before} → ${after} (${delta.deltaMinor >= 0 ? '+' : ''}${formatMoneyMinor(
    Math.abs(delta.deltaMinor),
    delta.currency,
  ) ?? delta.deltaMinor})`
}

/// Exposed for tests and future callers: drive the read-only simulation
/// with an explicit subscription id (nothing here can cancel anything).
function simulateCancel(subscriptionId: string): Promise<void> {
  simulateTarget.value = subscriptionId
  return runSimulation()
}

defineExpose({ load, simulateCancel })
onMounted(load)
</script>

<template>
  <section class="surface" style="padding: 18px">
    <div class="row" style="justify-content: space-between; align-items: center">
      <h2 style="margin: 0; font-size: 16px">{{ t('stack.insightsTitle') }}</h2>
      <button type="button" class="btn btn-sm" :disabled="loading" @click="load">
        <span v-if="loading" class="spinner spinner-sm" aria-hidden="true" />
        {{ t('common.refresh') }}
      </button>
    </div>

    <ErrorBanner v-if="error" :message="error" :title="t('stack.errors.loadFailed')" />

    <template v-else>
      <p v-if="!loading && recommendations.length === 0" class="hint">
        {{ t('stack.insightsEmpty') }}
      </p>

      <ul v-else class="insight-list">
        <li v-for="rec in recommendations" :key="rec.recommendationId" class="insight-item">
          <div class="row" style="gap: 8px; align-items: center">
            <StatusBadge
              :label="t(`stack.insightType.${rec.recommendationType}`)"
              :tone="rec.recommendationType === 'R5_DATA_GAP' ? 'neutral' : 'info'"
            />
            <strong style="font-size: 14px">{{ rec.title }}</strong>
          </div>
          <p class="hint" style="white-space: pre-wrap; margin: 6px 0 0">
            {{ rec.explanation }}
          </p>
          <p class="hint" style="margin: 4px 0 0">
            {{ t(`stack.insightConfidence.${rec.confidenceOrEvidenceLevel}`) }}
            · {{ t(`stack.insightAction.${rec.userActionRequired}`) }}
          </p>
        </li>
      </ul>
    </template>
  </section>
</template>
