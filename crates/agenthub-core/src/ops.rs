//! Transactional distribution engine.
//!
//! Every external write goes through:
//! preflight → snapshot → apply → verify → record. Failures restore the
//! snapshot of every item already applied, so a partially executed operation
//! never leaves an agent half-configured.

use std::fs;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::db::{Database, InstallationRow};
use crate::error::{Error, Result};
use crate::library::Library;
use crate::model::{
    ItemResult, OperationPlan, OperationResult, OperationStatus, PlanAction, PlanItem, SnapshotView,
    InstallStrategy, InstallationState,
};
use crate::skill;
use crate::util;

/// Append a structured audit line to `<library>/logs/operations.log`.
/// Secrets are never passed to this function.
pub fn audit_log(library: &Library, event: &str, detail: serde_json::Value) {
    let dir = library.logs_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let entry = serde_json::json!({
        "ts": util::now(),
        "event": event,
        "detail": detail,
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("operations.log"))
    {
        use std::io::Write;
        let _ = writeln!(file, "{entry}");
    }
}

/// Advisory cross-process lock for operations that write to the library.
pub struct LibraryLock {
    file: fs::File,
}

impl Drop for LibraryLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub fn acquire_lock(library: &Library) -> Result<LibraryLock> {
    fs::create_dir_all(library.root())?;
    let path = library.root().join(".agenthub.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)?;
    file.lock_exclusive()
        .map_err(|err| Error::msg(format!("cannot lock library {}: {err}", path.display())))?;
    Ok(LibraryLock { file })
}

/// Content hash of whatever currently lives at `path`.
pub fn current_target_hash(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        let resolved = fs::canonicalize(path).ok()?;
        if resolved.is_dir() {
            util::hash_dir(&resolved).ok()
        } else {
            util::hash_file(&resolved).ok()
        }
    } else if meta.is_dir() {
        util::hash_dir(path).ok()
    } else {
        util::hash_file(path).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotInfo {
    kind: String,
    backup_path: String,
    existed: bool,
    hash_before: Option<String>,
}

pub struct Executor<'a> {
    pub db: &'a Database,
    pub library: &'a Library,
}

impl<'a> Executor<'a> {
    pub fn apply(&self, plan: &OperationPlan, force: bool) -> Result<OperationResult> {
        if plan.kind != crate::model::OperationKind::Install
            && plan.kind != crate::model::OperationKind::Uninstall
            && plan.kind != crate::model::OperationKind::Update
            && plan.kind != crate::model::OperationKind::ProfileApply
            && plan.kind != crate::model::OperationKind::Sync
            && plan.kind != crate::model::OperationKind::McpDistribute
            && plan.kind != crate::model::OperationKind::McpRemove
        {
            return Err(Error::unsupported(format!(
                "operations of kind {:?} cannot be applied by the skill executor",
                plan.kind
            )));
        }
        self.preflight(plan, force)?;
        let _lock = acquire_lock(self.library)?;
        audit_log(
            self.library,
            "operation.start",
            serde_json::json!({
                "operationId": plan.operation_id,
                "kind": plan.kind.as_str(),
                "items": plan.items.len(),
                "force": force,
            }),
        );
        fs::create_dir_all(self.library.snapshot_dir(&plan.operation_id))?;
        self.db.update_operation_status(
            &plan.operation_id,
            "running",
            &serde_json::to_string(&OperationResult {
                operation_id: plan.operation_id.clone(),
                status: OperationStatus::Running,
                items: Vec::new(),
                diagnostics: Vec::new(),
                finished_at: None,
            })?,
            None,
        )?;

        let mut results: Vec<ItemResult> = Vec::new();
        let mut applied: Vec<PlanItem> = Vec::new();
        let mut diagnostics: Vec<String> = Vec::new();

        for (index, item) in plan.items.iter().enumerate() {
            if item.action == PlanAction::Blocked {
                results.push(ItemResult {
                    plan_item_id: item.id.clone(),
                    agent_key: item.agent_key.clone(),
                    target_path: item.target_path.clone(),
                    status: "skipped".to_string(),
                    message: item.warnings.join("; "),
                    backup_path: None,
                    installation_id: None,
                });
                diagnostics.push(format!(
                    "skipped {} on {}: {}",
                    item.capability_name,
                    item.agent_key,
                    item.warnings.join("; ")
                ));
                continue;
            }
            match self.apply_item(&plan.operation_id, item, index, force) {
                Ok(result) => {
                    if result.status != "skipped" {
                        applied.push(item.clone());
                    }
                    results.push(result);
                }
                Err(err) => {
                    diagnostics.push(format!(
                        "item {} ({}) failed: {err}",
                        item.agent_key, item.target_path
                    ));
                    // Undo everything already applied, newest first.
                    for previous in applied.iter().rev() {
                        if let Err(undo_err) = self.undo_item(&plan.operation_id, previous, true) {
                            diagnostics.push(format!(
                                "automatic undo failed for {}: {undo_err}",
                                previous.target_path
                            ));
                        }
                    }
                    let result = OperationResult {
                        operation_id: plan.operation_id.clone(),
                        status: OperationStatus::Failed,
                        items: results,
                        diagnostics,
                        finished_at: Some(util::now()),
                    };
                    self.db.update_operation_status(
                        &plan.operation_id,
                        "failed",
                        &serde_json::to_string(&result)?,
                        Some(&err.to_string()),
                    )?;
                    audit_log(
                        self.library,
                        "operation.failed",
                        serde_json::json!({
                            "operationId": plan.operation_id,
                            "error": err.to_string(),
                        }),
                    );
                    return Ok(result);
                }
            }
        }

        let all_ok = results.iter().all(|item| {
            item.status == "success" || item.status == "noop" || item.status == "skipped"
        });
        let any_skipped = results.iter().any(|item| item.status == "skipped");
        let status = if all_ok && !any_skipped {
            OperationStatus::Success
        } else if all_ok {
            OperationStatus::Partial
        } else {
            OperationStatus::Partial
        };
        let result = OperationResult {
            operation_id: plan.operation_id.clone(),
            status,
            items: results,
            diagnostics,
            finished_at: Some(util::now()),
        };
        self.db.update_operation_status(
            &plan.operation_id,
            status.as_str(),
            &serde_json::to_string(&result)?,
            None,
        )?;
        audit_log(
            self.library,
            "operation.finish",
            serde_json::json!({
                "operationId": plan.operation_id,
                "status": status.as_str(),
                "items": result.items.len(),
            }),
        );
        Ok(result)
    }

    fn preflight(&self, plan: &OperationPlan, force: bool) -> Result<()> {
        if plan.items.is_empty() {
            return Err(Error::msg("plan has no items"));
        }
        for item in &plan.items {
            if item.action == PlanAction::Blocked {
                // Unsupported targets are skipped and reported; the rest of
                // the plan can still be applied.
                continue;
            }
            let target = PathBuf::from(&item.target_path);
            if !target.is_absolute() {
                return Err(Error::blocked(format!(
                    "target path must be absolute: {}",
                    item.target_path
                )));
            }
            if target.parent().is_none() || target.file_name().is_none() {
                return Err(Error::blocked(format!(
                    "refusing to operate on unsafe target {}",
                    item.target_path
                )));
            }
            // MCP plan items use entry-level semantics while the shared config
            // file already exists; compare the managed entry, not the file hash.
            if item.install_kind == "mcp_config" {
                if item.action == PlanAction::Conflict && !force {
                    return Err(Error::conflict(format!(
                        "{} for {} needs confirmation: {}",
                        item.capability_name,
                        item.agent_key,
                        item.warnings.join("; ")
                    )));
                }
                let (Some(format), Some(key)) =
                    (item.target_format.as_deref(), item.config_key.as_deref())
                else {
                    return Err(Error::msg("MCP plan item is missing format/key"));
                };
                let current = crate::mcp::read_entry(&target, format, key).ok().flatten();
                let current_hash = current.as_ref().map(crate::mcp::entry_hash);
                let mismatch = current_hash != item.current_hash;
                match item.action {
                    PlanAction::Create | PlanAction::Replace | PlanAction::Remove
                        if mismatch && !force =>
                    {
                        return Err(Error::ExternalModification(format!(
                            "MCP entry '{key}' changed on disk since the preview"
                        )));
                    }
                    _ => {}
                }
                continue;
            }
            let current = current_target_hash(&target);
            match item.action {
                PlanAction::Conflict if !force => {
                    return Err(Error::conflict(format!(
                        "{} for {} needs confirmation: {}",
                        item.capability_name,
                        item.agent_key,
                        item.warnings.join("; ")
                    )))
                }
                PlanAction::Create => {
                    if current.is_some() && !force {
                        return Err(Error::ExternalModification(format!(
                            "{} changed since the preview was created",
                            item.target_path
                        )));
                    }
                }
                PlanAction::Replace => {
                    if current != item.current_hash && !force {
                        return Err(Error::ExternalModification(format!(
                            "{} changed on disk since the preview (expected {:?}, found {:?})",
                            item.target_path, item.current_hash, current
                        )));
                    }
                }
                PlanAction::Remove => {
                    if current != item.current_hash && !force {
                        return Err(Error::ExternalModification(format!(
                            "{} changed on disk after the preview; refusing to remove it",
                            item.target_path
                        )));
                    }
                }
                PlanAction::Conflict | PlanAction::Noop => {}
                PlanAction::Blocked => {}
            }
        }
        Ok(())
    }

    fn apply_item(
        &self,
        operation_id: &str,
        item: &PlanItem,
        index: usize,
        force: bool,
    ) -> Result<ItemResult> {
        let target = PathBuf::from(&item.target_path);
        let action = if item.action == PlanAction::Conflict && force {
            PlanAction::Replace
        } else {
            item.action
        };
        if action == PlanAction::Blocked {
            return Err(Error::blocked(format!(
                "{} is blocked for {}",
                item.capability_name, item.agent_key
            )));
        }
        if item.install_kind != "skill_dir" && item.install_kind != "mcp_config" {
            return Err(Error::unsupported(format!(
                "install kind '{}' is not handled by this build",
                item.install_kind
            )));
        }

        let snapshot = self.snapshot_target(operation_id, item, index)?;

        if item.install_kind == "mcp_config" {
            return self.apply_mcp_item(item, action, &snapshot);
        }

        if action == PlanAction::Remove {
            if snapshot.existed {
                util::remove_path(&target)?;
            }
            if let Some(previous) = &item.previous_installation {
                if let Ok(row) = serde_json::from_value::<InstallationRow>(previous.clone()) {
                    self.db.delete_installation(&row.id)?;
                }
            }
            return Ok(ItemResult {
                plan_item_id: item.id.clone(),
                agent_key: item.agent_key.clone(),
                target_path: item.target_path.clone(),
                status: "success".to_string(),
                message: "Removed from agent (snapshot kept for rollback)".to_string(),
                backup_path: if snapshot.existed {
                    Some(snapshot.backup_path.clone())
                } else {
                    None
                },
                installation_id: None,
            });
        }

        if action == PlanAction::Noop {
            // Adopt an existing identical skill so AgentHub can track it.
            let existing = self.db.find_installation(&item.agent_id, &item.target_path)?;
            if existing.is_none() {
                if let (Some(capability_id), Some(version_id)) =
                    (&item.capability_id, &item.version_id)
                {
                    self.upsert_installation(item, capability_id, version_id, snapshot.hash_before.clone())?;
                }
            }
            return Ok(ItemResult {
                plan_item_id: item.id.clone(),
                agent_key: item.agent_key.clone(),
                target_path: item.target_path.clone(),
                status: "noop".to_string(),
                message: "Already installed with identical content".to_string(),
                backup_path: None,
                installation_id: self
                    .db
                    .find_installation(&item.agent_id, &item.target_path)?
                    .map(|row| row.id),
            });
        }

        if snapshot.existed {
            util::remove_path(&target)?;
        }

        let source = item
            .source_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| Error::msg("plan item is missing a source path"))?;
        if !source.is_dir() {
            return Err(Error::not_found(format!(
                "library version directory is missing: {}",
                source.display()
            )));
        }

        let install_result = match item.strategy {
            InstallStrategy::Symlink => self.install_symlink(&source, &target),
            InstallStrategy::Copy => self.install_copy(&source, &target),
        };
        if let Err(err) = install_result {
            let _ = util::remove_path(&target);
            let _ = self.restore_snapshot(&target, &snapshot);
            return Err(err);
        }

        if let Err(err) = self.verify(&target, item) {
            let _ = util::remove_path(&target);
            let _ = self.restore_snapshot(&target, &snapshot);
            return Err(err);
        }

        let (Some(capability_id), Some(version_id)) = (&item.capability_id, &item.version_id) else {
            return Err(Error::msg("plan item is missing capability/version ids"));
        };
        let installation_id =
            self.upsert_installation(item, capability_id, version_id, item.desired_hash.clone())?;

        Ok(ItemResult {
            plan_item_id: item.id.clone(),
            agent_key: item.agent_key.clone(),
            target_path: item.target_path.clone(),
            status: "success".to_string(),
            message: format!("Installed {} via {}", item.capability_name, item.strategy.as_str()),
            backup_path: if snapshot.existed {
                Some(snapshot.backup_path.clone())
            } else {
                None
            },
            installation_id: Some(installation_id),
        })
    }

    fn apply_mcp_item(
        &self,
        item: &PlanItem,
        action: PlanAction,
        snapshot: &SnapshotInfo,
    ) -> Result<ItemResult> {
        let target = PathBuf::from(&item.target_path);
        if action == PlanAction::Noop {
            return Ok(ItemResult {
                plan_item_id: item.id.clone(),
                agent_key: item.agent_key.clone(),
                target_path: item.target_path.clone(),
                status: "noop".to_string(),
                message: "MCP entry is already identical".to_string(),
                backup_path: None,
                installation_id: None,
            });
        }
        if action == PlanAction::Blocked {
            return Err(Error::blocked("MCP plan item is blocked"));
        }
        let remove = action == PlanAction::Remove;
        // MCP config writers edit the file in place; never delete the whole
        // file while replacing one entry.
        if snapshot.existed && util::path_is_symlink(&target) {
            util::remove_path(&target)?;
        }
        match crate::mcp::apply_config_item(self.db, item, remove) {
            Ok(installation_id) => Ok(ItemResult {
                plan_item_id: item.id.clone(),
                agent_key: item.agent_key.clone(),
                target_path: item.target_path.clone(),
                status: "success".to_string(),
                message: if remove {
                    "Removed MCP entry from agent config".to_string()
                } else {
                    format!("Wrote MCP entry {}", item.config_key.clone().unwrap_or_default())
                },
                backup_path: if snapshot.existed {
                    Some(snapshot.backup_path.clone())
                } else {
                    None
                },
                installation_id,
            }),
            Err(err) => {
                // Restore the exact previous file content.
                let _ = util::remove_path(&target);
                let _ = self.restore_snapshot(&target, snapshot);
                Err(err)
            }
        }
    }

    fn install_symlink(&self, source: &Path, target: &Path) -> Result<()> {
        let source = fs::canonicalize(source)?;
        if !source.is_dir() {
            return Err(Error::blocked(format!(
                "symlink source is not a directory: {}",
                source.display()
            )));
        }
        util::atomic_symlink(&source, target)
    }

    fn install_copy(&self, source: &Path, target: &Path) -> Result<()> {
        let parent = target
            .parent()
            .ok_or_else(|| Error::msg("target has no parent directory"))?;
        fs::create_dir_all(parent)?;
        let staging = parent.join(format!(".agenthub-staging-{}", util::new_id()));
        if let Err(err) = util::copy_dir(source, &staging) {
            let _ = util::remove_path(&staging);
            return Err(err);
        }
        if let Err(err) = util::atomic_place_dir(&staging, target) {
            let _ = util::remove_path(&staging);
            return Err(err);
        }
        Ok(())
    }

    fn verify(&self, target: &Path, item: &PlanItem) -> Result<()> {
        if fs::symlink_metadata(target).is_err() {
            return Err(Error::Verification(format!(
                "target was not created: {}",
                target.display()
            )));
        }
        if !target.join("SKILL.md").is_file() {
            return Err(Error::Verification(format!(
                "SKILL.md is not readable after install: {}",
                target.display()
            )));
        }
        if let Some(desired) = &item.desired_hash {
            let actual = current_target_hash(target);
            if actual.as_deref() != Some(desired.as_str()) {
                return Err(Error::Verification(format!(
                    "content hash mismatch at {}: expected {}, got {:?}",
                    target.display(),
                    desired,
                    actual
                )));
            }
        }
        if item.strategy == InstallStrategy::Symlink {
            let link = fs::read_link(target)?;
            let resolved = fs::canonicalize(target)?;
            let expected = item
                .source_path
                .as_ref()
                .map(PathBuf::from)
                .and_then(|path| fs::canonicalize(path).ok())
                .ok_or_else(|| {
                    Error::Verification("library source directory is missing".to_string())
                })?;
            if resolved != expected {
                return Err(Error::Verification(format!(
                    "symlink {} points to unexpected target {}",
                    target.display(),
                    link.display()
                )));
            }
        }
        Ok(())
    }

    fn upsert_installation(
        &self,
        item: &PlanItem,
        capability_id: &str,
        version_id: &str,
        content_hash: Option<String>,
    ) -> Result<String> {
        let existing = self.db.find_installation(&item.agent_id, &item.target_path)?;
        let id = existing
            .as_ref()
            .map(|row| row.id.clone())
            .unwrap_or_else(util::new_id);
        let now = util::now();
        let row = InstallationRow {
            id: id.clone(),
            capability_id: Some(capability_id.to_string()),
            version_id: Some(version_id.to_string()),
            agent_id: item.agent_id.clone(),
            kind: "skill".to_string(),
            scope: item.scope.as_str().to_string(),
            strategy: item.strategy.as_str().to_string(),
            target_path: item.target_path.clone(),
            source_path: item.source_path.clone(),
            content_hash,
            state: InstallationState::Installed.as_str().to_string(),
            config_key: item.config_key.clone().unwrap_or_default(),
            installed_at: now.clone(),
            verified_at: Some(now.clone()),
            last_check_at: Some(now),
            metadata: serde_json::json!({
                "capabilityName": item.capability_name,
                "versionLabel": item.version_label,
            })
            .to_string(),
        };
        self.db.upsert_installation(&row)?;
        Ok(id)
    }

    fn snapshot_target(
        &self,
        operation_id: &str,
        item: &PlanItem,
        index: usize,
    ) -> Result<SnapshotInfo> {
        let target = PathBuf::from(&item.target_path);
        let dir = self.library.snapshot_dir(operation_id);
        fs::create_dir_all(&dir)?;
        let meta = match fs::symlink_metadata(&target) {
            Ok(meta) => meta,
            Err(_) => {
                self.db.insert_snapshot(&crate::db::SnapshotRow {
                    id: util::new_id(),
                    operation_id: operation_id.to_string(),
                    kind: "absent".to_string(),
                    original_path: item.target_path.clone(),
                    backup_path: String::new(),
                    existed: false,
                    hash_before: None,
                    created_at: util::now(),
                })?;
                return Ok(SnapshotInfo {
                    kind: "absent".to_string(),
                    backup_path: String::new(),
                    existed: false,
                    hash_before: None,
                });
            }
        };
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "target".to_string());
        let hash_before = current_target_hash(&target);
        let info = if meta.file_type().is_symlink() {
            let link_target = fs::read_link(&target)?;
            SnapshotInfo {
                kind: "symlink".to_string(),
                backup_path: link_target.to_string_lossy().to_string(),
                existed: true,
                hash_before,
            }
        } else if meta.is_dir() {
            let backup = dir.join(format!("{index:03}-{name}"));
            util::copy_dir(&target, &backup)?;
            SnapshotInfo {
                kind: "dir".to_string(),
                backup_path: backup.to_string_lossy().to_string(),
                existed: true,
                hash_before,
            }
        } else {
            let backup = dir.join(format!("{index:03}-{name}"));
            fs::create_dir_all(&dir)?;
            fs::copy(&target, &backup)?;
            SnapshotInfo {
                kind: "file".to_string(),
                backup_path: backup.to_string_lossy().to_string(),
                existed: true,
                hash_before,
            }
        };
        self.db.insert_snapshot(&crate::db::SnapshotRow {
            id: util::new_id(),
            operation_id: operation_id.to_string(),
            kind: info.kind.clone(),
            original_path: item.target_path.clone(),
            backup_path: info.backup_path.clone(),
            existed: info.existed,
            hash_before: info.hash_before.clone(),
            created_at: util::now(),
        })?;
        Ok(info)
    }

    fn restore_snapshot(&self, target: &Path, snapshot: &SnapshotInfo) -> Result<()> {
        util::remove_path(target)?;
        if !snapshot.existed {
            return Ok(());
        }
        match snapshot.kind.as_str() {
            "symlink" => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                std::os::unix::fs::symlink(&snapshot.backup_path, target)?;
            }
            "dir" => {
                util::copy_dir(Path::new(&snapshot.backup_path), target)?;
            }
            "file" => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&snapshot.backup_path, target)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn undo_item(&self, operation_id: &str, item: &PlanItem, force: bool) -> Result<()> {
        let target = PathBuf::from(&item.target_path);
        if !force && item.install_kind == "skill_dir" {
            if let Some(desired) = &item.desired_hash {
                if let Some(current) = current_target_hash(&target) {
                    if &current != desired {
                        return Err(Error::ExternalModification(format!(
                            "{} was modified after the operation; refusing to overwrite",
                            item.target_path
                        )));
                    }
                }
            }
        }
        let snapshots = self.db.list_snapshots(operation_id)?;
        let snapshot = snapshots
            .iter()
            .rev()
            .find(|row| row.original_path == item.target_path)
            .map(|row| SnapshotInfo {
                kind: row.kind.clone(),
                backup_path: row.backup_path.clone(),
                existed: row.existed,
                hash_before: row.hash_before.clone(),
            })
            .unwrap_or(SnapshotInfo {
                kind: "absent".to_string(),
                backup_path: String::new(),
                existed: false,
                hash_before: None,
            });
        self.restore_snapshot(&target, &snapshot)?;
        match &item.previous_installation {
            Some(previous) => {
                let row: InstallationRow = serde_json::from_value(previous.clone())?;
                self.db.upsert_installation(&row)?;
            }
            None => {
                if let Some(existing) = self.db.find_installation(&item.agent_id, &item.target_path)? {
                    self.db.delete_installation(&existing.id)?;
                }
            }
        }
        Ok(())
    }

    pub fn rollback(&self, operation_id: &str) -> Result<OperationResult> {
        let operation = self
            .db
            .get_operation(operation_id)?
            .ok_or_else(|| Error::not_found(format!("operation {operation_id} does not exist")))?;
        let status = OperationStatus::parse(&operation.status);
        if status == OperationStatus::RolledBack {
            return Err(Error::conflict("operation has already been rolled back"));
        }
        if status != OperationStatus::Success && status != OperationStatus::Partial {
            return Err(Error::conflict(format!(
                "operation in state '{}' cannot be rolled back",
                operation.status
            )));
        }
        let rollback_ops = self
            .db
            .list_operations(200)?
            .into_iter()
            .filter(|op| op.rollback_of.as_deref() == Some(operation_id))
            .filter(|op| {
                matches!(
                    OperationStatus::parse(&op.status),
                    OperationStatus::Success | OperationStatus::Partial | OperationStatus::Running
                )
            })
            .count();
        if rollback_ops > 0 {
            return Err(Error::conflict("operation already has a rollback"));
        }
        let plan: OperationPlan = serde_json::from_str(&operation.plan)
            .map_err(|err| Error::msg(format!("operation plan is unreadable: {err}")))?;
        if plan.items.is_empty() {
            return Err(Error::unsupported(
                "this operation has no file items to roll back",
            ));
        }
        let _lock = acquire_lock(self.library)?;
        let rollback_id = util::new_id();
        self.db.insert_operation(
            &rollback_id,
            "rollback",
            false,
            &format!("Rollback of {}", operation.summary),
            &serde_json::to_string(&plan)?,
            Some(operation_id),
        )?;
        let mut results = Vec::new();
        let mut diagnostics = Vec::new();
        for item in plan.items.iter().rev() {
            match self.undo_item(operation_id, item, false) {
                Ok(()) => results.push(ItemResult {
                    plan_item_id: item.id.clone(),
                    agent_key: item.agent_key.clone(),
                    target_path: item.target_path.clone(),
                    status: "success".to_string(),
                    message: "Restored previous state".to_string(),
                    backup_path: None,
                    installation_id: None,
                }),
                Err(err) => {
                    diagnostics.push(format!("{}: {err}", item.target_path));
                    results.push(ItemResult {
                        plan_item_id: item.id.clone(),
                        agent_key: item.agent_key.clone(),
                        target_path: item.target_path.clone(),
                        status: "failed".to_string(),
                        message: err.to_string(),
                        backup_path: None,
                        installation_id: None,
                    });
                }
            }
        }
        let any_failed = results.iter().any(|item| item.status == "failed");
        let status = if any_failed {
            OperationStatus::Partial
        } else {
            OperationStatus::Success
        };
        let result = OperationResult {
            operation_id: rollback_id.clone(),
            status,
            items: results,
            diagnostics,
            finished_at: Some(util::now()),
        };
        self.db.update_operation_status(
            &rollback_id,
            status.as_str(),
            &serde_json::to_string(&result)?,
            None,
        )?;
        self.db.update_operation_status(
            operation_id,
            "rolled_back",
            &operation.result,
            operation.error.as_deref(),
        )?;
        Ok(result)
    }

    pub fn list_snapshots(&self, operation_id: &str) -> Result<Vec<SnapshotView>> {
        Ok(self
            .db
            .list_snapshots(operation_id)?
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
}

/// Convenience: hash a skill directory as deployed (used by drift checks).
pub fn deployed_hash(path: &Path) -> Option<String> {
    skill::skill_hash(path)
}
