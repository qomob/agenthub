//! v3.1 Phase 1 CLI contract tests.
//!
//! Every command runs as a real process against an isolated temporary
//! `AGENTHUB_HOME`, so the unified `ApiResult` envelope, the approval-token
//! lifecycle and the normalized exit codes are verified at the binary boundary:
//!
//!   0 success · 2 partial · 3 invalid/unsupported · 4 approval/stale · 5 runtime/recover
//!
//! The recovery test uses the public core API (`AgentHub::db().conn()`) to
//! simulate a crash after a partial write, exactly like the core v3.1 tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};
use serde_json::Value;
use tempfile::TempDir;

const SKILL_MD: &str = "---\nname: alpha\ndescription: v3.1 CLI contract test skill.\nversion: 1.0.0\n---\n\n# alpha\n\nv3.1 cli body\n";

fn cli(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agenthub"))
        .env("AGENTHUB_HOME", home)
        .env_remove("AGENTHUB_DATA_DIR")
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

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout is not valid JSON ({err})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn stderr_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!(
            "stderr is not a JSON error envelope ({err})\nstdout: {}\nstderr: {}",
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

/// Assert the stderr failure envelope shape and return its `error` object.
fn assert_error_envelope(output: &Output, expected_code: &str) -> Value {
    assert!(
        output.stdout.is_empty(),
        "failure output must go to stderr only, stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let envelope = stderr_json(output);
    assert_eq!(envelope["ok"], false, "envelope: {envelope}");
    let error = envelope["error"].clone();
    assert_eq!(error["code"], expected_code, "envelope: {envelope}");
    assert!(
        error["message"].as_str().unwrap_or_default().len() > 3,
        "error message must explain the failure: {envelope}"
    );
    assert!(
        error["retryable"].is_boolean(),
        "error.retryable must be present: {envelope}"
    );
    assert!(
        !envelope["requestId"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "every envelope must carry a requestId: {envelope}"
    );
    error
}

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".codex")).expect("fake .codex");
        fs::create_dir_all(home.join(".workbuddy")).expect("fake .workbuddy");
        let source = temp.path().join("sources/alpha");
        fs::create_dir_all(&source).expect("skill source dir");
        fs::write(source.join("SKILL.md"), SKILL_MD).expect("write SKILL.md");
        Fixture {
            _temp: temp,
            home,
            source,
        }
    }

    fn cli(&self, args: &[&str]) -> Output {
        cli(&self.home, args)
    }

    fn import(&self) -> String {
        let output = self.cli(&[
            "--json",
            "skills",
            "import",
            "--path",
            &self.source.to_string_lossy(),
        ]);
        assert_exit(&output, 0);
        let imported = stdout_json(&output);
        imported["skill"]["id"]
            .as_str()
            .expect("imported skill id")
            .to_string()
    }

    /// Create a persisted install plan with `--dry-run` and return its id and
    /// plan hash (as shown by `operations preview`).
    fn planned_install(&self, agents: &str) -> (String, String) {
        let skill_id = self.import();
        let dry = self.cli(&[
            "--json",
            "skills",
            "install",
            &skill_id,
            "--agents",
            agents,
            "--dry-run",
        ]);
        assert_exit(&dry, 0);
        let payload = stdout_json(&dry);
        let operation_id = payload["plan"]["operationId"]
            .as_str()
            .expect("planned operation id")
            .to_string();

        let preview = self.cli(&["--json", "operations", "preview", &operation_id]);
        assert_exit(&preview, 0);
        let envelope = stdout_json(&preview);
        assert_eq!(envelope["ok"], true, "envelope: {envelope}");
        assert_eq!(
            envelope["data"]["id"], operation_id,
            "preview must describe the requested operation"
        );
        assert_eq!(
            envelope["data"]["approvalState"], "none",
            "a fresh plan has no approval"
        );
        let plan_hash = envelope["data"]["planHash"]
            .as_str()
            .expect("preview planHash")
            .to_string();
        assert_eq!(plan_hash.len(), 64, "plan hash must be a sha256: {plan_hash}");
        assert!(
            !envelope["requestId"].as_str().unwrap_or_default().is_empty(),
            "preview must carry a requestId: {envelope}"
        );
        (operation_id, plan_hash)
    }

    fn codex_target(&self) -> PathBuf {
        self.home.join(".codex/skills/alpha")
    }

    fn workbuddy_target(&self) -> PathBuf {
        self.home.join(".workbuddy/skills/alpha")
    }
}

#[test]
fn apply_without_approval_uses_fake_token_and_exits_4() {
    let fixture = Fixture::new();
    let (operation_id, _plan_hash) = fixture.planned_install("codex");

    let output = fixture.cli(&[
        "--json",
        "operations",
        "apply",
        &operation_id,
        "--token",
        "not-a-real-token",
    ]);
    assert_exit(&output, 4);
    let error = assert_error_envelope(&output, "PLAN_STALE");
    assert_eq!(error["retryable"], false, "a missing approval is not retryable");
    assert!(
        !fixture.codex_target().exists(),
        "an unapproved apply must not write"
    );
}

#[test]
fn approve_with_wrong_plan_hash_exits_4_and_keeps_the_operation_pending() {
    let fixture = Fixture::new();
    let (operation_id, plan_hash) = fixture.planned_install("codex");

    let wrong_hash = "0".repeat(64);
    let output = fixture.cli(&[
        "--json",
        "operations",
        "approve",
        &operation_id,
        "--plan-hash",
        &wrong_hash,
    ]);
    assert_exit(&output, 4);
    assert_error_envelope(&output, "PLAN_STALE");

    // The rejected approval must not consume the plan: the real hash works.
    let approved = fixture.cli(&[
        "--json",
        "operations",
        "approve",
        &operation_id,
        "--plan-hash",
        &plan_hash,
    ]);
    assert_exit(&approved, 0);
}

#[test]
fn approve_then_apply_succeeds_with_request_id_and_plan_hash() {
    let fixture = Fixture::new();
    let (operation_id, plan_hash) = fixture.planned_install("codex");

    let approve = fixture.cli(&[
        "--json",
        "operations",
        "approve",
        &operation_id,
        "--plan-hash",
        &plan_hash,
    ]);
    assert_exit(&approve, 0);
    let approve_envelope = stdout_json(&approve);
    assert_eq!(approve_envelope["ok"], true, "envelope: {approve_envelope}");
    assert!(
        !approve_envelope["requestId"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "approve envelope must carry a requestId: {approve_envelope}"
    );
    assert_eq!(
        approve_envelope["data"]["planHash"], plan_hash,
        "approve must echo the approved plan hash"
    );
    assert_eq!(approve_envelope["data"]["operationId"], operation_id);
    let token = approve_envelope["data"]["approvalToken"]
        .as_str()
        .expect("one-time approval token")
        .to_string();
    assert_eq!(token.len(), 64);

    let apply = fixture.cli(&[
        "--json",
        "operations",
        "apply",
        &operation_id,
        "--token",
        &token,
        "--idempotency-key",
        "cli-v31-apply-1",
    ]);
    assert_exit(&apply, 0);
    let apply_envelope = stdout_json(&apply);
    assert_eq!(apply_envelope["ok"], true, "envelope: {apply_envelope}");
    assert!(
        !apply_envelope["requestId"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "apply envelope must carry a requestId: {apply_envelope}"
    );
    assert_eq!(
        apply_envelope["data"]["planHash"], plan_hash,
        "apply must report the approved plan hash"
    );
    assert_eq!(apply_envelope["data"]["status"], "success");
    assert_eq!(apply_envelope["data"]["idempotencyKey"], "cli-v31-apply-1");
    assert!(fixture.codex_target().join("SKILL.md").is_file());

    // Replaying the same idempotency key returns the stored result (exit 0).
    let replay = fixture.cli(&[
        "--json",
        "operations",
        "apply",
        &operation_id,
        "--token",
        &token,
        "--idempotency-key",
        "cli-v31-apply-1",
    ]);
    assert_exit(&replay, 0);
    let replay_envelope = stdout_json(&replay);
    assert_eq!(replay_envelope["data"]["status"], "success");
}

#[test]
fn recover_after_external_modification_exits_5_with_needs_manual_recovery() {
    let fixture = Fixture::new();
    let hub = AgentHub::open(CoreConfig::for_home(&fixture.home)).expect("open core hub");
    let imported = hub
        .import_skill(ImportRequest {
            source: ImportSource::Folder {
                path: fixture.source.to_string_lossy().to_string(),
            },
            skill_path: None,
            version_label: None,
            notes: None,
        })
        .expect("import alpha");
    let agents = hub.list_agents().expect("agents");
    let agent_id = |key: &str| {
        agents
            .iter()
            .find(|agent| agent.key == key)
            .unwrap_or_else(|| panic!("agent {key} missing"))
            .id
            .clone()
    };
    let planned = hub
        .plan_install(InstallRequest {
            capability_id: imported.skill.id.clone(),
            version_id: None,
            agent_ids: vec![agent_id("codex"), agent_id("workbuddy")],
            strategy: Some(InstallStrategy::Copy),
            force: false,
        })
        .expect("plan install");
    let grant = hub
        .approve_operation(&planned.operation_id, &planned.plan_hash)
        .expect("approve");
    let result = hub
        .apply_operation_approved(&planned.operation_id, &grant.approval_token, None)
        .expect("apply");
    assert_eq!(result.status, OperationStatus::Success);

    // Simulate a crash after a partial write: another program edits the Codex
    // deployment, so recovery must keep that file and ask for manual review.
    let external_body = "edited by another app";
    fs::write(
        fixture.codex_target().join("SKILL.md"),
        external_body,
    )
    .expect("external edit");
    hub.db()
        .conn()
        .execute(
            "UPDATE operations SET status = 'needs_manual_recovery' WHERE id = ?1",
            [&planned.operation_id],
        )
        .expect("mark operation as needing recovery");
    drop(hub);

    let output = fixture.cli(&[
        "--json",
        "operations",
        "recover",
        &planned.operation_id,
    ]);
    assert_exit(&output, 5);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true, "recovery ran, envelope: {envelope}");
    assert!(
        !envelope["requestId"].as_str().unwrap_or_default().is_empty(),
        "recover envelope must carry a requestId: {envelope}"
    );
    assert_eq!(
        envelope["data"]["operationId"],
        planned.operation_id,
        "recovery must describe the recovered operation"
    );
    assert_eq!(
        envelope["data"]["status"], "needs_manual_recovery",
        "external edits force manual recovery: {envelope}"
    );
    assert_eq!(
        fs::read_to_string(fixture.codex_target().join("SKILL.md")).expect("read external edit"),
        external_body,
        "recovery must never overwrite another program's edit"
    );
    assert!(
        !fixture.workbuddy_target().exists(),
        "the operation's own write must still be rolled back"
    );
}

#[test]
fn invalid_input_exits_3_with_invalid_input_code() {
    let fixture = Fixture::new();

    let preview = fixture.cli(&["--json", "operations", "preview", "does-not-exist"]);
    assert_exit(&preview, 3);
    assert_error_envelope(&preview, "INVALID_INPUT");

    let inspect = fixture.cli(&["--json", "agents", "inspect", "no-such-agent"]);
    assert_exit(&inspect, 3);
    assert_error_envelope(&inspect, "INVALID_INPUT");

    // `agents inspect` works for a detected agent and uses the same envelope.
    let codex = fixture.cli(&["--json", "agents", "inspect", "codex"]);
    assert_exit(&codex, 0);
    let envelope = stdout_json(&codex);
    assert_eq!(envelope["ok"], true, "envelope: {envelope}");
    assert_eq!(envelope["data"]["agentKey"], "codex");
    assert!(
        envelope["data"]["capabilities"]
            .as_array()
            .expect("capabilities")
            .len()
            >= 11,
        "adapter evidence must be listed: {envelope}"
    );
    assert!(
        !envelope["requestId"].as_str().unwrap_or_default().is_empty(),
        "inspect must carry a requestId: {envelope}"
    );
}
