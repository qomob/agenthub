//! High-level AgentHub service. This is the single implementation used by both
//! the Tauri command layer and the CLI.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::adapters::{all_adapters, find_adapter, AgentAdapter};
use crate::db::{AgentRow, Database, InstallationRow, McpRow, ProfileRow};
use crate::error::{Error, Result};
use crate::library::{self, Library};
use crate::model::*;
use crate::ops::{
    approval_expiry_from_now, current_target_hash, plan_digest, Executor, APPROVAL_TTL_MINUTES,
};
use crate::scan;
use crate::skill::{self, StagingDir};
use crate::util;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// The user's home directory (or a test fixture home).
    pub home: PathBuf,
    /// AgentHub metadata directory (`~/.agenthub`).
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub default_library_root: PathBuf,
}

impl CoreConfig {
    pub fn resolve() -> Result<Self> {
        let home = match std::env::var_os("AGENTHUB_HOME") {
            Some(value) => PathBuf::from(value),
            None => dirs::home_dir().ok_or_else(|| Error::msg("cannot resolve home directory"))?,
        };
        let data_dir = match std::env::var_os("AGENTHUB_DATA_DIR") {
            Some(value) => PathBuf::from(value),
            None => home.join(".agenthub"),
        };
        Ok(CoreConfig {
            home,
            db_path: data_dir.join("agenthub.db"),
            default_library_root: data_dir.clone(),
            data_dir,
        })
    }

    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let data_dir = home.join(".agenthub");
        CoreConfig {
            db_path: data_dir.join("agenthub.db"),
            default_library_root: data_dir.clone(),
            home,
            data_dir,
        }
    }
}

pub struct AgentHub {
    config: CoreConfig,
    db: Database,
    /// Serializes writes originating from the UI process.
    write_lock: Mutex<()>,
}

impl AgentHub {
    pub fn open(config: CoreConfig) -> Result<Self> {
        fs::create_dir_all(&config.data_dir)?;
        let db = Database::open(&config.db_path)?;
        let hub = AgentHub {
            config,
            db,
            write_lock: Mutex::new(()),
        };
        hub.seed_defaults()?;
        let library = hub.library()?;
        library.init()?;
        hub.sync_agents()?;
        hub.seed_profiles()?;
        Ok(hub)
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn config(&self) -> &CoreConfig {
        &self.config
    }

    pub fn home(&self) -> &Path {
        &self.config.home
    }

    pub fn app_version(&self) -> &'static str {
        APP_VERSION
    }

    pub fn library(&self) -> Result<Library> {
        let root = self
            .db
            .get_setting("library_root")?
            .map(PathBuf::from)
            .unwrap_or_else(|| self.config.default_library_root.clone());
        Ok(Library::new(root))
    }

    fn seed_defaults(&self) -> Result<()> {
        let data_dir = self.config.data_dir.to_string_lossy().to_string();
        let defaults: &[(&str, String)] = &[
            ("library_root", data_dir),
            ("default_strategy", "copy".to_string()),
            ("language", "zh".to_string()),
            ("theme", "system".to_string()),
            ("scan_on_startup", "true".to_string()),
            ("project_roots", "[]".to_string()),
            ("initialized", "false".to_string()),
            ("git_import_enabled", "true".to_string()),
        ];
        for (key, value) in defaults {
            if self.db.get_setting(key)?.is_none() {
                self.db.set_setting(key, value)?;
            }
        }
        Ok(())
    }

    // -- agents ------------------------------------------------------------

    pub fn sync_agents(&self) -> Result<()> {
        let home = &self.config.home;
        for adapter in all_adapters() {
            let detection = adapter.detect(home);
            let existing = self.db.get_agent_by_key(adapter.key())?;
            let now = util::now();
            let record = AgentRecord {
                id: existing
                    .as_ref()
                    .map(|row| row.id.clone())
                    .unwrap_or_else(|| format!("agent-{}", adapter.key())),
                key: adapter.key().to_string(),
                name: adapter.name().to_string(),
                vendor: adapter.vendor().to_string(),
                adapter_id: adapter.id().to_string(),
                app_path: detection
                    .app_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                home_path: detection
                    .home_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                custom_skill_roots: existing
                    .as_ref()
                    .map(|row| row.custom_skill_roots.clone())
                    .unwrap_or_default(),
                notes: detection.notes,
                created_at: existing
                    .as_ref()
                    .map(|row| row.created_at.clone())
                    .unwrap_or_else(|| now.clone()),
                updated_at: now,
            };
            self.db.upsert_agent(&record)?;
            self.seed_agent_evidence(&*adapter, &record, detection.status)?;
        }
        Ok(())
    }

    /// Record conservative per-feature evidence for an agent. Read-side facts
    /// observed on this machine become `verified_config`; every write/execute
    /// feature starts as `unverified` until an authorized real-host test
    /// produces runtime evidence. This is the Phase 0/1 evidence baseline.
    fn seed_agent_evidence(
        &self,
        adapter: &dyn AgentAdapter,
        record: &AgentRecord,
        status: AgentStatus,
    ) -> Result<()> {
        let installed = matches!(
            status,
            AgentStatus::Installed | AgentStatus::VerificationRequired
        );
        let skill_roots = adapter.skill_roots(&self.config.home);
        let roots_exist = skill_roots.iter().any(|path| path.is_dir());
        let mcp_targets = adapter.mcp_target_specs(&self.config.home);
        let mcp_exist = mcp_targets.iter().any(|target| target.path.exists());
        let plugin_manageable = matches!(
            adapter.plugin_support(),
            PluginSupport::NativeManaged | PluginSupport::FileManaged
        );
        let feature = |feature_key: &str,
                       support_state: &str,
                       evidence_level: &str,
                       notes: String| {
            crate::db::AdapterEvidenceRow {
                id: util::new_id(),
                agent_key: record.key.clone(),
                feature_key: feature_key.to_string(),
                support_state: support_state.to_string(),
                evidence_level: evidence_level.to_string(),
                source_url: Some("docs/evidence/phase0/agents/07_FEATURE_PROBES.md".to_string()),
                tested_at: Some(util::now()),
                agent_version: None,
                adapter_version: crate::adapters::suite_version().to_string(),
                notes: Some(notes),
            }
        };
        let mut rows = Vec::new();
        rows.push(feature(
            "detect",
            if installed { "verified_config" } else { "unverified" },
            if installed { "verified_config" } else { "unverified" },
            format!("detection status: {:?}", status),
        ));
        rows.push(feature(
            "read_skills",
            if roots_exist { "verified_config" } else { "unverified" },
            if roots_exist { "verified_config" } else { "unverified" },
            format!("{} candidate skill root(s)", skill_roots.len()),
        ));
        rows.push(feature(
            "read_mcp",
            if mcp_exist { "verified_config" } else { "unverified" },
            if mcp_exist { "verified_config" } else { "unverified" },
            format!("{} candidate MCP target(s)", mcp_targets.len()),
        ));
        rows.push(feature(
            "read_plugins",
            if plugin_manageable { "verified_config" } else { "unverified" },
            if plugin_manageable { "verified_config" } else { "unverified" },
            format!("plugin support: {:?}", adapter.plugin_support()),
        ));
        for key in [
            "write_skills",
            "write_mcp",
            "read_rules",
            "write_rules",
            "model_config",
            "manage_plugins",
            "execute",
            "observe_usage",
            "hot_reload",
        ] {
            rows.push(feature(key, "unverified", "unverified", "not verified on this machine".to_string()));
        }
        for row in rows {
            self.db.upsert_adapter_evidence(&row)?;
        }
        Ok(())
    }

    /// Per-feature capability evidence for one agent instance.
    pub fn agent_inspect(&self, agent_id: &str) -> Result<AgentInspection> {
        let row = self
            .db
            .get_agent(agent_id)?
            .ok_or_else(|| Error::not_found(format!("agent {agent_id} does not exist")))?;
        let capabilities = self.db.list_adapter_evidence(&row.key)?;
        Ok(AgentInspection {
            agent_id: row.id.clone(),
            agent_key: row.key.clone(),
            capabilities,
            inspected_at: util::now(),
        })
    }

    pub fn list_operation_steps(&self, operation_id: &str) -> Result<Vec<OperationStepView>> {
        self.db.list_operation_steps(operation_id)
    }

    pub fn integrity_report(&self) -> Result<IntegrityReport> {
        self.db.integrity_check()
    }

    /// Recover a failed/partial operation by restoring only journals whose
    /// current content still matches what this operation wrote.
    pub fn recover_operation(&self, operation_id: &str) -> Result<RecoveryReport> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let executor = Executor {
            db: &self.db,
            library: &self.library()?,
        };
        executor.recover(operation_id)
    }

    pub fn list_agents(&self) -> Result<Vec<AgentView>> {
        self.sync_agents()?;
        let project_roots = self.project_roots()?;
        let home = &self.config.home;
        let mut out = Vec::new();
        for row in self.db.list_agents()? {
            let adapter = find_adapter(&row.adapter_id)
                .or_else(|| find_adapter(&row.key))
                .ok_or_else(|| Error::msg(format!("unknown adapter '{}'", row.adapter_id)))?;
            let detection = adapter.detect(home);
            let record = row.to_model();
            let mut roots = adapter.resolved_skill_roots(home, Some(&record));
            for project in &project_roots {
                for path in adapter.project_skill_roots(project) {
                    if roots.iter().any(|root| root.path == path) {
                        continue;
                    }
                    roots.push(crate::adapters::SkillRootSpec {
                        path,
                        scope: Scope::Project,
                        source: "project".to_string(),
                    });
                }
            }
            let scan_rows = self.db.list_scan_results(&row.id)?;
            let mut counts: HashMap<String, usize> = HashMap::new();
            for scan_row in &scan_rows {
                if let Ok(discovered) =
                    serde_json::from_str::<DiscoveredSkill>(&scan_row.payload)
                {
                    *counts.entry(discovered.root).or_insert(0) += 1;
                }
            }
            let skill_roots: Vec<SkillRoot> = roots
                .iter()
                .map(|root| {
                    root.to_model(
                        counts
                            .get(root.path.to_string_lossy().as_ref())
                            .copied()
                            .unwrap_or(0),
                    )
                })
                .collect();
            let mut mcp_specs = adapter.resolved_mcp_targets(home);
            for project in &project_roots {
                for spec in adapter.project_mcp_target_specs(project) {
                    if !mcp_specs.iter().any(|item| item.path == spec.path) {
                        mcp_specs.push(spec);
                    }
                }
            }
            let mcp_targets: Vec<McpTarget> =
                mcp_specs.iter().map(|spec| spec.to_model()).collect();
            let installations = self.db.list_installations_for_agent(&row.id)?;
            out.push(AgentView {
                id: row.id.clone(),
                key: row.key.clone(),
                name: row.name.clone(),
                vendor: row.vendor.clone(),
                adapter_id: row.adapter_id.clone(),
                status: detection.status,
                app_path: row.app_path.clone(),
                home_path: row.home_path.clone(),
                custom_skill_roots: row.custom_skill_roots.clone(),
                detected_at: Some(row.updated_at.clone()),
                last_scan_at: self.db.latest_scan_at(&row.id)?,
                skill_roots,
                mcp_targets,
                plugin_support: adapter.plugin_support(),
                symlink_support: adapter.symlink_support(),
                notes: detection.notes,
                skill_count: scan_rows
                    .iter()
                    .filter(|row| row.status == "ok")
                    .count(),
                installed_skill_count: installations.len(),
            });
        }
        Ok(out)
    }

    pub fn update_agent_paths(&self, update: AgentPathUpdate) -> Result<AgentView> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let row = self
            .db
            .get_agent(&update.agent_id)?
            .ok_or_else(|| Error::not_found("agent does not exist"))?;
        let mut record = row.to_model();
        if let Some(app_path) = &update.app_path {
            if !app_path.trim().is_empty() {
                record.app_path = Some(app_path.clone());
            }
        }
        if let Some(home_path) = &update.home_path {
            if !home_path.trim().is_empty() {
                record.home_path = Some(home_path.clone());
            }
        }
        for root in &update.custom_skill_roots {
            let path = PathBuf::from(root);
            if !path.is_absolute() {
                return Err(Error::msg(format!(
                    "custom skill root must be an absolute path: {root}"
                )));
            }
        }
        record.custom_skill_roots = update.custom_skill_roots;
        record.updated_at = util::now();
        self.db.update_agent_paths(&record)?;
        self.list_agents()?
            .into_iter()
            .find(|agent| agent.id == update.agent_id)
            .ok_or_else(|| Error::not_found("agent does not exist"))
    }

    pub fn project_roots(&self) -> Result<Vec<PathBuf>> {
        let raw = self
            .db
            .get_setting("project_roots")?
            .unwrap_or_else(|| "[]".to_string());
        let paths: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        Ok(paths.into_iter().map(PathBuf::from).collect())
    }

    pub fn set_project_roots(&self, roots: Vec<String>) -> Result<Vec<String>> {
        let mut valid = Vec::new();
        for root in roots {
            let path = PathBuf::from(&root);
            if !path.is_dir() {
                return Err(Error::not_found(format!(
                    "project root does not exist: {}",
                    path.display()
                )));
            }
            valid.push(path.to_string_lossy().to_string());
        }
        valid.sort();
        valid.dedup();
        self.db
            .set_setting("project_roots", &serde_json::to_string(&valid)?)?;
        Ok(valid)
    }

    // -- settings ----------------------------------------------------------

    pub fn settings(&self) -> Result<AppSettings> {
        let library_root = self
            .db
            .get_setting("library_root")?
            .unwrap_or_else(|| self.config.default_library_root.to_string_lossy().to_string());
        let strategy = InstallStrategy::parse(
            &self
                .db
                .get_setting("default_strategy")?
                .unwrap_or_else(|| "copy".to_string()),
        );
        Ok(AppSettings {
            library_root,
            default_strategy: strategy,
            language: self
                .db
                .get_setting("language")?
                .unwrap_or_else(|| "zh".to_string()),
            theme: self
                .db
                .get_setting("theme")?
                .unwrap_or_else(|| "system".to_string()),
            scan_on_startup: self
                .db
                .get_setting("scan_on_startup")?
                .map(|value| value == "true")
                .unwrap_or(true),
            git_import_enabled: self
                .db
                .get_setting("git_import_enabled")?
                .map(|value| value == "true")
                .unwrap_or(true),
            keychain_available: Path::new("/usr/bin/security").exists(),
            database_path: self.db.path().to_string_lossy().to_string(),
            app_version: APP_VERSION.to_string(),
            initialized: self
                .db
                .get_setting("initialized")?
                .map(|value| value == "true")
                .unwrap_or(false),
        })
    }

    pub fn update_settings(&self, update: SettingsUpdate) -> Result<AppSettings> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        if let Some(root) = &update.library_root {
            self.change_library_root(Path::new(root))?;
        }
        if let Some(strategy) = update.default_strategy {
            self.db.set_setting("default_strategy", strategy.as_str())?;
        }
        if let Some(language) = &update.language {
            if language != "zh" && language != "en" && language != "system" {
                return Err(Error::msg("language must be zh, en or system"));
            }
            self.db.set_setting("language", language)?;
        }
        if let Some(theme) = &update.theme {
            if !["system", "light", "dark"].contains(&theme.as_str()) {
                return Err(Error::msg("theme must be system, light or dark"));
            }
            self.db.set_setting("theme", theme)?;
        }
        if let Some(scan_on_startup) = update.scan_on_startup {
            self.db
                .set_setting("scan_on_startup", if scan_on_startup { "true" } else { "false" })?;
        }
        self.settings()
    }

    pub fn mark_initialized(&self) -> Result<AppSettings> {
        self.db.set_setting("initialized", "true")?;
        self.settings()
    }

    fn change_library_root(&self, new_root: &Path) -> Result<()> {
        let current = self.library()?.root().to_path_buf();
        if util::same_path(&current, new_root) {
            return Ok(());
        }
        if new_root.starts_with(&current) || current.starts_with(new_root) {
            return Err(Error::msg(
                "the new library location cannot be inside the current one (or vice versa)",
            ));
        }
        library::validate_new_root(new_root)?;
        let new_library = Library::new(new_root);
        new_library.init()?;
        if current.exists() {
            let has_content = fs::read_dir(&current)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false);
            if has_content {
                util::copy_dir(&current, new_root)?;
            }
        }
        self.db
            .set_setting("library_root", &new_root.to_string_lossy())?;
        Ok(())
    }

    // -- scanning ----------------------------------------------------------

    pub fn scan(&self) -> Result<ScanReport> {
        self.sync_agents()?;
        let home = self.config.home.clone();
        let project_roots = self.project_roots()?;
        let mut agents: Vec<(AgentRecord, Box<dyn AgentAdapter>)> = Vec::new();
        for row in self.db.list_agents()? {
            let adapter = find_adapter(&row.adapter_id)
                .or_else(|| find_adapter(&row.key))
                .ok_or_else(|| Error::msg(format!("unknown adapter '{}'", row.adapter_id)))?;
            agents.push((row.to_model(), adapter));
        }
        let report = scan::scan_agents(&self.db, &agents, &home, &project_roots)?;
        self.db.set_setting("last_scan_at", &report.scanned_at)?;
        Ok(report)
    }

    // -- skills ------------------------------------------------------------

    pub fn list_skills(&self) -> Result<Vec<Skill>> {
        let mut out = Vec::new();
        for capability in self.db.list_capabilities("skill")? {
            out.push(self.build_skill(&capability)?);
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    fn build_skill(&self, capability: &crate::db::CapabilityRow) -> Result<Skill> {
        let Some(current_version_id) = capability.current_version_id.clone() else {
            return Err(Error::msg(format!(
                "skill {} has no current version",
                capability.name
            )));
        };
        let version = self
            .db
            .get_version(&current_version_id)?
            .ok_or_else(|| Error::not_found("current version is missing"))?;
        let installations = self.db.list_installations_for_capability(&capability.id)?;
        let mut views = Vec::new();
        for row in &installations {
            views.push(self.installation_view(row)?);
        }
        let drift_count = views.iter().filter(|view| view.drift).count();
        let update_available = installations
            .iter()
            .any(|row| row.version_id.as_deref() != Some(current_version_id.as_str()));
        Ok(Skill {
            id: capability.id.clone(),
            slug: capability.slug.clone(),
            name: capability.name.clone(),
            description: capability.description.clone(),
            version: version.version_label.clone(),
            content_hash: version.content_hash.clone(),
            tags: capability.tags.clone(),
            license: capability.license.clone(),
            source: self.db.get_source(&capability.id)?,
            current_version_id,
            size_bytes: version.size_bytes,
            file_count: version.file_count,
            created_at: capability.created_at.clone(),
            updated_at: capability.updated_at.clone(),
            installations: views,
            update_available,
            drift_count,
            conflict: None,
        })
    }

    pub fn get_skill(&self, id: &str) -> Result<SkillDetail> {
        let capability = self
            .db
            .get_capability(id)?
            .ok_or_else(|| Error::not_found("skill does not exist"))?;
        if capability.kind != "skill" {
            return Err(Error::msg("capability is not a skill"));
        }
        let skill = self.build_skill(&capability)?;
        let versions = self
            .db
            .list_versions(&capability.id)?
            .into_iter()
            .map(|row| self.version_model(&row, &skill.current_version_id))
            .collect();
        let current = self
            .db
            .get_version(&skill.current_version_id)?
            .ok_or_else(|| Error::not_found("current version is missing"))?;
        let manifest: serde_json::Value =
            serde_json::from_str(&current.manifest).unwrap_or(serde_json::json!({}));
        let files = skill::find_skill_dirs(Path::new(&current.storage_path));
        let file_entries = if files.len() == 1 {
            skill::read_skill_dir(&files[0])
                .map(|parsed| parsed.files)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(SkillDetail {
            skill,
            versions,
            files: file_entries,
            skill_md: current.skill_md.clone(),
            manifest,
        })
    }

    fn version_model(
        &self,
        row: &crate::db::VersionRow,
        current_version_id: &str,
    ) -> CapabilityVersion {
        CapabilityVersion {
            id: row.id.clone(),
            capability_id: row.capability_id.clone(),
            version_label: row.version_label.clone(),
            content_hash: row.content_hash.clone(),
            storage_path: row.storage_path.clone(),
            size_bytes: row.size_bytes,
            file_count: row.file_count,
            notes: row.notes.clone(),
            created_at: row.created_at.clone(),
            is_current: row.id == current_version_id,
        }
    }

    fn installation_view(&self, row: &InstallationRow) -> Result<InstallationView> {
        let agent = self
            .db
            .get_agent(&row.agent_id)?
            .ok_or_else(|| Error::not_found("installation agent is missing"))?;
        let version = match &row.version_id {
            Some(version_id) => self.db.get_version(version_id)?,
            None => None,
        };
        let target = PathBuf::from(&row.target_path);
        let current = current_target_hash(&target);
        let mut warnings = Vec::new();
        let missing = fs::symlink_metadata(&target).is_err();
        let drift = if missing {
            false
        } else if row.kind == "mcp" {
            // For MCP, content_hash covers the managed entry while metadata
            // stores the hash of the whole config file at write time.
            let file_hash = serde_json::from_str::<serde_json::Value>(&row.metadata)
                .ok()
                .and_then(|value| value.get("fileHash").and_then(|hash| hash.as_str()).map(str::to_string));
            match (current, file_hash) {
                (Some(current), Some(recorded)) => current != recorded,
                _ => false,
            }
        } else {
            match (&current, &row.content_hash) {
                (Some(current), Some(recorded)) => current != recorded,
                (None, _) => false,
                (Some(_), None) => true,
            }
        };
        if missing {
            warnings.push("The deployed directory no longer exists.".to_string());
        } else if drift {
            warnings.push("The deployed content was modified outside AgentHub.".to_string());
        }
        Ok(InstallationView {
            id: row.id.clone(),
            capability_id: row.capability_id.clone().unwrap_or_default(),
            version_id: row.version_id.clone().unwrap_or_default(),
            version_label: version
                .as_ref()
                .map(|version| version.version_label.clone())
                .unwrap_or_default(),
            agent_id: row.agent_id.clone(),
            agent_key: agent.key.clone(),
            agent_name: agent.name.clone(),
            scope: Scope::parse(&row.scope),
            strategy: InstallStrategy::parse(&row.strategy),
            target_path: row.target_path.clone(),
            state: if missing {
                InstallationState::Missing
            } else if drift {
                InstallationState::Conflict
            } else {
                InstallationState::parse(&row.state)
            },
            content_hash: row.content_hash.clone(),
            installed_at: row.installed_at.clone(),
            verified_at: row.verified_at.clone(),
            last_check_at: row.last_check_at.clone(),
            drift,
            warnings,
        })
    }

    // -- import ------------------------------------------------------------

    pub fn import_skill(&self, request: ImportRequest) -> Result<ImportResult> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let library = self.library()?;
        library.init()?;
        let staging = StagingDir::create(&library.staging_root())?;
        let mut warnings = Vec::new();

        let (source_dir, source_type, uri, reference) = match &request.source {
            ImportSource::Folder { path } => {
                let dir = self.resolve_skill_folder(Path::new(path), &mut warnings)?;
                (
                    dir,
                    "folder".to_string(),
                    path.clone(),
                    None::<String>,
                )
            }
            ImportSource::AgentDir { agent_id, path } => {
                let agent = self
                    .db
                    .get_agent(agent_id)?
                    .ok_or_else(|| Error::not_found("agent does not exist"))?;
                let dir = self.resolve_skill_folder(Path::new(path), &mut warnings)?;
                (
                    dir,
                    "agent_dir".to_string(),
                    path.clone(),
                    Some(agent.key),
                )
            }
            ImportSource::SkillMd { path } => {
                let file = Path::new(path);
                if !file.is_file() {
                    return Err(Error::not_found(format!(
                        "SKILL.md does not exist: {}",
                        file.display()
                    )));
                }
                let dir = staging.path().join("single");
                fs::create_dir_all(&dir)?;
                fs::copy(file, dir.join("SKILL.md"))?;
                (dir, "skill_md".to_string(), path.clone(), None)
            }
            ImportSource::Archive { path } => {
                let archive = Path::new(path);
                if !archive.is_file() {
                    return Err(Error::not_found(format!(
                        "archive does not exist: {}",
                        archive.display()
                    )));
                }
                let extracted = skill::extract_archive(archive, staging.path())?;
                let candidates = skill::find_skill_dirs_recursive(&extracted, 4);
                let dir = self.pick_candidate(candidates, request.skill_path.as_deref(), &extracted)?;
                (dir, "zip".to_string(), path.clone(), None)
            }
            ImportSource::Git { url, reference } => {
                if !self
                    .db
                    .get_setting("git_import_enabled")?
                    .map(|value| value == "true")
                    .unwrap_or(true)
                {
                    return Err(Error::unsupported("git import is disabled in settings"));
                }
                let repo = skill::clone_git(url, reference.as_deref(), staging.path())?;
                let candidates = skill::find_skill_dirs_recursive(&repo, 3);
                let dir = self.pick_candidate(candidates, request.skill_path.as_deref(), &repo)?;
                (dir, "git".to_string(), url.clone(), reference.clone())
            }
        };

        let parsed = skill::read_skill_dir(&source_dir)?;
        warnings.extend(parsed.issues.iter().cloned());
        let slug = parsed.slug();
        if slug.is_empty() {
            return Err(Error::invalid("skill name does not produce a usable slug"));
        }
        let now = util::now();
        let storage_path = library.store_version(&slug, &parsed.content_hash, &source_dir)?;
        let storage_string = storage_path.to_string_lossy().to_string();

        let existing = self.db.find_capability("skill", &slug)?;
        let (capability_id, version_id, created) = match &existing {
            Some(capability) => {
                let capability_id = capability.id.clone();
                if let Some(version) =
                    self.db
                        .find_version_by_hash(&capability_id, &parsed.content_hash)?
                {
                    warnings.push("This exact content is already in the library.".to_string());
                    let skill = self.build_skill(capability)?;
                    return Ok(ImportResult {
                        skill,
                        version: self.version_model(&version, &version.id),
                        created: false,
                        warnings,
                    });
                }
                warnings.push(format!(
                    "A different version of '{}' already exists; importing as a new version (nothing was overwritten).",
                    capability.slug
                ));
                let version_id = util::new_id();
                self.db.insert_version(
                    &crate::db::VersionRow {
                        id: version_id.clone(),
                        capability_id: capability_id.clone(),
                        version_label: request
                            .version_label
                            .clone()
                            .unwrap_or_else(|| parsed.version_label()),
                        content_hash: parsed.content_hash.clone(),
                        storage_path: storage_string.clone(),
                        manifest: serde_json::to_string(&parsed.manifest)?,
                        skill_md: parsed.skill_md.clone(),
                        size_bytes: parsed.size_bytes,
                        file_count: parsed.file_count,
                        notes: request.notes.clone(),
                        created_at: now.clone(),
                    },
                    capability.current_version_id.as_deref(),
                )?;
                self.db.update_capability_meta(
                    &capability_id,
                    &parsed.frontmatter.name,
                    &parsed.frontmatter.description,
                    Some(&version_id),
                    parsed.frontmatter.license.as_deref(),
                )?;
                self.db.insert_source(
                    &capability_id,
                    &source_type,
                    &uri,
                    reference.as_deref(),
                )?;
                (capability_id, version_id, true)
            }
            None => {
                let capability_id = util::new_id();
                let version_id = util::new_id();
                self.db.insert_capability(&crate::db::CapabilityRow {
                    id: capability_id.clone(),
                    kind: "skill".to_string(),
                    slug: slug.clone(),
                    name: parsed.frontmatter.name.clone(),
                    description: parsed.frontmatter.description.clone(),
                    current_version_id: Some(version_id.clone()),
                    tags: parsed.frontmatter.tags.clone(),
                    license: parsed.frontmatter.license.clone(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                })?;
                self.db.insert_version(
                    &crate::db::VersionRow {
                        id: version_id.clone(),
                        capability_id: capability_id.clone(),
                        version_label: request
                            .version_label
                            .clone()
                            .unwrap_or_else(|| parsed.version_label()),
                        content_hash: parsed.content_hash.clone(),
                        storage_path: storage_string,
                        manifest: serde_json::to_string(&parsed.manifest)?,
                        skill_md: parsed.skill_md.clone(),
                        size_bytes: parsed.size_bytes,
                        file_count: parsed.file_count,
                        notes: request.notes.clone(),
                        created_at: now.clone(),
                    },
                    None,
                )?;
                self.db.insert_source(
                    &capability_id,
                    &source_type,
                    &uri,
                    reference.as_deref(),
                )?;
                (capability_id, version_id, true)
            }
        };
        let capability = self
            .db
            .get_capability(&capability_id)?
            .ok_or_else(|| Error::not_found("imported capability is missing"))?;
        let skill = self.build_skill(&capability)?;
        let version = self
            .db
            .get_version(&version_id)?
            .ok_or_else(|| Error::not_found("imported version is missing"))?;
        let operation_id = util::new_id();
        let plan = OperationPlan {
            operation_id: operation_id.clone(),
            kind: OperationKind::Import,
            dry_run: false,
            items: Vec::new(),
            warnings: warnings.clone(),
            requires_restart_agents: Vec::new(),
            requires_confirmation: false,
            summary: PlanSummary {
                create: 0,
                replace: 0,
                remove: 0,
                noop: 0,
                conflict: 0,
                blocked: 0,
            },
            created_at: util::now(),
            plan_hash: String::new(),
            expires_at: String::new(),
            approval_state: "none".to_string(),
        };
        self.db.insert_operation(
            &operation_id,
            "import",
            false,
            &format!("Import {} {}", skill.slug, version.version_label),
            &serde_json::to_string(&plan)?,
            None,
        )?;
        let result = OperationResult {
            operation_id: operation_id.clone(),
            status: OperationStatus::Success,
            items: vec![ItemResult {
                plan_item_id: String::new(),
                agent_key: String::new(),
                target_path: version.storage_path.clone(),
                status: "success".to_string(),
                message: "Stored in the central library".to_string(),
                backup_path: None,
                installation_id: None,
            }],
            diagnostics: warnings.clone(),
            finished_at: Some(util::now()),
        };
        self.db.update_operation_status(
            &operation_id,
            "success",
            &serde_json::to_string(&result)?,
            None,
        )?;
        crate::ops::audit_log(
            &library,
            "import",
            serde_json::json!({
                "capabilityId": skill.id,
                "slug": skill.slug,
                "version": version.version_label,
                "hash": version.content_hash,
            }),
        );
        Ok(ImportResult {
            skill,
            version: self.version_model(&version, &version_id),
            created,
            warnings,
        })
    }

    /// Adopt skills found by a previous scan into the central library.
    ///
    /// Only valid, deduplicated discovered paths are imported. Read-only roots
    /// (built-in/marketplace/workspace/connector caches) are skipped unless the
    /// caller explicitly opts in, because importing hundreds of marketplace
    /// entries is rarely what the user wants.
    pub fn import_scan_skills(&self, request: ScanImportRequest) -> Result<ScanImportResult> {
        self.sync_agents()?;
        let agents = self.db.list_agents()?;
        let mut candidates: Vec<(AgentRow, DiscoveredSkill)> = Vec::new();
        for agent in &agents {
            if let Some(filter) = &request.agent_ids {
                if !filter.iter().any(|id| id == &agent.id) {
                    continue;
                }
            }
            for row in self.db.list_scan_results(&agent.id)? {
                if row.status != "ok" {
                    continue;
                }
                let Ok(discovered) = serde_json::from_str::<DiscoveredSkill>(&row.payload) else {
                    continue;
                };
                if !discovered.valid {
                    continue;
                }
                if let Some(paths) = &request.paths {
                    if !paths.iter().any(|path| path == &discovered.path) {
                        continue;
                    }
                }
                candidates.push((agent.clone(), discovered));
            }
        }
        if candidates.is_empty() {
            return Err(Error::msg(
                "no matching scan results; run a scan first and select discovered skills",
            ));
        }
        let total_candidates = candidates.len();
        let max_items = request.max_items.unwrap_or(500).max(1);
        let read_only_sources = ["builtin", "marketplace", "workspace", "connector"];
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut result = ScanImportResult {
            imported: Vec::new(),
            imported_count: 0,
            skipped_existing: 0,
            skipped_duplicate: 0,
            skipped_read_only: 0,
            skipped_limit: 0,
            failed: Vec::new(),
            total_candidates,
            finished_at: util::now(),
        };
        for (agent, discovered) in candidates {
            let dedupe_key = (discovered.slug.clone(), discovered.content_hash.clone());
            if !seen.insert(dedupe_key) {
                result.skipped_duplicate += 1;
                continue;
            }
            if !request.include_read_only_sources
                && read_only_sources.contains(&discovered.root_source.as_str())
            {
                result.skipped_read_only += 1;
                continue;
            }
            if let Some(capability) = self.db.find_capability("skill", &discovered.slug)? {
                if self
                    .db
                    .find_version_by_hash(&capability.id, &discovered.content_hash)?
                    .is_some()
                {
                    result.skipped_existing += 1;
                    continue;
                }
            }
            if result.imported.len() + result.failed.len() >= max_items {
                result.skipped_limit += 1;
                continue;
            }
            if request.dry_run {
                result.imported.push(ScanImportedItem {
                    slug: discovered.slug.clone(),
                    name: discovered.name.clone(),
                    version_label: discovered
                        .version
                        .clone()
                        .unwrap_or_else(|| "0.0.0".to_string()),
                    agent_key: Some(agent.key.clone()),
                    path: discovered.path.clone(),
                    size_bytes: discovered.size_bytes,
                });
                continue;
            }
            match self.import_skill(ImportRequest {
                source: ImportSource::AgentDir {
                    agent_id: agent.id.clone(),
                    path: discovered.path.clone(),
                },
                skill_path: None,
                version_label: discovered.version.clone(),
                notes: Some(format!(
                    "Adopted from {} at {}",
                    agent.key, discovered.path
                )),
            }) {
                Ok(imported) => {
                    result.imported.push(ScanImportedItem {
                        slug: imported.skill.slug.clone(),
                        name: imported.skill.name.clone(),
                        version_label: imported.version.version_label.clone(),
                        agent_key: Some(agent.key.clone()),
                        path: discovered.path.clone(),
                        size_bytes: imported.version.size_bytes,
                    });
                }
                Err(err) => result.failed.push(ScanImportFailure {
                    path: discovered.path.clone(),
                    name: discovered.name.clone(),
                    error: err.to_string(),
                }),
            }
        }
        result.imported_count = result.imported.len();
        result.finished_at = util::now();
        crate::ops::audit_log(
            &self.library()?,
            "skills.adopt_scan",
            serde_json::json!({
                "totalCandidates": total_candidates,
                "imported": result.imported_count,
                "skippedExisting": result.skipped_existing,
                "skippedDuplicate": result.skipped_duplicate,
                "skippedReadOnly": result.skipped_read_only,
                "failed": result.failed.len(),
                "includeReadOnly": request.include_read_only_sources,
            }),
        );
        Ok(result)
    }

    fn resolve_skill_folder(&self, path: &Path, warnings: &mut Vec<String>) -> Result<PathBuf> {
        if !path.is_dir() {
            return Err(Error::not_found(format!(
                "directory does not exist: {}",
                path.display()
            )));
        }
        if path.join("SKILL.md").is_file() {
            return Ok(path.to_path_buf());
        }
        let candidates = skill::find_skill_dirs(path);
        if candidates.len() == 1 {
            return Ok(candidates[0].clone());
        }
        if candidates.is_empty() {
            return Err(Error::invalid(format!(
                "no skill directory with SKILL.md found under {}",
                path.display()
            )));
        }
        warnings.push(format!(
            "{} contains {} skills; import them one at a time.",
            path.display(),
            candidates.len()
        ));
        Err(Error::conflict(format!(
            "directory contains {} skill candidates; pass a specific skill path",
            candidates.len()
        )))
    }

    fn pick_candidate(
        &self,
        candidates: Vec<PathBuf>,
        skill_path: Option<&str>,
        root: &Path,
    ) -> Result<PathBuf> {
        if let Some(relative) = skill_path {
            let candidate = root.join(relative);
            if candidate.join("SKILL.md").is_file() {
                return Ok(candidate);
            }
            return Err(Error::not_found(format!(
                "{} does not contain a SKILL.md",
                candidate.display()
            )));
        }
        match candidates.len() {
            0 => Err(Error::invalid(
                "no skill directory with SKILL.md found in the imported source",
            )),
            1 => Ok(candidates[0].clone()),
            _ => {
                let listing: Vec<String> = candidates
                    .iter()
                    .take(10)
                    .map(|path| {
                        path.strip_prefix(root)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                Err(Error::conflict(format!(
                    "source contains {} skills; specify which one to import: {}",
                    candidates.len(),
                    listing.join(", ")
                )))
            }
        }
    }

    // -- distribution ------------------------------------------------------

    pub fn plan_install(&self, request: InstallRequest) -> Result<OperationPlan> {
        let capability = self
            .db
            .get_capability(&request.capability_id)?
            .ok_or_else(|| Error::not_found("skill does not exist"))?;
        if capability.kind != "skill" {
            return Err(Error::unsupported("only skills can be distributed in this build"));
        }
        let version_id = request
            .version_id
            .clone()
            .or_else(|| capability.current_version_id.clone())
            .ok_or_else(|| Error::msg("skill has no version to install"))?;
        let version = self
            .db
            .get_version(&version_id)?
            .ok_or_else(|| Error::not_found("version does not exist"))?;
        if version.capability_id != capability.id {
            return Err(Error::msg("version does not belong to the skill"));
        }
        if !Path::new(&version.storage_path).is_dir() {
            return Err(Error::Verification(format!(
                "library copy is missing: {}",
                version.storage_path
            )));
        }
        let strategy = request.strategy.unwrap_or_else(|| {
            self.settings()
                .map(|settings| settings.default_strategy)
                .unwrap_or(InstallStrategy::Copy)
        });
        let home = self.config.home.clone();
        let mut items = Vec::new();
        let mut warnings = Vec::new();
        let mut restarts: Vec<String> = Vec::new();

        for agent_id in &request.agent_ids {
            let Some(agent_row) = self.db.get_agent(agent_id)? else {
                warnings.push(format!("Unknown agent id {agent_id} was skipped."));
                continue;
            };
            let adapter = find_adapter(&agent_row.adapter_id)
                .or_else(|| find_adapter(&agent_row.key))
                .ok_or_else(|| Error::msg("unknown adapter"))?;
            let detection = adapter.detect(&home);
            let mut item_warnings = Vec::new();
            let (root, scope) = match self.pick_install_root(&adapter, &agent_row.to_model()) {
                Some(root) => root,
                None => {
                    items.push(self.blocked_item(
                        &capability,
                        &version,
                        agent_row.id.clone(),
                        agent_row.key.clone(),
                        agent_row.name.clone(),
                        format!(
                            "{} has no writable global skill root in AgentHub yet. Add a project skill root on the Agents page first.",
                            agent_row.name
                        ),
                    ));
                    continue;
                }
            };
            let mut chosen_strategy = strategy;
            if chosen_strategy == InstallStrategy::Symlink {
                match adapter.symlink_support() {
                    SymlinkSupport::Unsupported => {
                        chosen_strategy = InstallStrategy::Copy;
                        item_warnings.push(
                            "This agent does not support symlinked skills; copying instead."
                                .to_string(),
                        );
                    }
                    SymlinkSupport::Unverified => item_warnings.push(
                        "Symlink support for this agent is unverified; the link will be created and verified, but agent discovery should be checked manually."
                            .to_string(),
                    ),
                    SymlinkSupport::Supported => {}
                }
            }
            if detection.status != AgentStatus::Installed
                && detection.status != AgentStatus::VerificationRequired
            {
                items.push(self.blocked_item(
                    &capability,
                    &version,
                    agent_row.id.clone(),
                    agent_row.key.clone(),
                    agent_row.name.clone(),
                    format!("{} is not installed on this Mac.", agent_row.name),
                ));
                continue;
            }
            if let Some(note) = adapter.requires_restart_note() {
                restarts.push(format!("{}: {}", agent_row.name, note));
            }
            let target = root.join(&capability.slug);
            let previous = self
                .db
                .find_installation(&agent_row.id, &target.to_string_lossy())?;
            let current = current_target_hash(&target);
            let (action, diff) = if current.is_none() {
                (PlanAction::Create, None)
            } else {
                let recorded = previous
                    .as_ref()
                    .and_then(|row| row.content_hash.clone());
                let desired = Some(version.content_hash.clone());
                match &recorded {
                    Some(recorded_hash) if current.as_deref() == Some(recorded_hash.as_str()) => {
                        if current == desired {
                            (PlanAction::Noop, None)
                        } else {
                            (
                                PlanAction::Replace,
                                Some(format!(
                                    "Replace version {} with {}",
                                    previous
                                        .as_ref()
                                        .and_then(|row| row.version_id.clone())
                                        .unwrap_or_default(),
                                    version.version_label
                                )),
                            )
                        }
                    }
                    Some(_) => (
                        PlanAction::Conflict,
                        Some(
                            "The deployed copy was modified outside AgentHub (content hash drifted)."
                                .to_string(),
                        ),
                    ),
                    None => {
                        if current == desired {
                            (
                                PlanAction::Noop,
                                Some(
                                    "Existing identical skill directory is not tracked yet; AgentHub will adopt it."
                                        .to_string(),
                                ),
                            )
                        } else {
                            (
                                PlanAction::Conflict,
                                Some(
                                    "A different skill with the same directory name already exists and is not managed by AgentHub."
                                        .to_string(),
                                ),
                            )
                        }
                    }
                }
            };
            items.push(PlanItem {
                id: util::new_id(),
                capability_id: Some(capability.id.clone()),
                capability_name: capability.name.clone(),
                version_id: Some(version.id.clone()),
                version_label: Some(version.version_label.clone()),
                agent_id: agent_row.id.clone(),
                agent_key: agent_row.key.clone(),
                agent_name: agent_row.name.clone(),
                install_kind: "skill_dir".to_string(),
                action,
                strategy: chosen_strategy,
                target_path: target.to_string_lossy().to_string(),
                source_path: Some(version.storage_path.clone()),
                current_hash: current,
                desired_hash: Some(version.content_hash.clone()),
                config_key: None,
                scope: scope,
                target_format: None,
                payload: None,
                warnings: item_warnings,
                requires_restart: adapter.requires_restart_note().is_some(),
                diff,
                previous_installation: previous
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()?,
            });
            let _ = scope;
        }

        // When this is an update, surface version-to-version impact (scripts,
        // network, dependencies, affected deployments) in the plan itself.
        let mut warnings = warnings;
        let from_version_id = items
            .iter()
            .filter_map(|item| item.previous_installation.as_ref())
            .filter_map(|value| {
                serde_json::from_value::<InstallationRow>(value.clone())
                    .ok()
                    .and_then(|row| row.version_id)
            })
            .find(|version_id| version_id != &version.id);
        if let Some(from_version_id) = from_version_id {
            match self.preview_update(ImpactRequest {
                capability_id: capability.id.clone(),
                from_version_id: Some(from_version_id),
                to_version_id: version.id.clone(),
            }) {
                Ok(impact) => {
                    warnings.push(format!(
                        "Update impact: {} (risk {}, {} affected installation(s), revalidation required: {}).",
                        impact.summary,
                        impact.risk,
                        impact.affected_installation_count,
                        impact.requires_revalidation
                    ));
                    if impact.requires_revalidation {
                        warnings.push(
                            "This update changes scripts, SKILL.md, dependencies or network access. Re-record validation evidence after applying it."
                                .to_string(),
                        );
                    }
                }
                Err(err) => warnings.push(format!(
                    "Could not compute update impact automatically: {err}"
                )),
            }
        }
        let plan = self.persist_plan(
            OperationKind::Install,
            format!("Install {} to {} agent(s)", capability.name, request.agent_ids.len()),
            items,
            warnings,
            restarts,
            request.force,
        )?;
        Ok(plan)
    }

    fn pick_install_root(
        &self,
        adapter: &Box<dyn AgentAdapter>,
        record: &AgentRecord,
    ) -> Option<(PathBuf, Scope)> {
        for custom in &record.custom_skill_roots {
            let path = PathBuf::from(custom);
            if path.is_dir() || writable_ancestor(&path) {
                return Some((path, Scope::Custom));
            }
        }
        for path in adapter.install_roots(&self.config.home) {
            if path.is_dir() || writable_ancestor(&path) {
                return Some((path, Scope::Global));
            }
        }
        None
    }

    fn blocked_item(
        &self,
        capability: &crate::db::CapabilityRow,
        version: &crate::db::VersionRow,
        agent_id: String,
        agent_key: String,
        agent_name: String,
        reason: String,
    ) -> PlanItem {
        PlanItem {
            id: util::new_id(),
            capability_id: Some(capability.id.clone()),
            capability_name: capability.name.clone(),
            version_id: Some(version.id.clone()),
            version_label: Some(version.version_label.clone()),
            agent_id,
            agent_key,
            agent_name,
            install_kind: "skill_dir".to_string(),
            action: PlanAction::Blocked,
            strategy: InstallStrategy::Copy,
            target_path: String::new(),
            source_path: Some(version.storage_path.clone()),
            current_hash: None,
            desired_hash: Some(version.content_hash.clone()),
            config_key: None,
            scope: Scope::Global,
            target_format: None,
            payload: None,
            warnings: vec![reason],
            requires_restart: false,
            diff: None,
            previous_installation: None,
        }
    }

    pub fn plan_uninstall(&self, request: UninstallRequest) -> Result<OperationPlan> {
        let mut items = Vec::new();
        let mut warnings = Vec::new();
        for installation_id in &request.installation_ids {
            let Some(row) = self.db.get_installation(installation_id)? else {
                warnings.push(format!("Installation {installation_id} no longer exists."));
                continue;
            };
            if row.kind != "skill" {
                warnings.push(format!(
                    "Installation {installation_id} is a {} item; remove it from the MCP page instead.",
                    row.kind
                ));
                continue;
            }
            let (Some(capability_id), Some(version_id)) = (&row.capability_id, &row.version_id) else {
                warnings.push(format!("Installation {installation_id} has no capability link."));
                continue;
            };
            let capability = self
                .db
                .get_capability(capability_id)?
                .ok_or_else(|| Error::not_found("capability is missing"))?;
            let agent = self
                .db
                .get_agent(&row.agent_id)?
                .ok_or_else(|| Error::not_found("agent is missing"))?;
            let version = self.db.get_version(version_id)?;
            items.push(PlanItem {
                id: util::new_id(),
                capability_id: Some(capability_id.clone()),
                capability_name: capability.name.clone(),
                version_id: Some(version_id.clone()),
                version_label: version.as_ref().map(|v| v.version_label.clone()),
                agent_id: row.agent_id.clone(),
                agent_key: agent.key.clone(),
                agent_name: agent.name.clone(),
                install_kind: "skill_dir".to_string(),
                action: PlanAction::Remove,
                strategy: InstallStrategy::parse(&row.strategy),
                target_path: row.target_path.clone(),
                source_path: row.source_path.clone(),
                current_hash: current_target_hash(Path::new(&row.target_path)),
                desired_hash: row.content_hash.clone(),
                config_key: Some(row.config_key.clone()),
                scope: Scope::parse(&row.scope),
                target_format: None,
                payload: None,
                warnings: Vec::new(),
                requires_restart: false,
                diff: Some("Remove the deployed skill directory; a snapshot is kept for rollback.".to_string()),
                previous_installation: Some(serde_json::to_value(&row)?),
            });
        }
        if items.is_empty() {
            return Err(Error::not_found("no matching installations to remove"));
        }
        self.persist_plan(
            OperationKind::Uninstall,
            format!("Remove {} installation(s)", items.len()),
            items,
            warnings,
            Vec::new(),
            request.force,
        )
    }

    fn persist_plan(
        &self,
        kind: OperationKind,
        summary: String,
        items: Vec<PlanItem>,
        warnings: Vec<String>,
        restarts: Vec<String>,
        force: bool,
    ) -> Result<OperationPlan> {
        let operation_id = util::new_id();
        let summary_counts = plan_summary(&items);
        let plan_created_at = util::now();
        let plan_expires_at = approval_expiry_from_now(APPROVAL_TTL_MINUTES);
        let plan_hash = plan_digest(&kind, &items);
        let plan = OperationPlan {
            operation_id: operation_id.clone(),
            kind,
            dry_run: true,
            items,
            warnings,
            requires_restart_agents: restarts,
            requires_confirmation: summary_counts.conflict > 0 || force,
            summary: summary_counts,
            created_at: plan_created_at,
            plan_hash: plan_hash.clone(),
            expires_at: plan_expires_at.clone(),
            approval_state: "none".to_string(),
        };
        self.db.insert_operation(
            &operation_id,
            kind.as_str(),
            true,
            &summary,
            &serde_json::to_string(&plan)?,
            None,
        )?;
        self.db
            .set_operation_plan_meta(&operation_id, &plan_hash, &plan_expires_at)?;
        Ok(plan)
    }

    pub fn get_operation(&self, operation_id: &str) -> Result<OperationView> {
        let row = self
            .db
            .get_operation(operation_id)?
            .ok_or_else(|| Error::not_found("operation does not exist"))?;
        self.operation_view(&row)
    }

    pub fn list_operations(&self, limit: usize) -> Result<Vec<OperationView>> {
        let rows = self.db.list_operations(limit)?;
        let mut out = Vec::new();
        for row in &rows {
            out.push(self.operation_view(row)?);
        }
        Ok(out)
    }

    fn operation_view(&self, row: &crate::db::OperationRow) -> Result<OperationView> {
        let plan: Option<OperationPlan> = serde_json::from_str(&row.plan).ok();
        let result: Option<OperationResult> = serde_json::from_str(&row.result)
            .ok()
            .filter(|result: &OperationResult| !result.items.is_empty() || result.status != OperationStatus::Planned);
        let status = OperationStatus::parse(&row.status);
        let has_items = plan
            .as_ref()
            .map(|plan| !plan.items.is_empty())
            .unwrap_or(false);
        let already_rolled_back = self
            .db
            .list_operations(200)?
            .into_iter()
            .any(|op| {
                op.rollback_of.as_deref() == Some(row.id.as_str())
                    && matches!(
                        OperationStatus::parse(&op.status),
                        OperationStatus::Success
                            | OperationStatus::Partial
                            | OperationStatus::Running
                    )
            });
        let approval_state = if row.approval_state == "approved"
            && row
                .approval_expires_at
                .as_deref()
                .map(crate::ops::approval_is_expired)
                .unwrap_or(false)
        {
            "expired".to_string()
        } else {
            row.approval_state.clone()
        };
        let steps = self.db.list_operation_steps(&row.id)?;
        Ok(OperationView {
            id: row.id.clone(),
            kind: OperationKind::parse(&row.kind),
            status,
            dry_run: row.dry_run,
            actor: row.actor.clone(),
            summary: row.summary.clone(),
            error: row.error.clone(),
            rollback_of: row.rollback_of.clone(),
            started_at: row.started_at.clone(),
            finished_at: row.finished_at.clone(),
            item_count: plan.as_ref().map(|plan| plan.items.len()).unwrap_or(0),
            can_rollback: has_items
                && !already_rolled_back
                && matches!(status, OperationStatus::Success | OperationStatus::Partial)
                && !matches!(
                    OperationKind::parse(&row.kind),
                    OperationKind::Rollback
                        | OperationKind::Backup
                        | OperationKind::Import
                        | OperationKind::Scan
                ),
            plan_hash: row.plan_hash.clone(),
            expires_at: row.approval_expires_at.clone(),
            approval_state,
            approved_at: row.approved_at.clone(),
            recovery_state: row.recovery_state.clone(),
            steps,
            plan,
            result,
        })
    }

    /// Read-only preview of the approval state, plan hash and journal.
    pub fn preview_operation(&self, operation_id: &str) -> Result<OperationView> {
        self.get_operation(operation_id)
    }

    /// Issue a one-time approval token bound to `expected_plan_hash`.
    ///
    /// The token itself is returned to the caller exactly once; the database
    /// stores only its SHA-256. A changed plan or an expired deadline forces a
    /// new approval.
    pub fn approve_operation(
        &self,
        operation_id: &str,
        expected_plan_hash: &str,
    ) -> Result<ApprovalGrant> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let row = self
            .db
            .get_operation(operation_id)?
            .ok_or_else(|| Error::not_found("operation does not exist"))?;
        let status = OperationStatus::parse(&row.status);
        if status != OperationStatus::Planned {
            return Err(Error::conflict(format!(
                "operation is in state '{}' and cannot be approved",
                row.status
            )));
        }
        let plan: OperationPlan = serde_json::from_str(&row.plan)
            .map_err(|err| Error::msg(format!("operation plan is unreadable: {err}")))?;
        let current_hash = plan_digest(&plan.kind, &plan.items);
        let stored_hash = row
            .plan_hash
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| current_hash.clone());
        if expected_plan_hash != stored_hash || stored_hash != current_hash {
            return Err(Error::ExternalModification(
                "plan hash changed; re-preview the operation and approve again".to_string(),
            ));
        }
        let expires_at = approval_expiry_from_now(APPROVAL_TTL_MINUTES);
        let token = crate::ops::new_approval_token();
        let token_hash = crate::ops::approval_token_hash(&token);
        let approved_at = util::now();
        self.db.set_operation_approval(
            operation_id,
            &token_hash,
            "local-user",
            &approved_at,
            &expires_at,
        )?;
        self.db.insert_audit_event(
            "local-user",
            "operation.approved",
            Some(operation_id),
            &serde_json::json!({
                "planHash": stored_hash,
                "expiresAt": expires_at,
            }),
        )?;
        Ok(ApprovalGrant {
            operation_id: operation_id.to_string(),
            plan_hash: stored_hash,
            approval_token: token,
            approved_at,
            expires_at,
        })
    }

    /// Apply an approved operation. Verifies the bearer token, deadline, plan
    /// hash and (in the executor) the on-disk CAS hashes before writing.
    pub fn apply_operation_approved(
        &self,
        operation_id: &str,
        approval_token: &str,
        idempotency_key: Option<&str>,
    ) -> Result<OperationResult> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        self.apply_operation_approved_locked(operation_id, approval_token, idempotency_key, false)
    }

    fn apply_operation_approved_locked(
        &self,
        operation_id: &str,
        approval_token: &str,
        idempotency_key: Option<&str>,
        force: bool,
    ) -> Result<OperationResult> {
        let row = self
            .db
            .get_operation(operation_id)?
            .ok_or_else(|| Error::not_found("operation does not exist"))?;
        let status = OperationStatus::parse(&row.status);
        if let Some(key) = idempotency_key {
            if let Some(existing) = self.db.find_operation_by_idempotency(key)? {
                if existing.id == operation_id {
                    let result: OperationResult = serde_json::from_str(&existing.result)
                        .map_err(|err| Error::msg(format!("stored result is unreadable: {err}")))?;
                    if matches!(
                        result.status,
                        OperationStatus::Success | OperationStatus::Partial
                    ) {
                        return Ok(result);
                    }
                } else {
                    return Err(Error::conflict(
                        "idempotency key is already used by another operation",
                    ));
                }
            }
        }
        if !matches!(
            status,
            OperationStatus::Planned | OperationStatus::Running
        ) {
            return Err(Error::conflict(format!(
                "operation is in state '{}' and cannot be applied",
                row.status
            )));
        }
        let plan: OperationPlan = serde_json::from_str(&row.plan)
            .map_err(|err| Error::msg(format!("operation plan is unreadable: {err}")))?;
        let current_hash = plan_digest(&plan.kind, &plan.items);
        if row.plan_hash.as_deref().unwrap_or("") != current_hash {
            return Err(Error::ExternalModification(
                "plan content changed since it was persisted; re-preview required".to_string(),
            ));
        }
        let stored_token_hash = row.approval_token_hash.as_deref().unwrap_or("");
        if stored_token_hash.is_empty() {
            return Err(Error::conflict(
                "operation has no approval; call operations_approve first",
            ));
        }
        let presented = crate::ops::approval_token_hash(approval_token);
        if !constant_time_eq(stored_token_hash.as_bytes(), presented.as_bytes()) {
            return Err(Error::Blocked(
                "approval token does not match this operation".to_string(),
            ));
        }
        let expires_at = row.approval_expires_at.as_deref().unwrap_or("");
        if crate::ops::approval_is_expired(expires_at) {
            return Err(Error::conflict(
                "approval expired; request a new approval token",
            ));
        }
        self.db.set_operation_state(
            operation_id,
            "running",
            None,
            idempotency_key,
        )?;
        let executor = Executor {
            db: &self.db,
            library: &self.library()?,
        };
        executor.apply(&plan, force)
    }

    /// Legacy/trusted entry point used by the existing CLI `--yes` flow and by
    /// tests. It still records an explicit approval bound to the plan hash so
    /// every external write remains auditable; new callers should use
    /// `approve_operation` + `apply_operation_approved`.
    pub fn apply_operation(&self, operation_id: &str, force: bool) -> Result<OperationResult> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let row = self
            .db
            .get_operation(operation_id)?
            .ok_or_else(|| Error::not_found("operation does not exist"))?;
        let status = OperationStatus::parse(&row.status);
        if status != OperationStatus::Planned {
            return Err(Error::conflict(format!(
                "operation is in state '{}' and cannot be applied again",
                row.status
            )));
        }
        let plan: OperationPlan = serde_json::from_str(&row.plan)?;
        let plan_hash = plan_digest(&plan.kind, &plan.items);
        let token = crate::ops::new_approval_token();
        let expires_at = approval_expiry_from_now(APPROVAL_TTL_MINUTES);
        self.db.set_operation_approval(
            operation_id,
            &crate::ops::approval_token_hash(&token),
            if force { "local-user(force)" } else { "local-user(yes)" },
            &util::now(),
            &expires_at,
        )?;
        self.db
            .set_operation_plan_meta(operation_id, &plan_hash, &expires_at)?;
        self.db.insert_audit_event(
            "local-user",
            "operation.auto_approved",
            Some(operation_id),
            &serde_json::json!({ "planHash": plan_hash, "force": force }),
        )?;
        self.apply_operation_approved_locked(operation_id, &token, None, force)
    }

    pub fn rollback_operation(&self, operation_id: &str) -> Result<OperationResult> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let executor = Executor {
            db: &self.db,
            library: &self.library()?,
        };
        executor.rollback(operation_id)
    }

    pub fn list_snapshots(&self, operation_id: &str) -> Result<Vec<SnapshotView>> {
        Executor {
            db: &self.db,
            library: &self.library()?,
        }
        .list_snapshots(operation_id)
    }

    pub fn list_snapshots_all(&self, limit: usize) -> Result<Vec<SnapshotView>> {
        Ok(self
            .db
            .list_all_snapshots(limit)?
            .into_iter()
            .map(|row| SnapshotView {
                id: row.id,
                operation_id: row.operation_id,
                kind: row.kind,
                original_path: row.original_path,
                backup_path: row.backup_path,
                existed: row.existed,
                hash_before: row.hash_before,
                created_at: row.created_at,
            })
            .collect())
    }

    pub fn create_backup(&self) -> Result<BackupResult> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        self.library()?.create_backup(&self.db, APP_VERSION)
    }

    pub fn list_backups(&self) -> Result<Vec<BackupView>> {
        self.library()?.list_backups()
    }

    // -- change detection --------------------------------------------------

    pub fn check_changes(&self) -> Result<ChangeCheck> {
        let mut drifted = Vec::new();
        let mut missing = Vec::new();
        let mut warnings = Vec::new();
        for row in self.db.list_installations()? {
            if row.kind != "skill" {
                continue;
            }
            let path = Path::new(&row.target_path);
            let current = current_target_hash(path);
            let view = self.installation_view(&row)?;
            if fs::symlink_metadata(path).is_err() {
                missing.push(view);
                continue;
            }
            match (&current, &row.content_hash) {
                (Some(current), Some(recorded)) if current != recorded => {
                    drifted.push(view);
                }
                (Some(_), None) => drifted.push(view),
                _ => {}
            }
        }
        if !drifted.is_empty() {
            warnings.push(format!(
                "{} deployed skill(s) were modified outside AgentHub.",
                drifted.len()
            ));
        }
        if !missing.is_empty() {
            warnings.push(format!(
                "{} tracked skill(s) are missing from disk.",
                missing.len()
            ));
        }
        let updates: Vec<Skill> = self
            .list_skills()?
            .into_iter()
            .filter(|skill| skill.update_available)
            .collect();
        Ok(ChangeCheck {
            checked_at: util::now(),
            drifted,
            missing,
            updates,
            warnings,
        })
    }

    // -- dashboard / doctor ------------------------------------------------

    pub fn dashboard(&self) -> Result<Dashboard> {
        let agents = self.list_agents()?;
        let skills = self.list_skills()?;
        let installations: Vec<InstallationRow> = self
            .db
            .list_installations()?
            .into_iter()
            .filter(|row| row.kind == "skill")
            .collect();
        let changes = self.check_changes()?;
        let recent_operations = self.list_operations(8)?;
        let mut issues = Vec::new();
        for agent in &agents {
            if agent.status == AgentStatus::VerificationRequired {
                issues.push(Issue {
                    severity: "warning".to_string(),
                    kind: "agent_verification".to_string(),
                    title: format!("{} needs verification", agent.name),
                    detail: agent.notes.join(" "),
                    agent_key: Some(agent.key.clone()),
                    capability_id: None,
                });
            }
            if agent.status == AgentStatus::Installed
                && agent.skill_roots.iter().all(|root| !root.exists)
            {
                issues.push(Issue {
                    severity: "info".to_string(),
                    kind: "missing_skill_root".to_string(),
                    title: format!("{} has no visible skill root yet", agent.name),
                    detail: "Add a project folder on the Agents page so AgentHub can discover and deploy skills."
                        .to_string(),
                    agent_key: Some(agent.key.clone()),
                    capability_id: None,
                });
            }
        }
        for installation in &changes.drifted {
            issues.push(Issue {
                severity: "warning".to_string(),
                kind: "drift".to_string(),
                title: format!("{} was modified outside AgentHub", installation.agent_name),
                detail: format!("{}", installation.target_path),
                agent_key: Some(installation.agent_key.clone()),
                capability_id: Some(installation.capability_id.clone()),
            });
        }
        for installation in &changes.missing {
            issues.push(Issue {
                severity: "error".to_string(),
                kind: "missing".to_string(),
                title: format!("{} is missing on {}", installation.target_path, installation.agent_name),
                detail: "The tracked installation no longer exists on disk.".to_string(),
                agent_key: Some(installation.agent_key.clone()),
                capability_id: Some(installation.capability_id.clone()),
            });
        }
        let pending_updates = changes.updates.len();
        let mcp_servers = self.db.list_mcp()?.len();
        let plugins = self.db.list_plugins()?.len();
        Ok(Dashboard {
            stats: DashboardStats {
                agents_detected: agents
                    .iter()
                    .filter(|agent| {
                        matches!(
                            agent.status,
                            AgentStatus::Installed | AgentStatus::VerificationRequired
                        )
                    })
                    .count(),
                agents_total: agents.len(),
                skills: skills.len(),
                mcp_servers,
                plugins,
                installations: installations.len(),
                pending_updates,
                conflicts: changes.drifted.len(),
                warnings: issues.iter().filter(|issue| issue.severity != "info").count(),
            },
            agents: agents
                .iter()
                .map(|agent| AgentSummary {
                    id: agent.id.clone(),
                    key: agent.key.clone(),
                    name: agent.name.clone(),
                    vendor: agent.vendor.clone(),
                    status: agent.status,
                    skill_count: agent.skill_count,
                    installed_skill_count: agent.installed_skill_count,
                })
                .collect(),
            recent_operations,
            issues,
            library_root: self.library()?.root().to_string_lossy().to_string(),
            initialized: self.settings()?.initialized,
            last_scan_at: self.db.latest_scan_global()?,
            updates: changes.updates,
        })
    }

    pub fn doctor(&self) -> Result<DoctorReport> {
        let mut checks = Vec::new();
        let library = self.library()?;
        let library_ok = util::is_writable_dir(library.root());
        checks.push(DoctorCheck {
            name: "library".to_string(),
            status: if library_ok { "ok" } else { "error" }.to_string(),
            detail: library.root().to_string_lossy().to_string(),
            hint: if library_ok {
                None
            } else {
                Some("Choose a writable library directory in Settings.".to_string())
            },
        });
        let integrity: Option<String> = self
            .db
            .conn()
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .ok();
        checks.push(DoctorCheck {
            name: "database".to_string(),
            status: if integrity.as_deref() == Some("ok") {
                "ok"
            } else {
                "error"
            }
            .to_string(),
            detail: format!(
                "{} ({})",
                self.db.path().display(),
                integrity.unwrap_or_else(|| "unknown".to_string())
            ),
            hint: None,
        });
        let data_writable = util::is_writable_dir(&self.config.data_dir);
        checks.push(DoctorCheck {
            name: "data_dir".to_string(),
            status: if data_writable { "ok" } else { "error" }.to_string(),
            detail: self.config.data_dir.to_string_lossy().to_string(),
            hint: None,
        });
        let git_ok = std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        checks.push(DoctorCheck {
            name: "git".to_string(),
            status: if git_ok { "ok" } else { "warning" }.to_string(),
            detail: if git_ok {
                "git found; Git imports enabled".to_string()
            } else {
                "git not found; Git imports unavailable".to_string()
            },
            hint: None,
        });
        let agents = self.list_agents()?;
        let mut agent_checks = Vec::new();
        for agent in &agents {
            let adapter = find_adapter(&agent.adapter_id);
            let detection = adapter
                .as_ref()
                .map(|adapter| adapter.detect(&self.config.home));
            agent_checks.push(AgentDoctorCheck {
                key: agent.key.clone(),
                name: agent.name.clone(),
                status: agent.status,
                install_evidence: detection.map(|d| d.evidence).unwrap_or_default(),
                skill_roots: agent
                    .skill_roots
                    .iter()
                    .map(|root| root.path.clone())
                    .collect(),
                mcp_targets: agent.mcp_targets.iter().map(|target| target.path.clone()).collect(),
                notes: agent.notes.clone(),
            });
        }
        let ok = checks.iter().all(|check| check.status != "error");
        Ok(DoctorReport {
            ok,
            checks,
            version: APP_VERSION.to_string(),
            home: self.config.data_dir.to_string_lossy().to_string(),
            library_root: self.library()?.root().to_string_lossy().to_string(),
            database_path: self.db.path().to_string_lossy().to_string(),
            agents: agent_checks,
        })
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        Ok(Bootstrap {
            settings: self.settings()?,
            agents: self.list_agents()?,
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            core_version: APP_VERSION.to_string(),
            project_roots: self
                .project_roots()?
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
        })
    }

    // -- plugins (read-only registry) --------------------------------------

    pub fn sync_plugins(&self) -> Result<Vec<PluginRecordView>> {
        self.sync_agents()?;
        for row in self.db.list_agents()? {
            let Some(adapter) = find_adapter(&row.adapter_id) else {
                continue;
            };
            let support = adapter.plugin_support();
            if matches!(
                support,
                PluginSupport::DiscoveryOnly
                    | PluginSupport::FileManaged
                    | PluginSupport::ManualOnly
            ) {
                for discovered in adapter.discover_plugins(&self.config.home, &row.id) {
                    self.db.upsert_plugin(&crate::db::PluginRow {
                        id: util::new_id(),
                        agent_id: discovered.agent_id,
                        name: discovered.name,
                        version: discovered.version,
                        source: discovered.source,
                        install_state: discovered.install_state,
                        managed_by: support.as_str().to_string(),
                        permissions: discovered.permissions,
                        official_url: discovered.official_url,
                        discovered_path: discovered.discovered_path,
                        metadata: serde_json::json!({ "notes": discovered.notes }).to_string(),
                        updated_at: util::now(),
                    })?;
                }
            }
        }
        let agents = self.db.list_agents()?;
        let agent_map: HashMap<String, String> = agents
            .iter()
            .map(|row| (row.key.clone(), row.name.clone()))
            .collect();
        let mut out = Vec::new();
        for row in self.db.list_plugins()? {
            let agent = self.db.get_agent(&row.agent_id)?;
            let agent_key = agent
                .as_ref()
                .map(|agent| agent.key.clone())
                .unwrap_or_default();
            let notes: Vec<String> = serde_json::from_str::<serde_json::Value>(&row.metadata)
                .ok()
                .and_then(|value| {
                    value
                        .get("notes")
                        .and_then(|notes| serde_json::from_value(notes.clone()).ok())
                })
                .unwrap_or_default();
            let _ = agent_map.get(&agent_key);
            out.push(PluginRecordView {
                id: row.id,
                agent_id: row.agent_id,
                agent_key,
                agent_name: agent.map(|agent| agent.name).unwrap_or_default(),
                name: row.name,
                version: row.version,
                source: row.source,
                install_state: row.install_state,
                managed_by: PluginSupport::parse(&row.managed_by),
                permissions: row.permissions,
                official_url: row.official_url,
                discovered_path: row.discovered_path,
                notes,
                updated_at: row.updated_at,
            });
        }
        Ok(out)
    }

    // -- profiles ----------------------------------------------------------

    fn seed_profiles(&self) -> Result<()> {
        if !self.db.list_profiles()?.is_empty() {
            return Ok(());
        }
        let now = util::now();
        for (name, description) in [
            ("Coding", "Skills and MCP servers used while writing code."),
            ("Research", "Discovery and synthesis capabilities."),
            ("Writing", "Long-form writing and editing capabilities."),
            ("Marketing", "Campaign, content and growth capabilities."),
        ] {
            self.db.insert_profile(
                &util::new_id(),
                name,
                description,
                true,
                &now,
            )?;
        }
        Ok(())
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileView>> {
        Ok(self
            .db
            .list_profiles()?
            .into_iter()
            .map(profile_view)
            .collect())
    }

    pub fn upsert_profile(&self, request: ProfileUpsertRequest) -> Result<ProfileView> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        if request.name.trim().is_empty() {
            return Err(Error::msg("profile name cannot be empty"));
        }
        let now = util::now();
        let id = match &request.id {
            Some(id) => {
                self.db
                    .update_profile(id, request.name.trim(), request.description.as_deref().unwrap_or(""))?;
                id.clone()
            }
            None => {
                let id = util::new_id();
                self.db.insert_profile(&id, request.name.trim(), request.description.as_deref().unwrap_or(""), false, &now)?;
                id
            }
        };
        self.db.delete_profile_items(&id)?;
        for item in &request.items {
            let capability = self
                .db
                .get_capability(&item.capability_id)?
                .ok_or_else(|| Error::not_found("profile item capability is missing"))?;
            let _ = capability;
            self.db.add_profile_item(
                &id,
                &item.capability_id,
                item.enabled,
                &item.target_agent_keys,
            )?;
        }
        self.list_profiles()?
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| Error::not_found("profile was not saved"))
    }

    pub fn delete_profile(&self, id: &str) -> Result<()> {
        self.db.delete_profile(id)
    }

    pub fn plan_profile_apply(&self, profile_id: &str) -> Result<OperationPlan> {
        let profiles = self.list_profiles()?;
        let profile = profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| Error::not_found("profile does not exist"))?;
        let mut items: Vec<PlanItem> = Vec::new();
        let mut warnings = Vec::new();
        let mut restarts = Vec::new();
        let installed_agents: HashMap<String, String> = self
            .list_agents()?
            .into_iter()
            .filter(|agent| {
                matches!(
                    agent.status,
                    AgentStatus::Installed | AgentStatus::VerificationRequired
                )
            })
            .map(|agent| (agent.key, agent.id))
            .collect();
        for item in profile.items.iter().filter(|item| item.enabled) {
            if item.kind != CapabilityKind::Skill {
                warnings.push(format!(
                    "{} is a {:?} capability; distribution of this kind is not available yet and was skipped.",
                    item.name, item.kind
                ));
                continue;
            }
            let target_ids: Vec<String> = if item.target_agent_keys.is_empty() {
                installed_agents.values().cloned().collect()
            } else {
                item.target_agent_keys
                    .iter()
                    .filter_map(|key| installed_agents.get(key).cloned())
                    .collect()
            };
            if target_ids.is_empty() {
                warnings.push(format!("No installed agent targets for {}.", item.name));
                continue;
            }
            let plan = self.plan_install(InstallRequest {
                capability_id: item.capability_id.clone(),
                version_id: None,
                agent_ids: target_ids,
                strategy: None,
                force: false,
            })?;
            warnings.extend(plan.warnings);
            restarts.extend(plan.requires_restart_agents);
            items.extend(plan.items);
        }
        if items.is_empty() {
            return Err(Error::conflict(
                "profile has no distributable items for the currently installed agents",
            ));
        }
        let _ = warnings;
        self.persist_plan(
            OperationKind::ProfileApply,
            format!("Apply profile {}", profile.name),
            items,
            warnings,
            restarts,
            false,
        )
    }

    // -- capability health, audits, dependencies, validation ----------------

    fn resolve_version(
        &self,
        capability_id: &str,
        requested: Option<&str>,
    ) -> Result<(crate::db::CapabilityRow, crate::db::VersionRow)> {
        let capability = self
            .db
            .get_capability(capability_id)?
            .ok_or_else(|| Error::not_found("skill does not exist"))?;
        if capability.kind != "skill" {
            return Err(Error::unsupported("only skill versions can be validated"));
        }
        let version_id = requested
            .map(str::to_string)
            .or_else(|| capability.current_version_id.clone())
            .ok_or_else(|| Error::msg("skill has no version"))?;
        let version = self
            .db
            .get_version(&version_id)?
            .ok_or_else(|| Error::not_found("version does not exist"))?;
        if version.capability_id != capability.id {
            return Err(Error::msg("version does not belong to the skill"));
        }
        Ok((capability, version))
    }

    pub fn audit_skill(&self, request: AuditRequest) -> Result<CapabilityAudit> {
        let (capability, version) =
            self.resolve_version(&request.capability_id, request.version_id.as_deref())?;
        let root = PathBuf::from(&version.storage_path);
        if !root.is_dir() {
            return Err(Error::not_found(format!(
                "library copy is missing: {}",
                root.display()
            )));
        }
        let kind = request.kind.unwrap_or_else(|| "static".to_string());
        if kind != "static" && kind != "environment" {
            return Err(Error::msg("audit kind must be static or environment"));
        }
        let result = if kind == "environment" {
            let deps = crate::deps::check_skill_dependencies(&root);
            crate::audit::AuditResult {
                status: deps.status,
                risk: deps.risk,
                summary: deps.summary,
                findings: Vec::new(),
            }
        } else {
            crate::audit::audit_skill_dir(&root)
        };
        let row = crate::db::AuditRow {
            id: util::new_id(),
            capability_id: capability.id.clone(),
            version_id: version.id.clone(),
            kind: kind.clone(),
            status: result.status.clone(),
            risk: result.risk.clone(),
            summary: result.summary.clone(),
            findings: serde_json::to_string(&result.findings)?,
            created_at: util::now(),
        };
        self.db.insert_audit(&row)?;
        crate::ops::audit_log(
            &self.library()?,
            "capability.audit",
            serde_json::json!({
                "capabilityId": capability.id,
                "version": version.version_label,
                "kind": kind,
                "status": result.status,
                "risk": result.risk,
            }),
        );
        Ok(CapabilityAudit {
            id: row.id,
            capability_id: capability.id,
            version_id: version.id,
            version_label: version.version_label,
            kind,
            status: result.status,
            risk: result.risk,
            summary: result.summary,
            findings: result.findings,
            created_at: row.created_at,
        })
    }

    pub fn skill_dependencies(&self, request: AuditRequest) -> Result<DependencyReport> {
        let (capability, version) =
            self.resolve_version(&request.capability_id, request.version_id.as_deref())?;
        let root = PathBuf::from(&version.storage_path);
        if !root.is_dir() {
            return Err(Error::not_found(format!(
                "library copy is missing: {}",
                root.display()
            )));
        }
        let result = crate::deps::check_skill_dependencies(&root);
        let checked_at = util::now();
        self.db.insert_dependency_report(&crate::db::DependencyRow {
            id: util::new_id(),
            capability_id: capability.id.clone(),
            version_id: version.id.clone(),
            status: result.status.clone(),
            risk: result.risk.clone(),
            summary: result.summary.clone(),
            items: serde_json::to_string(&result.items)?,
            checked_at: checked_at.clone(),
        })?;
        Ok(DependencyReport {
            capability_id: capability.id,
            version_id: version.id,
            status: result.status,
            risk: result.risk,
            summary: result.summary,
            items: result.items,
            checked_at,
        })
    }

    pub fn list_audits(&self, capability_id: &str, limit: usize) -> Result<Vec<CapabilityAudit>> {
        let rows = self.db.list_audits(capability_id, limit)?;
        let mut out = Vec::new();
        for row in rows {
            let version_label = self
                .db
                .get_version(&row.version_id)?
                .map(|version| version.version_label)
                .unwrap_or_default();
            let findings = serde_json::from_str(&row.findings).unwrap_or_default();
            out.push(CapabilityAudit {
                id: row.id,
                capability_id: row.capability_id,
                version_id: row.version_id,
                version_label,
                kind: row.kind,
                status: row.status,
                risk: row.risk,
                summary: row.summary,
                findings,
                created_at: row.created_at,
            });
        }
        Ok(out)
    }

    pub fn list_validations(&self, capability_id: &str, limit: usize) -> Result<Vec<ValidationRun>> {
        let rows = self.db.list_validation_runs(capability_id, limit)?;
        let mut out = Vec::new();
        for row in rows {
            let version_label = self
                .db
                .get_version(&row.version_id)?
                .map(|version| version.version_label)
                .unwrap_or_default();
            let agent = match &row.agent_id {
                Some(agent_id) => self.db.get_agent(agent_id)?,
                None => None,
            };
            out.push(ValidationRun {
                id: row.id,
                capability_id: row.capability_id,
                version_id: row.version_id,
                version_label,
                agent_id: row.agent_id,
                agent_key: agent.as_ref().map(|agent| agent.key.clone()),
                agent_name: agent.map(|agent| agent.name),
                kind: row.kind,
                status: row.status,
                case_name: row.case_name,
                notes: row.notes,
                evidence: row.evidence,
                actor: "user".to_string(),
                created_at: row.created_at,
            });
        }
        Ok(out)
    }

    pub fn record_validation(&self, request: ValidationRequest) -> Result<ValidationRun> {
        let _guard = self.write_lock.lock().map_err(|_| Error::msg("lock poisoned"))?;
        let allowed = ["passed", "failed", "needs_attention", "unverified"];
        if !allowed.contains(&request.status.as_str()) {
            return Err(Error::msg(
                "validation status must be passed, failed, needs_attention or unverified",
            ));
        }
        if request.case_name.trim().is_empty() {
            return Err(Error::msg("a validation case name is required"));
        }
        let (capability, version) =
            self.resolve_version(&request.capability_id, request.version_id.as_deref())?;
        if let Some(agent_id) = &request.agent_id {
            self.db
                .get_agent(agent_id)?
                .ok_or_else(|| Error::not_found("agent does not exist"))?;
        }
        let kind = request.kind.unwrap_or_else(|| "manual".to_string());
        if !["manual", "static", "environment", "regression"].contains(&kind.as_str()) {
            return Err(Error::msg("invalid validation kind"));
        }
        let id = util::new_id();
        self.db.insert_validation_run(&crate::db::ValidationRow {
            id: id.clone(),
            capability_id: capability.id.clone(),
            version_id: version.id.clone(),
            agent_id: request.agent_id.clone(),
            kind: kind.clone(),
            status: request.status.clone(),
            case_name: request.case_name.clone(),
            notes: request.notes.clone().unwrap_or_default(),
            evidence: request.evidence.clone().unwrap_or_default(),
            created_at: util::now(),
        })?;
        crate::ops::audit_log(
            &self.library()?,
            "capability.validation",
            serde_json::json!({
                "capabilityId": capability.id,
                "version": version.version_label,
                "agentId": request.agent_id,
                "status": request.status,
                "case": request.case_name,
            }),
        );
        let (agent_key, agent_name) = match &request.agent_id {
            Some(agent_id) => {
                let agent = self
                    .db
                    .get_agent(agent_id)?
                    .ok_or_else(|| Error::not_found("agent does not exist"))?;
                (Some(agent.key), Some(agent.name))
            }
            None => (None, None),
        };
        Ok(ValidationRun {
            id,
            capability_id: capability.id,
            version_id: version.id,
            version_label: version.version_label,
            agent_id: request.agent_id,
            agent_key,
            agent_name,
            kind,
            status: request.status,
            case_name: request.case_name,
            notes: request.notes.unwrap_or_default(),
            evidence: request.evidence.unwrap_or_default(),
            actor: "user".to_string(),
            created_at: util::now(),
        })
    }

    pub fn skill_health(&self, capability_id: &str) -> Result<CapabilityHealth> {
        let capability = self
            .db
            .get_capability(capability_id)?
            .ok_or_else(|| Error::not_found("skill does not exist"))?;
        if capability.kind != "skill" {
            return Err(Error::unsupported("only skills have health records"));
        }
        let version = match &capability.current_version_id {
            Some(version_id) => self.db.get_version(version_id)?,
            None => None,
        };
        let Some(version) = version else {
            return Ok(CapabilityHealth {
                capability_id: capability.id.clone(),
                slug: capability.slug.clone(),
                name: capability.name.clone(),
                description: capability.description.clone(),
                version_label: String::new(),
                status: HealthState::NotDeployed,
                reasons: vec!["skill has no stored version".to_string()],
                audit: None,
                environment: None,
                validations: Vec::new(),
                installation_count: 0,
                agent_keys: Vec::new(),
                verified_agent_keys: Vec::new(),
                last_checked_at: None,
            });
        };
        let audit = self
            .db
            .latest_audit(&capability.id, &version.id, "static")?
            .map(|row| CapabilityAudit {
                id: row.id,
                capability_id: row.capability_id,
                version_id: row.version_id,
                version_label: version.version_label.clone(),
                kind: row.kind,
                status: row.status,
                risk: row.risk,
                summary: row.summary,
                findings: serde_json::from_str(&row.findings).unwrap_or_default(),
                created_at: row.created_at,
            });
        let environment = self
            .db
            .latest_dependency_report(&capability.id, &version.id)?
            .map(|row| DependencyReport {
                capability_id: row.capability_id,
                version_id: row.version_id,
                status: row.status,
                risk: row.risk,
                summary: row.summary,
                items: serde_json::from_str(&row.items).unwrap_or_default(),
                checked_at: row.checked_at,
            });
        let validations: Vec<ValidationRun> = self
            .list_validations(&capability.id, 100)?
            .into_iter()
            .filter(|run| run.version_id == version.id)
            .collect();
        let installations: Vec<InstallationRow> = self
            .db
            .list_installations_for_capability(&capability.id)?
            .into_iter()
            .filter(|row| row.kind == "skill")
            .collect();
        let mut agent_keys = Vec::new();
        for installation in &installations {
            if let Some(agent) = self.db.get_agent(&installation.agent_id)? {
                agent_keys.push(agent.key);
            }
        }
        agent_keys.sort();
        agent_keys.dedup();
        let (status, reasons) =
            crate::validation::derive_health(audit.as_ref(), environment.as_ref(), &validations, &agent_keys);
        let verified_agent_keys: Vec<String> = validations
            .iter()
            .filter(|run| run.status == "passed" && run.kind == "manual")
            .filter_map(|run| run.agent_key.clone())
            .collect();
        let last_checked_at = [
            audit.as_ref().map(|audit| audit.created_at.clone()),
            environment.as_ref().map(|deps| deps.checked_at.clone()),
            validations.first().map(|run| run.created_at.clone()),
        ]
        .into_iter()
        .flatten()
        .max();
        Ok(CapabilityHealth {
            capability_id: capability.id,
            slug: capability.slug,
            name: capability.name,
            description: capability.description,
            version_label: version.version_label,
            status,
            reasons,
            audit,
            environment,
            validations,
            installation_count: installations.len(),
            agent_keys,
            verified_agent_keys,
            last_checked_at,
        })
    }

    pub fn health_overview(&self) -> Result<HealthOverview> {
        let skills = self.list_skills()?;
        let mut capabilities = Vec::new();
        for skill in &skills {
            capabilities.push(self.skill_health(&skill.id)?);
        }
        let mut summary = HealthSummary {
            total: capabilities.len(),
            verified: 0,
            unverified: 0,
            needs_attention: 0,
            failed: 0,
            not_deployed: 0,
            last_checked_at: None,
        };
        for capability in &capabilities {
            match capability.status {
                HealthState::Verified => summary.verified += 1,
                HealthState::Unverified => summary.unverified += 1,
                HealthState::NeedsAttention => summary.needs_attention += 1,
                HealthState::Failed => summary.failed += 1,
                HealthState::NotDeployed => summary.not_deployed += 1,
            }
            if let Some(checked_at) = &capability.last_checked_at {
                if summary
                    .last_checked_at
                    .as_ref()
                    .map(|current| checked_at > current)
                    .unwrap_or(true)
                {
                    summary.last_checked_at = Some(checked_at.clone());
                }
            }
        }
        capabilities.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(HealthOverview {
            summary,
            capabilities,
        })
    }

    /// Compute and persist the impact of moving a capability from one version to
    /// another. Used both by the UI preview and by plan generation.
    pub fn preview_update(&self, request: ImpactRequest) -> Result<UpdateImpact> {
        let (capability, to_version) =
            self.resolve_version(&request.capability_id, Some(&request.to_version_id))?;
        let installations = self
            .db
            .list_installations_for_capability(&capability.id)?;
        let from_version_id = match &request.from_version_id {
            Some(version_id) => version_id.clone(),
            None => installations
                .iter()
                .filter_map(|row| row.version_id.clone())
                .next()
                .or_else(|| capability.current_version_id.clone())
                .ok_or_else(|| Error::msg("no installed version to compare against"))?,
        };
        if from_version_id == to_version.id {
            return Err(Error::conflict("target version is already the compared version"));
        }
        if let Some(row) =
            self.db
                .latest_impact(&capability.id, &from_version_id, &to_version.id)?
        {
            if let Ok(mut impact) = serde_json::from_str::<UpdateImpact>(&row.payload) {
                self.attach_impact_scope(&mut impact, &installations)?;
                return Ok(impact);
            }
        }
        let from_version = self
            .db
            .get_version(&from_version_id)?
            .ok_or_else(|| Error::not_found("from version does not exist"))?;
        let old_root = PathBuf::from(&from_version.storage_path);
        let new_root = PathBuf::from(&to_version.storage_path);
        if !old_root.is_dir() || !new_root.is_dir() {
            return Err(Error::not_found("one of the library versions is missing on disk"));
        }
        let result = crate::impact::analyze_impact(&old_root, &new_root);
        let mut impact = UpdateImpact {
            id: util::new_id(),
            capability_id: capability.id.clone(),
            from_version_id: from_version.id.clone(),
            from_version_label: from_version.version_label.clone(),
            to_version_id: to_version.id.clone(),
            to_version_label: to_version.version_label.clone(),
            risk: result.risk,
            summary: result.summary,
            file_changes: result.file_changes,
            classes: result.classes,
            findings: result.findings,
            affected_installation_count: 0,
            affected_agent_keys: Vec::new(),
            affected_project_roots: Vec::new(),
            requires_revalidation: result.requires_revalidation,
            created_at: util::now(),
        };
        self.attach_impact_scope(&mut impact, &installations)?;
        self.db.insert_impact(&crate::db::ImpactRow {
            id: impact.id.clone(),
            capability_id: impact.capability_id.clone(),
            from_version_id: impact.from_version_id.clone(),
            to_version_id: impact.to_version_id.clone(),
            risk: impact.risk.clone(),
            summary: impact.summary.clone(),
            payload: serde_json::to_string(&impact)?,
            created_at: impact.created_at.clone(),
        })?;
        Ok(impact)
    }

    fn attach_impact_scope(
        &self,
        impact: &mut UpdateImpact,
        installations: &[InstallationRow],
    ) -> Result<()> {
        let project_roots = self.project_roots()?;
        let mut agent_keys = Vec::new();
        let mut roots = Vec::new();
        for installation in installations {
            if let Some(agent) = self.db.get_agent(&installation.agent_id)? {
                agent_keys.push(agent.key);
            }
            for root in &project_roots {
                if Path::new(&installation.target_path).starts_with(root) {
                    roots.push(root.to_string_lossy().to_string());
                }
            }
        }
        agent_keys.sort();
        agent_keys.dedup();
        roots.sort();
        roots.dedup();
        impact.affected_installation_count = installations.len();
        impact.affected_agent_keys = agent_keys;
        impact.affected_project_roots = roots;
        Ok(())
    }

    pub fn agent_health_report(&self, record: bool) -> Result<AgentHealthReport> {
        self.sync_agents()?;
        let agents = self.list_agents()?;
        let mut out = Vec::new();
        for agent in agents {
            let version = match &agent.app_path {
                Some(app_path) => crate::agent_health::detect_app_version(Path::new(app_path)),
                None => None,
            };
            let version = match version {
                Some(version) => Some(version),
                None if agent.key == "dsh" => crate::agent_health::detect_binary_version("dsh"),
                None => None,
            };
            let latest = if record {
                Some(self.db.record_agent_version(
                    &agent.id,
                    version.as_deref(),
                    &agent.key,
                )?)
            } else {
                self.db.latest_agent_version(&agent.id)?
            };
            let mut notes = agent.notes.clone();
            let version_changed = latest.as_ref().map(|row| row.changed).unwrap_or(false);
            if version_changed {
                if let Some(row) = &latest {
                    notes.push(format!(
                        "Agent version changed from {} to {}; re-run scan and re-record validation evidence.",
                        row.previous_version.clone().unwrap_or_else(|| "unknown".to_string()),
                        row.version.clone().unwrap_or_else(|| "unknown".to_string())
                    ));
                }
            }
            if agent
                .skill_roots
                .iter()
                .all(|root| !root.exists)
            {
                notes.push("No skill root exists on disk yet.".to_string());
            }
            out.push(AgentHealth {
                agent_id: agent.id,
                agent_key: agent.key,
                agent_name: agent.name,
                status: agent.status,
                version: latest.as_ref().and_then(|row| row.version.clone()),
                previous_version: latest.as_ref().and_then(|row| row.previous_version.clone()),
                version_changed,
                version_changed_at: latest
                    .as_ref()
                    .filter(|row| row.changed)
                    .map(|row| row.detected_at.clone()),
                paths_ok: agent
                    .skill_roots
                    .iter()
                    .any(|root| root.exists)
                    || agent.mcp_targets.iter().any(|target| target.exists),
                notes,
                checked_at: latest
                    .as_ref()
                    .map(|row| row.detected_at.clone())
                    .unwrap_or_else(util::now),
            });
        }
        Ok(AgentHealthReport {
            checked_at: util::now(),
            agents: out,
        })
    }

    // -- MCP (implemented in mcp.rs) ---------------------------------------
    pub fn mcp_servers(&self) -> Result<Vec<McpServerView>> {
        crate::mcp::list_servers(self)
    }

    pub fn mcp_discover(&self, agent_ids: Option<Vec<String>>) -> Result<Vec<DiscoveredMcp>> {
        crate::mcp::discover(self, agent_ids)
    }

    pub fn mcp_upsert(&self, request: McpUpsertRequest) -> Result<McpServerView> {
        crate::mcp::upsert(self, request)
    }

    pub fn mcp_import(&self, request: McpImportRequest) -> Result<McpServerView> {
        crate::mcp::import(self, request)
    }

    pub fn mcp_plan_distribute(&self, request: McpDistributeRequest) -> Result<OperationPlan> {
        crate::mcp::plan_distribute(self, request)
    }

    pub fn mcp_plan_remove(&self, server_id: &str, agent_ids: Vec<String>) -> Result<OperationPlan> {
        crate::mcp::plan_remove(self, server_id, agent_ids)
    }

    pub fn mcp_secret_states(&self, server_id: &str) -> Result<Vec<SecretState>> {
        let server = self
            .db
            .list_mcp()?
            .into_iter()
            .find(|row| row.id == server_id)
            .ok_or_else(|| Error::not_found("MCP server does not exist"))?;
        Ok(server
            .env_keys
            .iter()
            .map(|key| SecretState {
                key: key.clone(),
                present: crate::mcp::secret_present(server_id, key),
            })
            .collect())
    }

    pub fn mcp_set_secret(&self, server_id: &str, key: &str, value: &str) -> Result<()> {
        let server = self
            .db
            .list_mcp()?
            .into_iter()
            .find(|row| row.id == server_id)
            .ok_or_else(|| Error::not_found("MCP server does not exist"))?;
        if !server.env_keys.iter().any(|env_key| env_key == key) {
            return Err(Error::msg(format!(
                "'{key}' is not declared as an environment key for this MCP server"
            )));
        }
        crate::mcp::set_secret(server_id, key, value)
    }

    pub fn mcp_delete_secret(&self, server_id: &str, key: &str) -> Result<()> {
        crate::mcp::delete_secret(server_id, key)
    }

    // Internal accessors used by mcp.rs.
    pub(crate) fn db_ref(&self) -> &Database {
        &self.db
    }

    pub(crate) fn config_ref(&self) -> &CoreConfig {
        &self.config
    }

    pub(crate) fn persist_mcp_plan(
        &self,
        kind: OperationKind,
        summary: String,
        items: Vec<PlanItem>,
        warnings: Vec<String>,
        restarts: Vec<String>,
    ) -> Result<OperationPlan> {
        self.persist_plan(kind, summary, items, warnings, restarts, false)
    }

    pub(crate) fn mcp_row_to_view(
        &self,
        row: &McpRow,
        distributions: Vec<McpDistribution>,
    ) -> McpServerView {
        McpServerView {
            id: row.id.clone(),
            name: row.name.clone(),
            description: row.description.clone(),
            transport: McpTransport::parse(&row.transport),
            command: row.command.clone(),
            args: row.args.clone(),
            url: row.url.clone(),
            headers: row.headers.iter().cloned().collect(),
            env_keys: row.env_keys.clone(),
            credential_refs: row.credential_refs.clone(),
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
            distributions,
            issues: Vec::new(),
        }
    }

}

fn profile_view(row: ProfileRow) -> ProfileView {
    ProfileView {
        id: row.id,
        name: row.name,
        description: row.description,
        is_builtin: row.is_builtin,
        items: row
            .items
            .into_iter()
            .map(|item| ProfileItemView {
                capability_id: item.capability_id,
                kind: CapabilityKind::parse(&item.kind),
                name: item.name,
                description: item.description,
                enabled: item.enabled,
                target_agent_keys: item.target_agent_keys,
            })
            .collect(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// A directory may be created when its nearest existing ancestor is writable.
fn writable_ancestor(path: &Path) -> bool {
    let mut current = path;
    while let Some(parent) = current.parent() {
        if parent.is_dir() {
            return util::is_writable_dir(parent);
        }
        current = parent;
    }
    false
}

fn plan_summary(items: &[PlanItem]) -> PlanSummary {
    PlanSummary {
        create: items.iter().filter(|item| item.action == PlanAction::Create).count(),
        replace: items.iter().filter(|item| item.action == PlanAction::Replace).count(),
        remove: items.iter().filter(|item| item.action == PlanAction::Remove).count(),
        noop: items.iter().filter(|item| item.action == PlanAction::Noop).count(),
        conflict: items.iter().filter(|item| item.action == PlanAction::Conflict).count(),
        blocked: items.iter().filter(|item| item.action == PlanAction::Blocked).count(),
    }
}

/// Errors that still allow the UI to continue should be returned as strings at
/// the command boundary; this helper keeps serde of error values consistent.
pub fn error_json(error: &Error) -> serde_json::Value {
    serde_json::json!({ "message": error.to_string() })
}

#[allow(dead_code)]
fn unused_btreemap(_: BTreeMap<String, String>) {}

/// Constant-time byte comparison for stored approval-token hashes.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}
