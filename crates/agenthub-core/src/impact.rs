//! Update impact analysis between two skill versions.
//!
//! The comparison is purely local and deterministic: files are compared by
//! content bytes, symlinks by their target string, and the new version is
//! passed through the static audit so existing risks are carried forward.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::Path;

use walkdir::WalkDir;

use crate::audit::{
    audit_skill_dir, discover_scripts, extract_domains, read_text_limited, severity_rank,
    shebang_line, MAX_SCAN_BYTES,
};
use crate::deps::{manifest_dependency_names, manifest_file_names};
use crate::model::{AuditFinding, FileChange};
use crate::util;

/// `FileChange` entries are truncated to this many rows; the summary keeps the
/// real total so the UI can state "showing first N".
const MAX_FILE_CHANGES: usize = 500;

#[derive(Debug, Clone)]
pub struct ImpactResult {
    pub risk: String,
    pub summary: String,
    pub file_changes: Vec<FileChange>,
    pub classes: Vec<String>,
    pub findings: Vec<AuditFinding>,
    pub requires_revalidation: bool,
}

#[derive(Debug, Clone)]
struct EntryInfo {
    size: i64,
    mode: Option<u32>,
    is_symlink: bool,
    symlink_target: Option<String>,
    is_script: bool,
    is_binary: bool,
}

pub fn analyze_impact(old_dir: &Path, new_dir: &Path) -> ImpactResult {
    let old_entries = snapshot(old_dir);
    let new_entries = snapshot(new_dir);

    let all_paths: BTreeSet<&String> = old_entries.keys().chain(new_entries.keys()).collect();

    let mut total_changes = 0usize;
    let mut changed_paths: Vec<String> = Vec::new();
    let mut file_changes: Vec<FileChange> = Vec::new();
    let mut skill_md_changed = false;
    let mut added_scripts: Vec<String> = Vec::new();
    let mut removed_scripts: Vec<String> = Vec::new();
    let mut script_changed = false;
    let mut added_binaries: Vec<String> = Vec::new();
    let mut permission_changes: Vec<(String, u32, u32)> = Vec::new();
    let mut files_added = 0usize;
    let mut files_removed = 0usize;

    for path in &all_paths {
        let old_entry = old_entries.get(*path);
        let new_entry = new_entries.get(*path);
        let change = match (old_entry, new_entry) {
            (Some(old), None) => {
                files_removed += 1;
                if old.is_script {
                    removed_scripts.push((*path).clone());
                }
                Some("removed")
            }
            (None, Some(new)) => {
                files_added += 1;
                if new.is_script {
                    added_scripts.push((*path).clone());
                }
                if new.is_binary {
                    added_binaries.push((*path).clone());
                }
                Some("added")
            }
            (Some(old), Some(new)) => {
                if !same_entry(old_dir, new_dir, path, old, new) {
                    if old.is_script || new.is_script {
                        script_changed = true;
                    }
                    Some("modified")
                } else {
                    None
                }
            }
            (None, None) => None,
        };

        if let (Some(old), Some(new)) = (old_entry, new_entry) {
            if !old.is_symlink && !new.is_symlink {
                if let (Some(before), Some(after)) = (old.mode, new.mode) {
                    if before & 0o7777 != after & 0o7777 {
                        permission_changes.push(((*path).clone(), before, after));
                    }
                }
            }
        }

        let Some(change) = change else {
            continue;
        };
        total_changes += 1;
        changed_paths.push((*path).clone());
        if path.as_str() == "SKILL.md" {
            skill_md_changed = true;
        }
        if file_changes.len() < MAX_FILE_CHANGES {
            let (size_before, size_after) = match (old_entry, new_entry) {
                (Some(old), Some(new)) => (old.size, new.size),
                (Some(old), None) => (old.size, 0),
                (None, Some(new)) => (0, new.size),
                (None, None) => (0, 0),
            };
            file_changes.push(FileChange {
                path: (*path).clone(),
                change: change.to_string(),
                size_before,
                size_after,
            });
        }
    }

    let old_domains = collect_domains(old_dir);
    let new_domains = collect_domains(new_dir);
    let added_domains: BTreeSet<String> = new_domains.difference(&old_domains).cloned().collect();
    let removed_domains: BTreeSet<String> = old_domains.difference(&new_domains).cloned().collect();

    let old_manifests = manifest_file_names(old_dir);
    let new_manifests = manifest_file_names(new_dir);
    let old_dependencies = manifest_dependency_names(old_dir);
    let new_dependencies = manifest_dependency_names(new_dir);
    let new_dependency = new_manifests.difference(&old_manifests).next().is_some()
        || new_dependencies.difference(&old_dependencies).next().is_some();
    let removed_dependency = old_manifests.difference(&new_manifests).next().is_some()
        || old_dependencies.difference(&new_dependencies).next().is_some();

    let docs_only = total_changes > 0
        && !skill_md_changed
        && changed_paths.iter().all(|path| is_doc_path(path));
    let max_files = old_entries.len().max(new_entries.len()).max(1);
    let large_change = total_changes as f64 / max_files as f64 > 0.5;

    let old_audit = old_dir.is_dir().then(|| audit_skill_dir(old_dir));
    let new_audit = new_dir.is_dir().then(|| audit_skill_dir(new_dir));
    let risk_increased = match (&old_audit, &new_audit) {
        (Some(old), Some(new)) => severity_rank(&new.risk) > severity_rank(&old.risk),
        _ => false,
    };

    let mut findings: Vec<AuditFinding> = Vec::new();
    if let Some(audit) = new_audit.as_ref() {
        findings.extend(
            audit
                .findings
                .iter()
                .filter(|finding| severity_rank(&finding.severity) >= 2)
                .cloned(),
        );
    }
    if risk_increased {
        findings.push(AuditFinding {
            id: "impact.risk_increase".to_string(),
            severity: "medium".to_string(),
            category: "compatibility".to_string(),
            title: "Static audit risk increased".to_string(),
            detail: format!(
                "Static audit risk moved from {} to {}.",
                old_audit
                    .as_ref()
                    .map(|audit| audit.risk.as_str())
                    .unwrap_or("unknown"),
                new_audit
                    .as_ref()
                    .map(|audit| audit.risk.as_str())
                    .unwrap_or("unknown")
            ),
            path: None,
            suggestion: Some(
                "Review the new findings and re-run manual validation on the installed agents."
                    .to_string(),
            ),
        });
    }
    for path in added_scripts.iter().take(50) {
        findings.push(AuditFinding {
            id: "impact.new_script".to_string(),
            severity: "high".to_string(),
            category: "security".to_string(),
            title: "New script file".to_string(),
            detail: format!("{path} was added and introduces new executable behavior."),
            path: Some(path.clone()),
            suggestion: Some(
                "Review the new script and re-validate the capability before rollout.".to_string(),
            ),
        });
    }
    if !added_domains.is_empty() {
        let sample: Vec<&str> = added_domains.iter().take(5).map(String::as_str).collect();
        findings.push(AuditFinding {
            id: "impact.new_network_domain".to_string(),
            severity: "medium".to_string(),
            category: "security".to_string(),
            title: "New remote network domain".to_string(),
            detail: format!(
                "{} new remote domain(s): {}",
                added_domains.len(),
                sample.join(", ")
            ),
            path: None,
            suggestion: Some(
                "Confirm the new endpoint is expected and documented before rollout.".to_string(),
            ),
        });
    }
    for path in added_binaries.iter().take(20) {
        findings.push(AuditFinding {
            id: "impact.new_binary".to_string(),
            severity: "high".to_string(),
            category: "security".to_string(),
            title: "New executable binary".to_string(),
            detail: format!("{path} is a new executable binary; binaries cannot be audited as text."),
            path: Some(path.clone()),
            suggestion: Some(
                "Verify the binary's provenance and signature before distribution.".to_string(),
            ),
        });
    }
    for (path, before, after) in permission_changes.iter().take(20) {
        findings.push(AuditFinding {
            id: "impact.permission_change".to_string(),
            severity: "high".to_string(),
            category: "security".to_string(),
            title: "File permissions changed".to_string(),
            detail: format!("{path} changed mode from {before:o} to {after:o}."),
            path: Some(path.clone()),
            suggestion: Some(
                "Review executable/permission changes before distributing the update.".to_string(),
            ),
        });
    }

    let mut classes: Vec<String> = Vec::new();
    if docs_only {
        classes.push("docs_only".to_string());
    }
    if skill_md_changed {
        classes.push("skill_md_changed".to_string());
    }
    if !added_scripts.is_empty() {
        classes.push("new_script".to_string());
    }
    if !removed_scripts.is_empty() {
        classes.push("removed_script".to_string());
    }
    if script_changed {
        classes.push("script_changed".to_string());
    }
    if !added_domains.is_empty() {
        classes.push("new_network_domain".to_string());
    }
    if !removed_domains.is_empty() {
        classes.push("removed_network_domain".to_string());
    }
    if new_dependency {
        classes.push("new_dependency".to_string());
    }
    if removed_dependency {
        classes.push("removed_dependency".to_string());
    }
    if !added_binaries.is_empty() {
        classes.push("new_binary".to_string());
    }
    if files_added > 0 {
        classes.push("files_added".to_string());
    }
    if files_removed > 0 {
        classes.push("files_removed".to_string());
    }
    if large_change {
        classes.push("large_change".to_string());
    }

    let mut max_rank = findings
        .iter()
        .map(|finding| severity_rank(&finding.severity))
        .max()
        .unwrap_or(0);
    if new_dependency || removed_dependency || large_change {
        max_rank = max_rank.max(2);
    }
    // Impact risk is deliberately capped at `high`; the concrete finding
    // severities still carry `critical` where the audit found one.
    let risk = if max_rank >= 3 {
        "high"
    } else if max_rank == 2 {
        "medium"
    } else {
        "low"
    };

    let requires_revalidation = skill_md_changed
        || !added_scripts.is_empty()
        || script_changed
        || !added_domains.is_empty()
        || !removed_domains.is_empty()
        || new_dependency
        || removed_dependency
        || risk_increased
        || !added_binaries.is_empty()
        || !permission_changes.is_empty();

    findings = sort_findings(findings);

    let summary = summarize(
        total_changes,
        file_changes.len(),
        &changed_paths,
        &added_scripts,
        &removed_scripts,
        &added_domains,
        &removed_domains,
        &added_binaries,
        &permission_changes,
        docs_only,
        skill_md_changed,
        new_dependency,
        removed_dependency,
        large_change,
        risk,
        requires_revalidation,
    );

    ImpactResult {
        risk: risk.to_string(),
        summary,
        file_changes,
        classes,
        findings,
        requires_revalidation,
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, EntryInfo> {
    let mut entries = BTreeMap::new();
    if !root.is_dir() {
        return entries;
    }
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.path() == root || !util::is_ignored_path(entry.path()))
        .flatten()
    {
        if entry.path() == root {
            continue;
        }
        let Some(rel) = relative_path(root, entry.path()) else {
            continue;
        };
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            entries.insert(
                rel,
                EntryInfo {
                    size: 0,
                    mode: None,
                    is_symlink: true,
                    symlink_target: fs::read_link(entry.path())
                        .ok()
                        .map(|target| target.to_string_lossy().to_string()),
                    is_script: false,
                    is_binary: false,
                },
            );
        } else if file_type.is_file() {
            let meta = fs::symlink_metadata(entry.path()).ok();
            let mode = meta.as_ref().map(mode_from_metadata);
            let executable = mode.map(|mode| mode & 0o111 != 0).unwrap_or(false);
            let has_shebang = shebang_line(entry.path()).is_some();
            let is_binary = executable && !has_shebang && looks_binary(entry.path());
            entries.insert(
                rel,
                EntryInfo {
                    size: meta.as_ref().map(|meta| meta.len() as i64).unwrap_or(0),
                    mode,
                    is_symlink: false,
                    symlink_target: None,
                    is_script: has_shebang || (executable && !is_binary),
                    is_binary,
                },
            );
        }
    }
    entries
}

fn mode_from_metadata(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        0
    }
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .filter(|rel| !rel.as_os_str().is_empty())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

fn same_entry(
    old_root: &Path,
    new_root: &Path,
    rel: &str,
    old: &EntryInfo,
    new: &EntryInfo,
) -> bool {
    match (old.is_symlink, new.is_symlink) {
        (true, true) => old.symlink_target == new.symlink_target,
        (false, false) => {
            if old.size != new.size {
                return false;
            }
            files_equal(&old_root.join(rel), &new_root.join(rel))
        }
        _ => false,
    }
}

fn files_equal(left: &Path, right: &Path) -> bool {
    match (fs::read(left), fs::read(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn looks_binary(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut buffer = [0u8; 1024];
    let read = file.read(&mut buffer).unwrap_or(0);
    buffer[..read].contains(&0)
}

fn collect_domains(root: &Path) -> BTreeSet<String> {
    let mut domains = BTreeSet::new();
    if !root.is_dir() {
        return domains;
    }
    for script in discover_scripts(root) {
        if let Some(content) = script.content.as_deref() {
            domains.extend(extract_domains(content));
        }
    }
    if let Some(skill_md) = read_text_limited(&root.join("SKILL.md"), MAX_SCAN_BYTES) {
        domains.extend(extract_domains(&skill_md));
    }
    domains
}

fn is_doc_path(path: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(extension.as_str(), "md" | "txt")
}

fn sort_findings(findings: Vec<AuditFinding>) -> Vec<AuditFinding> {
    let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut unique = Vec::new();
    for finding in findings {
        let key = (
            finding.id.clone(),
            finding.path.clone().unwrap_or_default(),
            finding.detail.clone(),
        );
        if seen.insert(key) {
            unique.push(finding);
        }
    }
    unique.sort_by(|left, right| {
        severity_rank(&right.severity)
            .cmp(&severity_rank(&left.severity))
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left.path.cmp(&right.path))
    });
    unique
}

#[allow(clippy::too_many_arguments)]
fn summarize(
    total_changes: usize,
    shown_changes: usize,
    changed_paths: &[String],
    added_scripts: &[String],
    removed_scripts: &[String],
    added_domains: &BTreeSet<String>,
    removed_domains: &BTreeSet<String>,
    added_binaries: &[String],
    permission_changes: &[(String, u32, u32)],
    docs_only: bool,
    skill_md_changed: bool,
    new_dependency: bool,
    removed_dependency: bool,
    large_change: bool,
    risk: &str,
    requires_revalidation: bool,
) -> String {
    let revalidation = if requires_revalidation {
        "required"
    } else {
        "not required"
    };
    if total_changes == 0 {
        if permission_changes.is_empty() {
            return format!("no file changes: {risk} risk, revalidation {revalidation}");
        }
        return format!(
            "file metadata changes only (permission changes): {risk} risk, revalidation {revalidation}"
        );
    }

    let mut details: Vec<String> = Vec::new();
    if !added_scripts.is_empty() {
        details.push(format!("+{} script(s)", added_scripts.len()));
    }
    if !removed_scripts.is_empty() {
        details.push(format!("-{} script(s)", removed_scripts.len()));
    }
    if !added_domains.is_empty() {
        let sample: Vec<&str> = added_domains.iter().take(3).map(String::as_str).collect();
        details.push(format!("new domain {}", sample.join(", ")));
    }
    if !removed_domains.is_empty() {
        let sample: Vec<&str> = removed_domains.iter().take(3).map(String::as_str).collect();
        details.push(format!("removed domain {}", sample.join(", ")));
    }
    if !added_binaries.is_empty() {
        details.push(format!("+{} binary(ies)", added_binaries.len()));
    }
    if !permission_changes.is_empty() {
        details.push("permission changes".to_string());
    }
    if docs_only {
        details.push("docs only".to_string());
    } else if skill_md_changed {
        details.push("SKILL.md changed".to_string());
    }
    if new_dependency {
        details.push("new dependencies".to_string());
    }
    if removed_dependency {
        details.push("removed dependencies".to_string());
    }
    if large_change {
        details.push("large change".to_string());
    }
    if details.is_empty() {
        details.push("review files".to_string());
    }

    if shown_changes < total_changes {
        details.insert(0, format!("showing {shown_changes} of {}", changed_paths.len()));
    }
    format!(
        "{total_changes} file{} changed ({}): {risk} risk, revalidation {revalidation}",
        if total_changes == 1 { "" } else { "s" },
        details.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn skill_md() -> &'static str {
        "---\nname: impact-test\ndescription: A deterministic test skill used to exercise update impact analysis.\nlicense: MIT\n---\n\nBody\n"
    }

    fn make_version(root: &Path, body: &str) -> PathBuf {
        write_file(&root.join("SKILL.md"), &format!("{}{}", skill_md(), body));
        root.to_path_buf()
    }

    fn make_script(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        write_file(&path, contents);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
    }

    fn finding<'a>(result: &'a ImpactResult, id: &str) -> &'a AuditFinding {
        result
            .findings
            .iter()
            .find(|finding| finding.id == id)
            .unwrap_or_else(|| panic!("expected finding {id}, got {:?}", result.findings))
    }

    #[test]
    fn docs_only_change_does_not_require_revalidation() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Old docs");
        write_file(&old.path().join("README.md"), "old\n");
        make_version(new.path(), "Old docs");
        write_file(&new.path().join("README.md"), "new text\n");

        let result = analyze_impact(old.path(), new.path());
        assert!(result.classes.contains(&"docs_only".to_string()));
        assert!(!result.requires_revalidation);
        assert_eq!(result.risk, "low");
        assert_eq!(result.file_changes.len(), 1);
        assert_eq!(result.file_changes[0].path, "README.md");
        assert_eq!(result.file_changes[0].change, "modified");
        assert!(result.summary.contains("docs only"));
    }

    #[test]
    fn new_script_and_new_domain_require_revalidation() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Stable version");
        make_version(new.path(), "Stable version");
        make_script(
            new.path(),
            "scripts/run.sh",
            "#!/bin/bash\ncurl -fsSL https://api.example.com/data -o data.json\n",
        );

        let result = analyze_impact(old.path(), new.path());
        assert!(result.classes.contains(&"new_script".to_string()));
        assert!(result.classes.contains(&"new_network_domain".to_string()));
        assert!(result.requires_revalidation);
        assert_eq!(result.risk, "high");
        assert_eq!(finding(&result, "impact.new_script").severity, "high");
        let network = finding(&result, "impact.new_network_domain");
        assert_eq!(network.severity, "medium");
        assert!(network.detail.contains("api.example.com"));
        assert!(result.summary.contains("+1 script"));
    }

    #[test]
    fn dependency_addition_is_medium_and_requires_revalidation() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Stable version");
        make_version(new.path(), "Stable version");
        write_file(&old.path().join("requirements.txt"), "requests>=2.0\n");
        write_file(
            &new.path().join("requirements.txt"),
            "requests>=2.0\nflask==3.0.0\n",
        );

        let result = analyze_impact(old.path(), new.path());
        assert!(result.classes.contains(&"new_dependency".to_string()));
        assert!(result.requires_revalidation);
        assert_eq!(result.risk, "medium");
        assert_eq!(result.file_changes[0].change, "modified");
    }

    #[test]
    fn new_manifest_file_counts_as_dependency_change() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Stable version");
        make_version(new.path(), "Stable version");
        write_file(
            &new.path().join("package.json"),
            r#"{"name":"fixture","dependencies":{"undici":"6.0.0"}}"#,
        );

        let result = analyze_impact(old.path(), new.path());
        assert!(result.classes.contains(&"new_dependency".to_string()));
        assert!(result.requires_revalidation);
        assert!(result.classes.contains(&"files_added".to_string()));
    }

    #[test]
    fn permission_change_is_high_and_requires_revalidation() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Stable version");
        make_version(new.path(), "Stable version");
        write_file(
            &old.path().join("helper.py"),
            "#!/usr/bin/env python3\nprint('ok')\n",
        );
        write_file(
            &new.path().join("helper.py"),
            "#!/usr/bin/env python3\nprint('ok')\n",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = new.path().join("helper.py");
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }

        let result = analyze_impact(old.path(), new.path());
        let finding = finding(&result, "impact.permission_change");
        assert_eq!(finding.severity, "high");
        assert!(result.requires_revalidation);
        assert_eq!(result.risk, "high");
    }

    #[test]
    fn skill_md_change_is_classified() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Old instructions");
        make_version(new.path(), "New instructions");
        let result = analyze_impact(old.path(), new.path());
        assert!(result.classes.contains(&"skill_md_changed".to_string()));
        assert!(!result.classes.contains(&"docs_only".to_string()));
        assert!(result.requires_revalidation);
        assert_eq!(result.file_changes.len(), 1);
        assert_eq!(result.file_changes[0].path, "SKILL.md");
    }

    #[test]
    fn removed_script_is_classified_but_not_a_new_script() {
        let old = tempfile::tempdir().unwrap();
        let new = tempfile::tempdir().unwrap();
        make_version(old.path(), "Stable version");
        make_version(new.path(), "Stable version");
        make_script(old.path(), "scripts/old.sh", "#!/bin/bash\necho old\n");

        let result = analyze_impact(old.path(), new.path());
        assert!(result.classes.contains(&"removed_script".to_string()));
        assert!(result.classes.contains(&"files_removed".to_string()));
        assert!(!result.classes.contains(&"new_script".to_string()));
        assert_eq!(result.file_changes[0].change, "removed");
    }
}
