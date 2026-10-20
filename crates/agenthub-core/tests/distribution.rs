//! Integration tests: transactional distribution (copy / symlink), no-op
//! idempotency and rollback against a real filesystem.

mod common;

use std::fs;
use std::path::Path;

use agenthub_core::model::*;
use agenthub_core::Error;

use common::*;

fn imported_alpha(fixture: &Fixture) -> (agenthub_core::AgentHub, String, String, String) {
    let folder = write_skill(
        &fixture.project.join("sources"),
        "alpha",
        "Alpha skill for distribution tests.",
        "alpha v1 body",
    );
    let hub = fixture.hub();
    let imported = hub.import_skill(folder_import(&folder)).expect("import alpha");
    (
        hub,
        imported.skill.id,
        imported.version.storage_path,
        imported.version.content_hash,
    )
}

// ---------------------------------------------------------------------------
// 3. Copy distribution
// ---------------------------------------------------------------------------

#[test]
fn copy_install_deploys_to_two_agents_and_records_installations() {
    let fixture = Fixture::new();
    let (hub, capability_id, storage_path, content_hash) = imported_alpha(&fixture);
    let targets = agent_ids(&hub, &["codex", "workbuddy"]);

    let plan = hub
        .plan_install(install_request(
            &capability_id,
            targets.clone(),
            InstallStrategy::Copy,
        ))
        .expect("plan install");

    // A plan is always a dry-run preview and must not write to agents.
    assert!(plan.dry_run, "plans are previews (dry_run=true)");
    assert_eq!(plan.kind, OperationKind::Install);
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.summary.create, 2);
    assert!(plan.items.iter().all(|item| item.action == PlanAction::Create));
    assert!(plan
        .items
        .iter()
        .all(|item| item.strategy == InstallStrategy::Copy));
    let codex_target = fixture.codex_skills().join("alpha");
    let workbuddy_target = fixture.workbuddy_skills().join("alpha");
    assert!(!codex_target.exists(), "plan must not create agent files");

    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply install");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");
    assert!(result.items.iter().all(|item| item.status == "success"));

    // Both deployments contain exactly the library version content.
    for target in [&codex_target, &workbuddy_target] {
        assert!(target.join("SKILL.md").is_file(), "{} missing", target.display());
        assert_eq!(
            dir_hash(target),
            content_hash,
            "deployed hash must equal the library version hash at {}",
            target.display()
        );
        assert_eq!(
            read(&target.join("SKILL.md")),
            read(&Path::new(&storage_path).join("SKILL.md")),
            "deployed SKILL.md must match the library copy"
        );
    }

    // Both installations are recorded as installed with the expected hashes.
    let skill = skill_by_slug(&hub, "alpha");
    assert_eq!(skill.installations.len(), 2);
    for installation in &skill.installations {
        assert_eq!(installation.state, InstallationState::Installed);
        assert_eq!(installation.version_id, skill.current_version_id);
        assert_eq!(installation.content_hash.as_deref(), Some(content_hash.as_str()));
        assert!(!installation.drift);
        assert!(
            installation.target_path == codex_target.to_string_lossy()
                || installation.target_path == workbuddy_target.to_string_lossy()
        );
    }

    // check_changes is clean right after a successful install.
    let changes = hub.check_changes().expect("check changes");
    assert!(changes.drifted.is_empty(), "unexpected drift: {changes:?}");
    assert!(changes.missing.is_empty());
}

// ---------------------------------------------------------------------------
// 4. Symlink distribution
// ---------------------------------------------------------------------------

#[test]
fn symlink_install_points_at_library_and_uninstall_preserves_library() {
    let fixture = Fixture::new();
    let (hub, capability_id, storage_path, content_hash) = imported_alpha(&fixture);
    let codex = agent_id(&hub, "codex");

    let plan = hub
        .plan_install(install_request(
            &capability_id,
            vec![codex],
            InstallStrategy::Symlink,
        ))
        .expect("plan symlink install");
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].strategy, InstallStrategy::Symlink);

    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply symlink install");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");

    let target = fixture.codex_skills().join("alpha");
    assert!(is_symlink(&target), "{} should be a symlink", target.display());
    assert_eq!(
        fs::canonicalize(&target).expect("canonicalize link"),
        fs::canonicalize(&storage_path).expect("canonicalize library version"),
        "symlink must point at the immutable library version"
    );
    assert_eq!(dir_hash(&target), content_hash);

    // Removing the deployment (via the uninstall plan) must never touch the
    // library source files.
    let skill = skill_by_slug(&hub, "alpha");
    let installation_id = skill.installations[0].id.clone();
    let uninstall = hub
        .plan_uninstall(UninstallRequest {
            installation_ids: vec![installation_id],
            force: false,
        })
        .expect("plan uninstall");
    assert_eq!(uninstall.items[0].action, PlanAction::Remove);
    let result = hub
        .apply_operation(&uninstall.operation_id, false)
        .expect("apply uninstall");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");

    assert!(
        fs::symlink_metadata(&target).is_err(),
        "{} should be gone after uninstall",
        target.display()
    );
    let library_skill_md = Path::new(&storage_path).join("SKILL.md");
    assert!(
        library_skill_md.is_file(),
        "uninstalling must not delete the library source"
    );
    assert_eq!(dir_hash(Path::new(&storage_path)), content_hash);
    assert!(
        hub.list_skills().expect("list skills")[0]
            .installations
            .is_empty(),
        "installation record must be removed"
    );
}

// ---------------------------------------------------------------------------
// 5. Rollback
// ---------------------------------------------------------------------------

#[test]
fn rollback_restores_previous_deployment_and_installation_record() {
    let fixture = Fixture::new();
    let folder = write_skill(
        &fixture.project.join("sources"),
        "alpha",
        "Alpha skill for rollback tests.",
        "alpha v1 body",
    );
    let hub = fixture.hub();
    let v1 = hub.import_skill(folder_import(&folder)).expect("import v1");
    let capability_id = v1.skill.id.clone();
    let v1_version_id = v1.version.id.clone();
    let v1_hash = v1.version.content_hash.clone();
    let codex = agent_id(&hub, "codex");

    // Install v1.
    let install_v1 = hub
        .plan_install(install_request(
            &capability_id,
            vec![codex.clone()],
            InstallStrategy::Copy,
        ))
        .expect("plan v1 install");
    let apply_v1 = hub
        .apply_operation(&install_v1.operation_id, false)
        .expect("apply v1");
    assert_eq!(apply_v1.status, OperationStatus::Success);

    let target = fixture.codex_skills().join("alpha");
    assert_eq!(dir_hash(&target), v1_hash);

    // Import v2 and distribute it over v1.
    rewrite_skill(
        &folder,
        "alpha",
        "Alpha skill for rollback tests.",
        "alpha v2 body",
    );
    let v2 = hub.import_skill(folder_import(&folder)).expect("import v2");
    assert!(v2.created);
    assert_ne!(v2.version.id, v1_version_id);
    let v2_hash = v2.version.content_hash.clone();

    let install_v2 = hub
        .plan_install(install_request(
            &capability_id,
            vec![codex],
            InstallStrategy::Copy,
        ))
        .expect("plan v2 install");
    assert_eq!(install_v2.items[0].action, PlanAction::Replace);
    let apply_v2 = hub
        .apply_operation(&install_v2.operation_id, false)
        .expect("apply v2");
    assert_eq!(apply_v2.status, OperationStatus::Success);
    assert_eq!(dir_hash(&target), v2_hash);
    assert!(read(&target.join("SKILL.md")).contains("alpha v2 body"));

    // Roll back the successful v2 distribution: the deployed directory goes
    // back to the v1 snapshot and the installation row is restored.
    let rollback = hub
        .rollback_operation(&install_v2.operation_id)
        .expect("rollback v2 install");
    assert_eq!(rollback.status, OperationStatus::Success, "result: {rollback:?}");
    assert!(rollback.items.iter().all(|item| item.status == "success"));

    assert_eq!(
        dir_hash(&target),
        v1_hash,
        "deployment must be restored to the pre-operation snapshot"
    );
    assert!(read(&target.join("SKILL.md")).contains("alpha v1 body"));
    let skill = skill_by_slug(&hub, "alpha");
    assert_eq!(skill.installations.len(), 1);
    assert_eq!(
        skill.installations[0].version_id, v1_version_id,
        "installation record must point at v1 again"
    );
    assert_eq!(
        skill.installations[0].content_hash.as_deref(),
        Some(v1_hash.as_str())
    );

    // Library payloads are untouched by rollback.
    assert_eq!(
        dir_hash(Path::new(&v1.version.storage_path)),
        v1_hash,
        "v1 library payload must stay intact"
    );
    assert_eq!(
        dir_hash(Path::new(&v2.version.storage_path)),
        v2_hash,
        "v2 library payload must stay intact"
    );

    // Rollback is recorded as its own operation.
    let rollback_view = hub
        .list_operations(20)
        .expect("list operations")
        .into_iter()
        .find(|operation| operation.kind == OperationKind::Rollback)
        .expect("rollback operation recorded");
    assert_eq!(
        rollback_view.rollback_of.as_deref(),
        Some(install_v2.operation_id.as_str())
    );
}

#[test]
fn rollback_of_a_create_install_removes_the_deployed_directory() {
    let fixture = Fixture::new();
    let (hub, capability_id, _storage, _hash) = imported_alpha(&fixture);
    let codex = agent_id(&hub, "codex");
    let plan = hub
        .plan_install(install_request(
            &capability_id,
            vec![codex],
            InstallStrategy::Copy,
        ))
        .expect("plan install");
    hub.apply_operation(&plan.operation_id, false)
        .expect("apply install");
    let target = fixture.codex_skills().join("alpha");
    assert!(target.is_dir());

    hub.rollback_operation(&plan.operation_id)
        .expect("rollback create install");
    assert!(
        fs::symlink_metadata(&target).is_err(),
        "a rolled-back create must leave no deployment behind"
    );
}

// ---------------------------------------------------------------------------
// 7. Idempotency
// ---------------------------------------------------------------------------

#[test]
fn replanning_same_version_is_noop_and_applying_twice_is_rejected() {
    let fixture = Fixture::new();
    let (hub, capability_id, _storage, content_hash) = imported_alpha(&fixture);
    let codex = agent_id(&hub, "codex");
    let plan = hub
        .plan_install(install_request(
            &capability_id,
            vec![codex.clone()],
            InstallStrategy::Copy,
        ))
        .expect("plan install");
    let first = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply install");
    assert_eq!(first.status, OperationStatus::Success);

    // Same version again: plan is Noop, apply succeeds without side effects.
    let replan = hub
        .plan_install(install_request(
            &capability_id,
            vec![codex],
            InstallStrategy::Copy,
        ))
        .expect("replan install");
    assert_eq!(replan.summary.noop, 1, "summary: {:?}", replan.summary);
    assert_eq!(replan.items[0].action, PlanAction::Noop);
    let second = hub
        .apply_operation(&replan.operation_id, false)
        .expect("apply noop plan");
    assert_eq!(second.status, OperationStatus::Success);
    assert_eq!(second.items[0].status, "noop");
    assert_eq!(
        skill_by_slug(&hub, "alpha").installations.len(),
        1,
        "noop must not duplicate installation rows"
    );
    assert_eq!(dir_hash(&fixture.codex_skills().join("alpha")), content_hash);

    // Re-applying the already-applied operation is a conflict, not a silent
    // re-run.
    let err = hub
        .apply_operation(&plan.operation_id, false)
        .expect_err("applying an applied operation must fail");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got {err:?}"
    );
    assert!(
        err.to_string().contains("cannot be applied again"),
        "unexpected error message: {err}"
    );
}
