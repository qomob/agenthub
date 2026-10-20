import type { Tone } from './status'
import type { CapabilityHealth, HealthState } from './types'

/**
 * Pure helpers for the capability-health surface: health-state semantics,
 * filtering / risk ordering and label maps for the enum strings returned by the
 * Rust backend. Keeping them here makes the semantics unit-testable without
 * mounting any component.
 */

export const HEALTH_STATES: HealthState[] = [
  'verified',
  'needs_attention',
  'unverified',
  'failed',
  'not_deployed',
]

export type HealthFilter = 'all' | HealthState

export const HEALTH_FILTERS: HealthFilter[] = [
  'all',
  'verified',
  'needs_attention',
  'unverified',
  'failed',
  'not_deployed',
]

/** Text + icon + colour: verified=green, unverified=neutral, attention=amber, failed=red, not deployed=blue-grey. */
export const healthStateKey: Record<HealthState, string> = {
  verified: 'status.health.verified',
  unverified: 'status.health.unverified',
  needs_attention: 'status.health.needs_attention',
  failed: 'status.health.failed',
  not_deployed: 'status.health.not_deployed',
}

export const healthStateTone: Record<HealthState, Tone> = {
  verified: 'success',
  unverified: 'neutral',
  needs_attention: 'warn',
  failed: 'danger',
  not_deployed: 'slate',
}

export const healthStateIcon: Record<HealthState, string> = {
  verified: 'check-circle',
  unverified: 'alert-circle',
  needs_attention: 'alert-triangle',
  failed: 'x-circle',
  not_deployed: 'package',
}

/** Lower rank = more urgent; used for the "riskiest first" list ordering. */
export const healthRiskRank: Record<HealthState, number> = {
  failed: 0,
  needs_attention: 1,
  unverified: 2,
  not_deployed: 3,
  verified: 4,
}

export function healthStateLabelKey(state: string): string {
  return healthStateKey[state as HealthState] ?? 'status.health.unknown'
}

export function healthStateToneFor(state: string): Tone {
  return healthStateTone[state as HealthState] ?? 'neutral'
}

export function healthStateIconFor(state: string): string {
  return healthStateIcon[state as HealthState] ?? 'alert-circle'
}

export function healthRiskOf(state: string): number {
  return healthRiskRank[state as HealthState] ?? 5
}

/** First `limit` reasons, used for the compact reasons column. */
export function topHealthReasons(reasons: string[], limit = 2): string[] {
  return reasons.slice(0, Math.max(0, limit))
}

/**
 * Filter by health state and search text (name / slug / description / reasons).
 * `all` keeps every state and an empty query keeps every row.
 */
export function filterHealthCapabilities(
  capabilities: CapabilityHealth[],
  filter: HealthFilter,
  search: string,
): CapabilityHealth[] {
  const query = search.trim().toLowerCase()
  return capabilities.filter((capability) => {
    if (filter !== 'all' && capability.status !== filter) return false
    if (!query) return true
    const haystack = [
      capability.name,
      capability.slug,
      capability.description,
      capability.versionLabel,
      ...capability.reasons,
    ]
      .join(' ')
      .toLowerCase()
    return haystack.includes(query)
  })
}

/** Order by risk (failed → attention → unverified → not deployed → verified), then by name. */
export function sortHealthCapabilities(capabilities: CapabilityHealth[]): CapabilityHealth[] {
  return [...capabilities].sort((a, b) => {
    const rank = healthRiskOf(a.status) - healthRiskOf(b.status)
    if (rank !== 0) return rank
    return a.name.localeCompare(b.name)
  })
}

export function filterAndSortHealthCapabilities(
  capabilities: CapabilityHealth[],
  filter: HealthFilter,
  search: string,
): CapabilityHealth[] {
  return sortHealthCapabilities(filterHealthCapabilities(capabilities, filter, search))
}

// ---------------------------------------------------------------------------
// Audit / dependency / validation label maps (enum strings from model.rs)
// ---------------------------------------------------------------------------

export const auditStatusKey: Record<string, string> = {
  pass: 'status.auditStatus.pass',
  warn: 'status.auditStatus.warn',
  fail: 'status.auditStatus.fail',
  unknown: 'status.auditStatus.unknown',
}

export const auditStatusTone: Record<string, Tone> = {
  pass: 'success',
  warn: 'warn',
  fail: 'danger',
  unknown: 'neutral',
}

export const auditRiskKey: Record<string, string> = {
  info: 'status.auditRisk.info',
  low: 'status.auditRisk.low',
  medium: 'status.auditRisk.medium',
  high: 'status.auditRisk.high',
  critical: 'status.auditRisk.critical',
  unknown: 'status.auditRisk.unknown',
}

export const auditRiskTone: Record<string, Tone> = {
  info: 'neutral',
  low: 'info',
  medium: 'warn',
  high: 'danger',
  critical: 'danger',
  unknown: 'neutral',
}

export const findingSeverityKey: Record<string, string> = {
  info: 'status.findingSeverity.info',
  low: 'status.findingSeverity.low',
  medium: 'status.findingSeverity.medium',
  high: 'status.findingSeverity.high',
  critical: 'status.findingSeverity.critical',
}

export const findingCategoryKey: Record<string, string> = {
  spec: 'status.findingCategory.spec',
  security: 'status.findingCategory.security',
  dependency: 'status.findingCategory.dependency',
  quality: 'status.findingCategory.quality',
  compatibility: 'status.findingCategory.compatibility',
}

export const dependencyStatusKey: Record<string, string> = {
  ok: 'status.dependencyStatus.ok',
  missing: 'status.dependencyStatus.missing',
  warning: 'status.dependencyStatus.warning',
  unknown: 'status.dependencyStatus.unknown',
}

export const dependencyStatusTone: Record<string, Tone> = {
  ok: 'success',
  missing: 'danger',
  warning: 'warn',
  unknown: 'neutral',
}

export const environmentStatusKey: Record<string, string> = {
  healthy: 'status.environmentStatus.healthy',
  needs_attention: 'status.environmentStatus.needs_attention',
  unknown: 'status.environmentStatus.unknown',
}

export const environmentStatusTone: Record<string, Tone> = {
  healthy: 'success',
  needs_attention: 'warn',
  unknown: 'neutral',
}

export const dependencyKindKey: Record<string, string> = {
  runtime: 'status.dependencyKind.runtime',
  binary: 'status.dependencyKind.binary',
  manifest: 'status.dependencyKind.manifest',
  mcp: 'status.dependencyKind.mcp',
  credential: 'status.dependencyKind.credential',
  package: 'status.dependencyKind.package',
}

export const validationStatusKey: Record<string, string> = {
  passed: 'status.validationStatus.passed',
  failed: 'status.validationStatus.failed',
  needs_attention: 'status.validationStatus.needs_attention',
  unverified: 'status.validationStatus.unverified',
}

export const validationStatusTone: Record<string, Tone> = {
  passed: 'success',
  failed: 'danger',
  needs_attention: 'warn',
  unverified: 'neutral',
}

export const validationKindKey: Record<string, string> = {
  manual: 'status.validationKind.manual',
  static: 'status.validationKind.static',
  environment: 'status.validationKind.environment',
  regression: 'status.validationKind.regression',
}

export const fileChangeKey: Record<string, string> = {
  added: 'status.fileChange.added',
  removed: 'status.fileChange.removed',
  modified: 'status.fileChange.modified',
}

export const fileChangeTone: Record<string, Tone> = {
  added: 'success',
  removed: 'danger',
  modified: 'warn',
}

/** Impact classes are free-form strings from the engine; known ones get copy, others render raw. */
export const impactClassKey: Record<string, string> = {
  docs_only: 'health.impactClass.docs_only',
  new_script: 'health.impactClass.new_script',
  removed_script: 'health.impactClass.removed_script',
  scripts_changed: 'health.impactClass.scripts_changed',
  new_network: 'health.impactClass.new_network',
  new_dependency: 'health.impactClass.new_dependency',
  permissions: 'health.impactClass.permissions',
  manifest: 'health.impactClass.manifest',
}

/** Resolve a translated label for a known enum/map entry, else return the raw backend value. */
export function mappedLabel(map: Record<string, string>, value: string): string | null {
  return map[value] ?? null
}

export function toneFor(map: Record<string, Tone>, value: string): Tone {
  return map[value] ?? 'neutral'
}
