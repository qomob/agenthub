//! Integration test: git import from a local repository.
//!
//! The test is skipped (with a message) when the `git` binary is unavailable,
//! because the core engine shells out to git for repository imports.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use agenthub_core::model::*;
use agenthub_core::Error;

use common::*;

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=AgentHub Test"])
        .args(["-c", "user.email=test@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_import(url: &str, skill_path: Option<&str>) -> ImportRequest {
    ImportRequest {
        source: ImportSource::Git {
            url: url.to_string(),
            reference: None,
        },
        skill_path: skill_path.map(str::to_string),
        version_label: None,
        notes: None,
    }
}

#[test]
fn git_import_reports_candidates_and_imports_selected_skill() {
    if !git_available() {
        eprintln!("skipping git import test: git binary is not available");
        return;
    }

    let fixture = Fixture::new();
    let repo = fixture.project.join("skill-repo");
    fs::create_dir_all(&repo).expect("create repo dir");
    git(&repo, &["init", "--quiet"]);
    write_skill(
        &repo.join("skills"),
        "one",
        "First skill inside the git repository.",
        "repo body one",
    );
    write_skill(
        &repo.join("skills"),
        "two",
        "Second skill inside the git repository.",
        "repo body two",
    );
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "add two skills"]);

    let url = format!(
        "file://{}",
        fs::canonicalize(&repo).expect("canonicalize repo").display()
    );
    let hub = fixture.hub();

    // Without --skill-path the import must fail with a conflict listing both
    // candidate directories.
    let err = hub
        .import_skill(git_import(&url, None))
        .expect_err("ambiguous git import must be a conflict");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got {err:?}"
    );
    let message = err.to_string();
    assert!(
        message.contains("skills/one") && message.contains("skills/two"),
        "conflict message must list candidates, got: {message}"
    );

    // Explicit skill_path imports exactly one skill.
    let imported = hub
        .import_skill(git_import(&url, Some("skills/one")))
        .expect("import selected git skill");
    assert!(imported.created);
    assert_eq!(imported.skill.slug, "one");
    assert_eq!(
        imported.skill.source.as_ref().map(|source| source.source_type.as_str()),
        Some("git")
    );

    let second = hub
        .import_skill(git_import(&url, Some("skills/two")))
        .expect("import second git skill");
    assert_eq!(second.skill.slug, "two");
    assert_eq!(
        hub.list_skills().expect("list skills").len(),
        2,
        "both selected skills should be registered"
    );

    // A bogus skill_path is reported clearly instead of quietly importing the
    // wrong skill.
    let err = hub
        .import_skill(git_import(&url, Some("skills/does-not-exist")))
        .expect_err("unknown skill path must fail");
    assert!(matches!(err, Error::NotFound(_)), "got {err:?}");
}
