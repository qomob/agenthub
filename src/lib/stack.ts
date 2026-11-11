import type {
  BillingPeriod,
  BillingType,
  StackDetectionState,
  StackItemSource,
  StackSubscriptionState,
  SubscriptionStatus,
} from './types'

// ---------------------------------------------------------------------------
// My AI Stack (M1-B) — display helpers. No business rules live here: status
// derivations, folding and aggregation stay in agenthub-core; these helpers
// only format what the core already decided.
// ---------------------------------------------------------------------------

export const stackDetectionStateKey: Record<StackDetectionState, string> = {
  installed: 'stack.detectionState.installed',
  not_installed: 'stack.detectionState.not_installed',
  unknown: 'stack.detectionState.unknown',
}

export const stackSourceKey: Record<StackItemSource, string> = {
  detected: 'stack.source.detected',
  manual: 'stack.source.manual',
}

export const stackSubscriptionStateKey: Record<StackSubscriptionState, string> = {
  registered: 'stack.subscriptionState.registered',
  none: 'stack.subscriptionState.none',
}

export const billingTypeKey: Record<BillingType, string> = {
  free: 'stack.billingType.free',
  subscription: 'stack.billingType.subscription',
  usage: 'stack.billingType.usage',
  hybrid: 'stack.billingType.hybrid',
  unknown: 'stack.billingType.unknown',
}

export const billingPeriodKey: Record<BillingPeriod, string> = {
  monthly: 'stack.billingPeriod.monthly',
  quarterly: 'stack.billingPeriod.quarterly',
  semiannual: 'stack.billingPeriod.semiannual',
  yearly: 'stack.billingPeriod.yearly',
  usage: 'stack.billingPeriod.usage',
  unknown: 'stack.billingPeriod.unknown',
}

export const subscriptionStatusKey: Record<SubscriptionStatus, string> = {
  active: 'stack.subscriptionStatus.active',
  paused: 'stack.subscriptionStatus.paused',
  cancelled: 'stack.subscriptionStatus.cancelled',
  expired: 'stack.subscriptionStatus.expired',
  unknown: 'stack.subscriptionStatus.unknown',
}

/** i18n key per missing-cost reason (IncompleteItem.missing entries). */
export const missingReasonKey: Record<string, string> = {
  amount: 'stack.missing.amount',
  currency: 'stack.missing.currency',
  billing_period: 'stack.missing.billing_period',
  billing_type: 'stack.missing.billing_type',
  usage_not_foldable: 'stack.missing.usage_not_foldable',
}

/** ISO-4217 minor-unit exponents (subset; mirrors core money.rs). */
const CURRENCY_EXPONENTS: Record<string, number> = {
  JPY: 0,
  KWD: 3,
  BHD: 3,
  OMR: 3,
  JOD: 3,
  TND: 3,
}

export function currencyExponent(currency: string): number {
  return CURRENCY_EXPONENTS[currency] ?? 2
}

/**
 * Format integer minor units as a display string, using integer arithmetic
 * only (no float division): `123456, USD` → `1,234.56 USD`.
 * Returns `undefined` for unknown amounts so callers can show an explicit
 * "amount unknown" label instead of a fake zero.
 */
export function formatMoneyMinor(
  amountMinor: number | null | undefined,
  currency: string | null | undefined,
): string | undefined {
  if (amountMinor === null || amountMinor === undefined || !Number.isFinite(amountMinor)) {
    return undefined
  }
  if (!currency) return undefined
  const exponent = currencyExponent(currency)
  const negative = amountMinor < 0
  const abs = Math.abs(Math.trunc(amountMinor))
  const denominator = 10 ** exponent
  const whole = Math.floor(abs / denominator)
  const fraction = abs - whole * denominator
  const wholeText = whole.toLocaleString('en-US')
  if (exponent === 0) {
    return `${negative ? '-' : ''}${wholeText} ${currency}`
  }
  const fractionText = String(fraction).padStart(exponent, '0')
  return `${negative ? '-' : ''}${wholeText}.${fractionText} ${currency}`
}

/**
 * Parse a user-entered amount in MAJOR units (e.g. `20.5`) into integer minor
 * units using integer arithmetic only — no float multiplication. Returns
 * `undefined` for blank input (unknown amount) and `null` for invalid input.
 */
export function parseMoneyMajor(
  text: string,
  currency: string | null | undefined,
): number | null | undefined {
  const trimmed = text.trim()
  if (trimmed === '') return undefined
  if (!currency) return null
  const exponent = currencyExponent(currency)
  if (exponent === 0) {
    if (!/^-?\d+$/.test(trimmed)) return null
    const value = Number(trimmed)
    return Number.isSafeInteger(value) ? value : null
  }
  const match = /^(-)?(\d+)(?:\.(\d+))?$/.exec(trimmed)
  if (!match) return null
  const [, negative, whole, fractionRaw = ''] = match
  const fractionText = (fractionRaw + '0'.repeat(exponent)).slice(0, exponent)
  const wholeNumber = Number(whole)
  const fractionNumber = Number(fractionText)
  const value = wholeNumber * 10 ** exponent + fractionNumber
  if (!Number.isSafeInteger(value)) return null
  return negative ? -value : value
}
