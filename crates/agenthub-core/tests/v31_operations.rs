//! v3.1 Phase 1 safety-contract tests: plan hash, approval token, CAS,
//! durable journal, recovery and adapter evidence.

mod common;

use std::fs;
use std::path::Path;

use agenthub_core::model::*;
use agenthub_core::Error;

use common::*;

fn imported_alpha(fixture: &Fixture) -> (agenthub_core::AgentHub, String) {
    let folder = write_skill(
        &fixture.project.join("sources"),
        "alpha",
        "Alpha skill for v3.1 operation tests.",
        "alpha v1 body",
    );
    let hub = fixture.hub();
    let imported = hub.import_skill(folder_import(&folder)).expect("import alpha");
    (hub, imported.skill.id)
}

fn plan(hub: &agenthub_core::AgentHub, capability_id: &str, agents: &[&str]) -> OperationPlan {
    let fixture_targets = agent_ids(hub, agents);
    hub.plan_install(install_request(
        capability_id,
        fixture_targets,
        InstallStrategy::Copy,
    ))
    .expect("plan install")
}

fn target_for(fixture: &Fixture, agent: &str, slug: &str) -> std::path::PathBuf {
    match agent {
        "codex" => fixture.codex_skills().join(slug),
        "workbuddy" => fixture.workbuddy_skills().join(slug),
        other => panic!("unexpected agent {other}"),
    }
}

#[test]
fn plan_hash_is_stable_and_changes_with_targets() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let first = plan(&hub, &capability_id, &["codex", "workbuddy"]);
    let second = plan(&hub, &capability_id, &["codex", "workbuddy"]);
    assert_eq!(first.plan_hash.len(), 64, "plan hash must be a sha256");
    assert_eq!(first.plan_hash, second.plan_hash, "same plan => same hash");
    assert!(!first.expires_at.is_empty());

    let single = plan(&hub, &capability_id, &["codex"]);
    assert_ne!(
        first.plan_hash, single.plan_hash,
        "different targets must produce a different plan hash"
    );

    // The persisted operation must expose the same hash and no approval yet.
    let view = hub.get_operation(&first.operation_id).expect("view");
    assert_eq!(view.plan_hash.as_deref(), Some(first.plan_hash.as_str()));
    assert_eq!(view.approval_state, "none");
    assert_eq!(view.steps.len(), 0);
}

#[test]
fn approval_binds_plan_hash_and_rejects_bad_tokens() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex"]);

    // No approval yet: the raw apply entry point must refuse a bogus token.
    let err = hub
        .apply_operation_approved(&planned.operation_id, "not-a-token", None)
        .expect_err("apply without approval must fail");
    assert!(matches!(err, Error::Conflict(_) | Error::Blocked(_)), "unexpected: {err}");

    // A token bound to a different hash must be rejected.
    let wrong_hash = "0".repeat(64);
    let err = hub
        .approve_operation(&planned.operation_id, &wrong_hash)
        .expect_err("wrong plan hash must be rejected");
    assert!(matches!(err, Error::ExternalModification(_)), "unexpected: {err}");

    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");
    assert_eq!(grant.plan_hash, planned.plan_hash);
    assert_eq!(grant.approval_token.len(), 64);

    let result = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect("apply with approval");
    assert_eq!(result.status, OperationStatus::Success);

    let view = hub.get_operation(&planned.operation_id).expect("view");
    assert_eq!(view.approval_state, "approved");
    assert_eq!(view.steps.len(), 1);
    assert_eq!(view.steps[0].state, "applied");
    assert!(view.steps[0].after_hash.is_some());
    assert_eq!(
        fs::read_to_string(target_for(&fixture, "codex", "alpha").join("SKILL.md")).unwrap(),
        read(&target_for(&fixture, "codex", "alpha").join("SKILL.md"))
    );

    // The same token cannot be replayed after a successful apply.
    let err = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect_err("completed operation cannot be applied twice");
    assert!(matches!(err, Error::Conflict(_)), "unexpected: {err}");
}

#[test]
fn expired_approval_is_rejected() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex"]);
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");
    hub.db()
        .conn()
        .execute(
            "UPDATE operations SET approval_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
            [&planned.operation_id],
        )
        .expect("expire approval");
    let err = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect_err("expired approval must fail");
    assert!(matches!(err, Error::Conflict(_)), "unexpected: {err}");
    assert!(
        !target_for(&fixture, "codex", "alpha").exists(),
        "no write may happen with an expired approval"
    );
}

#[test]
fn external_modification_between_plan_and_apply_is_not_overwritten() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex"]);
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");

    // Another program creates the target path after the plan was approved.
    let target = target_for(&fixture, "codex", "alpha");
    fs::create_dir_all(&target).expect("create foreign target");
    fs::write(target.join("SKILL.md"), "foreign content").expect("write foreign skill");

    let err = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect_err("CAS mismatch must abort");
    assert!(matches!(err, Error::ExternalModification(_)), "unexpected: {err}");
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).expect("foreign file kept"),
        "foreign content",
        "external content must never be overwritten"
    );
}

#[test]
fn idempotency_key_returns_original_result_once() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex"]);
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");
    let first = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, Some("idem-1"))
        .expect("apply");
    let replay = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, Some("idem-1"))
        .expect("idempotent replay");
    assert_eq!(first.status, replay.status);

    // A second operation may not reuse the key for different work.
    let other = plan(&hub, &capability_id, &["workbuddy"]);
    let other_grant = hub
        .approve_operation(&other.operation_id, &other.plan_hash)
        .expect("approve other");
    let err = hub
        .apply_operation_approved(&other.operation_id, &other_grant.approval_token, Some("idem-1"))
        .expect_err("key reuse must be rejected");
    assert!(matches!(err, Error::Conflict(_)), "unexpected: {err}");
}

#[test]
fn recovery_restores_own_writes_but_never_external_changes() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex", "workbuddy"]);
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");
    hub.apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect("apply");

    // Simulate a crash after a partial write: mark the operation as needing
    // recovery and let another program edit one of the two targets.
    let codex_target = target_for(&fixture, "codex", "alpha");
    let workbuddy_target = target_for(&fixture, "workbuddy", "alpha");
    fs::write(codex_target.join("SKILL.md"), "edited by another app").expect("external edit");
    hub.db()
        .conn()
        .execute(
            "UPDATE operations SET status = 'needs_manual_recovery' WHERE id = ?1",
            [&planned.operation_id],
        )
        .expect("mark partial");

    let report = hub
        .recover_operation(&planned.operation_id)
        .expect("recovery runs");
    assert_eq!(report.status, "needs_manual_recovery");
    assert_eq!(
        fs::read_to_string(codex_target.join("SKILL.md")).expect("codex kept"),
        "edited by another app",
        "recovery must not overwrite external edits"
    );
    assert!(
        !workbuddy_target.exists(),
        "the operation's own write should be rolled back"
    );
}

#[test]
fn recovery_rolls_back_own_writes_when_no_external_change() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex", "workbuddy"]);
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");
    hub.apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect("apply");
    hub.db()
        .conn()
        .execute(
            "UPDATE operations SET status = 'partial' WHERE id = ?1",
            [&planned.operation_id],
        )
        .expect("mark partial");
    let report = hub
        .recover_operation(&planned.operation_id)
        .expect("recovery runs");
    assert_eq!(report.status, "recovered");
    assert!(!target_for(&fixture, "codex", "alpha").exists());
    assert!(!target_for(&fixture, "workbuddy", "alpha").exists());
}

#[test]
fn adapter_evidence_is_conservative() {
    let fixture = Fixture::new();
    let hub = fixture.hub();
    let agents = hub.list_agents().expect("agents");
    let codex = agents.iter().find(|agent| agent.key == "codex").expect("codex");
    let inspection = hub.agent_inspect(&codex.id).expect("inspect");
    assert!(inspection.capabilities.len() >= 11);
    for capability in &inspection.capabilities {
        assert_ne!(
            capability.support_state, "verified_runtime",
            "Phase 1 must not claim runtime verification for {}",
            capability.feature_key
        );
        let allowed = [
            "verified_runtime",
            "verified_config",
            "documented",
            "community",
            "assumed",
            "unsupported",
            "manual",
            "unverified",
        ];
        assert!(allowed.contains(&capability.support_state.as_str()));
        assert_eq!(capability.agent_key, "codex");
    }
    let write_skills = inspection
        .capabilities
        .iter()
        .find(|capability| capability.feature_key == "write_skills")
        .expect("write_skills evidence");
    assert_eq!(write_skills.support_state, "unverified");
    let read_skills = inspection
        .capabilities
        .iter()
        .find(|capability| capability.feature_key == "read_skills")
        .expect("read_skills evidence");
    assert!(matches!(
        read_skills.support_state.as_str(),
        "verified_config" | "unverified"
    ));
}

#[test]
fn v3_migration_creates_a_pre_migration_backup() {
    let temp = tempfile::tempdir().expect("temp dir");
    let home = temp.path().join("migrate-home");
    let config = agenthub_core::CoreConfig::for_home(&home);
    {
        let hub = agenthub_core::AgentHub::open(config.clone()).expect("first open");
        hub.db()
            .conn()
            .execute_batch(
                "DROP TABLE IF EXISTS operation_steps;
                 DROP TABLE IF EXISTS adapter_evidence;
                 DROP TABLE IF EXISTS audit_events;
                 DROP TABLE IF EXISTS schema_migrations;
                 PRAGMA user_version = 2;",
            )
            .expect("simulate v2 database");
    }
    let _hub = agenthub_core::AgentHub::open(config.clone()).expect("reopen migrates");
    let backups = config.data_dir.join("backups");
    let mut found = false;
    for entry in fs::read_dir(&backups).expect("backups dir") {
        let entry = entry.expect("entry");
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("pre-migration-v2-")
        {
            found = true;
            assert!(entry.path().join("agenthub.db").is_file());
            assert!(entry.path().join("manifest.json").is_file());
        }
    }
    assert!(found, "pre-migration backup was not created");
    let integrity = _hub.integrity_report().expect("integrity");
    assert!(integrity.ok, "integrity: {:?}", integrity.details);
    assert_eq!(integrity.schema_version, 3);
    let _: &Path = backups.as_path();
}

#[test]
fn approved_uninstall_removes_deployment_and_keeps_library_version() {
    let fixture = Fixture::new();
    let (hub, capability_id) = imported_alpha(&fixture);
    let planned = plan(&hub, &capability_id, &["codex"]);
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve install");
    hub.apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect("apply install");
    assert!(target_for(&fixture, "codex", "alpha").join("SKILL.md").is_file());

    let skill = hub.list_skills().expect("skills").into_iter().find(|skill| skill.id == capability_id).expect("skill");
    let installation_ids: Vec<String> = skill
        .installations
        .iter()
        .map(|installation| installation.id.clone())
        .collect();
    assert_eq!(installation_ids.len(), 1, "installation was recorded");
    let version_id = skill.current_version_id.clone();

    let uninstall = hub
        .plan_uninstall(UninstallRequest {
            installation_ids,
            force: false,
        })
        .expect("plan uninstall");
    let uninstall_grant = hub
        .approve_operation(&uninstall.operation_id, &uninstall.plan_hash)
        .expect("approve uninstall");
    let result = hub
        .apply_operation_approved(&uninstall.operation_id, &uninstall_grant.approval_token, None)
        .expect("apply uninstall");
    assert_eq!(result.status, OperationStatus::Success);
    assert!(!target_for(&fixture, "codex", "alpha").exists());
    assert!(
        hub.get_skill(&capability_id)
            .expect("skill detail")
            .versions
            .iter()
            .any(|version| version.id == version_id),
        "central library version must survive uninstall"
    );
    let view = hub.get_operation(&uninstall.operation_id).expect("view");
    assert_eq!(view.steps.len(), 1);
    assert_eq!(view.steps[0].state, "applied");
}
