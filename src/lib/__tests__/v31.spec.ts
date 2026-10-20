import { describe, expect, it } from 'vitest'

import { setLocale, t } from '../../i18n'
import type { AdapterEvidenceView } from '../types'
import {
  approvalErrorHintKey,
  approvalStateInfo,
  canRecoverOperation,
  capabilityAvailabilityInfo,
  evidenceLevelInfo,
  featureLabelKey,
  formatPlanHash,
  hasManualRecovery,
  isCapabilityAvailable,
  isDeadlineExpired,
  recoveryItemInfo,
  recoveryStatusInfo,
  resolveApprovalState,
  supportStateInfo,
} from '../v31'

const NOW = Date.parse('2026-10-20T12:00:00Z')

function evidence(overrides: Partial<AdapterEvidenceView> = {}): AdapterEvidenceView {
  return {
    id: 'ev-1',
    agentKey: 'codex',
    featureKey: 'read_skills',
    supportState: 'supported',
    evidenceLevel: 'verified_runtime',
    sourceUrl: null,
    testedAt: '2026-10-20T00:00:00Z',
    agentVersion: '1.0.0',
    adapterVersion: '0.4.0',
    notes: null,
    ...overrides,
  }
}

describe('approval formatting', () => {
  it('shows only the first 12 plan-hash characters and never invents one', () => {
    const hash = '0123456789abcdef'.repeat(4)
    expect(formatPlanHash(hash)).toBe('0123456789ab')
    expect(formatPlanHash(hash, 8)).toBe('01234567')
    expect(formatPlanHash('abc')).toBe('abc')
    expect(formatPlanHash(null)).toBe('—')
    expect(formatPlanHash('')).toBe('—')
  })

  it('resolves approved/expired/none states and their badge copy', () => {
    const future = '2026-10-20T12:15:00Z'
    const past = '2026-10-20T11:45:00Z'

    expect(
      resolveApprovalState({ approvalState: 'approved', expiresAt: future }, NOW),
    ).toBe('approved')
    expect(resolveApprovalState({ approvalState: 'approved', expiresAt: past }, NOW)).toBe(
      'expired',
    )
    expect(resolveApprovalState({ approvalState: 'none', expiresAt: future }, NOW)).toBe('none')
    expect(isDeadlineExpired(past, NOW)).toBe(true)
    expect(isDeadlineExpired(future, NOW)).toBe(false)
    expect(isDeadlineExpired(null, NOW)).toBe(false)

    expect(approvalStateInfo('none')).toMatchObject({
      key: 'status.approval.none',
      requiresReapproval: true,
    })
    expect(approvalStateInfo('approved')).toMatchObject({
      key: 'status.approval.approved',
      tone: 'success',
      requiresReapproval: false,
    })
    const expired = approvalStateInfo('expired')
    expect(expired.key).toBe('status.approval.expired')
    expect(expired.tone).toBe('danger')
    expect(expired.requiresReapproval).toBe(true)
    expect(approvalStateInfo('something-new').key).toBeNull()
  })

  it('maps backend error codes to actionable approval hints', () => {
    expect(approvalErrorHintKey('PLAN_STALE')).toBe('plan.staleHint')
    expect(approvalErrorHintKey('EXTERNAL_MODIFICATION')).toBe('plan.staleHint')
    expect(approvalErrorHintKey('PERMISSION_DENIED')).toBe('plan.deniedHint')
    expect(approvalErrorHintKey('UNVERIFIED_ADAPTER')).toBe('plan.unsupportedHint')
    expect(approvalErrorHintKey('INTERNAL')).toBe('plan.approvalFailedHint')
    expect(approvalErrorHintKey(null)).toBe('plan.approvalFailedHint')
  })
})

describe('recovery display', () => {
  it('offers recovery only for failed, partial or interrupted operations', () => {
    expect(canRecoverOperation('failed')).toBe(true)
    expect(canRecoverOperation('partial')).toBe(true)
    expect(canRecoverOperation('needs_manual_recovery')).toBe(true)
    expect(canRecoverOperation('running')).toBe(true)
    expect(canRecoverOperation('success')).toBe(false)
    expect(canRecoverOperation('rolled_back')).toBe(false)
  })

  it('flags a report as manual recovery when any target was changed elsewhere', () => {
    const clean = {
      status: 'recovered',
      items: [
        {
          planItemId: '',
          agentKey: 'codex',
          targetPath: '/tmp/skills/demo',
          status: 'success',
          message: 'Restored pre-operation content',
          backupPath: null,
          installationId: null,
        },
      ],
    }
    expect(hasManualRecovery(clean)).toBe(false)
    expect(recoveryStatusInfo(clean.status).key).toBe('status.recovery.recovered')

    const manual = {
      status: 'needs_manual_recovery',
      items: [
        {
          planItemId: '',
          agentKey: 'codex',
          targetPath: '/tmp/skills/demo',
          status: 'failed',
          message: 'External modification detected; manual recovery required',
          backupPath: null,
          installationId: null,
        },
      ],
    }
    expect(hasManualRecovery(manual)).toBe(true)
    expect(recoveryStatusInfo(manual.status).key).toBe('status.recovery.needs_manual_recovery')
    expect(recoveryItemInfo(manual.items[0].status).key).toBe('status.result.failed')
    expect(recoveryItemInfo(clean.items[0].status).key).toBe('status.result.success')
    expect(recoveryItemInfo('unknown-state').key).toBeNull()
  })

  it('uses the paused-overwrite copy for manual recovery in Chinese', () => {
    setLocale('zh')
    expect(t('activity.recoverManualNotice')).toBe('文件已被其他程序修改，已暂停覆盖。')
    expect(t('status.recovery.needs_manual_recovery')).toBe('需人工恢复')
  })
})

describe('adapter evidence copy', () => {
  it('never presents an unverified feature as available', () => {
    setLocale('zh')
    const unverifiedSupport = evidence({ supportState: 'unverified', evidenceLevel: 'unverified' })
    const supportedButUnverified = evidence({
      supportState: 'supported',
      evidenceLevel: 'unverified',
    })
    const supportedConfig = evidence({
      supportState: 'supported',
      evidenceLevel: 'verified_config',
    })
    const runtimeVerified = evidence({ supportState: 'supported', evidenceLevel: 'verified_runtime' })

    expect(supportStateInfo('unverified').key).toBe('status.support.unverified')
    expect(t('status.support.unverified')).toBe('未验证/仅配置')
    expect(t('status.evidence.unverified')).toBe('未验证')

    expect(isCapabilityAvailable(unverifiedSupport)).toBe(false)
    expect(isCapabilityAvailable(supportedButUnverified)).toBe(false)
    expect(isCapabilityAvailable(supportedConfig)).toBe(true)
    expect(isCapabilityAvailable(runtimeVerified)).toBe(true)

    const unverifiedVerdict = capabilityAvailabilityInfo(supportedButUnverified)
    expect(unverifiedVerdict.available).toBe(false)
    expect(unverifiedVerdict.key).toBe('agents.capabilitySupportUnverified')
    expect(t(unverifiedVerdict.key)).toContain('未经验证')
    expect(t(unverifiedVerdict.key)).toContain('不可视为可用')

    expect(capabilityAvailabilityInfo(runtimeVerified)).toMatchObject({
      available: true,
      key: 'agents.capabilityRuntimeVerified',
      tone: 'success',
    })
    expect(capabilityAvailabilityInfo(supportedConfig)).toMatchObject({
      available: true,
      key: 'agents.capabilityConfigVerified',
    })
    // Newer cores store evidence vocabulary in support_state for read features.
    const configSupport = evidence({
      supportState: 'verified_config',
      evidenceLevel: 'verified_config',
    })
    expect(supportStateInfo('verified_config').key).toBe('status.support.verified_config')
    expect(isCapabilityAvailable(configSupport)).toBe(true)
    expect(capabilityAvailabilityInfo(configSupport)).toMatchObject({
      available: true,
      key: 'agents.capabilityConfigVerified',
    })
    expect(
      capabilityAvailabilityInfo(evidence({ supportState: 'unsupported', evidenceLevel: 'unsupported' })),
    ).toMatchObject({ available: false, key: 'agents.capabilityUnsupported' })
    expect(
      capabilityAvailabilityInfo(evidence({ supportState: 'manual', evidenceLevel: 'manual' })),
    ).toMatchObject({ available: false })
  })

  it('labels known features and falls back to the raw key for unknown ones', () => {
    setLocale('zh')
    expect(featureLabelKey('read_skills')).toBe('agents.features.read_skills')
    expect(featureLabelKey('read_plugins')).toBe('agents.features.read_plugins')
    expect(t('agents.features.write_mcp')).toBe('写入 MCP')
    expect(featureLabelKey('future_feature')).toBeNull()
    expect(evidenceLevelInfo('verified_runtime').key).toBe('status.evidence.verified_runtime')
    expect(t('status.evidence.verified_runtime')).toBe('运行已验证')
    expect(evidenceLevelInfo('assumed').tone).toBe('warn')
  })
})
