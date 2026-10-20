<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import DataTable from '../components/DataTable.vue'
import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import HealthStatusBadge from '../components/HealthStatusBadge.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatCard from '../components/StatCard.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { formatDateTime, formatRelativeTime } from '../lib/format'
import {
  HEALTH_FILTERS,
  auditRiskKey,
  auditRiskTone,
  filterAndSortHealthCapabilities,
  healthStateKey,
  topHealthReasons,
} from '../lib/health'
import type { HealthFilter } from '../lib/health'
import type { CapabilityHealth, HealthOverview } from '../lib/types'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const router = useRouter()
const ui = useUiStore()

const overview = ref<HealthOverview | null>(null)
const loading = ref(false)
const refreshing = ref(false)
const error = ref<string | null>(null)
const search = ref('')
const healthFilter = ref<HealthFilter>('all')

const summary = computed(() => overview.value?.summary ?? null)
const capabilities = computed(() => overview.value?.capabilities ?? [])
const filtered = computed(() =>
  filterAndSortHealthCapabilities(capabilities.value, healthFilter.value, search.value),
)
const hasFilter = computed(() => search.value.trim().length > 0 || healthFilter.value !== 'all')

const filterOptions = computed(() =>
  HEALTH_FILTERS.map((key) => ({
    key,
    label: key === 'all' ? t('health.filterAll') : t(healthStateKey[key]),
  })),
)

const columns = computed(() => [
  { key: 'name', label: t('health.columns.name') },
  { key: 'version', label: t('health.columns.version'), width: '100px' },
  { key: 'health', label: t('health.columns.health'), width: '150px' },
  { key: 'reasons', label: t('health.columns.reasons') },
  { key: 'installations', label: t('health.columns.installations'), width: '100px' },
  { key: 'verifiedAgents', label: t('health.columns.verifiedAgents'), width: '115px' },
  { key: 'auditRisk', label: t('health.columns.auditRisk'), width: '130px' },
])

onMounted(() => {
  void load()
})

async function load(): Promise<void> {
  loading.value = true
  error.value = null
  try {
    overview.value = await api.healthOverview()
  } catch (err) {
    error.value = normalizeError(err, t('errors.healthFailed'))
  } finally {
    loading.value = false
  }
}

/** Overview refresh + record this agent-version detection pass (record = true). */
async function recheck(): Promise<void> {
  refreshing.value = true
  error.value = null
  try {
    overview.value = await api.healthOverview()
    const report = await api.agentHealth(true)
    const changed = report.agents.filter((agent) => agent.versionChanged).length
    ui.notify('success', t('health.recheckDone'), {
      detail: changed > 0 ? t('health.agentVersionChanged', { count: changed }) : undefined,
    })
  } catch (err) {
    error.value = normalizeError(err, t('errors.healthFailed'))
  } finally {
    refreshing.value = false
  }
}

function openCapability(capability: CapabilityHealth): void {
  void router.push({ name: 'skill-detail', params: { id: capability.capabilityId } })
}

function clearFilters(): void {
  search.value = ''
  healthFilter.value = 'all'
}

function lastCheckedLabel(capability: CapabilityHealth): string {
  return capability.lastCheckedAt
    ? formatRelativeTime(capability.lastCheckedAt, locale.value)
    : t('health.neverChecked')
}

function auditRiskLabel(capability: CapabilityHealth): string {
  if (!capability.audit) return t('health.notAudited')
  return t(auditRiskKey[capability.audit.risk] ?? 'status.auditRisk.unknown')
}

function auditRiskToneFor(capability: CapabilityHealth) {
  return capability.audit ? (auditRiskTone[capability.audit.risk] ?? 'neutral') : 'neutral'
}

function capabilityKey(capability: CapabilityHealth): string {
  return capability.capabilityId
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('health.title')" :subtitle="t('health.subtitle')">
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="load">
          <span v-if="loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="refreshing" @click="recheck">
          <span v-if="refreshing" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="scan" :size="13" />
          {{ refreshing ? t('health.rechecking') : t('health.recheck') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.healthFailed')"
        :retry-label="t('common.retry')"
        :retrying="loading"
        @retry="load"
      />

      <LoadingState v-if="loading && !overview" :label="t('health.loading')" />

      <template v-else-if="overview && summary">
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('health.summaryTitle') }}</p>
              <p class="card-subtitle">
                {{ t('health.lastChecked') }}：
                <template v-if="summary.lastCheckedAt">
                  {{ formatRelativeTime(summary.lastCheckedAt, locale) }}
                  <span class="subtle">({{ formatDateTime(summary.lastCheckedAt, locale) }})</span>
                </template>
                <template v-else>{{ t('health.neverChecked') }}</template>
              </p>
            </div>
            <span class="badge badge-outline">
              {{ t('health.totalCapabilities', { count: summary.total }) }}
            </span>
          </div>
          <div class="card-body">
            <div class="stat-grid">
              <StatCard
                :label="t('health.summaryTotal')"
                :value="summary.total"
                :hint="t('health.summaryTotalHint')"
                icon="skills"
              />
              <StatCard
                :label="t('health.summaryVerified')"
                :value="summary.verified"
                :hint="t('health.summaryVerifiedHint')"
                icon="check-circle"
                tone="success"
              />
              <StatCard
                :label="t('health.summaryNeedsAttention')"
                :value="summary.needsAttention"
                :hint="t('health.summaryNeedsAttentionHint')"
                icon="alert-triangle"
                :tone="summary.needsAttention > 0 ? 'warn' : 'neutral'"
              />
              <StatCard
                :label="t('health.summaryUnverified')"
                :value="summary.unverified"
                :hint="t('health.summaryUnverifiedHint')"
                icon="alert-circle"
                tone="neutral"
              />
              <StatCard
                :label="t('health.summaryFailed')"
                :value="summary.failed"
                :hint="t('health.summaryFailedHint')"
                icon="x-circle"
                :tone="summary.failed > 0 ? 'danger' : 'neutral'"
              />
              <StatCard
                :label="t('health.summaryNotDeployed')"
                :value="summary.notDeployed"
                :hint="t('health.summaryNotDeployedHint')"
                icon="package"
                tone="slate"
              />
            </div>
            <p class="hint" style="margin-top: 10px">{{ t('health.unverifiedHint') }}</p>
          </div>
        </div>

        <div class="card">
          <div class="card-body">
            <div class="toolbar">
              <div class="search-wrap">
                <span class="search-icon"><Icon name="search" :size="13" /></span>
                <input
                  id="health-search"
                  v-model="search"
                  class="input"
                  type="search"
                  :placeholder="t('health.searchPlaceholder')"
                  :aria-label="t('health.searchLabel')"
                />
              </div>
              <select
                v-model="healthFilter"
                class="select"
                style="width: 160px"
                :aria-label="t('health.filterLabel')"
              >
                <option v-for="option in filterOptions" :key="option.key" :value="option.key">
                  {{ option.label }}
                </option>
              </select>
              <span class="badge badge-accent">
                {{ filtered.length }} / {{ capabilities.length }} {{ t('common.items') }}
              </span>
              <button
                v-if="hasFilter"
                type="button"
                class="btn btn-ghost btn-sm"
                @click="clearFilters"
              >
                {{ t('common.clearFilters') }}
              </button>
            </div>
          </div>
        </div>

        <EmptyState
          v-if="capabilities.length === 0"
          icon="shield"
          :title="t('health.emptyTitle')"
          :body="t('health.emptyBody')"
        />

        <EmptyState
          v-else-if="filtered.length === 0"
          icon="search"
          :title="t('health.noMatchTitle')"
          :body="t('health.noMatchBody')"
        >
          <template #actions>
            <button type="button" class="btn" @click="clearFilters">
              {{ t('common.clearFilters') }}
            </button>
          </template>
        </EmptyState>

        <div v-else class="card">
          <div class="card-body flush">
            <DataTable
              :columns="columns"
              :rows="filtered"
              :row-key="capabilityKey"
              clickable
              compact
              @row-click="openCapability"
            >
              <template #cell-name="{ row }">
                <div style="min-width: 0">
                  <div class="row" style="gap: 5px">
                    <span style="font-weight: 600">{{ row.name }}</span>
                    <span class="chip">{{ row.slug }}</span>
                  </div>
                  <p class="hint truncate" style="max-width: 340px" :title="row.description">
                    {{ row.description || '—' }}
                  </p>
                </div>
              </template>
              <template #cell-version="{ row }">
                <span class="chip">{{ row.versionLabel || '—' }}</span>
              </template>
              <template #cell-health="{ row }">
                <HealthStatusBadge :state="row.status" :title="lastCheckedLabel(row)" />
              </template>
              <template #cell-reasons="{ row }">
                <div v-if="row.reasons.length === 0" class="hint">{{ t('health.noReasons') }}</div>
                <ul v-else class="list-reset stack-sm">
                  <li
                    v-for="reason in topHealthReasons(row.reasons)"
                    :key="reason"
                    class="hint truncate"
                    :title="reason"
                    style="max-width: 320px"
                  >
                    • {{ reason }}
                  </li>
                </ul>
              </template>
              <template #cell-installations="{ row }">
                <span class="num">{{ row.installationCount }}</span>
              </template>
              <template #cell-verifiedAgents="{ row }">
                <span class="num">
                  {{ row.verifiedAgentKeys.length }} / {{ row.agentKeys.length }}
                </span>
                <p
                  v-if="row.agentKeys.length > 0"
                  class="hint truncate"
                  style="max-width: 110px"
                  :title="row.agentKeys.join(' · ')"
                >
                  {{ row.agentKeys.join(' · ') }}
                </p>
              </template>
              <template #cell-auditRisk="{ row }">
                <div class="stack-sm">
                  <StatusBadge
                    :label="auditRiskLabel(row)"
                    :tone="auditRiskToneFor(row)"
                    :dot="false"
                  />
                  <p v-if="row.audit" class="hint">
                    {{ formatRelativeTime(row.audit.createdAt, locale) }}
                  </p>
                </div>
              </template>
            </DataTable>
          </div>
        </div>

        <p class="hint">{{ t('health.unverifiedHint') }}</p>
      </template>
    </div>
  </div>
</template>
