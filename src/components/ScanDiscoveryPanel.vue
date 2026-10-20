<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import BaseModal from './BaseModal.vue'
import EmptyState from './EmptyState.vue'
import ErrorBanner from './ErrorBanner.vue'
import Icon from './Icon.vue'
import StatusBadge from './StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import {
  buildScanImportRequest,
  countValidGroups,
  filterDiscoveredGroups,
  groupDiscoveredSkills,
  isReadOnlyRootSource,
  isSelectableGroup,
  normalizeRootSource,
  pageDiscoveredGroups,
  selectableGroupKeys,
  selectionSummary,
  type DiscoveredGroup,
  type DiscoveredStatusFilter,
} from '../lib/discovered'
import { formatBytes, formatDateTime } from '../lib/format'
import type { ScanImportResult } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const app = useAppStore()
const skillsStore = useSkillsStore()
const ui = useUiStore()

const PAGE_SIZE = 100

const search = ref('')
const status = ref<DiscoveredStatusFilter>('discovered')
const showReadOnly = ref(false)
const pageLimit = ref(PAGE_SIZE)
const expanded = ref<Set<string>>(new Set())
const selected = ref<Set<string>>(new Set())

const confirmOpen = ref(false)
const includeReadOnly = ref(false)
const importing = ref(false)
const importError = ref<string | null>(null)
const importResult = ref<ScanImportResult | null>(null)
const resultOpen = ref(false)
const refreshing = ref(false)
const refreshError = ref<string | null>(null)

const scanning = computed(() => app.scanning)
const report = computed(() => app.scanReport)
const groups = computed(() => groupDiscoveredSkills(report.value))
const validCount = computed(() => countValidGroups(groups.value))
const filtered = computed(() =>
  filterDiscoveredGroups(groups.value, {
    search: search.value,
    status: status.value,
    showReadOnly: showReadOnly.value,
  }),
)
const visible = computed(() => pageDiscoveredGroups(filtered.value, pageLimit.value))
const summary = computed(() => selectionSummary(groups.value, selected.value))
const selectedSizeLabel = computed(() => formatBytes(summary.value.sizeBytes))
const eligibleForSelectAll = computed(() =>
  selectableGroupKeys(filtered.value, { includeReadOnly: showReadOnly.value }),
)
const canBulkSelect = computed(() => eligibleForSelectAll.value.length > 0)
const hiddenByPagination = computed(() =>
  Math.max(0, filtered.value.length - visible.value.length),
)

const statusOptions = computed<Array<{ key: DiscoveredStatusFilter; label: string }>>(() => [
  { key: 'all', label: t('skills.discovery.filterAll') },
  { key: 'discovered', label: t('skills.discovery.filterDiscovered') },
  { key: 'library', label: t('skills.discovery.filterLibrary') },
  { key: 'readonly', label: t('skills.discovery.filterReadonly') },
])

const reportSummary = computed(() => {
  if (!report.value) return ''
  return `${t('skills.discovery.scannedAt', {
    time: formatDateTime(report.value.scannedAt, locale.value),
  })} · ${t('skills.discovery.validCount', { count: validCount.value })} · ${t(
    'skills.discovery.totalRows',
    { count: groups.value.length },
  )}`
})

const resultChips = computed(() => {
  const result = importResult.value
  if (!result) return []
  return [
    {
      key: 'imported',
      label: t('skills.discovery.resultImported'),
      value: result.importedCount,
      tone: 'success',
    },
    {
      key: 'existing',
      label: t('skills.discovery.resultSkippedExisting'),
      value: result.skippedExisting,
      tone: 'neutral',
    },
    {
      key: 'duplicate',
      label: t('skills.discovery.resultSkippedDuplicate'),
      value: result.skippedDuplicate,
      tone: 'neutral',
    },
    {
      key: 'readonly',
      label: t('skills.discovery.resultSkippedReadOnly'),
      value: result.skippedReadOnly,
      tone: 'warn',
    },
    {
      key: 'limit',
      label: t('skills.discovery.resultSkippedLimit'),
      value: result.skippedLimit,
      tone: 'warn',
    },
    {
      key: 'failed',
      label: t('skills.discovery.resultFailed'),
      value: result.failed.length,
      tone: result.failed.length > 0 ? 'danger' : 'neutral',
    },
  ]
})

watch([search, status, showReadOnly], () => {
  pageLimit.value = PAGE_SIZE
})

watch(status, (value) => {
  if (value === 'readonly') showReadOnly.value = true
})

function rootSourceLabel(source: string): string {
  return normalizeRootSource(source)
}

function isReadOnlyGroup(group: DiscoveredGroup): boolean {
  return isReadOnlyRootSource(group.representativeRootSource)
}

async function runScan(): Promise<void> {
  try {
    const scan = await app.scan({ refreshDashboard: false })
    selected.value = new Set()
    expanded.value = new Set()
    pageLimit.value = PAGE_SIZE
    ui.notify('success', t('skills.discovery.scanDone'), {
      detail: t('skills.discovery.validCount', {
        count: countValidGroups(groupDiscoveredSkills(scan)),
      }),
    })
  } catch (err) {
    ui.notifyError(t('skills.discovery.scanFailed'), normalizeError(err))
  }
}

function isSelected(group: DiscoveredGroup): boolean {
  return selected.value.has(group.key)
}

function toggleSelected(group: DiscoveredGroup): void {
  if (!isSelectableGroup(group)) return
  const next = new Set(selected.value)
  if (next.has(group.key)) next.delete(group.key)
  else next.add(group.key)
  selected.value = next
}

function selectAllFiltered(): void {
  const next = new Set(selected.value)
  for (const key of eligibleForSelectAll.value) next.add(key)
  selected.value = next
}

function clearSelection(): void {
  selected.value = new Set()
}

function isExpanded(group: DiscoveredGroup): boolean {
  return expanded.value.has(group.key)
}

function toggleExpanded(group: DiscoveredGroup): void {
  const next = new Set(expanded.value)
  if (next.has(group.key)) next.delete(group.key)
  else next.add(group.key)
  expanded.value = next
}

function resetFilters(): void {
  search.value = ''
  status.value = 'discovered'
  showReadOnly.value = false
}

function openConfirm(): void {
  if (summary.value.count === 0) {
    ui.notify('warning', t('skills.discovery.noSelection'))
    return
  }
  includeReadOnly.value = summary.value.requiresIncludeReadOnly
  importError.value = null
  confirmOpen.value = true
}

function closeConfirm(): void {
  if (importing.value) return
  confirmOpen.value = false
}

async function refreshAfterImport(): Promise<void> {
  refreshing.value = true
  refreshError.value = null
  const errors: string[] = []
  try {
    await app.scan({ refreshDashboard: false })
  } catch (err) {
    errors.push(normalizeError(err))
  }
  try {
    await skillsStore.refresh()
  } catch (err) {
    errors.push(normalizeError(err))
  }
  try {
    await app.loadDashboard(true)
  } catch (err) {
    errors.push(normalizeError(err))
  }
  if (errors.length > 0) {
    refreshError.value = errors.join('\n')
    ui.notifyError(t('skills.discovery.refreshFailed'), refreshError.value)
  }
  refreshing.value = false
}

async function confirmImport(): Promise<void> {
  if (importing.value) return
  if (summary.value.requiresIncludeReadOnly && !includeReadOnly.value) {
    importError.value = t('skills.discovery.readonlyRequired')
    return
  }
  importing.value = true
  importError.value = null
  try {
    const request = buildScanImportRequest(groups.value, selected.value, {
      includeReadOnlySources: includeReadOnly.value,
    })
    importResult.value = await api.skillsImportScan(request)
    confirmOpen.value = false
    selected.value = new Set()
    expanded.value = new Set()
    resultOpen.value = true
    await refreshAfterImport()
  } catch (err) {
    importError.value = normalizeError(err, t('skills.discovery.importFailed'))
  } finally {
    importing.value = false
  }
}

function closeResult(): void {
  if (importing.value) return
  resultOpen.value = false
}
</script>

<template>
  <div class="section-stack">
    <div class="notice notice-info">
      <Icon class="notice-icon" name="info" :size="14" />
      <p>{{ t('skills.discovery.lead') }}</p>
    </div>

    <div v-if="!report" class="card">
      <EmptyState
        icon="scan"
        :title="t('skills.discovery.neverScanned')"
        :body="t('skills.discovery.neverScannedBody')"
      >
        <template #actions>
          <button type="button" class="btn btn-primary" :disabled="scanning" @click="runScan">
            <span v-if="scanning" class="spinner spinner-sm" aria-hidden="true" />
            <Icon v-else name="scan" :size="13" />
            {{ scanning ? t('skills.discovery.scanning') : t('skills.discovery.scanNow') }}
          </button>
        </template>
      </EmptyState>
      <div v-if="app.scanError" class="card-body" style="border-top: 1px solid var(--ah-line)">
        <ErrorBanner
          :message="app.scanError"
          :title="t('skills.discovery.scanFailed')"
          :retry-label="t('common.retry')"
          :retrying="scanning"
          @retry="runScan"
        />
      </div>
    </div>

    <template v-else>
      <div class="card">
        <div class="card-body stack-sm">
          <div class="toolbar">
            <div class="search-wrap">
              <span class="search-icon"><Icon name="search" :size="13" /></span>
              <input
                id="skills-discovery-search"
                v-model="search"
                class="input"
                type="search"
                :placeholder="t('skills.discovery.searchPlaceholder')"
                :aria-label="t('skills.searchLabel')"
              />
            </div>
            <select v-model="status" class="select" style="width: 160px">
              <option v-for="option in statusOptions" :key="option.key" :value="option.key">
                {{ option.label }}
              </option>
            </select>
            <label class="checkbox-row" style="white-space: nowrap">
              <input v-model="showReadOnly" type="checkbox" class="checkbox" />
              <span>{{ t('skills.discovery.showReadOnly') }}</span>
            </label>
            <span class="spacer" />
            <button type="button" class="btn" :disabled="scanning || importing" @click="runScan">
              <span v-if="scanning" class="spinner spinner-sm" aria-hidden="true" />
              <Icon v-else name="refresh" :size="13" />
              {{ scanning ? t('skills.discovery.scanning') : t('skills.discovery.rescan') }}
            </button>
          </div>
          <p class="hint">{{ reportSummary }}</p>
          <p class="hint">{{ t('skills.discovery.showReadOnlyHint') }}</p>
        </div>
      </div>

      <ErrorBanner
        v-if="app.scanError"
        :message="app.scanError"
        :title="t('skills.discovery.scanFailed')"
        :retry-label="t('common.retry')"
        :retrying="scanning"
        @retry="runScan"
      />

      <div class="card">
        <div class="card-body">
          <div class="row row-wrap" style="gap: 8px">
            <button
              type="button"
              class="btn btn-sm"
              :disabled="!canBulkSelect || scanning || importing"
              :title="canBulkSelect ? t('skills.discovery.selectableHint') : undefined"
              @click="selectAllFiltered"
            >
              <Icon name="check" :size="12" />
              {{ t('skills.discovery.selectAll') }}
            </button>
            <button
              v-if="summary.count > 0"
              type="button"
              class="btn btn-sm btn-ghost"
              :disabled="importing"
              @click="clearSelection"
            >
              {{ t('skills.discovery.clearSelection') }}
            </button>
            <span class="badge" :class="summary.count > 0 ? 'badge-accent' : 'badge-neutral'">
              {{
                t('skills.discovery.selectedSummary', {
                  count: summary.count,
                  size: selectedSizeLabel,
                })
              }}
            </span>
            <span class="spacer" />
            <button
              type="button"
              class="btn btn-primary"
              :disabled="summary.count === 0 || importing"
              @click="openConfirm"
            >
              <Icon name="import" :size="13" />
              {{ t('skills.discovery.importSelected', { count: summary.count }) }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="filtered.length === 0" class="card">
        <EmptyState
          icon="search"
          :title="t('skills.discovery.emptyFiltered')"
          :body="
            groups.length === 0
              ? t('onboarding.scan.noDiscoveries')
              : t('skills.discovery.emptyFilteredBody')
          "
        >
          <template #actions>
            <button type="button" class="btn" @click="resetFilters">
              {{ t('skills.discovery.clearFilters') }}
            </button>
          </template>
        </EmptyState>
      </div>

      <div v-else class="card">
        <div class="card-body flush">
          <div class="table-wrap" style="max-height: calc(100vh - 360px)">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col" style="width: 32px"></th>
                  <th scope="col">{{ t('skills.discovery.columns.skill') }}</th>
                  <th scope="col">{{ t('skills.discovery.columns.state') }}</th>
                  <th scope="col">{{ t('skills.discovery.columns.agents') }}</th>
                  <th scope="col">{{ t('skills.discovery.columns.source') }}</th>
                  <th scope="col" style="text-align: right">
                    {{ t('skills.discovery.columns.size') }}
                  </th>
                  <th scope="col" style="width: 34px"></th>
                </tr>
              </thead>
              <tbody>
                <template v-for="group in visible" :key="group.key">
                  <tr class="row-click" @click="toggleExpanded(group)">
                    <td @click.stop>
                      <input
                        type="checkbox"
                        class="checkbox"
                        :checked="isSelected(group)"
                        :disabled="!isSelectableGroup(group)"
                        :aria-label="group.name"
                        :title="
                          isSelectableGroup(group)
                            ? undefined
                            : t('skills.discovery.selectableHint')
                        "
                        @change="toggleSelected(group)"
                      />
                    </td>
                    <td>
                      <div class="row" style="gap: 5px">
                        <span style="font-weight: 600">{{ group.name }}</span>
                        <span class="chip">{{ group.slug }}</span>
                        <span v-if="group.version" class="chip">{{ group.version }}</span>
                      </div>
                      <p class="hint truncate" style="max-width: 380px" :title="group.description">
                        {{ group.description || '—' }}
                      </p>
                    </td>
                    <td>
                      <div class="row row-wrap" style="gap: 4px">
                        <StatusBadge
                          v-if="group.inLibrary"
                          :label="t('skills.discovery.inLibrary')"
                          tone="success"
                        />
                        <StatusBadge
                          v-else
                          :label="t('skills.discovery.notImported')"
                          tone="info"
                        />
                        <StatusBadge
                          v-if="group.readOnlyOnly"
                          :label="t('skills.discovery.readonly')"
                          tone="warn"
                        />
                        <StatusBadge
                          v-if="group.invalidCount > 0"
                          :label="t('skills.discovery.invalid')"
                          tone="danger"
                        />
                      </div>
                    </td>
                    <td>
                      <div class="row row-wrap" style="gap: 3px">
                        <span
                          v-for="agent in group.agents.slice(0, 3)"
                          :key="agent.agentId"
                          class="tag"
                        >
                          {{ agent.agentName }}
                        </span>
                        <span v-if="group.agents.length > 3" class="tag">
                          +{{ group.agents.length - 3 }}
                        </span>
                      </div>
                      <p class="hint">
                        {{ t('skills.discovery.agentsCount', { count: group.agents.length }) }} ·
                        {{ t('skills.discovery.pathsCount', { count: group.paths.length }) }}
                      </p>
                    </td>
                    <td>
                      <div class="row row-wrap" style="gap: 3px">
                        <span
                          v-for="source in group.rootSources.slice(0, 2)"
                          :key="source"
                          class="chip"
                        >
                          {{ source }}
                        </span>
                        <span v-if="group.rootSources.length > 2" class="tag">
                          +{{ group.rootSources.length - 2 }}
                        </span>
                      </div>
                      <p v-if="isReadOnlyGroup(group)" class="hint tone-warn">
                        {{ t('skills.discovery.readonlyOnly') }}
                      </p>
                    </td>
                    <td class="num" style="text-align: right">
                      {{ formatBytes(group.sizeBytes) }}
                    </td>
                    <td>
                      <button
                        type="button"
                        class="btn btn-ghost btn-icon btn-sm"
                        :aria-label="
                          isExpanded(group)
                            ? t('skills.discovery.collapse')
                            : t('skills.discovery.details')
                        "
                        @click.stop="toggleExpanded(group)"
                      >
                        <Icon
                          :name="isExpanded(group) ? 'chevron-down' : 'chevron-right'"
                          :size="12"
                        />
                      </button>
                    </td>
                  </tr>
                  <tr v-if="isExpanded(group)">
                    <td :colspan="7" style="background: var(--ah-surface-2)">
                      <div class="stack-md" style="padding: 3px 2px">
                        <div class="grid-2" style="gap: 12px">
                          <div>
                            <p class="section-title">{{ t('skills.discovery.description') }}</p>
                            <p class="text-sm" style="white-space: pre-wrap">
                              {{ group.description || '—' }}
                            </p>
                          </div>
                          <div>
                            <p class="section-title">{{ t('skills.discovery.agents') }}</p>
                            <div class="stack-sm">
                              <div
                                v-for="agent in group.agents"
                                :key="agent.agentId"
                                class="text-sm"
                              >
                                <span style="font-weight: 550">{{ agent.agentName }}</span>
                                <span class="hint"> · {{ agent.agentKey }}</span>
                                <p class="path-text">{{ agent.path }}</p>
                              </div>
                            </div>
                          </div>
                          <div>
                            <p class="section-title">{{ t('skills.discovery.paths') }}</p>
                            <div class="list-scroll" style="max-height: 160px">
                              <p
                                v-for="path in group.paths"
                                :key="path"
                                class="path-text"
                              >
                                {{ path }}
                              </p>
                            </div>
                          </div>
                          <div>
                            <p class="section-title">{{ t('skills.discovery.rootSources') }}</p>
                            <div class="row row-wrap" style="gap: 3px">
                              <span
                                v-for="source in group.rootSources"
                                :key="source"
                                class="chip"
                              >
                                {{ rootSourceLabel(source) }}
                              </span>
                            </div>
                            <p class="section-title" style="margin-top: 10px">
                              {{ t('skills.discovery.representativePath') }}
                            </p>
                            <p class="path-text">{{ group.representativePath }}</p>
                            <dl class="definition-grid" style="margin-top: 10px">
                              <dt>{{ t('skills.discovery.hash') }}</dt>
                              <dd class="mono">{{ group.contentHash || '—' }}</dd>
                              <dt>{{ t('skills.discovery.version') }}</dt>
                              <dd>{{ group.version || '—' }}</dd>
                              <dt>{{ t('skills.discovery.fileCount') }}</dt>
                              <dd>{{ group.fileCount }}</dd>
                            </dl>
                          </div>
                        </div>
                        <div v-if="group.issues.length > 0">
                          <p class="section-title">{{ t('skills.discovery.issues') }}</p>
                          <p
                            v-for="(issue, index) in group.issues"
                            :key="index"
                            class="hint tone-warn"
                          >
                            ⚠ {{ issue }}
                          </p>
                        </div>
                        <p v-if="group.conflict" class="hint tone-warn">{{ group.conflict }}</p>
                      </div>
                    </td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>
        </div>
        <div class="card-body row row-wrap" style="border-top: 1px solid var(--ah-line)">
          <p class="hint">
            {{
              t('skills.discovery.shownCount', {
                shown: visible.length,
                total: filtered.length,
              })
            }}
          </p>
          <span class="spacer" />
          <button
            v-if="hiddenByPagination > 0"
            type="button"
            class="btn btn-sm"
            @click="pageLimit += PAGE_SIZE"
          >
            {{ t('skills.discovery.loadMore', { count: hiddenByPagination }) }}
          </button>
        </div>
      </div>
    </template>

    <BaseModal
      v-if="confirmOpen"
      :title="t('skills.discovery.confirmTitle')"
      :subtitle="t('skills.discovery.confirmBody', { count: summary.count, size: selectedSizeLabel })"
      size="md"
      @close="closeConfirm"
    >
      <div class="stack-md">
        <div class="row row-wrap">
          <span class="badge badge-accent">{{ t('skills.discovery.size') }} {{ selectedSizeLabel }}</span>
          <span class="badge badge-neutral">
            {{ t('skills.discovery.agentsCount', { count: summary.agentNames.length }) }}
          </span>
          <span class="badge badge-neutral">
            {{ t('skills.discovery.pathsCount', { count: summary.paths.length }) }}
          </span>
        </div>

        <div>
          <p class="section-title">{{ t('skills.discovery.confirmAgents') }}</p>
          <div class="row row-wrap" style="gap: 3px">
            <span v-for="name in summary.agentNames" :key="name" class="tag">{{ name }}</span>
            <span v-if="summary.agentNames.length === 0" class="hint">—</span>
          </div>
        </div>

        <div>
          <p class="section-title">{{ t('skills.discovery.confirmPaths') }}</p>
          <div class="surface-inset list-scroll" style="max-height: 150px; padding: 8px">
            <p v-for="path in summary.paths" :key="path" class="path-text">{{ path }}</p>
          </div>
        </div>

        <label class="checkbox-row surface-inset" style="padding: 9px 10px">
          <input v-model="includeReadOnly" type="checkbox" class="checkbox" />
          <span>
            <span style="font-weight: 600">{{ t('skills.discovery.includeReadOnly') }}</span>
            <span class="hint" style="display: block">{{ t('skills.discovery.includeReadOnlyHint') }}</span>
          </span>
        </label>

        <p
          v-if="summary.requiresIncludeReadOnly && !includeReadOnly"
          class="notice notice-warn"
        >
          {{ t('skills.discovery.readonlyRequired') }}
        </p>
        <p v-else-if="summary.requiresIncludeReadOnly" class="hint">
          {{ t('skills.discovery.readonlySelectionHint') }}
        </p>

        <ErrorBanner
          v-if="importError"
          :message="importError"
          :title="t('skills.discovery.importFailed')"
        />
        <p v-if="refreshing" class="hint">{{ t('skills.discovery.refreshing') }}</p>
      </div>
      <template #footer>
        <button
          type="button"
          class="btn"
          :disabled="importing"
          @click="closeConfirm"
        >
          {{ t('common.cancel') }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="importing || (summary.requiresIncludeReadOnly && !includeReadOnly)"
          @click="confirmImport"
        >
          <span v-if="importing" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="import" :size="13" />
          {{ importing ? t('skills.discovery.importing') : t('skills.discovery.confirmImport') }}
        </button>
      </template>
    </BaseModal>

    <BaseModal
      v-if="resultOpen && importResult"
      :title="t('skills.discovery.resultTitle')"
      :subtitle="t('skills.discovery.resultLead')"
      size="xl"
      @close="closeResult"
    >
      <div class="stack-md">
        <div class="row row-wrap" style="gap: 5px">
          <span
            v-for="chip in resultChips"
            :key="chip.key"
            class="badge"
            :class="`badge-${chip.tone}`"
          >
            {{ chip.label }} {{ chip.value }}
          </span>
          <span class="badge badge-outline">
            {{ t('skills.discovery.resultTotalCandidates', { count: importResult.totalCandidates }) }}
          </span>
        </div>

        <p v-if="refreshing" class="hint">{{ t('skills.discovery.refreshing') }}</p>
        <ErrorBanner
          v-if="refreshError"
          :message="refreshError"
          :title="t('skills.discovery.refreshFailed')"
        />

        <p
          v-if="importResult.skippedReadOnly > 0"
          class="notice notice-warn"
        >
          {{ t('skills.discovery.resultReadonlyHint', { count: importResult.skippedReadOnly }) }}
        </p>

        <div>
          <p class="section-title">{{ t('skills.discovery.resultImportedTitle') }}</p>
          <div
            v-if="importResult.imported.length === 0"
            class="hint"
          >
            {{ t('common.noData') }}
          </div>
          <div v-else class="table-wrap surface-inset" style="max-height: 300px">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('common.name') }}</th>
                  <th scope="col">{{ t('common.version') }}</th>
                  <th scope="col">{{ t('skills.discovery.agents') }}</th>
                  <th scope="col">{{ t('common.path') }}</th>
                  <th scope="col" style="text-align: right">{{ t('common.size') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in importResult.imported" :key="`${item.slug}:${item.path}`">
                  <td>
                    <div class="row" style="gap: 5px">
                      <span style="font-weight: 550">{{ item.name }}</span>
                      <span class="chip">{{ item.slug }}</span>
                    </div>
                  </td>
                  <td><span class="chip">{{ item.versionLabel }}</span></td>
                  <td>{{ item.agentKey ?? '—' }}</td>
                  <td class="path-text">{{ item.path }}</td>
                  <td class="num" style="text-align: right">{{ formatBytes(item.sizeBytes) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <details v-if="importResult.failed.length > 0" class="surface-inset" style="padding: 9px 11px">
          <summary class="text-sm" style="cursor: pointer; font-weight: 600">
            {{ t('skills.discovery.resultFailuresTitle', { count: importResult.failed.length }) }}
          </summary>
          <div class="table-wrap" style="margin-top: 8px; max-height: 220px">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('common.name') }}</th>
                  <th scope="col">{{ t('common.path') }}</th>
                  <th scope="col">{{ t('common.error') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="failure in importResult.failed" :key="`${failure.name}:${failure.path}`">
                  <td>{{ failure.name }}</td>
                  <td class="path-text">{{ failure.path }}</td>
                  <td class="hint tone-danger">{{ failure.error }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </details>
      </div>
      <template #footer>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="importing || refreshing"
          @click="closeResult"
        >
          {{ t('common.close') }}
        </button>
      </template>
    </BaseModal>
  </div>
</template>
