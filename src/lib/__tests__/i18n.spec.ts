import { describe, expect, it } from 'vitest'

import { en } from '../../i18n/en'
import { zh } from '../../i18n/zh'

function flatten(value: unknown, prefix = ''): string[] {
  if (Array.isArray(value)) {
    return [prefix]
  }
  if (value && typeof value === 'object') {
    return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
      flatten(child, prefix ? `${prefix}.${key}` : key),
    )
  }
  return [prefix]
}

function leafStrings(value: unknown, prefix = ''): Array<[string, string]> {
  if (Array.isArray(value)) return []
  if (value && typeof value === 'object') {
    return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
      leafStrings(child, prefix ? `${prefix}.${key}` : key),
    )
  }
  return [[prefix, String(value)]]
}

describe('i18n dictionaries', () => {
  it('zh and en expose exactly the same keys', () => {
    expect(flatten(en).sort()).toEqual(flatten(zh).sort())
  })

  it('has no empty UI strings', () => {
    for (const [key, value] of leafStrings(zh)) {
      expect(value.trim(), `zh.${key}`).not.toBe('')
    }
    for (const [key, value] of leafStrings(en)) {
      expect(value.trim(), `en.${key}`).not.toBe('')
    }
  })

  it('keeps list-shaped copy the same length in both locales', () => {
    expect(en.settings.limitations).toHaveLength(zh.settings.limitations.length)
    expect(Object.keys(en.onboarding.privacy.points).sort()).toEqual(
      Object.keys(zh.onboarding.privacy.points).sort(),
    )
  })
})
