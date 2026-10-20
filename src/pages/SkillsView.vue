<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import DataTable from '../components/DataTable.vue'
import EmptyState from '../components/EmptyState.vue'
import HealthStatusBadge from '../components/HealthStatusBadge.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import SkillImportDialog from '../components/SkillImportDialog.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { formatBytes, formatDateTime, formatRelativeTime } from '../lib/format'
import type { CapabilityHealth, HealthOverview, Skill } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'

type StatusFilter = 'all' | 'installed' | 'not_installed' | 'updates' | 'drift' | 'conflict'
type SortKey = 'name' | 'updated' | 'size' | 'installations'

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useSkillsStore()
const app = useAppStore()
const ui = useUiStore()

const search = ref('')
const statusFilter = ref<StatusFilter>('all')
const sortKey = ref<SortKey>('name')
const agentFilter = ref<string | null>(null)
const showImport = ref(false)

const healthOverview = ref<HealthOverview | null>(null)
const healthByCapability = computed(
  () =>
    new Map((healthOverview.value?.capabilities ?? []).map((item) => [item.capabilityId, item])),
)

const statusOptions = computed<Array<{ key: StatusFilter; label: string }>>(() => [
  { key: 'all', label: t('skills.filterAll') },
  { key: 'installed', label: t('skills.filterInstalled') },
  { key: 'not_installed', label: t('skills.filterNotInstalled') },
  { key: 'updates', label: t('skills.filterUpdates') },
  { key: 'drift', label: t('skills.filterDrift') },
  { key: 'conflict', label: t('skills.filterConflict') },
])

const sortOptions = computed<Array<{ key: SortKey; label: string }>>(() => [
  { key: 'name', label: t('skills.sortName') },
  { key: 'updated', label: t('skills.sortUpdated') },
  { key: 'size', label: t('skills.sortSize') },
  { key: 'installations', label: t('skills.sortInstallations') },
])

const columns = computed(() => [
  { key: 'name', label: t('skills.columns.name') },
  { key: 'version', label: t('skills.columns.version'), width: '110px' },
  { key: 'installs', label: t('skills.columns.installs') },
  { key: 'state', label: t('skills.columns.state') },
  { key: 'health', label: t('skills.columns.health'), width: '140px' },
  { key: 'size', label: t('skills.columns.size'), align: 'right' as const, width: '90px' },
  { key: 'source', label: t('skills.columns.source'), width: '110px' },
  { key: 'updated', label: t('skills.columns.updated'), width: '130px' },
])

const agentFilterName = computed(
  () => app.agents.find((agent) => agent.key === agentFilter.value)?.name ?? agentFilter.value,
)

const filteredSkills = computed<Skill[]>(() => {
  const query = search.value.trim().toLowerCase()
  const list = store.skills.filter((skill) => {
    if (agentFilter.value) {
      if (!skill.installations.some((item) => item.agentKey === agentFilter.value)) return false
    }
    switch (statusFilter.value) {
      case 'installed':
        if (skill.installations.length === 0) return false
        break
      case 'not_installed':
        if (skill.installations.length > 0) return false
        break
      case 'updates':
        if (!skill.updateAvailable) return false
        break
      case 'drift':
        if (skill.driftCount === 0 && !skill.installations.some((item) => item.drift)) return false
        break
      case 'conflict':
        if (!skill.conflict && !skill.installations.some((item) => item.state === 'conflict')) {
          return false
        }
        break
      default:
        break
    }
    if (!query) return true
    const haystack = [
      skill.name,
      skill.slug,
      skill.description,
      skill.tags.join(' '),
    ]
      .join(' ')
      .toLowerCase()
    return haystack.includes(query)
  })

  const sorted = [...list]
  sorted.sort((a, b) => {
    switch (sortKey.value) {
      case 'updated':
        return b.updatedAt.localeCompare(a.updatedAt)
      case 'size':
        return b.sizeBytes - a.sizeBytes
      case 'installations':
        return b.installations.length - a.installations.length
      default:
        return a.name.localeCompare(b.name)
    }
  })
  return sorted
})

onMounted(async () => {
  applyRouteQuery()
  void loadHealth()
  try {
    await store.load(true)
  } catch (err) {
    ui.notifyError(t('errors.skillsFailed'), normalizeError(err))
  }
})

/** Loaded once for the whole table (capabilityId → health) to avoid N+1 calls. */
async function loadHealth(): Promise<void> {
  try {
    healthOverview.value = await api.healthOverview()
  } catch {
    healthOverview.value = null
  }
}

function healthFor(skill: Skill): CapabilityHealth | null {
  return healthByCapability.value.get(skill.id) ?? null
}

watch(
  () => route.query,
  () => applyRouteQuery(),
)

function applyRouteQuery(): void {
  const agent = route.query.agent
  agentFilter.value = typeof agent === 'string' && agent.length > 0 ? agent : null
  const filter = route.query.filter
  if (
    typeof filter === 'string' &&
    ['installed', 'not_installed', 'updates', 'drift', 'conflict'].includes(filter)
  ) {
    statusFilter.value = filter as StatusFilter
  }
}

function skillKey(skill: Skill): string {
  return skill.id
}

function openSkill(skill: Skill): void {
  void router.push({ name: 'skill-detail', params: { id: skill.id } })
}

function installedAgentNames(skill: Skill): string[] {
  return skill.installations.map((item) => item.agentName)
}

interface BadgeSpec {
  label: string
  tone: 'success' | 'warn' | 'danger' | 'info' | 'neutral'
}

function rowBadges(skill: Skill): BadgeSpec[] {
  const badges: BadgeSpec[] = []
  if (skill.updateAvailable) badges.push({ label: t('skills.updateAvailable'), tone: 'info' })
  if (skill.driftCount > 0) {
    badges.push({ label: `${t('skills.drift')} ${skill.driftCount}`, tone: 'warn' })
  }
  if (skill.conflict) badges.push({ label: t('skills.conflict'), tone: 'warn' })
  if (skill.installations.length === 0) {
    badges.push({ label: t('skills.notInstalled'), tone: 'neutral' })
  }
  return badges
}

function clearFilters(): void {
  search.value = ''
  statusFilter.value = 'all'
  sortKey.value = 'name'
  agentFilter.value = null
  if (Object.keys(route.query).length > 0) void router.replace({ name: 'skills' })
}

async function refresh(): Promise<void> {
  void loadHealth()
  try {
    await store.refresh()
  } catch (err) {
    ui.notifyError(t('errors.skillsFailed'), normalizeError(err))
  }
}

function onImported(skillId: string): void {
  showImport.value = false
  void router.push({ name: 'skill-detail', params: { id: skillId } })
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('skills.title')" :subtitle="t('skills.subtitle')">
      <template #meta>
        <p class="hint" style="margin-top: 6px">
          {{ filteredSkills.length }} / {{ store.skills.length }} {{ t('common.items') }}
          <span v-if="agentFilter"> · {{ t('skills.agentFilter', { agent: agentFilterName }) }}</span>
        </p>
      </template>
      <template #actions>
        <button type="button" class="btn" :disabled="store.loading" @click="refresh">
          <span v-if="store.loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" @click="showImport = true">
          <Icon name="import" :size="13" />
          {{ t('skills.import') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="store.error"
        :message="store.error"
        :title="t('errors.skillsFailed')"
        :retry-label="t('common.retry')"
        :retrying="store.loading"
        @retry="refresh"
      />

      <div class="card">
        <div class="card-body">
          <div class="toolbar">
            <div class="search-wrap">
              <span class="search-icon"><Icon name="search" :size="13" /></span>
              <input
                id="skills-search"
                v-model="search"
                class="input"
                type="search"
                :placeholder="t('skills.searchPlaceholder')"
                :aria-label="t('skills.searchLabel')"
              />
            </div>
            <select v-model="statusFilter" class="select" style="width: 150px">
              <option v-for="option in statusOptions" :key="option.key" :value="option.key">
                {{ option.label }}
              </option>
            </select>
            <select v-model="sortKey" class="select" style="width: 150px">
              <option v-for="option in sortOptions" :key="option.key" :value="option.key">
                {{ option.label }}
              </option>
            </select>
            <span v-if="agentFilter" class="badge badge-accent">
              {{ t('skills.agentFilter', { agent: agentFilterName }) }}
              <button
                type="button"
                class="btn btn-ghost btn-icon"
                style="width: 16px; height: 16px"
                :aria-label="t('skills.clearAgentFilter')"
                @click="clearFilters"
              >
                <Icon name="close" :size="10" />
              </button>
            </span>
            <button
              v-if="search || statusFilter !== 'all' || sortKey !== 'name' || agentFilter"
              type="button"
              class="btn btn-ghost btn-sm"
              @click="clearFilters"
            >
              {{ t('skills.clearFilters') }}
            </button>
          </div>
        </div>
      </div>

      <LoadingState v-if="store.loading && store.skills.length === 0" :label="t('skills.loading')" />

      <EmptyState
        v-else-if="store.skills.length === 0"
        icon="skills"
        :title="t('skills.emptyTitle')"
        :body="t('skills.emptyBody')"
      >
        <template #actions>
          <button type="button" class="btn btn-primary" @click="showImport = true">
            <Icon name="import" :size="13" />
            {{ t('skills.import') }}
          </button>
        </template>
      </EmptyState>

      <EmptyState
        v-else-if="filteredSkills.length === 0"
        icon="search"
        :title="t('skills.noMatchTitle')"
        :body="t('skills.noMatchBody')"
      >
        <template #actions>
          <button type="button" class="btn" @click="clearFilters">
            {{ t('skills.clearFilters') }}
          </button>
        </template>
      </EmptyState>

      <div v-else class="card">
        <div class="card-body flush">
          <DataTable
            :columns="columns"
            :rows="filteredSkills"
            :row-key="skillKey"
            clickable
            compact
            @row-click="openSkill"
          >
            <template #cell-name="{ row }">
              <div style="min-width: 0">
                <div class="row" style="gap: 5px">
                  <span style="font-weight: 600">{{ row.name }}</span>
                  <span class="chip">{{ row.slug }}</span>
                </div>
                <p class="hint truncate" style="max-width: 360px" :title="row.description">
                  {{ row.description || '—' }}
                </p>
                <div v-if="row.tags.length > 0" class="row row-wrap" style="gap: 3px; margin-top: 2px">
                  <span v-for="tag in row.tags.slice(0, 4)" :key="tag" class="tag">{{ tag }}</span>
                </div>
              </div>
            </template>
            <template #cell-version="{ row }">
              <span class="chip">{{ row.version || '—' }}</span>
            </template>
            <template #cell-installs="{ row }">
              <div v-if="row.installations.length === 0" class="hint">
                {{ t('skills.notInstalled') }}
              </div>
              <div v-else class="row row-wrap" style="gap: 3px">
                <span
                  v-for="name in installedAgentNames(row).slice(0, 2)"
                  :key="name"
                  class="badge badge-success"
                >
                  {{ name }}
                </span>
                <span v-if="row.installations.length > 2" class="badge badge-neutral">
                  {{ t('skills.moreInstallations', { count: row.installations.length - 2 }) }}
                </span>
              </div>
            </template>
            <template #cell-state="{ row }">
              <div class="row row-wrap" style="gap: 4px">
                <StatusBadge
                  v-for="badge in rowBadges(row)"
                  :key="badge.label"
                  :label="badge.label"
                  :tone="badge.tone"
                />
              </div>
            </template>
            <template #cell-health="{ row }">
              <HealthStatusBadge v-if="healthFor(row)" :state="healthFor(row)?.status ?? 'unverified'" />
              <StatusBadge
                v-else
                :label="t('skills.healthUnchecked')"
                tone="neutral"
                icon="alert-circle"
              />
            </template>
            <template #cell-size="{ row }">
              <span class="num">{{ formatBytes(row.sizeBytes) }}</span>
            </template>
            <template #cell-source="{ row }">
              <span class="chip">{{ row.source?.sourceType ?? t('skills.sourceLocal') }}</span>
            </template>
            <template #cell-updated="{ row }">
              <span :title="formatDateTime(row.updatedAt, locale)" class="text-xs">
                {{ formatRelativeTime(row.updatedAt, locale) }}
              </span>
            </template>
          </DataTable>
        </div>
      </div>

      <p class="hint">{{ t('skills.scannedImportHint') }}</p>
    </div>

    <SkillImportDialog
      v-if="showImport"
      @close="showImport = false"
      @imported="onImported"
    />
  </div>
</template>
