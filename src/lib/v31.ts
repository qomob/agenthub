/**
 * v3.1 desktop semantics that are shared by the approval dialog, the recovery
 * panel and the agent evidence table. Kept free of Vue/Tauri imports so the
 * Vitest suite can exercise them directly.
 */

import type { Tone } from './status'
import type {
  AdapterEvidenceView,
  ApprovalState,
  OperationStatus,
  OperationView,
  RecoveryReport,
} from './types'

// ---------------------------------------------------------------------------
// Approval surface
// ---------------------------------------------------------------------------

/** Display form of a plan hash: first 12 hex chars, full hash stays available
 * in a tooltip. Never invent a value when the backend did not provide one. */
export function formatPlanHash(hash: string | null | undefined, length = 12): string {
  if (!hash) return '—'
  return hash.length <= length ? hash : hash.slice(0, length)
}

/** `true` when an RFC3339 timestamp is absent or already in the past. */
export function isDeadlineExpired(
  deadline: string | null | undefined,
  now: number = Date.now(),
): boolean {
  if (!deadline) return false
  const timestamp = Date.parse(deadline)
  return Number.isNaN(timestamp) || timestamp <= now
}

/**
 * Effective approval state for display. The backend already recomputes
 * `expired`; this protects the UI when the clock advanced after the last load.
 */
export function resolveApprovalState(
  view: Pick<OperationView, 'approvalState' | 'expiresAt'>,
  now: number = Date.now(),
): ApprovalState {
  const state = view.approvalState
  if (state !== 'approved') {
    return state === 'expired' ? 'expired' : 'none'
  }
  return isDeadlineExpired(view.expiresAt, now) ? 'expired' : 'approved'
}

export interface ApprovalStateInfo {
  /** i18n key, or null when the backend introduced an unknown state. */
  key: string | null
  tone: Tone
  /** Whether a fresh approval must be requested before applying. */
  requiresReapproval: boolean
}

const APPROVAL_STATE_INFO: Record<ApprovalState, ApprovalStateInfo> = {
  none: { key: 'status.approval.none', tone: 'warn', requiresReapproval: true },
  approved: { key: 'status.approval.approved', tone: 'success', requiresReapproval: false },
  expired: { key: 'status.approval.expired', tone: 'danger', requiresReapproval: true },
}

export function approvalStateInfo(state: string): ApprovalStateInfo {
  return (
    APPROVAL_STATE_INFO[state as ApprovalState] ?? {
      key: null,
      tone: 'neutral',
      requiresReapproval: true,
    }
  )
}

/** Explains why an approved apply failed, using the stable error code. */
export function approvalErrorHintKey(code: string | null | undefined): string {
  switch (code) {
    case 'PLAN_STALE':
    case 'EXTERNAL_MODIFICATION':
      return 'plan.staleHint'
    case 'PERMISSION_DENIED':
      return 'plan.deniedHint'
    case 'UNVERIFIED_ADAPTER':
    case 'UNSUPPORTED_CAPABILITY':
      return 'plan.unsupportedHint'
    default:
      return 'plan.approvalFailedHint'
  }
}

// ---------------------------------------------------------------------------
// Recovery surface
// ---------------------------------------------------------------------------

/** Operations that can safely offer the manual “尝试恢复” entry point. */
export function canRecoverOperation(status: OperationStatus | string): boolean {
  return (
    status === 'failed' ||
    status === 'partial' ||
    status === 'needs_manual_recovery' ||
    status === 'running'
  )
}

export interface RecoveryStatusInfo {
  key: string | null
  tone: Tone
}

const RECOVERY_STATUS_INFO: Record<string, RecoveryStatusInfo> = {
  recovered: { key: 'status.recovery.recovered', tone: 'success' },
  needs_manual_recovery: { key: 'status.recovery.needs_manual_recovery', tone: 'danger' },
}

export function recoveryStatusInfo(status: string): RecoveryStatusInfo {
  return RECOVERY_STATUS_INFO[status] ?? { key: null, tone: 'warn' }
}

const RECOVERY_ITEM_STATUS_INFO: Record<string, RecoveryStatusInfo> = {
  success: { key: 'status.result.success', tone: 'success' },
  noop: { key: 'status.result.noop', tone: 'neutral' },
  skipped: { key: 'status.result.skipped', tone: 'warn' },
  failed: { key: 'status.result.failed', tone: 'danger' },
}

export function recoveryItemInfo(status: string): RecoveryStatusInfo {
  return RECOVERY_ITEM_STATUS_INFO[status] ?? { key: null, tone: 'neutral' }
}

/**
 * `true` when at least one target was modified by another program and was
 * deliberately left untouched: the UI must show the paused-overwrite copy and
 * must not claim the operation was fully restored.
 */
export function hasManualRecovery(report: Pick<RecoveryReport, 'status' | 'items'>): boolean {
  return (
    report.status === 'needs_manual_recovery' ||
    report.items.some((item) => item.status === 'failed')
  )
}

// ---------------------------------------------------------------------------
// Adapter evidence surface
// ---------------------------------------------------------------------------

const FEATURE_LABEL_KEYS: Record<string, string> = {
  detect: 'agents.features.detect',
  read_skills: 'agents.features.read_skills',
  write_skills: 'agents.features.write_skills',
  read_mcp: 'agents.features.read_mcp',
  write_mcp: 'agents.features.write_mcp',
  read_plugins: 'agents.features.read_plugins',
  read_rules: 'agents.features.read_rules',
  write_rules: 'agents.features.write_rules',
  model_config: 'agents.features.model_config',
  manage_plugins: 'agents.features.manage_plugins',
  execute: 'agents.features.execute',
  observe_usage: 'agents.features.observe_usage',
  hot_reload: 'agents.features.hot_reload',
}

export function featureLabelKey(featureKey: string): string | null {
  return FEATURE_LABEL_KEYS[featureKey] ?? null
}

/**
 * Support vocabulary mirrors `AGENT_SUPPORT_MATRIX.csv` and the conservative
 * seeding in `service.rs`: newer cores store the evidence vocabulary in
 * `support_state` for read features (e.g. `verified_config`), so both are
 * accepted instead of rendering raw machine strings.
 */
const SUPPORT_STATE_INFO: Record<string, RecoveryStatusInfo> = {
  supported: { key: 'status.support.supported', tone: 'success' },
  partial: { key: 'status.support.partial', tone: 'warn' },
  manual: { key: 'status.support.manual', tone: 'warn' },
  unsupported: { key: 'status.support.unsupported', tone: 'neutral' },
  unverified: { key: 'status.support.unverified', tone: 'warn' },
  verified_runtime: { key: 'status.support.verified_runtime', tone: 'success' },
  verified_config: { key: 'status.support.verified_config', tone: 'info' },
}

export function supportStateInfo(state: string): RecoveryStatusInfo {
  return SUPPORT_STATE_INFO[state] ?? { key: null, tone: 'neutral' }
}

/** Evidence levels mirror `AdapterEvidenceView` / `AGENT_SUPPORT_MATRIX.csv`. */
const EVIDENCE_LEVEL_INFO: Record<string, RecoveryStatusInfo> = {
  verified_runtime: { key: 'status.evidence.verified_runtime', tone: 'success' },
  verified_config: { key: 'status.evidence.verified_config', tone: 'info' },
  documented: { key: 'status.evidence.documented', tone: 'info' },
  community: { key: 'status.evidence.community', tone: 'neutral' },
  assumed: { key: 'status.evidence.assumed', tone: 'warn' },
  manual: { key: 'status.evidence.manual', tone: 'warn' },
  unsupported: { key: 'status.evidence.unsupported', tone: 'neutral' },
  unverified: { key: 'status.evidence.unverified', tone: 'warn' },
}

export function evidenceLevelInfo(level: string): RecoveryStatusInfo {
  return EVIDENCE_LEVEL_INFO[level] ?? { key: null, tone: 'neutral' }
}

/** Evidence levels that justify calling a capability "available". */
const VERIFIED_EVIDENCE_LEVELS = new Set(['verified_runtime', 'verified_config'])
/** Support states that (with verified evidence) mean the capability works. */
const SUPPORTED_STATES = new Set([
  'supported',
  'partial',
  'verified_runtime',
  'verified_config',
])

/**
 * A capability is only presented as available when support is real AND the
 * evidence level is verified. `unverified` and every weaker level stay
 * explicitly unverified — the UI must never promote them to "可用".
 */
export function isCapabilityAvailable(
  item: Pick<AdapterEvidenceView, 'supportState' | 'evidenceLevel'>,
): boolean {
  return (
    SUPPORTED_STATES.has(item.supportState) &&
    VERIFIED_EVIDENCE_LEVELS.has(item.evidenceLevel)
  )
}

export interface CapabilityAvailabilityInfo {
  available: boolean
  key: string
  tone: Tone
}

/**
 * Combined support × evidence verdict shown in the agent detail table.
 * `unverified` copy is shared with the support badge: “未验证/仅配置”.
 */
export function capabilityAvailabilityInfo(
  item: Pick<AdapterEvidenceView, 'supportState' | 'evidenceLevel'>,
): CapabilityAvailabilityInfo {
  if (item.evidenceLevel === 'unverified') {
    if (SUPPORTED_STATES.has(item.supportState)) {
      return { available: false, key: 'agents.capabilitySupportUnverified', tone: 'warn' }
    }
    return { available: false, key: 'status.support.unverified', tone: 'warn' }
  }
  if (item.supportState === 'unsupported' || item.evidenceLevel === 'unsupported') {
    return { available: false, key: 'agents.capabilityUnsupported', tone: 'neutral' }
  }
  if (item.supportState === 'unverified' || item.supportState === 'manual') {
    return { available: false, key: 'status.support.unverified', tone: 'warn' }
  }
  if (isCapabilityAvailable(item)) {
    return {
      available: true,
      key:
        item.evidenceLevel === 'verified_runtime'
          ? 'agents.capabilityRuntimeVerified'
          : 'agents.capabilityConfigVerified',
      tone: item.evidenceLevel === 'verified_runtime' ? 'success' : 'info',
    }
  }
  return { available: false, key: 'agents.capabilityUnverified', tone: 'warn' }
}
