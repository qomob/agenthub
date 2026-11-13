<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import ErrorBanner from './ErrorBanner.vue'
import StatusBadge from './StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { useStackStore } from '../stores/stack'
import type {
  OptimizerMoneyDelta,
  OptimizerRecommendation,
  OptimizerReport,
  OptimizerScenarioResult,
  OptimizerSettings,
  ScenarioAction,
  SpendingOverview,
} from '../lib/types'
import { formatMoneyMinor } from '../lib/stack'

const { t } = useI18n()

const report = ref<OptimizerReport | null>(null)
const settings = ref<OptimizerSettings | null>(null)
// Spending figures come from the stack store: the TTL cache owns them, and a
// panel-level duplicate request would break the M1-C.1 caching contract.
const stackStore = useStackStore()
const spending = computed<SpendingOverview | null>(() => stackStore.spending)
const loading = ref(false)
const error = ref<string | null>(null)

// --- Settings editor (M2-B review P1-1) -------------------------------------
// Rows are local edits of the persisted config; save sends exactly the
// changed shape and the service merges under the runtime lock.
const thresholdRows = ref<Array<{ currency: string; minor: string }>>([])
const windowDays = ref<string>('')
const savingSettings = ref(false)
const settingsError = ref<string | null>(null)
const settingsSaved = ref(false)

function hydrateEditor(): void {
  if (!settings.value) return
  thresholdRows.value = Object.entries(settings.value.costThresholdsMinor).map(
    ([currency, minor]) => ({ currency, minor: String(minor) }),
  )
  if (thresholdRows.value.length === 0) thresholdRows.value = [{ currency: '', minor: '' }]
  windowDays.value = settings.value.renewalWindowDays?.toString() ?? ''
}

function addThresholdRow(): void {
  thresholdRows.value.push({ currency: '', minor: '' })
}

function removeThresholdRow(index: number): void {
  thresholdRows.value.splice(index, 1)
}

async function saveSettings(): Promise<void> {
  savingSettings.value = true
  settingsError.value = null
  settingsSaved.value = false
  try {
    // Only non-empty rows become thresholds; an emptied editor clears the
    // whole table (the service validates the same way for GUI and CLI).
    const rows = thresholdRows.value.filter(
      (row) => row.currency.trim() !== '' || row.minor.trim() !== '',
    )
    const anyEmpty =
      rows.length === 0 ||
      rows.some((row) => row.currency.trim() === '' || row.minor.trim() === '')
    const thresholds: Record<string, number> | undefined = anyEmpty
      ? undefined
      : Object.fromEntries(
          rows.map((row) => [row.currency.trim().toUpperCase(), Number(row.minor.trim())]),
        )
    // Option<Option<i64>>: undefined = leave unchanged, null = clear to the
    // engine default, number = set.
    const windowArg =
      windowDays.value.trim() === '' ? null : Number(windowDays.value.trim())
    settings.value = await api.optimizerUpdateSettings(
      thresholdRows.value.length === 0 || anyEmpty ? undefined : thresholds,
      windowDays.value.trim() === '' ? null : windowArg,
    )
    settingsSaved.value = true
    hydrateEditor()
    // Rules depend on the config: re-run the report so the panel reflects it.
    report.value = await api.optimizerReport()
  } catch (err) {
    settingsError.value = normalizeError(err, t('stack.errors.saveFailed'))
  } finally {
    savingSettings.value = false
  }
}

// --- Simulation (M2-B review P1-1) ------------------------------------------
// Pure arithmetic; nothing here can cancel a subscription.
const simulateTarget = ref<string>('')
const simulating = ref(false)
const simulateError = ref<string | null>(null)
const simulateResult = ref<OptimizerScenarioResult | null>(null)

const recommendations = computed(() => report.value?.recommendations ?? [])

/** Active subscriptions come from the spending overview buckets, whose union
 * covers every active record (renewal classes + incomplete + usage). */
const simulationTargets = computed(() => {
  const overview = spending.value
  if (!overview) return []
  const seen = new Set<string>()
  const out: Array<{ id: string; label: string }> = []
  for (const sub of [
    ...overview.upcomingRenewals,
    ...overview.renewalsUnknown,
    ...overview.staleRenewals,
    ...overview.laterRenewals,
    ...overview.incomplete.map((entry) => entry.subscription),
    ...overview.usageWithoutBills,
  ]) {
    if (seen.has(sub.id)) continue
    seen.add(sub.id)
    out.push({ id: sub.id, label: sub.productName })
  }
  return out
})

async function runSimulation(): Promise<void> {
  if (!simulateTarget.value) return
  simulating.value = true
  simulateError.value = null
  simulateResult.value = null
  try {
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
  const abs = formatMoneyMinor(Math.abs(delta.deltaMinor), delta.currency) ?? `${Math.abs(delta.deltaMinor)}`
  return `${before} → ${after} (${delta.deltaMinor >= 0 ? '+' : '−'}${abs})`
}

async function load(): Promise<void> {
  loading.value = true
  error.value = null
  try {
    // Both surfaces read the same persisted settings — one config, two views.
    const [rep, cfg] = await Promise.all([api.optimizerReport(), api.optimizerSettings()])
    // Cached per the store TTL; no extra core round-trip.
    await stackStore.loadSpending()
    report.value = rep
    settings.value = cfg
    hydrateEditor()
  } catch (err) {
    error.value = normalizeError(err, t('stack.errors.loadFailed'))
  } finally {
    loading.value = false
  }
}

defineExpose({ load, simulateCancel })

/** Test hook and programmatic entry: simulate cancelling one subscription. */
function simulateCancel(subscriptionId: string): Promise<void> {
  simulateTarget.value = subscriptionId
  return runSimulation()
}

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
      <p v-if="recommendations.length === 0" class="hint">
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

      <!-- Simulation entry: click-driven, read-only arithmetic. -->
      <div class="insight-item" style="margin-top: 14px">
        <strong style="font-size: 14px">{{ t('stack.costSimulateTitle') }}</strong>
        <p class="hint" style="margin: 4px 0 8px">{{ t('stack.costSimulateHint') }}</p>
        <div v-if="simulationTargets.length === 0" class="hint">
          {{ t('stack.costSimulateNothing') }}
        </div>
        <div v-else class="row" style="gap: 8px; align-items: center">
          <select v-model="simulateTarget" class="input" style="max-width: 260px" data-testid="simulate-target">
            <option value="" disabled>—</option>
            <option v-for="target in simulationTargets" :key="target.id" :value="target.id">
              {{ target.label }}
            </option>
          </select>
          <button
            type="button"
            class="btn"
            data-testid="simulate-run"
            :disabled="simulating || simulateTarget === ''"
            @click="runSimulation"
          >
            <span v-if="simulating" class="spinner spinner-sm" aria-hidden="true" />
            {{ t('stack.costSimulateRun') }}
          </button>
        </div>
        <ErrorBanner
          v-if="simulateError"
          :message="simulateError"
          :title="t('stack.errors.loadFailed')"
        />
        <div
          v-if="simulateResult"
          data-testid="simulate-result"
          style="margin-top: 10px"
        >
          <p class="hint" style="margin: 0 0 4px">{{ t('stack.costSimulateResult') }}</p>
          <table class="table">
            <tbody>
              <tr v-for="delta in simulateResult.deltasMonthly" :key="`m-${delta.currency}`">
                <td>{{ delta.currency }} · monthly</td>
                <td style="font-variant-numeric: tabular-nums">{{ formatDelta(delta) }}</td>
              </tr>
              <tr v-for="delta in simulateResult.deltasAnnual" :key="`a-${delta.currency}`">
                <td>{{ delta.currency }} · annual</td>
                <td style="font-variant-numeric: tabular-nums">{{ formatDelta(delta) }}</td>
              </tr>
            </tbody>
          </table>
          <p v-for="note in simulateResult.notes" :key="note" class="hint" style="margin: 2px 0 0">
            {{ note }}
          </p>
        </div>
      </div>

      <!-- Settings editor: same validation/storage path as `cost set`. -->
      <div class="insight-item" style="margin-top: 14px">
        <strong style="font-size: 14px">{{ t('stack.costSettingsTitle') }}</strong>
        <p class="hint" style="margin: 4px 0 8px">{{ t('stack.costThresholdHint') }}</p>
        <div
          v-for="(row, index) in thresholdRows"
          :key="index"
          class="row"
          style="gap: 8px; align-items: center; margin-bottom: 6px"
        >
          <input
            v-model="row.currency"
            class="input"
            style="max-width: 110px"
            placeholder="USD"
            :aria-label="t('stack.costThresholdLabel')"
            :data-testid="`threshold-currency-${index}`"
          />
          <input
            v-model="row.minor"
            class="input"
            style="max-width: 140px"
            inputmode="numeric"
            placeholder="5000"
            :data-testid="`threshold-minor-${index}`"
          />
          <button
            type="button"
            class="btn btn-sm btn-ghost"
            @click="removeThresholdRow(index)"
          >
            {{ t('common.delete') }}
          </button>
        </div>
        <button type="button" class="btn btn-sm" style="margin-bottom: 10px" @click="addThresholdRow">
          +
        </button>
        <div class="row" style="gap: 8px; align-items: center">
          <label class="label" for="optimizer-window">{{ t('stack.costWindowLabel') }}</label>
          <input
            id="optimizer-window"
            v-model="windowDays"
            class="input"
            style="max-width: 110px"
            inputmode="numeric"
            placeholder="7"
            data-testid="optimizer-window"
          />
        </div>
        <p class="hint" style="margin: 4px 0 8px">{{ t('stack.costWindowHint') }}</p>
        <ErrorBanner
          v-if="settingsError"
          :message="settingsError"
          :title="t('stack.errors.saveFailed')"
        />
        <div class="row" style="gap: 8px; align-items: center">
          <button
            type="button"
            class="btn btn-primary"
            data-testid="settings-save"
            :disabled="savingSettings"
            @click="saveSettings"
          >
            <span v-if="savingSettings" class="spinner spinner-sm" aria-hidden="true" />
            {{ t('common.save') }}
          </button>
          <span v-if="settingsSaved" class="hint">{{ t('stack.costSettingsSaved') }}</span>
        </div>
      </div>
    </template>
  </section>
</template>
