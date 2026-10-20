<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import ConfirmDialog from '../components/ConfirmDialog.vue'
import ErrorBanner from '../components/ErrorBanner.vue'
import Icon from '../components/Icon.vue'
import LoadingState from '../components/LoadingState.vue'
import PageHeader from '../components/PageHeader.vue'
import StatusBadge from '../components/StatusBadge.vue'
import { normalizeError } from '../lib/api'
import { chooseDirectory } from '../lib/dialogs'
import type { InstallStrategy } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useUiStore } from '../stores/ui'

const { t, tm } = useI18n()
const app = useAppStore()
const ui = useUiStore()

const settings = computed(() => app.settings)
const savingKey = ref<string | null>(null)
const saveError = ref<string | null>(null)

const libraryDraft = ref('')
const libraryBusy = ref(false)
const libraryError = ref<string | null>(null)
const libraryConfirmOpen = ref(false)

const rootsBusy = ref(false)
const rootsError = ref<string | null>(null)
const refreshing = ref(false)

const limitations = computed<string[]>(() => {
  const raw = tm('settings.limitations')
  return Array.isArray(raw)
    ? raw.map((item) => (typeof item === 'string' ? item : String(item)))
    : []
})

onMounted(() => {
  libraryDraft.value = app.settings?.libraryRoot ?? ''
})

async function persist(
  key: string,
  update: {
    defaultStrategy?: InstallStrategy | null
    language?: string | null
    theme?: string | null
    scanOnStartup?: boolean | null
  },
  successKey: string,
): Promise<void> {
  savingKey.value = key
  saveError.value = null
  try {
    await app.updateSettings(update)
    ui.notify('success', t(successKey))
  } catch (err) {
    saveError.value = normalizeError(err, t('errors.settingsFailed'))
    ui.notifyError(t('errors.settingsFailed'), saveError.value)
  } finally {
    savingKey.value = null
  }
}

function changeStrategy(event: Event): void {
  const value = (event.target as HTMLSelectElement).value as InstallStrategy
  if (!settings.value || settings.value.defaultStrategy === value) return
  void persist('strategy', { defaultStrategy: value }, 'settings.strategySaved')
}

function changeLanguage(language: 'zh' | 'en'): void {
  if (!settings.value || settings.value.language === language) return
  void persist('language', { language }, 'settings.languageSaved')
}

function changeTheme(theme: 'system' | 'light' | 'dark'): void {
  if (!settings.value || settings.value.theme === theme) return
  void persist('theme', { theme }, 'settings.themeSaved')
}

function changeScanOnStartup(event: Event): void {
  const value = (event.target as HTMLInputElement).checked
  if (!settings.value || settings.value.scanOnStartup === value) return
  void persist('scan', { scanOnStartup: value }, 'settings.scanSaved')
}

async function refreshSettings(): Promise<void> {
  refreshing.value = true
  saveError.value = null
  try {
    await app.reloadSettings()
    libraryDraft.value = app.settings?.libraryRoot ?? ''
  } catch (err) {
    saveError.value = normalizeError(err, t('errors.settingsFailed'))
    ui.notifyError(t('errors.settingsFailed'), saveError.value)
  } finally {
    refreshing.value = false
  }
}

async function pickLibrary(): Promise<void> {
  try {
    const selected = await chooseDirectory(settings.value?.libraryRoot)
    if (selected) {
      libraryDraft.value = selected
      libraryError.value = null
    }
  } catch (err) {
    libraryError.value = normalizeError(err, t('errors.dialogFailed'))
  }
}

function applyLibrary(): void {
  if (!settings.value) return
  const next = libraryDraft.value.trim()
  if (!next) {
    libraryError.value = t('errors.pathRequired')
    return
  }
  if (next === settings.value.libraryRoot) {
    libraryError.value = t('settings.librarySame')
    return
  }
  libraryConfirmOpen.value = true
}

async function confirmLibrary(): Promise<void> {
  if (!settings.value) return
  const next = libraryDraft.value.trim()
  if (!next) return
  libraryConfirmOpen.value = false
  libraryBusy.value = true
  libraryError.value = null
  try {
    const updated = await app.updateSettings({ libraryRoot: next })
    libraryDraft.value = updated.libraryRoot
    ui.notify('success', t('settings.libraryApplied'))
    await app.loadDashboard(true).catch(() => undefined)
  } catch (err) {
    libraryError.value = normalizeError(err, t('errors.settingsFailed'))
  } finally {
    libraryBusy.value = false
  }
}

async function addProjectRoot(): Promise<void> {
  try {
    const selected = await chooseDirectory()
    if (!selected) return
    if (app.projectRoots.includes(selected)) return
    rootsBusy.value = true
    rootsError.value = null
    try {
      await app.saveProjectRoots([...app.projectRoots, selected])
      ui.notify('success', t('settings.projectRootsSaved'))
    } catch (err) {
      rootsError.value = normalizeError(err, t('errors.saveFailed'))
    } finally {
      rootsBusy.value = false
    }
  } catch (err) {
    rootsError.value = normalizeError(err, t('errors.dialogFailed'))
  }
}

async function removeProjectRoot(root: string): Promise<void> {
  rootsBusy.value = true
  rootsError.value = null
  try {
    await app.saveProjectRoots(app.projectRoots.filter((item) => item !== root))
    ui.notify('success', t('settings.projectRootsSaved'))
  } catch (err) {
    rootsError.value = normalizeError(err, t('errors.saveFailed'))
  } finally {
    rootsBusy.value = false
  }
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('settings.title')" :subtitle="t('settings.subtitle')">
      <template #meta>
        <p class="hint" style="margin-top: 6px">{{ t('settings.restartHint') }}</p>
      </template>
      <template #actions>
        <button type="button" class="btn" :disabled="refreshing" @click="refreshSettings">
          <span v-if="refreshing" class="spinner spinner-sm" aria-hidden="true" />
          <Icon v-else name="refresh" :size="13" />
          {{ t('common.refresh') }}
        </button>
      </template>
    </PageHeader>

    <div class="section-stack">
      <ErrorBanner v-if="saveError" :message="saveError" :title="t('errors.settingsFailed')" />

      <LoadingState v-if="!settings" :label="t('settings.loading')" />

      <template v-else>
        <!-- Library -->
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('settings.librarySection') }}</p>
              <p class="card-subtitle">{{ t('settings.libraryLead') }}</p>
            </div>
            <span v-if="libraryBusy" class="spinner spinner-sm" aria-hidden="true" />
          </div>
          <div class="card-body stack-md">
            <div class="field">
              <span class="label">{{ t('settings.libraryCurrent') }}</span>
              <p class="path-text">{{ settings.libraryRoot }}</p>
            </div>
            <div class="field">
              <label class="label" for="settings-library-path">{{ t('settings.libraryNew') }}</label>
              <div class="row">
                <input
                  id="settings-library-path"
                  v-model="libraryDraft"
                  class="input input-mono"
                  type="text"
                  spellcheck="false"
                />
                <button type="button" class="btn" :disabled="libraryBusy" @click="pickLibrary">
                  <Icon name="folder" :size="13" />
                  {{ t('settings.chooseLibrary') }}
                </button>
                <button
                  type="button"
                  class="btn btn-primary"
                  :disabled="libraryBusy || !libraryDraft.trim()"
                  @click="applyLibrary"
                >
                  <span v-if="libraryBusy" class="spinner spinner-sm" aria-hidden="true" />
                  <Icon v-else name="check" :size="13" />
                  {{ t('settings.applyLibrary') }}
                </button>
              </div>
            </div>
            <div class="notice notice-warn">
              <Icon class="notice-icon" name="alert-triangle" :size="14" />
              <div class="notice-body">
                <p>{{ t('settings.libraryWarning') }}</p>
                <p class="hint" style="margin-top: 3px">{{ t('settings.libraryHint') }}</p>
              </div>
            </div>
            <ErrorBanner v-if="libraryError" :message="libraryError" :title="t('errors.settingsFailed')" />
          </div>
        </div>

        <div class="grid-2">
          <!-- Install strategy + scan -->
          <div class="card">
            <div class="card-header">
              <div>
                <p class="card-title">{{ t('settings.installSection') }}</p>
                <p class="card-subtitle">{{ t('settings.installLead') }}</p>
              </div>
              <span v-if="savingKey" class="spinner spinner-sm" aria-hidden="true" />
            </div>
            <div class="card-body stack-md">
              <div class="field">
                <label class="label" for="settings-strategy">{{ t('settings.defaultStrategy') }}</label>
                <select
                  id="settings-strategy"
                  class="select"
                  :value="settings.defaultStrategy"
                  :disabled="savingKey !== null"
                  @change="changeStrategy"
                >
                  <option value="copy">{{ t('status.strategy.copy') }}</option>
                  <option value="symlink">{{ t('status.strategy.symlink') }}</option>
                </select>
                <p class="hint">
                  {{
                    settings.defaultStrategy === 'symlink'
                      ? t('settings.strategySymlinkHint')
                      : t('settings.strategyCopyHint')
                  }}
                </p>
              </div>

              <hr class="divider" />

              <div class="stack-sm">
                <span class="label">{{ t('settings.scanSection') }}</span>
                <p class="hint">{{ t('settings.scanLead') }}</p>
                <label class="checkbox-row">
                  <input
                    type="checkbox"
                    class="checkbox"
                    :checked="settings.scanOnStartup"
                    :disabled="savingKey !== null"
                    @change="changeScanOnStartup"
                  />
                  <span>
                    <span style="font-weight: 600">{{ t('settings.scanOnStartup') }}</span>
                    <span class="hint" style="display: block">{{ t('settings.scanOnStartupHint') }}</span>
                  </span>
                </label>
              </div>
            </div>
          </div>

          <!-- Language + theme -->
          <div class="card">
            <div class="card-header">
              <div>
                <p class="card-title">{{ t('settings.languageSection') }} / {{ t('settings.themeSection') }}</p>
                <p class="card-subtitle">{{ t('settings.restartHint') }}</p>
              </div>
            </div>
            <div class="card-body stack-md">
              <div class="field">
                <span class="label">{{ t('settings.languageSection') }}</span>
                <div class="segmented">
                  <button
                    type="button"
                    :class="{ active: settings.language === 'zh' }"
                    :disabled="savingKey !== null"
                    @click="changeLanguage('zh')"
                  >
                    {{ t('settings.languageZh') }}
                  </button>
                  <button
                    type="button"
                    :class="{ active: settings.language === 'en' }"
                    :disabled="savingKey !== null"
                    @click="changeLanguage('en')"
                  >
                    {{ t('settings.languageEn') }}
                  </button>
                </div>
                <p class="hint">{{ t('settings.languageLead') }}</p>
              </div>

              <hr class="divider" />

              <div class="field">
                <span class="label">{{ t('settings.themeSection') }}</span>
                <div class="segmented">
                  <button
                    type="button"
                    :class="{ active: settings.theme === 'system' }"
                    :disabled="savingKey !== null"
                    @click="changeTheme('system')"
                  >
                    <Icon name="monitor" :size="11" />
                    {{ t('settings.themeSystem') }}
                  </button>
                  <button
                    type="button"
                    :class="{ active: settings.theme === 'light' }"
                    :disabled="savingKey !== null"
                    @click="changeTheme('light')"
                  >
                    <Icon name="sun" :size="11" />
                    {{ t('settings.themeLight') }}
                  </button>
                  <button
                    type="button"
                    :class="{ active: settings.theme === 'dark' }"
                    :disabled="savingKey !== null"
                    @click="changeTheme('dark')"
                  >
                    <Icon name="moon" :size="11" />
                    {{ t('settings.themeDark') }}
                  </button>
                </div>
                <p class="hint">{{ t('settings.themeLead') }}</p>
              </div>
            </div>
          </div>
        </div>

        <!-- Project roots -->
        <div class="card">
          <div class="card-header">
            <div>
              <p class="card-title">{{ t('settings.projectRootsSection') }}</p>
              <p class="card-subtitle">{{ t('settings.projectRootsLead') }}</p>
            </div>
            <div class="row">
              <span v-if="rootsBusy" class="spinner spinner-sm" aria-hidden="true" />
              <button type="button" class="btn btn-sm" :disabled="rootsBusy" @click="addProjectRoot">
                <Icon name="folder" :size="12" />
                {{ t('settings.addProjectRoot') }}
              </button>
            </div>
          </div>
          <div class="card-body stack-sm">
            <p v-if="app.projectRoots.length === 0" class="hint">
              {{ t('settings.projectRootsEmpty') }}
            </p>
            <div
              v-for="root in app.projectRoots"
              :key="root"
              class="row surface-inset"
              style="padding: 6px 9px"
            >
              <Icon name="folder" :size="12" class="subtle" />
              <span class="path-text" style="flex: 1">{{ root }}</span>
              <button
                type="button"
                class="btn btn-ghost btn-sm"
                :disabled="rootsBusy"
                @click="removeProjectRoot(root)"
              >
                <Icon name="trash" :size="11" />
                {{ t('settings.removeProjectRoot') }}
              </button>
            </div>
            <ErrorBanner v-if="rootsError" :message="rootsError" :title="t('errors.saveFailed')" />
          </div>
        </div>

        <div class="grid-2">
          <!-- System info -->
          <div class="card">
            <div class="card-header">
              <p class="card-title">{{ t('settings.infoSection') }}</p>
            </div>
            <div class="card-body">
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.databasePath') }}</span>
                <span class="info-row-value path-text">{{ settings.databasePath }}</span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.libraryRoot') }}</span>
                <span class="info-row-value path-text">{{ settings.libraryRoot }}</span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.appVersion') }}</span>
                <span class="info-row-value mono">{{ settings.appVersion }}</span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.coreVersion') }}</span>
                <span class="info-row-value mono">{{ app.coreVersion }}</span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.platform') }}</span>
                <span class="info-row-value mono">{{ app.platform }}</span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.keychain') }}</span>
                <span class="info-row-value">
                  <StatusBadge
                    v-if="settings.keychainAvailable"
                    :label="t('settings.keychainAvailable')"
                    tone="success"
                  />
                  <StatusBadge
                    v-else
                    :label="t('settings.keychainUnavailable')"
                    tone="warn"
                  />
                </span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.gitImport') }}</span>
                <span class="info-row-value">
                  {{ settings.gitImportEnabled ? t('settings.gitImportEnabled') : t('settings.gitImportDisabled') }}
                </span>
              </div>
              <div class="info-row">
                <span class="info-row-label">{{ t('settings.initialized') }}</span>
                <span class="info-row-value">
                  {{ settings.initialized ? t('settings.initializedYes') : t('settings.initializedNo') }}
                </span>
              </div>
            </div>
          </div>

          <!-- Known limitations -->
          <div class="card">
            <div class="card-header">
              <div>
                <p class="card-title">{{ t('settings.limitationsSection') }}</p>
                <p class="card-subtitle">{{ t('settings.limitationsLead') }}</p>
              </div>
            </div>
            <div class="card-body">
              <ul class="bullet-list">
                <li v-for="(limitation, index) in limitations" :key="index">{{ limitation }}</li>
              </ul>
              <p class="hint" style="margin-top: 10px">{{ t('settings.securityNote') }}</p>
            </div>
          </div>
        </div>
      </template>
    </div>

    <ConfirmDialog
      v-if="libraryConfirmOpen"
      :title="t('settings.librarySection')"
      :message="t('settings.libraryConfirm')"
      :confirm-label="t('settings.applyLibrary')"
      :busy="libraryBusy"
      @cancel="libraryConfirmOpen = false"
      @confirm="confirmLibrary"
    />
  </div>
</template>
