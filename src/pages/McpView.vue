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
import {
  formatDateTime,
  formatRelativeTime,
  parseKeyValueLines,
  splitLines,
} from '../lib/format'
import {
  agentStatusKey,
  installationStateKey,
  isAgentTargetable,
  toneForAgentStatus,
  toneForInstallationState,
  transportKey,
} from '../lib/status'
import type {
  DiscoveredMcp,
  McpServerView,
  McpTransport,
  McpUpsertRequest,
  OperationPlan,
  SecretState,
} from '../lib/types'
import { useAppStore } from '../stores/app'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const app = useAppStore()
const ui = useUiStore()

const servers = ref<McpServerView[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const plans = ref<OperationPlan[] | null>(null)

const transports: McpTransport[] = ['stdio', 'http', 'sse', 'streamable_http']
const targetAgents = computed(() => app.agents.filter(isAgentTargetable))
const keychainAvailable = computed(() => app.settings?.keychainAvailable ?? false)

// -- list -------------------------------------------------------------------

async function load(): Promise<void> {
  loading.value = true
  error.value = null
  try {
    servers.value = await api.mcpList()
  } catch (err) {
    error.value = normalizeError(err, t('errors.mcpFailed'))
  } finally {
    loading.value = false
  }
}

onMounted(load)

async function refresh(): Promise<void> {
  await load()
}

// -- editor -----------------------------------------------------------------

const editorOpen = ref(false)
const editing = ref<McpServerView | null>(null)
const saving = ref(false)
const editorError = ref<string | null>(null)

const form = reactive({
  name: '',
  description: '',
  transport: 'stdio' as McpTransport,
  command: '',
  argsText: '',
  url: '',
  headersText: '',
  envKeysText: '',
  credentialRefsText: '',
})

function resetForm(server: McpServerView | null): void {
  form.name = server?.name ?? ''
  form.description = server?.description ?? ''
  form.transport = server?.transport ?? 'stdio'
  form.command = server?.command ?? ''
  form.argsText = (server?.args ?? []).join('\n')
  form.url = server?.url ?? ''
  form.headersText = Object.entries(server?.headers ?? {})
    .map(([key, value]) => `${key}: ${value}`)
    .join('\n')
  form.envKeysText = (server?.envKeys ?? []).join('\n')
  form.credentialRefsText = (server?.credentialRefs ?? []).join('\n')
}

function openCreate(): void {
  editing.value = null
  editorError.value = null
  resetForm(null)
  editorOpen.value = true
}

function openEdit(server: McpServerView): void {
  editing.value = server
  editorError.value = null
  resetForm(server)
  editorOpen.value = true
}

async function save(): Promise<void> {
  const name = form.name.trim()
  if (!name) {
    editorError.value = t('mcp.validationName')
    return
  }
  const isStdio = form.transport === 'stdio'
  if (isStdio && !form.command.trim()) {
    editorError.value = t('mcp.validationStdio')
    return
  }
  if (!isStdio && !form.url.trim()) {
    editorError.value = t('mcp.validationHttp')
    return
  }
  let headers: Record<string, string> = {}
  if (!isStdio && form.headersText.trim()) {
    try {
      headers = parseKeyValueLines(form.headersText)
    } catch (err) {
      editorError.value = normalizeError(err, t('mcp.argsInvalid'))
      return
    }
  }
  const request: McpUpsertRequest = {
    id: editing.value?.id ?? null,
    name,
    description: form.description.trim() || null,
    transport: form.transport,
    command: isStdio ? form.command.trim() || null : null,
    args: splitLines(form.argsText),
    url: isStdio ? null : form.url.trim() || null,
    headers,
    envKeys: splitLines(form.envKeysText),
    credentialRefs: splitLines(form.credentialRefsText),
  }
  saving.value = true
  editorError.value = null
  try {
    await api.mcpUpsert(request)
    editorOpen.value = false
    ui.notify('success', t('mcp.saved'), { detail: name })
    await load()
  } catch (err) {
    editorError.value = normalizeError(err, t('errors.saveFailed'))
  } finally {
    saving.value = false
  }
}

// -- discovery --------------------------------------------------------------

const discoverOpen = ref(false)
const discovered = ref<DiscoveredMcp[]>([])
const discoverLoading = ref(false)
const discoverError = ref<string | null>(null)
const importingKey = ref<string | null>(null)

function discoveryKey(item: DiscoveredMcp): string {
  return `${item.agentId}:${item.targetPath}:${item.name}`
}

async function loadDiscovery(): Promise<void> {
  discoverLoading.value = true
  discoverError.value = null
  try {
    discovered.value = await api.mcpDiscover()
  } catch (err) {
    discoverError.value = normalizeError(err, t('errors.discoverFailed'))
  } finally {
    discoverLoading.value = false
  }
}

function openDiscover(): void {
  discoverOpen.value = true
  void loadDiscovery()
}

async function importDiscovered(item: DiscoveredMcp): Promise<void> {
  const key = discoveryKey(item)
  importingKey.value = key
  discoverError.value = null
  try {
    await api.mcpImport({
      agentId: item.agentId,
      targetPath: item.targetPath,
      name: item.name,
    })
    ui.notify('success', t('mcp.saved'), { detail: item.name })
    await Promise.all([load(), loadDiscovery()])
  } catch (err) {
    discoverError.value = normalizeError(err, t('errors.saveFailed'))
  } finally {
    importingKey.value = null
  }
}

// -- secrets ----------------------------------------------------------------

const secretServer = ref<McpServerView | null>(null)
const secretStates = ref<SecretState[]>([])
const secretLoading = ref(false)
const secretError = ref<string | null>(null)
const secretBusyKey = ref<string | null>(null)
const secretValues = reactive<Record<string, string>>({})
const deleteSecretKey = ref<string | null>(null)

function closeSecrets(): void {
  secretServer.value = null
  secretError.value = null
  for (const key of Object.keys(secretValues)) delete secretValues[key]
}

async function openSecrets(server: McpServerView): Promise<void> {
  secretServer.value = server
  secretError.value = null
  secretStates.value = []
  for (const key of Object.keys(secretValues)) delete secretValues[key]
  await loadSecrets()
}

async function loadSecrets(): Promise<void> {
  const server = secretServer.value
  if (!server) return
  secretLoading.value = true
  secretError.value = null
  try {
    secretStates.value = await api.mcpSecretStates(server.id)
  } catch (err) {
    secretError.value = normalizeError(err, t('errors.secretFailed'))
  } finally {
    secretLoading.value = false
  }
}

async function setSecret(key: string): Promise<void> {
  const server = secretServer.value
  if (!server) return
  const value = secretValues[key] ?? ''
  if (!value.trim()) {
    secretError.value = t('errors.invalidInput')
    return
  }
  secretBusyKey.value = key
  secretError.value = null
  try {
    await api.mcpSetSecret(server.id, key, value)
    secretValues[key] = ''
    ui.notify('success', t('mcp.secretSaved'), { detail: key })
    await Promise.all([loadSecrets(), load()])
  } catch (err) {
    secretError.value = normalizeError(err, t('errors.secretFailed'))
  } finally {
    secretBusyKey.value = null
  }
}

async function deleteSecret(): Promise<void> {
  const server = secretServer.value
  const key = deleteSecretKey.value
  if (!server || !key) return
  secretBusyKey.value = key
  secretError.value = null
  try {
    await api.mcpDeleteSecret(server.id, key)
    deleteSecretKey.value = null
    ui.notify('success', t('mcp.secretDeleted'), { detail: key })
    await Promise.all([loadSecrets(), load()])
  } catch (err) {
    secretError.value = normalizeError(err, t('errors.secretFailed'))
  } finally {
    secretBusyKey.value = null
  }
}

// -- distribute / remove ----------------------------------------------------

const distributeServer = ref<McpServerView | null>(null)
const distributeAgentIds = ref<string[]>([])
const distributeError = ref<string | null>(null)
const distributeBusy = ref(false)

function openDistribute(server: McpServerView): void {
  distributeServer.value = server
  distributeAgentIds.value = []
  distributeError.value = null
}

async function previewDistribute(): Promise<void> {
  const server = distributeServer.value
  if (!server) return
  if (distributeAgentIds.value.length === 0) {
    distributeError.value = t('errors.noSelection')
    return
  }
  distributeBusy.value = true
  distributeError.value = null
  try {
    const plan = await api.mcpPlanDistribute({
      serverId: server.id,
      agentIds: distributeAgentIds.value,
      force: false,
    })
    plans.value = [plan]
    distributeServer.value = null
  } catch (err) {
    distributeError.value = normalizeError(err, t('errors.planFailed'))
  } finally {
    distributeBusy.value = false
  }
}

const removeServer = ref<McpServerView | null>(null)
const removeAgentIds = ref<string[]>([])
const removeError = ref<string | null>(null)
const removeBusy = ref(false)

function openRemove(server: McpServerView): void {
  removeServer.value = server
  removeAgentIds.value = server.distributions.map((item) => item.agentId)
  removeError.value = null
}

async function previewRemove(): Promise<void> {
  const server = removeServer.value
  if (!server) return
  if (removeAgentIds.value.length === 0) {
    removeError.value = t('errors.noSelection')
    return
  }
  removeBusy.value = true
  removeError.value = null
  try {
    const plan = await api.mcpPlanRemove(server.id, removeAgentIds.value)
    plans.value = [plan]
    removeServer.value = null
  } catch (err) {
    removeError.value = normalizeError(err, t('errors.planFailed'))
  } finally {
    removeBusy.value = false
  }
}

async function afterApplied(): Promise<void> {
  await load()
  await app.loadDashboard(true).catch(() => undefined)
}

function distributionStateLabel(server: McpServerView, agentId: string): string {
  const distribution = server.distributions.find((item) => item.agentId === agentId)
  return distribution ? t(installationStateKey[distribution.state]) : '—'
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('mcp.title')" :subtitle="t('mcp.subtitle')">
      <template #meta>
        <p class="hint" style="margin-top: 6px">
          {{ servers.length }} {{ t('common.items') }} ·
          {{ keychainAvailable ? t('mcp.keychainAvailable') : t('mcp.keychainUnavailable') }}
        </p>
      </template>
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="refresh">
          <span v-if="loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('mcp.refresh') }}
        </button>
        <button type="button" class="btn" @click="openDiscover">
          <Icon name="scan" :size="13" />
          {{ t('mcp.discover') }}
        </button>
        <button type="button" class="btn btn-primary" @click="openCreate">
          <Icon name="plus" :size="13" />
          {{ t('mcp.new') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.mcpFailed')"
        :retry-label="t('common.retry')"
        @retry="load"
      />

      <div v-if="!keychainAvailable" class="notice notice-warn">
        <Icon class="notice-icon" name="key" :size="14" />
        <p>{{ t('mcp.keychainUnavailable') }}</p>
      </div>

      <LoadingState v-if="loading && servers.length === 0" :label="t('common.loadingData')" />

      <EmptyState
        v-else-if="servers.length === 0"
        icon="mcp"
        :title="t('mcp.emptyTitle')"
        :body="t('mcp.emptyBody')"
      >
        <template #actions>
          <button type="button" class="btn btn-primary" @click="openCreate">
            <Icon name="plus" :size="13" />
            {{ t('mcp.new') }}
          </button>
          <button type="button" class="btn" @click="openDiscover">
            <Icon name="scan" :size="13" />
            {{ t('mcp.discover') }}
          </button>
        </template>
      </EmptyState>

      <div v-else class="section-stack">
        <article v-for="server in servers" :key="server.id" class="card">
          <div class="card-body stack-md">
            <div class="row row-wrap" style="justify-content: space-between; align-items: flex-start">
              <div style="min-width: 0">
                <div class="row row-wrap" style="gap: 5px">
                  <h3 style="font-size: 13px; font-weight: 650">{{ server.name }}</h3>
                  <StatusBadge :label="t(transportKey[server.transport])" tone="info" :dot="false" />
                  <StatusBadge
                    v-if="server.issues.length > 0"
                    :label="`${t('mcp.issues')} ${server.issues.length}`"
                    tone="warn"
                  />
                </div>
                <p class="hint" style="margin-top: 3px">{{ server.description || '—' }}</p>
              </div>
              <div class="row row-wrap">
                <button type="button" class="btn btn-sm" @click="openEdit(server)">
                  <Icon name="edit" :size="12" />
                  {{ t('mcp.edit') }}
                </button>
                <button
                  type="button"
                  class="btn btn-sm"
                  :disabled="server.envKeys.length === 0"
                  @click="openSecrets(server)"
                >
                  <Icon name="key" :size="12" />
                  {{ t('mcp.credentials') }}
                </button>
                <button
                  type="button"
                  class="btn btn-sm"
                  :disabled="targetAgents.length === 0"
                  @click="openDistribute(server)"
                >
                  <Icon name="upload" :size="12" />
                  {{ t('mcp.distribute') }}
                </button>
                <button
                  type="button"
                  class="btn btn-sm"
                  :disabled="server.distributions.length === 0"
                  @click="openRemove(server)"
                >
                  <Icon name="trash" :size="12" />
                  {{ t('mcp.remove') }}
                </button>
              </div>
            </div>

            <div class="row row-wrap" style="gap: 5px">
              <span v-if="server.envKeys.length === 0" class="hint">{{ t('mcp.noEnv') }}</span>
              <span v-for="key in server.envKeys" :key="key" class="chip">
                <Icon name="key" :size="9" />
                {{ key }}
              </span>
              <span class="subtle">|</span>
              <span v-if="server.credentialRefs.length === 0" class="hint">
                {{ t('mcp.noCredentialRefs') }}
              </span>
              <span v-for="ref in server.credentialRefs" :key="ref" class="chip">{{ ref }}</span>
            </div>

            <div class="row row-wrap" style="gap: 12px">
              <p v-if="server.command" class="hint mono">
                {{ t('mcp.command') }}: {{ server.command }} {{ server.args.join(' ') }}
              </p>
              <p v-if="server.url" class="hint mono">{{ t('mcp.url') }}: {{ server.url }}</p>
              <span class="spacer" />
              <p class="hint">
                {{ t('mcp.created') }} {{ formatRelativeTime(server.createdAt, locale) }} ·
                {{ t('mcp.updated') }} {{ formatDateTime(server.updatedAt, locale) }}
              </p>
            </div>

            <div v-if="server.issues.length > 0" class="notice notice-warn">
              <Icon class="notice-icon" name="alert-triangle" :size="14" />
              <div class="notice-body">
                <p class="notice-title">{{ t('mcp.issues') }}</p>
                <p v-for="(issue, index) in server.issues" :key="index">{{ issue }}</p>
                <button
                  v-if="server.envKeys.length > 0"
                  type="button"
                  class="btn btn-sm"
                  style="margin-top: 6px"
                  @click="openSecrets(server)"
                >
                  <Icon name="key" :size="12" />
                  {{ t('mcp.credentials') }}
                </button>
              </div>
            </div>

            <div>
              <p class="section-title">{{ t('mcp.distributions') }}</p>
              <p v-if="server.distributions.length === 0" class="hint">
                {{ t('mcp.noDistributions') }}
              </p>
              <div v-else class="table-wrap surface-inset">
                <table class="table table-compact">
                  <thead>
                    <tr>
                      <th scope="col">{{ t('mcp.discoverAgent') }}</th>
                      <th scope="col">{{ t('mcp.installState') }}</th>
                      <th scope="col">{{ t('mcp.configKey') }}</th>
                      <th scope="col">{{ t('mcp.targetPath') }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="distribution in server.distributions" :key="distribution.installationId ?? distribution.agentId">
                      <td>{{ distribution.agentName || distribution.agentKey }}</td>
                      <td>
                        <StatusBadge
                          :label="t(installationStateKey[distribution.state])"
                          :tone="toneForInstallationState(distribution.state)"
                        />
                      </td>
                      <td><span class="chip">{{ distribution.configKey }}</span></td>
                      <td class="path-text">{{ distribution.targetPath }}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <p class="hint" style="margin-top: 5px">{{ t('mcp.distributionsHint') }}</p>
            </div>
          </div>
        </article>
      </div>
    </div>

    <!-- Editor -->
    <BaseModal
      v-if="editorOpen"
      :title="editing ? t('mcp.editorTitleEdit') : t('mcp.editorTitleCreate')"
      :subtitle="t('mcp.editHint')"
      size="lg"
      @close="editorOpen = false"
    >
      <div class="stack-md">
        <div class="row" style="align-items: flex-start">
          <div class="field" style="flex: 1">
            <label class="label" for="mcp-name">{{ t('mcp.formName') }}</label>
            <input id="mcp-name" v-model="form.name" class="input" type="text" spellcheck="false" />
            <p class="hint">{{ t('mcp.formNameHint') }}</p>
          </div>
          <div class="field" style="width: 180px; flex: none">
            <label class="label" for="mcp-transport">{{ t('mcp.formTransport') }}</label>
            <select id="mcp-transport" v-model="form.transport" class="select">
              <option v-for="transport in transports" :key="transport" :value="transport">
                {{ t(transportKey[transport]) }}
              </option>
            </select>
          </div>
        </div>

        <div class="field">
          <label class="label" for="mcp-description">{{ t('mcp.formDescription') }}</label>
          <input id="mcp-description" v-model="form.description" class="input" type="text" />
        </div>

        <template v-if="form.transport === 'stdio'">
          <div class="field">
            <label class="label" for="mcp-command">{{ t('mcp.formCommand') }}</label>
            <input
              id="mcp-command"
              v-model="form.command"
              class="input input-mono"
              type="text"
              spellcheck="false"
            />
            <p class="hint">{{ t('mcp.formCommandHint') }}</p>
          </div>
          <div class="field">
            <label class="label" for="mcp-args">{{ t('mcp.formArgs') }}</label>
            <textarea
              id="mcp-args"
              v-model="form.argsText"
              class="textarea textarea-mono"
              rows="3"
            />
            <p class="hint">{{ t('mcp.formArgsHint') }}</p>
          </div>
          <div class="notice notice-warn">
            <Icon class="notice-icon" name="mcp" :size="14" />
            <p>{{ t('mcp.stdioWarning') }}</p>
          </div>
        </template>

        <template v-else>
          <div class="field">
            <label class="label" for="mcp-url">{{ t('mcp.formUrl') }}</label>
            <input
              id="mcp-url"
              v-model="form.url"
              class="input input-mono"
              type="text"
              spellcheck="false"
              placeholder="https://…"
            />
            <p class="hint">{{ t('mcp.formUrlHint') }}</p>
          </div>
          <div class="field">
            <label class="label" for="mcp-headers">{{ t('mcp.formHeaders') }}</label>
            <textarea
              id="mcp-headers"
              v-model="form.headersText"
              class="textarea textarea-mono"
              rows="2"
              placeholder="Authorization: Bearer …"
            />
            <p class="hint">{{ t('mcp.formHeadersHint') }}</p>
          </div>
        </template>

        <div v-if="form.transport !== 'stdio'" class="notice notice-warn">
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <p>{{ t('mcp.headersWarning') }}</p>
        </div>

        <div class="row" style="align-items: flex-start">
          <div class="field" style="flex: 1">
            <label class="label" for="mcp-env">{{ t('mcp.formEnvKeys') }}</label>
            <textarea
              id="mcp-env"
              v-model="form.envKeysText"
              class="textarea textarea-mono"
              rows="3"
            />
            <p class="hint">{{ t('mcp.formEnvKeysHint') }}</p>
          </div>
          <div class="field" style="flex: 1">
            <label class="label" for="mcp-refs">{{ t('mcp.formCredentialRefs') }}</label>
            <textarea
              id="mcp-refs"
              v-model="form.credentialRefsText"
              class="textarea textarea-mono"
              rows="3"
            />
            <p class="hint">{{ t('mcp.formCredentialRefsHint') }}</p>
          </div>
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
          {{ t('mcp.save') }}
        </button>
      </template>
    </BaseModal>

    <!-- Discovery -->
    <BaseModal
      v-if="discoverOpen"
      :title="t('mcp.discoverTitle')"
      :subtitle="t('mcp.discoverLead')"
      size="xl"
      @close="discoverOpen = false"
    >
      <div class="stack-md">
        <div class="row">
          <button type="button" class="btn btn-sm" :disabled="discoverLoading" @click="loadDiscovery">
            <span v-if="discoverLoading" class="spinner spinner-sm" aria-hidden="true" />
            <Icon v-else name="refresh" :size="12" />
            {{ t('common.refresh') }}
          </button>
          <span class="hint">{{ discovered.length }} {{ t('common.items') }}</span>
        </div>
        <ErrorBanner v-if="discoverError" :message="discoverError" :title="t('errors.discoverFailed')" />
        <LoadingState v-if="discoverLoading && discovered.length === 0" :label="t('common.loadingData')" />
        <EmptyState
          v-else-if="discovered.length === 0"
          icon="mcp"
          :title="t('mcp.discoverEmpty')"
        />
        <div v-else class="table-wrap surface-inset list-scroll" style="max-height: 480px">
          <table class="table table-compact">
            <thead>
              <tr>
                <th scope="col">{{ t('common.name') }}</th>
                <th scope="col">{{ t('mcp.discoverAgent') }}</th>
                <th scope="col">{{ t('mcp.discoverTransport') }}</th>
                <th scope="col">{{ t('mcp.discoverTarget') }}</th>
                <th scope="col" style="text-align: right">{{ t('common.actions') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="item in discovered" :key="discoveryKey(item)">
                <td>
                  <span style="font-weight: 550">{{ item.name }}</span>
                  <p v-if="item.command" class="hint mono">{{ item.command }} {{ item.args.join(' ') }}</p>
                  <p v-if="item.url" class="hint mono">{{ item.url }}</p>
                  <p v-for="(issue, index) in item.issues" :key="index" class="hint tone-danger">
                    {{ issue }}
                  </p>
                </td>
                <td>{{ item.agentName }}</td>
                <td><span class="chip">{{ t(transportKey[item.transport]) }}</span></td>
                <td class="path-text">{{ item.targetPath }}</td>
                <td style="text-align: right">
                  <StatusBadge v-if="item.alreadyKnown" :label="t('mcp.discoverKnown')" tone="success" />
                  <button
                    v-else
                    type="button"
                    class="btn btn-sm btn-primary"
                    :disabled="importingKey === discoveryKey(item) || item.issues.length > 0"
                    @click="importDiscovered(item)"
                  >
                    <span
                      v-if="importingKey === discoveryKey(item)"
                      class="spinner spinner-sm"
                      aria-hidden="true"
                    />
                    <Icon v-else name="import" :size="11" />
                    {{ t('mcp.discoverImport') }}
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
      <template #footer>
        <button type="button" class="btn" @click="discoverOpen = false">{{ t('common.close') }}</button>
      </template>
    </BaseModal>

    <!-- Secrets -->
    <BaseModal
      v-if="secretServer"
      :title="t('mcp.secretTitle')"
      :subtitle="secretServer.name"
      size="md"
      @close="closeSecrets"
    >
      <div class="stack-md">
        <div class="notice notice-warn">
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <div class="notice-body">
            <p>{{ t('mcp.secretWarning') }}</p>
            <p class="hint" style="margin-top: 3px">{{ t('mcp.secretLead') }}</p>
          </div>
        </div>
        <ErrorBanner v-if="secretError" :message="secretError" :title="t('errors.secretFailed')" />
        <LoadingState v-if="secretLoading" :label="t('common.loadingData')" compact />
        <p v-else-if="secretStates.length === 0" class="hint">{{ t('mcp.secretNoKeys') }}</p>
        <div v-else class="stack-sm">
          <div
            v-for="state in secretStates"
            :key="state.key"
            class="surface-inset"
            style="padding: 9px"
          >
            <div class="row" style="justify-content: space-between">
              <div class="row" style="gap: 6px">
                <Icon name="key" :size="12" />
                <span class="mono">{{ state.key }}</span>
              </div>
              <StatusBadge
                :label="state.present ? t('mcp.secretPresent') : t('mcp.secretMissing')"
                :tone="state.present ? 'success' : 'warn'"
              />
            </div>
            <div class="row" style="margin-top: 7px">
              <input
                v-model="secretValues[state.key]"
                class="input input-mono"
                type="password"
                autocomplete="off"
                :placeholder="t('mcp.secretValueHint')"
                :aria-label="`${state.key} ${t('mcp.secretValue')}`"
              />
              <button
                type="button"
                class="btn btn-sm"
                :disabled="!keychainAvailable || secretBusyKey === state.key"
                @click="setSecret(state.key)"
              >
                <span
                  v-if="secretBusyKey === state.key"
                  class="spinner spinner-sm"
                  aria-hidden="true"
                />
                {{ t('mcp.secretSet') }}
              </button>
              <button
                v-if="state.present"
                type="button"
                class="btn btn-sm btn-ghost"
                :disabled="secretBusyKey === state.key"
                @click="deleteSecretKey = state.key"
              >
                <Icon name="trash" :size="11" />
                {{ t('mcp.secretDelete') }}
              </button>
            </div>
          </div>
        </div>
      </div>
      <template #footer>
        <button type="button" class="btn" @click="closeSecrets">{{ t('common.close') }}</button>
      </template>
    </BaseModal>

    <!-- Distribute -->
    <BaseModal
      v-if="distributeServer"
      :title="t('mcp.distributeTitle')"
      :subtitle="distributeServer.name"
      size="md"
      @close="distributeServer = null"
    >
      <div class="stack-md">
        <div class="notice notice-warn">
          <Icon class="notice-icon" name="alert-triangle" :size="14" />
          <div class="notice-body">
            <p>{{ t('mcp.distributeWarning') }}</p>
            <p class="hint" style="margin-top: 3px">{{ t('mcp.applyHint') }}</p>
            <p v-if="distributeServer.issues.length > 0" class="hint" style="margin-top: 3px">
              {{ t('mcp.secretRequiredHint') }}
            </p>
          </div>
        </div>
        <div class="field">
          <span class="label">{{ t('mcp.chooseAgents') }}</span>
          <div v-if="app.agents.length === 0" class="hint">{{ t('mcp.noAgents') }}</div>
          <div v-else class="stack-sm list-scroll" style="max-height: 260px">
            <label
              v-for="agent in app.agents"
              :key="agent.id"
              class="checkbox-row surface-inset"
              style="padding: 7px 9px"
              :style="{ opacity: isAgentTargetable(agent) ? 1 : 0.55 }"
            >
              <input
                v-model="distributeAgentIds"
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
                <span class="hint" style="display: block">{{ agent.vendor }}</span>
              </span>
            </label>
          </div>
          <p class="hint">{{ t('mcp.selectedCount', { count: distributeAgentIds.length }) }}</p>
        </div>
        <ErrorBanner
          v-if="distributeError"
          :message="distributeError"
          :title="t('errors.planFailed')"
        />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="distributeBusy" @click="distributeServer = null">
          {{ t('common.cancel') }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="distributeBusy || distributeAgentIds.length === 0"
          @click="previewDistribute"
        >
          <span v-if="distributeBusy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="eye" :size="13" />
          {{ t('mcp.planDistribute') }}
        </button>
      </template>
    </BaseModal>

    <!-- Remove -->
    <BaseModal
      v-if="removeServer"
      :title="t('mcp.removeTitle')"
      :subtitle="removeServer.name"
      size="md"
      @close="removeServer = null"
    >
      <div class="stack-md">
        <p class="hint">{{ t('mcp.removeLead') }}</p>
        <p v-if="removeServer.distributions.length === 0" class="hint">{{ t('mcp.removeEmpty') }}</p>
        <div v-else class="stack-sm">
          <label
            v-for="distribution in removeServer.distributions"
            :key="distribution.installationId ?? distribution.agentId"
            class="checkbox-row surface-inset"
            style="padding: 7px 9px"
          >
            <input
              v-model="removeAgentIds"
              type="checkbox"
              class="checkbox"
              :value="distribution.agentId"
            />
            <span style="flex: 1">
              <span class="row" style="gap: 5px">
                <span style="font-weight: 550">{{ distribution.agentName }}</span>
                <StatusBadge
                  :label="distributionStateLabel(removeServer, distribution.agentId)"
                  :tone="toneForInstallationState(distribution.state)"
                />
              </span>
              <span class="path-text" style="display: block">{{ distribution.targetPath }}</span>
            </span>
          </label>
        </div>
        <ErrorBanner v-if="removeError" :message="removeError" :title="t('errors.planFailed')" />
      </div>
      <template #footer>
        <button type="button" class="btn" :disabled="removeBusy" @click="removeServer = null">
          {{ t('common.cancel') }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="removeBusy || removeAgentIds.length === 0"
          @click="previewRemove"
        >
          <span v-if="removeBusy" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="eye" :size="13" />
          {{ t('mcp.removePlan') }}
        </button>
      </template>
    </BaseModal>

    <ConfirmDialog
      v-if="deleteSecretKey && secretServer"
      :title="t('mcp.secretDelete')"
      :message="t('mcp.secretDeleteConfirm', { key: deleteSecretKey })"
      :confirm-label="t('mcp.secretDelete')"
      danger
      :busy="secretBusyKey === deleteSecretKey"
      @cancel="deleteSecretKey = null"
      @confirm="deleteSecret"
    />

    <PlanDialog v-if="plans" :plans="plans" @close="plans = null" @applied="afterApplied" />
  </div>
</template>
