//! Integration tests: conflict detection, drift reporting and refusal to
//! overwrite content AgentHub does not own.

mod common;

use std::fs;

use agenthub_core::model::*;
use agenthub_core::Error;

use common::*;

#[test]
fn unmanaged_same_name_directory_is_conflict_and_apply_refuses() {
    let fixture = Fixture::new();
    let folder = write_skill(
        &fixture.project.join("sources"),
        "alpha",
        "Alpha skill for conflict tests.",
        "managed body",
    );
    let hub = fixture.hub();
    let imported = hub.import_skill(folder_import(&folder)).expect("import");
    let codex = agent_id(&hub, "codex");

    // A directory with the same skill name but different content that AgentHub
    // does not know about.
    let target = fixture.codex_skills().join("alpha");
    write(
        &target.join("SKILL.md"),
        &skill_md("alpha", "Unmanaged skill", "unmanaged body"),
    );
    let unmanaged_hash = dir_hash(&target);

    let plan = hub
        .plan_install(install_request(
            &imported.skill.id,
            vec![codex],
            InstallStrategy::Copy,
        ))
        .expect("plan install over unmanaged directory");
    assert_eq!(plan.items[0].action, PlanAction::Conflict);
    assert_eq!(plan.summary.conflict, 1);
    assert_eq!(plan.summary.create, 0);
    assert!(plan.requires_confirmation);
    assert!(plan.items[0].diff.is_some());

    // Applying without force must refuse and leave the directory untouched.
    let err = hub
        .apply_operation(&plan.operation_id, false)
        .expect_err("conflict without force must be rejected");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got {err:?}"
    );
    assert_eq!(
        dir_hash(&target),
        unmanaged_hash,
        "the unmanaged directory must not be modified"
    );
    assert!(
        hub.get_skill(&imported.skill.id)
            .expect("skill")
            .skill
            .installations
            .is_empty(),
        "a refused install must not create an installation record"
    );
}

#[test]
fn external_drift_is_reported_and_blocks_non_forced_reapply() {
    let fixture = Fixture::new();
    let folder = write_skill(
        &fixture.project.join("sources"),
        "alpha",
        "Alpha skill for drift tests.",
        "original body",
    );
    let hub = fixture.hub();
    let imported = hub.import_skill(folder_import(&folder)).expect("import");
    let codex = agent_id(&hub, "codex");
    let plan = hub
        .plan_install(install_request(
            &imported.skill.id,
            vec![codex],
            InstallStrategy::Copy,
        ))
        .expect("plan install");
    let applied = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply install");
    assert_eq!(applied.status, OperationStatus::Success);

    let target = fixture.codex_skills().join("alpha");
    let deployed_md = target.join("SKILL.md");
    let tampered = skill_md("alpha", "Tampered skill", "tampered outside agenthub");
    fs::write(&deployed_md, &tampered).expect("tamper deployed SKILL.md");

    // check_changes reports the drift and marks the installation view.
    let changes = hub.check_changes().expect("check changes");
    assert_eq!(changes.drifted.len(), 1, "drift must be detected: {changes:?}");
    assert_eq!(changes.drifted[0].agent_key, "codex");
    assert!(changes
        .warnings
        .iter()
        .any(|warning| warning.contains("modified outside AgentHub")));

    let skill = skill_by_slug(&hub, "alpha");
    assert_eq!(skill.drift_count, 1);
    let installation = &skill.installations[0];
    assert!(installation.drift);
    assert_eq!(installation.state, InstallationState::Conflict);
    assert!(!installation.warnings.is_empty());

    // Re-applying the same version is a conflict and refuses to overwrite.
    let replan = hub
        .plan_install(install_request(
            &imported.skill.id,
            vec![agent_id(&hub, "codex")],
            InstallStrategy::Copy,
        ))
        .expect("replan over drifted deployment");
    assert_eq!(replan.items[0].action, PlanAction::Conflict);
    let err = hub
        .apply_operation(&replan.operation_id, false)
        .expect_err("drifted deployment must not be overwritten without force");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got {err:?}"
    );
    assert_eq!(
        read(&deployed_md),
        tampered,
        "refused apply must keep the drifted content"
    );

    // A deleted deployment is reported as missing instead of drift.
    fs::remove_dir_all(&target).expect("remove deployment");
    let changes = hub.check_changes().expect("check changes after delete");
    assert_eq!(changes.missing.len(), 1);
    assert!(changes.drifted.is_empty());
    assert!(changes
        .warnings
        .iter()
        .any(|warning| warning.contains("missing from disk")));
}
