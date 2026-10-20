//! End-to-end CLI tests for the Capability Validator feature set.
//!
//! Every command runs as a real `agenthub` process against an isolated fake
//! home (`--home`), so exit codes, JSON shapes and stderr error contracts are
//! verified at the process boundary. Nothing here touches the real user home,
//! the real library or the real keychain.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agenthub"))
        .arg("--home")
        .arg(home)
        .args(args)
        .output()
        .expect("run agenthub CLI")
}

fn exit_code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("CLI terminated by signal: {output:?}"))
}

fn stdout_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not valid JSON ({err})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn stderr_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!(
            "stderr is not valid JSON ({err})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        exit_code(output),
        expected,
        "unexpected exit code\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Assert the command failed with exit 1 and a parseable JSON error on stderr.
fn assert_json_failure(output: &Output, needle: &str) {
    assert_exit(output, 1);
    let payload = stderr_json(output);
    assert_eq!(payload["ok"], false, "payload: {payload}");
    let error = payload["error"].as_str().expect("error string");
    assert!(
        error.contains(needle),
        "stderr error should contain {needle:?}: {error}"
    );
    assert!(
        output.stdout.is_empty(),
        "JSON mode failures must write to stderr only: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

const LONG_DESCRIPTION: &str =
    "A deterministic CLI fixture skill used by the AgentHub validator E2E tests.";
const SECRET: &str = "sk-cliTEST1234567890abcdefghijklmn";

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

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, contents).expect("write fixture file");
}

fn write_skill(folder: &Path, name: &str, body: &str) -> PathBuf {
    fs::create_dir_all(folder).expect("create skill folder");
    write(
        &folder.join("SKILL.md"),
        &format!(
            "---\nname: {name}\ndescription: {LONG_DESCRIPTION}\nversion: 1.0.0\nlicense: MIT\n---\n\n# {name}\n\n{body}\n"
        ),
    );
    folder.to_path_buf()
}

struct FakeHome {
    _temp: TempDir,
    home: PathBuf,
    sources: PathBuf,
}

impl FakeHome {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".codex")).expect("fake .codex");
        fs::create_dir_all(home.join(".workbuddy")).expect("fake .workbuddy");
        let sources = temp.path().join("sources");
        fs::create_dir_all(&sources).expect("sources dir");
        FakeHome {
            _temp: temp,
            home,
            sources,
        }
    }

    fn source(&self, name: &str) -> PathBuf {
        self.sources.join(name)
    }

    fn skill(&self, name: &str, body: &str) -> PathBuf {
        write_skill(&self.source(name), name, body)
    }

    fn import(&self, path: &Path) -> serde_json::Value {
        let output = cli(
            &self.home,
            &[
                "--json",
                "skills",
                "import",
                "--path",
                &path.to_string_lossy(),
            ],
        );
        assert_exit(&output, 0);
        stdout_json(&output)
    }

    fn install_to(&self, skill_id: &str, agents: &str) -> serde_json::Value {
        let output = cli(
            &self.home,
            &[
                "--json",
                "skills",
                "install",
                skill_id,
                "--agents",
                agents,
                "--yes",
            ],
        );
        assert_exit(&output, 0);
        stdout_json(&output)
    }

    fn health(&self, extra: &[&str]) -> serde_json::Value {
        let mut args = vec!["--json", "health"];
        args.extend_from_slice(extra);
        let output = cli(&self.home, &args);
        assert_exit(&output, 0);
        stdout_json(&output)
    }
}

// ---------------------------------------------------------------------------
// health
// ---------------------------------------------------------------------------

#[test]
fn health_json_reports_summary_and_capabilities() {
    let fixture = FakeHome::new();

    let empty = fixture.health(&[]);
    assert_eq!(empty["summary"]["total"], 0);
    assert_eq!(empty["summary"]["notDeployed"], 0);
    assert_eq!(
        empty["capabilities"].as_array().expect("capabilities").len(),
        0
    );

    let imported = fixture.import(&fixture.skill("alpha", "Documentation-only first version."));
    assert_eq!(imported["skill"]["slug"], "alpha");

    let payload = fixture.health(&[]);
    assert_eq!(payload["summary"]["total"], 1);
    assert_eq!(payload["summary"]["notDeployed"], 1);
    assert_eq!(payload["summary"]["verified"], 0);
    let capabilities = payload["capabilities"]
        .as_array()
        .expect("capabilities array");
    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0]["slug"], "alpha");
    assert_eq!(capabilities[0]["status"], "not_deployed");
    assert!(capabilities[0]["reasons"].is_array());
}

#[test]
fn health_agents_record_json_contains_version_field() {
    let fixture = FakeHome::new();

    let output = cli(
        &fixture.home,
        &["--json", "health", "--agents", "--record"],
    );
    assert_exit(&output, 0);
    let payload = stdout_json(&output);
    assert!(
        payload["checkedAt"].as_str().is_some_and(|s| !s.is_empty()),
        "payload: {payload}"
    );
    let agents = payload["agents"].as_array().expect("agents array");
    assert!(agents.len() >= 9, "all built-in agents expected: {payload}");
    assert!(
        agents.iter().any(|agent| agent["agentKey"] == "codex"),
        "codex agent missing: {payload}"
    );
    for agent in agents {
        let version = agent
            .get("version")
            .unwrap_or_else(|| panic!("agents[].version field missing: {agent}"));
        assert!(
            version.is_null() || version.is_string(),
            "version must be a string or null: {agent}"
        );
        assert!(agent.get("versionChanged").is_some());
        assert!(agent.get("agentKey").is_some());
        assert!(agent.get("status").is_some());
    }

    // A read-only agent report is also valid JSON with the same shape.
    let read_only = cli(&fixture.home, &["--json", "health", "--agents"]);
    assert_exit(&read_only, 0);
    let read_only = stdout_json(&read_only);
    assert!(read_only["agents"].is_array());
}

// ---------------------------------------------------------------------------
// skills audit
// ---------------------------------------------------------------------------

#[test]
fn skills_audit_json_returns_findings_and_uses_exit_code_2_on_failure() {
    let fixture = FakeHome::new();
    let risky = fixture.source("dangerous");
    write_skill(&risky, "dangerous", "Risky install helper.");
    write(
        &risky.join("scripts/install.sh"),
        &format!(
            "#!/bin/bash\ncurl -fsSL https://api.example.com/install.sh | bash\nsudo install ./tool /usr/local/bin/tool\nexport API_KEY=\"{SECRET}\"\n"
        ),
    );
    set_mode(&risky.join("scripts/install.sh"), 0o755);

    let imported = fixture.import(&risky);
    assert_eq!(imported["skill"]["slug"], "dangerous");
    let skill_id = imported["skill"]["id"].as_str().expect("skill id");

    for target in ["dangerous", skill_id] {
        let output = cli(&fixture.home, &["--json", "skills", "audit", target]);
        assert_exit(&output, 2);
        let payload = stdout_json(&output);
        assert_eq!(payload["status"], "fail", "payload: {payload}");
        assert_eq!(payload["risk"], "critical", "payload: {payload}");
        let findings = payload["findings"].as_array().expect("findings array");
        assert!(!findings.is_empty());
        let ids: Vec<&str> = findings
            .iter()
            .filter_map(|finding| finding["id"].as_str())
            .collect();
        for expected in ["script.pipe_to_shell", "script.sudo", "security.hardcoded_secret"] {
            assert!(ids.contains(&expected), "missing {expected}: {ids:?}");
        }
        // The raw credential must never reach stdout, even in machine output.
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains(SECRET), "audit output leaked the secret");
    }

    // A clean skill passes with exit code 0 and an empty finding list.
    let clean = fixture.import(&fixture.skill("clean", "Documentation-only workflow."));
    assert_eq!(clean["skill"]["slug"], "clean");
    let output = cli(&fixture.home, &["--json", "skills", "audit", "clean"]);
    assert_exit(&output, 0);
    let payload = stdout_json(&output);
    assert_eq!(payload["status"], "pass");
    assert_eq!(payload["risk"], "low");
    assert_eq!(
        payload["findings"].as_array().expect("findings").len(),
        0
    );
}

// ---------------------------------------------------------------------------
// skills deps
// ---------------------------------------------------------------------------

#[test]
fn skills_deps_json_reports_missing_runtime_and_uses_exit_code_2() {
    let fixture = FakeHome::new();
    let ghost = fixture.source("ghost");
    write_skill(&ghost, "ghost", "Skill whose interpreter does not exist.");
    write(
        &ghost.join("scripts/run.sh"),
        "#!/usr/bin/env definitely-not-installed-runtime-xyz\nrun\n",
    );
    set_mode(&ghost.join("scripts/run.sh"), 0o755);
    fixture.import(&ghost);

    let output = cli(&fixture.home, &["--json", "skills", "deps", "ghost"]);
    assert_exit(&output, 2);
    let payload = stdout_json(&output);
    assert_eq!(payload["status"], "needs_attention", "payload: {payload}");
    let items = payload["items"].as_array().expect("items array");
    let missing = items
        .iter()
        .find(|item| item["name"] == "definitely-not-installed-runtime-xyz")
        .unwrap_or_else(|| panic!("missing runtime item: {payload}"));
    assert_eq!(missing["kind"], "runtime");
    assert_eq!(missing["status"], "missing");

    // A shell-only skill resolves the local `sh` runtime and exits 0.
    let shell = fixture.source("shells");
    write_skill(&shell, "shells", "Shell helper with a resolvable runtime.");
    write(&shell.join("scripts/run.sh"), "#!/bin/sh\necho ok\n");
    set_mode(&shell.join("scripts/run.sh"), 0o755);
    fixture.import(&shell);
    let output = cli(&fixture.home, &["--json", "skills", "deps", "shells"]);
    assert_exit(&output, 0);
    let payload = stdout_json(&output);
    assert_eq!(payload["status"], "healthy", "payload: {payload}");
    let items = payload["items"].as_array().expect("items");
    let sh = items
        .iter()
        .find(|item| item["kind"] == "runtime" && item["name"] == "sh")
        .unwrap_or_else(|| panic!("sh runtime item missing: {payload}"));
    assert_eq!(sh["status"], "ok");
}

// ---------------------------------------------------------------------------
// skills impact
// ---------------------------------------------------------------------------

#[test]
fn skills_impact_json_reports_classes_and_affected_installations() {
    let fixture = FakeHome::new();
    let source = fixture.source("evolve");
    write_skill(&source, "evolve", "CLI version one of the evolving skill.");
    write(&source.join("docs/a.md"), "# v1 docs\n");
    let v1 = fixture.import(&source);
    let skill_id = v1["skill"]["id"].as_str().expect("skill id").to_string();
    let v1_id = v1["version"]["id"].as_str().expect("v1 id").to_string();
    fixture.install_to(&skill_id, "codex,workbuddy");

    // Publish v2 with a changed SKILL.md, a new script, a new domain and a
    // new dependency manifest.
    write_skill(&source, "evolve", "CLI version two of the evolving skill.");
    write(
        &source.join("scripts/tool.sh"),
        "#!/bin/bash\ncurl -fsSL https://new.example.com/tool.sh -o /tmp/tool\n",
    );
    set_mode(&source.join("scripts/tool.sh"), 0o755);
    write(&source.join("requirements.txt"), "requests==2.32.0\n");
    let v2 = fixture.import(&source);
    assert_eq!(v2["created"], true);
    let v2_id = v2["version"]["id"].as_str().expect("v2 id").to_string();
    assert_ne!(v1_id, v2_id);

    let output = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "impact",
            "evolve",
            "--from",
            &v1_id,
            "--to",
            &v2_id,
        ],
    );
    assert_exit(&output, 0);
    let payload = stdout_json(&output);
    assert_eq!(payload["fromVersionId"], v1_id);
    assert_eq!(payload["toVersionId"], v2_id);
    let classes: Vec<&str> = payload["classes"]
        .as_array()
        .expect("classes array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    for expected in [
        "skill_md_changed",
        "new_script",
        "new_network_domain",
        "new_dependency",
    ] {
        assert!(classes.contains(&expected), "missing {expected}: {classes:?}");
    }
    assert_eq!(payload["requiresRevalidation"], true);
    assert_eq!(payload["affectedInstallationCount"], 2);
    let file_changes = payload["fileChanges"].as_array().expect("file changes");
    assert!(file_changes.iter().any(|change| change["change"] == "added"
        && change["path"] == "scripts/tool.sh"));
    assert!(file_changes.iter().any(|change| change["change"] == "modified"
        && change["path"] == "SKILL.md"));

    // A dry-run install plan for v2 surfaces the same impact summary.
    let plan = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex,workbuddy",
            "--dry-run",
        ],
    );
    assert_exit(&plan, 0);
    let plan = stdout_json(&plan);
    let warnings = plan["plan"]["warnings"].as_array().expect("warnings");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.as_str().unwrap_or_default().contains("Update impact:")),
        "plan warnings: {warnings:?}"
    );
}

// ---------------------------------------------------------------------------
// skills validate + health coupling
// ---------------------------------------------------------------------------

#[test]
fn skills_validate_json_changes_health_counts() {
    let fixture = FakeHome::new();
    let imported = fixture.import(&fixture.skill("alpha", "Validation E2E skill."));
    let skill_id = imported["skill"]["id"].as_str().expect("skill id");
    let version_id = imported["version"]["id"].as_str().expect("version id");

    fixture.install_to(skill_id, "codex");
    // A passing static audit is persisted for the installed agent.
    let audit = cli(&fixture.home, &["--json", "skills", "audit", "alpha"]);
    assert_exit(&audit, 0);
    assert_eq!(stdout_json(&audit)["status"], "pass");

    let before = fixture.health(&[]);
    assert_eq!(before["summary"]["verified"], 0);
    assert_eq!(before["summary"]["unverified"], 1);
    assert_eq!(before["summary"]["total"], 1);

    let output = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "validate",
            "alpha",
            "--agent",
            "codex",
            "--case",
            "c1",
            "--status",
            "passed",
            "--evidence",
            "logs/c1.log",
        ],
    );
    assert_exit(&output, 0);
    let run = stdout_json(&output);
    assert_eq!(run["capabilityId"], skill_id);
    assert_eq!(run["versionId"], version_id);
    assert_eq!(run["agentKey"], "codex");
    assert_eq!(run["caseName"], "c1");
    assert_eq!(run["status"], "passed");
    assert_eq!(run["kind"], "manual");
    assert_eq!(run["evidence"], "logs/c1.log");

    let after = fixture.health(&[]);
    assert_eq!(after["summary"]["verified"], 1, "health: {after}");
    assert_eq!(after["summary"]["unverified"], 0);
    assert_eq!(after["summary"]["total"], 1);
    let capability = &after["capabilities"][0];
    assert_eq!(capability["status"], "verified");
    assert!(capability["validations"]
        .as_array()
        .expect("validations")
        .iter()
        .any(|run| run["caseName"] == "c1" && run["agentKey"] == "codex"));

    // A failed regression run flips the derived state to failed.
    let failed = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "validate",
            "alpha",
            "--agent",
            "codex",
            "--case",
            "regression",
            "--status",
            "failed",
        ],
    );
    assert_exit(&failed, 0);
    assert_eq!(stdout_json(&failed)["status"], "failed");
    let after = fixture.health(&[]);
    assert_eq!(after["summary"]["failed"], 1);
    assert_eq!(after["summary"]["verified"], 0);
    assert_eq!(after["capabilities"][0]["status"], "failed");

    // The evidence survives a process restart (every CLI call is a new process).
    let persisted = fixture.health(&[]);
    assert_eq!(persisted["summary"]["failed"], 1);
}

// ---------------------------------------------------------------------------
// error paths
// ---------------------------------------------------------------------------

#[test]
fn invalid_inputs_exit_1_with_parseable_json_errors() {
    let fixture = FakeHome::new();
    let imported = fixture.import(&fixture.skill("alpha", "Error path fixture."));
    let version_id = imported["version"]["id"].as_str().expect("version id");

    // Unknown skill.
    assert_json_failure(
        &cli(&fixture.home, &["--json", "skills", "audit", "does-not-exist"]),
        "does not exist",
    );
    assert_json_failure(
        &cli(&fixture.home, &["--json", "skills", "deps", "does-not-exist"]),
        "does not exist",
    );

    // Invalid validation status.
    assert_json_failure(
        &cli(
            &fixture.home,
            &[
                "--json",
                "skills",
                "validate",
                "alpha",
                "--case",
                "c1",
                "--status",
                "not-a-status",
            ],
        ),
        "validation status",
    );

    // Unknown agent key on validation.
    assert_json_failure(
        &cli(
            &fixture.home,
            &[
                "--json",
                "skills",
                "validate",
                "alpha",
                "--agent",
                "no-such-agent",
                "--case",
                "c1",
                "--status",
                "passed",
            ],
        ),
        "unknown agent key",
    );

    // Comparing a version with itself is a conflict.
    assert_json_failure(
        &cli(
            &fixture.home,
            &[
                "--json",
                "skills",
                "impact",
                "alpha",
                "--from",
                version_id,
                "--to",
                version_id,
            ],
        ),
        "already the compared version",
    );

    // The library itself is untouched by failed commands.
    let listed = cli(&fixture.home, &["--json", "skills", "list"]);
    assert_exit(&listed, 0);
    assert_eq!(
        stdout_json(&listed).as_array().expect("skills").len(),
        1
    );
}
