//! Read-only skill discovery across every installed agent.
//!
//! Scanning never writes to an agent directory. The only side effect is
//! refreshing AgentHub's own `scan_results` cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::adapters::{AgentAdapter, SkillRootSpec};
use crate::db::{Database, ScanResultRow};
use crate::error::Result;
use crate::model::{
    AgentRecord, AgentScan, AgentStatus, DiscoveredSkill, RootScan, ScanReport, Scope,
};
use crate::skill;
use crate::util;

pub fn scan_agents(
    db: &Database,
    agents: &[(AgentRecord, Box<dyn AgentAdapter>)],
    home: &Path,
    project_roots: &[PathBuf],
) -> Result<ScanReport> {
    let scanned_at = util::now();
    let mut report = ScanReport {
        scanned_at: scanned_at.clone(),
        agents: Vec::new(),
        total_skills: 0,
        duplicates: 0,
        conflicts: 0,
        invalid: 0,
        warnings: Vec::new(),
    };
    // slug -> content hash -> agents that have it
    let mut slug_contents: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

    for (record, adapter) in agents {
        let detection = adapter.detect(home);
        let mut roots: Vec<SkillRootSpec> = adapter
            .resolved_skill_roots(home, Some(record))
            .into_iter()
            .collect();
        for project in project_roots {
            for path in adapter.project_skill_roots(project) {
                let already = roots.iter().any(|root| root.path == path);
                if !already {
                    roots.push(SkillRootSpec {
                        path,
                        scope: Scope::Project,
                        source: "project".to_string(),
                    });
                }
            }
        }

        let mut agent_scan = AgentScan {
            agent_id: record.id.clone(),
            agent_key: record.key.clone(),
            agent_name: record.name.clone(),
            status: detection.status,
            roots: Vec::new(),
            skills: Vec::new(),
            errors: Vec::new(),
        };
        let mut persisted: Vec<ScanResultRow> = Vec::new();

        for root in &roots {
            let exists = root.path.is_dir();
            let mut root_scan = RootScan {
                path: root.path.to_string_lossy().to_string(),
                scope: root.scope,
                exists,
                discovered: 0,
                invalid: 0,
                errors: Vec::new(),
            };
            if !exists {
                agent_scan.roots.push(root_scan);
                continue;
            }
            let dirs = skill::find_skill_dirs(&root.path);
            for dir in dirs {
                match skill::read_skill_dir(&dir) {
                    Ok(parsed) => {
                        let slug = parsed.slug();
                        let capability = db.find_capability("skill", &slug)?;
                        let library_skill_id =
                            capability.as_ref().map(|row| row.id.clone());
                        let version = match &capability {
                            Some(capability) => {
                                db.find_version_by_hash(&capability.id, &parsed.content_hash)?
                            }
                            None => None,
                        };
                        let library_version_id = version.as_ref().map(|row| row.id.clone());
                        let conflict = match (&capability, &version) {
                            (Some(_), None) => Some(
                                "same skill name exists in the library with different content"
                                    .to_string(),
                            ),
                            _ => None,
                        };
                        if conflict.is_some() {
                            report.conflicts += 1;
                        }
                        let mut issues = parsed.issues.clone();
                        if root.source == "builtin" {
                            issues.push("Built-in agent skill (read-only).".to_string());
                        }
                        let discovered = DiscoveredSkill {
                            name: parsed.frontmatter.name.clone(),
                            slug: slug.clone(),
                            description: parsed.frontmatter.description.clone(),
                            version: parsed.frontmatter.version.clone(),
                            path: dir.to_string_lossy().to_string(),
                            root: root.path.to_string_lossy().to_string(),
                            root_source: root.source.clone(),
                            scope: root.scope,
                            agent_key: Some(record.key.clone()),
                            content_hash: parsed.content_hash.clone(),
                            file_count: parsed.file_count,
                            size_bytes: parsed.size_bytes,
                            valid: true,
                            issues,
                            library_skill_id,
                            library_version_id,
                            in_sync: version.is_some(),
                            conflict,
                        };
                        slug_contents
                            .entry(slug.clone())
                            .or_default()
                            .entry(parsed.content_hash.clone())
                            .or_default()
                            .push(record.key.clone());
                        persisted.push(ScanResultRow {
                            agent_id: Some(record.id.clone()),
                            scope: Some(root.scope.as_str().to_string()),
                            path: dir.to_string_lossy().to_string(),
                            kind: "skill".to_string(),
                            status: "ok".to_string(),
                            fingerprint: Some(parsed.content_hash.clone()),
                            payload: serde_json::to_string(&discovered)?,
                            scanned_at: scanned_at.clone(),
                        });
                        root_scan.discovered += 1;
                        agent_scan.skills.push(discovered);
                        report.total_skills += 1;
                    }
                    Err(err) => {
                        root_scan.invalid += 1;
                        report.invalid += 1;
                        let name = dir
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        let invalid = DiscoveredSkill {
                            name: name.clone(),
                            slug: util::safe_segment(&name),
                            description: String::new(),
                            version: None,
                            path: dir.to_string_lossy().to_string(),
                            root: root.path.to_string_lossy().to_string(),
                            root_source: root.source.clone(),
                            scope: root.scope,
                            agent_key: Some(record.key.clone()),
                            content_hash: String::new(),
                            file_count: 0,
                            size_bytes: 0,
                            valid: false,
                            issues: vec![err.to_string()],
                            library_skill_id: None,
                            library_version_id: None,
                            in_sync: false,
                            conflict: None,
                        };
                        persisted.push(ScanResultRow {
                            agent_id: Some(record.id.clone()),
                            scope: Some(root.scope.as_str().to_string()),
                            path: dir.to_string_lossy().to_string(),
                            kind: "skill".to_string(),
                            status: "invalid".to_string(),
                            fingerprint: None,
                            payload: serde_json::to_string(&invalid)?,
                            scanned_at: scanned_at.clone(),
                        });
                        agent_scan.skills.push(invalid);
                    }
                }
            }
            agent_scan.roots.push(root_scan);
        }

        if detection.status == AgentStatus::Installed
            && agent_scan.roots.iter().all(|root| !root.exists)
        {
            agent_scan.errors.push(
                "Agent detected but none of its skill roots exist yet; add a project skill root or create the directory."
                    .to_string(),
            );
        }
        let _ = db.replace_scan_results(&record.id, &persisted);
        report.agents.push(agent_scan);
    }

    for (slug, variants) in &slug_contents {
        if variants.len() > 1 {
            report.conflicts += 1;
            let mut detail = Vec::new();
            for (hash, owners) in variants {
                let mut owners = owners.clone();
                owners.sort();
                owners.dedup();
                detail.push(format!("{} in {}", util::short_hash(hash), owners.join("/")));
            }
            report.warnings.push(format!(
                "Skill name '{slug}' has different content in different agents: {}",
                detail.join(", ")
            ));
        } else if let Some(owners) = variants.values().next() {
            let mut unique = owners.clone();
            unique.sort();
            unique.dedup();
            if unique.len() > 1 {
                // Same content deployed to several agents is the normal outcome
                // of distribution; report it as information, not as a conflict.
                report.duplicates += 1;
            }
        }
    }
    report.agents.sort_by(|a, b| a.agent_name.cmp(&b.agent_name));
    Ok(report)
}
