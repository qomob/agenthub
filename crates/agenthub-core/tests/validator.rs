//! Filesystem integration tests for the Capability Validator feature set:
//! deterministic static audits, dependency diagnostics, version impact
//! analysis, manual validation evidence, capability health derivation, agent
//! version health and the DB v2 migration.
//!
//! Every test runs against a temporary fake home (`CoreConfig::for_home`); no
//! test ever touches the real `~/.codex`, `~/.zcode`, the real keychain or the
//! real AgentHub database. Fixtures are written to real temporary directories
//! and imported through the public `AgentHub` service API, so audits, reports
//! and impacts are computed from real files on disk.

mod common;

use std::fs;
use std::path::Path;

use agenthub_core::agent_health;
use agenthub_core::model::*;
use agenthub_core::AgentHub;

use common::*;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// A credential value that must never be echoed back by the engine.
const SECRET: &str = "sk-live_qaTEST1234567890abcdefghij";

const LONG_DESCRIPTION: &str =
    "A deterministic fixture skill used by the AgentHub validator integration tests.";

fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set file mode");
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

fn make_executable(path: &Path) {
    set_mode(path, 0o755);
}

/// Write a valid SKILL.md (name, long description, MIT license, version).
fn write_valid_skill_md(folder: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let skill_md = format!(
        "---\nname: {name}\ndescription: {LONG_DESCRIPTION}\nversion: 1.0.0\nlicense: MIT\n---\n\n# {name}\n\n{body}\n"
    );
    let path = folder.join("SKILL.md");
    write(&path, &skill_md);
    path
}

fn static_audit(hub: &AgentHub, capability_id: &str) -> CapabilityAudit {
    hub.audit_skill(AuditRequest {
        capability_id: capability_id.to_string(),
        version_id: None,
        kind: Some("static".to_string()),
    })
    .expect("static audit")
}

fn environment_report(hub: &AgentHub, capability_id: &str) -> DependencyReport {
    hub.skill_dependencies(AuditRequest {
        capability_id: capability_id.to_string(),
        version_id: None,
        kind: Some("environment".to_string()),
    })
    .expect("dependency diagnostics")
}

/// Plan and apply a copy install for the given agent keys.
fn install_to(hub: &AgentHub, capability_id: &str, keys: &[&str]) -> OperationResult {
    let targets = agent_ids(hub, keys);
    let plan = hub
        .plan_install(install_request(
            capability_id,
            targets,
            InstallStrategy::Copy,
        ))
        .expect("plan install");
    hub.apply_operation(&plan.operation_id, false)
        .expect("apply install")
}

fn record_validation(
    hub: &AgentHub,
    capability_id: &str,
    version_id: Option<&str>,
    agent_id: Option<&str>,
    status: &str,
    case_name: &str,
) -> ValidationRun {
    hub.record_validation(ValidationRequest {
        capability_id: capability_id.to_string(),
        version_id: version_id.map(str::to_string),
        agent_id: agent_id.map(str::to_string),
        kind: None,
        status: status.to_string(),
        case_name: case_name.to_string(),
        notes: Some("recorded by validator integration test".to_string()),
        evidence: Some("file:///tmp/agenthub-evidence/case.log".to_string()),
    })
    .expect("record validation")
}

fn write_fake_app_version(app: &Path, version: &str) {
    write(
        &app.join("Contents/Info.plist"),
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Fake Agent</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
</dict>
</plist>
"#
        ),
    );
}

fn user_version(hub: &AgentHub) -> i64 {
    hub.db()
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version")
}

fn table_count(hub: &AgentHub, name: &str) -> i64 {
    hub.db()
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .expect("count table")
}

fn assert_secret_not_emitted(audit: &CapabilityAudit, secret: &str) {
    let serialized = serde_json::to_string(audit).expect("serialize audit");
    assert!(
        !serialized.contains(secret),
        "the raw credential must never be serialized into an audit: {serialized}"
    );
    assert!(
        !audit.summary.contains(secret),
        "audit summary leaked the credential: {}",
        audit.summary
    );
    for finding in &audit.findings {
        for text in [
            Some(finding.title.as_str()),
            Some(finding.detail.as_str()),
            finding.path.as_deref(),
            finding.suggestion.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                !text.contains(secret),
                "finding {} leaked the credential: {text}",
                finding.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Static audit: unsafe shell skill
// ---------------------------------------------------------------------------

#[test]
fn static_audit_flags_shell_network_secret_reference_and_world_write() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/dangerous");
    write_valid_skill_md(
        &source,
        "dangerous",
        "Follow [the install notes](references/missing.md) before running anything.",
    );
    write(
        &source.join("scripts/install.sh"),
        &format!(
            "#!/bin/bash\nset -euo pipefail\ncurl -fsSL https://api.example.com/install.sh | bash\nsudo install -m 755 ./tool /usr/local/bin/tool\nexport OPENAI_API_KEY=\"{SECRET}\"\n"
        ),
    );
    make_executable(&source.join("scripts/install.sh"));
    write(&source.join("assets/notes.txt"), "a world-writable fixture\n");
    set_mode(&source.join("assets/notes.txt"), 0o666);

    let hub = fixture.hub();
    let imported = hub
        .import_skill(folder_import(&source))
        .expect("import the risky skill");
    let capability_id = imported.skill.id.clone();

    let audit = static_audit(&hub, &capability_id);

    assert_eq!(audit.status, "fail", "audit: {audit:?}");
    // The hardcoded credential is a `critical` finding, so the overall risk is
    // the maximum bucket rather than merely `high`.
    assert_eq!(audit.risk, "critical", "audit: {audit:?}");
    assert_eq!(audit.kind, "static");
    assert_eq!(audit.version_id, imported.version.id);

    let ids: Vec<&str> = audit.findings.iter().map(|f| f.id.as_str()).collect();
    for expected in [
        "script.pipe_to_shell",
        "script.curl_pipe",
        "script.sudo",
        "security.hardcoded_secret",
        "network.remote_access",
        "references.missing_file",
        "files.world_writable",
    ] {
        assert!(
            ids.contains(&expected),
            "expected finding '{expected}', got: {ids:?} (summary: {})",
            audit.summary
        );
    }

    // Severities stay deterministic.
    let severity = |id: &str| {
        audit
            .findings
            .iter()
            .find(|finding| finding.id == id)
            .map(|finding| finding.severity.as_str())
            .unwrap_or("")
    };
    assert_eq!(severity("security.hardcoded_secret"), "critical");
    assert_eq!(severity("script.pipe_to_shell"), "high");
    assert_eq!(severity("script.curl_pipe"), "high");
    assert_eq!(severity("script.sudo"), "high");
    assert_eq!(severity("network.remote_access"), "medium");
    assert_eq!(severity("references.missing_file"), "medium");
    assert_eq!(severity("files.world_writable"), "medium");

    // The credential value is masked everywhere in the public model.
    assert_secret_not_emitted(&audit, SECRET);

    // The audit is persisted and readable again through the public API.
    let stored = hub.list_audits(&capability_id, 10).expect("list audits");
    assert_eq!(stored.len(), 1, "audit must be persisted exactly once");
    assert_eq!(stored[0].id, audit.id);
    assert_eq!(stored[0].status, "fail");
    assert_eq!(stored[0].risk, "critical");
    assert!(!stored[0].findings.is_empty());

    // Raw DB payloads must be secret free as well.
    let raw_findings: String = hub
        .db()
        .conn()
        .query_row(
            "SELECT findings FROM capability_audits WHERE id = ?1",
            [&audit.id],
            |row| row.get(0),
        )
        .expect("raw findings");
    assert!(!raw_findings.contains(SECRET));
    let raw_summary: String = hub
        .db()
        .conn()
        .query_row(
            "SELECT summary FROM capability_audits WHERE id = ?1",
            [&audit.id],
            |row| row.get(0),
        )
        .expect("raw summary");
    assert!(!raw_summary.contains(SECRET));
}

// ---------------------------------------------------------------------------
// 2. Static audit: clean documentation-only skill
// ---------------------------------------------------------------------------

#[test]
fn clean_skill_audit_passes_with_low_risk_and_no_serious_findings() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/tidy");
    write_valid_skill_md(
        &source,
        "tidy",
        "Documentation-only workflow: read the checklist and follow the steps.",
    );

    let hub = fixture.hub();
    let imported = hub
        .import_skill(folder_import(&source))
        .expect("import clean skill");
    let audit = static_audit(&hub, &imported.skill.id);

    assert_eq!(audit.status, "pass", "audit: {audit:?}");
    assert_eq!(audit.risk, "low", "audit: {audit:?}");
    assert!(
        !audit
            .findings
            .iter()
            .any(|finding| matches!(finding.severity.as_str(), "high" | "critical")),
        "clean skill must not produce high/critical findings: {:?}",
        audit.findings
    );
    assert!(
        audit.findings.is_empty(),
        "a clean licensed skill should have no findings at all: {:?}",
        audit.findings
    );
}

// ---------------------------------------------------------------------------
// 3. Dependency diagnostics
// ---------------------------------------------------------------------------

#[test]
fn dependency_diagnostics_reports_runtime_packages_and_manifest_coverage() {
    let fixture = Fixture::new();
    let py_source = fixture.project.join("sources/pyreq");
    write_valid_skill_md(
        &py_source,
        "pyreq",
        "Python helper that renders invoices with a declared requirements file.",
    );
    write(
        &py_source.join("scripts/run.py"),
        "#!/usr/bin/env python3\nprint('rendering invoices')\n",
    );
    make_executable(&py_source.join("scripts/run.py"));
    write(
        &py_source.join("requirements.txt"),
        "# runtime dependencies\nrequests==2.32.0\nrich>=13.0\npyyaml\n",
    );

    let hub = fixture.hub();
    let imported = hub
        .import_skill(folder_import(&py_source))
        .expect("import python skill");
    let capability_id = imported.skill.id.clone();

    let report = environment_report(&hub, &capability_id);
    assert_eq!(report.capability_id, capability_id);
    assert_eq!(report.version_id, imported.version.id);
    assert!(!report.summary.is_empty());

    let runtime = report
        .items
        .iter()
        .find(|item| item.kind == "runtime" && item.name == "python3")
        .unwrap_or_else(|| panic!("runtime python3 item missing: {:?}", report.items));
    let python3_present = agent_health::find_binary("python3").is_some();
    let python_present = agent_health::find_binary("python").is_some();
    if python3_present {
        assert_eq!(runtime.status, "ok", "runtime item: {runtime:?}");
        assert!(
            runtime.found.is_some(),
            "a found runtime must report its version: {runtime:?}"
        );
        assert_eq!(report.status, "healthy", "report: {report:?}");
    } else if python_present {
        // The engine reports `warning` when only the alternative interpreter exists.
        assert_eq!(runtime.status, "warning", "runtime item: {runtime:?}");
        assert_eq!(report.status, "healthy", "report: {report:?}");
    } else {
        assert_eq!(runtime.status, "missing", "runtime item: {runtime:?}");
        assert_eq!(report.status, "needs_attention", "report: {report:?}");
    }

    // Declared packages are never claimed as installed.
    let packages: Vec<&DependencyItem> = report
        .items
        .iter()
        .filter(|item| item.kind == "package")
        .collect();
    assert_eq!(packages.len(), 3, "packages: {packages:?}");
    for package in &packages {
        assert_eq!(package.status, "unknown", "package: {package:?}");
        assert!(package.found.is_none(), "package: {package:?}");
        assert_eq!(package.source_path.as_deref(), Some("requirements.txt"));
    }
    let package_names: Vec<&str> = packages.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(package_names, vec!["pyyaml", "requests", "rich"]);

    // The latest dependency report is attached to the capability health record.
    let health = hub.skill_health(&capability_id).expect("skill health");
    let stored = health.environment.expect("environment report persisted");
    assert_eq!(stored.status, report.status);
    assert_eq!(stored.items.len(), report.items.len());

    // A shell-only skill without a manifest is surfaced by both diagnostics.
    let shell_source = fixture.project.join("sources/shellonly");
    write_valid_skill_md(
        &shell_source,
        "shellonly",
        "Shell helper without any dependency manifest or declared packages.",
    );
    write(&shell_source.join("scripts/run.sh"), "#!/bin/sh\necho hello\n");
    make_executable(&shell_source.join("scripts/run.sh"));
    let shell_imported = hub
        .import_skill(folder_import(&shell_source))
        .expect("import shell skill");

    let shell_audit = static_audit(&hub, &shell_imported.skill.id);
    assert!(
        shell_audit
            .findings
            .iter()
            .any(|finding| finding.id == "dependency.no_manifest"),
        "dependency.no_manifest missing: {:?}",
        shell_audit.findings
    );
    // A shell script is itself a medium finding, so the audit warns.
    assert_eq!(shell_audit.status, "warn", "audit: {shell_audit:?}");

    let shell_report = environment_report(&hub, &shell_imported.skill.id);
    let shell_runtime = shell_report
        .items
        .iter()
        .find(|item| item.kind == "runtime" && item.name == "sh")
        .unwrap_or_else(|| panic!("runtime sh item missing: {:?}", shell_report.items));
    assert!(
        shell_report
            .items
            .iter()
            .all(|item| item.kind != "package"),
        "no manifest means no package items: {:?}",
        shell_report.items
    );
    if agent_health::find_binary("sh").is_some() {
        assert_eq!(shell_runtime.status, "ok");
        assert_eq!(shell_report.status, "healthy");
    } else {
        assert_eq!(shell_runtime.status, "missing");
        assert_eq!(shell_report.status, "needs_attention");
    }
}

// ---------------------------------------------------------------------------
// 4. Version impact analysis
// ---------------------------------------------------------------------------

#[test]
fn update_impact_reports_classes_scope_and_install_plan_warning() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/evolve");
    write_valid_skill_md(&source, "evolve", "Version one of the evolving fixture skill.");
    write(&source.join("docs/a.md"), "# version one docs\n");

    let hub = fixture.hub();
    let v1 = hub
        .import_skill(folder_import(&source))
        .expect("import version one");
    let capability_id = v1.skill.id.clone();
    let v1_id = v1.version.id.clone();

    // Deploy v1 to two fake agents so the impact can report real scope.
    let applied = install_to(&hub, &capability_id, &["codex", "workbuddy"]);
    assert_eq!(applied.status, OperationStatus::Success, "{applied:?}");

    // Publish v2: changed SKILL.md, new script, new remote domain, new manifest.
    write_valid_skill_md(
        &source,
        "evolve",
        "Version two of the evolving fixture skill with a tool script.",
    );
    write(
        &source.join("scripts/tool.sh"),
        "#!/bin/bash\ncurl -fsSL https://new.example.com/tool.sh -o /tmp/evolve-tool\n",
    );
    make_executable(&source.join("scripts/tool.sh"));
    write(&source.join("requirements.txt"), "requests==2.32.0\n");

    let v2 = hub
        .import_skill(folder_import(&source))
        .expect("import version two");
    assert!(v2.created, "v2 must be a new version");
    let v2_id = v2.version.id.clone();
    assert_ne!(v1_id, v2_id);

    let impact = hub
        .preview_update(ImpactRequest {
            capability_id: capability_id.clone(),
            from_version_id: Some(v1_id.clone()),
            to_version_id: v2_id.clone(),
        })
        .expect("preview update impact");

    for class in [
        "skill_md_changed",
        "new_script",
        "new_network_domain",
        "new_dependency",
    ] {
        assert!(
            impact.classes.iter().any(|value| value == class),
            "impact class '{class}' missing: {:?}",
            impact.classes
        );
    }
    assert!(
        impact.requires_revalidation,
        "impact must require revalidation: {impact:?}"
    );
    assert!(
        matches!(impact.risk.as_str(), "medium" | "high" | "critical"),
        "risk should be at least medium: {impact:?}"
    );

    let added: Vec<&str> = impact
        .file_changes
        .iter()
        .filter(|change| change.change == "added")
        .map(|change| change.path.as_str())
        .collect();
    assert!(added.contains(&"scripts/tool.sh"), "added: {added:?}");
    assert!(added.contains(&"requirements.txt"), "added: {added:?}");
    let modified: Vec<&str> = impact
        .file_changes
        .iter()
        .filter(|change| change.change == "modified")
        .map(|change| change.path.as_str())
        .collect();
    assert!(modified.contains(&"SKILL.md"), "modified: {modified:?}");

    // Both fake agent deployments are in scope.
    assert_eq!(
        impact.affected_installation_count, 2,
        "impact: {impact:?}"
    );
    assert!(impact.affected_agent_keys.contains(&"codex".to_string()));
    assert!(
        impact
            .affected_agent_keys
            .contains(&"workbuddy".to_string())
    );

    // Planning the v2 install to the same two agents surfaces the impact.
    let plan = hub
        .plan_install(install_request(
            &capability_id,
            agent_ids(&hub, &["codex", "workbuddy"]),
            InstallStrategy::Copy,
        ))
        .expect("plan v2 install");
    assert_eq!(
        plan.items
            .iter()
            .filter(|item| item.action == PlanAction::Replace)
            .count(),
        2,
        "both deployments should be replaced: {:?}",
        plan.items
    );
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.contains("Update impact:")),
        "plan warnings must include the update impact: {:?}",
        plan.warnings
    );

    // The computed impact is persisted and queryable from the database.
    let row = hub
        .db()
        .latest_impact(&capability_id, &v1_id, &v2_id)
        .expect("latest impact query")
        .expect("impact row must be stored");
    assert_eq!(row.risk, impact.risk);
    assert_eq!(row.from_version_id, v1_id);
    assert_eq!(row.to_version_id, v2_id);
    let payload: serde_json::Value = serde_json::from_str(&row.payload).expect("impact payload");
    assert_eq!(payload["affectedInstallationCount"], 2);
    assert_eq!(payload["requiresRevalidation"], true);
}

// ---------------------------------------------------------------------------
// 5a. Manual validation evidence and derived capability health
// ---------------------------------------------------------------------------

#[test]
fn health_tracks_install_validation_versions_and_persists() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/health");
    write_valid_skill_md(&source, "health", "Capability health fixture, version one.");

    let hub = fixture.hub();
    let imported = hub
        .import_skill(folder_import(&source))
        .expect("import health skill");
    let capability_id = imported.skill.id.clone();
    let v1_id = imported.version.id.clone();

    // (a) Undeployed skills are `not_deployed`, never `unverified`.
    let overview = hub.health_overview().expect("health overview");
    assert_eq!(overview.summary.total, 1);
    assert_eq!(overview.summary.not_deployed, 1);
    assert_eq!(overview.summary.verified, 0);
    assert_eq!(overview.capabilities[0].status, HealthState::NotDeployed);
    assert_eq!(overview.capabilities[0].slug, "health");

    // (b) Installing to two fake agents is not verification.
    let applied = install_to(&hub, &capability_id, &["codex", "workbuddy"]);
    assert_eq!(applied.status, OperationStatus::Success, "{applied:?}");
    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(
        health.status,
        HealthState::Unverified,
        "installed agents are unverified until evidence exists: {:?}",
        health.reasons
    );
    assert_eq!(health.installation_count, 2);
    assert!(health.verified_agent_keys.is_empty());
    assert!(health
        .reasons
        .iter()
        .any(|reason| reason.contains("缺少端到端验证证据")));
    let overview = hub.health_overview().expect("overview");
    assert_eq!(overview.summary.unverified, 1);

    // The static audit must pass so the only remaining gate is evidence.
    let audit = static_audit(&hub, &capability_id);
    assert_eq!(audit.status, "pass", "audit: {audit:?}");
    assert_eq!(
        hub.skill_health(&capability_id).expect("health").status,
        HealthState::Unverified,
        "a passing audit alone must not verify a capability"
    );

    // (c) Partial evidence stays unverified; pass every installed agent to verify.
    let codex_id = agent_id(&hub, "codex");
    let workbuddy_id = agent_id(&hub, "workbuddy");
    let run = record_validation(
        &hub,
        &capability_id,
        None,
        Some(&codex_id),
        "passed",
        "generate report",
    );
    assert_eq!(run.status, "passed");
    assert_eq!(run.version_id, v1_id, "defaults to the current version");
    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(health.status, HealthState::Unverified);
    assert_eq!(health.verified_agent_keys, vec!["codex".to_string()]);
    assert!(health
        .reasons
        .iter()
        .any(|reason| reason.contains("尚未") && reason.contains("workbuddy")));

    record_validation(
        &hub,
        &capability_id,
        None,
        Some(&workbuddy_id),
        "passed",
        "generate report",
    );
    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(health.status, HealthState::Verified, "health: {health:?}");
    let mut verified = health.verified_agent_keys.clone();
    verified.sort();
    assert_eq!(verified, vec!["codex".to_string(), "workbuddy".to_string()]);
    assert!(health
        .reasons
        .iter()
        .any(|reason| reason.contains("均通过当前版本手动验收")));
    let overview = hub.health_overview().expect("overview");
    assert_eq!(overview.summary.verified, 1);
    assert_eq!(overview.summary.not_deployed, 0);

    // Evidence is queryable with agent key, case and evidence payload.
    let runs = hub
        .list_validations(&capability_id, 50)
        .expect("list validations");
    assert_eq!(runs.len(), 2, "runs: {runs:?}");
    assert!(runs.iter().all(|run| run.version_id == v1_id));
    assert!(runs.iter().all(|run| run.agent_key.is_some()));
    assert!(runs.iter().any(|run| run.case_name == "generate report"
        && run.status == "passed"
        && run.evidence.contains("evidence")));

    // (d) Publishing v2 invalidates the current-version evidence.
    write_valid_skill_md(&source, "health", "Capability health fixture, version two.");
    let v2 = hub
        .import_skill(folder_import(&source))
        .expect("import v2");
    assert!(v2.created);
    let v2_id = v2.version.id.clone();
    assert_ne!(v1_id, v2_id);
    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(
        health.status,
        HealthState::Unverified,
        "evidence for v1 must not verify v2: {:?}",
        health.reasons
    );

    // Recording old-version passes changes nothing for the current version.
    record_validation(
        &hub,
        &capability_id,
        Some(&v1_id),
        Some(&codex_id),
        "passed",
        "stale case",
    );
    record_validation(
        &hub,
        &capability_id,
        Some(&v1_id),
        Some(&workbuddy_id),
        "passed",
        "stale case",
    );
    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(
        health.status,
        HealthState::Unverified,
        "stale-version evidence must be ignored: {:?}",
        health.reasons
    );

    // (e) Any failed run on the current version makes the capability fail.
    record_validation(
        &hub,
        &capability_id,
        None,
        Some(&codex_id),
        "failed",
        "v2 regression",
    );
    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(health.status, HealthState::Failed, "health: {health:?}");
    assert!(health
        .reasons
        .iter()
        .any(|reason| reason.contains("v2 regression")));
    let overview = hub.health_overview().expect("overview");
    assert_eq!(overview.summary.failed, 1);
    assert_eq!(overview.summary.verified, 0);
    assert_eq!(overview.summary.total, 1);

    // (f) Everything above is persisted across a real reopen.
    drop(hub);
    let hub = fixture.hub();
    let runs = hub
        .list_validations(&capability_id, 50)
        .expect("persisted validations");
    assert!(runs.len() >= 5, "runs: {runs:?}");
    assert!(runs.iter().any(|run| run.status == "failed"));
    let health = hub.skill_health(&capability_id).expect("persisted health");
    assert_eq!(health.status, HealthState::Failed);
    let overview = hub.health_overview().expect("persisted overview");
    assert_eq!(overview.summary.failed, 1);
    assert_eq!(overview.summary.total, 1);
}

// ---------------------------------------------------------------------------
// 5b. Safety first: a failed audit beats manual evidence
// ---------------------------------------------------------------------------

#[test]
fn manual_pass_cannot_override_a_failed_static_audit() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/risky");
    write_valid_skill_md(&source, "risky", "Skill whose script needs sudo.");
    write(&source.join("scripts/setup.sh"), "#!/bin/bash\nsudo mkdir -p /opt/risky\n");
    make_executable(&source.join("scripts/setup.sh"));

    let hub = fixture.hub();
    let imported = hub
        .import_skill(folder_import(&source))
        .expect("import risky skill");
    let capability_id = imported.skill.id.clone();

    let audit = static_audit(&hub, &capability_id);
    assert_eq!(audit.status, "fail", "audit: {audit:?}");
    assert!(audit.findings.iter().any(|f| f.id == "script.sudo"));

    let applied = install_to(&hub, &capability_id, &["codex"]);
    assert_eq!(applied.status, OperationStatus::Success);
    let codex_id = agent_id(&hub, "codex");
    record_validation(
        &hub,
        &capability_id,
        None,
        Some(&codex_id),
        "passed",
        "manual smoke test",
    );

    let health = hub.skill_health(&capability_id).expect("health");
    assert_eq!(
        health.status,
        HealthState::NeedsAttention,
        "security findings outrank manual evidence: {:?}",
        health.reasons
    );
    assert!(health
        .reasons
        .iter()
        .any(|reason| reason.contains("静态审计失败")));
    // The evidence itself is still recorded for the audit trail.
    assert_eq!(
        health
            .validations
            .iter()
            .filter(|run| run.status == "passed")
            .count(),
        1
    );
    let overview = hub.health_overview().expect("overview");
    assert_eq!(overview.summary.needs_attention, 1);
    assert_eq!(overview.summary.verified, 0);
}

// ---------------------------------------------------------------------------
// 6. Agent health / version change detection
// ---------------------------------------------------------------------------

#[test]
fn agent_health_detects_app_version_and_reports_real_changes() {
    let fixture = Fixture::new();
    // A bundle name the built-in adapters can only discover through the
    // explicitly configured `app_path`, so the real `/Applications` can never
    // leak a version into this assertion.
    let fake_app = fixture.project.join("FakeTrae.app");
    write_fake_app_version(&fake_app, "1.2.3");

    let hub = fixture.hub();
    let trae_id = agent_id(&hub, "trae");
    let updated = hub
        .update_agent_paths(AgentPathUpdate {
            agent_id: trae_id.clone(),
            app_path: Some(fake_app.to_string_lossy().to_string()),
            home_path: None,
            custom_skill_roots: Vec::new(),
        })
        .expect("set app path");
    assert_eq!(
        updated.app_path.as_deref(),
        Some(fake_app.to_string_lossy().as_ref()),
        "manually configured app path must persist"
    );

    // First observation is a baseline, not a version change.
    let first = hub
        .agent_health_report(true)
        .expect("record agent health");
    let trae = first
        .agents
        .iter()
        .find(|agent| agent.agent_key == "trae")
        .expect("trae in report");
    assert_eq!(trae.version.as_deref(), Some("1.2.3"), "agent: {trae:?}");
    assert!(!trae.version_changed, "baseline must not be a change");
    assert!(!trae
        .notes
        .iter()
        .any(|note| note.to_lowercase().contains("version changed")));

    // Recording the same version again must still report no change.
    let second = hub
        .agent_health_report(true)
        .expect("record agent health again");
    let trae = second
        .agents
        .iter()
        .find(|agent| agent.agent_key == "trae")
        .expect("trae in report");
    assert_eq!(trae.version.as_deref(), Some("1.2.3"));
    assert!(!trae.version_changed, "same version is not a change");
    assert_eq!(trae.previous_version.as_deref(), Some("1.2.3"));

    // A real bundle update is reported as a change with an explanatory note.
    // `agent_versions.detected_at` has second precision (`util::now`), so wait
    // for the next second before writing the changed observation; otherwise the
    // latest-row lookup can tie with the baseline (see the ignored reproducer
    // `latest_agent_version_tie_returns_stale_row_within_same_second`).
    std::thread::sleep(std::time::Duration::from_millis(1100));
    write_fake_app_version(&fake_app, "1.2.4");
    let third = hub
        .agent_health_report(true)
        .expect("record changed agent health");
    let trae = third
        .agents
        .iter()
        .find(|agent| agent.agent_key == "trae")
        .expect("trae in report");
    assert_eq!(trae.version.as_deref(), Some("1.2.4"), "agent: {trae:?}");
    assert_eq!(trae.previous_version.as_deref(), Some("1.2.3"));
    assert!(trae.version_changed, "1.2.3 -> 1.2.4 must be a change");
    assert!(
        trae
            .notes
            .iter()
            .any(|note| note.to_lowercase().contains("version changed")),
        "notes must explain the change: {:?}",
        trae.notes
    );
    assert!(trae.version_changed_at.is_some());

    // Reading without recording still exposes the latest persisted change.
    let read_only = hub
        .agent_health_report(false)
        .expect("read agent health");
    let trae = read_only
        .agents
        .iter()
        .find(|agent| agent.agent_key == "trae")
        .expect("trae in report");
    assert_eq!(trae.version.as_deref(), Some("1.2.4"));
    assert!(trae.version_changed);
}

#[test]
fn agent_health_report_handles_real_machine_apps_without_panicking() {
    let fixture = Fixture::new();
    let hub = fixture.hub();
    // `--record` performs the local probes; a read-only report only replays the
    // persisted observations, so recording is required to assert on versions.
    let report = hub
        .agent_health_report(true)
        .expect("agent health report on a real machine");
    assert!(report.agents.len() >= 9, "agents: {:?}", report.agents);
    // Detected real local apps may or may not expose a version; both are fine,
    // but the probe must never panic and never invent a version.
    for agent in &report.agents {
        if let Some(app_path) = hub
            .db()
            .get_agent(&agent.agent_id)
            .expect("agent row")
            .and_then(|row| row.app_path)
        {
            if Path::new(&app_path).exists() {
                assert!(
                    agent.version.as_deref().is_none_or(|value| !value.is_empty()),
                    "version, when present, must not be empty: {agent:?}"
                );
            }
        }
        assert!(!agent.checked_at.is_empty());
    }
    // Codex is installed on this machine; if its app bundle is present the
    // adapter must report a real version.
    if Path::new("/Applications/Codex.app").exists() {
        let codex = report
            .agents
            .iter()
            .find(|agent| agent.agent_key == "codex")
            .expect("codex in report");
        assert!(
            codex.version.is_some(),
            "real Codex.app should report a version: {codex:?}"
        );
    }
}

/// Regression test for the same-second ordering bug (fixed by ordering on
/// `(detected_at DESC, rowid DESC)`).
///
/// `Database::latest_agent_version` orders by `detected_at DESC`, but
/// `util::now()` only has second precision. Two observations recorded within
/// the same second tie, and SQLite resolves the tie to the older row, so a
/// read-only health report (`agent_health_report(false)` / `health --agents`)
/// can return the previous version and `versionChanged == false` right after a
/// real version change was recorded.
///
/// Reproduced end-to-end by the `agent_health_detects_app_version_and_reports_real_changes`
/// test before the explicit 1.1s wait was added (version read back 1.2.3 after
/// recording 1.2.4). This DB-level version is deterministic.
#[test]
fn latest_agent_version_tie_returns_stale_row_within_same_second() {
    let fixture = Fixture::new();
    let hub = fixture.hub();
    let trae_id = agent_id(&hub, "trae");
    let detected_at = "2026-01-01T00:00:00Z";
    hub.db()
        .conn()
        .execute_batch(&format!(
            "INSERT INTO agent_versions (id, agent_id, version, previous_version, changed, evidence, detected_at) \
             VALUES ('row-baseline', '{trae_id}', '1.2.3', NULL, 0, 'trae', '{detected_at}');\n\
             INSERT INTO agent_versions (id, agent_id, version, previous_version, changed, evidence, detected_at) \
             VALUES ('row-changed', '{trae_id}', '1.2.4', '1.2.3', 1, 'trae', '{detected_at}');"
        ))
        .expect("insert same-second version rows");

    let latest = hub
        .db()
        .latest_agent_version(&trae_id)
        .expect("latest agent version")
        .expect("a version row");
    assert_eq!(
        latest.version.as_deref(),
        Some("1.2.4"),
        "the most recently inserted observation must win the tie on detected_at"
    );
    assert!(latest.changed);
}

// ---------------------------------------------------------------------------
// 7. DB v2 migration
// ---------------------------------------------------------------------------

#[test]
fn v2_migration_rebuilds_tables_and_preserves_v1_data() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/migrate");
    write_valid_skill_md(&source, "migrate", "Skill kept across the migration test.");

    let hub = fixture.hub();
    assert_eq!(user_version(&hub), 3, "fresh databases migrate to v3");
    for table in [
        "capability_audits",
        "dependency_reports",
        "update_impacts",
        "validation_runs",
        "agent_versions",
    ] {
        assert_eq!(table_count(&hub, table), 1, "v2 table {table} missing");
    }

    let imported = hub
        .import_skill(folder_import(&source))
        .expect("import before migration");
    let capability_id = imported.skill.id.clone();
    let v1_id = imported.version.id.clone();
    let applied = install_to(&hub, &capability_id, &["codex"]);
    assert_eq!(applied.status, OperationStatus::Success);
    assert_eq!(static_audit(&hub, &capability_id).status, "pass");
    drop(hub);

    // Simulate a database that predates v2: drop the new tables and roll the
    // schema version back to 1.
    {
        let raw = agenthub_core::db::Database::open(&fixture.home.join(".agenthub/agenthub.db"))
            .expect("open raw database");
        raw.conn()
            .execute_batch(
                "PRAGMA foreign_keys = OFF;\n\
                 DROP TABLE capability_audits;\n\
                 DROP TABLE dependency_reports;\n\
                 DROP TABLE update_impacts;\n\
                 DROP TABLE validation_runs;\n\
                 DROP TABLE agent_versions;\n\
                 PRAGMA user_version = 1;\n\
                 PRAGMA foreign_keys = ON;",
            )
            .expect("downgrade schema to v1");
        let version: i64 = raw
            .conn()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read downgraded version");
        assert_eq!(version, 1);
        for table in [
            "capability_audits",
            "dependency_reports",
            "update_impacts",
            "validation_runs",
            "agent_versions",
        ] {
            assert_eq!(
                raw.conn()
                    .query_row(
                        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        [table],
                        |row| row.get::<_, i64>(0),
                    )
                    .expect("count table"),
                0,
                "table {table} should be gone before the re-migration"
            );
        }
    }

    // Reopening re-runs the v2 migration without touching v1 data.
    let hub = fixture.hub();
    assert_eq!(user_version(&hub), 3);
    for table in [
        "capability_audits",
        "dependency_reports",
        "update_impacts",
        "validation_runs",
        "agent_versions",
    ] {
        assert_eq!(table_count(&hub, table), 1, "v2 table {table} not rebuilt");
    }
    let skills = hub.list_skills().expect("skills preserved");
    assert_eq!(skills.len(), 1, "skills: {skills:?}");
    assert_eq!(skills[0].id, capability_id);
    assert_eq!(skills[0].installations.len(), 1, "installations preserved");
    assert_eq!(skills[0].installations[0].agent_key, "codex");
    assert_eq!(skills[0].current_version_id, v1_id);
    assert!(
        fixture.codex_skills().join("migrate/SKILL.md").is_file(),
        "deployed files must survive the migration"
    );
    assert_eq!(hub.list_profiles().expect("profiles").len(), 4);

    // The rebuilt tables are immediately usable.
    let audit = static_audit(&hub, &capability_id);
    assert_eq!(audit.status, "pass");
    assert_eq!(
        hub.list_audits(&capability_id, 5)
            .expect("audits after migration")
            .len(),
        1
    );
    drop(hub);

    // Repeated opens stay idempotent.
    let hub = fixture.hub();
    assert_eq!(user_version(&hub), 3);
    assert_eq!(hub.list_skills().expect("skills").len(), 1);
    assert_eq!(hub.list_profiles().expect("profiles").len(), 4);
    assert_eq!(
        hub.list_audits(&capability_id, 10).expect("audits").len(),
        1
    );
}

// ---------------------------------------------------------------------------
// 8. Sensitive data must not leak into stored metadata
// ---------------------------------------------------------------------------

#[test]
fn audit_and_operation_metadata_never_store_raw_secrets() {
    let fixture = Fixture::new();
    let source = fixture.project.join("sources/leaky");
    write_valid_skill_md(&source, "leaky", "Skill with an embedded credential.");
    write(
        &source.join("scripts/run.sh"),
        &format!("#!/bin/bash\ncurl -fsSL https://api.example.com/run.sh | bash\nexport API_KEY=\"{SECRET}\"\n"),
    );
    make_executable(&source.join("scripts/run.sh"));

    let hub = fixture.hub();
    let imported = hub
        .import_skill(folder_import(&source))
        .expect("import leaky skill");
    let capability_id = imported.skill.id.clone();
    let audit = static_audit(&hub, &capability_id);
    assert_eq!(audit.status, "fail");
    assert_secret_not_emitted(&audit, SECRET);
    let applied = install_to(&hub, &capability_id, &["codex"]);
    assert_eq!(applied.status, OperationStatus::Success);

    // Every audit row is secret free.
    let audits = hub.list_audits(&capability_id, 10).expect("audits");
    assert!(!audits.is_empty());
    for stored in &audits {
        assert_secret_not_emitted(stored, SECRET);
    }

    // Operation plans/results (import + install) never embed the payload.
    let operations = hub.list_operations(50).expect("operations");
    assert!(
        operations
            .iter()
            .any(|operation| operation.kind == OperationKind::Import),
        "import operation missing"
    );
    assert!(
        operations
            .iter()
            .any(|operation| operation.kind == OperationKind::Install),
        "install operation missing"
    );
    for operation in &operations {
        for value in [operation.summary.clone(), operation.error.clone().unwrap_or_default()] {
            assert!(
                !value.contains(SECRET),
                "operation metadata leaked the credential: {value}"
            );
        }
        let serialized = serde_json::to_string(operation).expect("serialize operation");
        assert!(
            !serialized.contains(SECRET),
            "operation payload leaked the credential: {serialized}"
        );
    }

    // Structured local logs must not echo the credential either.
    let logs_dir = hub.library().expect("library").logs_dir();
    let mut stack = vec![logs_dir];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(text) = fs::read_to_string(&path) {
                assert!(
                    !text.contains(SECRET),
                    "log file {} leaked the credential",
                    path.display()
                );
            }
        }
    }
}
