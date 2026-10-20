//! Bulk adoption of previously scanned skills into the central library.
//!
//! Reproduces the user report "central library shows only the two imported
//! skills even though the machine has hundreds": scan must surface discovered
//! skills, and adoption must dedupe across agents and skip read-only caches.

mod common;

use agenthub_core::model::*;
use common::*;

fn write_alpha(home: &std::path::Path, rel: &str, body: &str) {
    let root = home.join(rel);
    write_skill(&root, "alpha", "Alpha skill", body);
}

#[test]
fn scan_adopt_dedupes_across_agents_and_skips_read_only_caches() {
    let fixture = Fixture::with_agents(&[".codex", ".workbuddy"]);
    // Same skill content in two agents -> one library entry.
    write_alpha(&fixture.home, ".codex/skills", "shared body");
    write_alpha(&fixture.home, ".workbuddy/skills", "shared body");
    // A codex-only skill.
    write_skill(
        &fixture.home.join(".codex/skills"),
        "beta",
        "Beta skill",
        "beta body",
    );
    // A marketplace cache skill must not be imported by default.
    write_skill(
        &fixture.home.join(".workbuddy/skills-marketplace/skills"),
        "gamma",
        "Gamma market skill",
        "gamma body",
    );

    let hub = fixture.hub();
    let report = hub.scan().expect("scan");
    let alpha = report
        .agents
        .iter()
        .flat_map(|agent| agent.skills.iter())
        .find(|skill| skill.slug == "alpha")
        .expect("alpha discovered");
    assert_eq!(alpha.root_source, "adapter", "root source propagated");
    let gamma = report
        .agents
        .iter()
        .flat_map(|agent| agent.skills.iter())
        .find(|skill| skill.slug == "gamma")
        .expect("gamma discovered");
    assert_eq!(gamma.root_source, "marketplace");

    // Preview must not write the library.
    let preview = hub
        .import_scan_skills(ScanImportRequest {
            dry_run: true,
            include_read_only_sources: false,
            max_items: Some(50),
            ..Default::default()
        })
        .expect("preview adopt");
    assert_eq!(preview.imported_count, 2, "alpha + beta would be adopted");
    assert_eq!(preview.skipped_duplicate, 1, "alpha exists in two agents");
    assert_eq!(preview.skipped_read_only, 1, "gamma is a marketplace cache");
    assert!(hub.list_skills().expect("skills").is_empty(), "preview wrote nothing");

    // Real adoption: alpha once + beta; gamma skipped.
    let adopted = hub
        .import_scan_skills(ScanImportRequest {
            dry_run: false,
            include_read_only_sources: false,
            max_items: Some(50),
            ..Default::default()
        })
        .expect("adopt");
    assert_eq!(adopted.imported_count, 2);
    assert_eq!(adopted.skipped_duplicate, 1);
    assert_eq!(adopted.skipped_read_only, 1);
    let skills = hub.list_skills().expect("skills");
    assert_eq!(skills.len(), 2, "{skills:?}");
    assert!(skills.iter().any(|skill| skill.slug == "alpha"));
    assert!(skills.iter().any(|skill| skill.slug == "beta"));
    assert!(!skills.iter().any(|skill| skill.slug == "gamma"));

    // Re-running is idempotent for existing content.
    let again = hub
        .import_scan_skills(ScanImportRequest {
            dry_run: false,
            include_read_only_sources: false,
            max_items: Some(50),
            ..Default::default()
        })
        .expect("adopt again");
    assert_eq!(again.imported_count, 0);
    assert!(again.skipped_existing >= 2, "{again:?}");

    // Opting into read-only roots still only imports the selected path here.
    let gamma_path = gamma.path.clone();
    let with_read_only = hub
        .import_scan_skills(ScanImportRequest {
            dry_run: false,
            paths: Some(vec![gamma_path]),
            include_read_only_sources: true,
            max_items: Some(10),
            ..Default::default()
        })
        .expect("adopt read-only");
    assert_eq!(with_read_only.imported_count, 1);
    assert!(hub
        .list_skills()
        .expect("skills")
        .iter()
        .any(|skill| skill.slug == "gamma"));
}

#[test]
fn scan_adopt_can_restrict_to_one_agent() {
    let fixture = Fixture::with_agents(&[".codex", ".workbuddy"]);
    write_skill(
        &fixture.home.join(".codex/skills"),
        "codex-only",
        "Codex only",
        "body",
    );
    write_skill(
        &fixture.home.join(".workbuddy/skills"),
        "workbuddy-only",
        "WorkBuddy only",
        "body",
    );
    let hub = fixture.hub();
    hub.scan().expect("scan");
    let codex_id = agent_id(&hub, "codex");
    let result = hub
        .import_scan_skills(ScanImportRequest {
            agent_ids: Some(vec![codex_id]),
            dry_run: false,
            include_read_only_sources: false,
            max_items: Some(10),
            ..Default::default()
        })
        .expect("adopt codex only");
    assert_eq!(result.imported_count, 1);
    let skills = hub.list_skills().expect("skills");
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].slug, "codex-only");
}
