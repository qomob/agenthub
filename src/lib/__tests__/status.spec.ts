import { describe, expect, it } from 'vitest'

import {
  countPlanActions,
  matrixCellState,
  planHasBlockedItems,
  planNeedsForce,
} from '../status'
import type {
  AgentView,
  InstallationView,
  OperationPlan,
  PlanItem,
  Skill,
} from '../types'

function agent(overrides: Partial<AgentView> = {}): AgentView {
  return {
    id: 'agent-1',
    key: 'codex',
    name: 'Codex',
    vendor: 'OpenAI',
    adapterId: 'builtin.codex',
    status: 'installed',
    appPath: null,
    homePath: null,
    customSkillRoots: [],
    detectedAt: null,
    lastScanAt: null,
    skillRoots: [],
    mcpTargets: [],
    pluginSupport: 'discovery_only',
    symlinkSupport: 'unverified',
    notes: [],
    skillCount: 0,
    installedSkillCount: 0,
    ...overrides,
  }
}

function installation(overrides: Partial<InstallationView> = {}): InstallationView {
  return {
    id: 'inst-1',
    capabilityId: 'skill-1',
    versionId: 'v1',
    versionLabel: '1.0.0',
    agentId: 'agent-1',
    agentKey: 'codex',
    agentName: 'Codex',
    scope: 'global',
    strategy: 'copy',
    targetPath: '/tmp/skills/demo',
    state: 'installed',
    contentHash: 'hash',
    installedAt: '2026-10-20T00:00:00Z',
    verifiedAt: null,
    lastCheckAt: null,
    drift: false,
    warnings: [],
    ...overrides,
  }
}

function skill(overrides: Partial<Skill> = {}): Skill {
  return {
    id: 'skill-1',
    slug: 'demo',
    name: 'Demo',
    description: '',
    version: '1.0.0',
    contentHash: 'hash',
    tags: [],
    license: null,
    source: null,
    currentVersionId: 'v1',
    sizeBytes: 10,
    fileCount: 1,
    createdAt: '2026-10-20T00:00:00Z',
    updatedAt: '2026-10-20T00:00:00Z',
    installations: [],
    updateAvailable: false,
    driftCount: 0,
    conflict: null,
    ...overrides,
  }
}

function planItem(action: PlanItem['action']): PlanItem {
  return {
    id: `item-${action}`,
    capabilityId: 'skill-1',
    capabilityName: 'Demo',
    versionId: 'v1',
    versionLabel: '1.0.0',
    agentId: 'agent-1',
    agentKey: 'codex',
    agentName: 'Codex',
    installKind: 'skill_dir',
    action,
    strategy: 'copy',
    targetPath: '/tmp/demo',
    sourcePath: null,
    currentHash: null,
    desiredHash: null,
    configKey: null,
    targetFormat: null,
    payload: null,
    warnings: [],
    requiresRestart: false,
    diff: null,
    previousInstallation: null,
  }
}

function plan(actions: PlanItem['action'][]): OperationPlan {
  const items = actions.map(planItem)
  return {
    operationId: 'op-1',
    kind: 'install',
    dryRun: true,
    items,
    warnings: [],
    requiresRestartAgents: [],
    requiresConfirmation: false,
    summary: countPlanActions(items),
    createdAt: '2026-10-20T00:00:00Z',
  }
}

describe('matrixCellState', () => {
  it('marks a targetable agent without an installation as not installed', () => {
    const cell = matrixCellState(skill(), agent())
    expect(cell.state).toBe('not_installed')
    expect(cell.selectableForInstall).toBe(true)
    expect(cell.selectableForUninstall).toBe(false)
  })

  it('marks unsupported / missing agents as not installable', () => {
    const cell = matrixCellState(skill(), agent({ status: 'not_installed' }))
    expect(cell.state).toBe('unsupported')
    expect(cell.selectableForInstall).toBe(false)
  })

  it('requires verification for detected-but-unverified agents', () => {
    const cell = matrixCellState(skill(), agent({ status: 'verification_required' }))
    expect(cell.state).toBe('verification')
    expect(cell.selectableForInstall).toBe(true)
  })

  it('surfaces drift, updates and missing states with the right precedence', () => {
    const drifted = matrixCellState(
      skill({ installations: [installation({ drift: true })] }),
      agent(),
    )
    expect(drifted.state).toBe('drift')
    expect(drifted.selectableForUninstall).toBe(true)

    const outdated = matrixCellState(
      skill({ installations: [installation()], updateAvailable: true }),
      agent(),
    )
    expect(outdated.state).toBe('outdated')

    const missing = matrixCellState(
      skill({ installations: [installation({ state: 'missing', drift: true })] }),
      agent(),
    )
    expect(missing.state).toBe('missing')
  })

  it('reports a plain installation as installed', () => {
    const cell = matrixCellState(skill({ installations: [installation()] }), agent())
    expect(cell.state).toBe('installed')
    expect(cell.installationId).toBe('inst-1')
  })
})

describe('plan helpers', () => {
  it('counts actions from plan items', () => {
    const summary = countPlanActions([
      planItem('create'),
      planItem('create'),
      planItem('replace'),
      planItem('conflict'),
      planItem('blocked'),
      planItem('noop'),
    ])
    expect(summary).toEqual({
      create: 2,
      replace: 1,
      remove: 0,
      noop: 1,
      conflict: 1,
      blocked: 1,
    })
  })

  it('only requires force for conflicts, while blocked items are reported separately', () => {
    expect(planNeedsForce(plan(['create', 'noop']))).toBe(false)
    expect(planNeedsForce(plan(['conflict']))).toBe(true)
    expect(planHasBlockedItems(plan(['blocked']))).toBe(true)
    expect(planHasBlockedItems(plan(['create']))).toBe(false)
  })
})
