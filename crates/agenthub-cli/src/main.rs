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
//!   1  failure (invalid input, I/O or engine error)
//!   2  partial success, or a plan that contains blocked/conflict items, or a
//!      plan that was printed for confirmation because `--yes` was missing
//!
//! Every mutating command follows the same contract: a plan is produced first
//! and printed; nothing is written unless `--yes` is passed and the plan has no
//! blocked items. `--dry-run` always stops after printing the plan.

use std::path::{Path, PathBuf};

use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};

const EXIT_OK: u8 = 0;
const EXIT_FAIL: u8 = 1;
const EXIT_PARTIAL: u8 = 2;

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
}

#[derive(Debug, Subcommand)]
enum AgentsCommand {
    /// List every supported agent with detection status and roots.
    List,
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
}

#[derive(Debug, Subcommand)]
enum BackupsCommand {
    /// List backups present in the library.
    List,
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

// ---------------------------------------------------------------------------
// Error / entry point
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CliError(String);

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<agenthub_core::Error> for CliError {
    fn from(value: agenthub_core::Error) -> Self {
        CliError(value.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        CliError(format!("json error: {value}"))
    }
}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        CliError(value)
    }
}

impl From<&str> for CliError {
    fn from(value: &str) -> Self {
        CliError(value.to_string())
    }
}

type CliResult<T> = std::result::Result<T, CliError>;

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            if json {
                let payload = json!({ "ok": false, "error": error.to_string() });
                eprintln!(
                    "{}",
                    serde_json::to_string(&payload).unwrap_or_else(|_| "{\"ok\":false}".to_string())
                );
            } else {
                eprintln!("error: {error}");
            }
            std::process::ExitCode::from(EXIT_FAIL)
        }
    }
}

fn run(cli: Cli) -> CliResult<u8> {
    let hub = open_hub(cli.home.as_deref())?;
    let ctx = Ctx {
        hub,
        json: cli.json,
    };
    match cli.command {
        Commands::Doctor => cmd_doctor(&ctx),
        Commands::Agents { command } => match command {
            AgentsCommand::List => cmd_agents_list(&ctx),
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
        },
        Commands::Mcp { command } => match command {
            McpCommand::List => cmd_mcp_list(&ctx),
            McpCommand::Discover(args) => cmd_mcp_discover(&ctx, args),
            McpCommand::Distribute(args) => cmd_mcp_distribute(&ctx, args),
        },
        Commands::Operations { command } => match command {
            OperationsCommand::List(args) => cmd_operations_list(&ctx, args),
        },
        Commands::Profiles { command } => match command {
            ProfilesCommand::List => cmd_profiles_list(&ctx),
        },
        Commands::Health(args) => cmd_health(&ctx, args),
        Commands::Backup { command } => match command {
            BackupCommand::Create => cmd_backup_create(&ctx),
        },
        Commands::Backups { command } => match command {
            BackupsCommand::List => cmd_backups_list(&ctx),
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
        return Err(CliError(format!(
            "unknown agent key(s): {}; available agents: {}",
            unknown.join(", "),
            available.join(", ")
        )));
    }
    if ids.is_empty() {
        return Err(CliError("no agent keys were given".to_string()));
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
        Err(CliError(format!(
            "skill '{wanted}' does not exist; the library is empty (import a skill first)"
        )))
    } else {
        Err(CliError(format!(
            "skill '{wanted}' does not exist; available skills: {}",
            available.join(", ")
        )))
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

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
            return Err(CliError("--git requires a non-empty URL".to_string()));
        }
        return Ok(ImportSource::Git {
            url: url.clone(),
            reference: args.reference.clone(),
        });
    }
    let Some(path) = &args.path else {
        return Err(CliError(
            "nothing to import: pass --path <dir|zip|SKILL.md> or --git <url>".to_string(),
        ));
    };
    if !path.exists() {
        return Err(CliError(format!(
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
        Err(CliError(format!(
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
        .ok_or_else(|| CliError("skill disappeared".to_string()))?;
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
            .ok_or_else(|| CliError("there is no rollbackable operation".to_string()))?
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
                CliError(format!(
                    "MCP server '{}' does not exist; no MCP servers are defined yet",
                    args.server_id
                ))
            } else {
                CliError(format!(
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
