<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterView, useRouter } from 'vue-router'

import AppSidebar from './components/AppSidebar.vue'
import ErrorBanner from './components/ErrorBanner.vue'
import ToastHost from './components/ToastHost.vue'
import { isDesktop, normalizeError } from './lib/api'
import { navItems } from './router'
import OnboardingView from './pages/OnboardingView.vue'
import { useAppStore } from './stores/app'
import { useUiStore } from './stores/ui'

const { t } = useI18n()
const router = useRouter()
const app = useAppStore()
const ui = useUiStore()

const desktop = isDesktop()
const booting = ref(desktop)
const bootError = ref<string | null>(null)

const showShell = computed(
  () => desktop && !booting.value && !bootError.value && !!app.bootstrapData?.settings.initialized,
)
const showOnboarding = computed(
  () => desktop && !booting.value && !bootError.value && !!app.bootstrapData && !showShell.value,
)

async function boot(): Promise<void> {
  if (!desktop) return
  booting.value = true
  bootError.value = null
  try {
    await app.initialize()
    booting.value = false
    if (app.initialized && app.settings?.scanOnStartup) {
      void runStartupScan()
    }
  } catch (error) {
    bootError.value = app.error ?? normalizeError(error, t('errors.bootstrapFailed'))
    booting.value = false
  }
}

async function runStartupScan(): Promise<void> {
  try {
    await app.scan()
    if (app.scanReport) {
      ui.notify('info', t('dashboard.scanDone'), {
        detail: t('dashboard.scanResultSummary', {
          skills: app.scanReport.totalSkills,
          duplicates: app.scanReport.duplicates,
          conflicts: app.scanReport.conflicts,
          invalid: app.scanReport.invalid,
        }),
      })
    }
  } catch (error) {
    ui.notifyError(t('errors.scanFailed'), normalizeError(error))
  }
}

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  const tag = target.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target.isContentEditable
}

async function focusSkillSearch(): Promise<void> {
  await router.push({ name: 'skills' })
  await nextTick()
  const input = document.getElementById('skills-search')
  if (input instanceof HTMLElement) {
    input.focus()
    if (input instanceof HTMLInputElement) input.select()
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (!showShell.value) return
  if (!(event.metaKey || event.ctrlKey) || event.altKey) return

  const key = event.key.toLowerCase()
  if (key === 'k') {
    event.preventDefault()
    void focusSkillSearch()
    return
  }
  if (event.key === ',') {
    event.preventDefault()
    void router.push({ name: 'settings' })
    return
  }
  if (event.key === '/') {
    event.preventDefault()
    window.dispatchEvent(new CustomEvent('agenthub:shortcuts'))
    return
  }
  if (/^[1-9]$/.test(event.key) && !isEditable(event.target)) {
    const item = navItems[Number(event.key) - 1]
    if (item) {
      event.preventDefault()
      void router.push(item.path)
    }
  }
}

function focusMain(): void {
  document.getElementById('main-content')?.focus()
}

onMounted(() => {
  ui.watchSystemTheme()
  ui.applyTheme('system')
  document.addEventListener('keydown', onKeydown)
  void boot()
})

watch(
  () => ui.locale,
  () => {
    const meta = router.currentRoute.value.meta
    const key = typeof meta.titleKey === 'string' ? meta.titleKey : 'common.appName'
    document.title = `${t(key)} · AgentHub`
  },
)

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <template v-if="!desktop">
    <div class="center-screen">
      <div class="splash-card">
        <div class="brand-mark-lg" aria-hidden="true">AH</div>
        <p class="splash-title">{{ t('errors.notTauriTitle') }}</p>
        <p class="splash-body">{{ t('errors.notTauriBody') }}</p>
        <p class="text-sm">{{ t('errors.notTauriHint') }}</p>
        <code class="code-line">{{ t('errors.notTauriCommand') }}</code>
        <p class="hint">{{ t('errors.notTauriBrowserHint') }}</p>
      </div>
    </div>
  </template>

  <div v-else-if="booting" class="center-screen">
    <div class="splash-card">
      <div class="brand-mark-lg" aria-hidden="true">AH</div>
      <span class="spinner" style="width: 20px; height: 20px" aria-hidden="true" />
      <p class="splash-title">{{ t('common.bootstrap') }}</p>
    </div>
  </div>

  <div v-else-if="bootError" class="center-screen">
    <div class="splash-card" style="justify-items: stretch; text-align: left">
      <div class="brand-mark-lg" aria-hidden="true">AH</div>
      <p class="splash-title">{{ t('errors.bootstrapFailed') }}</p>
      <ErrorBanner
        :message="bootError"
        :title="t('errors.bootstrapFailed')"
        :retry-label="t('common.retry')"
        @retry="boot"
      />
    </div>
  </div>

  <OnboardingView v-else-if="showOnboarding" />

  <div v-else-if="showShell" class="app-shell">
    <a href="#main-content" class="skip-link" @click.prevent="focusMain">
      {{ t('common.skipToContent') }}
    </a>
    <AppSidebar />
    <div id="main-content" class="app-main" tabindex="-1">
      <div class="main-scroll">
        <RouterView />
      </div>
    </div>
  </div>

  <div v-else class="center-screen">
    <div class="splash-card" style="justify-items: stretch; text-align: left">
      <div class="brand-mark-lg" aria-hidden="true">AH</div>
      <ErrorBanner
        :message="app.error ?? t('errors.bootstrapFailed')"
        :title="t('errors.bootstrapFailed')"
        :retry-label="t('common.retry')"
        @retry="boot"
      />
    </div>
  </div>

  <ToastHost />
</template>
