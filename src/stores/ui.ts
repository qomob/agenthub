import { defineStore } from 'pinia'
import { ref } from 'vue'

import { setLocale, type AppLocale } from '../i18n'
import type { AppSettings } from '../lib/types'

export type ToastKind = 'success' | 'error' | 'info' | 'warning'
export type ThemeMode = 'system' | 'light' | 'dark'

export interface Toast {
  id: number
  kind: ToastKind
  message: string
  detail?: string
}

const TOAST_TIMEOUT: Record<ToastKind, number> = {
  success: 4200,
  info: 4200,
  warning: 7000,
  error: 9000,
}

export function normalizeTheme(value: string | null | undefined): ThemeMode {
  return value === 'light' || value === 'dark' ? value : 'system'
}

function systemPrefersDark(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

export const useUiStore = defineStore('ui', () => {
  const toasts = ref<Toast[]>([])
  const theme = ref<ThemeMode>('system')
  const locale = ref<AppLocale>('zh')

  let nextToastId = 1
  let media: MediaQueryList | null = null
  let mediaListener: (() => void) | null = null

  function dismiss(id: number): void {
    toasts.value = toasts.value.filter((toast) => toast.id !== id)
  }

  function notify(
    kind: ToastKind,
    message: string,
    options: { detail?: string; timeout?: number } = {},
  ): number {
    const id = nextToastId++
    toasts.value = [...toasts.value, { id, kind, message, detail: options.detail }]
    const timeout = options.timeout ?? TOAST_TIMEOUT[kind]
    if (timeout > 0) {
      window.setTimeout(() => dismiss(id), timeout)
    }
    return id
  }

  function notifyError(message: string, detail?: string): number {
    return notify('error', message, { detail })
  }

  function applyTheme(mode: ThemeMode): void {
    theme.value = mode
    if (typeof document === 'undefined') return
    const dark = mode === 'dark' || (mode === 'system' && systemPrefersDark())
    document.documentElement.classList.toggle('dark', dark)
    document.documentElement.dataset.theme = dark ? 'dark' : 'light'
    document.documentElement.style.colorScheme = dark ? 'dark' : 'light'
  }

  function watchSystemTheme(): void {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return
    if (media) return
    media = window.matchMedia('(prefers-color-scheme: dark)')
    mediaListener = () => {
      if (theme.value === 'system') applyTheme('system')
    }
    media.addEventListener('change', mediaListener)
  }

  function applyLanguage(value: string | null | undefined): void {
    const next = setLocale(value)
    locale.value = next
  }

  /** Apply the persisted preferences from the backend settings payload. */
  function applySettings(settings: Pick<AppSettings, 'language' | 'theme'>): void {
    watchSystemTheme()
    applyTheme(normalizeTheme(settings.theme))
    applyLanguage(settings.language)
  }

  return {
    toasts,
    theme,
    locale,
    notify,
    notifyError,
    dismiss,
    applyTheme,
    applyLanguage,
    applySettings,
    watchSystemTheme,
  }
})
