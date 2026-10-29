/**
 * Hand-written mirror of `crates/agenthub-core/src/model.rs`.
 *
 * Serialization rules used by the Rust side:
 * - all structs are `#[serde(rename_all = "camelCase")]`;
 * - all enums are `#[serde(rename_all = "snake_case")]`;
 * - `ImportSource` is an internally tagged enum (`type`), and its variant
 *   fields are NOT camelCased by serde, so `AgentDir` carries `agent_id`.
 */

export type Scope = 'global' | 'project' | 'custom'
export type InstallStrategy = 'symlink' | 'copy'
export type CapabilityKind = 'skill' | 'mcp' | 'plugin'
export type AgentStatus =
  | 'installed'
  | 'not_installed'
  | 'unsupported'
  | 'verification_required'
  | 'error'
export type SymlinkSupport = 'supported' | 'unsupported' | 'unverified'
export type PluginSupport =
  | 'native_managed'
  | 'file_managed'
  | 'discovery_only'
  | 'manual_only'
  | 'unsupported'
  | 'unverified'
export type InstallationState = 'installed' | 'disabled' | 'missing' | 'conflict' | 'error'
export type OperationKind =
  | 'import'
  | 'install'
  | 'uninstall'
  | 'update'
  | 'sync'
  | 'mcp_import'
  | 'mcp_distribute'
  | 'mcp_remove'
  | 'profile_apply'
  | 'rollback'
  | 'backup'
  | 'scan'
export type OperationStatus =
  | 'planned'
  | 'running'
  | 'success'
  | 'partial'
  | 'failed'
  | 'rolled_back'
  | 'needs_manual_recovery'
  | 'recovered'
export type PlanAction = 'create' | 'replace' | 'noop' | 'conflict' | 'blocked' | 'remove'
export type McpTransport = 'stdio' | 'http' | 'sse' | 'streamable_http'
/** v3.1 approval lifecycle exposed by `OperationView.approvalState`. */
export type ApprovalState = 'none' | 'approved' | 'expired'
/** v3.1 recovery lifecycle exposed by `OperationView.recoveryState`. */
export type RecoveryState = 'recovered' | 'needs_manual_recovery'

// ---------------------------------------------------------------------------
// v3.1 unified IPC envelope (mirrors `agenthub_core::contract`)
// ---------------------------------------------------------------------------

export interface ApiErrorPayload {
  code: string
  message: string
  retryable: boolean
  details?: unknown
}

/** `{ok:true,data,requestId}` | `{ok:false,error,requestId}`. */
export type ApiResult<T> =
  | { ok: true; data: T; error?: never; requestId: string }
  | { ok: false; data?: never; error: ApiErrorPayload; requestId: string }


// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export interface SkillRoot {
  path: string
  scope: Scope
  source: string
  exists: boolean
  writable: boolean
  skillCount: number
}

export interface McpTarget {
  path: string
  format: string
  scope: Scope
  exists: boolean
  writable: boolean
  transports: McpTransport[]
  notes: string[]
}

export interface AgentView {
  id: string
  key: string
  name: string
  vendor: string
  adapterId: string
  status: AgentStatus
  appPath: string | null
  homePath: string | null
  /** User-added skill roots (project folders or custom locations). */
  customSkillRoots: string[]
  detectedAt: string | null
  lastScanAt: string | null
  skillRoots: SkillRoot[]
  mcpTargets: McpTarget[]
  pluginSupport: PluginSupport
  symlinkSupport: SymlinkSupport
  notes: string[]
  skillCount: number
  installedSkillCount: number
}

export interface AgentSummary {
  id: string
  key: string
  name: string
  vendor: string
  status: AgentStatus
  skillCount: number
  installedSkillCount: number
}

export interface AgentPathUpdate {
  agentId: string
  appPath: string | null
  homePath: string | null
  customSkillRoots: string[]
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

export interface SourceInfo {
  id: string
  sourceType: string
  uri: string
  reference: string | null
  importedAt: string
  lastCheckedAt: string | null
}

export interface CapabilityVersion {
  id: string
  capabilityId: string
  versionLabel: string
  contentHash: string
  storagePath: string
  sizeBytes: number
  fileCount: number
  notes: string | null
  createdAt: string
  isCurrent: boolean
}

export interface FileEntry {
  path: string
  sizeBytes: number
}

export interface InstallationView {
  id: string
  capabilityId: string
  versionId: string
  versionLabel: string
  agentId: string
  agentKey: string
  agentName: string
  scope: Scope
  strategy: InstallStrategy
  targetPath: string
  state: InstallationState
  contentHash: string | null
  installedAt: string
  verifiedAt: string | null
  lastCheckAt: string | null
  drift: boolean
  warnings: string[]
}

export interface Skill {
  id: string
  slug: string
  name: string
  description: string
  version: string
  contentHash: string
  tags: string[]
  license: string | null
  source: SourceInfo | null
  currentVersionId: string
  sizeBytes: number
  fileCount: number
  createdAt: string
  updatedAt: string
  installations: InstallationView[]
  updateAvailable: boolean
  driftCount: number
  conflict: string | null
}

export interface SkillDetail {
  skill: Skill
  versions: CapabilityVersion[]
  files: FileEntry[]
  skillMd: string
  manifest: unknown
}

/** Internally tagged with `type`. Variant fields keep Rust snake_case. */
export type ImportSource =
  | { type: 'folder'; path: string }
  | { type: 'archive'; path: string }
  | { type: 'skill_md'; path: string }
  | { type: 'git'; url: string; reference: string | null }
  | { type: 'agent_dir'; agent_id: string; path: string }

export interface ImportRequest {
  source: ImportSource
  skillPath: string | null
  versionLabel: string | null
  notes: string | null
}

export interface ImportResult {
  skill: Skill
  version: CapabilityVersion
  created: boolean
  warnings: string[]
}

export interface InstallRequest {
  capabilityId: string
  versionId: string | null
  agentIds: string[]
  strategy: InstallStrategy | null
  force: boolean
}

export interface UninstallRequest {
  installationIds: string[]
  force: boolean
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

export interface DiscoveredSkill {
  name: string
  slug: string
  description: string
  version: string | null
  path: string
  root: string
  /** Source classification of the root; empty for older scan payloads. */
  rootSource: string
  scope: Scope
  agentKey: string | null
  contentHash: string
  fileCount: number
  sizeBytes: number
  valid: boolean
  issues: string[]
  librarySkillId: string | null
  libraryVersionId: string | null
  inSync: boolean
  conflict: string | null
}

export interface RootScan {
  path: string
  scope: Scope
  exists: boolean
  discovered: number
  invalid: number
  errors: string[]
}

export interface AgentScan {
  agentId: string
  agentKey: string
  agentName: string
  status: AgentStatus
  roots: RootScan[]
  skills: DiscoveredSkill[]
  errors: string[]
}

export interface ScanReport {
  scannedAt: string
  agents: AgentScan[]
  totalSkills: number
  duplicates: number
  conflicts: number
  invalid: number
  warnings: string[]
}

export interface ScanImportRequest {
  agentIds: string[] | null
  paths: string[] | null
  includeReadOnlySources: boolean
  maxItems: number | null
  dryRun: boolean
}

export interface ScanImportedItem {
  slug: string
  name: string
  versionLabel: string
  agentKey: string | null
  path: string
  sizeBytes: number
}

export interface ScanImportFailure {
  path: string
  name: string
  error: string
}

export interface ScanImportResult {
  imported: ScanImportedItem[]
  importedCount: number
  skippedExisting: number
  skippedDuplicate: number
  skippedReadOnly: number
  skippedLimit: number
  failed: ScanImportFailure[]
  totalCandidates: number
  finishedAt: string
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

export interface McpDistribution {
  installationId: string | null
  agentId: string
  agentKey: string
  agentName: string
  targetPath: string
  configKey: string
  state: InstallationState
  contentHash: string | null
  installedAt: string | null
}

export interface McpServerView {
  id: string
  name: string
  description: string
  transport: McpTransport
  command: string | null
  args: string[]
  url: string | null
  headers: Record<string, string>
  envKeys: string[]
  credentialRefs: string[]
  createdAt: string
  updatedAt: string
  distributions: McpDistribution[]
  issues: string[]
}

export interface McpUpsertRequest {
  id: string | null
  name: string
  description: string | null
  transport: McpTransport
  command: string | null
  args: string[]
  url: string | null
  headers: Record<string, string>
  envKeys: string[]
  credentialRefs: string[]
}

export interface McpDistributeRequest {
  serverId: string
  agentIds: string[]
  force: boolean
}

export interface DiscoveredMcp {
  name: string
  agentId: string
  agentKey: string
  agentName: string
  targetPath: string
  format: string
  transport: McpTransport
  command: string | null
  args: string[]
  url: string | null
  headers: Record<string, string>
  envKeys: string[]
  raw: unknown
  issues: string[]
  alreadyKnown: boolean
}

export interface McpImportRequest {
  agentId: string
  targetPath: string
  name: string
}

export interface SecretState {
  key: string
  present: boolean
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

export interface PluginRecordView {
  id: string
  agentId: string
  agentKey: string
  agentName: string
  name: string
  version: string | null
  source: string | null
  installState: string
  managedBy: PluginSupport
  permissions: string[]
  officialUrl: string | null
  discoveredPath: string | null
  notes: string[]
  updatedAt: string
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

export interface ProfileItemView {
  capabilityId: string
  kind: CapabilityKind
  name: string
  description: string
  enabled: boolean
  targetAgentKeys: string[]
}

export interface ProfileView {
  id: string
  name: string
  description: string
  isBuiltin: boolean
  items: ProfileItemView[]
  createdAt: string
  updatedAt: string
}

export interface ProfileItemInput {
  capabilityId: string
  enabled: boolean
  targetAgentKeys: string[]
}

export interface ProfileUpsertRequest {
  id: string | null
  name: string
  description: string | null
  items: ProfileItemInput[]
}

// ---------------------------------------------------------------------------
// Operations / plans / snapshots
// ---------------------------------------------------------------------------

export interface PlanItem {
  id: string
  capabilityId: string | null
  capabilityName: string
  versionId: string | null
  versionLabel: string | null
  agentId: string
  agentKey: string
  agentName: string
  installKind: string
  action: PlanAction
  strategy: InstallStrategy
  targetPath: string
  sourcePath: string | null
  currentHash: string | null
  desiredHash: string | null
  configKey: string | null
  targetFormat: string | null
  payload: unknown
  warnings: string[]
  requiresRestart: boolean
  diff: string | null
  previousInstallation: unknown
}

export interface PlanSummary {
  create: number
  replace: number
  remove: number
  noop: number
  conflict: number
  blocked: number
}

export interface OperationPlan {
  operationId: string
  kind: OperationKind
  dryRun: boolean
  items: PlanItem[]
  warnings: string[]
  requiresRestartAgents: string[]
  requiresConfirmation: boolean
  summary: PlanSummary
  createdAt: string
  /** v3.1: SHA-256 of the canonical plan payload; approval is bound to it. */
  planHash: string
  /** v3.1: ISO timestamp after which the plan/approval must be re-issued. */
  expiresAt: string
  /** v3.1: `none` / `approved` / `expired`. */
  approvalState: string
}

export interface ItemResult {
  planItemId: string
  agentKey: string
  targetPath: string
  status: string
  message: string
  backupPath: string | null
  installationId: string | null
}

export interface OperationResult {
  operationId: string
  status: OperationStatus
  items: ItemResult[]
  diagnostics: string[]
  finishedAt: string | null
}

/** Durable journal entry for one target touched by an operation. */
export interface OperationStepView {
  id: string
  operationId: string
  sequenceNo: number
  targetPath: string
  beforeHash: string | null
  afterHash: string | null
  snapshotRef: string | null
  state: string
  errorCode: string | null
  startedAt: string
  finishedAt: string | null
}

export interface OperationView {
  id: string
  kind: OperationKind
  status: OperationStatus
  dryRun: boolean
  actor: string
  summary: string
  error: string | null
  rollbackOf: string | null
  startedAt: string
  finishedAt: string | null
  itemCount: number
  plan: OperationPlan | null
  result: OperationResult | null
  canRollback: boolean
  /** v3.1 approval surface. */
  planHash: string | null
  expiresAt: string | null
  approvalState: string
  approvedAt: string | null
  recoveryState: string | null
  steps: OperationStepView[]
}

/** One-time approval grant returned by `operations_approve`. */
export interface ApprovalGrant {
  operationId: string
  planHash: string
  /** Bearer token: keep in memory only, never log or persist in the UI. */
  approvalToken: string
  approvedAt: string
  expiresAt: string
}

/** Result of `operations_recover`. */
export interface RecoveryReport {
  operationId: string
  status: string
  items: ItemResult[]
  diagnostics: string[]
  finishedAt: string
}

/** `PRAGMA integrity_check` + `foreign_key_check` result. */
export interface IntegrityReport {
  ok: boolean
  schemaVersion: number
  details: string[]
}

/**
 * v3.1 per-agent feature evidence. `supportState` answers "is it possible",
 * `evidenceLevel` answers "how strongly do we know". They are independent:
 * a supported feature can still be unverified.
 */
export interface AdapterEvidenceView {
  id: string
  agentKey: string
  featureKey: string
  supportState: string
  evidenceLevel: string
  sourceUrl: string | null
  testedAt: string | null
  agentVersion: string | null
  adapterVersion: string
  notes: string | null
}

export interface AgentInspection {
  agentId: string
  agentKey: string
  capabilities: AdapterEvidenceView[]
  inspectedAt: string
}

export interface SnapshotView {
  id: string
  operationId: string
  kind: string
  originalPath: string
  backupPath: string
  existed: boolean
  hashBefore: string | null
  createdAt: string
}

// ---------------------------------------------------------------------------
// Dashboard / settings / doctor
// ---------------------------------------------------------------------------

export interface DashboardStats {
  agentsDetected: number
  agentsTotal: number
  skills: number
  mcpServers: number
  plugins: number
  installations: number
  pendingUpdates: number
  conflicts: number
  warnings: number
}

export interface Issue {
  severity: string
  kind: string
  title: string
  detail: string
  agentKey: string | null
  capabilityId: string | null
}

export interface Dashboard {
  stats: DashboardStats
  agents: AgentSummary[]
  recentOperations: OperationView[]
  issues: Issue[]
  libraryRoot: string
  initialized: boolean
  lastScanAt: string | null
  updates: Skill[]
}

export interface AppSettings {
  libraryRoot: string
  defaultStrategy: InstallStrategy
  language: string
  theme: string
  scanOnStartup: boolean
  gitImportEnabled: boolean
  keychainAvailable: boolean
  databasePath: string
  appVersion: string
  initialized: boolean
}

export interface SettingsUpdate {
  libraryRoot?: string | null
  defaultStrategy?: InstallStrategy | null
  language?: string | null
  theme?: string | null
  scanOnStartup?: boolean | null
}

export interface DoctorCheck {
  name: string
  status: string
  detail: string
  hint: string | null
}

export interface AgentDoctorCheck {
  key: string
  name: string
  status: AgentStatus
  installEvidence: string[]
  skillRoots: string[]
  mcpTargets: string[]
  notes: string[]
}

export interface DoctorReport {
  ok: boolean
  checks: DoctorCheck[]
  version: string
  home: string
  libraryRoot: string
  databasePath: string
  agents: AgentDoctorCheck[]
}

export interface ChangeCheck {
  checkedAt: string
  drifted: InstallationView[]
  missing: InstallationView[]
  updates: Skill[]
  warnings: string[]
}

export interface BackupResult {
  id: string
  path: string
  createdAt: string
  entries: number
}

export interface BackupView {
  id: string
  path: string
  createdAt: string
  sizeBytes: number
}

export interface RestoreResult {
  backupId: string
  restoredAt: string
  safetyBackupId: string
  schemaVersion: number
  integrityOk: boolean
  capabilityCount: number
}

export interface Bootstrap {
  settings: AppSettings
  agents: AgentView[]
  platform: string
  coreVersion: string
  projectRoots: string[]
}

// ---------------------------------------------------------------------------
// Capability health, audits, dependency diagnostics, validation evidence
// ---------------------------------------------------------------------------

export type HealthState = 'verified' | 'unverified' | 'needs_attention' | 'failed' | 'not_deployed'
export type AuditStatus = 'pass' | 'warn' | 'fail' | 'unknown'
export type AuditRisk = 'low' | 'medium' | 'high' | 'critical'
export type FindingSeverity = 'info' | 'low' | 'medium' | 'high' | 'critical'
export type FindingCategory = 'spec' | 'security' | 'dependency' | 'quality' | 'compatibility'
export type DependencyStatus = 'ok' | 'missing' | 'warning' | 'unknown'
export type DependencyKind =
  | 'runtime'
  | 'binary'
  | 'manifest'
  | 'mcp'
  | 'credential'
  | 'package'
export type FileChangeKind = 'added' | 'removed' | 'modified'
export type ValidationStatus = 'passed' | 'failed' | 'needs_attention' | 'unverified'
export type ValidationKind = 'manual' | 'static' | 'environment' | 'regression'
export type AuditKind = 'static' | 'environment'

export interface AuditFinding {
  id: string
  severity: FindingSeverity
  category: FindingCategory
  title: string
  detail: string
  path: string | null
  suggestion: string | null
}

export interface CapabilityAudit {
  id: string
  capabilityId: string
  versionId: string
  versionLabel: string
  kind: AuditKind | string
  status: AuditStatus
  risk: AuditRisk
  summary: string
  findings: AuditFinding[]
  createdAt: string
}

export interface DependencyItem {
  name: string
  kind: DependencyKind
  requiredVersion: string | null
  found: string | null
  status: DependencyStatus
  detail: string
  sourcePath: string | null
  suggestion: string | null
}

export interface DependencyReport {
  capabilityId: string
  versionId: string
  status: 'healthy' | 'needs_attention' | 'unknown'
  risk: AuditRisk
  summary: string
  items: DependencyItem[]
  checkedAt: string
}

export interface FileChange {
  path: string
  change: FileChangeKind
  sizeBefore: number
  sizeAfter: number
}

export interface UpdateImpact {
  id: string
  capabilityId: string
  fromVersionId: string
  fromVersionLabel: string
  toVersionId: string
  toVersionLabel: string
  risk: AuditRisk
  summary: string
  fileChanges: FileChange[]
  /** e.g. docs_only, new_script, removed_script, new_network, new_dependency */
  classes: string[]
  findings: AuditFinding[]
  affectedInstallationCount: number
  affectedAgentKeys: string[]
  affectedProjectRoots: string[]
  requiresRevalidation: boolean
  createdAt: string
}

export interface ValidationRun {
  id: string
  capabilityId: string
  versionId: string
  versionLabel: string
  agentId: string | null
  agentKey: string | null
  agentName: string | null
  kind: ValidationKind | string
  status: ValidationStatus
  caseName: string
  notes: string
  evidence: string
  actor: string
  createdAt: string
}

export interface ValidationRequest {
  capabilityId: string
  versionId: string | null
  agentId: string | null
  kind: string | null
  status: ValidationStatus
  caseName: string
  notes: string | null
  evidence: string | null
}

export interface CapabilityHealth {
  capabilityId: string
  slug: string
  name: string
  description: string
  versionLabel: string
  status: HealthState
  reasons: string[]
  audit: CapabilityAudit | null
  environment: DependencyReport | null
  validations: ValidationRun[]
  installationCount: number
  agentKeys: string[]
  verifiedAgentKeys: string[]
  lastCheckedAt: string | null
}

export interface HealthSummary {
  total: number
  verified: number
  unverified: number
  needsAttention: number
  failed: number
  notDeployed: number
  lastCheckedAt: string | null
}

export interface HealthOverview {
  summary: HealthSummary
  capabilities: CapabilityHealth[]
}

export interface AgentHealth {
  agentId: string
  agentKey: string
  agentName: string
  status: AgentStatus
  version: string | null
  previousVersion: string | null
  versionChanged: boolean
  versionChangedAt: string | null
  pathsOk: boolean
  notes: string[]
  checkedAt: string
}

export interface AgentHealthReport {
  checkedAt: string
  agents: AgentHealth[]
}

export interface AuditRequest {
  capabilityId: string
  versionId: string | null
  kind: string | null
}

export interface ImpactRequest {
  capabilityId: string
  fromVersionId: string | null
  toVersionId: string
}
