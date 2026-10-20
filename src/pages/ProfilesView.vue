<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import BaseModal from '../components/BaseModal.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import PlanDialog from '../components/PlanDialog.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { formatDateTime } from '../lib/format'
import { capabilityKindKey, isAgentTargetable } from '../lib/status'
import type { OperationPlan, ProfileView, Skill } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSkillsStore } from '../stores/skills'
import { useUiStore } from '../stores/ui'

interface DraftItem {
  capabilityId: string
  enabled: boolean
  targetAgentKeys: string[]
}

const { t, locale } = useI18n()
const app = useAppStore()
const skillsStore = useSkillsStore()
const ui = useUiStore()

const profiles = ref<ProfileView[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const plans = ref<OperationPlan[] | null>(null)
const applyingId = ref<string | null>(null)
const applyError = ref<string | null>(null)

const skills = computed(() => skillsStore.skills)
const targetAgents = computed(() => app.agents.filter(isAgentTargetable))
const skillsLoading = computed(() => skillsStore.loading && skillsStore.skills.length === 0)

interface DraftRow {
  item: DraftItem
  skill: Skill
}

const draftRows = computed<DraftRow[]>(() =>
  draft.items
    .map((item) => {
      const skill = skills.value.find((candidate) => candidate.id === item.capabilityId)
      return skill ? { item, skill } : null
    })
    .filter((row): row is DraftRow => row !== null),
)

const editorOpen = ref(false)
const editingId = ref<string | null>(null)
const originalItemIds = ref<Set<string>>(new Set())
const saving = ref(false)
const editorError = ref<string | null>(null)
const draft = reactive({
  name: '',
  description: '',
  items: [] as DraftItem[],
})

const deleteTarget = ref<ProfileView | null>(null)
const deleting = ref(false)
const deleteError = ref<string | null>(null)

onMounted(async () => {
  loading.value = true
  try {
    await Promise.all([loadProfiles(), skillsStore.load()])
  } catch (err) {
    error.value = normalizeError(err, t('errors.profilesFailed'))
  } finally {
    loading.value = false
  }
})

async function loadProfiles(): Promise<void> {
  profiles.value = await api.profilesList()
}

async function refreshProfiles(): Promise<void> {
  try {
    await loadProfiles()
  } catch (err) {
    error.value = normalizeError(err, t('errors.profilesFailed'))
    ui.notifyError(t('errors.profilesFailed'), normalizeError(err))
  }
}

async function applyProfile(profile: ProfileView): Promise<void> {
  applyingId.value = profile.id
  applyError.value = null
  try {
    const plan = await api.profilesPlanApply(profile.id)
    plans.value = [plan]
  } catch (err) {
    applyError.value = normalizeError(err, t('profiles.applyFailed'))
    ui.notifyError(t('profiles.applyFailed'), applyError.value)
  } finally {
    applyingId.value = null
  }
}

function openEditor(profile?: ProfileView): void {
  editingId.value = profile?.id ?? null
  draft.name = profile?.name ?? ''
  draft.description = profile?.description ?? ''
  const existing = new Map((profile?.items ?? []).map((item) => [item.capabilityId, item]))
  originalItemIds.value = new Set(existing.keys())
  draft.items = skills.value.map((skill) => {
    const item = existing.get(skill.id)
    return {
      capabilityId: skill.id,
      enabled: item?.enabled ?? false,
      targetAgentKeys: [...(item?.targetAgentKeys ?? [])],
    }
  })
  editorError.value = null
  editorOpen.value = true
}

function toggleAgent(item: DraftItem, agentKey: string): void {
  if (item.targetAgentKeys.includes(agentKey)) {
    item.targetAgentKeys = item.targetAgentKeys.filter((key) => key !== agentKey)
  } else {
    item.targetAgentKeys = [...item.targetAgentKeys, agentKey]
  }
}

async function save(): Promise<void> {
  if (!draft.name.trim()) {
    editorError.value = t('profiles.name')
    return
  }
  saving.value = true
  editorError.value = null
  try {
    const items = draft.items
      .filter((item) => item.enabled || originalItemIds.value.has(item.capabilityId))
      .map((item) => ({
        capabilityId: item.capabilityId,
        enabled: item.enabled,
        targetAgentKeys: item.targetAgentKeys,
      }))
    await api.profilesUpsert({
      id: editingId.value,
      name: draft.name.trim(),
      description: draft.description.trim() || null,
      items,
    })
    editorOpen.value = false
    ui.notify('success', t('profiles.saved'), { detail: draft.name.trim() })
    await loadProfiles()
  } catch (err) {
    editorError.value = normalizeError(err, t('errors.saveFailed'))
  } finally {
    saving.value = false
  }
}

async function confirmDelete(): Promise<void> {
  const profile = deleteTarget.value
  if (!profile) return
  deleting.value = true
  deleteError.value = null
  try {
    await api.profilesDelete(profile.id)
    deleteTarget.value = null
    ui.notify('success', t('profiles.delete'), { detail: profile.name })
    await loadProfiles()
  } catch (err) {
    deleteError.value = normalizeError(err, t('errors.deleteFailed'))
    ui.notifyError(t('errors.deleteFailed'), deleteError.value)
  } finally {
    deleting.value = false
  }
}

async function afterApplied(): Promise<void> {
  await Promise.allSettled([skillsStore.refresh(), app.refreshAgents(), app.loadDashboard(true)])
}

function canApplyProfile(profile: ProfileView): boolean {
  const hasEnabledSkill = profile.items.some((item) => item.enabled && item.kind === 'skill')
  return hasEnabledSkill && targetAgents.value.length > 0
}

function itemSummary(profile: ProfileView): string {
  return t('profiles.itemCount', { count: profile.items.length })
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('profiles.title')" :subtitle="t('profiles.subtitle')">
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="refreshProfiles">
          <Icon name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="skillsLoading" @click="openEditor()">
          <Icon name="plus" :size="13" />
          {{ t('profiles.create') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.profilesFailed')"
        :retry-label="t('common.retry')"
        @retry="refreshProfiles"
      />
      <ErrorBanner v-if="applyError" :message="applyError" :title="t('errors.planFailed')" />

      <div class="notice notice-info">
        <Icon class="notice-icon" name="info" :size="14" />
        <div class="notice-body">
          <p>{{ t('profiles.plansHint') }}</p>
          <p class="hint" style="margin-top: 3px">{{ t('profiles.skillOnlyNote') }}</p>
        </div>
      </div>

      <LoadingState v-if="loading && profiles.length === 0" :label="t('common.loadingData')" />

      <EmptyState
        v-else-if="profiles.length === 0"
        icon="profiles"
        :title="t('profiles.emptyTitle')"
        :body="t('profiles.emptyBody')"
      >
        <template #actions>
          <button type="button" class="btn btn-primary" @click="openEditor()">
            <Icon name="plus" :size="13" />
            {{ t('profiles.create') }}
          </button>
        </template>
      </EmptyState>

      <div v-else class="grid-cards">
        <article v-for="profile in profiles" :key="profile.id" class="card">
          <div class="card-body stack-md">
            <div class="row row-wrap" style="justify-content: space-between; align-items: flex-start">
              <div style="min-width: 0">
                <div class="row row-wrap" style="gap: 5px">
                  <h3 style="font-size: 13px; font-weight: 650">{{ profile.name }}</h3>
                  <span class="badge" :class="profile.isBuiltin ? 'badge-outline' : 'badge-accent'">
                    {{ profile.isBuiltin ? t('profiles.builtin') : t('profiles.custom') }}
                  </span>
                  <span class="badge badge-neutral">{{ itemSummary(profile) }}</span>
                </div>
                <p class="hint" style="margin-top: 3px">{{ profile.description || '—' }}</p>
              </div>
              <div class="row">
                <button
                  type="button"
                  class="btn btn-sm"
                  :disabled="applyingId === profile.id || !canApplyProfile(profile)"
                  :title="canApplyProfile(profile) ? t('profiles.plansHint') : t('profiles.applyDisabled')"
                  @click="applyProfile(profile)"
                >
                  <span
                    v-if="applyingId === profile.id"
                    class="spinner spinner-sm"
                    aria-hidden="true"
                  />
                  <Icon v-else name="play" :size="11" />
                  {{ t('profiles.apply') }}
                </button>
                <button type="button" class="btn btn-sm btn-ghost" @click="openEditor(profile)">
                  <Icon name="edit" :size="11" />
                  {{ t('profiles.edit') }}
                </button>
                <button
                  v-if="!profile.isBuiltin"
                  type="button"
                  class="btn btn-sm btn-ghost"
                  :title="t('profiles.deleteBuiltinDisabled')"
                  @click="deleteTarget = profile"
                >
                  <Icon name="trash" :size="11" />
                </button>
              </div>
            </div>

            <div v-if="profile.items.length === 0" class="hint">{{ t('profiles.noItems') }}</div>
            <ul v-else class="list-reset stack-sm">
              <li v-for="item in profile.items" :key="item.capabilityId" class="surface-inset" style="padding: 7px 9px">
                <div class="row row-wrap">
                  <span class="badge badge-outline">{{ t(capabilityKindKey[item.kind]) }}</span>
                  <span class="text-sm" style="font-weight: 550">{{ item.name }}</span>
                  <StatusBadge
                    :label="item.enabled ? t('profiles.enabled') : t('profiles.disabled')"
                    :tone="item.enabled ? 'success' : 'neutral'"
                  />
                </div>
                <p class="hint" style="margin-top: 2px">
                  {{ t('profiles.targets') }}：
                  <template v-if="item.targetAgentKeys.length === 0">
                    {{ t('profiles.allAgents') }}
                  </template>
                  <template v-else>
                    <span v-for="key in item.targetAgentKeys" :key="key" class="chip" style="margin-right: 3px">
                      {{ app.agents.find((agent) => agent.key === key)?.name ?? key }}
                    </span>
                  </template>
                </p>
              </li>
            </ul>

            <p class="hint">
              {{ t('profiles.updatedAt') }}：{{ formatDateTime(profile.updatedAt, locale) }}
            </p>
          </div>
        </article>
      </div>

      <p class="hint">{{ t('profiles.builtinNote') }}</p>
    </div>

    <BaseModal
      v-if="editorOpen"
      :title="editingId ? t('profiles.editorTitleEdit') : t('profiles.editorTitleCreate')"
      :subtitle="t('profiles.skillsHint')"
      size="xl"
      @close="editorOpen = false"
    >
      <div class="stack-md">
        <div class="row" style="align-items: flex-start">
          <div class="field" style="flex: 1">
            <label class="label" for="profile-name">{{ t('profiles.name') }}</label>
            <input id="profile-name" v-model="draft.name" class="input" type="text" />
            <p class="hint">{{ t('profiles.nameHint') }}</p>
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="profile-description">{{ t('profiles.description') }}</label>
            <input id="profile-description" v-model="draft.description" class="input" type="text" />
          </div>
        </div>

        <div class="notice notice-info">
          <Icon class="notice-icon" name="info" :size="14" />
          <p>{{ t('profiles.skillOnlyNote') }}</p>
        </div>

        <div class="field">
          <span class="label">{{ t('profiles.skillsTitle') }}</span>
          <div v-if="skills.length === 0" class="hint">{{ t('profiles.noSkills') }}</div>
          <div v-else class="list-scroll surface-inset" style="max-height: 420px; padding: 8px">
            <div
              v-for="row in draftRows"
              :key="row.item.capabilityId"
              class="stack-sm"
              style="padding: 7px 0"
            >
              <label class="checkbox-row">
                <input v-model="row.item.enabled" type="checkbox" class="checkbox" />
                <span style="flex: 1">
                  <span class="row" style="gap: 5px">
                    <span style="font-weight: 550">{{ row.skill.name }}</span>
                    <span class="chip">{{ row.skill.version }}</span>
                    <span class="hint">
                      {{ t('matrix.installedCount', { count: row.skill.installations.length }) }}
                    </span>
                  </span>
                  <span class="hint" style="display: block">{{ row.skill.description || '—' }}</span>
                </span>
              </label>
              <div class="row row-wrap" style="gap: 4px; padding-left: 22px">
                <span class="hint">{{ t('profiles.targets') }}：</span>
                <button
                  v-for="agent in targetAgents"
                  :key="agent.id"
                  type="button"
                  class="btn btn-sm"
                  :class="{ 'btn-primary': row.item.targetAgentKeys.includes(agent.key) }"
                  @click="toggleAgent(row.item, agent.key)"
                >
                  {{ agent.name }}
                </button>
                <span v-if="row.item.targetAgentKeys.length === 0" class="hint">
                  {{ t('profiles.targetAllHint') }}
                </span>
              </div>
            </div>
          </div>
          <p class="hint">{{ t('profiles.targetAllHint') }}</p>
        </div>

        <ErrorBanner v-if="editorError" :message="editorError" :title="t('errors.saveFailed')" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="saving" @click="editorOpen = false">
          {{ t('common.cancel') }}
        </button>
        <button type="button" class="btn btn-primary" :disabled="saving" @click="save">
          <span v-if="saving" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="check" :size="13" />
          {{ t('profiles.save') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="deleteTarget"
      :title="t('profiles.deleteTitle')"
      :message="t('profiles.deleteBody', { name: deleteTarget.name })"
      :confirm-label="t('profiles.deleteConfirm')"
      danger
      :busy="deleting"
      @cancel="deleteTarget = null"
      @confirm="confirmDelete"
    />
    <ErrorBanner v-if="deleteError" :message="deleteError" :title="t('errors.deleteFailed')" />

    <PlanDialog v-if="plans" :plans="plans" @close="plans = null" @applied="afterApplied" />
  </div>
</template>
