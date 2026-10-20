import type {
  DiscoveredSkill,
  ScanImportRequest,
  ScanReport,
} from './types'

/**
 * Root sources whose contents are caches owned by the agent itself. The backend
 * skips these for scan-import unless `includeReadOnlySources` is true.
 */
export const READ_ONLY_ROOT_SOURCES = [
  'builtin',
  'marketplace',
  'workspace',
  'connector',
] as const

export function normalizeRootSource(source: string | null | undefined): string {
  const value = (source ?? '').trim()
  return value.length > 0 ? value : 'unknown'
}

export function isReadOnlyRootSource(source: string | null | undefined): boolean {
  if (!source) return false
  return (READ_ONLY_ROOT_SOURCES as readonly string[]).includes(source)
}

export interface DiscoveredAgent {
  agentId: string
  agentKey: string
  agentName: string
  path: string
}

export interface DiscoveredEntry {
  skill: DiscoveredSkill
  agentId: string
  agentKey: string
  agentName: string
}

export interface DiscoveredGroup {
  /** Stable identity: slug + content hash (falls back to path for empty hashes). */
  key: string
  slug: string
  name: string
  description: string
  version: string | null
  contentHash: string
  sizeBytes: number
  fileCount: number
  agents: DiscoveredAgent[]
  agentKeys: string[]
  paths: string[]
  rootSources: string[]
  entries: DiscoveredEntry[]
  representativePath: string
  representativeRootSource: string
  representativeAgent: DiscoveredAgent | null
  hasWritablePath: boolean
  /** Every path of this group comes from a read-only root. */
  readOnlyOnly: boolean
  inLibrary: boolean
  /** At least one discovered copy is valid. */
  valid: boolean
  invalidCount: number
  issues: string[]
  conflict: string | null
}

export interface DiscoveredFilterOptions {
  search?: string
  status?: DiscoveredStatusFilter
  showReadOnly?: boolean
}

export type DiscoveredStatusFilter = 'all' | 'discovered' | 'library' | 'readonly'

export interface DiscoveredSelectionSummary {
  count: number
  sizeBytes: number
  agentKeys: string[]
  agentNames: string[]
  /** Representative path per selected group (what gets sent to the command). */
  paths: string[]
  /** Selected groups that can only be imported with includeReadOnlySources. */
  readOnlyCount: number
  requiresIncludeReadOnly: boolean
}

/** Flatten a scan report into one entry per agent/discovered skill. */
export function collectDiscoveredEntries(
  report: ScanReport | null | undefined,
): DiscoveredEntry[] {
  if (!report) return []
  const entries: DiscoveredEntry[] = []
  for (const agent of report.agents) {
    for (const skill of agent.skills) {
      entries.push({
        skill,
        agentId: agent.agentId,
        agentKey: skill.agentKey ?? agent.agentKey,
        agentName: agent.agentName,
      })
    }
  }
  return entries
}

function groupKey(skill: DiscoveredSkill): string {
  return `${skill.slug}\u0000${skill.contentHash || skill.path}`
}

function uniquePush<T>(list: T[], value: T): void {
  if (!list.includes(value)) list.push(value)
}

function sourceOf(entry: DiscoveredEntry): string {
  return normalizeRootSource(entry.skill.rootSource)
}

/**
 * Group a scan report by (slug, contentHash), merging copies discovered across
 * different agents so one batch-import row represents one library version.
 * Groups are sorted with unimported skills first, then by name.
 */
export function groupDiscoveredSkills(
  report: ScanReport | null | undefined,
): DiscoveredGroup[] {
  const byKey = new Map<string, DiscoveredEntry[]>()
  for (const entry of collectDiscoveredEntries(report)) {
    const key = groupKey(entry.skill)
    const bucket = byKey.get(key)
    if (bucket) bucket.push(entry)
    else byKey.set(key, [entry])
  }

  const groups: DiscoveredGroup[] = []
  for (const [key, entries] of byKey) {
    const representative =
      entries.find(
        (entry) => entry.skill.valid && !isReadOnlyRootSource(sourceOf(entry)),
      ) ??
      entries.find((entry) => entry.skill.valid) ??
      entries.find((entry) => !isReadOnlyRootSource(sourceOf(entry))) ??
      entries[0]

    const agents: DiscoveredAgent[] = []
    const agentKeys: string[] = []
    const paths: string[] = []
    const rootSources: string[] = []
    const issues: string[] = []
    let sizeBytes = 0
    let fileCount = 0
    let inLibrary = false
    let valid = false
    let invalidCount = 0
    let conflict: string | null = null
    let name = ''
    let description = ''
    let version: string | null = null

    for (const entry of entries) {
      const skill = entry.skill
      if (!agents.some((agent) => agent.agentId === entry.agentId)) {
        agents.push({
          agentId: entry.agentId,
          agentKey: entry.agentKey,
          agentName: entry.agentName,
          path: skill.path,
        })
      }
      uniquePush(agentKeys, entry.agentKey)
      uniquePush(paths, skill.path)
      uniquePush(rootSources, sourceOf(entry))
      for (const issue of skill.issues) uniquePush(issues, issue)
      sizeBytes = Math.max(sizeBytes, skill.sizeBytes)
      fileCount = Math.max(fileCount, skill.fileCount)
      if (skill.librarySkillId !== null || skill.inSync) inLibrary = true
      if (skill.valid) valid = true
      else invalidCount += 1
      if (!conflict && skill.conflict) conflict = skill.conflict
      if (!name && skill.name) name = skill.name
      if (skill.description && skill.description.length > description.length) {
        description = skill.description
      }
      if (!version && skill.version) version = skill.version
    }

    const representativeSource = sourceOf(representative)
    groups.push({
      key,
      slug: representative.skill.slug,
      name: name || representative.skill.name || representative.skill.slug,
      description,
      version,
      contentHash: representative.skill.contentHash,
      sizeBytes,
      fileCount,
      agents,
      agentKeys,
      paths,
      rootSources,
      entries,
      representativePath: representative.skill.path,
      representativeRootSource: representativeSource,
      representativeAgent: agents.find((agent) => agent.agentId === representative.agentId) ?? null,
      hasWritablePath: entries.some((entry) => !isReadOnlyRootSource(sourceOf(entry))),
      readOnlyOnly: entries.every((entry) => isReadOnlyRootSource(sourceOf(entry))),
      inLibrary,
      valid,
      invalidCount,
      issues,
      conflict,
    })
  }

  groups.sort((a, b) => {
    if (a.inLibrary !== b.inLibrary) return a.inLibrary ? 1 : -1
    return a.name.localeCompare(b.name) || a.slug.localeCompare(b.slug)
  })
  return groups
}

/** M in the "扫描发现 (M)" tab: valid skills deduped by slug + hash. */
export function countValidGroups(groups: DiscoveredGroup[]): number {
  return groups.filter((group) => group.valid).length
}

/** Whether a group can be imported as part of a default batch. */
export function isImportableGroup(
  group: DiscoveredGroup,
  includeReadOnlySources = false,
): boolean {
  if (!group.valid || group.inLibrary) return false
  return group.entries.some(
    (entry) =>
      entry.skill.valid &&
      (includeReadOnlySources || !isReadOnlyRootSource(sourceOf(entry))),
  )
}

/** Count used by the Dashboard card: unimported, non-read-only groups. */
export function countDefaultImportableGroups(groups: DiscoveredGroup[]): number {
  return groups.filter((group) => isImportableGroup(group, false)).length
}

/** Groups are selectable for a future import; invalid / already-imported ones are not. */
export function isSelectableGroup(group: DiscoveredGroup): boolean {
  return group.valid && !group.inLibrary
}

export function filterDiscoveredGroups(
  groups: DiscoveredGroup[],
  options: DiscoveredFilterOptions = {},
): DiscoveredGroup[] {
  const query = (options.search ?? '').trim().toLowerCase()
  const status = options.status ?? 'all'
  const showReadOnly = options.showReadOnly ?? false

  return groups.filter((group) => {
    if (!showReadOnly && group.readOnlyOnly) return false
    if (status === 'readonly' && !group.readOnlyOnly) return false
    if (status === 'discovered' && group.inLibrary) return false
    if (status === 'library' && !group.inLibrary) return false

    if (!query) return true
    const haystack = [
      group.name,
      group.slug,
      group.description,
      group.contentHash,
      ...group.agentKeys,
      ...group.agents.map((agent) => agent.agentName),
      ...group.paths,
      ...group.rootSources,
    ]
      .join(' ')
      .toLowerCase()
    return haystack.includes(query)
  })
}

export function selectableGroupKeys(
  groups: DiscoveredGroup[],
  options: { includeReadOnly?: boolean } = {},
): string[] {
  return groups
    .filter(
      (group) =>
        isSelectableGroup(group) &&
        (options.includeReadOnly ||
          isImportableGroup(group, false)),
    )
    .map((group) => group.key)
}

export function selectionSummary(
  groups: DiscoveredGroup[],
  selectedKeys: Iterable<string>,
): DiscoveredSelectionSummary {
  const wanted = new Set(selectedKeys)
  const selected = groups.filter((group) => wanted.has(group.key))
  const paths: string[] = []
  const agentKeys: string[] = []
  const agentNames: string[] = []
  let sizeBytes = 0
  let readOnlyCount = 0

  for (const group of selected) {
    uniquePush(paths, group.representativePath)
    for (const key of group.agentKeys) uniquePush(agentKeys, key)
    for (const agent of group.agents) uniquePush(agentNames, agent.agentName)
    sizeBytes += group.sizeBytes
    if (!isImportableGroup(group, false)) readOnlyCount += 1
  }

  return {
    count: selected.length,
    sizeBytes,
    agentKeys,
    agentNames,
    paths,
    readOnlyCount,
    requiresIncludeReadOnly: readOnlyCount > 0,
  }
}

/**
 * Build the backend request for the current selection. `paths` uses one
 * representative directory per group; the backend dedupes by slug + hash.
 */
export function buildScanImportRequest(
  groups: DiscoveredGroup[],
  selectedKeys: Iterable<string>,
  options: { includeReadOnlySources: boolean },
): ScanImportRequest {
  const summary = selectionSummary(groups, selectedKeys)
  return {
    agentIds: null,
    paths: summary.paths,
    includeReadOnlySources: options.includeReadOnlySources,
    maxItems: summary.count + 10,
    dryRun: false,
  }
}

/** Slice used by the "load more" pagination (selection is kept outside it). */
export function pageDiscoveredGroups(
  groups: DiscoveredGroup[],
  limit: number,
): DiscoveredGroup[] {
  if (!Number.isFinite(limit) || limit <= 0) return []
  return groups.slice(0, Math.floor(limit))
}
