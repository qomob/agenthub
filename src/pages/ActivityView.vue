<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import BaseModal from '../components/BaseModal.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import OperationResultView from '../components/OperationResultView.vue'
import PageHeader from '../components/PageHeader.vue'
import PlanPreview from '../components/PlanPreview.vue'
import RecoveryReportView from '../components/RecoveryReportView.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { normalizeError } from '../lib/api'
import { formatBytes, formatDateTime, formatRelativeTime } from '../lib/format'
import {
  agentStatusKey,
  operationKindKey,
  operationStatusKey,
  toneForAgentStatus,
  toneForInstallationState,
  toneForOperationStatus,
  installationStateKey,
} from '../lib/status'
import { canRecoverOperation, hasManualRecovery } from '../lib/v31'
import type { OperationView, RecoveryReport } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useOperationsStore } from '../stores/operations'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'

type Tab = 'operations' | 'snapshots' | 'backups' | 'diagnostics'

const { t, locale } = useI18n()
const route = useRoute()
const store = useOperationsStore()
const app = useAppStore()
const skillsStore = useSkillsStore()
const ui = useUiStore()

const tab = ref<Tab>('operations')
const limit = ref(60)
const limitOptions = [20, 60, 120]

const detailId = ref<string | null>(null)
const selected = ref<OperationView | null>(null)
const detailLoading = ref(false)
const detailError = ref<string | null>(null)

const rollbackTarget = ref<OperationView | null>(null)
const rollingBack = ref(false)
const rollbackError = ref<string | null>(null)
const backupBusy = ref(false)

// v3.1 recovery: only ever started by an explicit user click.
const recoverTarget = ref<OperationView | null>(null)
const recovering = ref(false)
const recoverError = ref<string | null>(null)
const recoverReport = ref<RecoveryReport | null>(null)

const tabItems = computed<Array<{ key: Tab; label: string }>>(() => [
  { key: 'operations', label: t('activity.tabs.operations') },
  { key: 'snapshots', label: t('activity.tabs.snapshots') },
  { key: 'backups', label: t('activity.tabs.backups') },
  { key: 'diagnostics', label: t('activity.tabs.diagnostics') },
])

onMounted(async () => {
  await refreshOperations()
  const opId = typeof route.query.op === 'string' ? route.query.op : null
  if (opId) void openDetail(opId)
})

watch(limit, () => {
  void refreshOperations()
})

async function refreshOperations(): Promise<void> {
  try {
    await store.loadOperations(limit.value)
  } catch (err) {
    ui.notifyError(t('errors.operationsFailed'), normalizeError(err))
  }
}

async function switchTab(next: Tab): Promise<void> {
  tab.value = next
  if (next === 'snapshots' && store.snapshots.length === 0) {
    await store.loadSnapshots().catch((err) => {
      ui.notifyError(t('errors.loadFailed'), normalizeError(err))
    })
  }
  if (next === 'backups' && store.backups.length === 0) {
    await store.loadBackups().catch((err) => {
      ui.notifyError(t('errors.loadFailed'), normalizeError(err))
    })
  }
  if (next === 'diagnostics' && !store.doctor) {
    await store.loadDoctor().catch(() => undefined)
  }
}

async function openDetail(id: string): Promise<void> {
  detailId.value = id
  selected.value = null
  detailError.value = null
  detailLoading.value = true
  try {
    selected.value = await store.loadOperation(id)
  } catch (err) {
    detailError.value = normalizeError(err, t('errors.operationsFailed'))
  } finally {
    detailLoading.value = false
  }
}

function closeDetail(): void {
  detailId.value = null
  selected.value = null
  detailError.value = null
}

async function viewSnapshots(operationId: string): Promise<void> {
  closeDetail()
  tab.value = 'snapshots'
  try {
    await store.loadSnapshots(operationId)
  } catch (err) {
    ui.notifyError(t('errors.loadFailed'), normalizeError(err))
  }
}

async function confirmRollback(): Promise<void> {
  const operation = rollbackTarget.value
  if (!operation) return
  rollingBack.value = true
  rollbackError.value = null
  try {
    const result = await store.rollback(operation.id)
    ui.notify('success', t('activity.rollbackDone'), {
      detail: t(operationStatusKey[result.status]),
    })
    rollbackTarget.value = null
    await Promise.allSettled([
      skillsStore.refresh(),
      app.refreshAgents(),
      app.loadDashboard(true),
    ])
    if (detailId.value) await openDetail(detailId.value)
    if (tab.value === 'snapshots') await store.loadSnapshots().catch(() => undefined)
  } catch (err) {
    rollbackError.value = normalizeError(err, t('errors.rollbackFailed'))
    ui.notifyError(t('errors.rollbackFailed'), rollbackError.value)
  } finally {
    rollingBack.value = false
  }
}

async function confirmRecover(): Promise<void> {
  const operation = recoverTarget.value
  if (!operation) return
  recovering.value = true
  recoverError.value = null
  recoverReport.value = null
  try {
    const report = await store.recover(operation.id)
    recoverReport.value = report
    recoverTarget.value = null
    if (hasManualRecovery(report)) {
      ui.notify('warning', t('activity.recoverDone'), {
        detail: t('activity.recoverManualNotice'),
      })
    } else {
      ui.notify('success', t('activity.recoverDone'))
    }
    await Promise.allSettled([
      skillsStore.refresh(),
      app.refreshAgents(),
      app.loadDashboard(true),
    ])
    if (detailId.value) await openDetail(detailId.value)
    if (tab.value === 'snapshots') await store.loadSnapshots().catch(() => undefined)
  } catch (err) {
    recoverTarget.value = null
    recoverError.value = normalizeError(err, t('errors.recoverFailed'))
    ui.notifyError(t('errors.recoverFailed'), recoverError.value)
  } finally {
    recovering.value = false
  }
}

function closeRecoverReport(): void {
  recoverReport.value = null
}

function backToOperations(): void {
  recoverReport.value = null
  tab.value = 'operations'
}

async function refreshSnapshots(): Promise<void> {  try {
    await store.loadSnapshots()
  } catch (err) {
    ui.notifyError(t('errors.loadFailed'), normalizeError(err))
  }
}

async function refreshBackups(): Promise<void> {
  try {
    await store.loadBackups()
  } catch (err) {
    ui.notifyError(t('errors.loadFailed'), normalizeError(err))
  }
}

async function createBackup(): Promise<void> {
  backupBusy.value = true
  try {
    const result = await store.createBackup()
    ui.notify('success', t('activity.backupCreated', { path: result.path }))
    tab.value = 'backups'
  } catch (err) {
    ui.notifyError(t('errors.backupFailed'), normalizeError(err))
  } finally {
    backupBusy.value = false
  }
}

async function runDoctor(): Promise<void> {
  try {
    await store.loadDoctor()
  } catch (err) {
    ui.notifyError(t('errors.doctorFailed'), normalizeError(err))
  }
}

async function runChanges(): Promise<void> {
  try {
    await store.loadChanges()
  } catch (err) {
    ui.notifyError(t('errors.changesFailed'), normalizeError(err))
  }
}

function checkTone(status: string): 'success' | 'warn' | 'danger' | 'neutral' {
  if (status === 'ok') return 'success'
  if (status === 'warning') return 'warn'
  if (status === 'error') return 'danger'
  return 'neutral'
}

function doctorTone(ok: boolean): 'success' | 'warn' {
  return ok ? 'success' : 'warn'
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('activity.title')" :subtitle="t('activity.subtitle')">
      <template #actions>
        <button type="button" class="btn" :disabled="store.loading" @click="refreshOperations">
          <span v-if="store.loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('activity.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="backupBusy" @click="createBackup">
          <span v-if="backupBusy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="database" :size="13" />
          {{ backupBusy ? t('activity.backupCreating') : t('activity.backupCreate') }}
        </button>
      </template>
    </PageHeader>

    <div class="tabs" role="tablist">
      <button
        v-for="item in tabItems"
        :key="item.key"
        type="button"
        class="tab"
        :class="{ active: tab === item.key }"
        role="tab"
        :aria-selected="tab === item.key"
        @click="switchTab(item.key)"
      >
        {{ item.label }}
      </button>
    </div>

    <div class="section-stack">
      <ErrorBanner v-if="rollbackError" :message="rollbackError" :title="t('errors.rollbackFailed')" />
      <ErrorBanner v-if="recoverError" :message="recoverError" :title="t('errors.recoverFailed')" />

      <!-- Operations -->
      <template v-if="tab === 'operations'">
        <div class="toolbar">
          <span class="hint">{{ t('activity.limit') }}</span>
          <select v-model.number="limit" class="select" style="width: 90px">
            <option v-for="option in limitOptions" :key="option" :value="option">{{ option }}</option>
          </select>
          <span class="spacer" />
          <span class="hint">{{ store.operations.length }} {{ t('common.items') }}</span>
        </div>

        <ErrorBanner
          v-if="store.error"
          :message="store.error"
          :title="t('errors.operationsFailed')"
          :retry-label="t('common.retry')"
          :retrying="store.loading"
          @retry="refreshOperations"
        />

        <LoadingState v-if="store.loading && store.operations.length === 0" :label="t('common.loadingData')" />

        <div v-else-if="store.operations.length === 0" class="card">
          <EmptyState icon="activity" :title="t('activity.operationsEmpty')" />
        </div>

        <div v-else class="card">
          <div class="card-body flush">
            <div class="table-wrap" style="max-height: calc(100vh - 300px)">
              <table class="table table-compact">
                <thead>
                  <tr>
                    <th scope="col">{{ t('activity.columns.status') }}</th>
                    <th scope="col">{{ t('activity.columns.kind') }}</th>
                    <th scope="col">{{ t('activity.columns.summary') }}</th>
                    <th scope="col">{{ t('activity.columns.started') }}</th>
                    <th scope="col">{{ t('activity.columns.items') }}</th>
                    <th scope="col" style="text-align: right">{{ t('activity.columns.actions') }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="operation in store.operations"
                    :key="operation.id"
                    class="row-click"
                    @click="openDetail(operation.id)"
                  >
                    <td>
                      <StatusBadge
                        :label="t(operationStatusKey[operation.status])"
                        :tone="toneForOperationStatus(operation.status)"
                      />
                    </td>
                    <td>
                      <div>{{ t(operationKindKey[operation.kind]) }}</div>
                      <p v-if="operation.dryRun" class="hint">{{ t('plan.dryRun') }}</p>
                    </td>
                    <td>
                      <p class="truncate" style="max-width: 340px" :title="operation.summary">
                        {{ operation.summary }}
                      </p>
                      <p v-if="operation.error" class="hint tone-danger truncate" :title="operation.error">
                        {{ operation.error }}
                      </p>
                      <p v-if="operation.rollbackOf" class="hint mono">
                        {{ t('status.operationKind.rollback') }}: {{ operation.rollbackOf }}
                      </p>
                    </td>
                    <td class="text-xs">
                      {{ formatRelativeTime(operation.startedAt, locale) }}
                      <p class="hint">{{ formatDateTime(operation.startedAt, locale) }}</p>
                    </td>
                    <td class="num">{{ operation.itemCount }}</td>
                    <td style="text-align: right">
                      <div class="row" style="justify-content: flex-end">
                        <button
                          type="button"
                          class="btn btn-sm"
                          @click.stop="openDetail(operation.id)"
                        >
                          {{ t('activity.details') }}
                        </button>
                        <button
                          v-if="operation.canRollback"
                          type="button"
                          class="btn btn-sm"
                          @click.stop="rollbackTarget = operation"
                        >
                          <Icon name="rollback" :size="11" />
                          {{ t('activity.rollback') }}
                        </button>
                        <button
                          v-if="canRecoverOperation(operation.status)"
                          type="button"
                          class="btn btn-sm"
                          :title="t('activity.recoverHint')"
                          @click.stop="recoverTarget = operation"
                        >
                          <Icon name="shield" :size="11" />
                          {{ t('activity.recover') }}
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </template>

      <!-- Snapshots -->
      <template v-else-if="tab === 'snapshots'">
        <div class="toolbar">
          <button type="button" class="btn btn-sm" :disabled="store.snapshotsLoading" @click="refreshSnapshots">
            <span v-if="store.snapshotsLoading" class="spinner spinner-sm" aria-hidden="true" />
            <Icon v-else name="refresh" :size="12" />
            {{ t('common.refresh') }}
          </button>
          <span class="hint">{{ t('activity.snapshotsHint') }}</span>
        </div>

        <ErrorBanner
          v-if="store.snapshotsError"
          :message="store.snapshotsError"
          :title="t('errors.loadFailed')"
        />
        <LoadingState v-if="store.snapshotsLoading && store.snapshots.length === 0" :label="t('common.loadingData')" />
        <div v-else-if="store.snapshots.length === 0" class="card">
          <EmptyState icon="database" :title="t('activity.snapshotsEmpty')" />
        </div>
        <div v-else class="card">
          <div class="card-body flush">
            <div class="table-wrap" style="max-height: calc(100vh - 300px)">
              <table class="table table-compact">
                <thead>
                  <tr>
                    <th scope="col">{{ t('activity.snapshotOriginal') }}</th>
                    <th scope="col">{{ t('activity.snapshotBackup') }}</th>
                    <th scope="col">{{ t('common.type') }}</th>
                    <th scope="col">{{ t('activity.existed') }}</th>
                    <th scope="col">{{ t('activity.createdAt') }}</th>
                    <th scope="col">{{ t('activity.columns.actions') }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="snapshot in store.snapshots" :key="snapshot.id">
                    <td class="path-text">{{ snapshot.originalPath }}</td>
                    <td class="path-text">{{ snapshot.backupPath }}</td>
                    <td><span class="chip">{{ snapshot.kind }}</span></td>
                    <td>
                      <StatusBadge
                        :label="snapshot.existed ? t('common.yes') : t('common.no')"
                        :tone="snapshot.existed ? 'success' : 'neutral'"
                      />
                    </td>
                    <td class="text-xs">{{ formatDateTime(snapshot.createdAt, locale) }}</td>
                    <td>
                      <button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        @click="openDetail(snapshot.operationId)"
                      >
                        {{ t('activity.details') }}
                      </button>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </template>

      <!-- Backups -->
      <template v-else-if="tab === 'backups'">
        <div class="toolbar">
          <button type="button" class="btn btn-sm" :disabled="store.backupsLoading" @click="refreshBackups">
            <span v-if="store.backupsLoading" class="spinner spinner-sm" aria-hidden="true" />
            <Icon v-else name="refresh" :size="12" />
            {{ t('common.refresh') }}
          </button>
          <span class="hint">{{ t('activity.backupsHint') }}</span>
        </div>
        <ErrorBanner
          v-if="store.backupsError"
          :message="store.backupsError"
          :title="t('errors.loadFailed')"
        />
        <LoadingState v-if="store.backupsLoading && store.backups.length === 0" :label="t('common.loadingData')" />
        <div v-else-if="store.backups.length === 0" class="card">
          <EmptyState icon="database" :title="t('activity.backupsEmpty')">
            <template #actions>
              <button type="button" class="btn btn-primary" :disabled="backupBusy" @click="createBackup">
                <Icon name="database" :size="13" />
                {{ t('activity.backupCreate') }}
              </button>
            </template>
          </EmptyState>
        </div>
        <div v-else class="card">
          <div class="card-body flush">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('activity.backupPathLabel') }}</th>
                  <th scope="col" style="text-align: right">{{ t('activity.size') }}</th>
                  <th scope="col">{{ t('activity.createdAt') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="backup in store.backups" :key="backup.id">
                  <td class="path-text">{{ backup.path }}</td>
                  <td class="num" style="text-align: right">{{ formatBytes(backup.sizeBytes) }}</td>
                  <td class="text-xs">{{ formatDateTime(backup.createdAt, locale) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </template>

      <!-- Diagnostics -->
      <template v-else>
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('activity.tabs.diagnostics') }}</p>
              <p class="card-subtitle">{{ t('activity.changesTitle') }}</p>
            </div>
            <div class="row">
              <button type="button" class="btn btn-sm" :disabled="store.doctorLoading" @click="runDoctor">
                <span v-if="store.doctorLoading" class="spinner spinner-sm" aria-hidden="true" />
                <Icon v-else name="activity" :size="12" />
                {{ store.doctorLoading ? t('activity.doctorRunning') : t('activity.doctorRun') }}
              </button>
              <button type="button" class="btn btn-sm" :disabled="store.changesLoading" @click="runChanges">
                <span v-if="store.changesLoading" class="spinner spinner-sm" aria-hidden="true" />
                <Icon v-else name="refresh" :size="12" />
                {{ store.changesLoading ? t('activity.changesRunning') : t('activity.changesCheck') }}
              </button>
            </div>
          </div>
          <div class="card-body stack-md">
            <ErrorBanner v-if="store.doctorError" :message="store.doctorError" :title="t('errors.doctorFailed')" />
            <ErrorBanner v-if="store.changesError" :message="store.changesError" :title="t('errors.changesFailed')" />

            <LoadingState v-if="store.doctorLoading && !store.doctor" :label="t('activity.doctorRunning')" compact />

            <template v-else-if="store.doctor">
              <div class="notice" :class="store.doctor.ok ? 'notice-success' : 'notice-warn'">
                <Icon
                  class="notice-icon"
                  :name="store.doctor.ok ? 'check-circle' : 'alert-triangle'"
                  :size="14"
                />
                <div class="notice-body">
                  <p class="notice-title">
                    {{ store.doctor.ok ? t('activity.doctorOk') : t('activity.doctorIssues') }}
                  </p>
                  <p class="hint">
                    {{ t('activity.version') }} {{ store.doctor.version }} ·
                    {{ app.platform }} · {{ store.doctor.home }}
                  </p>
                </div>
              </div>

              <dl class="definition-grid">
                <dt>{{ t('activity.libraryRoot') }}</dt>
                <dd class="path-text">{{ store.doctor.libraryRoot }}</dd>
                <dt>{{ t('activity.database') }}</dt>
                <dd class="path-text">{{ store.doctor.databasePath }}</dd>
                <dt>{{ t('activity.home') }}</dt>
                <dd class="path-text">{{ store.doctor.home }}</dd>
              </dl>

              <div class="table-wrap surface-inset">
                <table class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('activity.checks') }}</th>
                      <th scope="col">{{ t('common.status') }}</th>
                      <th scope="col">{{ t('common.details') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="check in store.doctor.checks" :key="check.name">
                      <td><span class="chip">{{ check.name }}</span></td>
                      <td>
                        <StatusBadge :label="check.status" :tone="checkTone(check.status)" />
                      </td>
                      <td class="text-sm">
                        {{ check.detail }}
                        <p v-if="check.hint" class="hint">{{ check.hint }}</p>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <div class="table-wrap surface-inset">
                <table class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('activity.agents') }}</th>
                      <th scope="col">{{ t('common.status') }}</th>
                      <th scope="col">{{ t('activity.evidence') }}</th>
                      <th scope="col">{{ t('agents.skillRoots') }} / {{ t('agents.mcpTargets') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="agent in store.doctor.agents" :key="agent.key">
                      <td>{{ agent.name }}</td>
                      <td>
                        <StatusBadge
                          :label="t(agentStatusKey[agent.status])"
                          :tone="toneForAgentStatus(agent.status)"
                        />
                      </td>
                      <td class="text-xs">
                        <p v-for="(evidence, index) in agent.installEvidence" :key="index">
                          {{ evidence }}
                        </p>
                        <p v-for="(note, index) in agent.notes" :key="`n-${index}`" class="hint">
                          {{ note }}
                        </p>
                      </td>
                      <td class="text-xs">
                        <p v-for="root in agent.skillRoots" :key="root" class="path-text">S: {{ root }}</p>
                        <p v-for="target in agent.mcpTargets" :key="target" class="path-text">
                          M: {{ target }}
                        </p>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>
            <p v-else-if="!store.doctorLoading" class="hint">{{ t('activity.doctorEmpty') }}</p>

            <hr class="divider" />

            <div v-if="store.changeCheck" class="stack-md">
              <div class="row row-wrap">
                <span class="badge" :class="store.changeCheck.drifted.length > 0 ? 'badge-warn' : 'badge-neutral'">
                  {{ t('activity.drifted') }} {{ store.changeCheck.drifted.length }}
                </span>
                <span class="badge" :class="store.changeCheck.missing.length > 0 ? 'badge-danger' : 'badge-neutral'">
                  {{ t('activity.missing') }} {{ store.changeCheck.missing.length }}
                </span>
                <span class="badge" :class="store.changeCheck.updates.length > 0 ? 'badge-info' : 'badge-neutral'">
                  {{ t('activity.updates') }} {{ store.changeCheck.updates.length }}
                </span>
                <span class="hint">
                  {{ t('activity.changesSummary', { time: formatDateTime(store.changeCheck.checkedAt, locale) }) }}
                </span>
              </div>

              <p
                v-if="
                  store.changeCheck.drifted.length === 0 &&
                  store.changeCheck.missing.length === 0 &&
                  store.changeCheck.updates.length === 0
                "
                class="hint"
              >
                {{ t('activity.noChanges') }}
              </p>

              <div
                v-if="store.changeCheck.drifted.length + store.changeCheck.missing.length > 0"
                class="table-wrap surface-inset"
              >
                <table class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('plan.agent') }}</th>
                      <th scope="col">{{ t('common.status') }}</th>
                      <th scope="col">{{ t('skillDetail.targetPath') }}</th>
                      <th scope="col">{{ t('common.updatedAt') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="installation in [
                        ...store.changeCheck.drifted,
                        ...store.changeCheck.missing,
                      ]"
                      :key="installation.id"
                    >
                      <td>{{ installation.agentName }}</td>
                      <td>
                        <StatusBadge
                          :label="t(installationStateKey[installation.state])"
                          :tone="toneForInstallationState(installation.state)"
                        />
                        <StatusBadge
                          v-if="installation.drift"
                          :label="t('skills.drift')"
                          tone="warn"
                          style="margin-left: 4px"
                        />
                      </td>
                      <td class="path-text">{{ installation.targetPath }}</td>
                      <td class="text-xs">{{ formatRelativeTime(installation.installedAt, locale) }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <div v-if="store.changeCheck.updates.length > 0" class="stack-sm">
                <p class="section-title">{{ t('activity.updates') }}</p>
                <div class="row row-wrap" style="gap: 5px">
                  <span v-for="skill in store.changeCheck.updates" :key="skill.id" class="chip">
                    {{ skill.name }} {{ skill.version }}
                  </span>
                </div>
              </div>

              <div v-if="store.changeCheck.warnings.length > 0" class="notice notice-warn">
                <Icon class="notice-icon" name="alert-triangle" :size="14" />
                <div class="notice-body">
                  <p class="notice-title">{{ t('activity.changeWarnings') }}</p>
                  <p v-for="(warning, index) in store.changeCheck.warnings" :key="index">{{ warning }}</p>
                </div>
              </div>
            </div>
            <p v-else class="hint">{{ t('activity.changesCheck') }}</p>
          </div>
        </div>
      </template>
    </div>

    <BaseModal
      v-if="detailId"
      :title="t('activity.detailTitle')"
      :subtitle="selected?.summary"
      size="xl"
      @close="closeDetail"
    >
      <LoadingState v-if="detailLoading" :label="t('activity.detailLoading')" />
      <ErrorBanner v-else-if="detailError" :message="detailError" :title="t('errors.operationsFailed')" />
      <div v-else-if="selected" class="stack-md">
        <div class="row row-wrap">
          <StatusBadge
            :label="t(operationStatusKey[selected.status])"
            :tone="toneForOperationStatus(selected.status)"
          />
          <span class="badge badge-outline">{{ t(operationKindKey[selected.kind]) }}</span>
          <span class="chip">{{ selected.actor }}</span>
          <span class="hint mono">{{ selected.id }}</span>
          <span class="spacer" />
          <span class="hint">
            {{ formatDateTime(selected.startedAt, locale) }}
            <template v-if="selected.finishedAt"> → {{ formatDateTime(selected.finishedAt, locale) }}</template>
          </span>
        </div>

        <div v-if="selected.error" class="notice notice-danger">
          <Icon class="notice-icon" name="alert-circle" :size="14" />
          <div class="notice-body">
            <p class="notice-title">{{ t('activity.errorTitle') }}</p>
            <p class="mono" style="white-space: pre-wrap">{{ selected.error }}</p>
          </div>
        </div>

        <div
          v-if="
            selected.status === 'needs_manual_recovery' ||
            selected.recoveryState === 'needs_manual_recovery'
          "
          class="notice notice-warn"
          role="alert"
        >
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <div class="notice-body">
            <p class="notice-title">{{ t('activity.recoverManualNotice') }}</p>
            <p class="hint">{{ t('activity.recoverManualHint') }}</p>
          </div>
        </div>

        <section>
          <p class="section-title">{{ t('activity.plan') }}</p>
          <PlanPreview v-if="selected.plan" :plan="selected.plan" />
          <p v-else class="hint">{{ t('activity.noPlan') }}</p>
        </section>

        <section>
          <p class="section-title">{{ t('activity.result') }}</p>
          <OperationResultView
            v-if="selected.result"
            :results="[selected.result]"
          />
          <p v-else class="hint">{{ t('activity.noResult') }}</p>
        </section>

        <p v-if="!selected.canRollback" class="hint">{{ t('activity.rollbackHint') }}</p>
      </div>
      <template #footer>
        <button type="button" class="btn" @click="closeDetail">{{ t('common.close') }}</button>
        <button
          v-if="selected"
          type="button"
          class="btn"
          @click="viewSnapshots(selected.id)"
        >
          <Icon name="database" :size="13" />
          {{ t('activity.viewSnapshots') }}
        </button>
        <button
          v-if="selected?.canRollback"
          type="button"
          class="btn btn-primary"
          @click="rollbackTarget = selected"
        >
          <Icon name="rollback" :size="13" />
          {{ t('activity.rollback') }}
        </button>
        <button
          v-if="selected && canRecoverOperation(selected.status)"
          type="button"
          class="btn"
          :title="t('activity.recoverHint')"
          @click="recoverTarget = selected"
        >
          <Icon name="shield" :size="13" />
          {{ t('activity.recover') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="rollbackTarget"
      :title="t('activity.rollbackTitle')"
      :message="t('activity.rollbackBody', { id: rollbackTarget.id })"
      :confirm-label="t('activity.rollbackConfirm')"
      danger
      :busy="rollingBack"
      @cancel="rollbackTarget = null"
      @confirm="confirmRollback"
    />

    <ConfirmDialog
      v-if="recoverTarget"
      :title="t('activity.recoverTitle')"
      :message="t('activity.recoverBody', { id: recoverTarget.id })"
      :confirm-label="t('activity.recoverConfirm')"
      :busy="recovering"
      @cancel="recoverTarget = null"
      @confirm="confirmRecover"
    />

    <BaseModal
      v-if="recoverReport"
      :title="t('activity.recoverResultTitle')"
      :subtitle="t('activity.recoverHint')"
      size="lg"
      @close="closeRecoverReport"
    >
      <RecoveryReportView :report="recoverReport" />
      <template #footer>
        <button type="button" class="btn" @click="closeRecoverReport">
          {{ t('common.close') }}
        </button>
        <button type="button" class="btn" @click="backToOperations">
          <Icon name="activity" :size="13" />
          {{ t('activity.tabs.operations') }}
        </button>
      </template>
    </BaseModal>
  </div>
</template>
