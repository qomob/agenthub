//! Central capability library.
//!
//! Layout (root is user selectable, default `~/.agenthub`):
//!
//! ```text
//! <root>/
//!   library/skills/<slug>/<hash12>/...   immutable skill versions
//!   snapshots/<operation-id>/...          pre-write backups for rollback
//!   backups/<timestamp>/...               explicit full backups
//!   staging/                              temporary import/install workspaces
//!   logs/                                 structured local logs
//!   agenthub.db                           SQLite registry (metadata only)
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::skill;
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub id: String,
    pub created_at: String,
    pub app_version: String,
    pub library_root: String,
    pub database: String,
    pub capability_count: i64,
    pub installation_count: i64,
}

#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Library { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(self.skills_dir())?;
        fs::create_dir_all(self.snapshots_dir())?;
        fs::create_dir_all(self.staging_root())?;
        fs::create_dir_all(self.backups_dir())?;
        fs::create_dir_all(self.root.join("logs"))?;
        Ok(())
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("library/skills")
    }

    pub fn skill_dir(&self, slug: &str) -> PathBuf {
        self.skills_dir().join(slug)
    }

    pub fn version_dir(&self, slug: &str, content_hash: &str) -> PathBuf {
        self.skill_dir(slug)
            .join(util::short_hash(content_hash))
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    pub fn snapshot_dir(&self, operation_id: &str) -> PathBuf {
        self.snapshots_dir().join(operation_id)
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn staging_root(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Copy a validated skill directory into the immutable version store.
    pub fn store_version(&self, slug: &str, content_hash: &str, source: &Path) -> Result<PathBuf> {
        let target = self.version_dir(slug, content_hash);
        if target.join("SKILL.md").is_file() {
            return Ok(target);
        }
        let staging = skill::StagingDir::create(&self.staging_root())?;
        let staged = staging.path().join("version");
        util::copy_dir(source, &staged)?;
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| Error::msg("invalid version target"))?,
        )?;
        // A concurrent import may have placed the same content already.
        if target.join("SKILL.md").is_file() {
            return Ok(target);
        }
        fs::rename(&staged, &target)?;
        Ok(target)
    }

    pub fn write_manifest(&self, name: &str, contents: &str) -> Result<PathBuf> {
        let path = self.root.join(name);
        util::atomic_write(&path, contents.as_bytes())?;
        Ok(path)
    }

    /// Create a full backup of metadata plus a manifest. Skill payloads are
    /// content-addressed and therefore already immutable inside the library.
    pub fn create_backup(&self, db: &crate::db::Database, app_version: &str) -> Result<crate::model::BackupResult> {
        let created_at = util::now();
        let id = format!("backup-{}", util::now_for_path());
        let dir = self.backups_dir().join(&id);
        fs::create_dir_all(&dir)?;
        let db_target = dir.join("agenthub.db");
        // VACUUM INTO produces a consistent single-file snapshot including WAL content.
        let sql = format!("VACUUM INTO '{}'", db_target.to_string_lossy().replace('\'', "''"));
        db.conn().execute_batch(&sql)?;
        let manifest = BackupManifest {
            id: id.clone(),
            created_at: created_at.clone(),
            app_version: app_version.to_string(),
            library_root: self.root.to_string_lossy().to_string(),
            database: db_target.to_string_lossy().to_string(),
            capability_count: db.count_capabilities("skill")?,
            installation_count: db.count("installations")?,
        };
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        util::atomic_write(&dir.join("manifest.json"), manifest_json.as_bytes())?;
        // Record the backup inside the registry for the Activity page.
        let plan = crate::model::OperationPlan {
            operation_id: String::new(),
            kind: crate::model::OperationKind::Backup,
            dry_run: false,
            items: Vec::new(),
            warnings: Vec::new(),
            requires_restart_agents: Vec::new(),
            requires_confirmation: false,
            summary: crate::model::PlanSummary {
                create: 1,
                replace: 0,
                remove: 0,
                noop: 0,
                conflict: 0,
                blocked: 0,
            },
            created_at: created_at.clone(),
        };
        let op_id = util::new_id();
        let mut plan = plan;
        plan.operation_id = op_id.clone();
        db.insert_operation(
            &op_id,
            "backup",
            false,
            &format!("Backup {}", id),
            &serde_json::to_string(&plan)?,
            None,
        )?;
        let result = crate::model::OperationResult {
            operation_id: op_id.clone(),
            status: crate::model::OperationStatus::Success,
            items: vec![crate::model::ItemResult {
                plan_item_id: String::new(),
                agent_key: String::new(),
                target_path: dir.to_string_lossy().to_string(),
                status: "success".to_string(),
                message: "Backup created".to_string(),
                backup_path: Some(dir.to_string_lossy().to_string()),
                installation_id: None,
            }],
            diagnostics: Vec::new(),
            finished_at: Some(util::now()),
        };
        db.update_operation_status(&op_id, "success", &serde_json::to_string(&result)?, None)?;
        Ok(crate::model::BackupResult {
            id,
            path: dir.to_string_lossy().to_string(),
            created_at,
            entries: manifest.installation_count as usize,
        })
    }

    pub fn list_backups(&self) -> Result<Vec<crate::model::BackupView>> {
        let mut out = Vec::new();
        let dir = self.backups_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }
            let manifest_path = entry.path().join("manifest.json");
            let Ok(text) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<BackupManifest>(&text) else {
                continue;
            };
            let size_bytes = dir_size(&entry.path());
            out.push(crate::model::BackupView {
                id: manifest.id,
                path: entry.path().to_string_lossy().to_string(),
                created_at: manifest.created_at,
                size_bytes,
            });
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }
}

fn dir_size(root: &Path) -> i64 {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok().map(|meta| meta.len() as i64))
        .sum()
}

/// Check a proposed library root before the user commits to it.
pub fn validate_new_root(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            return Err(Error::msg(format!(
                "{} exists and is not a directory",
                path.display()
            )));
        }
        let mut entries = fs::read_dir(path)?;
        if entries.next().is_some() {
            return Err(Error::msg(format!(
                "{} is not empty; choose an empty directory for the AgentHub library",
                path.display()
            )));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::msg("library root needs a parent directory"))?;
    if !parent.is_dir() {
        return Err(Error::msg(format!(
            "parent directory {} does not exist",
            parent.display()
        )));
    }
    if !util::is_writable_dir(parent) {
        return Err(Error::msg(format!(
            "no write permission for {}",
            parent.display()
        )));
    }
    Ok(())
}
