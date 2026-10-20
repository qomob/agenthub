import { describe, expect, it } from 'vitest'

import {
  filterAndSortHealthCapabilities,
  filterHealthCapabilities,
  healthRiskOf,
  healthStateIconFor,
  healthStateLabelKey,
  healthStateToneFor,
  sortHealthCapabilities,
  topHealthReasons,
} from '../health'
import type { CapabilityHealth, HealthState } from '../types'

function capability(overrides: Partial<CapabilityHealth> = {}): CapabilityHealth {
  return {
    capabilityId: 'cap-1',
    slug: 'demo',
    name: 'Demo',
    description: '',
    versionLabel: '1.0.0',
    status: 'unverified',
    reasons: [],
    audit: null,
    environment: null,
    validations: [],
    installationCount: 0,
    agentKeys: [],
    verifiedAgentKeys: [],
    lastCheckedAt: null,
    ...overrides,
  }
}

describe('health state semantics', () => {
  it('maps every state to a distinct tone, icon and translation key', () => {
    const states: HealthState[] = [
      'verified',
      'unverified',
      'needs_attention',
      'failed',
      'not_deployed',
    ]
    expect(states.map(healthStateToneFor)).toEqual([
      'success',
      'neutral',
      'warn',
      'danger',
      'slate',
    ])
    expect(states.map(healthStateIconFor)).toEqual([
      'check-circle',
      'alert-circle',
      'alert-triangle',
      'x-circle',
      'package',
    ])
    expect(new Set(states.map(healthStateLabelKey)).size).toBe(states.length)
    expect(healthStateLabelKey('verified')).toBe('status.health.verified')
  })

  it('falls back to a neutral, explicit unknown state for unexpected backend values', () => {
    expect(healthStateToneFor('mystery')).toBe('neutral')
    expect(healthStateIconFor('mystery')).toBe('alert-circle')
    expect(healthStateLabelKey('mystery')).toBe('status.health.unknown')
    expect(healthRiskOf('mystery')).toBeGreaterThan(healthRiskOf('verified'))
  })

  it('caps the reasons column at two entries', () => {
    expect(topHealthReasons(['a', 'b', 'c'])).toEqual(['a', 'b'])
    expect(topHealthReasons([], 2)).toEqual([])
  })
})

describe('health list filtering and risk ordering', () => {
  const list: CapabilityHealth[] = [
    capability({ capabilityId: 'a', name: 'Alpha', status: 'verified' }),
    capability({ capabilityId: 'b', name: 'Bravo', status: 'needs_attention' }),
    capability({ capabilityId: 'c', name: 'Charlie', status: 'failed' }),
    capability({ capabilityId: 'd', name: 'Delta', status: 'not_deployed' }),
    capability({
      capabilityId: 'e',
      name: 'Echo',
      slug: 'echo-search',
      status: 'unverified',
      reasons: ['never audited'],
    }),
  ]

  it('filters by state and by search text across metadata and reasons', () => {
    expect(filterHealthCapabilities(list, 'failed', '').map((item) => item.capabilityId)).toEqual([
      'c',
    ])
    expect(
      filterHealthCapabilities(list, 'all', 'echo-search').map((item) => item.capabilityId),
    ).toEqual(['e'])
    expect(
      filterHealthCapabilities(list, 'all', 'never audited').map((item) => item.capabilityId),
    ).toEqual(['e'])
    expect(filterHealthCapabilities(list, 'verified', 'echo')).toEqual([])
  })

  it('orders failed → attention → unverified → not deployed → verified, then by name', () => {
    const sorted = sortHealthCapabilities(list).map((item) => item.capabilityId)
    expect(sorted).toEqual(['c', 'b', 'e', 'd', 'a'])
  })

  it('combines filtering and risk ordering in one pass', () => {
    const riskier = filterAndSortHealthCapabilities(list, 'all', 'a')
    expect(riskier.length).toBeGreaterThan(0)
    expect(riskier[0].status).toBe('failed')
  })
})
