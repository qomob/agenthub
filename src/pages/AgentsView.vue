<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink, useRoute, useRouter } from 'vue-router'

import BaseModal from '../components/BaseModal.vue'
import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { chooseDirectory } from '../lib/dialogs'
import { isReadOnlyRootSource } from '../lib/discovered'
import { formatDateTime, formatRelativeTime } from '../lib/format'
import {
  agentStatusKey,
  pluginSupportKey,
  scopeKey,
  symlinkSupportKey,
  toneForAgentStatus,
  toneForPluginSupport,
  toneForSymlinkSupport,
  transportKey,
} from '../lib/status'
import type {
  AgentDoctorCheck,
  AgentHealth,
  AgentHealthReport,
  AgentView,
  DoctorReport,
} from '../lib/types'
import { useAppStore } from '../stores/app'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const app = useAppStore()
const ui = useUiStore()
const route = useRoute()
const router = useRouter()

const agents = computed(() => app.agents)
const refreshing = computed(() => app.refreshingAgents)
const loading = ref(false)
const error = ref<string | null>(null)

const activeAgent = ref<AgentView | null>(null)
const form = ref({ appPath: '', homePath: '', customRoots: [] as string[] })
const saving = ref(false)
const saveError = ref<string | null>(null)

const doctor = ref<DoctorReport | null>(null)
const doctorLoading = ref(false)
const doctorError = ref<string | null>(null)

const healthReport = ref<AgentHealthReport | null>(null)
const healthLoading = ref(false)
const healthError = ref<string | null>(null)
const healthByKey = computed(
  () => new Map((healthReport.value?.agents ?? []).map((item) => [item.agentKey, item])),
)

const doctorCheck = computed<AgentDoctorCheck | null>(() => {
  if (!doctor.value || !activeAgent.value) return null
  return doctor.value.agents.find((item) => item.key === activeAgent.value?.key) ?? null
})

onMounted(async () => {
  await reloadAgents()
  void loadAgentHealth(false)
  const key = typeof route.query.agent === 'string' ? route.query.agent : null
  if (key) {
    const agent = app.agents.find((item) => item.key === key)
    if (agent) void openAgent(agent)
  }
})

watch(
  () => route.query.agent,
  (value) => {
    if (typeof value !== 'string') return
    if (activeAgent.value?.key === value) return
    const agent = app.agents.find((item) => item.key === value)
    if (agent) void openAgent(agent)
  },
)

async function reloadAgents(): Promise<void> {
  loading.value = true
  error.value = null
  try {
    await app.refreshAgents()
  } catch (err) {
    error.value = normalizeError(err, t('errors.agentsFailed'))
  } finally {
    loading.value = false
  }
}

/** Read (record=false) or record (record=true) the detected agent versions/paths. */
async function loadAgentHealth(record: boolean): Promise<void> {
  healthLoading.value = true
  healthError.value = null
  try {
    healthReport.value = await api.agentHealth(record)
    if (record) {
      const changed = healthReport.value.agents.filter((item) => item.versionChanged).length
      ui.notify('success', t('agents.recheckVersionDone'), {
        detail:
          changed > 0
            ? t('agents.versionChangedCount', { count: changed })
            : t('agents.recheckVersionNoChange'),
      })
    }
  } catch (err) {
    healthError.value = normalizeError(err, t('errors.agentHealthFailed'))
  } finally {
    healthLoading.value = false
  }
}

function agentHealthFor(agent: AgentView): AgentHealth | null {
  return healthByKey.value.get(agent.key) ?? null
}

function agentVersionLabel(agent: AgentView): string {
  const entry = agentHealthFor(agent)
  if (!entry) return t('agents.versionNotDetected')
  return entry.version || t('agents.versionNotDetected')
}

function agentPathsLabel(agent: AgentView): string {
  const entry = agentHealthFor(agent)
  if (!entry) return t('agents.pathsUnknown')
  return entry.pathsOk ? t('agents.pathsOk') : t('agents.pathsNotOk')
}

function agentVersionChanged(agent: AgentView): boolean {
  return agentHealthFor(agent)?.versionChanged ?? false
}

function syncForm(agent: AgentView): void {
  form.value = {
    appPath: agent.appPath ?? '',
    homePath: agent.homePath ?? '',
    customRoots: [...agent.customSkillRoots],
  }
}

function syncActiveById(id: string): void {
  const found = app.agents.find((item) => item.id === id)
  if (found) {
    activeAgent.value = found
    syncForm(found)
  }
}

async function loadEvidence(): Promise<void> {
  doctorLoading.value = true
  doctorError.value = null
  try {
    doctor.value = await api.doctor()
  } catch (err) {
    doctorError.value = normalizeError(err, t('agents.evidenceFailed'))
  } finally {
    doctorLoading.value = false
  }
}

async function openAgent(agent: AgentView): Promise<void> {
  activeAgent.value = agent
  saveError.value = null
  syncForm(agent)
  await loadEvidence()
}

function closeDetails(): void {
  activeAgent.value = null
  if (route.query.agent) void router.replace({ name: 'agents' })
}

async function addRoot(): Promise<void> {
  try {
    const selected = await chooseDirectory(activeAgent.value?.homePath ?? undefined)
    if (!selected) return
    if (form.value.customRoots.includes(selected)) return
    form.value.customRoots = [...form.value.customRoots, selected]
  } catch (err) {
    ui.notifyError(t('errors.dialogFailed'), normalizeError(err))
  }
}

function removeRoot(index: number): void {
  form.value.customRoots = form.value.customRoots.filter((_, i) => i !== index)
}

async function savePaths(): Promise<void> {
  const agent = activeAgent.value
  if (!agent) return
  saving.value = true
  saveError.value = null
  try {
    const updated = await api.updateAgentPaths({
      agentId: agent.id,
      appPath: form.value.appPath.trim() || null,
      homePath: form.value.homePath.trim() || null,
      customSkillRoots: form.value.customRoots,
    })
    activeAgent.value = updated
    syncForm(updated)
    await app.refreshAgents()
    syncActiveById(agent.id)
    ui.notify('success', t('agents.savedPaths'))
  } catch (err) {
    saveError.value = normalizeError(err, t('errors.saveFailed'))
  } finally {
    saving.value = false
  }
}

async function scanAll(): Promise<void> {
  try {
    const report = await app.scan()
    if (activeAgent.value) syncActiveById(activeAgent.value.id)
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

function initials(name: string): string {
  return name.slice(0, 2).toUpperCase()
}

function rootSummary(agent: AgentView): string {
  const existing = agent.skillRoots.filter((root) => root.exists).length
  return `${existing}/${agent.skillRoots.length}`
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('agents.title')" :subtitle="t('agents.subtitle')">
      <template #actions>
        <RouterLink :to="{ name: 'skills', query: { tab: 'discovered' } }" class="btn btn-ghost">
          <Icon name="scan" :size="13" />
          {{ t('agents.viewDiscovered') }}
        </RouterLink>
        <button type="button" class="btn" :disabled="healthLoading" @click="loadAgentHealth(true)">
          <span v-if="healthLoading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="shield" :size="13" />
          {{ t('agents.recheckVersion') }}
        </button>
        <button type="button" class="btn" :disabled="refreshing" @click="reloadAgents">
          <span v-if="refreshing" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('agents.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="app.scanning" @click="scanAll">
          <span v-if="app.scanning" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="scan" :size="13" />
          {{ app.scanning ? t('dashboard.scanning') : t('agents.scanAll') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.agentsFailed')"
        :retry-label="t('common.retry')"
        @retry="reloadAgents"
      />

      <div class="notice notice-info">
        <Icon class="notice-icon" name="info" :size="14" />
        <div class="notice-body">
          <p class="notice-title">{{ t('agents.infoTitle') }}</p>
          <p>{{ t('agents.infoBody') }}</p>
        </div>
      </div>

      <LoadingState v-if="loading && agents.length === 0" :label="t('common.loadingData')" />

      <EmptyState
        v-else-if="agents.length === 0"
        icon="agents"
        :title="t('agents.emptyTitle')"
        :body="t('agents.emptyBody')"
      >
        <template #actions>
          <button type="button" class="btn" :disabled="refreshing" @click="reloadAgents">
            <Icon name="refresh" :size="13" />
            {{ t('agents.refresh') }}
          </button>
          <button type="button" class="btn btn-primary" :disabled="app.scanning" @click="scanAll">
            <Icon name="scan" :size="13" />
            {{ t('agents.scanAll') }}
          </button>
        </template>
      </EmptyState>

      <div v-else class="grid-cards">
        <article v-for="agent in agents" :key="agent.id" class="card">
          <div class="card-body stack-md">
            <div class="agent-card-header">
              <div class="row">
                <div class="agent-avatar" aria-hidden="true">{{ initials(agent.name) }}</div>
                <div style="min-width: 0">
                  <p style="font-weight: 600">{{ agent.name }}</p>
                  <p class="hint">{{ agent.vendor }} · {{ agent.adapterId }}</p>
                </div>
              </div>
              <StatusBadge
                :label="t(agentStatusKey[agent.status])"
                :tone="toneForAgentStatus(agent.status)"
              />
            </div>

            <div class="row row-wrap" style="gap: 5px">
              <span class="badge badge-neutral">
                {{ t('agents.skillCount', { count: agent.skillCount }) }}
              </span>
              <span class="badge badge-accent">
                {{ t('skills.installCount', { count: agent.installedSkillCount }) }}
              </span>
              <span class="badge badge-outline">
                {{ t('agents.skillRoots') }}: {{ rootSummary(agent) }}
              </span>
            </div>

            <div class="stack-sm">
              <div v-if="agentHealthFor(agent)" class="row row-wrap" style="gap: 5px">
                <StatusBadge
                  :label="`${t('agents.version')}：${agentVersionLabel(agent)}`"
                  :tone="agentHealthFor(agent)?.version ? 'success' : 'neutral'"
                  :icon="agentHealthFor(agent)?.version ? 'check-circle' : 'alert-circle'"
                />
                <StatusBadge
                  :label="agentPathsLabel(agent)"
                  :tone="agentHealthFor(agent)?.pathsOk ? 'success' : 'warn'"
                  :icon="agentHealthFor(agent)?.pathsOk ? 'check-circle' : 'alert-triangle'"
                />
                <StatusBadge
                  v-if="agentVersionChanged(agent)"
                  :label="t('agents.versionChanged')"
                  tone="warn"
                  icon="alert-triangle"
                />
              </div>
              <p v-else-if="healthLoading" class="hint">{{ t('agents.versionChecking') }}</p>
              <p v-else class="hint">
                {{ healthError ? t('agents.versionUnknown') : t('agents.versionNotDetected') }}
              </p>
              <p v-if="agentVersionChanged(agent)" class="hint tone-warn">
                {{
                  t('agents.versionChangedWarning', {
                    previous: agentHealthFor(agent)?.previousVersion ?? '—',
                    version: agentVersionLabel(agent),
                  })
                }}
              </p>
              <p
                v-if="agentHealthFor(agent) && !agentHealthFor(agent)?.pathsOk"
                class="hint tone-warn"
              >
                {{ t('agents.pathsNotOkHint') }}
              </p>
            </div>

            <div class="stack-sm">
              <div v-if="agent.appPath" class="row" style="gap: 5px">
                <span class="hint" style="flex: none">appPath</span>
                <span class="path-text truncate" :title="agent.appPath">{{ agent.appPath }}</span>
              </div>
              <div v-if="agent.homePath" class="row" style="gap: 5px">
                <span class="hint" style="flex: none">homePath</span>
                <span class="path-text truncate" :title="agent.homePath">{{ agent.homePath }}</span>
              </div>
              <p class="hint">
                {{ t('agents.lastScan') }}：
                {{
                  agent.lastScanAt
                    ? formatRelativeTime(agent.lastScanAt, locale)
                    : t('common.never')
                }}
                <span v-if="agent.lastScanAt" class="subtle">
                  ({{ formatDateTime(agent.lastScanAt, locale) }})
                </span>
              </p>
            </div>

            <div class="row row-wrap">
              <button type="button" class="btn btn-sm" @click="openAgent(agent)">
                <Icon name="eye" :size="12" />
                {{ t('agents.details') }}
              </button>
              <RouterLink
                :to="{ name: 'skills', query: { agent: agent.key } }"
                class="btn btn-sm btn-ghost"
              >
                {{ t('agents.openSkills') }}
              </RouterLink>
              <RouterLink
                :to="{ name: 'matrix', query: { agent: agent.key } }"
                class="btn btn-sm btn-ghost"
              >
                {{ t('agents.openMatrix') }}
              </RouterLink>
            </div>
          </div>
        </article>
      </div>
    </div>

    <BaseModal
      v-if="activeAgent"
      drawer
      :title="activeAgent.name"
      :subtitle="`${activeAgent.vendor} · ${activeAgent.adapterId}`"
      @close="closeDetails"
    >
      <div class="stack-md">
        <div class="row row-wrap">
          <StatusBadge
            :label="t(agentStatusKey[activeAgent.status])"
            :tone="toneForAgentStatus(activeAgent.status)"
          />
          <StatusBadge
            :label="t(symlinkSupportKey[activeAgent.symlinkSupport])"
            :tone="toneForSymlinkSupport(activeAgent.symlinkSupport)"
          />
          <StatusBadge
            :label="t(pluginSupportKey[activeAgent.pluginSupport])"
            :tone="toneForPluginSupport(activeAgent.pluginSupport)"
          />
          <span class="spacer" />
          <RouterLink
            :to="{ name: 'skills', query: { agent: activeAgent.key } }"
            class="btn btn-sm btn-ghost"
          >
            {{ t('agents.openSkills') }}
          </RouterLink>
          <RouterLink
            :to="{ name: 'matrix', query: { agent: activeAgent.key } }"
            class="btn btn-sm btn-ghost"
          >
            {{ t('agents.openMatrix') }}
          </RouterLink>
        </div>

        <section>
          <p class="section-title">{{ t('agents.versionHealth') }}</p>
          <LoadingState v-if="healthLoading" :label="t('agents.versionChecking')" compact />
          <ErrorBanner
            v-else-if="healthError"
            :message="healthError"
            :title="t('errors.agentHealthFailed')"
            :retry-label="t('common.retry')"
            @retry="loadAgentHealth(false)"
          />
          <div v-else class="stack-sm">
            <div class="row row-wrap" style="gap: 5px">
              <StatusBadge
                :label="`${t('agents.version')}：${agentVersionLabel(activeAgent)}`"
                :tone="agentHealthFor(activeAgent)?.version ? 'success' : 'neutral'"
                :icon="agentHealthFor(activeAgent)?.version ? 'check-circle' : 'alert-circle'"
              />
              <StatusBadge
                :label="agentPathsLabel(activeAgent)"
                :tone="agentHealthFor(activeAgent)?.pathsOk ? 'success' : 'warn'"
                :icon="agentHealthFor(activeAgent)?.pathsOk ? 'check-circle' : 'alert-triangle'"
              />
              <StatusBadge
                v-if="agentVersionChanged(activeAgent)"
                :label="t('agents.versionChanged')"
                tone="warn"
                icon="alert-triangle"
              />
            </div>
            <p v-if="agentHealthFor(activeAgent)?.checkedAt" class="hint">
              {{ t('agents.checkedAt') }}：{{ formatDateTime(agentHealthFor(activeAgent)?.checkedAt ?? null, locale) }}
            </p>
            <p v-if="agentVersionChanged(activeAgent)" class="hint tone-warn">
              {{
                t('agents.versionChangedWarning', {
                  previous: agentHealthFor(activeAgent)?.previousVersion ?? '—',
                  version: agentVersionLabel(activeAgent),
                })
              }}
            </p>
            <p v-if="agentHealthFor(activeAgent)?.previousVersion && !agentVersionChanged(activeAgent)" class="hint">
              {{ t('agents.previousVersion') }}：{{ agentHealthFor(activeAgent)?.previousVersion }}
            </p>
            <ul
              v-if="(agentHealthFor(activeAgent)?.notes.length ?? 0) > 0"
              class="list-reset stack-sm"
            >
              <li
                v-for="(note, index) in agentHealthFor(activeAgent)?.notes ?? []"
                :key="index"
                class="hint"
              >
                • {{ note }}
              </li>
            </ul>
            <p v-if="!agentHealthFor(activeAgent)" class="hint">{{ t('agents.versionNotDetected') }}</p>
            <button
              type="button"
              class="btn btn-sm"
              :disabled="healthLoading"
              @click="loadAgentHealth(true)"
            >
              <span v-if="healthLoading" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="shield" :size="12" />
              {{ t('agents.recheckVersion') }}
            </button>
          </div>
        </section>

        <hr class="divider" />

        <section>
          <p class="section-title">{{ t('agents.editPaths') }}</p>
          <div class="stack-md">
            <div class="field">
              <label class="label" for="agent-app-path">{{ t('agents.appPath') }}</label>
              <input
                id="agent-app-path"
                v-model="form.appPath"
                class="input input-mono"
                type="text"
                spellcheck="false"
                :placeholder="activeAgent.appPath ?? ''"
              />
            </div>
            <div class="field">
              <label class="label" for="agent-home-path">{{ t('agents.homePath') }}</label>
              <input
                id="agent-home-path"
                v-model="form.homePath"
                class="input input-mono"
                type="text"
                spellcheck="false"
                :placeholder="activeAgent.homePath ?? ''"
              />
            </div>

            <div class="field">
              <div class="row" style="justify-content: space-between">
                <span class="label">{{ t('agents.customRoots') }}</span>
                <button type="button" class="btn btn-sm" @click="addRoot">
                  <Icon name="folder" :size="12" />
                  {{ t('agents.addRoot') }}
                </button>
              </div>
              <p class="hint">{{ t('agents.customRootsHint') }}</p>
              <div v-if="form.customRoots.length === 0" class="hint">—</div>
              <div v-else class="stack-sm">
                <div
                  v-for="(root, index) in form.customRoots"
                  :key="`${root}-${index}`"
                  class="row surface-inset"
                  style="padding: 5px 8px"
                >
                  <span class="path-text" style="flex: 1">{{ root }}</span>
                  <button
                    type="button"
                    class="btn btn-ghost btn-sm"
                    :aria-label="t('agents.removeRoot')"
                    @click="removeRoot(index)"
                  >
                    <Icon name="trash" :size="12" />
                  </button>
                </div>
              </div>
            </div>

            <p class="hint">{{ t('agents.pathHint') }}</p>
            <ErrorBanner v-if="saveError" :message="saveError" :title="t('errors.saveFailed')" />
          </div>
        </section>

        <hr class="divider" />

        <section>
          <p class="section-title">{{ t('agents.installEvidence') }}</p>
          <LoadingState v-if="doctorLoading" :label="t('agents.evidenceLoading')" compact />
          <ErrorBanner
            v-else-if="doctorError"
            :message="doctorError"
            :title="t('agents.evidenceFailed')"
            :retry-label="t('common.retry')"
            @retry="loadEvidence"
          />
          <div v-else class="stack-sm">
            <ul v-if="doctorCheck && doctorCheck.installEvidence.length > 0" class="list-reset stack-sm">
              <li
                v-for="(evidence, index) in doctorCheck.installEvidence"
                :key="index"
                class="path-text"
              >
                ✓ {{ evidence }}
              </li>
            </ul>
            <p v-else class="hint">{{ t('agents.evidenceEmpty') }}</p>
            <p v-if="doctorCheck && doctorCheck.notes.length > 0" class="hint">
              {{ doctorCheck.notes.join(' ') }}
            </p>
          </div>
        </section>

        <hr class="divider" />

        <section>
          <p class="section-title">{{ t('agents.skillRoots') }}</p>
          <p class="hint" style="margin-bottom: 7px">{{ t('agents.readOnlyRootHint') }}</p>
          <div class="table-wrap surface-inset">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('common.path') }}</th>
                  <th scope="col">{{ t('agents.source') }}</th>
                  <th scope="col">{{ t('agents.scope') }}</th>
                  <th scope="col">{{ t('common.status') }}</th>
                  <th scope="col">{{ t('common.count') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="root in activeAgent.skillRoots" :key="root.path">
                  <td class="path-text">{{ root.path }}</td>
                  <td>
                    <div class="row row-wrap" style="gap: 3px">
                      <span class="chip">{{ root.source }}</span>
                      <StatusBadge
                        v-if="isReadOnlyRootSource(root.source)"
                        :label="t('agents.readOnlySource')"
                        tone="neutral"
                        :dot="false"
                      />
                    </div>
                  </td>
                  <td><span class="badge badge-outline">{{ t(scopeKey[root.scope]) }}</span></td>
                  <td>
                    <div class="row row-wrap" style="gap: 4px">
                      <StatusBadge
                        :label="root.exists ? t('agents.rootExists') : t('agents.rootMissing')"
                        :tone="root.exists ? 'success' : 'danger'"
                      />
                      <StatusBadge
                        v-if="root.exists"
                        :label="root.writable ? t('agents.writable') : t('agents.readOnly')"
                        :tone="root.writable ? 'success' : 'warn'"
                      />
                    </div>
                  </td>
                  <td class="num">{{ root.skillCount }}</td>
                </tr>
                <tr v-if="activeAgent.skillRoots.length === 0">
                  <td colspan="5" class="hint">—</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section>
          <p class="section-title">{{ t('agents.mcpTargets') }}</p>
          <div class="table-wrap surface-inset">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('common.path') }}</th>
                  <th scope="col">{{ t('agents.format') }}</th>
                  <th scope="col">{{ t('agents.transport') }}</th>
                  <th scope="col">{{ t('common.status') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="target in activeAgent.mcpTargets" :key="target.path">
                  <td>
                    <p class="path-text">{{ target.path }}</p>
                    <p v-for="(note, index) in target.notes" :key="index" class="hint">
                      {{ note }}
                    </p>
                  </td>
                  <td><span class="chip">{{ target.format }}</span></td>
                  <td>
                    <div class="row row-wrap" style="gap: 3px">
                      <span v-for="transport in target.transports" :key="transport" class="chip">
                        {{ t(transportKey[transport]) }}
                      </span>
                    </div>
                  </td>
                  <td>
                    <div class="row row-wrap" style="gap: 4px">
                      <StatusBadge
                        :label="target.exists ? t('agents.rootExists') : t('agents.rootMissing')"
                        :tone="target.exists ? 'success' : 'neutral'"
                      />
                      <StatusBadge
                        v-if="target.exists"
                        :label="target.writable ? t('agents.writable') : t('agents.readOnly')"
                        :tone="target.writable ? 'success' : 'warn'"
                      />
                    </div>
                  </td>
                </tr>
                <tr v-if="activeAgent.mcpTargets.length === 0">
                  <td colspan="4" class="hint">—</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section>
          <p class="section-title">{{ t('agents.notes') }}</p>
          <ul v-if="activeAgent.notes.length > 0" class="list-reset stack-sm">
            <li v-for="(note, index) in activeAgent.notes" :key="index" class="hint">• {{ note }}</li>
          </ul>
          <p v-else class="hint">{{ t('agents.noNotes') }}</p>
        </section>
      </div>

      <template #footer>
        <button type="button" class="btn" @click="closeDetails">{{ t('common.close') }}</button>
        <button type="button" class="btn" :disabled="app.scanning" @click="scanAll">
          <span v-if="app.scanning" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="scan" :size="13" />
          {{ t('agents.rescanAgent') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="saving" @click="savePaths">
          <span v-if="saving" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="check" :size="13" />
          {{ saving ? t('agents.savingPaths') : t('agents.savePaths') }}
        </button>
      </template>
    </BaseModal>
  </div>
</template>
