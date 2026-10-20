//! End-to-end CLI tests: every command is executed as a real process against an
//! isolated fake home, so exit codes, JSON shape and "no writes without --yes"
//! are verified at the binary boundary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

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

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        exit_code(output),
        expected,
        "unexpected exit code\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct FakeHome {
    _temp: TempDir,
    home: PathBuf,
    skill_source: PathBuf,
}

impl FakeHome {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".codex")).expect("fake .codex");
        fs::create_dir_all(home.join(".workbuddy")).expect("fake .workbuddy");
        let skill_source = temp.path().join("sources/alpha");
        fs::create_dir_all(&skill_source).expect("skill source dir");
        fs::write(
            skill_source.join("SKILL.md"),
            "---\nname: alpha\ndescription: CLI integration test skill.\nversion: 1.0.0\n---\n\n# alpha\n\ncli body v1\n",
        )
        .expect("write SKILL.md");
        FakeHome {
            _temp: temp,
            home,
            skill_source,
        }
    }

    fn skill_md(&self) -> PathBuf {
        self.skill_source.join("SKILL.md")
    }

    fn codex_target(&self) -> PathBuf {
        self.home.join(".codex/skills/alpha")
    }

    fn rewrite_source(&self, body: &str) {
        fs::write(
            self.skill_md(),
            format!(
                "---\nname: alpha\ndescription: CLI integration test skill.\nversion: 1.0.0\n---\n\n# alpha\n\n{body}\n"
            ),
        )
        .expect("rewrite source SKILL.md");
    }

    fn import(&self) -> serde_json::Value {
        let output = cli(
            &self.home,
            &[
                "--json",
                "skills",
                "import",
                "--path",
                &self.skill_source.to_string_lossy(),
            ],
        );
        assert_exit(&output, 0);
        stdout_json(&output)
    }
}

#[test]
fn doctor_and_listing_commands_are_machine_readable() {
    let fixture = FakeHome::new();

    let doctor = cli(&fixture.home, &["--json", "doctor"]);
    assert_exit(&doctor, 0);
    let report = stdout_json(&doctor);
    assert_eq!(report["ok"], true);
    assert!(report["checks"].as_array().expect("checks").len() >= 4);
    assert_eq!(report["agents"].as_array().expect("agents").len() >= 9, true);
    assert!(
        report["databasePath"]
            .as_str()
            .expect("database path")
            .starts_with(&fixture.home.to_string_lossy().to_string()),
        "database must live inside the fake home: {report}"
    );

    let agents = cli(&fixture.home, &["--json", "agents", "list"]);
    assert_exit(&agents, 0);
    let agents = stdout_json(&agents);
    let codex = agents
        .as_array()
        .expect("agents array")
        .iter()
        .find(|agent| agent["key"] == "codex")
        .expect("codex agent present");
    assert_eq!(codex["status"], "installed");
    assert!(codex["skillRoots"]
        .as_array()
        .expect("roots")
        .iter()
        .any(|root| root["path"]
            .as_str()
            .unwrap_or_default()
            .ends_with(".codex/skills")));

    let skills = cli(&fixture.home, &["--json", "skills", "list"]);
    assert_exit(&skills, 0);
    assert_eq!(stdout_json(&skills), serde_json::json!([]));

    let profiles = cli(&fixture.home, &["--json", "profiles", "list"]);
    assert_exit(&profiles, 0);
    let profiles = stdout_json(&profiles);
    let profiles = profiles.as_array().expect("profiles array");
    assert_eq!(profiles.len(), 4);
    assert!(profiles.iter().all(|profile| profile["isBuiltin"] == true));
}

#[test]
fn import_and_scan_commands_work_end_to_end() {
    let fixture = FakeHome::new();
    let imported = fixture.import();
    assert_eq!(imported["created"], true);
    assert_eq!(imported["skill"]["slug"], "alpha");
    assert!(imported["version"]["storagePath"]
        .as_str()
        .expect("storage path")
        .starts_with(&fixture.home.to_string_lossy().to_string()));
    let hash = imported["version"]["contentHash"]
        .as_str()
        .expect("content hash")
        .to_string();

    let listed = stdout_json(&cli(&fixture.home, &["--json", "skills", "list"]));
    assert_eq!(listed.as_array().expect("skills").len(), 1);
    assert_eq!(listed[0]["contentHash"], hash);

    // Scanning discovers the skill in the fake Codex home? No: it is only in
    // the library. Scanning must stay read-only and report no invalid entries.
    let scan = cli(&fixture.home, &["--json", "skills", "scan"]);
    assert_exit(&scan, 0);
    let report = stdout_json(&scan);
    assert_eq!(report["invalid"], 0);

    // A second import of the same content is reported as not created.
    let again = fixture.import();
    assert_eq!(again["created"], false);
    assert_eq!(
        again["version"]["id"],
        imported["version"]["id"],
        "identical content must reuse the version"
    );
}

#[test]
fn install_requires_yes_and_never_writes_during_dry_run() {
    let fixture = FakeHome::new();
    let imported = fixture.import();
    let skill_id = imported["skill"]["id"].as_str().expect("skill id").to_string();

    // --dry-run prints a plan, exits 0 for a clean plan and writes nothing.
    let dry = cli(
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
    assert_exit(&dry, 0);
    let payload = stdout_json(&dry);
    assert_eq!(payload["applied"], false);
    let plan = &payload["plan"];
    assert_eq!(plan["summary"]["create"], 2);
    assert_eq!(plan["items"].as_array().expect("items").len(), 2);
    for item in plan["items"].as_array().expect("items") {
        assert_eq!(item["action"], "create");
        assert!(item["targetPath"]
            .as_str()
            .unwrap_or_default()
            .contains("/skills/alpha"));
        assert!(!item["desiredHash"].as_str().unwrap_or_default().is_empty());
    }
    assert!(!fixture.codex_target().exists(), "dry run must not write");

    // Without --yes nothing is applied and the exit code is 2.
    let no_yes = cli(
        &fixture.home,
        &["--json", "skills", "install", &skill_id, "--agents", "codex"],
    );
    assert_exit(&no_yes, 2);
    let payload = stdout_json(&no_yes);
    assert_eq!(payload["applied"], false);
    assert!(!fixture.codex_target().exists(), "--yes was missing");

    // With --yes the plan is applied.
    let applied = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex,workbuddy",
            "--yes",
        ],
    );
    assert_exit(&applied, 0);
    let payload = stdout_json(&applied);
    assert_eq!(payload["applied"], true);
    assert_eq!(payload["result"]["status"], "success");
    assert!(fixture.codex_target().join("SKILL.md").is_file());
    assert!(fixture
        .home
        .join(".workbuddy/skills/alpha/SKILL.md")
        .is_file());
    let deployed_hash = agenthub_core::util::hash_dir(&fixture.codex_target()).expect("hash");
    assert_eq!(payload["plan"]["items"][0]["desiredHash"], deployed_hash);

    // The installations are visible through `skills list`.
    let listed = stdout_json(&cli(&fixture.home, &["--json", "skills", "list"]));
    let installations = listed[0]["installations"].as_array().expect("installations");
    assert_eq!(installations.len(), 2);
    assert!(installations.iter().all(|installation| installation["state"] == "installed"));

    // Re-installing the same version is a no-op.
    let noop = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex",
            "--yes",
        ],
    );
    assert_exit(&noop, 0);
    let payload = stdout_json(&noop);
    assert_eq!(payload["plan"]["summary"]["noop"], 1);
    assert_eq!(payload["result"]["items"][0]["status"], "noop");

    // Operations were recorded.
    let operations = stdout_json(&cli(&fixture.home, &["--json", "operations", "list"]));
    let operations = operations.as_array().expect("operations");
    assert!(!operations.is_empty());
    assert!(operations.iter().any(|operation| operation["kind"] == "install"
        && (operation["status"] == "success" || operation["status"] == "partial")));
}

#[test]
fn conflict_plan_exits_2_and_is_not_applied() {
    let fixture = FakeHome::new();
    let imported = fixture.import();
    let skill_id = imported["skill"]["id"].as_str().expect("skill id").to_string();

    // Unmanaged directory with the same name but different content.
    let target = fixture.codex_target();
    fs::create_dir_all(&target).expect("unmanaged target");
    fs::write(
        target.join("SKILL.md"),
        "---\nname: alpha\ndescription: unmanaged\n---\n\nunmanaged body\n",
    )
    .expect("write unmanaged SKILL.md");
    let before = fs::read_to_string(target.join("SKILL.md")).expect("read unmanaged");

    let dry = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex",
            "--dry-run",
        ],
    );
    assert_exit(&dry, 2);
    let payload = stdout_json(&dry);
    assert_eq!(payload["plan"]["items"][0]["action"], "conflict");
    assert_eq!(payload["plan"]["summary"]["conflict"], 1);

    let yes = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex",
            "--yes",
        ],
    );
    assert_exit(&yes, 2);
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).expect("read"),
        before,
        "conflict without --force must not be applied"
    );
}

#[test]
fn rollback_last_restores_the_previous_state() {
    let fixture = FakeHome::new();
    let imported = fixture.import();
    let skill_id = imported["skill"]["id"].as_str().expect("skill id").to_string();
    let applied = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex",
            "--yes",
        ],
    );
    assert_exit(&applied, 0);
    assert!(fixture.codex_target().is_dir());

    let rollback = cli(&fixture.home, &["--json", "skills", "rollback", "last"]);
    assert_exit(&rollback, 0);
    let payload = stdout_json(&rollback);
    assert_eq!(payload["result"]["status"], "success");
    assert!(!fixture.codex_target().exists(), "rollback must remove the deployment");
}

#[test]
fn sync_dry_run_plans_updates_without_writing() {
    let fixture = FakeHome::new();
    let imported = fixture.import();
    let skill_id = imported["skill"]["id"].as_str().expect("skill id").to_string();
    let install = cli(
        &fixture.home,
        &[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            "codex",
            "--yes",
        ],
    );
    assert_exit(&install, 0);
    let old_hash = agenthub_core::util::hash_dir(&fixture.codex_target()).expect("old hash");

    // Publish a new version of the skill.
    fixture.rewrite_source("cli body v2");
    let updated = fixture.import();
    assert_eq!(updated["created"], true);

    let sync = cli(&fixture.home, &["--json", "skills", "sync", "--dry-run"]);
    assert_exit(&sync, 0);
    let payload = stdout_json(&sync);
    assert_eq!(payload["applied"], false);
    let plans = payload["plans"].as_array().expect("plans");
    assert_eq!(plans.len(), 1, "one update plan expected: {payload}");
    assert_eq!(plans[0]["items"][0]["action"], "replace");
    assert_eq!(
        agenthub_core::util::hash_dir(&fixture.codex_target()).expect("deployed hash"),
        old_hash,
        "sync --dry-run must not write"
    );

    // --yes applies the update.
    let sync = cli(&fixture.home, &["--json", "skills", "sync", "--yes"]);
    assert_exit(&sync, 0);
    let payload = stdout_json(&sync);
    assert_eq!(payload["applied"], true);
    assert_eq!(payload["results"][0]["status"], "success");
    let deployed = fs::read_to_string(fixture.codex_target().join("SKILL.md")).expect("read");
    assert!(deployed.contains("cli body v2"), "sync did not update the deployment");
}

#[test]
fn mcp_discover_reports_existing_entries() {
    let fixture = FakeHome::new();
    fs::write(
        fixture.home.join(".codex/config.toml"),
        "# keep me\nmodel = \"gpt-5\"\n\n[mcp_servers.existing]\ncommand = \"existing-server\"\n",
    )
    .expect("write codex config");
    fs::create_dir_all(fixture.home.join(".workbuddy/connectors/default")).expect("workbuddy dir");
    fs::write(
        fixture.home.join(".workbuddy/connectors/default/mcp.json"),
        "{\"mcpServers\":{\"existing-wb\":{\"url\":\"http://localhost:1234/mcp\",\"type\":\"http\"}}}\n",
    )
    .expect("write workbuddy mcp json");

    let discovered = cli(&fixture.home, &["--json", "mcp", "discover"]);
    assert_exit(&discovered, 0);
    let entries = stdout_json(&discovered);
    let entries = entries.as_array().expect("discovered array");
    assert!(entries.iter().any(|entry| entry["name"] == "existing"
        && entry["agentKey"] == "codex"
        && entry["command"] == "existing-server"));
    assert!(entries.iter().any(|entry| entry["name"] == "existing-wb"
        && entry["agentKey"] == "workbuddy"
        && entry["transport"] == "http"));

    // The config files are untouched by discovery.
    assert!(fs::read_to_string(fixture.home.join(".codex/config.toml"))
        .expect("read")
        .starts_with("# keep me"));

    let mcp_list = cli(&fixture.home, &["--json", "mcp", "list"]);
    assert_exit(&mcp_list, 0);
    assert_eq!(stdout_json(&mcp_list), serde_json::json!([]));
}

#[test]
fn backup_commands_create_and_list_backups() {
    let fixture = FakeHome::new();
    let created = cli(&fixture.home, &["--json", "backup", "create"]);
    assert_exit(&created, 0);
    let backup = stdout_json(&created);
    let backup_path = PathBuf::from(backup["path"].as_str().expect("backup path"));
    assert!(backup_path.is_dir(), "backup directory must exist");
    assert!(backup_path.join("manifest.json").is_file());
    assert!(backup_path
        .join("agenthub.db")
        .is_file());

    let listed = cli(&fixture.home, &["--json", "backups", "list"]);
    assert_exit(&listed, 0);
    let backups = stdout_json(&listed);
    let backups = backups.as_array().expect("backups array");
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0]["id"], backup["id"]);
}

#[test]
fn failures_exit_1_and_write_errors_to_stderr() {
    let fixture = FakeHome::new();

    let missing = cli(
        &fixture.home,
        &["skills", "install", "does-not-exist", "--agents", "codex", "--dry-run"],
    );
    assert_exit(&missing, 1);
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        stderr.contains("does not exist"),
        "stderr should explain the failure: {stderr}"
    );
    assert!(missing.stdout.is_empty(), "failure output belongs on stderr");

    let bad_agent = cli(
        &fixture.home,
        &["skills", "install", "whatever", "--agents", "nope", "--dry-run"],
    );
    assert_exit(&bad_agent, 1);
    let stderr = String::from_utf8_lossy(&bad_agent.stderr);
    assert!(
        stderr.contains("unknown agent key"),
        "stderr should list valid agents: {stderr}"
    );

    // In JSON mode the error is machine readable on stderr.
    let json_error = cli(&fixture.home, &["--json", "skills", "install", "nope", "--agents", "codex"]);
    assert_exit(&json_error, 1);
    let stderr: serde_json::Value =
        serde_json::from_slice(&json_error.stderr).expect("stderr must be JSON error");
    assert_eq!(stderr["ok"], false);
    assert!(stderr["error"].as_str().unwrap_or_default().contains("does not exist"));
}
