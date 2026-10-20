import { createI18n } from 'vue-i18n'

import { en } from './en'
import { zh } from './zh'

export const messages = { zh, en }
export type AppLocale = 'zh' | 'en'
export const DEFAULT_LOCALE: AppLocale = 'zh'

export const i18n = createI18n({
  legacy: false,
  locale: DEFAULT_LOCALE,
  fallbackLocale: 'zh',
  messages,
  missingWarn: false,
  fallbackWarn: false,
})

export function normalizeLocale(value: string | null | undefined): AppLocale {
  return value === 'en' ? 'en' : 'zh'
}

export function setLocale(value: string | null | undefined): AppLocale {
  const next = normalizeLocale(value)
  i18n.global.locale.value = next
  if (typeof document !== 'undefined') {
    document.documentElement.lang = next === 'zh' ? 'zh-CN' : 'en'
  }
  return next
}

export function t(key: string, named?: Record<string, unknown>): string {
  return named ? i18n.global.t(key, named) : i18n.global.t(key)
}
