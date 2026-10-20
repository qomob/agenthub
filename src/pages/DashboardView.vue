<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink, useRouter } from 'vue-router'

import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatCard from '../components/StatCard.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { countDefaultImportableGroups, groupDiscoveredSkills } from '../lib/discovered'
import { formatDateTime, formatRelativeTime } from '../lib/format'
import {
  agentStatusKey,
  operationKindKey,
  operationStatusKey,
  toneForAgentStatus,
  toneForOperationStatus,
  severityKey,
  toneForSeverity,
} from '../lib/status'
import type { ChangeCheck, HealthOverview, Issue } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const app = useAppStore()
const ui = useUiStore()
const router = useRouter()

const dashboard = computed(() => app.dashboard)
const loading = computed(() => app.dashboardLoading && !app.dashboard)
const error = computed(() => app.dashboardError)
const scanReport = computed(() => app.scanReport)
const importableDiscovered = computed(() =>
  countDefaultImportableGroups(groupDiscoveredSkills(app.scanReport)),
)
const changes = ref<ChangeCheck | null>(null)
const checkingChanges = ref(false)
const stats = computed(() => dashboard.value?.stats ?? null)

const healthOverview = ref<HealthOverview | null>(null)
const healthLoading = ref(false)
const healthError = ref<string | null>(null)
const health = computed(() => healthOverview.value?.summary ?? null)

const issues = computed(() => dashboard.value?.issues ?? [])
const updates = computed(() => dashboard.value?.updates ?? [])
const recentOperations = computed(() => dashboard.value?.recentOperations ?? [])
const agents = computed(() => dashboard.value?.agents ?? [])

onMounted(async () => {
  void loadHealth()
  try {
    await app.loadDashboard()
  } catch (err) {
    ui.notifyError(t('errors.dashboardFailed'), normalizeError(err))
  }
})

async function loadHealth(): Promise<void> {
  healthLoading.value = true
  healthError.value = null
  try {
    healthOverview.value = await api.healthOverview()
  } catch (err) {
    healthError.value = normalizeError(err, t('errors.healthFailed'))
  } finally {
    healthLoading.value = false
  }
}

async function refresh(): Promise<void> {
  void loadHealth()
  try {
    await app.loadDashboard(true)
  } catch (err) {
    ui.notifyError(t('errors.dashboardFailed'), normalizeError(err))
  }
}

async function checkChanges(): Promise<void> {
  checkingChanges.value = true
  try {
    changes.value = await api.checkChanges()
    const total =
      changes.value.drifted.length + changes.value.missing.length + changes.value.updates.length
    ui.notify('info', total > 0 ? t('dashboard.changesFound', { count: total }) : t('dashboard.changesNone'))
  } catch (err) {
    ui.notifyError(t('errors.changesFailed'), normalizeError(err))
  } finally {
    checkingChanges.value = false
  }
}

async function runScan(): Promise<void> {
  try {
    const report = await app.scan()
    ui.notify('success', t('dashboard.scanDone'), {
      detail: t('dashboard.scanResultSummary', {
        skills: report.totalSkills,
        duplicates: report.duplicates,
        conflicts: report.conflicts,
        invalid: report.invalid,
      }),
    })
  } catch (err) {
    ui.notifyError(t('errors.scanFailed'), normalizeError(err))
  }
}

function issueTarget(issue: Issue): string | null {
  if (issue.capabilityId) {
    return `/skills/${encodeURIComponent(issue.capabilityId)}`
  }
  if (issue.agentKey) {
    return `/agents?agent=${encodeURIComponent(issue.agentKey)}`
  }
  return null
}

function severityLabel(issue: Issue): string {
  const key = severityKey[issue.severity]
  return key ? t(key) : issue.severity
}

function openOperation(id: string): void {
  void router.push({ name: 'activity', query: { op: id } })
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('dashboard.title')" :subtitle="t('dashboard.subtitle')">
      <template #meta>
        <p class="hint" style="margin-top: 6px">
          {{ t('dashboard.lastScan') }}：
          {{
            dashboard?.lastScanAt
              ? formatRelativeTime(dashboard.lastScanAt, locale)
              : t('dashboard.neverScanned')
          }}
          <span v-if="dashboard" class="subtle"> · {{ t('dashboard.libraryRoot') }}：</span>
          <span v-if="dashboard" class="path-text" style="display: inline">{{ dashboard.libraryRoot }}</span>
        </p>
      </template>
      <template #actions>
        <button
          type="button"
          class="btn btn-ghost"
          :disabled="checkingChanges"
          @click="checkChanges"
        >
          <span v-if="checkingChanges" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="activity" :size="13" />
          {{ t('dashboard.checkChanges') }}
        </button>
        <button type="button" class="btn" :disabled="app.dashboardLoading" @click="refresh">
          <span v-if="app.dashboardLoading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="app.scanning" @click="runScan">
          <span v-if="app.scanning" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="scan" :size="13" />
          {{ app.scanning ? t('dashboard.scanning') : t('dashboard.scan') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.dashboardFailed')"
        :retry-label="t('common.retry')"
        @retry="refresh"
      />
      <ErrorBanner
        v-if="app.scanError"
        :message="app.scanError"
        :title="t('errors.scanFailed')"
        :retry-label="t('common.retry')"
        :retrying="app.scanning"
        @retry="runScan"
      />

      <LoadingState v-if="loading" :label="t('dashboard.loadingHint')" />

      <template v-else-if="dashboard">
        <div class="stat-grid">
          <StatCard
            :label="t('dashboard.statAgents')"
            :value="`${stats?.agentsDetected ?? 0} / ${stats?.agentsTotal ?? 0}`"
            :hint="t('dashboard.statAgentsHint')"
            icon="agents"
            to="/agents"
          />
          <StatCard
            :label="t('dashboard.statSkills')"
            :value="stats?.skills ?? 0"
            :hint="t('dashboard.statSkillsHint')"
            icon="skills"
            to="/skills"
          />
          <StatCard
            :label="t('dashboard.statInstallations')"
            :value="stats?.installations ?? 0"
            :hint="t('dashboard.statInstallationsHint')"
            icon="matrix"
            to="/matrix"
          />
          <StatCard
            :label="t('dashboard.statMcp')"
            :value="stats?.mcpServers ?? 0"
            :hint="t('dashboard.statMcpHint')"
            icon="mcp"
            to="/mcp"
          />
          <StatCard
            :label="t('dashboard.statPlugins')"
            :value="stats?.plugins ?? 0"
            :hint="t('dashboard.statPluginsHint')"
            icon="plugins"
            to="/plugins"
          />
          <StatCard
            :label="t('dashboard.statUpdates')"
            :value="stats?.pendingUpdates ?? 0"
            :hint="t('dashboard.statUpdatesHint')"
            icon="import"
            :tone="(stats?.pendingUpdates ?? 0) > 0 ? 'info' : 'neutral'"
            to="/skills?filter=updates"
          />
          <StatCard
            :label="t('dashboard.statConflicts')"
            :value="stats?.conflicts ?? 0"
            :hint="t('dashboard.statConflictsHint')"
            icon="alert-triangle"
            :tone="(stats?.conflicts ?? 0) > 0 ? 'warn' : 'neutral'"
            to="/activity"
          />
          <StatCard
            :label="t('dashboard.statWarnings')"
            :value="stats?.warnings ?? 0"
            :hint="t('dashboard.statWarningsHint')"
            icon="activity"
            :tone="(stats?.warnings ?? 0) > 0 ? 'danger' : 'neutral'"
          />
          <StatCard
            v-if="importableDiscovered > 0"
            :label="t('dashboard.discoveredCard')"
            :value="importableDiscovered"
            :hint="t('dashboard.discoveredHint', { count: importableDiscovered })"
            icon="scan"
            tone="info"
            to="/skills?tab=discovered"
          />
        </div>

        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('dashboard.healthTitle') }}</p>
              <p class="card-subtitle">
                <template v-if="health?.lastCheckedAt">
                  {{ t('health.lastChecked') }}：{{ formatRelativeTime(health.lastCheckedAt, locale) }}
                </template>
                <template v-else>{{ t('health.neverChecked') }}</template>
              </p>
            </div>
            <RouterLink :to="{ name: 'health' }" class="btn-link">
              {{ t('dashboard.viewHealth') }}
            </RouterLink>
          </div>
          <div class="card-body stack-sm">
            <ErrorBanner
              v-if="healthError"
              :message="healthError"
              :title="t('errors.healthFailed')"
              :retry-label="t('common.retry')"
              :retrying="healthLoading"
              @retry="loadHealth"
            />
            <LoadingState
              v-else-if="healthLoading && !healthOverview"
              :label="t('health.loading')"
              compact
            />
            <div v-else-if="health" class="row row-wrap" style="gap: 5px">
              <span class="badge badge-outline">
                {{ t('dashboard.healthTotal', { count: health.total }) }}
              </span>
              <span class="badge badge-success">
                {{ t('dashboard.healthVerified', { count: health.verified }) }}
              </span>
              <span class="badge" :class="health.needsAttention > 0 ? 'badge-warn' : 'badge-neutral'">
                {{ t('dashboard.healthNeedsAttention', { count: health.needsAttention }) }}
              </span>
              <span class="badge badge-neutral">
                {{ t('dashboard.healthUnverified', { count: health.unverified }) }}
              </span>
              <span class="badge" :class="health.failed > 0 ? 'badge-danger' : 'badge-neutral'">
                {{ t('dashboard.healthFailed', { count: health.failed }) }}
              </span>
              <span class="badge badge-slate">
                {{ t('dashboard.healthNotDeployed', { count: health.notDeployed }) }}
              </span>
            </div>
            <p class="hint">{{ t('dashboard.healthHint') }}</p>
          </div>
        </div>

        <div v-if="changes" class="card">
          <div class="card-header">
            <p class="card-title">{{ t('dashboard.changesResult') }}</p>
            <RouterLink :to="{ name: 'activity' }" class="btn-link">
              {{ t('dashboard.viewAll') }}
            </RouterLink>
          </div>
          <div class="card-body stack-sm">
            <div class="row row-wrap">
              <span class="badge" :class="changes.drifted.length > 0 ? 'badge-warn' : 'badge-neutral'">
                {{ t('activity.drifted') }} {{ changes.drifted.length }}
              </span>
              <span class="badge" :class="changes.missing.length > 0 ? 'badge-danger' : 'badge-neutral'">
                {{ t('activity.missing') }} {{ changes.missing.length }}
              </span>
              <span class="badge" :class="changes.updates.length > 0 ? 'badge-info' : 'badge-neutral'">
                {{ t('activity.updates') }} {{ changes.updates.length }}
              </span>
            </div>
            <p
              v-if="
                changes.drifted.length === 0 &&
                changes.missing.length === 0 &&
                changes.updates.length === 0
              "
              class="hint"
            >
              {{ t('dashboard.changesNone') }}
            </p>
            <p v-for="(warning, index) in changes.warnings" :key="index" class="hint tone-warn">
              {{ warning }}
            </p>
          </div>
        </div>

        <div v-if="scanReport" class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('dashboard.scanResultTitle') }}</p>
              <p class="card-subtitle">
                {{ formatDateTime(scanReport.scannedAt, locale) }}
              </p>
            </div>
            <div class="row row-wrap">
              <span class="badge badge-info">
                {{ t('onboarding.scan.total') }} {{ scanReport.totalSkills }}
              </span>
              <span class="badge badge-neutral">
                {{ t('onboarding.scan.duplicates') }} {{ scanReport.duplicates }}
              </span>
              <span class="badge" :class="scanReport.conflicts > 0 ? 'badge-warn' : 'badge-neutral'">
                {{ t('onboarding.scan.conflicts') }} {{ scanReport.conflicts }}
              </span>
              <span class="badge" :class="scanReport.invalid > 0 ? 'badge-danger' : 'badge-neutral'">
                {{ t('onboarding.scan.invalid') }} {{ scanReport.invalid }}
              </span>
            </div>
          </div>
          <div class="card-body stack-sm">
            <p class="text-sm">
              {{
                t('dashboard.scanResultSummary', {
                  skills: scanReport.totalSkills,
                  duplicates: scanReport.duplicates,
                  conflicts: scanReport.conflicts,
                  invalid: scanReport.invalid,
                })
              }}
            </p>
            <div v-if="scanReport.agents.length === 0" class="hint">
              {{ t('dashboard.scanResultEmpty') }}
            </div>
            <div v-else class="row row-wrap" style="gap: 5px">
              <span v-for="agent in scanReport.agents" :key="agent.agentId" class="chip">
                {{ agent.agentName }}: {{ agent.skills.length }}
              </span>
            </div>
            <div v-if="scanReport.warnings.length > 0" class="stack-sm">
              <p class="hint tone-warn">
                {{ t('dashboard.scanResultWarnings', { count: scanReport.warnings.length }) }}
              </p>
              <p v-for="(warning, index) in scanReport.warnings" :key="index" class="hint">
                {{ warning }}
              </p>
            </div>
          </div>
        </div>

        <div class="grid-2">
          <div class="section-stack">
            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('dashboard.issuesTitle') }}</p>
                <span class="badge" :class="issues.length > 0 ? 'badge-warn' : 'badge-success'">
                  {{ issues.length }}
                </span>
              </div>
              <div class="card-body flush">
                <EmptyState
                  v-if="issues.length === 0"
                  icon="check-circle"
                  :title="t('dashboard.issuesEmpty')"
                />
                <ul v-else class="list-reset">
                  <li
                    v-for="(issue, index) in issues"
                    :key="`${issue.kind}-${index}`"
                    style="padding: 11px 14px"
                    :style="index > 0 ? { borderTop: '1px solid var(--ah-line)' } : undefined"
                  >
                    <div class="row" style="align-items: flex-start">
                      <StatusBadge
                        :label="severityLabel(issue)"
                        :tone="toneForSeverity(issue.severity)"
                        class="status-badge-inline"
                      />
                      <div style="flex: 1; min-width: 0">
                        <p class="text-sm" style="font-weight: 600">{{ issue.title }}</p>
                        <p class="hint">{{ issue.detail }}</p>
                        <p v-if="issue.agentKey" class="hint mono">{{ issue.agentKey }}</p>
                      </div>
                      <RouterLink
                        v-if="issueTarget(issue)"
                        :to="issueTarget(issue) ?? '/activity'"
                        class="btn btn-ghost btn-sm"
                      >
                        {{ t('dashboard.issueDetail') }}
                      </RouterLink>
                    </div>
                  </li>
                </ul>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('dashboard.updatesTitle') }}</p>
                <span class="badge" :class="updates.length > 0 ? 'badge-info' : 'badge-neutral'">
                  {{ updates.length }}
                </span>
              </div>
              <div class="card-body flush">
                <EmptyState
                  v-if="updates.length === 0"
                  icon="check-circle"
                  :title="t('dashboard.updatesEmpty')"
                />
                <ul v-else class="list-reset">
                  <li
                    v-for="(skill, index) in updates"
                    :key="skill.id"
                    style="padding: 9px 14px"
                    :style="index > 0 ? { borderTop: '1px solid var(--ah-line)' } : undefined"
                  >
                    <div class="row">
                      <div style="flex: 1; min-width: 0">
                        <div class="row" style="gap: 5px">
                          <span class="text-sm" style="font-weight: 600">{{ skill.name }}</span>
                          <span class="chip">{{ skill.version }}</span>
                          <StatusBadge :label="t('skills.updateAvailable')" tone="info" />
                        </div>
                        <p class="hint">
                          {{ t('skills.installedOn') }}：
                          {{ skill.installations.map((item) => item.agentName).join(' · ') || '—' }}
                        </p>
                      </div>
                      <RouterLink
                        :to="{ name: 'skill-detail', params: { id: skill.id } }"
                        class="btn btn-sm"
                      >
                        {{ t('dashboard.issueDetail') }}
                        <Icon name="chevron-right" :size="11" />
                      </RouterLink>
                    </div>
                  </li>
                </ul>
              </div>
            </div>
          </div>

          <div class="section-stack">
            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('dashboard.recentTitle') }}</p>
                <RouterLink :to="{ name: 'activity' }" class="btn-link">
                  {{ t('dashboard.viewAll') }}
                </RouterLink>
              </div>
              <div class="card-body flush">
                <EmptyState
                  v-if="recentOperations.length === 0"
                  icon="activity"
                  :title="t('dashboard.recentEmpty')"
                />
                <ul v-else class="list-reset">
                  <li
                    v-for="(operation, index) in recentOperations"
                    :key="operation.id"
                    class="row-click"
                    style="padding: 9px 14px"
                    :style="index > 0 ? { borderTop: '1px solid var(--ah-line)' } : undefined"
                    @click="openOperation(operation.id)"
                  >
                    <div style="flex: 1; min-width: 0">
                      <div class="row" style="gap: 5px">
                        <StatusBadge
                          :label="t(operationStatusKey[operation.status])"
                          :tone="toneForOperationStatus(operation.status)"
                        />
                        <span class="text-sm" style="font-weight: 600">
                          {{ t(operationKindKey[operation.kind]) }}
                        </span>
                      </div>
                      <p class="hint truncate" :title="operation.summary">{{ operation.summary }}</p>
                      <p class="hint">
                        {{ formatRelativeTime(operation.startedAt, locale) }} ·
                        {{ operation.itemCount }} {{ t('common.items') }}
                      </p>
                    </div>
                    <Icon name="chevron-right" :size="13" class="subtle" />
                  </li>
                </ul>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('dashboard.quickAgents') }}</p>
                <RouterLink :to="{ name: 'agents' }" class="btn-link">
                  {{ t('dashboard.openAgents') }}
                </RouterLink>
              </div>
              <div class="card-body flush">
                <EmptyState v-if="agents.length === 0" icon="agents" :title="t('dashboard.noAgents')">
                  <template #actions>
                    <RouterLink :to="{ name: 'agents' }" class="btn btn-sm">
                      {{ t('dashboard.openAgents') }}
                    </RouterLink>
                  </template>
                </EmptyState>
                <ul v-else class="list-reset">
                  <li
                    v-for="(agent, index) in agents"
                    :key="agent.id"
                    style="padding: 9px 14px"
                    :style="index > 0 ? { borderTop: '1px solid var(--ah-line)' } : undefined"
                  >
                    <div class="row">
                      <div style="flex: 1; min-width: 0">
                        <div class="row" style="gap: 5px">
                          <span class="text-sm" style="font-weight: 600">{{ agent.name }}</span>
                          <StatusBadge
                            :label="t(agentStatusKey[agent.status])"
                            :tone="toneForAgentStatus(agent.status)"
                          />
                        </div>
                        <p class="hint">
                          {{ agent.vendor }} · {{ t('agents.counts') }}:
                          {{ agent.skillCount }} / {{ agent.installedSkillCount }}
                        </p>
                      </div>
                      <RouterLink
                        :to="{ name: 'agents', query: { agent: agent.key } }"
                        class="btn btn-ghost btn-icon btn-sm"
                        :aria-label="t('agents.details')"
                      >
                        <Icon name="chevron-right" :size="13" />
                      </RouterLink>
                    </div>
                  </li>
                </ul>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>
