//! Built-in adapters for the agents AgentHub understands today.
//!
//! Evidence for every path used here is recorded in `docs/AGENT-COMPATIBILITY.md`.
//! When a capability has not been verified on a real installation the adapter
//! reports `Unverified` / `VerificationRequired` instead of pretending it works.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{
    find_app, home_dir, AgentAdapter, Detection, McpTargetSpec,
};
use crate::model::{
    AgentStatus, DiscoveredPlugin, McpTransport, PluginSupport, Scope, SymlinkSupport,
};

pub fn suite_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn all_adapters() -> Vec<Box<dyn AgentAdapter>> {
    vec![
        Box::new(CodexAdapter),
        Box::new(ChatGptAdapter),
        Box::new(TraeAdapter),
        Box::new(TraeCnAdapter),
        Box::new(ZCodeAdapter),
        Box::new(WorkBuddyAdapter),
        Box::new(DshAdapter),
        Box::new(ClaudeCodeAdapter),
        Box::new(CursorAdapter),
    ]
}

pub fn find_adapter(key: &str) -> Option<Box<dyn AgentAdapter>> {
    all_adapters().into_iter().find(|adapter| adapter.key() == key || adapter.id() == key)
}

fn detect_dirs(
    home: &Path,
    dot_dir: &str,
    app_names: &[&str],
    notes: Vec<String>,
) -> Detection {
    let config_dir = home_dir(home, dot_dir);
    let app = find_app(home, app_names);
    let mut evidence = Vec::new();
    if config_dir.is_dir() {
        evidence.push(config_dir.to_string_lossy().to_string());
    }
    if let Some(app) = &app {
        evidence.push(app.to_string_lossy().to_string());
    }
    if evidence.is_empty() {
        return Detection::not_installed(notes);
    }
    Detection::installed(app, if config_dir.is_dir() { Some(config_dir) } else { None }, evidence, notes)
}

fn mcp_json(path: PathBuf, format: &'static str, transports: Vec<McpTransport>) -> McpTargetSpec {
    McpTargetSpec {
        path,
        format,
        scope: Scope::Global,
        transports,
        notes: vec!["Written atomically; unrelated keys in the file are preserved.".to_string()],
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "builtin.codex"
    }
    fn key(&self) -> &'static str {
        "codex"
    }
    fn name(&self) -> &'static str {
        "Codex"
    }
    fn vendor(&self) -> &'static str {
        "OpenAI"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        // No public documentation confirms whether Codex follows symlinked
        // skill directories; copy is offered as the safe default.
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::FileManaged
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://developers.openai.com/codex/")
    }
    fn requires_restart_note(&self) -> Option<&'static str> {
        Some("Codex reads skills at session start; start a new session to pick up changes.")
    }

    fn detect(&self, home: &Path) -> Detection {
        detect_dirs(
            home,
            ".codex",
            &["Codex.app"],
            vec!["Detected through ~/.codex or the Codex app bundle.".to_string()],
        )
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        // Codex CLI also reads the shared ~/.agents/skills convention.
        vec![
            home_dir(home, ".codex/skills"),
            home_dir(home, ".agents/skills"),
        ]
    }

    fn install_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".codex/skills")]
    }

    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: home.join(".codex/config.toml"),
            format: "toml:mcp_servers",
            scope: Scope::Global,
            transports: vec![McpTransport::Stdio],
            notes: vec![
                "Codex stores MCP servers as [mcp_servers.<name>] tables in config.toml."
                    .to_string(),
                "Other config.toml sections are preserved when writing.".to_string(),
            ],
        }]
    }

    fn discover_plugins(&self, home: &Path, agent_id: &str) -> Vec<DiscoveredPlugin> {
        let config = home.join(".codex/config.toml");
        let Ok(text) = std::fs::read_to_string(&config) else {
            return Vec::new();
        };
        let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
            return Vec::new();
        };
        let Some(plugins) = doc.get("plugins").and_then(|item| item.as_table_like()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, value) in plugins.iter() {
            let enabled = value
                .get("enabled")
                .and_then(|item| item.as_bool())
                .unwrap_or(false);
            let source = name.split_once('@').map(|(_, market)| market.to_string());
            out.push(DiscoveredPlugin {
                agent_id: agent_id.to_string(),
                agent_key: "codex".to_string(),
                agent_name: "Codex".to_string(),
                name: name.to_string(),
                version: None,
                source,
                install_state: if enabled { "enabled" } else { "disabled" }.to_string(),
                permissions: Vec::new(),
                official_url: Some("https://developers.openai.com/codex/".to_string()),
                discovered_path: Some(config.to_string_lossy().to_string()),
                notes: vec![
                    "Discovered from config.toml [plugins.<name>] (enable/disable is file-managed)."
                        .to_string(),
                ],
            });
        }
        out
    }
}

// ---------------------------------------------------------------------------
// ChatGPT desktop
// ---------------------------------------------------------------------------

pub struct ChatGptAdapter;

impl AgentAdapter for ChatGptAdapter {
    fn id(&self) -> &'static str {
        "builtin.chatgpt"
    }
    fn key(&self) -> &'static str {
        "chatgpt"
    }
    fn name(&self) -> &'static str {
        "ChatGPT"
    }
    fn vendor(&self) -> &'static str {
        "OpenAI"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unsupported
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::ManualOnly
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://chatgpt.com/apps")
    }

    fn detect(&self, home: &Path) -> Detection {
        match find_app(home, &["ChatGPT.app"]) {
            Some(app) => Detection::installed(
                Some(app.clone()),
                None,
                vec![app.to_string_lossy().to_string()],
                vec![
                    "ChatGPT desktop plugins/apps are account-authorized connectors, not local SKILL.md directories."
                        .to_string(),
                    "AgentHub can detect the app but cannot install local skills or write MCP config for it."
                        .to_string(),
                ],
            ),
            None => Detection::not_installed(vec![
                "ChatGPT.app not found in /Applications.".to_string(),
            ]),
        }
    }

    fn skill_roots(&self, _home: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    fn mcp_target_specs(&self, _home: &Path) -> Vec<McpTargetSpec> {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// TRAE (international + CN)
// ---------------------------------------------------------------------------

pub struct TraeAdapter;

impl TraeAdapter {
    fn base(home: &Path) -> PathBuf {
        home_dir(home, ".trae")
    }
}

impl AgentAdapter for TraeAdapter {
    fn id(&self) -> &'static str {
        "builtin.trae"
    }
    fn key(&self) -> &'static str {
        "trae"
    }
    fn name(&self) -> &'static str {
        "TRAE (International)"
    }
    fn vendor(&self) -> &'static str {
        "ByteDance"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::DiscoveryOnly
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://docs.trae.ai/")
    }
    fn requires_restart_note(&self) -> Option<&'static str> {
        Some("TRAE discovers project skills when a project is opened; reopen the project or restart TRAE.")
    }

    fn detect(&self, home: &Path) -> Detection {
        detect_dirs(
            home,
            ".trae",
            &["Trae.app"],
            vec!["TRAE stores skills under ~/.trae and per-project .trae/skills.".to_string()],
        )
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        let base = Self::base(home);
        vec![
            base.join("skills"),
            base.join("builtin_skills"),
        ]
    }

    fn install_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![Self::base(home).join("skills")]
    }

    fn mcp_target_specs(&self, _home: &Path) -> Vec<McpTargetSpec> {
        // TRAE keeps MCP definitions inside per-workspace state
        // (~/.trae/mcps/<workspace>/<agent>/); no stable documented file format
        // has been verified, so AgentHub does not write it in the MVP.
        Vec::new()
    }

    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        vec![project.join(".trae/skills"), project.join(".agents/skills")]
    }

    fn project_mcp_target_specs(&self, project: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: project.join(".trae/mcp.json"),
            format: "json:mcpServers",
            scope: Scope::Project,
            transports: vec![McpTransport::Sse, McpTransport::StreamableHttp],
            notes: vec![
                "TRAE project MCP config. Only ${workspaceFolder} variables are supported by TRAE."
                    .to_string(),
            ],
        }]
    }
}

pub struct TraeCnAdapter;

impl AgentAdapter for TraeCnAdapter {
    fn id(&self) -> &'static str {
        "builtin.trae-cn"
    }
    fn key(&self) -> &'static str {
        "trae-cn"
    }
    fn name(&self) -> &'static str {
        "TRAE CN"
    }
    fn vendor(&self) -> &'static str {
        "ByteDance"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::DiscoveryOnly
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://docs.trae.cn/")
    }
    fn requires_restart_note(&self) -> Option<&'static str> {
        Some("TRAE CN discovers project skills when a project is opened; reopen the project or restart TRAE.")
    }

    fn detect(&self, home: &Path) -> Detection {
        detect_dirs(
            home,
            ".trae-cn",
            &["Trae CN.app"],
            vec![
                "TRAE CN keeps user skills under ~/.trae-cn and workspace installs under ~/.trae-cn/work."
                    .to_string(),
            ],
        )
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        let base = home_dir(home, ".trae-cn");
        let mut roots = vec![
            base.join("skills"),
            home_dir(home, ".trae/skills"),
            home_dir(home, ".agents/skills"),
            base.join("builtin_skills"),
        ];
        // Installed Work skills live in ~/.trae-cn/work/<workspace>/<skill>.
        if let Ok(entries) = std::fs::read_dir(base.join("work")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let has_skill = std::fs::read_dir(&path)
                    .map(|children| {
                        children.flatten().any(|child| {
                            child.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                                && child.path().join("SKILL.md").is_file()
                        })
                    })
                    .unwrap_or(false);
                if has_skill {
                    roots.push(path);
                }
            }
        }
        roots
    }

    fn install_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".trae-cn/skills")]
    }

    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: home.join("Library/Application Support/Trae CN/User/mcp.json"),
            format: "json:mcpServers",
            scope: Scope::Global,
            transports: vec![McpTransport::Sse, McpTransport::StreamableHttp],
            notes: vec![
                "TRAE CN global MCP config (application support directory).".to_string(),
            ],
        }]
    }

    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        vec![project.join(".trae/skills"), project.join(".agents/skills")]
    }

    fn project_mcp_target_specs(&self, project: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: project.join(".trae/mcp.json"),
            format: "json:mcpServers",
            scope: Scope::Project,
            transports: vec![McpTransport::Sse, McpTransport::StreamableHttp],
            notes: vec![
                "TRAE CN project MCP config. Only ${workspaceFolder} variables are supported."
                    .to_string(),
            ],
        }]
    }

    fn discover_plugins(&self, home: &Path, agent_id: &str) -> Vec<DiscoveredPlugin> {
        let base = home_dir(home, ".trae-cn");
        let mut out = Vec::new();
        if let Some(doc) = read_json(&base.join("installed-plugins.json")) {
            collect_trae_plugins(&doc, agent_id, "TRAE CN", &mut out);
        }
        if let Some(doc) = read_json(&base.join("plugin-config.json")) {
            collect_trae_plugins(&doc, agent_id, "TRAE CN", &mut out);
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.dedup_by(|a, b| a.name == b.name);
        out
    }
}

fn collect_trae_plugins(doc: &Value, agent_id: &str, agent_name: &str, out: &mut Vec<DiscoveredPlugin>) {
    let mut stack = vec![doc];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(map) => {
                if let Some(name) = map
                    .get("plugin_id")
                    .or_else(|| map.get("name"))
                    .and_then(|v| v.as_str())
                {
                    let display = map
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name);
                    if !display.is_empty() {
                        out.push(DiscoveredPlugin {
                            agent_id: agent_id.to_string(),
                            agent_key: "trae-cn".to_string(),
                            agent_name: agent_name.to_string(),
                            name: display.to_string(),
                            version: map
                                .get("version")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            source: map
                                .get("registry")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            install_state: "discovered".to_string(),
                            permissions: Vec::new(),
                            official_url: Some("https://docs.trae.cn/".to_string()),
                            discovered_path: None,
                            notes: vec![
                                "Read-only discovery from TRAE CN plugin metadata; installation stays in TRAE's own marketplace."
                                    .to_string(),
                            ],
                        });
                    }
                }
                for child in map.values() {
                    stack.push(child);
                }
            }
            Value::Array(items) => {
                for child in items {
                    stack.push(child);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// ZCode
// ---------------------------------------------------------------------------

pub struct ZCodeAdapter;

impl AgentAdapter for ZCodeAdapter {
    fn id(&self) -> &'static str {
        "builtin.zcode"
    }
    fn key(&self) -> &'static str {
        "zcode"
    }
    fn name(&self) -> &'static str {
        "ZCode"
    }
    fn vendor(&self) -> &'static str {
        "Z.ai"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::Unverified
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://zcode.z.ai/")
    }
    fn requires_restart_note(&self) -> Option<&'static str> {
        Some("Restart ZCode or start a new task after changing its skills.")
    }

    fn detect(&self, home: &Path) -> Detection {
        detect_dirs(
            home,
            ".zcode",
            &["ZCode.app"],
            vec!["ZCode stores global skills under ~/.zcode/skills and project skills under <project>/.zcode/skills.".to_string()],
        )
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".zcode/skills")]
    }

    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: home.join(".zcode/cli/config.json"),
            format: "json:mcp.servers",
            scope: Scope::Global,
            transports: vec![McpTransport::Stdio, McpTransport::Http, McpTransport::StreamableHttp],
            notes: vec![
                "ZCode CLI config stores MCP servers under mcp.servers.".to_string(),
                "If a project has .zcode/mcp.json, ZCode ignores .agents/mcp.json for that project."
                    .to_string(),
            ],
        }]
    }

    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        vec![project.join(".zcode/skills")]
    }

    fn project_mcp_target_specs(&self, project: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: project.join(".agents/mcp.json"),
            format: "json:mcpServers",
            scope: Scope::Project,
            transports: vec![
                McpTransport::Stdio,
                McpTransport::Http,
                McpTransport::StreamableHttp,
            ],
            notes: vec![
                "Shared .agents/mcp.json convention; ZCode may ignore it if a higher-priority ZCode config exists."
                    .to_string(),
            ],
        }]
    }
}

// ---------------------------------------------------------------------------
// WorkBuddy
// ---------------------------------------------------------------------------

pub struct WorkBuddyAdapter;

impl AgentAdapter for WorkBuddyAdapter {
    fn id(&self) -> &'static str {
        "builtin.workbuddy"
    }
    fn key(&self) -> &'static str {
        "workbuddy"
    }
    fn name(&self) -> &'static str {
        "WorkBuddy"
    }
    fn vendor(&self) -> &'static str {
        "Tencent"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::DiscoveryOnly
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://www.workbuddy.cn/")
    }
    fn requires_restart_note(&self) -> Option<&'static str> {
        Some("WorkBuddy refreshes its skill list on startup; restart or reopen the workspace.")
    }

    fn detect(&self, home: &Path) -> Detection {
        detect_dirs(
            home,
            ".workbuddy",
            &["WorkBuddy.app"],
            vec!["WorkBuddy stores its skills under ~/.workbuddy/skills.".to_string()],
        )
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        let base = home_dir(home, ".workbuddy");
        vec![
            base.join("skills"),
            base.join("skills-marketplace/skills"),
            base.join("connectors/skills"),
        ]
    }

    fn install_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".workbuddy/skills")]
    }

    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec> {
        vec![mcp_json(
            home.join(".workbuddy/connectors/default/mcp.json"),
            "json:mcpServers",
            vec![
                McpTransport::Http,
                McpTransport::StreamableHttp,
                McpTransport::Sse,
            ],
        )]
    }

    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        vec![project.join(".workbuddy/skills")]
    }

    fn discover_plugins(&self, home: &Path, agent_id: &str) -> Vec<DiscoveredPlugin> {
        let base = home_dir(home, ".workbuddy");
        let mut out = Vec::new();
        let candidates = [
            base.join("connectors-marketplace/marketplace.json"),
            base.join("connectors-marketplace/connectors-marketplace.json"),
            base.join("connectors/enterprise-registry.json"),
        ];
        for path in candidates {
            let Some(doc) = read_json(&path) else { continue };
            let mut stack = vec![&doc];
            while let Some(value) = stack.pop() {
                match value {
                    Value::Object(map) => {
                        if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                            out.push(DiscoveredPlugin {
                                agent_id: agent_id.to_string(),
                                agent_key: "workbuddy".to_string(),
                                agent_name: "WorkBuddy".to_string(),
                                name: name.to_string(),
                                version: map
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                source: Some("workbuddy-marketplace".to_string()),
                                install_state: "discovered".to_string(),
                                permissions: Vec::new(),
                                official_url: Some("https://www.workbuddy.cn/".to_string()),
                                discovered_path: Some(path.to_string_lossy().to_string()),
                                notes: vec![
                                    "Read-only discovery from WorkBuddy connector registry."
                                        .to_string(),
                                ],
                            });
                        }
                        for child in map.values() {
                            stack.push(child);
                        }
                    }
                    Value::Array(items) => items.iter().for_each(|child| stack.push(child)),
                    _ => {}
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.dedup_by(|a, b| a.name == b.name);
        out
    }
}

// ---------------------------------------------------------------------------
// DeepSeek Harness (DSH)
// ---------------------------------------------------------------------------

pub struct DshAdapter;

impl AgentAdapter for DshAdapter {
    fn id(&self) -> &'static str {
        "builtin.dsh"
    }
    fn key(&self) -> &'static str {
        "dsh"
    }
    fn name(&self) -> &'static str {
        "DeepSeek Harness"
    }
    fn vendor(&self) -> &'static str {
        "DeepSeek"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        // Verified on this machine: ~/.dsh/vendor/huashu-motion -> skills is
        // discovered through a symlink.
        SymlinkSupport::Supported
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::DiscoveryOnly
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://github.com/deepseek-ai")
    }
    fn requires_restart_note(&self) -> Option<&'static str> {
        Some(
            "DSH loads project skills when a session starts; start a new session to pick up changes.",
        )
    }

    fn detect(&self, home: &Path) -> Detection {
        let mut evidence = Vec::new();
        let config = home_dir(home, ".dsh");
        if config.is_dir() {
            evidence.push(config.to_string_lossy().to_string());
        }
        if let Some(app) = find_app(home, &["DeepSeek Harness.app"]) {
            evidence.push(app.to_string_lossy().to_string());
        }
        if evidence.is_empty() {
            return Detection::not_installed(vec![
                "DSH config directory and app bundle were not found.".to_string(),
            ]);
        }
        let notes = vec![
            "DSH project skills live in <project>/.dsh/skills; no verified global skill directory exists."
                .to_string(),
            "DSH plugins are managed by the Harness plugin manager; AgentHub only discovers them."
                .to_string(),
        ];
        Detection {
            status: AgentStatus::Installed,
            app_path: find_app(home, &["DeepSeek Harness.app"]),
            home_path: Some(config),
            evidence,
            notes,
        }
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        // DSH skill root precedence (verified from the DSH skill-filesystem
        // plugin): project .dsh/skills > project .agents/skills > custom >
        // ~/.dsh/skills > ~/.agents/skills > bundled.
        vec![
            home_dir(home, ".dsh/skills"),
            home_dir(home, ".agents/skills"),
        ]
    }

    fn install_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".dsh/skills")]
    }

    fn mcp_target_specs(&self, _home: &Path) -> Vec<McpTargetSpec> {
        Vec::new()
    }

    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        vec![project.join(".dsh/skills"), project.join(".agents/skills")]
    }
}

// ---------------------------------------------------------------------------
// Claude Code (extended, reserved in MVP)
// ---------------------------------------------------------------------------

pub struct ClaudeCodeAdapter;

impl AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "builtin.claude-code"
    }
    fn key(&self) -> &'static str {
        "claude-code"
    }
    fn name(&self) -> &'static str {
        "Claude Code"
    }
    fn vendor(&self) -> &'static str {
        "Anthropic"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::DiscoveryOnly
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://docs.anthropic.com/en/docs/claude-code")
    }

    fn detect(&self, home: &Path) -> Detection {
        let config = home_dir(home, ".claude");
        let global = home.join(".claude.json");
        let mut evidence = Vec::new();
        if config.is_dir() {
            evidence.push(config.to_string_lossy().to_string());
        }
        if global.is_file() {
            evidence.push(global.to_string_lossy().to_string());
        }
        if evidence.is_empty() {
            return Detection::not_installed(vec!["~/.claude was not found.".to_string()]);
        }
        Detection {
            status: AgentStatus::VerificationRequired,
            app_path: None,
            home_path: Some(config),
            evidence,
            notes: vec![
                "Reserved adapter: skills and MCP discovery are implemented, installation is marked Verification Required."
                    .to_string(),
            ],
        }
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".claude/skills")]
    }

    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: home.join(".claude.json"),
            format: "json:mcpServers",
            scope: Scope::Global,
            transports: vec![McpTransport::Stdio, McpTransport::Http],
            notes: vec!["Reserved adapter, Verification Required.".to_string()],
        }]
    }

    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        vec![project.join(".claude/skills")]
    }
}

// ---------------------------------------------------------------------------
// Cursor (extended)
// ---------------------------------------------------------------------------

pub struct CursorAdapter;

impl AgentAdapter for CursorAdapter {
    fn id(&self) -> &'static str {
        "builtin.cursor"
    }
    fn key(&self) -> &'static str {
        "cursor"
    }
    fn name(&self) -> &'static str {
        "Cursor"
    }
    fn vendor(&self) -> &'static str {
        "Anysphere"
    }
    fn symlink_support(&self) -> SymlinkSupport {
        SymlinkSupport::Unverified
    }
    fn plugin_support(&self) -> PluginSupport {
        PluginSupport::Unverified
    }
    fn official_url(&self) -> Option<&'static str> {
        Some("https://docs.cursor.com/")
    }

    fn detect(&self, home: &Path) -> Detection {
        let config = home_dir(home, ".cursor");
        let app = find_app(home, &["Cursor.app"]);
        if !config.is_dir() && app.is_none() {
            return Detection::not_installed(vec!["Cursor was not found.".to_string()]);
        }
        let mut evidence = Vec::new();
        if config.is_dir() {
            evidence.push(config.to_string_lossy().to_string());
        }
        if let Some(app) = &app {
            evidence.push(app.to_string_lossy().to_string());
        }
        Detection {
            status: AgentStatus::VerificationRequired,
            app_path: app,
            home_path: if config.is_dir() { Some(config) } else { None },
            evidence,
            notes: vec!["Reserved adapter, skill installation not verified.".to_string()],
        }
    }

    fn skill_roots(&self, home: &Path) -> Vec<PathBuf> {
        vec![home_dir(home, ".cursor/skills")]
    }

    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec> {
        vec![McpTargetSpec {
            path: home.join(".cursor/mcp.json"),
            format: "json:mcpServers",
            scope: Scope::Global,
            transports: vec![McpTransport::Stdio, McpTransport::Http],
            notes: vec!["Reserved adapter, Verification Required.".to_string()],
        }]
    }
}
