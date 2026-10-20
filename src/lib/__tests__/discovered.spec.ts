import { describe, expect, it } from 'vitest'

import {
  buildScanImportRequest,
  countDefaultImportableGroups,
  countValidGroups,
  filterDiscoveredGroups,
  groupDiscoveredSkills,
  isImportableGroup,
  isReadOnlyRootSource,
  normalizeRootSource,
  pageDiscoveredGroups,
  selectableGroupKeys,
  selectionSummary,
} from '../discovered'
import type { AgentScan, DiscoveredSkill, ScanReport } from '../types'

function skill(overrides: Partial<DiscoveredSkill> = {}): DiscoveredSkill {
  return {
    name: 'Demo skill',
    slug: 'demo',
    description: 'A demo skill',
    version: '1.0.0',
    path: '/Users/me/.codex/skills/demo',
    root: '/Users/me/.codex/skills',
    rootSource: 'adapter',
    scope: 'global',
    agentKey: 'codex',
    contentHash: 'hash-a',
    fileCount: 3,
    sizeBytes: 1024,
    valid: true,
    issues: [],
    librarySkillId: null,
    libraryVersionId: null,
    inSync: false,
    conflict: null,
    ...overrides,
  }
}

function agent(
  agentKey: string,
  agentName: string,
  skills: DiscoveredSkill[],
): AgentScan {
  return {
    agentId: `id-${agentKey}`,
    agentKey,
    agentName,
    status: 'installed',
    roots: [],
    skills,
    errors: [],
  }
}

function groupKey(groups: ReturnType<typeof groupDiscoveredSkills>, slug: string): string {
  const group = groups.find((item) => item.slug === slug)
  if (!group) throw new Error(`group not found: ${slug}`)
  return group.key
}

function report(agents: AgentScan[]): ScanReport {
  return {
    scannedAt: '2026-10-20T00:00:00Z',
    agents,
    totalSkills: agents.reduce((sum, item) => sum + item.skills.length, 0),
    duplicates: 0,
    conflicts: 0,
    invalid: 0,
    warnings: [],
  }
}

describe('groupDiscoveredSkills', () => {
  it('merges copies from different agents by slug + content hash', () => {
    const groups = groupDiscoveredSkills(
      report([
        agent('codex', 'Codex', [
          skill({
            path: '/a/demo',
            sizeBytes: 1024,
            issues: ['first issue'],
          }),
        ]),
        agent('trae', 'TRAE', [
          skill({
            path: '/b/demo',
            agentKey: 'trae',
            sizeBytes: 2048,
            fileCount: 5,
            issues: ['second issue'],
            librarySkillId: 'library-1',
          }),
        ]),
      ]),
    )

    expect(groups).toHaveLength(1)
    const group = groups[0]
    expect(group.slug).toBe('demo')
    expect(group.agents.map((item) => item.agentName)).toEqual(['Codex', 'TRAE'])
    expect(group.paths).toEqual(['/a/demo', '/b/demo'])
    expect(group.sizeBytes).toBe(2048)
    expect(group.fileCount).toBe(5)
    expect(group.issues).toEqual(['first issue', 'second issue'])
    expect(group.inLibrary).toBe(true)
    expect(group.valid).toBe(true)
  })

  it('separates different content and falls back to path for empty hashes', () => {
    const groups = groupDiscoveredSkills(
      report([
        agent('codex', 'Codex', [
          skill({ path: '/a/demo', contentHash: 'hash-a' }),
          skill({ path: '/b/demo', name: 'Other version', contentHash: 'hash-b' }),
          skill({ path: '/c/broken', slug: 'broken', contentHash: '', valid: false }),
          skill({ path: '/d/broken', slug: 'broken', contentHash: '', valid: false }),
        ]),
      ]),
    )

    expect(groups).toHaveLength(4)
    const broken = groups.filter((group) => group.slug === 'broken')
    expect(broken).toHaveLength(2)
    expect(broken.every((group) => !group.valid && group.invalidCount === 1)).toBe(true)
  })

  it('prefers a valid writable representative and detects read-only-only groups', () => {
    const groups = groupDiscoveredSkills(
      report([
        agent('codex', 'Codex', [
          skill({
            path: '/market/demo',
            rootSource: 'marketplace',
            valid: true,
          }),
          skill({
            path: '/adapter/demo',
            rootSource: 'adapter',
            valid: false,
          }),
        ]),
        agent('trae', 'TRAE', [
          skill({
            path: '/market/other',
            slug: 'other',
            rootSource: 'marketplace',
            agentKey: 'trae',
          }),
        ]),
      ]),
    )

    const demo = groups.find((group) => group.slug === 'demo')!
    expect(demo.representativePath).toBe('/market/demo')
    expect(demo.representativeRootSource).toBe('marketplace')
    expect(demo.hasWritablePath).toBe(true)
    expect(demo.readOnlyOnly).toBe(false)
    expect(isImportableGroup(demo, false)).toBe(false)
    expect(isImportableGroup(demo, true)).toBe(true)

    const other = groups.find((group) => group.slug === 'other')!
    expect(other.readOnlyOnly).toBe(true)
    expect(other.representativeRootSource).toBe('marketplace')
    expect(isReadOnlyRootSource('connector')).toBe(true)
    expect(isReadOnlyRootSource('shared')).toBe(false)
    expect(normalizeRootSource('')).toBe('unknown')
  })
})

describe('scan discovery filters', () => {
  const groups = groupDiscoveredSkills(
    report([
      agent('codex', 'Codex', [
        skill({ path: '/writable/demo', rootSource: 'adapter' }),
        skill({
          path: '/library/demo',
          slug: 'imported',
          contentHash: 'hash-lib',
          librarySkillId: 'cap-1',
          inSync: true,
        }),
      ]),
      agent('trae', 'TRAE', [
        skill({
          path: '/market/demo-market',
          slug: 'market-demo',
          contentHash: 'hash-market',
          rootSource: 'marketplace',
          agentKey: 'trae',
        }),
      ]),
    ]),
  )

  it('defaults to unimported rows and hides read-only sources', () => {
    const visible = filterDiscoveredGroups(groups, {
      status: 'discovered',
      showReadOnly: false,
    })
    expect(visible.map((group) => group.slug)).toEqual(['demo'])
  })

  it('supports the read-only filter and search across agents/paths', () => {
    const readOnly = filterDiscoveredGroups(groups, {
      status: 'readonly',
      showReadOnly: true,
    })
    expect(readOnly.map((group) => group.slug)).toEqual(['market-demo'])

    const searched = filterDiscoveredGroups(groups, {
      search: 'trae',
      status: 'all',
      showReadOnly: true,
    })
    expect(searched.map((group) => group.slug)).toEqual(['market-demo'])
  })
})

describe('scan import selection', () => {
  const groups = groupDiscoveredSkills(
    report([
      agent('codex', 'Codex', [
        skill({ path: '/writable/demo', rootSource: 'adapter', sizeBytes: 100 }),
        skill({
          path: '/library/demo',
          slug: 'imported',
          contentHash: 'hash-lib',
          librarySkillId: 'cap-1',
        }),
        skill({
          path: '/market/demo-market',
          slug: 'market-demo',
          contentHash: 'hash-market',
          rootSource: 'marketplace',
          agentKey: 'codex',
          sizeBytes: 200,
        }),
        skill({
          path: '/broken/demo',
          slug: 'broken',
          contentHash: 'hash-broken',
          valid: false,
        }),
      ]),
    ]),
  )

  it('selects only valid, unimported, writable rows by default', () => {
    const keys = selectableGroupKeys(groups)
    expect(keys).toHaveLength(1)
    expect(keys).toEqual([groupKey(groups, 'demo')])
  })

  it('summarises selected count, size, agents and paths', () => {
    const summary = selectionSummary(groups, [
      groupKey(groups, 'demo'),
      groupKey(groups, 'market-demo'),
    ])
    expect(summary.count).toBe(2)
    expect(summary.sizeBytes).toBe(300)
    expect(summary.agentKeys).toEqual(['codex'])
    expect(summary.agentNames).toEqual(['Codex'])
    expect(summary.paths).toHaveLength(2)
    expect(summary.readOnlyCount).toBe(1)
    expect(summary.requiresIncludeReadOnly).toBe(true)
  })

  it('builds the backend request with maxItems headroom and no fake limits', () => {
    const selected = [
      groupKey(groups, 'demo'),
      groupKey(groups, 'market-demo'),
    ]
    const request = buildScanImportRequest(groups, selected, {
      includeReadOnlySources: true,
    })
    expect(request.agentIds).toBeNull()
    expect(request.dryRun).toBe(false)
    expect(request.includeReadOnlySources).toBe(true)
    expect(request.maxItems).toBe(12)
    expect(request.paths).toHaveLength(2)
  })

  it('counts valid groups and default-importable groups', () => {
    expect(countValidGroups(groups)).toBe(3)
    expect(countDefaultImportableGroups(groups)).toBe(1)
    expect(pageDiscoveredGroups(groups, 2)).toHaveLength(2)
    expect(pageDiscoveredGroups(groups, 0)).toHaveLength(0)
  })
})
