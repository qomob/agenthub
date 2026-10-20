//! Public data model shared by the desktop UI, the CLI and the core engine.
//!
//! All structures serialized to the frontend use camelCase field names; enum
//! variants use snake_case strings so they are stable across languages.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Common enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Global,
    Project,
    Custom,
}

impl Default for Scope {
    fn default() -> Self {
        Scope::Global
    }
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Project => "project",
            Scope::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStrategy {
    Symlink,
    Copy,
}

impl InstallStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallStrategy::Symlink => "symlink",
            InstallStrategy::Copy => "copy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Skill,
    Mcp,
    Plugin,
}

impl CapabilityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityKind::Skill => "skill",
            CapabilityKind::Mcp => "mcp",
            CapabilityKind::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Installed,
    NotInstalled,
    Unsupported,
    VerificationRequired,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymlinkSupport {
    Supported,
    Unsupported,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSupport {
    NativeManaged,
    FileManaged,
    DiscoveryOnly,
    ManualOnly,
    Unsupported,
    Unverified,
}

impl PluginSupport {
    pub fn as_str(self) -> &'static str {
        match self {
            PluginSupport::NativeManaged => "native_managed",
            PluginSupport::FileManaged => "file_managed",
            PluginSupport::DiscoveryOnly => "discovery_only",
            PluginSupport::ManualOnly => "manual_only",
            PluginSupport::Unsupported => "unsupported",
            PluginSupport::Unverified => "unverified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationState {
    Installed,
    Disabled,
    Missing,
    Conflict,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Import,
    Install,
    Uninstall,
    Update,
    Sync,
    McpImport,
    McpDistribute,
    McpRemove,
    ProfileApply,
    Rollback,
    Backup,
    Scan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Planned,
    Running,
    Success,
    Partial,
    Failed,
    RolledBack,
    /// v3.1: an external modification prevented automatic compensation.
    NeedsManualRecovery,
    /// v3.1: a recovery run restored the affected targets.
    Recovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Create,
    Replace,
    Noop,
    Conflict,
    Blocked,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
    StreamableHttp,
}

impl McpTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            McpTransport::Stdio => "stdio",
            McpTransport::Http => "http",
            McpTransport::Sse => "sse",
            McpTransport::StreamableHttp => "streamable_http",
        }
    }
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

/// Persisted agent registration. The adapter id determines behaviour; paths can
/// be overridden by the user when an agent is installed in a non-standard place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    pub id: String,
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub adapter_id: String,
    pub app_path: Option<String>,
    pub home_path: Option<String>,
    pub custom_skill_roots: Vec<String>,
    pub notes: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRoot {
    pub path: String,
    pub scope: Scope,
    pub source: String,
    pub exists: bool,
    pub writable: bool,
    pub skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTarget {
    pub path: String,
    pub format: String,
    pub scope: Scope,
    pub exists: bool,
    pub writable: bool,
    pub transports: Vec<McpTransport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    pub id: String,
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub adapter_id: String,
    pub status: AgentStatus,
    pub app_path: Option<String>,
    pub home_path: Option<String>,
    /// User-added skill roots (project folders or custom locations).
    pub custom_skill_roots: Vec<String>,
    pub detected_at: Option<String>,
    pub last_scan_at: Option<String>,
    pub skill_roots: Vec<SkillRoot>,
    pub mcp_targets: Vec<McpTarget>,
    pub plugin_support: PluginSupport,
    pub symlink_support: SymlinkSupport,
    pub notes: Vec<String>,
    pub skill_count: usize,
    pub installed_skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    pub id: String,
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub status: AgentStatus,
    pub skill_count: usize,
    pub installed_skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPathUpdate {
    pub agent_id: String,
    pub app_path: Option<String>,
    pub home_path: Option<String>,
    pub custom_skill_roots: Vec<String>,
}

// ---------------------------------------------------------------------------
// Skills / capability registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: String,
    pub source_type: String,
    pub uri: String,
    pub reference: Option<String>,
    pub imported_at: String,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityVersion {
    pub id: String,
    pub capability_id: String,
    pub version_label: String,
    pub content_hash: String,
    pub storage_path: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub content_hash: String,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub source: Option<SourceInfo>,
    pub current_version_id: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub installations: Vec<InstallationView>,
    pub update_available: bool,
    pub drift_count: usize,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub skill: Skill,
    pub versions: Vec<CapabilityVersion>,
    pub files: Vec<FileEntry>,
    pub skill_md: String,
    pub manifest: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationView {
    pub id: String,
    pub capability_id: String,
    pub version_id: String,
    pub version_label: String,
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub scope: Scope,
    pub strategy: InstallStrategy,
    pub target_path: String,
    pub state: InstallationState,
    pub content_hash: Option<String>,
    pub installed_at: String,
    pub verified_at: Option<String>,
    pub last_check_at: Option<String>,
    /// True when the deployed content no longer matches the recorded version.
    pub drift: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub source: ImportSource,
    pub skill_path: Option<String>,
    pub version_label: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImportSource {
    Folder { path: String },
    Archive { path: String },
    SkillMd { path: String },
    Git { url: String, reference: Option<String> },
    AgentDir { agent_id: String, path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub skill: Skill,
    pub version: CapabilityVersion,
    pub created: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub capability_id: String,
    pub version_id: Option<String>,
    pub agent_ids: Vec<String>,
    pub strategy: Option<InstallStrategy>,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallRequest {
    pub installation_ids: Vec<String>,
    pub force: bool,
}

// ---------------------------------------------------------------------------
// Discovery / scanning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredSkill {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub version: Option<String>,
    pub path: String,
    pub root: String,
    /// Source classification of the root (adapter/project/custom/builtin/
    /// marketplace/workspace/connector/shared); empty for older scan payloads.
    #[serde(default)]
    pub root_source: String,
    pub scope: Scope,
    pub agent_key: Option<String>,
    pub content_hash: String,
    pub file_count: i64,
    pub size_bytes: i64,
    pub valid: bool,
    pub issues: Vec<String>,
    pub library_skill_id: Option<String>,
    pub library_version_id: Option<String>,
    pub in_sync: bool,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootScan {
    pub path: String,
    pub scope: Scope,
    pub exists: bool,
    pub discovered: usize,
    pub invalid: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentScan {
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub status: AgentStatus,
    pub roots: Vec<RootScan>,
    pub skills: Vec<DiscoveredSkill>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned_at: String,
    pub agents: Vec<AgentScan>,
    pub total_skills: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub invalid: usize,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub env_keys: Vec<String>,
    pub credential_refs: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub distributions: Vec<McpDistribution>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDistribution {
    pub installation_id: Option<String>,
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub target_path: String,
    pub config_key: String,
    pub state: InstallationState,
    pub content_hash: Option<String>,
    pub installed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpUpsertRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub env_keys: Vec<String>,
    pub credential_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDistributeRequest {
    pub server_id: String,
    pub agent_ids: Vec<String>,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDiscoverRequest {
    pub agent_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredMcp {
    pub name: String,
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub target_path: String,
    pub format: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub env_keys: Vec<String>,
    pub raw: serde_json::Value,
    pub issues: Vec<String>,
    pub already_known: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpImportRequest {
    pub agent_id: String,
    pub target_path: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecordView {
    pub id: String,
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub name: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub install_state: String,
    pub managed_by: PluginSupport,
    pub permissions: Vec<String>,
    pub official_url: Option<String>,
    pub discovered_path: Option<String>,
    pub notes: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPlugin {
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub name: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub install_state: String,
    pub permissions: Vec<String>,
    pub official_url: Option<String>,
    pub discovered_path: Option<String>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItemView {
    pub capability_id: String,
    pub kind: CapabilityKind,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub target_agent_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub items: Vec<ProfileItemView>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpsertRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<ProfileItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItemInput {
    pub capability_id: String,
    pub enabled: bool,
    pub target_agent_keys: Vec<String>,
}

// ---------------------------------------------------------------------------
// Operations, plans and snapshots
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: String,
    pub capability_id: Option<String>,
    pub capability_name: String,
    pub version_id: Option<String>,
    pub version_label: Option<String>,
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub install_kind: String,
    pub action: PlanAction,
    pub strategy: InstallStrategy,
    pub target_path: String,
    pub source_path: Option<String>,
    pub current_hash: Option<String>,
    pub desired_hash: Option<String>,
    pub config_key: Option<String>,
    /// Installation scope (global / project / custom root).
    #[serde(default)]
    pub scope: Scope,
    /// Format of the target configuration file (MCP items only).
    pub target_format: Option<String>,
    /// Canonical JSON payload of the managed entry (MCP items only).
    pub payload: Option<serde_json::Value>,
    pub warnings: Vec<String>,
    pub requires_restart: bool,
    pub diff: Option<String>,
    /// Serialized previous installation row (used to restore DB state on rollback).
    pub previous_installation: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub create: usize,
    pub replace: usize,
    pub remove: usize,
    pub noop: usize,
    pub conflict: usize,
    pub blocked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationPlan {
    pub operation_id: String,
    pub kind: OperationKind,
    pub dry_run: bool,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
    pub requires_restart_agents: Vec<String>,
    pub requires_confirmation: bool,
    pub summary: PlanSummary,
    pub created_at: String,
    /// v3.1: SHA-256 of the canonical plan payload. Approval is bound to it.
    #[serde(default)]
    pub plan_hash: String,
    /// v3.1: ISO timestamp after which an approval must be re-issued.
    #[serde(default)]
    pub expires_at: String,
    /// v3.1: `none` / `approved` / `expired`.
    #[serde(default = "default_approval_state")]
    pub approval_state: String,
}

fn default_approval_state() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub plan_item_id: String,
    pub agent_key: String,
    pub target_path: String,
    pub status: String,
    pub message: String,
    pub backup_path: Option<String>,
    pub installation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub operation_id: String,
    pub status: OperationStatus,
    pub items: Vec<ItemResult>,
    pub diagnostics: Vec<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationView {
    pub id: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub dry_run: bool,
    pub actor: String,
    pub summary: String,
    pub error: Option<String>,
    pub rollback_of: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub item_count: usize,
    pub plan: Option<OperationPlan>,
    pub result: Option<OperationResult>,
    pub can_rollback: bool,
    /// v3.1 approval surface.
    #[serde(default)]
    pub plan_hash: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub approval_state: String,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub recovery_state: Option<String>,
    #[serde(default)]
    pub steps: Vec<OperationStepView>,
}

/// `PRAGMA integrity_check` + `foreign_key_check` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReport {
    pub ok: bool,
    pub schema_version: i64,
    pub details: Vec<String>,
}

/// A single durable journal entry for a target touched by an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStepView {
    pub id: String,
    pub operation_id: String,
    pub sequence_no: i64,
    pub target_path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub snapshot_ref: Option<String>,
    pub state: String,
    pub error_code: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// One-time approval grant returned by `operations_approve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGrant {
    pub operation_id: String,
    pub plan_hash: String,
    pub approval_token: String,
    pub approved_at: String,
    pub expires_at: String,
}

/// v3.1 per-agent feature evidence. `support_state` and `evidence_level` are
/// deliberately separate: support answers "is it possible" while evidence
/// answers "how strongly do we know".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterEvidenceView {
    pub id: String,
    pub agent_key: String,
    pub feature_key: String,
    pub support_state: String,
    pub evidence_level: String,
    pub source_url: Option<String>,
    pub tested_at: Option<String>,
    pub agent_version: Option<String>,
    pub adapter_version: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInspection {
    pub agent_id: String,
    pub agent_key: String,
    pub capabilities: Vec<AdapterEvidenceView>,
    pub inspected_at: String,
}

/// Result of `operations_recover`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub operation_id: String,
    pub status: String,
    pub items: Vec<ItemResult>,
    pub diagnostics: Vec<String>,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub id: String,
    pub operation_id: String,
    pub kind: String,
    pub original_path: String,
    pub backup_path: String,
    pub existed: bool,
    pub hash_before: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Dashboard / settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub agents_detected: usize,
    pub agents_total: usize,
    pub skills: usize,
    pub mcp_servers: usize,
    pub plugins: usize,
    pub installations: usize,
    pub pending_updates: usize,
    pub conflicts: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub severity: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub agent_key: Option<String>,
    pub capability_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub stats: DashboardStats,
    pub agents: Vec<AgentSummary>,
    pub recent_operations: Vec<OperationView>,
    pub issues: Vec<Issue>,
    pub library_root: String,
    pub initialized: bool,
    pub last_scan_at: Option<String>,
    pub updates: Vec<Skill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub library_root: String,
    pub default_strategy: InstallStrategy,
    pub language: String,
    pub theme: String,
    pub scan_on_startup: bool,
    pub git_import_enabled: bool,
    pub keychain_available: bool,
    pub database_path: String,
    pub app_version: String,
    pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    pub library_root: Option<String>,
    pub default_strategy: Option<InstallStrategy>,
    pub language: Option<String>,
    pub theme: Option<String>,
    pub scan_on_startup: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
    pub version: String,
    pub home: String,
    pub library_root: String,
    pub database_path: String,
    pub agents: Vec<AgentDoctorCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDoctorCheck {
    pub key: String,
    pub name: String,
    pub status: AgentStatus,
    pub install_evidence: Vec<String>,
    pub skill_roots: Vec<String>,
    pub mcp_targets: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCheck {
    pub checked_at: String,
    pub drifted: Vec<InstallationView>,
    pub missing: Vec<InstallationView>,
    pub updates: Vec<Skill>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub id: String,
    pub path: String,
    pub created_at: String,
    pub entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupView {
    pub id: String,
    pub path: String,
    pub created_at: String,
    pub size_bytes: i64,
}

// ---------------------------------------------------------------------------
// String conversions (database / CLI stable values)
// ---------------------------------------------------------------------------

impl OperationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OperationStatus::Planned => "planned",
            OperationStatus::Running => "running",
            OperationStatus::Success => "success",
            OperationStatus::Partial => "partial",
            OperationStatus::Failed => "failed",
            OperationStatus::RolledBack => "rolled_back",
            OperationStatus::NeedsManualRecovery => "needs_manual_recovery",
            OperationStatus::Recovered => "recovered",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "planned" => OperationStatus::Planned,
            "running" => OperationStatus::Running,
            "success" => OperationStatus::Success,
            "partial" => OperationStatus::Partial,
            "rolled_back" => OperationStatus::RolledBack,
            "needs_manual_recovery" => OperationStatus::NeedsManualRecovery,
            "recovered" => OperationStatus::Recovered,
            _ => OperationStatus::Failed,
        }
    }
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OperationKind::Import => "import",
            OperationKind::Install => "install",
            OperationKind::Uninstall => "uninstall",
            OperationKind::Update => "update",
            OperationKind::Sync => "sync",
            OperationKind::McpImport => "mcp_import",
            OperationKind::McpDistribute => "mcp_distribute",
            OperationKind::McpRemove => "mcp_remove",
            OperationKind::ProfileApply => "profile_apply",
            OperationKind::Rollback => "rollback",
            OperationKind::Backup => "backup",
            OperationKind::Scan => "scan",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "import" => OperationKind::Import,
            "install" => OperationKind::Install,
            "uninstall" => OperationKind::Uninstall,
            "update" => OperationKind::Update,
            "sync" => OperationKind::Sync,
            "mcp_import" => OperationKind::McpImport,
            "mcp_distribute" => OperationKind::McpDistribute,
            "mcp_remove" => OperationKind::McpRemove,
            "profile_apply" => OperationKind::ProfileApply,
            "rollback" => OperationKind::Rollback,
            "backup" => OperationKind::Backup,
            _ => OperationKind::Scan,
        }
    }
}

impl PlanAction {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanAction::Create => "create",
            PlanAction::Replace => "replace",
            PlanAction::Noop => "noop",
            PlanAction::Conflict => "conflict",
            PlanAction::Blocked => "blocked",
            PlanAction::Remove => "remove",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "create" => PlanAction::Create,
            "replace" => PlanAction::Replace,
            "noop" => PlanAction::Noop,
            "conflict" => PlanAction::Conflict,
            "remove" => PlanAction::Remove,
            _ => PlanAction::Blocked,
        }
    }
}

impl InstallationState {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallationState::Installed => "installed",
            InstallationState::Disabled => "disabled",
            InstallationState::Missing => "missing",
            InstallationState::Conflict => "conflict",
            InstallationState::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "installed" => InstallationState::Installed,
            "disabled" => InstallationState::Disabled,
            "missing" => InstallationState::Missing,
            "conflict" => InstallationState::Conflict,
            _ => InstallationState::Error,
        }
    }
}

impl Scope {
    pub fn parse(value: &str) -> Self {
        match value {
            "project" => Scope::Project,
            "custom" => Scope::Custom,
            _ => Scope::Global,
        }
    }
}

impl InstallStrategy {
    pub fn parse(value: &str) -> Self {
        match value {
            "symlink" => InstallStrategy::Symlink,
            _ => InstallStrategy::Copy,
        }
    }
}

impl McpTransport {
    pub fn parse(value: &str) -> Self {
        match value {
            "http" => McpTransport::Http,
            "sse" => McpTransport::Sse,
            "streamable_http" => McpTransport::StreamableHttp,
            _ => McpTransport::Stdio,
        }
    }
}

impl CapabilityKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "mcp" => CapabilityKind::Mcp,
            "plugin" => CapabilityKind::Plugin,
            _ => CapabilityKind::Skill,
        }
    }
}

impl PluginSupport {
    pub fn parse(value: &str) -> Self {
        match value {
            "native_managed" => PluginSupport::NativeManaged,
            "file_managed" => PluginSupport::FileManaged,
            "discovery_only" => PluginSupport::DiscoveryOnly,
            "manual_only" => PluginSupport::ManualOnly,
            "unsupported" => PluginSupport::Unsupported,
            _ => PluginSupport::Unverified,
        }
    }
}

// ---------------------------------------------------------------------------
// Bootstrap payload
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub settings: AppSettings,
    pub agents: Vec<AgentView>,
    pub platform: String,
    pub core_version: String,
    pub project_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretState {
    pub key: String,
    pub present: bool,
}

// ---------------------------------------------------------------------------
// Capability health, audits, dependency diagnostics, validation evidence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Verified,
    Unverified,
    NeedsAttention,
    Failed,
    NotDeployed,
}

impl HealthState {
    pub fn as_str(self) -> &'static str {
        match self {
            HealthState::Verified => "verified",
            HealthState::Unverified => "unverified",
            HealthState::NeedsAttention => "needs_attention",
            HealthState::Failed => "failed",
            HealthState::NotDeployed => "not_deployed",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "verified" => HealthState::Verified,
            "needs_attention" => HealthState::NeedsAttention,
            "failed" => HealthState::Failed,
            "not_deployed" => HealthState::NotDeployed,
            _ => HealthState::Unverified,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFinding {
    /// Stable deterministic rule id, e.g. `script.pipe_to_shell`.
    pub id: String,
    /// info | low | medium | high | critical
    pub severity: String,
    /// spec | security | dependency | quality | compatibility
    pub category: String,
    pub title: String,
    pub detail: String,
    pub path: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAudit {
    pub id: String,
    pub capability_id: String,
    pub version_id: String,
    pub version_label: String,
    /// static | environment
    pub kind: String,
    /// pass | warn | fail | unknown
    pub status: String,
    /// low | medium | high | critical
    pub risk: String,
    pub summary: String,
    pub findings: Vec<AuditFinding>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyItem {
    pub name: String,
    /// runtime | binary | manifest | mcp | credential | package
    pub kind: String,
    pub required_version: Option<String>,
    pub found: Option<String>,
    /// ok | missing | warning | unknown
    pub status: String,
    pub detail: String,
    pub source_path: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyReport {
    pub capability_id: String,
    pub version_id: String,
    /// healthy | needs_attention | unknown
    pub status: String,
    pub risk: String,
    pub summary: String,
    pub items: Vec<DependencyItem>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// added | removed | modified
    pub change: String,
    pub size_before: i64,
    pub size_after: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateImpact {
    pub id: String,
    pub capability_id: String,
    pub from_version_id: String,
    pub from_version_label: String,
    pub to_version_id: String,
    pub to_version_label: String,
    pub risk: String,
    pub summary: String,
    pub file_changes: Vec<FileChange>,
    /// e.g. docs_only, new_script, removed_script, new_network, new_dependency
    pub classes: Vec<String>,
    pub findings: Vec<AuditFinding>,
    pub affected_installation_count: usize,
    pub affected_agent_keys: Vec<String>,
    pub affected_project_roots: Vec<String>,
    pub requires_revalidation: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationRun {
    pub id: String,
    pub capability_id: String,
    pub version_id: String,
    pub version_label: String,
    pub agent_id: Option<String>,
    pub agent_key: Option<String>,
    pub agent_name: Option<String>,
    /// manual | static | environment | regression
    pub kind: String,
    /// passed | failed | needs_attention | unverified
    pub status: String,
    pub case_name: String,
    pub notes: String,
    pub evidence: String,
    pub actor: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationRequest {
    pub capability_id: String,
    pub version_id: Option<String>,
    pub agent_id: Option<String>,
    pub kind: Option<String>,
    /// passed | failed | needs_attention | unverified
    pub status: String,
    pub case_name: String,
    pub notes: Option<String>,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityHealth {
    pub capability_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version_label: String,
    pub status: HealthState,
    pub reasons: Vec<String>,
    pub audit: Option<CapabilityAudit>,
    pub environment: Option<DependencyReport>,
    pub validations: Vec<ValidationRun>,
    pub installation_count: usize,
    pub agent_keys: Vec<String>,
    pub verified_agent_keys: Vec<String>,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSummary {
    pub total: usize,
    pub verified: usize,
    pub unverified: usize,
    pub needs_attention: usize,
    pub failed: usize,
    pub not_deployed: usize,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthOverview {
    pub summary: HealthSummary,
    pub capabilities: Vec<CapabilityHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealth {
    pub agent_id: String,
    pub agent_key: String,
    pub agent_name: String,
    pub status: AgentStatus,
    pub version: Option<String>,
    pub previous_version: Option<String>,
    pub version_changed: bool,
    pub version_changed_at: Option<String>,
    pub paths_ok: bool,
    pub notes: Vec<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealthReport {
    pub checked_at: String,
    pub agents: Vec<AgentHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRequest {
    pub capability_id: String,
    pub version_id: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactRequest {
    pub capability_id: String,
    pub from_version_id: Option<String>,
    pub to_version_id: String,
}


// ---------------------------------------------------------------------------
// Bulk import of previously discovered skills
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportRequest {
    /// Restrict to these agent ids; empty/None means every scanned agent.
    pub agent_ids: Option<Vec<String>>,
    /// Import only the discovered paths the user selected. None means all.
    pub paths: Option<Vec<String>>,
    /// Also adopt skills from read-only roots (built-in / marketplace /
    /// workspace / connector caches). Off by default.
    pub include_read_only_sources: bool,
    pub max_items: Option<usize>,
    /// Preview only: compute what would be imported without writing the library.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportedItem {
    pub slug: String,
    pub name: String,
    pub version_label: String,
    pub agent_key: Option<String>,
    pub path: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportFailure {
    pub path: String,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportResult {
    pub imported: Vec<ScanImportedItem>,
    pub imported_count: usize,
    pub skipped_existing: usize,
    pub skipped_duplicate: usize,
    pub skipped_read_only: usize,
    pub skipped_limit: usize,
    pub failed: Vec<ScanImportFailure>,
    pub total_candidates: usize,
    pub finished_at: String,
}
