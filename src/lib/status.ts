import type {
  AgentStatus,
  AgentView,
  CapabilityKind,
  InstallationState,
  InstallStrategy,
  Issue,
  McpTransport,
  OperationKind,
  OperationPlan,
  OperationStatus,
  PlanAction,
  PlanItem,
  PlanSummary,
  PluginSupport,
  Scope,
  Skill,
  SymlinkSupport,
} from './types'

export type Tone = 'neutral' | 'success' | 'warn' | 'danger' | 'info' | 'accent'

export const agentStatusKey: Record<AgentStatus, string> = {
  installed: 'status.agent.installed',
  not_installed: 'status.agent.not_installed',
  unsupported: 'status.agent.unsupported',
  verification_required: 'status.agent.verification_required',
  error: 'status.agent.error',
}

export const installationStateKey: Record<InstallationState, string> = {
  installed: 'status.installation.installed',
  disabled: 'status.installation.disabled',
  missing: 'status.installation.missing',
  conflict: 'status.installation.conflict',
  error: 'status.installation.error',
}

export const planActionKey: Record<PlanAction, string> = {
  create: 'status.plan.create',
  replace: 'status.plan.replace',
  remove: 'status.plan.remove',
  noop: 'status.plan.noop',
  conflict: 'status.plan.conflict',
  blocked: 'status.plan.blocked',
}

export const operationStatusKey: Record<OperationStatus, string> = {
  planned: 'status.operation.planned',
  running: 'status.operation.running',
  success: 'status.operation.success',
  partial: 'status.operation.partial',
  failed: 'status.operation.failed',
  rolled_back: 'status.operation.rolled_back',
}

export const operationKindKey: Record<OperationKind, string> = {
  import: 'status.operationKind.import',
  install: 'status.operationKind.install',
  uninstall: 'status.operationKind.uninstall',
  update: 'status.operationKind.update',
  sync: 'status.operationKind.sync',
  mcp_import: 'status.operationKind.mcp_import',
  mcp_distribute: 'status.operationKind.mcp_distribute',
  mcp_remove: 'status.operationKind.mcp_remove',
  profile_apply: 'status.operationKind.profile_apply',
  rollback: 'status.operationKind.rollback',
  backup: 'status.operationKind.backup',
  scan: 'status.operationKind.scan',
}

export const pluginSupportKey: Record<PluginSupport, string> = {
  native_managed: 'status.pluginSupport.native_managed',
  file_managed: 'status.pluginSupport.file_managed',
  discovery_only: 'status.pluginSupport.discovery_only',
  manual_only: 'status.pluginSupport.manual_only',
  unsupported: 'status.pluginSupport.unsupported',
  unverified: 'status.pluginSupport.unverified',
}

export const symlinkSupportKey: Record<SymlinkSupport, string> = {
  supported: 'status.symlink.supported',
  unsupported: 'status.symlink.unsupported',
  unverified: 'status.symlink.unverified',
}

export const transportKey: Record<McpTransport, string> = {
  stdio: 'status.transport.stdio',
  http: 'status.transport.http',
  sse: 'status.transport.sse',
  streamable_http: 'status.transport.streamable_http',
}

export const scopeKey: Record<Scope, string> = {
  global: 'status.scope.global',
  project: 'status.scope.project',
  custom: 'status.scope.custom',
}

export const strategyKey: Record<InstallStrategy, string> = {
  copy: 'status.strategy.copy',
  symlink: 'status.strategy.symlink',
}

export const capabilityKindKey: Record<CapabilityKind, string> = {
  skill: 'status.kind.skill',
  mcp: 'status.kind.mcp',
  plugin: 'status.kind.plugin',
}

export const severityKey: Record<string, string> = {
  info: 'status.severity.info',
  warning: 'status.severity.warning',
  error: 'status.severity.error',
}

const agentTone: Record<string, Tone> = {
  installed: 'success',
  verification_required: 'warn',
  not_installed: 'neutral',
  unsupported: 'neutral',
  error: 'danger',
}

const installationTone: Record<string, Tone> = {
  installed: 'success',
  disabled: 'neutral',
  missing: 'danger',
  conflict: 'warn',
  error: 'danger',
}

const planActionTone: Record<string, Tone> = {
  create: 'success',
  replace: 'info',
  remove: 'warn',
  noop: 'neutral',
  conflict: 'warn',
  blocked: 'danger',
}

const operationStatusTone: Record<string, Tone> = {
  planned: 'info',
  running: 'accent',
  success: 'success',
  partial: 'warn',
  failed: 'danger',
  rolled_back: 'neutral',
}

const pluginSupportTone: Record<string, Tone> = {
  native_managed: 'success',
  file_managed: 'info',
  discovery_only: 'info',
  manual_only: 'warn',
  unsupported: 'neutral',
  unverified: 'warn',
}

const symlinkTone: Record<string, Tone> = {
  supported: 'success',
  unsupported: 'neutral',
  unverified: 'warn',
}

const severityTone: Record<string, Tone> = {
  info: 'info',
  warning: 'warn',
  error: 'danger',
}

function toneFrom(map: Record<string, Tone>, value: string): Tone {
  return map[value] ?? 'neutral'
}

export const toneForAgentStatus = (value: AgentStatus): Tone => toneFrom(agentTone, value)
export const toneForInstallationState = (value: InstallationState): Tone =>
  toneFrom(installationTone, value)
export const toneForPlanAction = (value: PlanAction): Tone => toneFrom(planActionTone, value)
export const toneForOperationStatus = (value: OperationStatus): Tone =>
  toneFrom(operationStatusTone, value)
export const toneForPluginSupport = (value: PluginSupport): Tone =>
  toneFrom(pluginSupportTone, value)
export const toneForSymlinkSupport = (value: SymlinkSupport): Tone => toneFrom(symlinkTone, value)
export const toneForSeverity = (value: string): Tone => toneFrom(severityTone, value)

export function isAgentTargetable(agent: AgentView): boolean {
  return agent.status === 'installed' || agent.status === 'verification_required'
}

export function issueTone(issue: Issue): Tone {
  return toneForSeverity(issue.severity)
}

// ---------------------------------------------------------------------------
// Capability matrix
// ---------------------------------------------------------------------------

export type MatrixState =
  | 'installed'
  | 'disabled'
  | 'missing'
  | 'conflict'
  | 'drift'
  | 'outdated'
  | 'unsupported'
  | 'verification'
  | 'not_installed'
  | 'error'

export interface MatrixCell {
  state: MatrixState
  tone: Tone
  labelKey: string
  installationId: string | null
  path: string | null
  selectableForInstall: boolean
  selectableForUninstall: boolean
}

const matrixTone: Record<MatrixState, Tone> = {
  installed: 'success',
  disabled: 'neutral',
  missing: 'danger',
  conflict: 'warn',
  drift: 'warn',
  outdated: 'info',
  unsupported: 'neutral',
  verification: 'warn',
  not_installed: 'neutral',
  error: 'danger',
}

const matrixLabelKey: Record<MatrixState, string> = {
  installed: 'status.matrix.installed',
  disabled: 'status.matrix.disabled',
  missing: 'status.matrix.missing',
  conflict: 'status.matrix.conflict',
  drift: 'status.matrix.drift',
  outdated: 'status.matrix.outdated',
  unsupported: 'status.matrix.unsupported',
  verification: 'status.matrix.verification',
  not_installed: 'status.matrix.not_installed',
  error: 'status.matrix.error',
}

/**
 * Resolve the matrix state for one skill/agent pair.
 * Precedence: unsupported agent → skill conflict → installation state → drift → update.
 */
export function matrixCellState(skill: Skill, agent: AgentView): MatrixCell {
  const installation = skill.installations.find((item) => item.agentId === agent.id) ?? null
  const targetable = isAgentTargetable(agent)

  const build = (state: MatrixState): MatrixCell => ({
    state,
    tone: matrixTone[state],
    labelKey: matrixLabelKey[state],
    installationId: installation?.id ?? null,
    path: installation?.targetPath ?? null,
    selectableForInstall: targetable,
    selectableForUninstall: installation !== null,
  })

  if (!targetable) return build('unsupported')

  if (skill.conflict) return build('conflict')

  if (installation) {
    if (installation.state === 'missing') return build('missing')
    if (installation.state === 'conflict') return build('conflict')
    if (installation.state === 'error') return build('error')
    if (installation.drift) return build('drift')
    if (installation.state === 'disabled') return build('disabled')
    if (skill.updateAvailable) return build('outdated')
    return build('installed')
  }

  if (agent.status === 'verification_required') return build('verification')
  return build('not_installed')
}

// ---------------------------------------------------------------------------
// Plan helpers
// ---------------------------------------------------------------------------

export function countPlanActions(items: PlanItem[]): PlanSummary {
  const summary: PlanSummary = {
    create: 0,
    replace: 0,
    remove: 0,
    noop: 0,
    conflict: 0,
    blocked: 0,
  }
  for (const item of items) summary[item.action] += 1
  return summary
}

export function planNeedsForce(plan: OperationPlan): boolean {
  return plan.summary.conflict > 0
}

export function planHasBlockedItems(plan: OperationPlan): boolean {
  return plan.summary.blocked > 0 || plan.items.some((item) => item.action === 'blocked')
}

export function planActionCount(summary: PlanSummary): number {
  return (
    summary.create +
    summary.replace +
    summary.remove +
    summary.noop +
    summary.conflict +
    summary.blocked
  )
}

export function actionRequired(plan: OperationPlan): boolean {
  return plan.items.some((item) => item.action !== 'noop')
}
