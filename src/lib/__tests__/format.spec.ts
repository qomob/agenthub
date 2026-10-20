import { describe, expect, it } from 'vitest'

import {
  basename,
  formatBytes,
  formatDateTime,
  formatRelativeTime,
  parseKeyValueLines,
  shortHash,
  splitLines,
} from '../format'

describe('format helpers', () => {
  it('formats byte sizes with sensible precision', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(1024)).toBe('1.00 KB')
    expect(formatBytes(1536)).toBe('1.50 KB')
    expect(formatBytes(1024 * 1024)).toBe('1.00 MB')
    expect(formatBytes(null)).toBe('—')
  })

  it('shortens hashes and extracts basenames', () => {
    expect(shortHash('abcdef1234567890')).toBe('abcdef12')
    expect(shortHash('abc')).toBe('abc')
    expect(shortHash(null)).toBe('—')
    expect(basename('/Users/me/.codex/skills/demo/')).toBe('demo')
    expect(basename('C:\\Users\\me\\skills\\demo')).toBe('demo')
  })

  it('splits textarea lines, trims and deduplicates', () => {
    expect(splitLines(' a \n\nb\na\n  c  ')).toEqual(['a', 'b', 'c'])
  })

  it('parses header lines and rejects malformed ones', () => {
    expect(parseKeyValueLines('Authorization: Bearer x\nX-Title: hi')).toEqual({
      Authorization: 'Bearer x',
      'X-Title': 'hi',
    })
    expect(() => parseKeyValueLines('no-colon')).toThrow()
  })
})

describe('date helpers', () => {
  it('returns readable relative times', () => {
    const now = Date.parse('2026-10-20T12:00:00Z')
    expect(formatRelativeTime('2026-10-20T11:59:50Z', 'en', now)).toBe('just now')
    expect(formatRelativeTime('2026-10-20T11:59:50Z', 'zh', now)).toBe('刚刚')
    expect(formatRelativeTime('2026-10-20T11:58:00Z', 'en', now)).toContain('2 minutes')
  })

  it('falls back to the raw string for invalid dates', () => {
    expect(formatDateTime('not-a-date', 'en')).toBe('not-a-date')
    expect(formatRelativeTime('', 'zh')).toBe('—')
  })
})
