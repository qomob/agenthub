//! Agent adapter engine.
//!
//! Every supported agent is represented by an `AgentAdapter`. Adapters only
//! describe *where* an agent keeps its files and *what* it supports; all writes
//! are executed by the transactional distribution engine in `ops`.
//!
//! Adapters are compiled into the binary (no untrusted third-party executables
//! are loaded in the MVP).

mod builtin;

pub use builtin::{all_adapters, find_adapter, suite_version};

use std::path::{Path, PathBuf};

use crate::model::{
    AgentRecord, AgentStatus, DiscoveredPlugin, McpTarget, McpTransport, PluginSupport, Scope,
    SkillRoot, SymlinkSupport,
};

#[derive(Debug, Clone)]
pub struct Detection {
    pub status: AgentStatus,
    pub app_path: Option<PathBuf>,
    pub home_path: Option<PathBuf>,
    /// Concrete evidence used to decide detection (paths that exist).
    pub evidence: Vec<String>,
    pub notes: Vec<String>,
}

impl Detection {
    pub fn not_installed(notes: Vec<String>) -> Self {
        Detection {
            status: AgentStatus::NotInstalled,
            app_path: None,
            home_path: None,
            evidence: Vec::new(),
            notes,
        }
    }

    pub fn installed(
        app_path: Option<PathBuf>,
        home_path: Option<PathBuf>,
        evidence: Vec<String>,
        notes: Vec<String>,
    ) -> Self {
        Detection {
            status: AgentStatus::Installed,
            app_path,
            home_path,
            evidence,
            notes,
        }
    }
}

/// A location where an agent looks for skill directories.
#[derive(Debug, Clone)]
pub struct SkillRootSpec {
    pub path: PathBuf,
    pub scope: Scope,
    /// "adapter" for built-in locations, "custom" for user-added roots.
    pub source: String,
}

/// A configuration file that carries MCP server definitions.
#[derive(Debug, Clone)]
pub struct McpTargetSpec {
    pub path: PathBuf,
    pub format: &'static str,
    pub scope: Scope,
    pub transports: Vec<McpTransport>,
    pub notes: Vec<String>,
}

impl McpTargetSpec {
    pub fn to_model(&self) -> McpTarget {
        let meta = std::fs::metadata(&self.path);
        McpTarget {
            path: self.path.to_string_lossy().to_string(),
            format: self.format.to_string(),
            scope: self.scope,
            exists: meta.is_ok(),
            writable: crate::util::is_writable_dir(
                self.path.parent().unwrap_or_else(|| Path::new("/")),
            ),
            transports: self.transports.clone(),
            notes: self.notes.clone(),
        }
    }
}

impl SkillRootSpec {
    pub fn to_model(&self, skill_count: usize) -> SkillRoot {
        let exists = self.path.is_dir();
        SkillRoot {
            path: self.path.to_string_lossy().to_string(),
            scope: self.scope,
            source: self.source.clone(),
            exists,
            writable: exists && crate::util::is_writable_dir(&self.path),
            skill_count,
        }
    }
}

pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn key(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn vendor(&self) -> &'static str;
    fn symlink_support(&self) -> SymlinkSupport;
    fn plugin_support(&self) -> PluginSupport;
    fn official_url(&self) -> Option<&'static str> {
        None
    }
    /// Whether a restart / project reopen is required after file changes.
    fn requires_restart_note(&self) -> Option<&'static str> {
        None
    }
    fn detect(&self, home: &Path) -> Detection;
    fn skill_roots(&self, home: &Path) -> Vec<PathBuf>;
    /// Roots that AgentHub may create new skill directories in. Read-only
    /// locations (built-in skills, marketplace caches) are excluded.
    fn install_roots(&self, home: &Path) -> Vec<PathBuf> {
        self.skill_roots(home)
    }
    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec>;
    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> {
        let _ = project;
        Vec::new()
    }
    fn project_mcp_target_specs(&self, project: &Path) -> Vec<McpTargetSpec> {
        let _ = project;
        Vec::new()
    }
    fn discover_plugins(&self, home: &Path, agent_id: &str) -> Vec<DiscoveredPlugin> {
        let _ = (home, agent_id);
        Vec::new()
    }

    /// Resolve all skill roots including user custom roots.
    fn resolved_skill_roots(&self, home: &Path, record: Option<&AgentRecord>) -> Vec<SkillRootSpec> {
        let mut roots: Vec<SkillRootSpec> = self
            .skill_roots(home)
            .into_iter()
            .map(|path| SkillRootSpec {
                source: classify_root_source(&path),
                path,
                scope: Scope::Global,
            })
            .collect();
        if let Some(record) = record {
            for path in &record.custom_skill_roots {
                let path = PathBuf::from(path);
                let exists_already = roots.iter().any(|root| root.path == path);
                if !exists_already {
                    roots.push(SkillRootSpec {
                        path,
                        scope: Scope::Custom,
                        source: "custom".to_string(),
                    });
                }
            }
        }
        roots
    }

    fn resolved_mcp_targets(&self, home: &Path) -> Vec<McpTargetSpec> {
        self.mcp_target_specs(home)
    }
}

/// Label a root by what it actually contains so the UI can distinguish
/// writable install targets from read-only caches.
pub(crate) fn classify_root_source(path: &Path) -> String {
    let text = path.to_string_lossy();
    if text.contains("builtin_skills") {
        "builtin".to_string()
    } else if text.contains("skills-marketplace") || text.contains("marketplace") {
        "marketplace".to_string()
    } else if text.contains("/work/") {
        "workspace".to_string()
    } else if text.contains("connectors/skills") {
        "connector".to_string()
    } else if text.ends_with(".agents/skills") {
        "shared".to_string()
    } else {
        "adapter".to_string()
    }
}

/// Enumerate installed app bundles in the two standard application folders.
pub(crate) fn find_app(home: &Path, names: &[&str]) -> Option<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        home.join("Applications"),
    ];
    if let Ok(extra) = std::env::var("AGENTHUB_EXTRA_APP_DIRS") {
        for part in extra.split(':') {
            if !part.is_empty() {
                roots.push(PathBuf::from(part));
            }
        }
    }
    for root in roots {
        for name in names {
            let candidate = root.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Resolve `~/.<name>` using an injectable home so tests never touch the real home.
pub(crate) fn home_dir(home: &Path, name: &str) -> PathBuf {
    home.join(name)
}
