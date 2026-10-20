import { invoke, isTauri } from '@tauri-apps/api/core'

import { ApiEnvelopeError, ApiEnvelopeFormatError, unwrapApiResult } from './envelope'
import type {
  AgentHealthReport,
  AgentInspection,
  AgentPathUpdate,
  AgentView,
  AppSettings,
  ApprovalGrant,
  AuditRequest,
  BackupResult,
  BackupView,
  Bootstrap,
  CapabilityAudit,
  CapabilityHealth,
  ChangeCheck,
  Dashboard,
  DependencyReport,
  DiscoveredMcp,
  DoctorReport,
  HealthOverview,
  ImpactRequest,
  ImportRequest,
  ImportResult,
  InstallRequest,
  IntegrityReport,
  McpDistributeRequest,
  McpImportRequest,
  McpServerView,
  McpUpsertRequest,
  OperationPlan,
  OperationResult,
  OperationView,
  PluginRecordView,
  ProfileUpsertRequest,
  ProfileView,
  RecoveryReport,
  ScanImportRequest,
  ScanImportResult,
  ScanReport,
  SecretState,
  SettingsUpdate,
  Skill,
  SkillDetail,
  SnapshotView,
  UninstallRequest,
  UpdateImpact,
  ValidationRequest,
  ValidationRun,
} from './types'

/** Extra structured metadata carried by a failed envelope. */
export interface ApiErrorMeta {
  retryable?: boolean
  requestId?: string | null
  details?: unknown
}

/** Error raised by the Tauri bridge, normalized to a readable message. */
export class ApiError extends Error {
  readonly code: string | null
  readonly raw: unknown
  /** `true` when the backend marked the same request as safe to retry. */
  readonly retryable: boolean
  /** Server-side trace id from the unified ApiResult envelope. */
  readonly requestId: string | null
  readonly details: unknown

  constructor(
    message: string,
    code: string | null = null,
    raw: unknown = null,
    meta: ApiErrorMeta = {},
  ) {
    super(message)
    this.name = 'ApiError'
    this.code = code
    this.raw = raw
    this.retryable = meta.retryable ?? false
    this.requestId = meta.requestId ?? null
    this.details = meta.details
  }
}

export const NOT_TAURI_MESSAGE = 'AgentHub 的桌面后端不可用：请运行 pnpm tauri dev 打开桌面应用。'

function describe(value: unknown): string | null {
  if (typeof value === 'string') return value.length > 0 ? value : null
  if (value instanceof Error) return value.message || null
  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>
    for (const key of ['message', 'error', 'detail']) {
      const message = record[key]
      if (typeof message === 'string' && message.length > 0) return message
    }
    try {
      return JSON.stringify(value)
    } catch {
      return null
    }
  }
  if (value === null || value === undefined) return null
  return String(value)
}

/** Turn anything thrown by invoke (usually a Rust `String`) into a readable message. */
export function normalizeError(error: unknown, fallback = '未知错误'): string {
  const message = describe(error)
  if (message) return message
  return fallback
}

export function toApiError(error: unknown, fallback?: string): ApiError {
  if (error instanceof ApiError) return error
  if (error instanceof ApiEnvelopeError) {
    return new ApiError(error.message, error.code, error, {
      retryable: error.retryable,
      requestId: error.requestId,
      details: error.details,
    })
  }
  if (error instanceof ApiEnvelopeFormatError) {
    return new ApiError(error.message, 'INVALID_RESPONSE', error)
  }
  const raw = error && typeof error === 'object' ? (error as Record<string, unknown>) : null
  const code = raw && typeof raw.code === 'string' ? raw.code : null
  return new ApiError(normalizeError(error, fallback), code, error)
}

export function isDesktop(): boolean {
  try {
    return isTauri()
  } catch {
    return false
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop()) {
    throw new ApiError(NOT_TAURI_MESSAGE, 'not_tauri')
  }
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw toApiError(error)
  }
}

/**
 * Call a v3.1 command that returns the unified `ApiResult<T>` envelope.
 * Success unwraps `data`; failure throws a structured `ApiError` whose `code`,
 * `retryable`, `requestId` and `details` come from the backend error payload.
 */
async function callApi<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop()) {
    throw new ApiError(NOT_TAURI_MESSAGE, 'not_tauri')
  }
  try {
    const raw = await invoke<unknown>(command, args)
    return unwrapApiResult<T>(raw)
  } catch (error) {
    throw toApiError(error)
  }
}

/** Every Tauri command exposed by src-tauri/src/lib.rs, in one typed surface. */
export const api = {
  // bootstrap / settings
  bootstrap: () => call<Bootstrap>('app_bootstrap'),
  dashboard: () => call<Dashboard>('app_dashboard'),
  settings: () => call<AppSettings>('app_settings'),
  updateSettings: (update: SettingsUpdate) => call<AppSettings>('app_update_settings', { update }),
  markInitialized: () => call<AppSettings>('app_mark_initialized'),
  doctor: () => call<DoctorReport>('app_doctor'),
  projectRoots: () => call<string[]>('app_project_roots'),
  setProjectRoots: (roots: string[]) => call<string[]>('app_set_project_roots', { roots }),

  // agents / scan
  agentsList: () => call<AgentView[]>('agents_list'),
  updateAgentPaths: (update: AgentPathUpdate) => call<AgentView>('agents_update_paths', { update }),
  scan: () => call<ScanReport>('app_scan'),
  checkChanges: () => call<ChangeCheck>('app_check_changes'),

  // skills
  skillsList: () => call<Skill[]>('skills_list'),
  skillGet: (id: string) => call<SkillDetail>('skills_get', { id }),
  importSkill: (request: ImportRequest) => call<ImportResult>('skills_import', { request }),
  skillsImportScan: (request: ScanImportRequest) =>
    call<ScanImportResult>('skills_import_scan', { request }),
  planInstall: (request: InstallRequest) => call<OperationPlan>('skills_plan_install', { request }),
  planUninstall: (request: UninstallRequest) =>
    call<OperationPlan>('skills_plan_uninstall', { request }),

  // capability health / audits / validations
  skillsAudit: (request: AuditRequest) =>
    call<CapabilityAudit>('skills_audit', { request }),
  skillsDependencies: (request: AuditRequest) =>
    call<DependencyReport>('skills_dependencies', { request }),
  skillsHealth: (capabilityId: string) =>
    call<CapabilityHealth>('skills_health', { capability_id: capabilityId }),
  healthOverview: () => call<HealthOverview>('health_overview'),
  skillsPreviewUpdate: (request: ImpactRequest) =>
    call<UpdateImpact>('skills_preview_update', { request }),
  validationsList: (capabilityId: string, limit?: number) =>
    call<ValidationRun[]>('validations_list', {
      capability_id: capabilityId,
      limit: limit ?? null,
    }),
  validationsRecord: (request: ValidationRequest) =>
    call<ValidationRun>('validations_record', { request }),
  agentHealth: (record: boolean) => call<AgentHealthReport>('agent_health', { record }),

  // operations
  applyOperation: (operationId: string, force: boolean) =>
    call<OperationResult>('operations_apply', { operation_id: operationId, force }),
  rollbackOperation: (operationId: string) =>
    call<OperationResult>('operations_rollback', { operation_id: operationId }),
  operationsList: (limit?: number) =>
    call<OperationView[]>('operations_list', { limit: limit ?? null }),
  operationGet: (operationId: string) =>
    call<OperationView>('operations_get', { operation_id: operationId }),

  // v3.1 approval / recovery / evidence (unified ApiResult envelope)
  operationsPreview: (operationId: string) =>
    callApi<OperationView>('operations_preview', { operation_id: operationId }),
  operationsApprove: (operationId: string, planHash: string) =>
    callApi<ApprovalGrant>('operations_approve', {
      operation_id: operationId,
      plan_hash: planHash,
    }),
  /** The approval token is a bearer credential: pass it straight through. */
  operationsApplyApproved: (
    operationId: string,
    approvalToken: string,
    idempotencyKey?: string,
  ) =>
    callApi<OperationResult>('operations_apply_approved', {
      operation_id: operationId,
      approval_token: approvalToken,
      idempotency_key: idempotencyKey ?? null,
    }),
  operationsRecover: (operationId: string) =>
    callApi<RecoveryReport>('operations_recover', { operation_id: operationId }),
  agentsInspect: (agentId: string) =>
    callApi<AgentInspection>('agents_inspect', { agent_id: agentId }),
  appIntegrity: () => callApi<IntegrityReport>('app_integrity'),
  snapshotsList: (operationId?: string, limit?: number) =>
    call<SnapshotView[]>('snapshots_list', {
      operation_id: operationId ?? null,
      limit: limit ?? null,
    }),
  backupsCreate: () => call<BackupResult>('backups_create'),
  backupsList: () => call<BackupView[]>('backups_list'),

  // MCP
  mcpList: () => call<McpServerView[]>('mcp_list'),
  mcpDiscover: (agentIds?: string[]) =>
    call<DiscoveredMcp[]>('mcp_discover', { agent_ids: agentIds ?? null }),
  mcpUpsert: (request: McpUpsertRequest) => call<McpServerView>('mcp_upsert', { request }),
  mcpImport: (request: McpImportRequest) => call<McpServerView>('mcp_import', { request }),
  mcpPlanDistribute: (request: McpDistributeRequest) =>
    call<OperationPlan>('mcp_plan_distribute', { request }),
  mcpPlanRemove: (serverId: string, agentIds: string[]) =>
    call<OperationPlan>('mcp_plan_remove', { server_id: serverId, agent_ids: agentIds }),
  mcpSecretStates: (serverId: string) =>
    call<SecretState[]>('mcp_secret_states', { server_id: serverId }),
  mcpSetSecret: (serverId: string, key: string, value: string) =>
    call<void>('mcp_set_secret', { server_id: serverId, key, value }),
  mcpDeleteSecret: (serverId: string, key: string) =>
    call<void>('mcp_delete_secret', { server_id: serverId, key }),

  // plugins / profiles
  pluginsSync: () => call<PluginRecordView[]>('plugins_sync'),
  profilesList: () => call<ProfileView[]>('profiles_list'),
  profilesUpsert: (request: ProfileUpsertRequest) =>
    call<ProfileView>('profiles_upsert', { request }),
  profilesDelete: (id: string) => call<void>('profiles_delete', { id }),
  profilesPlanApply: (profileId: string) =>
    call<OperationPlan>('profiles_plan_apply', { profile_id: profileId }),
}

export type Api = typeof api
