<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink, useRoute } from 'vue-router'

import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import PlanDialog from '../components/PlanDialog.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { formatRelativeTime } from '../lib/format'
import { matrixCellState } from '../lib/status'
import type { AgentView, InstallStrategy, OperationPlan, Skill } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'

type Mode = 'install' | 'uninstall'

const { t, locale } = useI18n()
const route = useRoute()
const app = useAppStore()
const store = useSkillsStore()
const ui = useUiStore()

const mode = ref<Mode>('install')
const strategy = ref<'default' | InstallStrategy>('default')
const selected = ref<Set<string>>(new Set())
const plans = ref<OperationPlan[] | null>(null)
const planning = ref(false)
const planError = ref<string | null>(null)

const skills = computed(() => store.skills)
const agents = computed(() => app.agents)
const loading = computed(
  () =>
    (store.loading && store.skills.length === 0) ||
    (app.refreshingAgents && app.agents.length === 0),
)
const selectedCount = computed(() => selected.value.size)
const selectedLabel = computed(() => t('matrix.selectedCount', { count: selectedCount.value }))
const highlightAgentKey = computed(() =>
  typeof route.query.agent === 'string' ? route.query.agent : '',
)

onMounted(async () => {
  const results = await Promise.allSettled([store.load(true), app.refreshAgents()])
  const rejected = results.find((result) => result.status === 'rejected')
  if (rejected && rejected.status === 'rejected') {
    ui.notifyError(t('errors.loadFailed'), normalizeError(rejected.reason))
  }
})

function cellKey(skill: Skill, agent: AgentView): string {
  return `${skill.id}::${agent.id}`
}

function cellOf(skill: Skill, agent: AgentView) {
  return matrixCellState(skill, agent)
}

function isSelected(skill: Skill, agent: AgentView): boolean {
  return selected.value.has(cellKey(skill, agent))
}

function isSelectable(skill: Skill, agent: AgentView): boolean {
  const cell = matrixCellState(skill, agent)
  return mode.value === 'install' ? cell.selectableForInstall : cell.selectableForUninstall
}

function toggleCell(skill: Skill, agent: AgentView): void {
  if (!isSelectable(skill, agent)) return
  const next = new Set(selected.value)
  const key = cellKey(skill, agent)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  selected.value = next
}

function clearSelection(): void {
  selected.value = new Set()
}

function cellTitle(skill: Skill, agent: AgentView): string {
  const cell = matrixCellState(skill, agent)
  if (cell.installationId) return cell.path ?? t(cell.labelKey)
  return `${t(cell.labelKey)} · ${agent.name}`
}

interface SelectedPair {
  skill: Skill
  agent: AgentView
}

function selectedPairs(): SelectedPair[] {
  const pairs: SelectedPair[] = []
  for (const key of selected.value) {
    const [skillId, agentId] = key.split('::')
    const skill = skills.value.find((item) => item.id === skillId)
    const agent = agents.value.find((item) => item.id === agentId)
    if (skill && agent) pairs.push({ skill, agent })
  }
  return pairs
}

async function generatePlans(): Promise<void> {
  planError.value = null
  const pairs = selectedPairs()
  if (pairs.length === 0) {
    planError.value = t('matrix.nothingSelected')
    return
  }
  planning.value = true
  try {
    if (mode.value === 'uninstall') {
      const installationIds = pairs
        .map(({ skill, agent }) => matrixCellState(skill, agent).installationId)
        .filter((id): id is string => Boolean(id))
      if (installationIds.length === 0) {
        planError.value = t('matrix.noInstallationsSelected')
        return
      }
      const plan = await api.planUninstall({ installationIds, force: false })
      plans.value = [plan]
      return
    }

    const grouped = new Map<string, string[]>()
    for (const { skill, agent } of pairs) {
      const cell = matrixCellState(skill, agent)
      if (!cell.selectableForInstall) continue
      const existing = grouped.get(skill.id) ?? []
      existing.push(agent.id)
      grouped.set(skill.id, existing)
    }
    if (grouped.size === 0) {
      planError.value = t('matrix.noAgentsSelected')
      return
    }
    const created: OperationPlan[] = []
    for (const [capabilityId, agentIds] of grouped) {
      const plan = await api.planInstall({
        capabilityId,
        versionId: null,
        agentIds,
        strategy: strategy.value === 'default' ? null : strategy.value,
        force: false,
      })
      created.push(plan)
    }
    plans.value = created
  } catch (err) {
    planError.value = normalizeError(err, t('errors.planFailed'))
  } finally {
    planning.value = false
  }
}

async function refreshSkills(): Promise<void> {
  try {
    await store.refresh()
  } catch (err) {
    ui.notifyError(t('errors.skillsFailed'), normalizeError(err))
  }
}

async function refreshAgents(): Promise<void> {
  try {
    await app.refreshAgents()
  } catch (err) {
    ui.notifyError(t('errors.agentsFailed'), normalizeError(err))
  }
}

async function afterApplied(): Promise<void> {
  clearSelection()
  await Promise.allSettled([
    store.refresh(),
    app.refreshAgents(),
    app.loadDashboard(true),
  ])
}

function switchMode(next: Mode): void {
  if (mode.value === next) return
  mode.value = next
  clearSelection()
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('matrix.title')" :subtitle="t('matrix.subtitle')">
      <template #meta>
        <p class="hint" style="margin-top: 6px">
          {{ skills.length }} {{ t('matrix.skillHeader') }} × {{ agents.length }}
          {{ t('matrix.agentHeader') }} · {{ agents.length }} {{ t('matrix.cells') }}
        </p>
      </template>
      <template #actions>
        <button type="button" class="btn" :disabled="store.loading" @click="refreshSkills">
          <span v-if="store.loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="store.error"
        :message="store.error"
        :title="t('errors.skillsFailed')"
        :retry-label="t('common.retry')"
        @retry="refreshSkills"
      />
      <ErrorBanner
        v-if="app.agentsError"
        :message="app.agentsError"
        :title="t('errors.agentsFailed')"
        :retry-label="t('common.retry')"
        @retry="refreshAgents"
      />
      <ErrorBanner v-if="planError" :message="planError" :title="t('errors.planFailed')" />

      <div class="card">
        <div class="card-body">
          <div class="row row-wrap" style="gap: 10px">
            <div class="segmented" role="group" :aria-label="t('matrix.installMode')">
              <button
                type="button"
                :class="{ active: mode === 'install' }"
                @click="switchMode('install')"
              >
                {{ t('matrix.installMode') }}
              </button>
              <button
                type="button"
                :class="{ active: mode === 'uninstall' }"
                @click="switchMode('uninstall')"
              >
                {{ t('matrix.uninstallMode') }}
              </button>
            </div>

            <select v-if="mode === 'install'" v-model="strategy" class="select" style="width: 170px">
              <option value="default">{{ t('matrix.defaultStrategy') }}</option>
              <option value="copy">{{ t('status.strategy.copy') }}</option>
              <option value="symlink">{{ t('status.strategy.symlink') }}</option>
            </select>

            <span class="badge badge-accent">{{ selectedLabel }}</span>
            <button
              v-if="selectedCount > 0"
              type="button"
              class="btn btn-ghost btn-sm"
              @click="clearSelection"
            >
              {{ t('matrix.clearSelection') }}
            </button>

            <span class="spacer" />
            <button
              type="button"
              class="btn btn-primary"
              :disabled="planning || selectedCount === 0"
              @click="generatePlans"
            >
              <span v-if="planning" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="eye" :size="13" />
              {{
                planning
                  ? t('matrix.generating')
                  : mode === 'install'
                    ? t('matrix.generatePlan')
                    : t('matrix.uninstallPlan')
              }}
            </button>
          </div>
          <p class="hint" style="margin-top: 8px">
            {{ t('matrix.hint') }} {{ t('matrix.selectInstallHint') }}
          </p>
        </div>
      </div>

      <LoadingState v-if="loading" :label="t('common.loadingData')" />

      <EmptyState
        v-else-if="skills.length === 0"
        icon="skills"
        :title="t('matrix.emptyTitle')"
        :body="t('matrix.emptySkills')"
      >
        <template #actions>
          <RouterLink :to="{ name: 'skills' }" class="btn btn-primary">
            {{ t('nav.skills') }}
          </RouterLink>
        </template>
      </EmptyState>

      <EmptyState
        v-else-if="agents.length === 0"
        icon="agents"
        :title="t('matrix.emptyTitle')"
        :body="t('matrix.emptyAgents')"
      >
        <template #actions>
          <RouterLink :to="{ name: 'agents' }" class="btn btn-primary">
            {{ t('nav.agents') }}
          </RouterLink>
        </template>
      </EmptyState>

      <div v-else class="matrix-wrap">
        <table class="matrix">
          <thead>
            <tr>
              <th class="sticky-col" scope="col">{{ t('matrix.skillHeader') }}</th>
              <th
                v-for="agent in agents"
                :key="agent.id"
                scope="col"
                :style="
                  highlightAgentKey === agent.key
                    ? { background: 'var(--ah-accent-soft)' }
                    : undefined
                "
              >
                <RouterLink
                  :to="{ name: 'agents', query: { agent: agent.key } }"
                  class="row"
                  style="gap: 4px; text-decoration: none"
                >
                  <span style="font-weight: 600">{{ agent.name }}</span>
                  <Icon name="chevron-right" :size="10" class="subtle" />
                </RouterLink>
                <p class="hint" style="margin-top: 2px">
                  {{ agent.vendor }} · {{ t('matrix.installedCount', { count: agent.installedSkillCount }) }}
                </p>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="skill in skills" :key="skill.id">
              <td class="sticky-col">
                <RouterLink
                  :to="{ name: 'skill-detail', params: { id: skill.id } }"
                  style="text-decoration: none"
                >
                  <div class="row" style="gap: 4px">
                    <span style="font-weight: 600">{{ skill.name }}</span>
                    <Icon name="chevron-right" :size="10" class="subtle" />
                  </div>
                </RouterLink>
                <p class="hint">
                  <span class="chip">{{ skill.version }}</span>
                  · {{ t('matrix.installedCount', { count: skill.installations.length }) }}
                </p>
                <p v-if="skill.updateAvailable" class="hint tone-info">
                  {{ t('skills.updateAvailable') }}
                </p>
              </td>
              <td v-for="agent in agents" :key="agent.id">
                <button
                  type="button"
                  class="matrix-cell-btn"
                  :class="{ selected: isSelected(skill, agent) }"
                  :disabled="!isSelectable(skill, agent)"
                  :title="cellTitle(skill, agent)"
                  @click="toggleCell(skill, agent)"
                >
                  <StatusBadge
                    :label="t(cellOf(skill, agent).labelKey)"
                    :tone="cellOf(skill, agent).tone"
                    :dot="false"
                  />
                  <Icon
                    v-if="isSelected(skill, agent)"
                    name="check"
                    :size="10"
                    style="position: absolute; margin-left: 58px; margin-top: -14px"
                  />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <p v-if="agents.length > 0" class="hint">{{ t('matrix.planHint') }}</p>
    </div>

    <PlanDialog v-if="plans" :plans="plans" @close="plans = null" @applied="afterApplied" />
  </div>
</template>
