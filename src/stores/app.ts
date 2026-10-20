import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { api, normalizeError } from '../lib/api'
import type {
  AgentView,
  AppSettings,
  Bootstrap,
  Dashboard,
  ScanReport,
  SettingsUpdate,
} from '../lib/types'
import { useUiStore } from './ui'

export const useAppStore = defineStore('app', () => {
  const bootstrapData = ref<Bootstrap | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)

  const dashboard = ref<Dashboard | null>(null)
  const dashboardLoading = ref(false)
  const dashboardError = ref<string | null>(null)

  const scanning = ref(false)
  const scanError = ref<string | null>(null)
  const scanReport = ref<ScanReport | null>(null)

  const refreshingAgents = ref(false)
  const agentsError = ref<string | null>(null)

  const settings = computed<AppSettings | null>(() => bootstrapData.value?.settings ?? null)
  const agents = computed<AgentView[]>(() => bootstrapData.value?.agents ?? [])
  const projectRoots = computed<string[]>(() => bootstrapData.value?.projectRoots ?? [])
  const initialized = computed<boolean>(() => settings.value?.initialized ?? false)
  const platform = computed<string>(() => bootstrapData.value?.platform ?? '')
  const coreVersion = computed<string>(() => bootstrapData.value?.coreVersion ?? '')

  async function initialize(): Promise<void> {
    loading.value = true
    error.value = null
    try {
      const data = await api.bootstrap()
      bootstrapData.value = data
      useUiStore().applySettings(data.settings)
    } catch (err) {
      error.value = normalizeError(err, 'AgentHub 初始化失败')
      throw err
    } finally {
      loading.value = false
    }
  }

  async function refreshBootstrap(): Promise<void> {
    const data = await api.bootstrap()
    bootstrapData.value = data
    useUiStore().applySettings(data.settings)
  }

  function replaceSettings(next: AppSettings): void {
    if (!bootstrapData.value) return
    bootstrapData.value = { ...bootstrapData.value, settings: next }
  }

  async function updateSettings(update: SettingsUpdate): Promise<AppSettings> {
    const next = await api.updateSettings(update)
    replaceSettings(next)
    useUiStore().applySettings(next)
    return next
  }

  /** Refresh settings and project roots through `app_settings` / `app_project_roots`. */
  async function reloadSettings(): Promise<AppSettings> {
    const [next, roots] = await Promise.all([api.settings(), api.projectRoots()])
    replaceSettings(next)
    useUiStore().applySettings(next)
    if (bootstrapData.value) {
      bootstrapData.value = { ...bootstrapData.value, projectRoots: roots }
    }
    return next
  }

  async function markInitialized(): Promise<AppSettings> {
    const next = await api.markInitialized()
    replaceSettings(next)
    return next
  }

  async function refreshAgents(): Promise<AgentView[]> {
    refreshingAgents.value = true
    agentsError.value = null
    try {
      const next = await api.agentsList()
      if (bootstrapData.value) {
        bootstrapData.value = { ...bootstrapData.value, agents: next }
      }
      return next
    } catch (err) {
      agentsError.value = normalizeError(err, '无法加载 Agent 列表')
      throw err
    } finally {
      refreshingAgents.value = false
    }
  }

  async function loadDashboard(force = false): Promise<Dashboard | null> {
    if (dashboard.value && !force) return dashboard.value
    dashboardLoading.value = true
    dashboardError.value = null
    try {
      dashboard.value = await api.dashboard()
      return dashboard.value
    } catch (err) {
      dashboardError.value = normalizeError(err, '无法加载仪表盘')
      throw err
    } finally {
      dashboardLoading.value = false
    }
  }

  async function scan(options: { refreshDashboard?: boolean } = {}): Promise<ScanReport> {
    scanning.value = true
    scanError.value = null
    try {
      const report = await api.scan()
      scanReport.value = report
      if (options.refreshDashboard !== false) {
        await loadDashboard(true).catch(() => undefined)
        await refreshAgents().catch(() => undefined)
      }
      return report
    } catch (err) {
      scanError.value = normalizeError(err, '扫描失败')
      throw err
    } finally {
      scanning.value = false
    }
  }

  async function saveProjectRoots(roots: string[]): Promise<string[]> {
    const next = await api.setProjectRoots(roots)
    if (bootstrapData.value) {
      bootstrapData.value = { ...bootstrapData.value, projectRoots: next }
    }
    return next
  }

  return {
    bootstrapData,
    loading,
    error,
    dashboard,
    dashboardLoading,
    dashboardError,
    scanning,
    scanError,
    scanReport,
    refreshingAgents,
    agentsError,
    settings,
    agents,
    projectRoots,
    initialized,
    platform,
    coreVersion,
    initialize,
    refreshBootstrap,
    updateSettings,
    reloadSettings,
    markInitialized,
    refreshAgents,
    loadDashboard,
    scan,
    saveProjectRoots,
  }
})
