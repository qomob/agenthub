<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import EmptyState from '../components/EmptyState.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { api, normalizeError } from '../lib/api'
import { openExternal } from '../lib/dialogs'
import { formatRelativeTime } from '../lib/format'
import {
  pluginSupportKey,
  toneForPluginSupport,
} from '../lib/status'
import type { PluginRecordView } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useUiStore } from '../stores/ui'

const { t, locale } = useI18n()
const app = useAppStore()
const ui = useUiStore()

const plugins = ref<PluginRecordView[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const syncedAt = ref<string | null>(null)
const agentFilter = ref<string>('all')

const agentOptions = computed(() => {
  const map = new Map<string, string>()
  for (const agent of app.agents) map.set(agent.key, agent.name)
  for (const plugin of plugins.value) {
    if (!map.has(plugin.agentKey)) map.set(plugin.agentKey, plugin.agentName || plugin.agentKey)
  }
  return Array.from(map.entries()).map(([key, name]) => ({ key, name }))
})

const filtered = computed(() =>
  agentFilter.value === 'all'
    ? plugins.value
    : plugins.value.filter((plugin) => plugin.agentKey === agentFilter.value),
)

onMounted(() => {
  void sync()
})

async function sync(): Promise<void> {
  loading.value = true
  error.value = null
  try {
    plugins.value = await api.pluginsSync()
    syncedAt.value = new Date().toISOString()
  } catch (err) {
    error.value = normalizeError(err, t('errors.pluginsFailed'))
  } finally {
    loading.value = false
  }
}

async function openOfficial(url: string | null): Promise<void> {
  if (!url) return
  try {
    await openExternal(url)
  } catch (err) {
    ui.notifyError(t('errors.openFailed'), normalizeError(err))
  }
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('plugins.title')" :subtitle="t('plugins.subtitle')">
      <template #meta>
        <p class="hint" style="margin-top: 6px">
          <span class="badge badge-outline">{{ t('plugins.readOnly') }}</span>
          <span v-if="syncedAt"> · {{ t('plugins.lastSync') }} {{ formatRelativeTime(syncedAt, locale) }}</span>
        </p>
      </template>
      <template #actions>
        <button type="button" class="btn btn-primary" :disabled="loading" @click="sync">
          <span v-if="loading" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ loading ? t('plugins.syncing') : t('plugins.sync') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <div class="notice notice-info">
        <Icon class="notice-icon" name="info" :size="14" />
        <p>{{ t('plugins.lead') }}</p>
      </div>

      <ErrorBanner
        v-if="error"
        :message="error"
        :title="t('errors.pluginsFailed')"
        :retry-label="t('common.retry')"
        :retrying="loading"
        @retry="sync"
      />

      <div class="toolbar">
        <select v-model="agentFilter" class="select" style="width: 200px">
          <option value="all">{{ t('plugins.allAgents') }}</option>
          <option v-for="option in agentOptions" :key="option.key" :value="option.key">
            {{ option.name }}
          </option>
        </select>
        <span class="hint">{{ filtered.length }} {{ t('common.items') }}</span>
      </div>

      <LoadingState v-if="loading && plugins.length === 0" :label="t('common.loadingData')" />

      <EmptyState
        v-else-if="plugins.length === 0"
        icon="plugins"
        :title="t('plugins.emptyTitle')"
        :body="t('plugins.emptyBody')"
      >
        <template #actions>
          <button type="button" class="btn btn-primary" :disabled="loading" @click="sync">
            <Icon name="refresh" :size="13" />
            {{ t('plugins.sync') }}
          </button>
        </template>
      </EmptyState>

      <div v-else class="card">
        <div class="card-body flush">
          <div class="table-wrap" style="max-height: calc(100vh - 300px)">
            <table class="table table-compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('plugins.version') }}</th>
                  <th scope="col">{{ t('plugins.agent') }}</th>
                  <th scope="col">{{ t('plugins.managedBy') }}</th>
                  <th scope="col">{{ t('plugins.installState') }}</th>
                  <th scope="col">{{ t('plugins.source') }}</th>
                  <th scope="col">{{ t('plugins.permissions') }}</th>
                  <th scope="col">{{ t('plugins.discoveredPath') }}</th>
                  <th scope="col">{{ t('plugins.official') }}</th>
                  <th scope="col">{{ t('plugins.updatedAt') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="plugin in filtered" :key="plugin.id">
                  <td>
                    <div class="row" style="gap: 5px">
                      <span style="font-weight: 600">{{ plugin.name }}</span>
                      <span v-if="plugin.version" class="chip">{{ plugin.version }}</span>
                    </div>
                    <p v-for="(note, index) in plugin.notes" :key="index" class="hint">
                      {{ note }}
                    </p>
                  </td>
                  <td>{{ plugin.agentName || plugin.agentKey }}</td>
                  <td>
                    <StatusBadge
                      :label="t(pluginSupportKey[plugin.managedBy])"
                      :tone="toneForPluginSupport(plugin.managedBy)"
                    />
                  </td>
                  <td><span class="chip">{{ plugin.installState }}</span></td>
                  <td class="text-xs">{{ plugin.source || '—' }}</td>
                  <td>
                    <div v-if="plugin.permissions.length > 0" class="row row-wrap" style="gap: 3px">
                      <span
                        v-for="permission in plugin.permissions.slice(0, 3)"
                        :key="permission"
                        class="tag"
                      >
                        {{ permission }}
                      </span>
                      <span v-if="plugin.permissions.length > 3" class="tag">
                        +{{ plugin.permissions.length - 3 }}
                      </span>
                    </div>
                    <span v-else class="hint">{{ t('plugins.noPermissions') }}</span>
                  </td>
                  <td class="path-text">{{ plugin.discoveredPath || '—' }}</td>
                  <td>
                    <button
                      v-if="plugin.officialUrl"
                      type="button"
                      class="btn btn-sm btn-ghost"
                      @click="openOfficial(plugin.officialUrl)"
                    >
                      <Icon name="external" :size="11" />
                      {{ t('plugins.openOfficial') }}
                    </button>
                    <span v-else class="hint">{{ t('plugins.noUrl') }}</span>
                  </td>
                  <td class="text-xs">{{ formatRelativeTime(plugin.updatedAt, locale) }}</td>
                </tr>
                <tr v-if="filtered.length === 0">
                  <td colspan="9" class="hint" style="text-align: center">
                    {{ t('plugins.emptyBody') }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <div class="notice">
        <Icon class="notice-icon" name="info" :size="14" />
        <div class="notice-body">
          <p class="notice-title">{{ t('plugins.supportLegend') }}</p>
          <p class="hint">{{ t('plugins.supportLegendText') }}</p>
        </div>
      </div>
    </div>
  </div>
</template>
