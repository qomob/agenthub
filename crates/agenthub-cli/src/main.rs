//! AgentHub command line interface.
//!
//! This is a thin, scriptable front-end over the same `agenthub_core::AgentHub`
//! engine that powers the desktop app: it never re-implements discovery,
//! planning, execution or MCP writing.
//!
//! Global flags:
//!   --home <dir>   fake/alternate home directory (same meaning as AGENTHUB_HOME)
//!   --json         machine readable JSON on stdout (errors as JSON on stderr)
//!
//! Exit codes:
//!   0  success
//!   1  failure for the legacy commands (invalid input, I/O or engine error)
//!   2  partial success, or a plan that contains blocked/conflict items, or a
//!      plan that was printed for confirmation because `--yes` was missing
//!   3  invalid input / unsupported capability (v3.1 API contract)
//!   4  approval required, stale plan, expired approval or wrong token
//!   5  runtime failure or a recovery that needs manual intervention
//!
//! The v3.1 commands (`operations preview|approve|apply|recover`,
//! `agents inspect`) emit the shared `ApiResult` envelope as JSON, and their
//! `error.code` is derived from the same classification as the exit code.
//!
//! Every mutating command follows the same contract: a plan is produced first
//! and printed; nothing is written unless `--yes` is passed and the plan has no
//! blocked items. `--dry-run` always stops after printing the plan.

use std::path::{Path, PathBuf};

use agenthub_core::contract::{ApiError, ApiResult, ErrorCode};
use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};

const EXIT_OK: u8 = 0;
/// Legacy commands keep their historical generic-failure code.
const EXIT_FAIL: u8 = 1;
const EXIT_PARTIAL: u8 = 2;
/// v3.1: invalid input, unsupported capability, unverified adapter.
const EXIT_INVALID: u8 = 3;
/// v3.1: approval required, stale plan, expired approval, wrong token.
const EXIT_APPROVAL: u8 = 4;
/// v3.1: runtime failure, or a recovery that needs manual intervention.
const EXIT_RUNTIME: u8 = 5;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(
    name = "agenthub",
    version,
    about = "Local multi-agent capability manager (shared engine with the desktop app)"
)]
struct Cli {
    /// Alternate home directory used to resolve agent folders (maps AGENTHUB_HOME).
    #[arg(long, global = true, value_name = "DIR")]
    home: Option<PathBuf>,
    /// Machine readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run local diagnostics (library, database, data dir, git, agents).
    Doctor,
    /// Export a secret-free diagnostics report (JSON) for bug reports.
    Diagnostics {
        /// Output file path (defaults to ~/Downloads/agenthub-diagnostics-<ts>.json).
        #[arg(value_name = "FILE")]
        path: Option<PathBuf>,
    },
    /// Inspect detected agents.
    Agents {
        #[command(subcommand)]
        command: AgentsCommand,
    },
    /// Inspect, import and distribute skills.
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
    /// Inspect MCP servers and distribute them to agents.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Inspect the operation log.
    Operations {
        #[command(subcommand)]
        command: OperationsCommand,
    },
    /// Inspect capability profiles.
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
    /// Capability health overview (audits, validation evidence) or agent versions.
    Health(HealthArgs),
    /// Create a full metadata backup.
    Backup {
        #[command(subcommand)]
        command: BackupCommand,
    },
    /// List existing backups.
    Backups {
        #[command(subcommand)]
        command: BackupsCommand,
    },
    /// Manage the My AI Stack inventory (M1-B; reads + direct writes).
    Stack {
        #[command(subcommand)]
        command: StackCommand,
    },
    /// Manage subscriptions registered on stack items (M1-B).
    Sub {
        #[command(subcommand)]
        command: SubCommand,
    },
    /// Cost Optimizer (M2-B): read-only recommendations and scenario math.
    Cost {
        #[command(subcommand)]
        command: CostCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CostCommand {
    /// Print optimizer recommendations (read-only). Reads the shared
    /// threshold/window settings; write nothing.
    Report,
    /// Simulate scenario actions (read-only arithmetic, nothing persisted).
    /// Repeatable flags:
    ///   --cancel <subscription_id>
    ///   --reduce <subscription_id>:<new_amount_minor>
    ///   --replace <subscription_id>:<new_amount_minor>:<CURRENCY>:<period>
    ///   --deactivate <stack_item_id>
    Simulate {
        #[arg(long = "cancel")]
        cancel: Vec<String>,
        #[arg(long = "reduce")]
        reduce: Vec<String>,
        #[arg(long = "replace")]
        replace: Vec<String>,
        #[arg(long = "deactivate")]
        deactivate: Vec<String>,
    },
    /// Show the persisted optimizer settings (shared by GUI and CLI).
    Settings,
    /// Update optimizer settings (shared validation with the GUI).
    ///   --threshold <CURRENCY:minor> (repeatable; empty --threshold "" clears all)
    ///   --window <days> (1-30; omit to keep; `--window default` clears)
    Set {
        #[arg(long = "threshold")]
        threshold: Vec<String>,
        #[arg(long = "window")]
        window: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AgentsCommand {
    /// List every supported agent with detection status and roots.
    List,
    /// Inspect one agent: adapter support state and evidence (v3.1).
    Inspect(AgentInspectArgs),
}

#[derive(Debug, Subcommand)]
enum SkillsCommand {
    /// List skills stored in the library.
    List,
    /// Scan every agent skill root on disk (read-only).
    Scan,
    /// Import a skill from a folder, a zip archive, a single SKILL.md or git.
    Import(ImportArgs),
    /// Plan (and with --yes apply) a skill installation.
    Install(InstallArgs),
    /// Plan (and with --yes apply) removal of installed skills.
    Uninstall(UninstallArgs),
    /// Reconcile drift/updates: check_changes + a distribution plan.
    Sync(SyncArgs),
    /// Roll back a successful operation (or the latest rollbackable one).
    Rollback(RollbackArgs),
    /// Run the deterministic static security audit for a skill version.
    Audit(AuditArgs),
    /// Check runtimes, manifests and dependencies for a skill version.
    Deps(AuditArgs),
    /// Analyse the impact of updating a skill from one version to another.
    Impact(ImpactArgs),
    /// Record real (manual) validation evidence for a skill version.
    Validate(ValidateArgs),
    /// Adopt skills found by the last scan into the central library.
    Adopt(AdoptArgs),
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// List MCP server definitions known to AgentHub.
    List,
    /// Discover MCP entries already present in agent config files.
    Discover(DiscoverArgs),
    /// Plan (and with --yes apply) an MCP distribution.
    Distribute(McpDistributeArgs),
}

#[derive(Debug, Subcommand)]
enum OperationsCommand {
    /// List recent operations.
    List(OperationsArgs),
    /// Read-only approval preview: plan hash, expiry, approval state, journal.
    Preview(OperationIdArgs),
    /// Approve a planned operation and print its one-time approval token.
    Approve(ApproveArgs),
    /// Apply an approved operation with its one-time token.
    Apply(ApplyArgs),
    /// Recover a failed/partial operation from its durable journal.
    Recover(OperationIdArgs),
}

#[derive(Debug, Subcommand)]
enum ProfilesCommand {
    /// List profiles (built-in profiles are seeded on first open).
    List,
}

#[derive(Debug, Subcommand)]
enum BackupCommand {
    /// Snapshot the metadata database into the library backups directory.
    Create,
    /// Restore the metadata database from a validated backup (safety copy first).
    Restore(RestoreArgs),
}

#[derive(Debug, Args)]
struct RestoreArgs {
    /// Backup id from `backups list`.
    id: String,
}

#[derive(Debug, Subcommand)]
enum BackupsCommand {
    /// List backups present in the library.
    List,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BillingTypeArg {
    Free,
    Subscription,
    Usage,
    Hybrid,
    Unknown,
}

impl BillingTypeArg {
    fn as_str(self) -> &'static str {
        match self {
            BillingTypeArg::Free => "free",
            BillingTypeArg::Subscription => "subscription",
            BillingTypeArg::Usage => "usage",
            BillingTypeArg::Hybrid => "hybrid",
            BillingTypeArg::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BillingPeriodArg {
    Monthly,
    Quarterly,
    Semiannual,
    Yearly,
    Usage,
    Unknown,
}

impl BillingPeriodArg {
    fn as_str(self) -> &'static str {
        match self {
            BillingPeriodArg::Monthly => "monthly",
            BillingPeriodArg::Quarterly => "quarterly",
            BillingPeriodArg::Semiannual => "semiannual",
            BillingPeriodArg::Yearly => "yearly",
            BillingPeriodArg::Usage => "usage",
            BillingPeriodArg::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SubscriptionStatusArg {
    Active,
    Paused,
    Cancelled,
    Expired,
    Unknown,
}

impl SubscriptionStatusArg {
    fn as_str(self) -> &'static str {
        match self {
            SubscriptionStatusArg::Active => "active",
            SubscriptionStatusArg::Paused => "paused",
            SubscriptionStatusArg::Cancelled => "cancelled",
            SubscriptionStatusArg::Expired => "expired",
            SubscriptionStatusArg::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Subcommand)]
enum StackCommand {
    /// List stack items (read-only).
    List,
    /// Show one stack item: detections, subscriptions and merged children.
    Show { id: String },
    /// Add a manual stack item (direct write per D-1).
    Add(StackUpsertArgs),
    /// Edit a stack item (direct write per D-1). Omitted fields keep their
    /// current value; pass an empty string (`--vendor ""`) to clear a field.
    Edit(StackEditArgs),
    /// Mark a detected item as user-confirmed (direct write per D-1).
    Confirm { id: String },
    /// Merge one item into another (high impact: requires --yes).
    /// Subscriptions stay on the child item and the merge stays reversible
    /// (unmerge is available in the GUI / service layer, not exposed in CLI M1-B).
    Merge {
        /// The child item id.
        child_id: String,
        /// The target item id (must not itself be merged).
        #[arg(long)]
        into: String,
        /// Confirm the merge.
        #[arg(long)]
        yes: bool,
    },
    /// Delete a stack item (high impact: requires --yes). Items holding any
    /// subscription or merged children are rejected by the core.
    Delete {
        id: String,
        /// Confirm the deletion.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Args)]
struct StackUpsertArgs {
    /// Display name (required, non-empty).
    #[arg(long)]
    name: String,
    #[arg(long)]
    vendor: Option<String>,
    /// Near-purpose group label (Cost Optimizer R1 input).
    #[arg(long = "overlap-group")]
    overlap_group: Option<String>,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Args)]
struct StackEditArgs {
    /// Stack item id (see `stack list --json`).
    id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    vendor: Option<String>,
    #[arg(long = "overlap-group")]
    overlap_group: Option<String>,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Subcommand)]
enum SubCommand {
    /// Register a subscription on a stack item (direct write per D-1).
    /// Unknown facts are simply omitted; they are never invented.
    Add(SubAddArgs),
    /// Edit a subscription (direct write per D-1). Omitted fields keep their
    /// current value; pass an empty string to clear an optional field.
    Edit(SubEditArgs),
    /// Delete a subscription record (high impact: requires --yes).
    Delete {
        id: String,
        /// Confirm the deletion.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Args)]
struct SubAddArgs {
    /// Stack item id to attach the subscription to.
    #[arg(long = "stack")]
    stack_item_id: String,
    /// Product or service name (required).
    #[arg(long = "product")]
    product_name: String,
    #[arg(long)]
    vendor: Option<String>,
    /// Plan / tier name.
    #[arg(long)]
    plan: Option<String>,
    /// free | subscription | usage | hybrid | unknown.
    #[arg(long)]
    billing_type: BillingTypeArg,
    /// Amount in MAJOR units of the currency (e.g. `20.5` for USD 20.50).
    /// Omit when the amount is unknown — it is never filled with a default.
    #[arg(long)]
    amount: Option<String>,
    /// ISO-4217 code (required when --amount is given).
    #[arg(long)]
    currency: Option<String>,
    /// monthly | quarterly | semiannual | yearly | usage | unknown.
    #[arg(long)]
    period: Option<BillingPeriodArg>,
    /// Subscription start date (YYYY-MM-DD).
    #[arg(long = "started-on")]
    started_on: Option<String>,
    /// Next renewal date (YYYY-MM-DD).
    #[arg(long = "renews-on")]
    renews_on: Option<String>,
    /// active | paused | cancelled | expired | unknown (default: active).
    #[arg(long)]
    status: Option<SubscriptionStatusArg>,
    #[arg(long)]
    notes: Option<String>,
    /// Where this information came from (e.g. "user entry", invoice header).
    #[arg(long = "info-source")]
    info_source: Option<String>,
}

#[derive(Debug, Args)]
struct SubEditArgs {
    /// Subscription id (see `stack show --json` or the GUI).
    id: String,
    /// Move the subscription to another stack item (optional).
    #[arg(long = "stack")]
    stack_item_id: Option<String>,
    #[arg(long = "product")]
    product_name: Option<String>,
    #[arg(long)]
    vendor: Option<String>,
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    billing_type: Option<BillingTypeArg>,
    /// Amount in major units; empty string clears the amount.
    #[arg(long)]
    amount: Option<String>,
    /// ISO-4217 code; empty string clears it.
    #[arg(long)]
    currency: Option<String>,
    #[arg(long)]
    period: Option<BillingPeriodArg>,
    #[arg(long = "started-on")]
    started_on: Option<String>,
    #[arg(long = "renews-on")]
    renews_on: Option<String>,
    #[arg(long)]
    status: Option<SubscriptionStatusArg>,
    #[arg(long)]
    notes: Option<String>,
    #[arg(long = "info-source")]
    info_source: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StrategyArg {
    Copy,
    Symlink,
}

impl From<StrategyArg> for InstallStrategy {
    fn from(value: StrategyArg) -> Self {
        match value {
            StrategyArg::Copy => InstallStrategy::Copy,
            StrategyArg::Symlink => InstallStrategy::Symlink,
        }
    }
}

#[derive(Debug, Args)]
struct ImportArgs {
    /// Folder containing a skill (or a folder of skills), a .zip archive, or a SKILL.md file.
    #[arg(long, value_name = "PATH", conflicts_with = "git")]
    path: Option<PathBuf>,
    /// Git repository URL (https/ssh/git/file or an absolute local path).
    #[arg(long, value_name = "URL")]
    git: Option<String>,
    /// Branch or tag to clone when using --git.
    #[arg(long = "ref", value_name = "REF")]
    reference: Option<String>,
    /// Relative path of the skill inside the archive/repository when several exist.
    #[arg(long = "skill-path", value_name = "REL")]
    skill_path: Option<String>,
    /// Optional version label override.
    #[arg(long = "version-label", value_name = "LABEL")]
    version_label: Option<String>,
    /// Optional note stored with the version.
    #[arg(long, value_name = "TEXT")]
    notes: Option<String>,
}

#[derive(Debug, Args)]
struct InstallArgs {
    /// Skill id (or slug) from `skills list`.
    skill_id: String,
    /// Target agents as comma separated keys (e.g. codex,workbuddy).
    #[arg(long, value_name = "KEY,...", value_delimiter = ',', required = true)]
    agents: Vec<String>,
    /// Deployment strategy.
    #[arg(long, value_enum, default_value = "copy")]
    strategy: StrategyArg,
    /// Only print the plan; never write to agent directories.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Confirm and execute the plan.
    #[arg(long)]
    yes: bool,
    /// Allow overwriting content that AgentHub does not own (conflict items).
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct UninstallArgs {
    /// One or more installation ids (see `skills list --json`, installations[].id).
    #[arg(required = true)]
    installation_ids: Vec<String>,
    /// Only print the removal plan; never touch agent directories.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Confirm and execute the plan.
    #[arg(long)]
    yes: bool,
    /// Allow removing a target that changed after the preview.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct SyncArgs {
    /// Only print the change report and the distribution plan.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Confirm and execute every generated plan.
    #[arg(long)]
    yes: bool,
    /// Allow overwriting content that AgentHub does not own (conflict items).
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct RollbackArgs {
    /// Operation id, or `last` for the most recent rollbackable operation.
    target: String,
}

#[derive(Debug, Args)]
struct AuditArgs {
    /// Skill id or slug.
    skill: String,
    /// Version id (defaults to the current library version).
    #[arg(long)]
    version: Option<String>,
}

#[derive(Debug, Args)]
struct ImpactArgs {
    /// Skill id or slug.
    skill: String,
    /// Installed version id to compare from (defaults to the first installation).
    #[arg(long = "from")]
    from_version: Option<String>,
    /// Target version id (defaults to the current library version).
    #[arg(long = "to")]
    to_version: Option<String>,
}

#[derive(Debug, Args)]
struct ValidateArgs {
    /// Skill id or slug.
    skill: String,
    /// Agent key the evidence was produced with (omit for agent-neutral checks).
    #[arg(long)]
    agent: Option<String>,
    /// Validation case name, e.g. "generate invoice PDF".
    #[arg(long = "case")]
    case_name: String,
    /// passed | failed | needs_attention | unverified
    #[arg(long)]
    status: String,
    #[arg(long)]
    notes: Option<String>,
    #[arg(long)]
    evidence: Option<String>,
    /// Version id (defaults to the current library version).
    #[arg(long)]
    version: Option<String>,
    /// manual | static | environment | regression (defaults to manual).
    #[arg(long)]
    kind: Option<String>,
}

#[derive(Debug, Args)]
struct AdoptArgs {
    /// Restrict to these agent keys (comma separated); default = all scanned agents.
    #[arg(long, value_name = "KEY,...", value_delimiter = ',')]
    agents: Vec<String>,
    /// Also adopt skills from read-only roots (built-in/marketplace/workspace caches).
    #[arg(long = "include-read-only")]
    include_read_only: bool,
    /// Maximum number of skills to adopt in this run.
    #[arg(long, default_value_t = 200)]
    max: usize,
    /// Only print what would be adopted.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Confirm and write the central library.
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct HealthArgs {
    /// Show agent versions and health instead of the capability overview.
    #[arg(long)]
    agents: bool,
    /// Record the detected agent versions (implies --agents).
    #[arg(long)]
    record: bool,
}

#[derive(Debug, Args)]
struct DiscoverArgs {
    /// Restrict discovery to one agent key (e.g. codex).
    #[arg(long, value_name = "KEY")]
    agent: Option<String>,
}

#[derive(Debug, Args)]
struct McpDistributeArgs {
    /// MCP server id (or name) from `mcp list`.
    server_id: String,
    /// Target agents as comma separated keys.
    #[arg(long, value_name = "KEY,...", value_delimiter = ',', required = true)]
    agents: Vec<String>,
    /// Only print the plan; never write agent configuration files.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Confirm and execute the plan.
    #[arg(long)]
    yes: bool,
    /// Allow overwriting entries AgentHub does not manage (conflict items).
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct OperationsArgs {
    /// Maximum number of operations to return.
    #[arg(long, default_value_t = 50)]
    limit: usize,
}

#[derive(Debug, Args)]
struct OperationIdArgs {
    /// Operation id (see `operations list`).
    id: String,
}

#[derive(Debug, Args)]
struct ApproveArgs {
    /// Operation id to approve.
    id: String,
    /// Plan hash from `operations preview` / the plan that was shown to the user.
    #[arg(long = "plan-hash", value_name = "HASH")]
    plan_hash: String,
}

#[derive(Debug, Args)]
struct ApplyArgs {
    /// Operation id to apply.
    id: String,
    /// One-time approval token printed by `operations approve`.
    #[arg(long, value_name = "TOKEN")]
    token: String,
    /// Optional idempotency key so a retried apply reuses the stored result.
    #[arg(long = "idempotency-key", value_name = "KEY")]
    idempotency_key: Option<String>,
}

#[derive(Debug, Args)]
struct AgentInspectArgs {
    /// Agent key or id (see `agents list`).
    agent_key: String,
}

// ---------------------------------------------------------------------------
// Error / entry point
// ---------------------------------------------------------------------------

/// The v3.1 command family whose failures are classified into the shared
/// `ApiResult` envelope. The context matters because the same core error can
/// mean "stale plan" for one command and "runtime failure" for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiContext {
    Preview,
    Approve,
    Apply,
    Recover,
    Inspect,
}

/// A classified API failure: `ApiError` (code/message/retryable/details) plus
/// the process exit code produced by the same classification.
#[derive(Debug, Clone)]
struct ApiFailure {
    error: ApiError,
    exit: u8,
}

#[derive(Debug)]
struct CliError {
    message: String,
    /// Present for the v3.1 commands; legacy commands leave it `None` so their
    /// long-standing exit code 1 / plain stderr behaviour is preserved.
    api: Option<ApiFailure>,
}

impl CliError {
    fn legacy(message: impl Into<String>) -> Self {
        CliError {
            message: message.into(),
            api: None,
        }
    }

    fn api(failure: ApiFailure) -> Self {
        CliError {
            message: failure.error.message.clone(),
            api: Some(failure),
        }
    }

    fn invalid_input(message: impl Into<String>) -> Self {
        let code = ErrorCode::InvalidInput;
        CliError::api(ApiFailure {
            error: ApiError::new(code, message),
            exit: code.exit_code(),
        })
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<agenthub_core::Error> for CliError {
    fn from(value: agenthub_core::Error) -> Self {
        CliError::legacy(value.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        CliError::legacy(format!("json error: {value}"))
    }
}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        CliError::legacy(value)
    }
}

impl From<&str> for CliError {
    fn from(value: &str) -> Self {
        CliError::legacy(value.to_string())
    }
}

type CliResult<T> = std::result::Result<T, CliError>;

/// Map a core error to a stable code + exit code for the v3.1 commands.
///
/// `ErrorCode::exit_code()` is the baseline. Approval/staleness contexts remap
/// the sandbox/conflict family to `PLAN_STALE` (exit 4) as required by
/// `API_CONTRACTS.md` §6: missing approval, wrong token, expired approval and
/// a changed plan all mean "ask the user to re-approve", not "permission
/// denied" or "runtime failure".
fn map_api_error(context: ApiContext, error: &agenthub_core::Error) -> CliError {
    use agenthub_core::Error as CoreError;
    let base = ErrorCode::from(error);
    let (code, exit) = match context {
        ApiContext::Approve => match error {
            CoreError::Conflict(_)
            | CoreError::Blocked(_)
            | CoreError::ExternalModification(_) => (ErrorCode::PlanStale, EXIT_APPROVAL),
            _ => (base, base.exit_code()),
        },
        ApiContext::Apply => match error {
            CoreError::Conflict(_) | CoreError::Blocked(_) => {
                (ErrorCode::PlanStale, EXIT_APPROVAL)
            }
            // CAS races are runtime/recovery failures, not approval failures.
            CoreError::ExternalModification(_) => (base, base.exit_code()),
            _ => (base, base.exit_code()),
        },
        ApiContext::Preview | ApiContext::Recover | ApiContext::Inspect => {
            (base, base.exit_code())
        }
    };
    let mut api_error = ApiError::from_error(error);
    api_error.code = code;
    api_error.retryable = code.retryable();
    CliError::api(ApiFailure {
        error: api_error,
        exit,
    })
}

fn api_call<T>(
    context: ApiContext,
    result: agenthub_core::Result<T>,
) -> CliResult<T> {
    result.map_err(|error| map_api_error(context, &error))
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            let CliError { message, api } = error;
            if let Some(failure) = api {
                if json {
                    let envelope = ApiResult::<Value>::failure(failure.error);
                    match serde_json::to_string_pretty(&envelope) {
                        Ok(text) => eprintln!("{text}"),
                        Err(_) => eprintln!(
                            "{{\"ok\":false,\"error\":{{\"code\":\"INTERNAL\",\"message\":\"cannot serialize error\",\"retryable\":false}}}}"
                        ),
                    }
                } else {
                    eprintln!("error: {message}");
                }
                return std::process::ExitCode::from(failure.exit);
            }
            if json {
                let payload = json!({ "ok": false, "error": message });
                eprintln!(
                    "{}",
                    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"ok\":false}".to_string())
                );
            } else {
                eprintln!("error: {message}");
            }
            std::process::ExitCode::from(EXIT_FAIL)
        }
    }
}

fn run(cli: Cli) -> CliResult<u8> {
    // Restore must own the AgentHub instance because it replaces the database
    // file; it therefore runs before the shared context is created.
    if let Commands::Backup {
        command: BackupCommand::Restore(args),
    } = &cli.command
    {
        return cmd_backup_restore(cli.home.as_deref(), args, cli.json);
    }
    let hub = open_hub(cli.home.as_deref())?;
    let ctx = Ctx {
        hub,
        json: cli.json,
    };
    match cli.command {
        Commands::Doctor => cmd_doctor(&ctx),
        Commands::Diagnostics { path } => cmd_diagnostics(&ctx, path.as_deref()),
        Commands::Agents { command } => match command {
            AgentsCommand::List => cmd_agents_list(&ctx),
            AgentsCommand::Inspect(args) => cmd_agents_inspect(&ctx, args),
        },
        Commands::Skills { command } => match command {
            SkillsCommand::List => cmd_skills_list(&ctx),
            SkillsCommand::Scan => cmd_skills_scan(&ctx),
            SkillsCommand::Import(args) => cmd_skills_import(&ctx, args),
            SkillsCommand::Install(args) => cmd_skills_install(&ctx, args),
            SkillsCommand::Uninstall(args) => cmd_skills_uninstall(&ctx, args),
            SkillsCommand::Sync(args) => cmd_skills_sync(&ctx, args),
            SkillsCommand::Rollback(args) => cmd_skills_rollback(&ctx, args),
            SkillsCommand::Audit(args) => cmd_skills_audit(&ctx, args),
            SkillsCommand::Deps(args) => cmd_skills_deps(&ctx, args),
            SkillsCommand::Impact(args) => cmd_skills_impact(&ctx, args),
            SkillsCommand::Validate(args) => cmd_skills_validate(&ctx, args),
            SkillsCommand::Adopt(args) => cmd_skills_adopt(&ctx, args),
        },
        Commands::Mcp { command } => match command {
            McpCommand::List => cmd_mcp_list(&ctx),
            McpCommand::Discover(args) => cmd_mcp_discover(&ctx, args),
            McpCommand::Distribute(args) => cmd_mcp_distribute(&ctx, args),
        },
        Commands::Operations { command } => match command {
            OperationsCommand::List(args) => cmd_operations_list(&ctx, args),
            OperationsCommand::Preview(args) => cmd_operations_preview(&ctx, args),
            OperationsCommand::Approve(args) => cmd_operations_approve(&ctx, args),
            OperationsCommand::Apply(args) => cmd_operations_apply(&ctx, args),
            OperationsCommand::Recover(args) => cmd_operations_recover(&ctx, args),
        },
        Commands::Profiles { command } => match command {
            ProfilesCommand::List => cmd_profiles_list(&ctx),
        },
        Commands::Health(args) => cmd_health(&ctx, args),
        Commands::Backup { command } => match command {
            BackupCommand::Create => cmd_backup_create(&ctx),
            // Handled before the shared context is created (needs &mut AgentHub).
            BackupCommand::Restore(_) => unreachable!("restore is dispatched before context creation"),
        },
        Commands::Backups { command } => match command {
            BackupsCommand::List => cmd_backups_list(&ctx),
        },
        Commands::Stack { command } => match command {
            StackCommand::List => cmd_stack_list(&ctx),
            StackCommand::Show { id } => cmd_stack_show(&ctx, &id),
            StackCommand::Add(args) => cmd_stack_add(&ctx, &args),
            StackCommand::Edit(args) => cmd_stack_edit(&ctx, &args),
            StackCommand::Confirm { id } => cmd_stack_confirm(&ctx, &id),
            StackCommand::Merge {
                child_id,
                into,
                yes,
            } => cmd_stack_merge(&ctx, &child_id, &into, yes),
            StackCommand::Delete { id, yes } => cmd_stack_delete(&ctx, &id, yes),
        },
        Commands::Sub { command } => match command {
            SubCommand::Add(args) => cmd_sub_add(&ctx, &args),
            SubCommand::Edit(args) => cmd_sub_edit(&ctx, &args),
            SubCommand::Delete { id, yes } => cmd_sub_delete(&ctx, &id, yes),
        },
        Commands::Cost { command } => match command {
            CostCommand::Report => cmd_cost_report(&ctx),
            CostCommand::Simulate {
                cancel,
                reduce,
                replace,
                deactivate,
            } => cmd_cost_simulate(&ctx, &cancel, &reduce, &replace, &deactivate),
            CostCommand::Settings => cmd_cost_settings(&ctx),
            CostCommand::Set { threshold, window } => cmd_cost_set(&ctx, &threshold, window),
        },
    }
}

fn open_hub(home: Option<&Path>) -> CliResult<AgentHub> {
    let config = match home {
        Some(home) => CoreConfig::for_home(home),
        None => CoreConfig::resolve()?,
    };
    Ok(AgentHub::open(config)?)
}

struct Ctx {
    hub: AgentHub,
    json: bool,
}

impl Ctx {
    fn emit<T: Serialize>(&self, value: &T) -> CliResult<()> {
        if self.json {
            print_json(&serde_json::to_value(value)?);
        }
        Ok(())
    }

    fn emit_value(&self, value: Value) {
        if self.json {
            print_json(&value);
        }
    }

    /// Emit the v3.1 success envelope (`{ok,data,requestId}`) in JSON mode.
    /// Returns the generated request id so human output can reference it too.
    fn emit_api_success<T: Serialize>(&self, data: T) -> CliResult<String> {
        let envelope = ApiResult::success(data);
        let request_id = envelope.request_id.clone();
        if self.json {
            let value = serde_json::to_value(&envelope)?;
            print_json(&value);
        }
        Ok(request_id)
    }
}

fn print_json(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("error: cannot serialize JSON output: {error}"),
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Exact minor→major decimal rendering with integer math only (no floats):
/// `format_major(2050, 2)` → `"20.50"`, `format_major(100, 0)` → `"100"`.
/// Negative values are impossible for stored amounts (> 0, validated).
fn format_major(minor: i64, exponent: u8) -> String {
    if exponent == 0 {
        return minor.to_string();
    }
    let denominator = 10i64.pow(exponent as u32);
    let major = minor / denominator;
    let fraction = minor % denominator;
    let digits = format!("{fraction:0width$}", width = exponent as usize);
    format!("{major}.{digits}")
}

fn hash_prefix(hash: Option<&str>) -> String {
    match hash {
        Some(hash) if !hash.is_empty() => hash.chars().take(12).collect(),
        _ => "-".to_string(),
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn enum_label<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn display_target(item: &PlanItem) -> &str {
    if item.target_path.is_empty() {
        "(blocked: no writable target)"
    } else {
        &item.target_path
    }
}

fn print_plan(plan: &OperationPlan) {
    println!(
        "Plan {}  kind={}  dry-run={}",
        short_id(&plan.operation_id),
        plan.kind.as_str(),
        plan.dry_run
    );
    let summary = &plan.summary;
    println!(
        "  summary: create={} replace={} noop={} remove={} conflict={} blocked={}",
        summary.create,
        summary.replace,
        summary.noop,
        summary.remove,
        summary.conflict,
        summary.blocked
    );
    for item in &plan.items {
        println!(
            "  {:<8} agent={:<12} strategy={:<7} target={}",
            item.action.as_str(),
            item.agent_key,
            item.strategy.as_str(),
            display_target(item)
        );
        println!(
            "           {} v{} hash {}{}",
            item.capability_name,
            item.version_label.as_deref().unwrap_or("-"),
            hash_prefix(item.desired_hash.as_deref()),
            if item.current_hash.is_some() {
                format!(" (current {})", hash_prefix(item.current_hash.as_deref()))
            } else {
                String::new()
            }
        );
        if let Some(diff) = &item.diff {
            println!("           note: {diff}");
        }
        for warning in &item.warnings {
            println!("           warning: {warning}");
        }
    }
    for warning in &plan.warnings {
        println!("  warning: {warning}");
    }
    for restart in &plan.requires_restart_agents {
        println!("  restart required: {restart}");
    }
}

fn print_result(result: &OperationResult) {
    println!(
        "Result {}  status={}",
        short_id(&result.operation_id),
        result.status.as_str()
    );
    for item in &result.items {
        println!(
            "  {:<8} agent={:<12} target={}",
            item.status, item.agent_key, item.target_path
        );
        if !item.message.is_empty() {
            println!("           {}", item.message);
        }
    }
    for diagnostic in &result.diagnostics {
        println!("  diagnostic: {diagnostic}");
    }
}

/// Exit code implied by a plan: 2 when it contains blocked or conflict items.
fn plan_exit_code(plan: &OperationPlan) -> u8 {
    if plan.summary.blocked > 0 || plan.summary.conflict > 0 {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    }
}

/// Exit code implied by an applied result.
fn result_exit_code(result: &OperationResult) -> u8 {
    match result.status {
        OperationStatus::Success => EXIT_OK,
        OperationStatus::Partial => EXIT_PARTIAL,
        OperationStatus::Failed => {
            if result.items.iter().any(|item| item.status == "success") {
                EXIT_PARTIAL
            } else {
                EXIT_FAIL
            }
        }
        _ => EXIT_FAIL,
    }
}

fn plan_blocked(plan: &OperationPlan) -> bool {
    plan.summary.blocked > 0
}

// ---------------------------------------------------------------------------
// Shared plan/apply flow
// ---------------------------------------------------------------------------

/// Print a plan and decide whether the caller may continue to apply it.
///
/// Returns `Some(exit_code)` when the command must stop (dry-run, missing
/// confirmation, blocked/conflict plan without --force).
fn gate_plan(
    plan: &OperationPlan,
    dry_run: bool,
    yes: bool,
    force: bool,
    json: bool,
) -> Option<u8> {
    if dry_run {
        if !json {
            println!("\nDry run: nothing was written.");
        }
        return Some(plan_exit_code(plan));
    }
    if !yes {
        if !json {
            println!("\nPlan only. Re-run with --yes to apply it.");
        }
        return Some(EXIT_PARTIAL);
    }
    if plan_blocked(plan) {
        if !json {
            println!("\nNothing was applied: the plan contains blocked items (see warnings above).");
        }
        return Some(EXIT_PARTIAL);
    }
    if plan_exit_code(plan) == EXIT_PARTIAL && !force {
        if !json {
            println!(
                "\nNothing was applied: the plan contains conflict items. Re-run with --force to overwrite content AgentHub does not own."
            );
        }
        return Some(EXIT_PARTIAL);
    }
    None
}

// ---------------------------------------------------------------------------
// Agent resolution helpers
// ---------------------------------------------------------------------------

fn resolve_agent_ids(hub: &AgentHub, keys: &[String]) -> CliResult<Vec<String>> {
    let agents = hub.list_agents()?;
    let mut ids: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for key in keys {
        let value = key.trim();
        if value.is_empty() {
            continue;
        }
        match agents
            .iter()
            .find(|agent| agent.key == value || agent.id == value)
        {
            Some(agent) => {
                if !ids.contains(&agent.id) {
                    ids.push(agent.id.clone());
                }
            }
            None => unknown.push(value.to_string()),
        }
    }
    if !unknown.is_empty() {
        let available: Vec<&str> = agents.iter().map(|agent| agent.key.as_str()).collect();
        return Err(CliError::legacy(format!(
            "unknown agent key(s): {}; available agents: {}",
            unknown.join(", "),
            available.join(", ")
        )));
    }
    if ids.is_empty() {
        return Err(CliError::legacy("no agent keys were given".to_string()));
    }
    Ok(ids)
}

fn resolve_skill_id(hub: &AgentHub, wanted: &str) -> CliResult<String> {
    let skills = hub.list_skills()?;
    for skill in &skills {
        if skill.id == wanted || skill.slug == wanted {
            return Ok(skill.id.clone());
        }
    }
    let available: Vec<&str> = skills.iter().map(|skill| skill.slug.as_str()).collect();
    if available.is_empty() {
        Err(CliError::legacy(format!(
            "skill '{wanted}' does not exist; the library is empty (import a skill first)"
        )))
    } else {
        Err(CliError::legacy(format!(
            "skill '{wanted}' does not exist; available skills: {}",
            available.join(", ")
        )))
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

fn cmd_diagnostics(ctx: &Ctx, path: Option<&Path>) -> CliResult<u8> {
    let out_path = match path {
        Some(path) => path.to_path_buf(),
        None => {
            let base = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| CliError::legacy("cannot resolve a home directory"))?;
            base.join(format!("agenthub-diagnostics-{}.json", agenthub_core::util::now().replace(':', "-")))
        }
    };
    let export = ctx.hub.export_diagnostics(&out_path)?;
    if ctx.json {
        ctx.emit(&export)?;
    } else {
        println!(
            "Diagnostics written to {} ({} bytes)\nContains no secrets: doctor checks, agent list, masked MCP servers, dashboard stats.",
            export.path.display(),
            export.bytes
        );
    }
    Ok(0)
}

fn cmd_doctor(ctx: &Ctx) -> CliResult<u8> {
    let report = ctx.hub.doctor()?;
    if ctx.json {
        ctx.emit(&report)?;
    } else {
        println!("AgentHub doctor  version {}", report.version);
        println!("  home:         {}", report.home);
        println!("  library root: {}", report.library_root);
        println!("  database:     {}", report.database_path);
        println!("Checks:");
        for check in &report.checks {
            println!("  [{}] {}: {}", check.status, check.name, check.detail);
            if let Some(hint) = &check.hint {
                println!("        hint: {hint}");
            }
        }
        println!("Agents:");
        for agent in &report.agents {
            println!(
                "  {:<12} {:<22} {:<20} roots={} mcp={}",
                agent.key,
                agent.name,
                enum_label(&agent.status),
                agent.skill_roots.len(),
                agent.mcp_targets.len()
            );
            for evidence in &agent.install_evidence {
                println!("               evidence: {evidence}");
            }
        }
        println!("ok: {}", report.ok);
    }
    Ok(if report.ok { EXIT_OK } else { EXIT_FAIL })
}

// ---------------------------------------------------------------------------
// agents
// ---------------------------------------------------------------------------

fn cmd_agents_list(ctx: &Ctx) -> CliResult<u8> {
    let agents = ctx.hub.list_agents()?;
    if ctx.json {
        ctx.emit(&agents)?;
    } else {
        for agent in &agents {
            println!(
                "{:<12} {:<24} {:<20} skills={} installed={} symlink={}",
                agent.key,
                agent.name,
                enum_label(&agent.status),
                agent.skill_count,
                agent.installed_skill_count,
                enum_label(&agent.symlink_support)
            );
            for root in &agent.skill_roots {
                println!(
                    "  root  {}  scope={} exists={} writable={} skills={}",
                    root.path,
                    enum_label(&root.scope),
                    root.exists,
                    root.writable,
                    root.skill_count
                );
            }
            for target in &agent.mcp_targets {
                println!(
                    "  mcp   {}  format={} exists={} writable={}",
                    target.path, target.format, target.exists, target.writable
                );
            }
        }
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// agents inspect (v3.1)
// ---------------------------------------------------------------------------

fn cmd_agents_inspect(ctx: &Ctx, args: AgentInspectArgs) -> CliResult<u8> {
    let agents = api_call(ApiContext::Inspect, ctx.hub.list_agents())?;
    let wanted = args.agent_key.trim();
    let agent = agents
        .iter()
        .find(|agent| agent.key == wanted || agent.id == wanted)
        .ok_or_else(|| {
            let available: Vec<&str> = agents.iter().map(|agent| agent.key.as_str()).collect();
            CliError::invalid_input(format!(
                "unknown agent '{}'; available agents: {}",
                wanted,
                available.join(", ")
            ))
        })?;
    let inspection = api_call(ApiContext::Inspect, ctx.hub.agent_inspect(&agent.id))?;
    let request_id = ctx.emit_api_success(&inspection)?;
    if !ctx.json {
        println!(
            "Agent {} ({})  id={}",
            inspection.agent_key, agent.name, inspection.agent_id
        );
        println!("  inspected at: {}", inspection.inspected_at);
        if inspection.capabilities.is_empty() {
            println!("  no adapter capability evidence recorded yet");
        }
        for capability in &inspection.capabilities {
            println!(
                "  {:<28} support={:<22} evidence={}",
                capability.feature_key, capability.support_state, capability.evidence_level
            );
            if let Some(note) = &capability.notes {
                println!("      note: {note}");
            }
        }
        println!("  request id: {request_id}");
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// skills list / scan
// ---------------------------------------------------------------------------

fn cmd_skills_list(ctx: &Ctx) -> CliResult<u8> {
    let skills = ctx.hub.list_skills()?;
    if ctx.json {
        ctx.emit(&skills)?;
    } else if skills.is_empty() {
        println!("No skills in the library yet. Import one with `agenthub skills import --path <dir>`.");
    } else {
        for skill in &skills {
            println!(
                "{:<28} {:<16} hash={} installs={} drift={} update={}",
                skill.name,
                skill.version,
                hash_prefix(Some(&skill.content_hash)),
                skill.installations.len(),
                skill.drift_count,
                skill.update_available
            );
            println!("  id={} slug={}", skill.id, skill.slug);
            println!(
                "  files={} size={} bytes source={}",
                skill.file_count,
                skill.size_bytes,
                skill
                    .source
                    .as_ref()
                    .map(|source| format!("{} ({})", source.uri, source.source_type))
                    .unwrap_or_else(|| "-".to_string())
            );
            for installation in &skill.installations {
                println!(
                    "  deployed: agent={} state={} target={} hash={}",
                    installation.agent_key,
                    enum_label(&installation.state),
                    installation.target_path,
                    hash_prefix(installation.content_hash.as_deref())
                );
            }
        }
    }
    Ok(EXIT_OK)
}

fn cmd_skills_scan(ctx: &Ctx) -> CliResult<u8> {
    let report = ctx.hub.scan()?;
    if ctx.json {
        ctx.emit(&report)?;
    } else {
        println!("Scan at {}", report.scanned_at);
        println!(
            "  total skills: {}  duplicates: {}  conflicts: {}  invalid: {}",
            report.total_skills, report.duplicates, report.conflicts, report.invalid
        );
        for agent in &report.agents {
            println!(
                "{} ({}) — {}",
                agent.agent_key,
                enum_label(&agent.status),
                agent.agent_name
            );
            for root in &agent.roots {
                println!(
                    "  root [{}] {}  discovered={} invalid={}",
                    if root.exists { "exists" } else { "missing" },
                    root.path,
                    root.discovered,
                    root.invalid
                );
                for error in &root.errors {
                    println!("    error: {error}");
                }
            }
            for skill in &agent.skills {
                println!(
                    "  skill {:<24} agent={:<12} valid={} hash={} path={}",
                    skill.name,
                    skill.agent_key.as_deref().unwrap_or("-"),
                    skill.valid,
                    hash_prefix(Some(&skill.content_hash)),
                    skill.path
                );
                for issue in &skill.issues {
                    println!("    issue: {issue}");
                }
            }
            for error in &agent.errors {
                println!("  error: {error}");
            }
        }
        for warning in &report.warnings {
            println!("warning: {warning}");
        }
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// skills import
// ---------------------------------------------------------------------------

fn cmd_skills_import(ctx: &Ctx, args: ImportArgs) -> CliResult<u8> {
    let source = import_source(&args)?;
    let result = ctx.hub.import_skill(ImportRequest {
        source,
        skill_path: args.skill_path.clone(),
        version_label: args.version_label.clone(),
        notes: args.notes.clone(),
    })?;
    if ctx.json {
        ctx.emit(&result)?;
    } else {
        println!(
            "{} skill '{}' ({})",
            if result.created { "Imported" } else { "Already present" },
            result.skill.name,
            result.skill.slug
        );
        println!("  id:         {}", result.skill.id);
        println!("  version:    {}", result.version.version_label);
        println!(
            "  hash:       {} ({} files, {} bytes)",
            result.skill.content_hash, result.skill.file_count, result.skill.size_bytes
        );
        println!("  storage:    {}", result.version.storage_path);
        for warning in &result.warnings {
            println!("  warning: {warning}");
        }
    }
    Ok(EXIT_OK)
}

fn import_source(args: &ImportArgs) -> CliResult<ImportSource> {
    if let Some(url) = &args.git {
        if url.trim().is_empty() {
            return Err(CliError::legacy("--git requires a non-empty URL".to_string()));
        }
        return Ok(ImportSource::Git {
            url: url.clone(),
            reference: args.reference.clone(),
        });
    }
    let Some(path) = &args.path else {
        return Err(CliError::legacy(
            "nothing to import: pass --path <dir|zip|SKILL.md> or --git <url>".to_string(),
        ));
    };
    if !path.exists() {
        return Err(CliError::legacy(format!(
            "import path does not exist: {}",
            path.display()
        )));
    }
    if path.is_dir() {
        return Ok(ImportSource::Folder {
            path: path.to_string_lossy().to_string(),
        });
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        Ok(ImportSource::Archive {
            path: path.to_string_lossy().to_string(),
        })
    } else if name == "skill.md" || name.ends_with(".md") {
        Ok(ImportSource::SkillMd {
            path: path.to_string_lossy().to_string(),
        })
    } else {
        Err(CliError::legacy(format!(
            "unsupported import path (expected a folder, .zip or SKILL.md): {}",
            path.display()
        )))
    }
}

// ---------------------------------------------------------------------------
// skills install
// ---------------------------------------------------------------------------

fn cmd_skills_install(ctx: &Ctx, args: InstallArgs) -> CliResult<u8> {
    let agent_ids = resolve_agent_ids(&ctx.hub, &args.agents)?;
    let capability_id = resolve_skill_id(&ctx.hub, &args.skill_id)?;
    let plan = ctx.hub.plan_install(InstallRequest {
        capability_id,
        version_id: None,
        agent_ids,
        strategy: Some(args.strategy.into()),
        force: args.force,
    })?;
    match gate_plan(&plan, args.dry_run, args.yes, args.force, ctx.json) {
        Some(code) => {
            emit_plan_only(ctx, &plan)?;
            Ok(code)
        }
        None => {
            let result = ctx
                .hub
                .apply_operation(&plan.operation_id, args.force)?;
            if ctx.json {
                ctx.emit_value(json!({
                    "applied": true,
                    "plan": serde_json::to_value(&plan)?,
                    "result": serde_json::to_value(&result)?,
                }));
            } else {
                print_plan(&plan);
                println!();
                print_result(&result);
            }
            Ok(result_exit_code(&result))
        }
    }
}

fn cmd_skills_uninstall(ctx: &Ctx, args: UninstallArgs) -> CliResult<u8> {
    let plan = ctx.hub.plan_uninstall(UninstallRequest {
        installation_ids: args.installation_ids.clone(),
        force: args.force,
    })?;
    match gate_plan(&plan, args.dry_run, args.yes, args.force, ctx.json) {
        Some(code) => {
            emit_plan_only(ctx, &plan)?;
            Ok(code)
        }
        None => {
            let result = ctx.hub.apply_operation(&plan.operation_id, args.force)?;
            if ctx.json {
                ctx.emit_value(json!({
                    "applied": true,
                    "plan": serde_json::to_value(&plan)?,
                    "result": serde_json::to_value(&result)?,
                }));
            } else {
                print_plan(&plan);
                println!();
                print_result(&result);
            }
            Ok(result_exit_code(&result))
        }
    }
}

// ---------------------------------------------------------------------------
// capability health / audit / dependencies / impact / validation
// ---------------------------------------------------------------------------

fn cmd_health(ctx: &Ctx, args: HealthArgs) -> CliResult<u8> {
    if args.agents || args.record {
        let report = ctx.hub.agent_health_report(args.record)?;
        if ctx.json {
            ctx.emit_value(json!({ "checkedAt": report.checked_at, "agents": report.agents }));
        } else {
            println!("Agent health at {}", report.checked_at);
            for agent in &report.agents {
                println!(
                    "  {:<12} {:<22} version={:<14} changed={} pathsOk={}",
                    agent.agent_key,
                    agent.agent_name,
                    agent.version.clone().unwrap_or_else(|| "-".to_string()),
                    agent.version_changed,
                    agent.paths_ok
                );
                for note in &agent.notes {
                    println!("    note: {note}");
                }
            }
        }
        return Ok(EXIT_OK);
    }
    let overview = ctx.hub.health_overview()?;
    if ctx.json {
        ctx.emit_value(serde_json::to_value(&overview)?);
    } else {
        let summary = &overview.summary;
        println!(
            "Capability health: {} total | verified {} | unverified {} | needs attention {} | failed {} | not deployed {}",
            summary.total,
            summary.verified,
            summary.unverified,
            summary.needs_attention,
            summary.failed,
            summary.not_deployed
        );
        if let Some(checked_at) = &summary.last_checked_at {
            println!("  last check: {checked_at}");
        }
        for capability in &overview.capabilities {
            println!(
                "  {:<28} {:<22} {:<16} {}",
                capability.slug,
                capability.version_label,
                capability.status.as_str(),
                capability.reasons.first().cloned().unwrap_or_default()
            );
        }
    }
    Ok(EXIT_OK)
}

fn cmd_skills_audit(ctx: &Ctx, args: AuditArgs) -> CliResult<u8> {
    let capability_id = resolve_skill_id(&ctx.hub, &args.skill)?;
    let audit = ctx.hub.audit_skill(AuditRequest {
        capability_id,
        version_id: args.version,
        kind: Some("static".to_string()),
    })?;
    if ctx.json {
        ctx.emit_value(serde_json::to_value(&audit)?);
    } else {
        println!(
            "Static audit: status={} risk={} version={}",
            audit.status, audit.risk, audit.version_label
        );
        println!("  {}", audit.summary);
        for finding in &audit.findings {
            println!(
                "  [{}] {} ({}) {}",
                finding.severity, finding.id, finding.category, finding.title
            );
            if !finding.detail.is_empty() {
                println!("      {}", finding.detail);
            }
            if let Some(path) = &finding.path {
                println!("      path: {path}");
            }
        }
    }
    Ok(if audit.status == "fail" { EXIT_PARTIAL } else { EXIT_OK })
}

fn cmd_skills_deps(ctx: &Ctx, args: AuditArgs) -> CliResult<u8> {
    let capability_id = resolve_skill_id(&ctx.hub, &args.skill)?;
    let report = ctx.hub.skill_dependencies(AuditRequest {
        capability_id,
        version_id: args.version,
        kind: Some("environment".to_string()),
    })?;
    if ctx.json {
        ctx.emit_value(serde_json::to_value(&report)?);
    } else {
        println!(
            "Dependencies: status={} risk={}",
            report.status, report.risk
        );
        println!("  {}", report.summary);
        for item in &report.items {
            println!(
                "  {:<10} {:<20} {:<10} found={} — {}",
                item.kind,
                item.name,
                item.status,
                item.found.clone().unwrap_or_else(|| "-".to_string()),
                item.detail
            );
            if let Some(suggestion) = &item.suggestion {
                println!("      suggestion: {suggestion}");
            }
        }
    }
    Ok(if report.status == "needs_attention" {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    })
}

fn cmd_skills_impact(ctx: &Ctx, args: ImpactArgs) -> CliResult<u8> {
    let capability_id = resolve_skill_id(&ctx.hub, &args.skill)?;
    let skills = ctx.hub.list_skills()?;
    let skill = skills
        .iter()
        .find(|skill| skill.id == capability_id)
        .ok_or_else(|| CliError::legacy("skill disappeared".to_string()))?;
    let to_version = match args.to_version {
        Some(version) => version,
        None => skill.current_version_id.clone(),
    };
    let impact = ctx.hub.preview_update(ImpactRequest {
        capability_id,
        from_version_id: args.from_version,
        to_version_id: to_version,
    })?;
    if ctx.json {
        ctx.emit_value(serde_json::to_value(&impact)?);
    } else {
        println!(
            "Update impact: {} -> {} (risk {})",
            impact.from_version_label, impact.to_version_label, impact.risk
        );
        println!("  {}", impact.summary);
        println!(
            "  affected installations: {} across agents [{}]",
            impact.affected_installation_count,
            impact.affected_agent_keys.join(", ")
        );
        if !impact.affected_project_roots.is_empty() {
            println!(
                "  affected project roots: {}",
                impact.affected_project_roots.join(", ")
            );
        }
        println!(
            "  classes: {}",
            impact.classes.join(", ")
        );
        println!(
            "  revalidation required: {}",
            impact.requires_revalidation
        );
        for change in impact.file_changes.iter().take(40) {
            println!(
                "  {:<8} {:<60} {} -> {} bytes",
                change.change, change.path, change.size_before, change.size_after
            );
        }
        for finding in &impact.findings {
            println!("  [{}] {} {}", finding.severity, finding.id, finding.title);
        }
    }
    Ok(EXIT_OK)
}

fn cmd_skills_adopt(ctx: &Ctx, args: AdoptArgs) -> CliResult<u8> {
    let agent_ids = if args.agents.is_empty() {
        None
    } else {
        Some(resolve_agent_ids(&ctx.hub, &args.agents)?)
    };
    let preview = args.dry_run || !args.yes;
    let result = ctx.hub.import_scan_skills(ScanImportRequest {
        agent_ids,
        paths: None,
        include_read_only_sources: args.include_read_only,
        max_items: Some(args.max),
        dry_run: preview,
    })?;
    if ctx.json {
        ctx.emit_value(json!({
            "applied": !preview,
            "result": serde_json::to_value(&result)?,
        }));
    } else {
        println!(
            "{} {} candidate(s): adopt {}, already in library {}, duplicates {}, read-only skipped {}, limit skipped {}, failed {}",
            if preview { "Would adopt from" } else { "Adopted from" },
            result.total_candidates,
            result.imported_count,
            result.skipped_existing,
            result.skipped_duplicate,
            result.skipped_read_only,
            result.skipped_limit,
            result.failed.len()
        );
        for item in result.imported.iter().take(50) {
            println!(
                "  {:<28} {:<14} from {:<12} {}",
                item.slug,
                item.version_label,
                item.agent_key.clone().unwrap_or_else(|| "-".to_string()),
                item.path
            );
        }
        if result.imported.len() > 50 {
            println!("  ... and {} more", result.imported.len() - 50);
        }
        for failure in result.failed.iter().take(20) {
            println!("  FAILED {}: {}", failure.path, failure.error);
        }
        if preview {
            println!("Dry run: nothing was written. Re-run with --yes to adopt these skills.");
        }
    }
    Ok(if result.failed.is_empty() {
        if preview { EXIT_PARTIAL } else { EXIT_OK }
    } else {
        EXIT_PARTIAL
    })
}

fn cmd_skills_validate(ctx: &Ctx, args: ValidateArgs) -> CliResult<u8> {
    let capability_id = resolve_skill_id(&ctx.hub, &args.skill)?;
    let agent_id = match args.agent {
        Some(agent) => resolve_agent_ids(&ctx.hub, &[agent])?.into_iter().next(),
        None => None,
    };
    let run = ctx.hub.record_validation(ValidationRequest {
        capability_id,
        version_id: args.version,
        agent_id,
        kind: args.kind,
        status: args.status,
        case_name: args.case_name,
        notes: args.notes,
        evidence: args.evidence,
    })?;
    if ctx.json {
        ctx.emit_value(serde_json::to_value(&run)?);
    } else {
        println!(
            "Recorded validation: {} on {} (agent={}, version={}, case={})",
            run.status,
            run.agent_name.clone().unwrap_or_else(|| "agent-neutral".to_string()),
            run.agent_key.clone().unwrap_or_else(|| "-".to_string()),
            run.version_label,
            run.case_name
        );
    }
    Ok(EXIT_OK)
}

fn emit_plan_only(ctx: &Ctx, plan: &OperationPlan) -> CliResult<()> {
    if ctx.json {
        ctx.emit_value(json!({
            "applied": false,
            "plan": serde_json::to_value(plan)?,
        }));
    } else {
        print_plan(plan);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// skills sync
// ---------------------------------------------------------------------------

fn cmd_skills_sync(ctx: &Ctx, args: SyncArgs) -> CliResult<u8> {
    let changes = ctx.hub.check_changes()?;
    let mut plans: Vec<OperationPlan> = Vec::new();
    for skill in &changes.updates {
        let stale_agents: Vec<String> = skill
            .installations
            .iter()
            .filter(|installation| installation.version_id != skill.current_version_id)
            .map(|installation| installation.agent_id.clone())
            .collect();
        if stale_agents.is_empty() {
            continue;
        }
        let plan = ctx.hub.plan_install(InstallRequest {
            capability_id: skill.id.clone(),
            version_id: Some(skill.current_version_id.clone()),
            agent_ids: stale_agents,
            strategy: None,
            force: args.force,
        })?;
        plans.push(plan);
    }
    let plan_conflict = plans
        .iter()
        .any(|plan| plan.summary.blocked > 0 || plan.summary.conflict > 0);
    let check_dirty = !changes.drifted.is_empty() || !changes.missing.is_empty();
    let partial = plan_conflict || check_dirty;

    if !ctx.json {
        println!("Change check at {}", changes.checked_at);
        println!(
            "  drifted: {}  missing: {}  updates: {}",
            changes.drifted.len(),
            changes.missing.len(),
            changes.updates.len()
        );
        for installation in &changes.drifted {
            println!(
                "  drift: agent={} target={} (recorded hash {})",
                installation.agent_key,
                installation.target_path,
                hash_prefix(installation.content_hash.as_deref())
            );
        }
        for installation in &changes.missing {
            println!(
                "  missing: agent={} target={}",
                installation.agent_key, installation.target_path
            );
        }
        for warning in &changes.warnings {
            println!("  warning: {warning}");
        }
    }

    if args.dry_run || !args.yes {
        if !ctx.json {
            for plan in &plans {
                print_plan(plan);
            }
        }
        if ctx.json {
            ctx.emit_value(json!({
                "applied": false,
                "check": serde_json::to_value(&changes)?,
                "plans": plans.iter().map(serde_json::to_value).collect::<Result<Vec<_>, _>>()?,
            }));
        }
        if args.dry_run {
            if !ctx.json {
                println!("\nDry run: nothing was written.");
            }
            return Ok(if partial { EXIT_PARTIAL } else { EXIT_OK });
        }
        if !ctx.json {
            println!("\nPlan only. Re-run with --yes to apply the generated plans.");
        }
        return Ok(EXIT_PARTIAL);
    }

    if plans.is_empty() {
        if ctx.json {
            ctx.emit_value(json!({
                "applied": false,
                "check": serde_json::to_value(&changes)?,
                "plans": Vec::<Value>::new(),
            }));
        } else {
            println!("\nNothing to sync.");
        }
        return Ok(if partial { EXIT_PARTIAL } else { EXIT_OK });
    }

    let mut results: Vec<OperationResult> = Vec::new();
    let mut exit = EXIT_OK;
    let mut applied_plans: Vec<OperationPlan> = Vec::new();
    for plan in plans {
        if plan_blocked(&plan) || (plan_exit_code(&plan) == EXIT_PARTIAL && !args.force) {
            if !ctx.json {
                print_plan(&plan);
                println!(
                    "Skipped {}: plan contains blocked/conflict items (use --force to overwrite conflicts).",
                    short_id(&plan.operation_id)
                );
            }
            exit = EXIT_PARTIAL;
            applied_plans.push(plan);
            continue;
        }
        let result = ctx.hub.apply_operation(&plan.operation_id, args.force)?;
        if !ctx.json {
            print_plan(&plan);
            println!();
            print_result(&result);
        }
        let code = result_exit_code(&result);
        if code != EXIT_OK {
            exit = code;
        }
        applied_plans.push(plan);
        results.push(result);
    }
    if ctx.json {
        ctx.emit_value(json!({
            "applied": true,
            "check": serde_json::to_value(&changes)?,
            "plans": applied_plans.iter().map(serde_json::to_value).collect::<Result<Vec<_>, _>>()?,
            "results": results.iter().map(serde_json::to_value).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if partial && exit == EXIT_OK {
        exit = EXIT_PARTIAL;
    }
    Ok(exit)
}

// ---------------------------------------------------------------------------
// skills rollback
// ---------------------------------------------------------------------------

fn cmd_skills_rollback(ctx: &Ctx, args: RollbackArgs) -> CliResult<u8> {
    let operation_id = if args.target == "last" {
        ctx.hub
            .list_operations(100)?
            .into_iter()
            .find(|operation| operation.can_rollback)
            .map(|operation| operation.id)
            .ok_or_else(|| CliError::legacy("there is no rollbackable operation".to_string()))?
    } else {
        args.target.clone()
    };
    let operation = ctx.hub.get_operation(&operation_id)?;
    let result = ctx.hub.rollback_operation(&operation_id)?;
    if ctx.json {
        ctx.emit_value(json!({
            "operationId": operation_id,
            "operation": serde_json::to_value(&operation)?,
            "result": serde_json::to_value(&result)?,
        }));
    } else {
        println!(
            "Rolled back operation {} ({})",
            short_id(&operation_id),
            operation.summary
        );
        print_result(&result);
    }
    Ok(result_exit_code(&result))
}

// ---------------------------------------------------------------------------
// mcp list / discover / distribute
// ---------------------------------------------------------------------------

fn cmd_mcp_list(ctx: &Ctx) -> CliResult<u8> {
    let servers = ctx.hub.mcp_servers()?;
    if ctx.json {
        ctx.emit(&servers)?;
    } else if servers.is_empty() {
        println!("No MCP servers known yet. Discover one with `agenthub mcp discover`.");
    } else {
        for server in &servers {
            println!(
                "{:<24} transport={:<16} id={}",
                server.name,
                enum_label(&server.transport),
                server.id
            );
            if let Some(command) = &server.command {
                println!("  command: {command} {}", server.args.join(" "));
            }
            if let Some(url) = &server.url {
                println!("  url: {url}");
            }
            if !server.env_keys.is_empty() {
                println!("  env keys: {}", server.env_keys.join(", "));
            }
            for distribution in &server.distributions {
                println!(
                    "  distributed: agent={} state={} target={} key={}",
                    distribution.agent_key,
                    enum_label(&distribution.state),
                    distribution.target_path,
                    distribution.config_key
                );
            }
            for issue in &server.issues {
                println!("  issue: {issue}");
            }
        }
    }
    Ok(EXIT_OK)
}

fn cmd_mcp_discover(ctx: &Ctx, args: DiscoverArgs) -> CliResult<u8> {
    let filter = match &args.agent {
        Some(key) => Some(resolve_agent_ids(&ctx.hub, std::slice::from_ref(key))?),
        None => None,
    };
    let discovered = ctx.hub.mcp_discover(filter)?;
    if ctx.json {
        ctx.emit(&discovered)?;
    } else if discovered.is_empty() {
        println!("No MCP entries found in agent configuration files.");
    } else {
        for entry in &discovered {
            println!(
                "{:<24} agent={:<12} transport={:<16} known={} target={}",
                entry.name,
                entry.agent_key,
                enum_label(&entry.transport),
                entry.already_known,
                entry.target_path
            );
            if let Some(command) = &entry.command {
                println!("  command: {command} {}", entry.args.join(" "));
            }
            if let Some(url) = &entry.url {
                println!("  url: {url}");
            }
            for issue in &entry.issues {
                println!("  issue: {issue}");
            }
        }
    }
    Ok(EXIT_OK)
}

fn cmd_mcp_distribute(ctx: &Ctx, args: McpDistributeArgs) -> CliResult<u8> {
    let agent_ids = resolve_agent_ids(&ctx.hub, &args.agents)?;
    let servers = ctx.hub.mcp_servers()?;
    let server_id = servers
        .iter()
        .find(|server| server.id == args.server_id || server.name == args.server_id)
        .map(|server| server.id.clone())
        .ok_or_else(|| {
            let available: Vec<&str> = servers.iter().map(|server| server.name.as_str()).collect();
            if available.is_empty() {
                CliError::legacy(format!(
                    "MCP server '{}' does not exist; no MCP servers are defined yet",
                    args.server_id
                ))
            } else {
                CliError::legacy(format!(
                    "MCP server '{}' does not exist; available servers: {}",
                    args.server_id,
                    available.join(", ")
                ))
            }
        })?;
    let plan = ctx.hub.mcp_plan_distribute(McpDistributeRequest {
        server_id,
        agent_ids,
        force: args.force,
    })?;
    match gate_plan(&plan, args.dry_run, args.yes, args.force, ctx.json) {
        Some(code) => {
            emit_plan_only(ctx, &plan)?;
            Ok(code)
        }
        None => {
            let result = ctx.hub.apply_operation(&plan.operation_id, args.force)?;
            if ctx.json {
                ctx.emit_value(json!({
                    "applied": true,
                    "plan": serde_json::to_value(&plan)?,
                    "result": serde_json::to_value(&result)?,
                }));
            } else {
                print_plan(&plan);
                println!();
                print_result(&result);
            }
            Ok(result_exit_code(&result))
        }
    }
}

// ---------------------------------------------------------------------------
// operations / profiles / backups
// ---------------------------------------------------------------------------

fn cmd_operations_list(ctx: &Ctx, args: OperationsArgs) -> CliResult<u8> {
    let operations = ctx.hub.list_operations(args.limit)?;
    if ctx.json {
        ctx.emit(&operations)?;
    } else if operations.is_empty() {
        println!("No operations recorded yet.");
    } else {
        for operation in &operations {
            println!(
                "{:<10} {:<14} {:<12} items={} rollbackable={} started={}",
                short_id(&operation.id),
                operation.kind.as_str(),
                operation.status.as_str(),
                operation.item_count,
                operation.can_rollback,
                operation.started_at
            );
            println!("  {}", operation.summary);
            if let Some(error) = &operation.error {
                println!("  error: {error}");
            }
            if let Some(rollback_of) = &operation.rollback_of {
                println!("  rollback of: {}", short_id(rollback_of));
            }
        }
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// operations preview / approve / apply / recover (v3.1)
// ---------------------------------------------------------------------------

/// Success payload of `operations apply`: the shared `OperationResult` DTO
/// flattened together with the plan hash the approval was bound to.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyData<'a> {
    #[serde(flatten)]
    result: &'a OperationResult,
    plan_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency_key: Option<&'a str>,
}

/// Exit code implied by a freshly applied operation.
fn operation_status_exit(status: OperationStatus) -> u8 {
    match status {
        OperationStatus::Success => EXIT_OK,
        OperationStatus::Partial => EXIT_PARTIAL,
        OperationStatus::Failed | OperationStatus::NeedsManualRecovery => EXIT_RUNTIME,
        _ => EXIT_OK,
    }
}

fn cmd_operations_preview(ctx: &Ctx, args: OperationIdArgs) -> CliResult<u8> {
    let view = api_call(
        ApiContext::Preview,
        ctx.hub.preview_operation(&args.id),
    )?;
    ctx.emit_api_success(&view)?;
    if !ctx.json {
        println!(
            "Operation {}  kind={}  status={}",
            view.id,
            enum_label(&view.kind),
            enum_label(&view.status)
        );
        println!("  summary: {}", view.summary);
        println!(
            "  plan hash: {}",
            view.plan_hash.as_deref().unwrap_or("-")
        );
        println!(
            "  approval: {}  approved at: {}  expires at: {}",
            view.approval_state,
            view.approved_at.as_deref().unwrap_or("-"),
            view.expires_at.as_deref().unwrap_or("-")
        );
        if let Some(recovery_state) = &view.recovery_state {
            println!("  recovery state: {recovery_state}");
        }
        if let Some(plan) = &view.plan {
            println!();
            print_plan(plan);
        }
        if !view.steps.is_empty() {
            println!("Journal:");
            for step in &view.steps {
                println!(
                    "  #{:<3} state={:<10} target={} before={} after={}",
                    step.sequence_no,
                    step.state,
                    step.target_path,
                    hash_prefix(step.before_hash.as_deref()),
                    hash_prefix(step.after_hash.as_deref())
                );
                if let Some(code) = &step.error_code {
                    println!("       error code: {code}");
                }
            }
        }
        if view.approval_state == "none" && view.status.as_str() == "planned" {
            println!(
                "\nNext: agenthub operations approve {} --plan-hash {}",
                view.id,
                view.plan_hash.as_deref().unwrap_or("<plan-hash>")
            );
        }
    }
    Ok(EXIT_OK)
}

fn cmd_operations_approve(ctx: &Ctx, args: ApproveArgs) -> CliResult<u8> {
    let grant = api_call(
        ApiContext::Approve,
        ctx.hub.approve_operation(&args.id, &args.plan_hash),
    )?;
    ctx.emit_api_success(&grant)?;
    if !ctx.json {
        println!("Approved operation {}", grant.operation_id);
        println!("  plan hash: {}", grant.plan_hash);
        println!("  expires at: {}", grant.expires_at);
        println!("  approval token (one-time, keep secret):");
        println!("    {}", grant.approval_token);
        println!(
            "\nNext: agenthub operations apply {} --token {}",
            grant.operation_id, grant.approval_token
        );
    }
    Ok(EXIT_OK)
}

fn cmd_operations_apply(ctx: &Ctx, args: ApplyArgs) -> CliResult<u8> {
    let result = api_call(
        ApiContext::Apply,
        ctx.hub.apply_operation_approved(
            &args.id,
            &args.token,
            args.idempotency_key.as_deref(),
        ),
    )?;
    let plan_hash = ctx
        .hub
        .preview_operation(&args.id)
        .ok()
        .and_then(|view| view.plan_hash);
    let exit = operation_status_exit(result.status);
    ctx.emit_api_success(ApplyData {
        result: &result,
        plan_hash,
        idempotency_key: args.idempotency_key.as_deref(),
    })?;
    if !ctx.json {
        print_result(&result);
    }
    Ok(exit)
}

fn cmd_operations_recover(ctx: &Ctx, args: OperationIdArgs) -> CliResult<u8> {
    let report = api_call(
        ApiContext::Recover,
        ctx.hub.recover_operation(&args.id),
    )?;
    let exit = match report.status.as_str() {
        "recovered" => EXIT_OK,
        "needs_manual_recovery" => EXIT_RUNTIME,
        _ => EXIT_PARTIAL,
    };
    ctx.emit_api_success(&report)?;
    if !ctx.json {
        println!(
            "Recovery {}  status={}",
            short_id(&report.operation_id),
            report.status
        );
        for item in &report.items {
            println!(
                "  {:<8} target={} {}",
                item.status, item.target_path, item.message
            );
        }
        for diagnostic in &report.diagnostics {
            println!("  diagnostic: {diagnostic}");
        }
    }
    Ok(exit)
}

fn cmd_profiles_list(ctx: &Ctx) -> CliResult<u8> {
    let profiles = ctx.hub.list_profiles()?;
    if ctx.json {
        ctx.emit(&profiles)?;
    } else {
        for profile in &profiles {
            println!(
                "{:<16} builtin={} items={} id={}",
                profile.name,
                profile.is_builtin,
                profile.items.len(),
                profile.id
            );
            println!("  {}", profile.description);
            for item in &profile.items {
                println!(
                    "  item {} enabled={} targets={}",
                    item.name,
                    item.enabled,
                    item.target_agent_keys.join(",")
                );
            }
        }
    }
    Ok(EXIT_OK)
}

fn cmd_backup_create(ctx: &Ctx) -> CliResult<u8> {
    let backup = ctx.hub.create_backup()?;
    if ctx.json {
        ctx.emit(&backup)?;
    } else {
        println!("Backup {} created at {}", backup.id, backup.path);
        println!("  entries: {}", backup.entries);
    }
    Ok(EXIT_OK)
}

fn cmd_backup_restore(home: Option<&Path>, args: &RestoreArgs, json: bool) -> CliResult<u8> {
    use agenthub_core::Error as CoreError;
    let mut hub = open_hub(home)?;
    match hub.restore_backup(&args.id) {
        Ok(result) => {
            if json {
                let envelope = ApiResult::success(result.clone());
                print_json(&serde_json::to_value(envelope)?);
            } else {
                println!(
                    "Restored {} at {} (schema {}, integrity {}, capabilities {})",
                    result.backup_id,
                    result.restored_at,
                    result.schema_version,
                    result.integrity_ok,
                    result.capability_count
                );
                println!("Safety backup: {}", result.safety_backup_id);
            }
            Ok(EXIT_OK)
        }
        Err(error) => {
            let code = match &error {
                CoreError::InvalidSkill(_) | CoreError::NotFound(_) => ErrorCode::InvalidInput,
                _ => ErrorCode::RollbackBlocked,
            };
            let exit = if code == ErrorCode::InvalidInput {
                EXIT_INVALID
            } else {
                EXIT_RUNTIME
            };
            let mut api_error = ApiError::from_error(&error);
            api_error.code = code;
            api_error.retryable = false;
            Err(CliError::api(ApiFailure {
                error: api_error,
                exit,
            }))
        }
    }
}

fn cmd_backups_list(ctx: &Ctx) -> CliResult<u8> {
    let backups = ctx.hub.list_backups()?;
    if ctx.json {
        ctx.emit(&backups)?;
    } else if backups.is_empty() {
        println!("No backups yet. Create one with `agenthub backup create`.");
    } else {
        for backup in &backups {
            println!(
                "{:<28} size={} bytes created={} path={}",
                backup.id, backup.size_bytes, backup.created_at, backup.path
            );
        }
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// My AI Stack (M1-B)
//
// Reads and writes go straight to the shared core service (D-1): no approval
// plans are produced for stack commands. Deletion and merging are high-impact
// and therefore require `--yes`; the refusal is a usage error, not a backend
// contract error. All JSON output keeps the v3.1 `ApiResult` envelope, and the
// `ResourceConflict` / `InvalidInput` / `NotFound` error classification comes
// from the core untouched (`map_api_error` passthrough via `ApiContext::Inspect`).
// ---------------------------------------------------------------------------

/// Parse a user-entered amount in MAJOR units into integer minor units with
/// integer arithmetic only (no float multiplication). Rejects negatives and
/// values that would not be JSON-safe after folding.
fn parse_amount_major(text: &str, currency: &str) -> CliResult<i64> {
    let exponent = agenthub_core::money::currency_exponent(currency).ok_or_else(|| {
        CliError::invalid_input(format!("unsupported currency code {currency:?}"))
    })?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(CliError::invalid_input("amount must not be empty"));
    }
    if trimmed.starts_with('-') {
        return Err(CliError::invalid_input(
            "amounts must be positive; register unknown amounts by omitting --amount",
        ));
    }
    let (whole, fraction) = match trimmed.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (trimmed, ""),
    };
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CliError::invalid_input(format!(
            "amount {text:?} is not a valid decimal number"
        )));
    }
    if fraction.len() > exponent as usize {
        return Err(CliError::invalid_input(format!(
            "amount {text:?} has more than {exponent} decimal places for {currency}"
        )));
    }
    let mut fraction = fraction.to_string();
    while fraction.len() < exponent as usize {
        fraction.push('0');
    }
    let whole_number: i128 = whole
        .parse()
        .map_err(|_| CliError::invalid_input(format!("amount {text:?} is out of range")))?;
    let minor: i128 = whole_number
        .checked_mul(10i128.pow(exponent as u32))
        .and_then(|value| value.checked_add(fraction.parse::<i128>().unwrap_or(0) as i128))
        .ok_or_else(|| CliError::invalid_input(format!("amount {text:?} is out of range")))?;
    let minor_i64 = i64::try_from(minor).map_err(|_| {
        CliError::invalid_input(format!("amount {text:?} is out of the safe integer range"))
    })?;
    agenthub_core::money::validate_amount_minor(minor_i64)
        .map_err(|error| CliError::invalid_input(error.to_string()))?;
    Ok(minor_i64)
}

/// Refuse a high-impact stack command without `--yes` (D-1 contract).
fn require_yes(action: &str, yes: bool) -> CliResult<()> {
    if yes {
        return Ok(());
    }
    Err(CliError::legacy(format!(
        "refusing to {action}: this is a high-impact action; re-run with --yes to confirm"
    )))
}

fn print_stack_item(item: &StackItemView) {
    let merged = match &item.merged_into {
        Some(target) => format!(" merged-into={target}"),
        None => String::new(),
    };
    println!(
        "{:<38} source={} state={} confirmed={} subs={} detections={}{}",
        format!("{} ({})", item.name, item.id),
        item.source,
        item.detection_state,
        item.confirmed,
        item.subscription_count,
        item.detection_count,
        merged
    );
}

fn print_subscription(subscription: &SubscriptionView) {
    let amount = match (subscription.amount_minor, &subscription.currency) {
        (Some(minor), Some(currency)) => {
            let exponent = agenthub_core::money::currency_exponent(currency).unwrap_or(2);
            let denominator = 10i64.pow(exponent as u32);
            format!("{}/{}", minor / denominator, currency)
        }
        (Some(_), None) => "<amount without currency>".to_string(),
        _ => "<amount unknown>".to_string(),
    };
    let period = subscription.billing_period.as_deref().unwrap_or("unknown");
    println!(
        "{:<40} type={} amount={} period={} status={} renews={} item={}",
        format!("{} ({})", subscription.product_name, subscription.id),
        subscription.billing_type,
        amount,
        period,
        subscription.status,
        subscription.renews_on.as_deref().unwrap_or("unknown"),
        subscription.stack_item_id
    );
}

fn cmd_stack_list(ctx: &Ctx) -> CliResult<u8> {
    let items = api_call(ApiContext::Inspect, ctx.hub.stack_items())?;
    if ctx.json {
        ctx.emit_api_success(&items)?;
    } else if items.is_empty() {
        println!(
            "No stack items yet. Add one with `agenthub stack add` or refresh detections in the desktop app."
        );
    } else {
        for item in &items {
            print_stack_item(item);
        }
    }
    // Frozen plan: the spending summary folds into `stack list`. A spending
    // failure must not blank the item list or fake zero cost (UX-1): print the
    // explicit failure and keep the command successful about the items.
    if !ctx.json {
        match ctx.hub.spending_overview() {
            Ok(overview) => {
                if !overview.by_currency.is_empty() {
                    println!("spending (monthly equivalent, not this month's charge):");
                    for currency in &overview.by_currency {
                        let exponent = agenthub_core::money::currency_exponent(&currency.currency)
                            .unwrap_or(2);
                        println!(
                            "  {} {} / month, {} {} / year, {} active subscriptions",
                            format_major(currency.monthly_equivalent_minor, exponent),
                            currency.currency,
                            format_major(currency.annual_equivalent_minor, exponent),
                            currency.currency,
                            currency.subscription_count
                        );
                    }
                }
                if !overview.incomplete.is_empty() {
                    println!(
                        "  {} subscription(s) incomplete; unknown facts are shown as unknown, never as zero.",
                        overview.incomplete.len()
                    );
                }
                if !overview.usage_without_bills.is_empty() {
                    println!(
                        "  {} usage-based subscription(s) without bills are not estimated.",
                        overview.usage_without_bills.len()
                    );
                }
            }
            Err(spending_failure) => {
                let error = agenthub_core::contract::ApiError::from_error(&spending_failure);
                println!(
                    "spending overview failed: {} {} (items above are still shown; nothing was changed)",
                    error.code.as_str(),
                    spending_failure
                );
            }
        }
    }
    Ok(EXIT_OK)
}

fn cmd_stack_show(ctx: &Ctx, id: &str) -> CliResult<u8> {
    let detail = api_call(ApiContext::Inspect, ctx.hub.stack_item_detail(id))?;
    if ctx.json {
        ctx.emit_api_success(&detail)?;
        return Ok(EXIT_OK);
    }
    print_stack_item(&detail.item);
    println!("detections ({}):", detail.detections.len());
    for detection in &detail.detections {
        println!(
            "  {:<20} status={} at={}",
            detection.adapter_key, detection.detected_status, detection.detected_at
        );
        for evidence in &detection.evidence {
            println!("    evidence: {evidence}");
        }
    }
    println!("subscriptions ({}):", detail.subscriptions.len());
    for subscription in &detail.subscriptions {
        print_subscription(subscription);
    }
    if !detail.merged_children.is_empty() {
        println!("merged children ({}):", detail.merged_children.len());
        for child in &detail.merged_children {
            print_stack_item(child);
        }
    }
    Ok(EXIT_OK)
}

fn cmd_stack_add(ctx: &Ctx, args: &StackUpsertArgs) -> CliResult<u8> {
    let request = StackItemUpsert {
        id: None,
        name: args.name.clone(),
        vendor: args.vendor.clone(),
        overlap_group: args.overlap_group.clone(),
        notes: args.notes.clone(),
    };
    let item = api_call(ApiContext::Inspect, ctx.hub.stack_upsert_item(request))?;
    if ctx.json {
        ctx.emit_api_success(&item)?;
    } else {
        println!("Stack item added: {} ({})", item.name, item.id);
    }
    Ok(EXIT_OK)
}

fn cmd_stack_edit(ctx: &Ctx, args: &StackEditArgs) -> CliResult<u8> {
    // Omitted flags keep the current value (the core overwrite semantics would
    // otherwise silently clear fields); empty strings clear explicitly.
    let existing = api_call(ApiContext::Inspect, ctx.hub.stack_item_detail(&args.id))?;
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| existing.item.name.clone());
    let vendor = match &args.vendor {
        Some(value) => (!value.is_empty()).then(|| value.clone()),
        None => existing.item.vendor.clone(),
    };
    let overlap_group = match &args.overlap_group {
        Some(value) => (!value.is_empty()).then(|| value.clone()),
        None => existing.item.overlap_group.clone(),
    };
    let notes = match &args.notes {
        Some(value) => (!value.is_empty()).then(|| value.clone()),
        None => existing.item.notes.clone(),
    };
    let request = StackItemUpsert {
        id: Some(args.id.clone()),
        name,
        vendor,
        overlap_group,
        notes,
    };
    let item = api_call(ApiContext::Inspect, ctx.hub.stack_upsert_item(request))?;
    if ctx.json {
        ctx.emit_api_success(&item)?;
    } else {
        println!("Stack item updated: {} ({})", item.name, item.id);
    }
    Ok(EXIT_OK)
}

fn cmd_stack_confirm(ctx: &Ctx, id: &str) -> CliResult<u8> {
    let item = api_call(ApiContext::Inspect, ctx.hub.stack_confirm_item(id))?;
    if ctx.json {
        ctx.emit_api_success(&item)?;
    } else {
        println!("Stack item confirmed: {} ({})", item.name, item.id);
    }
    Ok(EXIT_OK)
}

fn cmd_stack_merge(ctx: &Ctx, child_id: &str, into_id: &str, yes: bool) -> CliResult<u8> {
    require_yes(&format!("merge {child_id} into {into_id}"), yes)?;
    let item = api_call(
        ApiContext::Inspect,
        ctx.hub.stack_merge_items(child_id, into_id),
    )?;
    if ctx.json {
        ctx.emit_api_success(&item)?;
    } else {
        println!("Stack item merged into {} ({})", item.name, item.id);
    }
    Ok(EXIT_OK)
}

fn cmd_stack_delete(ctx: &Ctx, id: &str, yes: bool) -> CliResult<u8> {
    require_yes(&format!("delete stack item {id}"), yes)?;
    api_call(ApiContext::Inspect, ctx.hub.stack_delete_item(id))?;
    if ctx.json {
        ctx.emit_api_success(&json!({ "deleted": id }))?;
    } else {
        println!("Stack item deleted: {id}");
    }
    Ok(EXIT_OK)
}

fn cmd_sub_add(ctx: &Ctx, args: &SubAddArgs) -> CliResult<u8> {
    let amount_minor = match (&args.amount, &args.currency) {
        (Some(text), Some(currency)) => Some(parse_amount_major(text, currency)?),
        (Some(_), None) => {
            return Err(CliError::invalid_input(
                "--currency is required when --amount is given; missing amounts stay unknown instead of zero",
            ))
        }
        (None, _) => None,
    };
    let request = SubscriptionUpsert {
        id: None,
        stack_item_id: args.stack_item_id.clone(),
        product_name: args.product_name.clone(),
        vendor: args.vendor.clone(),
        plan_name: args.plan.clone(),
        billing_type: args.billing_type.as_str().to_string(),
        amount_minor,
        currency: args.currency.clone(),
        billing_period: args.period.map(|period| period.as_str().to_string()),
        started_on: args.started_on.clone(),
        renews_on: args.renews_on.clone(),
        status: args.status.map(|status| status.as_str().to_string()),
        notes: args.notes.clone(),
        info_source: args.info_source.clone(),
    };
    let subscription = api_call(ApiContext::Inspect, ctx.hub.subscription_upsert(request))?;
    if ctx.json {
        ctx.emit_api_success(&subscription)?;
    } else {
        println!(
            "Subscription registered: {} ({}) on item {}",
            subscription.product_name, subscription.id, subscription.stack_item_id
        );
    }
    Ok(EXIT_OK)
}

fn cmd_sub_edit(ctx: &Ctx, args: &SubEditArgs) -> CliResult<u8> {
    // Omitted flags keep the current value; empty strings clear fields. The
    // core always rewrites the whole record, so this prefill keeps `sub edit`
    // from silently wiping facts the user did not mention.
    let subscriptions = api_call(ApiContext::Inspect, ctx.hub.subscriptions())?;
    let existing = subscriptions
        .iter()
        .find(|candidate| candidate.id == args.id)
        .ok_or_else(|| {
            CliError::invalid_input(format!("subscription {} does not exist", args.id))
        })?;
    let amount_minor = match &args.amount {
        Some(text) if text.is_empty() => None,
        Some(text) => {
            let currency = args
                .currency
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| existing.currency.clone())
                .ok_or_else(|| {
                    CliError::invalid_input(
                        "--currency is required when --amount is given (no current currency to reuse)",
                    )
                })?;
            Some(parse_amount_major(text, &currency)?)
        }
        None => existing.amount_minor,
    };
    let currency = match &args.currency {
        Some(value) => (!value.is_empty()).then(|| value.clone()),
        None => existing.currency.clone(),
    };
    let request = SubscriptionUpsert {
        id: Some(existing.id.clone()),
        stack_item_id: args
            .stack_item_id
            .clone()
            .unwrap_or_else(|| existing.stack_item_id.clone()),
        product_name: args
            .product_name
            .clone()
            .unwrap_or_else(|| existing.product_name.clone()),
        vendor: match &args.vendor {
            Some(value) => (!value.is_empty()).then(|| value.clone()),
            None => existing.vendor.clone(),
        },
        plan_name: match &args.plan {
            Some(value) => (!value.is_empty()).then(|| value.clone()),
            None => existing.plan_name.clone(),
        },
        billing_type: args
            .billing_type
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| existing.billing_type.clone()),
        amount_minor,
        currency,
        billing_period: args
            .period
            .map(|value| value.as_str().to_string())
            .or_else(|| existing.billing_period.clone()),
        started_on: args
            .started_on
            .clone()
            .or_else(|| existing.started_on.clone()),
        renews_on: args
            .renews_on
            .clone()
            .or_else(|| existing.renews_on.clone()),
        status: args
            .status
            .map(|value| value.as_str().to_string())
            .or_else(|| Some(existing.status.clone())),
        notes: match &args.notes {
            Some(value) => (!value.is_empty()).then(|| value.clone()),
            None => existing.notes.clone(),
        },
        info_source: match &args.info_source {
            Some(value) => (!value.is_empty()).then(|| value.clone()),
            None => existing.info_source.clone(),
        },
    };
    let subscription = api_call(ApiContext::Inspect, ctx.hub.subscription_upsert(request))?;
    if ctx.json {
        ctx.emit_api_success(&subscription)?;
    } else {
        println!(
            "Subscription updated: {} ({})",
            subscription.product_name, subscription.id
        );
    }
    Ok(EXIT_OK)
}

fn cmd_sub_delete(ctx: &Ctx, id: &str, yes: bool) -> CliResult<u8> {
    require_yes(&format!("delete subscription {id}"), yes)?;
    api_call(ApiContext::Inspect, ctx.hub.subscription_delete(id))?;
    if ctx.json {
        ctx.emit_api_success(&json!({ "deleted": id }))?;
    } else {
        println!("Subscription deleted: {id}");
    }
    Ok(EXIT_OK)
}

// -- Cost Optimizer (M2-B) ---------------------------------------------------

fn print_recommendation(rec: &agenthub_core::optimizer::Recommendation) {
    println!("[{}] {}", rec.recommendation_type, rec.title);
    println!("  id: {}", rec.recommendation_id);
    println!("  confidence: {}", rec.confidence_or_evidence_level);
    println!("  action: {}", rec.user_action_required);
    if !rec.missing_information.is_empty() {
        println!("  missing: {}", rec.missing_information.join(", "));
    }
    println!("  {}", rec.explanation.replace('\n', "\n  "));
    println!();
}

fn cmd_cost_report(ctx: &Ctx) -> CliResult<u8> {
    let report = api_call(ApiContext::Inspect, ctx.hub.optimizer_report())?;
    if ctx.json {
        ctx.emit_api_success(&report)?;
        return Ok(EXIT_OK);
    }
    if report.recommendations.is_empty() {
        println!("No recommendations. Everything the rules need is in place.");
    } else {
        for rec in &report.recommendations {
            print_recommendation(rec);
        }
    }
    Ok(EXIT_OK)
}

fn parse_reduce(spec: &str) -> CliResult<(String, i64)> {
    let (id, amount) = spec
        .split_once(':')
        .ok_or_else(|| CliError::invalid_input("--reduce expects <subscription_id>:<new_amount_minor>"))?;
    let amount: i64 = amount.trim().parse().map_err(|_| {
        CliError::invalid_input(format!("--reduce amount {amount:?} is not an integer"))
    })?;
    Ok((id.trim().to_string(), amount))
}

fn parse_replace(spec: &str) -> CliResult<(String, i64, String, String)> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 4 {
        return Err(CliError::invalid_input(
            "--replace expects <subscription_id>:<amount_minor>:<CURRENCY>:<period>",
        ));
    }
    let amount: i64 = parts[1].trim().parse().map_err(|_| {
        CliError::invalid_input(format!("--replace amount {:?} is not an integer", parts[1]))
    })?;
    Ok((
        parts[0].trim().to_string(),
        amount,
        parts[2].trim().to_uppercase(),
        parts[3].trim().to_lowercase(),
    ))
}

fn cmd_cost_simulate(
    ctx: &Ctx,
    cancel: &[String],
    reduce: &[String],
    replace: &[String],
    deactivate: &[String],
) -> CliResult<u8> {
    let mut actions: Vec<agenthub_core::optimizer::ScenarioAction> = Vec::new();
    for id in cancel {
        actions.push(agenthub_core::optimizer::ScenarioAction::Cancel {
            subscription_id: id.trim().to_string(),
        });
    }
    for spec in reduce {
        let (id, amount) = parse_reduce(spec)?;
        actions.push(agenthub_core::optimizer::ScenarioAction::Reduce {
            subscription_id: id,
            new_amount_minor: amount,
        });
    }
    for spec in replace {
        let (id, amount, currency, period) = parse_replace(spec)?;
        actions.push(agenthub_core::optimizer::ScenarioAction::Replace {
            subscription_id: id,
            new_amount_minor: amount,
            new_currency: currency,
            new_period: period,
        });
    }
    for id in deactivate {
        actions.push(agenthub_core::optimizer::ScenarioAction::DeactivateItem {
            stack_item_id: id.trim().to_string(),
        });
    }
    let result = api_call(ApiContext::Inspect, ctx.hub.optimizer_simulate(actions, agenthub_core::optimizer::OptimizerConfig::default()))?;
    if ctx.json {
        ctx.emit_api_success(&result)?;
        return Ok(EXIT_OK);
    }
    println!("Simulation only — nothing was changed and nothing is saved yet.");
    for delta in &result.deltas_monthly {
        println!(
            "  monthly {currency}: {before} -> {after} (delta {delta})",
            currency = delta.currency,
            before = delta.minor_before,
            after = delta.minor_after,
            delta = delta.delta_minor
        );
    }
    for delta in &result.deltas_annual {
        println!(
            "  annual  {currency}: {before} -> {after} (delta {delta})",
            currency = delta.currency,
            before = delta.minor_before,
            after = delta.minor_after,
            delta = delta.delta_minor
        );
    }
    for note in &result.notes {
        println!("  note: {note}");
    }
    Ok(EXIT_OK)
}

fn cmd_cost_settings(ctx: &Ctx) -> CliResult<u8> {
    let settings = api_call(ApiContext::Inspect, ctx.hub.optimizer_settings())?;
    if ctx.json {
        ctx.emit_api_success(&settings)?;
        return Ok(EXIT_OK);
    }
    if settings.cost_thresholds_minor.is_empty() {
        println!("cost thresholds: (none configured — R2 is silent)");
    } else {
        println!("cost thresholds:");
        for (currency, minor) in &settings.cost_thresholds_minor {
            println!("  {currency}: {minor} minor units / month");
        }
    }
    println!(
        "renewal window: {}",
        settings
            .renewal_window_days
            .map(|d| d.to_string())
            .unwrap_or_else(|| "default (7 days)".to_string())
    );
    Ok(EXIT_OK)
}

fn cmd_cost_set(ctx: &Ctx, thresholds: &[String], window: Option<String>) -> CliResult<u8> {
    let mut map: std::collections::BTreeMap<String, i64> = ctx
        .hub
        .optimizer_settings()
        .map_err(|e| CliError::invalid_input(format!("cannot read current settings: {e}")))?
        .cost_thresholds_minor;
    let mut clear_all = false;
    for spec in thresholds {
        let spec = spec.trim();
        if spec.is_empty() {
            clear_all = true;
            continue;
        }
        let (currency, minor) = spec
            .split_once(':')
            .ok_or_else(|| CliError::invalid_input("--threshold expects <CURRENCY:minor>, e.g. USD:5000"))?;
        let minor: i64 = minor.trim().parse().map_err(|_| {
            CliError::invalid_input(format!("--threshold value {minor:?} is not an integer"))
        })?;
        map.insert(currency.trim().to_uppercase(), minor);
    }
    if clear_all {
        map.clear();
    }
    // Option<Option<i64>>: outer None = leave unchanged; inner None = clear
    // back to the engine default; Some(days) = set the window.
    let window_arg: Option<Option<i64>> = match window.as_deref() {
        None => None,
        Some(raw) => {
            let raw = raw.trim();
            if raw.eq_ignore_ascii_case("default") || raw.is_empty() {
                Some(None)
            } else {
                let days: i64 = raw
                    .parse()
                    .map_err(|_| CliError::invalid_input(format!("--window value {raw:?} is not an integer")))?;
                Some(Some(days))
            }
        }
    };
    let stored = api_call(
        ApiContext::Inspect,
        ctx.hub.update_optimizer_settings(Some(map), window_arg),
    )?;
    if ctx.json {
        ctx.emit_api_success(&stored)?;
        return Ok(EXIT_OK);
    }
    println!("Optimizer settings updated.");
    cmd_cost_settings(ctx)
}
