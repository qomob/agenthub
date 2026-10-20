//! Integration tests: real-filesystem skill discovery and import lifecycle.
//!
//! Covers the task requirements:
//!  1. scan finds global + project skills and attributes the right agent_key
//!  2. folder / single-file import, content dedup, versioned re-import, invalid
//!     SKILL.md rejection

mod common;

use std::fs;
use std::path::Path;

use agenthub_core::model::*;
use agenthub_core::Error;

use common::*;

// ---------------------------------------------------------------------------
// 1. Scanning
// ---------------------------------------------------------------------------

#[test]
fn scan_discovers_global_and_project_skills_with_agent_attribution() {
    let fixture = Fixture::new();
    write_skill(
        &fixture.home.join(".codex/skills"),
        "alpha",
        "Alpha skill used by Codex.",
        "alpha body",
    );
    write_skill(
        &fixture.home.join(".workbuddy/skills"),
        "beta",
        "Beta skill used by WorkBuddy.",
        "beta body",
    );
    write_skill(
        &fixture.project.join(".trae/skills"),
        "gamma",
        "Gamma project skill for TRAE.",
        "gamma body",
    );
    write_skill(
        &fixture.project.join(".dsh/skills"),
        "delta",
        "Delta project skill for DSH.",
        "delta body",
    );

    let hub = fixture.hub();
    hub.set_project_roots(vec![fixture.project.to_string_lossy().to_string()])
        .expect("set project roots");
    let report = hub.scan().expect("scan");

    // No malformed skills in the fixture.
    assert_eq!(report.invalid, 0, "unexpected invalid skills: {report:?}");

    // The global skills are discovered by exactly the adapter that owns them;
    // the project skills are discovered by the adapters that read those project
    // folders (TRAE and TRAE CN share `<project>/.trae/skills` by design).
    let expectations: [(&str, &str, &Path); 4] = [
        ("codex", "alpha", &fixture.home.join(".codex/skills")),
        (
            "workbuddy",
            "beta",
            &fixture.home.join(".workbuddy/skills"),
        ),
        (
            "trae",
            "gamma",
            &fixture.project.join(".trae/skills"),
        ),
        ("dsh", "delta", &fixture.project.join(".dsh/skills")),
    ];
    for (agent_key, skill_name, expected_root) in expectations {
        let agent = report
            .agents
            .iter()
            .find(|agent| agent.agent_key == agent_key)
            .unwrap_or_else(|| panic!("agent {agent_key} missing from scan report"));
        let skill = agent
            .skills
            .iter()
            .find(|skill| skill.name == skill_name)
            .unwrap_or_else(|| panic!("skill {skill_name} not discovered for {agent_key}"));
        assert_eq!(
            skill.agent_key.as_deref(),
            Some(agent_key),
            "skill {skill_name} attributed to the wrong agent"
        );
        assert!(
            Path::new(&skill.path).starts_with(expected_root),
            "{} was discovered outside {expected_root:?}",
            skill.path
        );
        assert!(skill.valid);
        assert!(!skill.content_hash.is_empty());
        assert_eq!(skill.root, expected_root.to_string_lossy());
    }

    // Exactly four distinct skill directories: scanning must not invent skills.
    let mut discovered_paths: Vec<String> = report
        .agents
        .iter()
        .flat_map(|agent| agent.skills.iter())
        .filter(|skill| skill.valid)
        .map(|skill| skill.path.clone())
        .collect();
    discovered_paths.sort();
    discovered_paths.dedup();
    assert_eq!(
        discovered_paths.len(),
        4,
        "expected 4 unique skill directories, got {discovered_paths:?}"
    );

    // Scan results are cached per agent in SQLite.
    let codex_agent_id = agent_id(&hub, "codex");
    let cached = hub
        .db()
        .list_scan_results(&codex_agent_id)
        .expect("cached scan results");
    assert_eq!(cached.len(), 1, "codex should have one cached skill");
    assert_eq!(cached[0].status, "ok");
}

// ---------------------------------------------------------------------------
// 2. Import
// ---------------------------------------------------------------------------

#[test]
fn folder_import_dedups_identical_content_and_versions_changes() {
    let fixture = Fixture::new();
    let skill_folder = write_skill(
        &fixture.project.join("sources"),
        "alpha",
        "Alpha skill for import tests.",
        "version one body",
    );

    let hub = fixture.hub();
    let first = hub
        .import_skill(folder_import(&skill_folder))
        .expect("first import");
    assert!(first.created, "first import must create a skill");
    let capability_id = first.skill.id.clone();
    let v1_id = first.version.id.clone();
    let v1_hash = first.version.content_hash.clone();
    assert_eq!(first.skill.content_hash, v1_hash);
    assert_eq!(first.skill.version, "1.0.0");
    assert!(Path::new(&first.version.storage_path).join("SKILL.md").is_file());

    // Importing the exact same content again is a no-op (created=false).
    let duplicate = hub
        .import_skill(folder_import(&skill_folder))
        .expect("duplicate import");
    assert!(!duplicate.created, "identical content must not create a version");
    assert_eq!(duplicate.version.id, v1_id);
    assert_eq!(duplicate.skill.current_version_id, v1_id);
    assert!(
        duplicate
            .warnings
            .iter()
            .any(|warning| warning.contains("already in the library")),
        "duplicate import should warn: {:?}",
        duplicate.warnings
    );
    assert_eq!(
        hub.get_skill(&capability_id).unwrap().versions.len(),
        1,
        "duplicate import must not add a version row"
    );

    // Changing content creates a new version and moves `current` without
    // overwriting the previous immutable version.
    rewrite_skill(
        &skill_folder,
        "alpha",
        "Alpha skill for import tests.",
        "version two body",
    );
    let second = hub
        .import_skill(folder_import(&skill_folder))
        .expect("second version import");
    assert!(second.created, "changed content must create a new version");
    assert_ne!(second.version.id, v1_id);
    assert_ne!(second.version.content_hash, v1_hash);
    assert_eq!(second.skill.current_version_id, second.version.id);
    assert!(
        second
            .warnings
            .iter()
            .any(|warning| warning.contains("new version")),
        "new version import should warn: {:?}",
        second.warnings
    );

    let detail = hub.get_skill(&capability_id).expect("skill detail");
    assert_eq!(detail.versions.len(), 2, "both versions must be kept");
    let versions_by_id: std::collections::HashMap<&str, &CapabilityVersion> =
        detail.versions.iter().map(|v| (v.id.as_str(), v)).collect();
    let old = versions_by_id
        .get(v1_id.as_str())
        .expect("previous version row must survive");
    assert!(!old.is_current);
    assert_eq!(old.content_hash, v1_hash);
    assert!(
        Path::new(&old.storage_path).join("SKILL.md").is_file(),
        "previous version payload must stay on disk"
    );
    assert_eq!(
        read(&Path::new(&old.storage_path).join("SKILL.md")),
        skill_md("alpha", "Alpha skill for import tests.", "version one body"),
        "previous version content must not be overwritten"
    );
    let current = versions_by_id
        .get(detail.skill.current_version_id.as_str())
        .expect("current version exists");
    assert!(current.is_current);
    assert_eq!(current.content_hash, second.version.content_hash);
}

#[test]
fn single_skill_md_import_works() {
    let fixture = Fixture::new();
    let file = fixture.project.join("incoming/SKILL.md");
    write_skill_md_file(
        &file,
        "solo",
        "A skill imported from one SKILL.md file.",
        "single file body",
    );

    let hub = fixture.hub();
    let result = hub
        .import_skill(ImportRequest {
            source: ImportSource::SkillMd {
                path: file.to_string_lossy().to_string(),
            },
            skill_path: None,
            version_label: Some("2.0.0".to_string()),
            notes: Some("imported in test".to_string()),
        })
        .expect("single file import");
    assert!(result.created);
    assert_eq!(result.skill.slug, "solo");
    assert_eq!(result.version.version_label, "2.0.0");
    assert_eq!(result.version.notes.as_deref(), Some("imported in test"));
    assert_eq!(
        hub.list_skills().expect("list skills").len(),
        1,
        "single file import must register exactly one skill"
    );
}

#[test]
fn invalid_skill_md_is_rejected_and_not_stored() {
    let fixture = Fixture::new();
    let hub = fixture.hub();

    // 1. No frontmatter at all.
    let plain = fixture.project.join("plain");
    fs::create_dir_all(&plain).unwrap();
    fs::write(plain.join("SKILL.md"), "# Just a heading\n\nNo frontmatter here.\n").unwrap();
    let err = hub
        .import_skill(folder_import(&plain))
        .expect_err("frontmatter-less SKILL.md must be rejected");
    assert!(
        matches!(err, Error::InvalidSkill(_)),
        "expected InvalidSkill, got {err:?}"
    );

    // 2. Frontmatter without `description`.
    let no_description = fixture.project.join("no-description");
    fs::create_dir_all(&no_description).unwrap();
    fs::write(
        no_description.join("SKILL.md"),
        "---\nname: no-description\n---\n\nBody without description.\n",
    )
    .unwrap();
    let err = hub
        .import_skill(folder_import(&no_description))
        .expect_err("missing description must be rejected");
    assert!(
        matches!(err, Error::InvalidSkill(_)),
        "expected InvalidSkill, got {err:?}"
    );

    // 3. Unclosed frontmatter.
    let unclosed = fixture.project.join("unclosed");
    fs::create_dir_all(&unclosed).unwrap();
    fs::write(
        unclosed.join("SKILL.md"),
        "---\nname: unclosed\ndescription: never closed\n",
    )
    .unwrap();
    let err = hub
        .import_skill(folder_import(&unclosed))
        .expect_err("unclosed frontmatter must be rejected");
    assert!(
        matches!(err, Error::InvalidSkill(_)),
        "expected InvalidSkill, got {err:?}"
    );

    // None of the rejected imports may leave a library entry behind.
    assert!(
        hub.list_skills().expect("list skills").is_empty(),
        "rejected imports must not create capabilities"
    );
}
