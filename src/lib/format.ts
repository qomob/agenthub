const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes)) return '—'
  if (bytes <= 0) return '0 B'
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1)
  const value = bytes / Math.pow(1024, exponent)
  const digits = exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${BYTE_UNITS[exponent]}`
}

function asDate(value: string | null | undefined): Date | null {
  if (!value) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

export function formatDateTime(
  value: string | null | undefined,
  locale: string,
): string {
  if (!value) return '—'
  const date = asDate(value)
  if (!date) return value
  const intlLocale = locale === 'zh' ? 'zh-CN' : 'en-US'
  return new Intl.DateTimeFormat(intlLocale, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date)
}

export function formatRelativeTime(
  value: string | null | undefined,
  locale: string,
  now: number = Date.now(),
): string {
  if (!value) return '—'
  const date = asDate(value)
  if (!date) return value
  const diffSeconds = Math.round((date.getTime() - now) / 1000)
  const abs = Math.abs(diffSeconds)
  if (abs < 45) return locale === 'zh' ? '刚刚' : 'just now'
  const formatter = new Intl.RelativeTimeFormat(locale === 'zh' ? 'zh-CN' : 'en', {
    numeric: 'auto',
  })
  const units: Array<[Intl.RelativeTimeFormatUnit, number]> = [
    ['year', 60 * 60 * 24 * 365],
    ['month', 60 * 60 * 24 * 30],
    ['week', 60 * 60 * 24 * 7],
    ['day', 60 * 60 * 24],
    ['hour', 60 * 60],
    ['minute', 60],
    ['second', 1],
  ]
  for (const [unit, seconds] of units) {
    if (abs >= seconds || unit === 'second') {
      return formatter.format(Math.round(diffSeconds / seconds), unit)
    }
  }
  return locale === 'zh' ? '刚刚' : 'just now'
}

export function shortHash(hash: string | null | undefined, length = 8): string {
  if (!hash) return '—'
  return hash.length <= length ? hash : hash.slice(0, length)
}

/** Last path segment for both POSIX and Windows separators. */
export function basename(path: string | null | undefined): string {
  if (!path) return '—'
  const normalized = path.replace(/\\/g, '/').replace(/\/+$/, '')
  const index = normalized.lastIndexOf('/')
  return index >= 0 ? normalized.slice(index + 1) : normalized
}

export function truncateMiddle(value: string, max = 64): string {
  if (value.length <= max) return value
  const half = Math.floor((max - 1) / 2)
  return `${value.slice(0, half)}…${value.slice(value.length - half)}`
}

/** Split textarea input into trimmed, non-empty lines (deduplicated). */
export function splitLines(value: string): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const line of value.split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed || seen.has(trimmed)) continue
    seen.add(trimmed)
    out.push(trimmed)
  }
  return out
}

/** Parse `Key: value` lines into an object; throws on a line without a colon. */
export function parseKeyValueLines(value: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const line of value.split(/\r?\n/)) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const index = trimmed.indexOf(':')
    if (index <= 0) {
      throw new Error(`无法解析请求头行（缺少冒号）：${trimmed}`)
    }
    const key = trimmed.slice(0, index).trim()
    const val = trimmed.slice(index + 1).trim()
    if (key) out[key] = val
  }
  return out
}

export function asArray<T>(value: T[] | null | undefined): T[] {
  return Array.isArray(value) ? value : []
}

export function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error.length > 0) return error
  if (error instanceof Error && error.message.length > 0) return error.message
  try {
    const text = JSON.stringify(error)
    if (text && text !== '{}' && text !== 'null') return text
  } catch {
    // ignore
  }
  return fallback
}
