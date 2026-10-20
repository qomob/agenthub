<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import BaseModal from '../components/BaseModal.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import PlanDialog from '../components/PlanDialog.vue'
import SkillHealthPanel from '../components/SkillHealthPanel.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { formatBytes, formatDateTime, formatRelativeTime, shortHash } from '../lib/format'
import { parseFrontmatter, parseSkillMd } from '../lib/markdown'
import {
  agentStatusKey,
  installationStateKey,
  isAgentTargetable,
  operationKindKey,
  operationStatusKey,
  strategyKey,
  toneForAgentStatus,
  toneForInstallationState,
  toneForOperationStatus,
} from '../lib/status'
import type {
  InstallationState,
  InstallStrategy,
  OperationPlan,
  OperationView,
  SkillDetail,
  UpdateImpact,
} from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const app = useAppStore()
const store = useSkillsStore()
const ui = useUiStore()

const detail = ref<SkillDetail | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

const skillId = computed(() => (typeof route.params.id === 'string' ? route.params.id : ''))
const skill = computed(() => detail.value?.skill ?? null)
const mdBlocks = computed(() => (detail.value ? parseSkillMd(detail.value.skillMd) : []))
const frontmatter = computed(() => {
  const block = mdBlocks.value.find((item) => item.kind === 'frontmatter')
  return block && block.kind === 'frontmatter' ? parseFrontmatter(block.text) : []
})
const manifestJson = computed(() => JSON.stringify(detail.value?.manifest ?? {}, null, 2))

const relatedOps = ref<OperationView[]>([])
const relatedLoading = ref(false)

const distributeOpen = ref(false)
const selectedAgents = ref<string[]>([])
const distributeStrategy = ref<'default' | InstallStrategy>('default')
const distributeVersionId = ref<string>('')
const distributeBusy = ref(false)
const distributeError = ref<string | null>(null)
const distributeImpacts = ref<UpdateImpact[]>([])

const uninstallOpen = ref(false)
const selectedInstallations = ref<string[]>([])
const uninstallBusy = ref(false)
const uninstallError = ref<string | null>(null)

const plans = ref<OperationPlan[] | null>(null)
const rollbackTarget = ref<OperationView | null>(null)
const rollingBack = ref(false)
const rollbackError = ref<string | null>(null)

const agentChoices = computed(() => app.agents)
const verificationTargets = computed(() =>
  agentChoices.value.filter(
    (agent) => selectedAgents.value.includes(agent.id) && agent.status === 'verification_required',
  ),
)

onMounted(() => {
  void load()
  void loadRelatedOps()
})

watch(skillId, () => {
  void load()
  void loadRelatedOps()
})

async function load(silent = false): Promise<void> {
  if (!skillId.value) return
  if (!silent) loading.value = true
  error.value = null
  try {
    detail.value = await api.skillGet(skillId.value)
  } catch (err) {
    error.value = normalizeError(err, t('errors.skillFailed'))
  } finally {
    loading.value = false
  }
}

async function loadRelatedOps(): Promise<void> {
  if (!skillId.value) return
  relatedLoading.value = true
  try {
    const operations = await api.operationsList(80)
    relatedOps.value = operations.filter(
      (operation) =>
        operation.canRollback &&
        (operation.plan?.items ?? []).some((item) => item.capabilityId === skillId.value),
    )
  } catch {
    relatedOps.value = []
  } finally {
    relatedLoading.value = false
  }
}

function mdHeadingTag(level: number): string {
  return `h${Math.min(Math.max(level, 1), 6)}`
}

function back(): void {
  void router.push({ name: 'skills' })
}

function openDistribute(): void {
  selectedAgents.value = []
  distributeStrategy.value = 'default'
  distributeVersionId.value = detail.value?.skill.currentVersionId ?? ''
  distributeImpacts.value = []
  distributeError.value = null
  distributeOpen.value = true
}

/** One update-impact preview per distinct installed version that differs from the target. */
async function collectDistributeImpacts(targetVersionId: string): Promise<UpdateImpact[]> {
  const current = skill.value
  if (!current || targetVersionId.length === 0) return []
  const seen = new Set<string>()
  const collected: UpdateImpact[] = []
  for (const agentId of selectedAgents.value) {
    const installation = current.installations.find((item) => item.agentId === agentId)
    if (!installation || installation.versionId === targetVersionId) continue
    if (seen.has(installation.versionId)) continue
    seen.add(installation.versionId)
    collected.push(
      await api.skillsPreviewUpdate({
        capabilityId: current.id,
        fromVersionId: installation.versionId,
        toVersionId: targetVersionId,
      }),
    )
  }
  return collected
}

async function previewDistribute(): Promise<void> {
  if (!skill.value) return
  if (selectedAgents.value.length === 0) {
    distributeError.value = t('skillDetail.noAgentsSelected')
    return
  }
  distributeBusy.value = true
  distributeError.value = null
  distributeImpacts.value = []
  try {
    const targetVersionId = distributeVersionId.value || skill.value.currentVersionId
    const impactList = await collectDistributeImpacts(targetVersionId)
    const plan = await api.planInstall({
      capabilityId: skill.value.id,
      versionId: distributeVersionId.value || null,
      agentIds: selectedAgents.value,
      strategy: distributeStrategy.value === 'default' ? null : distributeStrategy.value,
      force: false,
    })
    distributeImpacts.value = impactList
    plans.value = [plan]
    distributeOpen.value = false
  } catch (err) {
    distributeError.value = normalizeError(err, t('errors.planFailed'))
  } finally {
    distributeBusy.value = false
  }
}

function openUninstall(): void {
  selectedInstallations.value = []
  uninstallError.value = null
  uninstallOpen.value = true
}

async function previewUninstall(): Promise<void> {
  if (selectedInstallations.value.length === 0) {
    uninstallError.value = t('skillDetail.noInstallationsSelected')
    return
  }
  uninstallBusy.value = true
  uninstallError.value = null
  try {
    const plan = await api.planUninstall({
      installationIds: selectedInstallations.value,
      force: false,
    })
    plans.value = [plan]
    uninstallOpen.value = false
  } catch (err) {
    uninstallError.value = normalizeError(err, t('errors.planFailed'))
  } finally {
    uninstallBusy.value = false
  }
}

async function afterApplied(): Promise<void> {
  await load(true)
  await loadRelatedOps()
  await store.refresh().catch(() => undefined)
  await app.refreshAgents().catch(() => undefined)
  await app.loadDashboard(true).catch(() => undefined)
}

async function confirmRollback(): Promise<void> {
  const operation = rollbackTarget.value
  if (!operation) return
  rollingBack.value = true
  rollbackError.value = null
  try {
    const result = await api.rollbackOperation(operation.id)
    ui.notify('success', t('skillDetail.rollbackDone'), {
      detail: t(operationStatusKey[result.status]),
    })
    rollbackTarget.value = null
    await afterApplied()
  } catch (err) {
    rollbackError.value = normalizeError(err, t('errors.rollbackFailed'))
  } finally {
    rollingBack.value = false
  }
}

function installationTone(state: InstallationState) {
  return toneForInstallationState(state)
}

function installationStateLabel(state: InstallationState): string {
  return t(installationStateKey[state])
}
</script>

<template>
  <div class="page">
    <PageHeader
      :title="skill?.name ?? t('nav.skills')"
      :subtitle="skill?.description || t('skillDetail.loading')"
    >
      <template #meta>
        <div v-if="skill" class="row row-wrap" style="margin-top: 7px; gap: 5px">
          <button type="button" class="btn btn-ghost btn-sm" @click="back">
            <Icon name="arrow-left" :size="12" />
            {{ t('skillDetail.backToSkills') }}
          </button>
          <span class="chip">{{ skill.version }}</span>
          <span class="badge badge-neutral">
            {{ t('skills.installCount', { count: skill.installations.length }) }}
          </span>
          <StatusBadge v-if="skill.updateAvailable" :label="t('skills.updateAvailable')" tone="info" />
          <StatusBadge
            v-if="skill.driftCount > 0"
            :label="`${t('skills.drift')} ${skill.driftCount}`"
            tone="warn"
          />
          <StatusBadge v-if="skill.conflict" :label="t('skills.conflict')" tone="warn" />
        </div>
      </template>
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="load()">
          <span v-if="loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="!skill" @click="openDistribute">
          <Icon name="upload" :size="13" />
          {{ t('skillDetail.distribute') }}
        </button>
        <button
          type="button"
          class="btn"
          :disabled="!skill || skill.installations.length === 0"
          @click="openUninstall"
        >
          <Icon name="trash" :size="13" />
          {{ t('skillDetail.uninstall') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.skillFailed')"
        :retry-label="t('common.retry')"
        @retry="load()"
      />
      <ErrorBanner v-if="rollbackError" :message="rollbackError" :title="t('errors.rollbackFailed')" />

      <LoadingState v-if="loading && !detail" :label="t('skillDetail.loading')" />

      <template v-else-if="detail && skill">
        <SkillHealthPanel :skill="skill" :versions="detail.versions" />

        <div class="grid-2">
          <div class="section-stack">
            <div class="card">
              <div class="card-header">
                <div>
                  <p class="card-title">{{ t('skillDetail.skillMd') }}</p>
                  <p class="card-subtitle mono">{{ shortHash(skill.contentHash, 16) }}</p>
                </div>
                <span class="badge badge-outline">{{ t('skillDetail.current') }} {{ skill.version }}</span>
              </div>
              <div class="card-body">
                <div v-if="mdBlocks.length === 0" class="hint">{{ t('skillDetail.noSkillMd') }}</div>
                <div v-else class="skill-md">
                  <template v-for="(block, index) in mdBlocks" :key="index">
                    <pre v-if="block.kind === 'frontmatter'" class="frontmatter-block">{{ block.text }}</pre>
                    <component v-else-if="block.kind === 'heading'" :is="mdHeadingTag(block.level)">
                      {{ block.text }}
                    </component>
                    <div v-else-if="block.kind === 'code'" class="code-block">
                      <div v-if="block.lang" class="code-lang">{{ block.lang }}</div>
                      <pre>{{ block.code }}</pre>
                    </div>
                    <ul v-else-if="block.kind === 'list' && !block.ordered" class="md-list">
                      <li v-for="(item, itemIndex) in block.items" :key="itemIndex">{{ item }}</li>
                    </ul>
                    <ol v-else-if="block.kind === 'list'" class="md-list">
                      <li v-for="(item, itemIndex) in block.items" :key="itemIndex">{{ item }}</li>
                    </ol>
                    <blockquote v-else-if="block.kind === 'quote'" class="md-quote">
                      {{ block.text }}
                    </blockquote>
                    <p v-else-if="block.kind === 'text'" class="md-text">{{ block.text }}</p>
                  </template>
                </div>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('skillDetail.versionsTitle') }}</p>
                <span class="badge badge-neutral">{{ detail.versions.length }}</span>
              </div>
              <div class="card-body flush">
                <div v-if="detail.versions.length === 0" class="card-body hint">
                  {{ t('skillDetail.noVersions') }}
                </div>
                <table v-else class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('common.version') }}</th>
                      <th scope="col">{{ t('skillDetail.hash') }}</th>
                      <th scope="col" style="text-align: right">{{ t('common.size') }}</th>
                      <th scope="col" style="text-align: right">{{ t('common.files') }}</th>
                      <th scope="col">{{ t('common.createdAt') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="version in detail.versions" :key="version.id">
                      <td>
                        <div class="row" style="gap: 5px">
                          <span class="chip">{{ version.versionLabel }}</span>
                          <StatusBadge
                            v-if="version.isCurrent"
                            :label="t('skillDetail.current')"
                            tone="success"
                          />
                        </div>
                        <p v-if="version.notes" class="hint">{{ version.notes }}</p>
                      </td>
                      <td class="mono">{{ shortHash(version.contentHash) }}</td>
                      <td class="num">{{ formatBytes(version.sizeBytes) }}</td>
                      <td class="num">{{ version.fileCount }}</td>
                      <td class="text-xs">{{ formatDateTime(version.createdAt, locale) }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('skillDetail.filesTitle') }}</p>
                <span class="badge badge-neutral">{{ detail.files.length }}</span>
              </div>
              <div class="card-body flush">
                <div v-if="detail.files.length === 0" class="card-body hint">
                  {{ t('skillDetail.noFiles') }}
                </div>
                <div v-else class="list-scroll" style="max-height: 300px">
                  <table class="table table-compact">
                    <thead>
                      <tr>
                        <th scope="col">{{ t('common.path') }}</th>
                        <th scope="col" style="text-align: right">{{ t('common.size') }}</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="file in detail.files" :key="file.path">
                        <td class="path-text">{{ file.path }}</td>
                        <td class="num">{{ formatBytes(file.sizeBytes) }}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </div>

          <div class="section-stack">
            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('skillDetail.metadata') }}</p>
              </div>
              <div class="card-body">
                <dl class="definition-grid">
                  <dt>{{ t('common.name') }}</dt>
                  <dd>{{ skill.name }}</dd>
                  <dt>slug</dt>
                  <dd class="mono">{{ skill.slug }}</dd>
                  <dt>{{ t('common.version') }}</dt>
                  <dd>{{ skill.version }}</dd>
                  <dt>{{ t('skillDetail.hash') }}</dt>
                  <dd class="mono" :title="skill.contentHash">{{ shortHash(skill.contentHash, 16) }}</dd>
                  <dt>{{ t('common.size') }}</dt>
                  <dd>{{ formatBytes(skill.sizeBytes) }}</dd>
                  <dt>{{ t('common.files') }}</dt>
                  <dd>{{ skill.fileCount }}</dd>
                  <dt>{{ t('skillDetail.license') }}</dt>
                  <dd>{{ skill.license || '—' }}</dd>
                  <dt>{{ t('skillDetail.importedAt') }}</dt>
                  <dd>{{ formatDateTime(skill.createdAt, locale) }}</dd>
                  <dt>{{ t('common.updatedAt') }}</dt>
                  <dd>{{ formatDateTime(skill.updatedAt, locale) }}</dd>
                  <template v-if="skill.tags.length > 0">
                    <dt>{{ t('skillDetail.tags') }}</dt>
                    <dd>
                      <div class="row row-wrap" style="gap: 3px">
                        <span v-for="tag in skill.tags" :key="tag" class="tag">{{ tag }}</span>
                      </div>
                    </dd>
                  </template>
                  <template v-if="frontmatter.length > 0">
                    <dt>{{ t('skillDetail.frontmatter') }}</dt>
                    <dd>
                      <div class="stack-sm">
                        <p v-for="[key, value] in frontmatter" :key="key" class="hint">
                          <span class="mono">{{ key }}</span>: {{ value }}
                        </p>
                      </div>
                    </dd>
                  </template>
                </dl>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('skillDetail.sourceTitle') }}</p>
              </div>
              <div class="card-body">
                <dl v-if="skill.source" class="definition-grid">
                  <dt>{{ t('skillDetail.sourceType') }}</dt>
                  <dd><span class="chip">{{ skill.source.sourceType }}</span></dd>
                  <dt>{{ t('skillDetail.sourceUri') }}</dt>
                  <dd class="path-text">{{ skill.source.uri }}</dd>
                  <dt v-if="skill.source.reference">{{ t('skillDetail.sourceRef') }}</dt>
                  <dd v-if="skill.source.reference" class="mono">{{ skill.source.reference }}</dd>
                  <dt>{{ t('skillDetail.importedAt') }}</dt>
                  <dd>{{ formatDateTime(skill.source.importedAt, locale) }}</dd>
                </dl>
                <p v-else class="hint">—</p>
                <dl class="definition-grid" style="margin-top: 10px">
                  <dt>{{ t('skillDetail.libraryPath') }}</dt>
                  <dd class="path-text">{{ detail.versions.find((v) => v.isCurrent)?.storagePath ?? '—' }}</dd>
                </dl>
                <details class="surface-inset" style="margin-top: 10px; padding: 9px 11px">
                  <summary class="text-sm" style="cursor: pointer; font-weight: 600">
                    {{ t('skillDetail.manifest') }}
                  </summary>
                  <pre class="code-block" style="margin: 8px 0 0; max-height: 220px">{{ manifestJson }}</pre>
                </details>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('skillDetail.installsTitle') }}</p>
                <span class="badge badge-neutral">{{ skill.installations.length }}</span>
              </div>
              <div class="card-body stack-sm">
                <p v-if="skill.installations.length === 0" class="hint">
                  {{ t('skillDetail.noInstalls') }}
                </p>
                <div
                  v-for="installation in skill.installations"
                  :key="installation.id"
                  class="surface-inset"
                  style="padding: 9px"
                >
                  <div class="row" style="justify-content: space-between">
                    <span class="text-sm" style="font-weight: 600">{{ installation.agentName }}</span>
                    <StatusBadge
                      :label="installationStateLabel(installation.state)"
                      :tone="installationTone(installation.state)"
                    />
                  </div>
                  <p class="path-text" style="margin-top: 4px">{{ installation.targetPath }}</p>
                  <p class="hint">
                    {{ t('skillDetail.strategy') }}：{{ t(strategyKey[installation.strategy]) }} ·
                    {{ t('skillDetail.installedAt') }}：
                    {{ formatRelativeTime(installation.installedAt, locale) }}
                  </p>
                  <p class="hint mono" :title="installation.contentHash ?? ''">
                    {{ t('skillDetail.hash') }}：{{ shortHash(installation.contentHash) }}
                  </p>
                  <p v-if="installation.verifiedAt" class="hint">
                    {{ t('skillDetail.verifiedAt') }}：{{ formatDateTime(installation.verifiedAt, locale) }}
                  </p>
                  <p v-if="installation.drift" class="hint tone-warn">
                    {{ t('skillDetail.driftWarning') }}
                  </p>
                  <p v-if="installation.state === 'missing'" class="hint tone-danger">
                    {{ t('skillDetail.missingWarning') }}
                  </p>
                  <p v-for="(warning, index) in installation.warnings" :key="index" class="hint">
                    ⚠ {{ warning }}
                  </p>
                </div>
              </div>
            </div>

            <div class="card">
              <div class="card-header">
                <p class="card-title">{{ t('skillDetail.relatedOperations') }}</p>
                <button
                  type="button"
                  class="btn btn-ghost btn-sm"
                  :disabled="relatedLoading"
                  @click="loadRelatedOps"
                >
                  <span v-if="relatedLoading" class="spinner spinner-sm" aria-hidden="true" />
                  <Icon v-else name="refresh" :size="11" />
                </button>
              </div>
              <div class="card-body stack-sm">
                <p class="hint">{{ t('skillDetail.relatedOpsHint') }}</p>
                <p v-if="relatedOps.length === 0" class="hint">{{ t('skillDetail.noRelatedOps') }}</p>
                <div
                  v-for="operation in relatedOps"
                  :key="operation.id"
                  class="surface-inset"
                  style="padding: 8px"
                >
                  <div class="row row-wrap">
                    <StatusBadge
                      :label="t(operationStatusKey[operation.status])"
                      :tone="toneForOperationStatus(operation.status)"
                    />
                    <span class="text-sm" style="font-weight: 550">
                      {{ t(operationKindKey[operation.kind]) }}
                    </span>
                    <span class="spacer" />
                    <button type="button" class="btn btn-sm" @click="rollbackTarget = operation">
                      <Icon name="rollback" :size="11" />
                      {{ t('skillDetail.rollback') }}
                    </button>
                  </div>
                  <p class="hint truncate" :title="operation.summary">{{ operation.summary }}</p>
                  <p class="hint">{{ formatRelativeTime(operation.startedAt, locale) }}</p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>

      <div v-else-if="!loading" class="card">
        <div class="card-body">
          <p class="text-sm">{{ t('skillDetail.notFound') }}</p>
          <button type="button" class="btn btn-sm" style="margin-top: 10px" @click="back">
            {{ t('skillDetail.backToSkills') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Distribute -->
    <BaseModal
      v-if="distributeOpen"
      :title="t('skillDetail.distributionTitle')"
      :subtitle="t('skillDetail.distributionLead')"
      size="md"
      @close="distributeOpen = false"
    >
      <div class="stack-md">
        <div class="field">
          <span class="label">{{ t('skillDetail.chooseAgents') }}</span>
          <div v-if="agentChoices.length === 0" class="hint">{{ t('skillDetail.noAgentsAvailable') }}</div>
          <div v-else class="stack-sm list-scroll" style="max-height: 280px">
            <label
              v-for="agent in agentChoices"
              :key="agent.id"
              class="checkbox-row surface-inset"
              style="padding: 7px 9px"
              :style="{ opacity: isAgentTargetable(agent) ? 1 : 0.55 }"
            >
              <input
                v-model="selectedAgents"
                type="checkbox"
                class="checkbox"
                :value="agent.id"
                :disabled="!isAgentTargetable(agent)"
              />
              <span style="flex: 1">
                <span class="row" style="gap: 5px">
                  <span style="font-weight: 550">{{ agent.name }}</span>
                  <StatusBadge
                    :label="t(agentStatusKey[agent.status])"
                    :tone="toneForAgentStatus(agent.status)"
                  />
                </span>
                <span class="hint" style="display: block">
                  {{ agent.vendor }} · {{ t('agents.skillCount', { count: agent.skillCount }) }}
                </span>
              </span>
            </label>
          </div>
        </div>

        <div class="field">
          <label class="label" for="distribute-version">{{ t('skillDetail.chooseVersion') }}</label>
          <select id="distribute-version" v-model="distributeVersionId" class="select">
            <option v-for="version in detail?.versions ?? []" :key="version.id" :value="version.id">
              {{ version.versionLabel }}{{ version.isCurrent ? ` · ${t('skillDetail.current')}` : '' }}
            </option>
          </select>
        </div>

        <div class="field">
          <label class="label" for="distribute-strategy">{{ t('skillDetail.chooseStrategy') }}</label>
          <select id="distribute-strategy" v-model="distributeStrategy" class="select">
            <option value="default">{{ t('skillDetail.defaultStrategy') }}</option>
            <option value="copy">{{ t('status.strategy.copy') }}</option>
            <option value="symlink">{{ t('status.strategy.symlink') }}</option>
          </select>
        </div>

        <p class="hint">
          {{ t('skillDetail.selectedCount', { count: selectedAgents.length }) }}
        </p>
        <div v-if="verificationTargets.length > 0" class="notice notice-warn">
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <div class="notice-body">
            <p class="notice-title">{{ t('matrix.verificationHint') }}</p>
            <p>{{ verificationTargets.map((agent) => agent.name).join(' · ') }}</p>
          </div>
        </div>
        <ErrorBanner v-if="distributeError" :message="distributeError" :title="t('errors.planFailed')" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="distributeBusy" @click="distributeOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="distributeBusy || selectedAgents.length === 0"
          @click="previewDistribute"
        >
          <span v-if="distributeBusy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="eye" :size="13" />
          {{ t('skillDetail.installAction') }}
        </button>
      </template>
    </BaseModal>

    <!-- Uninstall -->
    <BaseModal
      v-if="uninstallOpen"
      :title="t('skillDetail.uninstallTitle')"
      :subtitle="t('skillDetail.uninstallLead')"
      size="md"
      @close="uninstallOpen = false"
    >
      <div class="stack-md">
        <div class="field">
          <span class="label">{{ t('skillDetail.chooseInstallations') }}</span>
          <div class="stack-sm">
            <label
              v-for="installation in skill?.installations ?? []"
              :key="installation.id"
              class="checkbox-row surface-inset"
              style="padding: 7px 9px"
            >
              <input
                v-model="selectedInstallations"
                type="checkbox"
                class="checkbox"
                :value="installation.id"
              />
              <span style="flex: 1">
                <span class="row" style="gap: 5px">
                  <span style="font-weight: 550">{{ installation.agentName }}</span>
                  <StatusBadge
                    :label="installationStateLabel(installation.state)"
                    :tone="installationTone(installation.state)"
                  />
                </span>
                <span class="path-text" style="display: block">{{ installation.targetPath }}</span>
              </span>
            </label>
          </div>
        </div>
        <p class="hint">{{ t('skillDetail.forceNote') }}</p>
        <ErrorBanner v-if="uninstallError" :message="uninstallError" :title="t('errors.planFailed')" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="uninstallBusy" @click="uninstallOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="uninstallBusy || selectedInstallations.length === 0"
          @click="previewUninstall"
        >
          <span v-if="uninstallBusy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="eye" :size="13" />
          {{ t('skillDetail.uninstallAction') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="rollbackTarget"
      :title="t('skillDetail.rollbackTitle')"
      :message="t('skillDetail.rollbackBody', { id: rollbackTarget.id })"
      :confirm-label="t('skillDetail.rollback')"
      danger
      :busy="rollingBack"
      @cancel="rollbackTarget = null"
      @confirm="confirmRollback"
    />

    <PlanDialog
      v-if="plans"
      :plans="plans"
      :impacts="distributeImpacts"
      @close="plans = null"
      @applied="afterApplied"
    />
  </div>
</template>
