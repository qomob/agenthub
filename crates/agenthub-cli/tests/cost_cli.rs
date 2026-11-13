//! Cost Optimizer (M2-B) CLI contract tests.
//!
//! Runs the real binary against an isolated `AGENTHUB_HOME` and verifies:
//! * `cost report` / `cost settings` / `cost simulate` use the v3.1 envelope
//!   with the same camelCase data shapes the GUI consumes,
//! * settings writes go through the shared service validation (bad currency,
//!   non-positive threshold, out-of-range window → `INVALID_INPUT`, exit 3),
//! * GUI/CLI parity: the CLI's settings JSON equals the service-layer view,
//! * simulation stays read-only (the database is byte-identical after) and
//!   deltas stay per-currency.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use agenthub_core::{AgentHub, CoreConfig};
use serde_json::Value;
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".codex")).expect("fake .codex");
        Fixture { _temp: temp, home }
    }

    fn cli(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_agenthub"))
            .env("AGENTHUB_HOME", &self.home)
            .env_remove("AGENTHUB_DATA_DIR")
            .args(args)
            .output()
            .expect("run agenthub CLI")
    }

    fn hub(&self) -> AgentHub {
        AgentHub::open(CoreConfig::for_home(self.home.clone())).expect("open hub")
    }

    fn add_item(&self, name: &str) -> String {
        let output = self.cli(&["--json", "stack", "add", "--name", name]);
        assert_eq!(output.status.code(), Some(0), "stack add: {output:?}");
        stdout_json(&output)["data"]["id"]
            .as_str()
            .expect("item id")
            .to_string()
    }

    /// Registers a USD 20/month subscription (2000 minor) on the item and
    /// returns its id.
    fn add_subscription(&self, stack: &str, product: &str) -> String {
        let output = self.cli(&[
            "--json", "sub", "add",
            "--stack", stack,
            "--product", product,
            "--billing-type", "subscription",
            "--amount", "20",
            "--currency", "USD",
            "--period", "monthly",
        ]);
        assert_eq!(output.status.code(), Some(0), "sub add: {output:?}");
        stdout_json(&output)["data"]["id"]
            .as_str()
            .expect("subscription id")
            .to_string()
    }
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().unwrap_or_else(|| panic!("signal: {output:?}"))
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        exit_code(output),
        expected,
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!("stdout JSON ({err}): {}", String::from_utf8_lossy(&output.stdout))
    })
}

fn stderr_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!("stderr JSON ({err}): {}", String::from_utf8_lossy(&output.stderr))
    })
}

fn assert_error_envelope(output: &Output, expected_code: &str) -> Value {
    let envelope = stderr_json(output);
    assert_eq!(envelope["ok"], false, "envelope: {envelope}");
    assert_eq!(
        envelope["error"]["code"], expected_code,
        "envelope: {envelope}"
    );
    envelope
}

#[test]
fn cost_report_uses_the_v31_envelope_and_shared_shapes() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Alpha");
    fixture.add_subscription(&id, "Alpha Pro");

    let output = fixture.cli(&["--json", "cost", "report"]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true, "envelope: {envelope}");
    assert!(
        !envelope["requestId"].as_str().unwrap_or_default().is_empty(),
        "envelope: {envelope}"
    );
    let counts = &envelope["data"]["counts"];
    assert_eq!(counts["stackItems"], 1, "envelope: {envelope}");
    assert_eq!(counts["subscriptionsActive"], 1, "envelope: {envelope}");
    // R5 fires for the never-confirmed item (same data the GUI panel shows).
    assert!(
        envelope["data"]["recommendations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["recommendationType"] == "R5_DATA_GAP"),
        "envelope: {envelope}"
    );
}

#[test]
fn cost_settings_roundtrip_is_shared_between_surfaces() {
    let fixture = Fixture::new();
    let output = fixture.cli(&["--json", "cost", "set", "--threshold", "USD:5000", "--window", "14"]);
    assert_exit(&output, 0);
    let stored = stdout_json(&output);
    assert_eq!(stored["ok"], true, "envelope: {stored}");
    assert_eq!(
        stored["data"]["costThresholdsMinor"]["USD"], 5000,
        "envelope: {stored}"
    );
    assert_eq!(stored["data"]["renewalWindowDays"], 14, "envelope: {stored}");

    // Reading back through a separate process gives the same JSON shape the
    // GUI's optimizer_settings command returns.
    let read = fixture.cli(&["--json", "cost", "settings"]);
    assert_exit(&read, 0);
    let reread = stdout_json(&read);
    assert_eq!(reread["data"]["costThresholdsMinor"]["USD"], 5000);
    assert_eq!(reread["data"]["renewalWindowDays"], 14);

    // Service-layer parity: the hub sees exactly the same values.
    let settings = fixture.hub().optimizer_settings().unwrap();
    assert_eq!(settings.cost_thresholds_minor.get("USD"), Some(&5000));
    assert_eq!(settings.renewal_window_days, Some(14));
}

#[test]
fn cost_settings_rejects_invalid_parameters_as_invalid_input() {
    let fixture = Fixture::new();

    // Bad currency code → INVALID_INPUT (exit 3), same D-2 contract as core.
    let output = fixture.cli(&["--json", "cost", "set", "--threshold", "usdd:5000"]);
    assert_exit(&output, 3);
    assert_error_envelope(&output, "INVALID_INPUT");

    // Non-positive threshold.
    let output = fixture.cli(&["--json", "cost", "set", "--threshold", "USD:0"]);
    assert_exit(&output, 3);
    assert_error_envelope(&output, "INVALID_INPUT");

    // Window out of range.
    let output = fixture.cli(&["--json", "cost", "set", "--window", "31"]);
    assert_exit(&output, 3);
    assert_error_envelope(&output, "INVALID_INPUT");
}

#[test]
fn cost_simulate_is_read_only_and_per_currency() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Alpha");
    fixture.add_subscription(&id, "Alpha Pro");
    // Subscription is USD 60/month → 6000 minor.
    let detail = fixture
        .hub()
        .stack_item_detail(&id)
        .unwrap();
    let sub_id = detail.subscriptions[0].id.clone();

    let db_before = std::fs::read(fixture.home.join(".agenthub/agenthub.db")).unwrap();

    let output = fixture.cli(&["--json", "cost", "simulate", "--cancel", &sub_id]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true, "envelope: {envelope}");
    let deltas = envelope["data"]["deltasMonthly"].as_array().expect("deltas");
    assert_eq!(deltas.len(), 1, "one currency, never summed: {envelope}");
    assert_eq!(deltas[0]["currency"], "USD");
    assert_eq!(deltas[0]["minorBefore"], 2000);
    assert_eq!(deltas[0]["minorAfter"], 0);
    assert_eq!(deltas[0]["deltaMinor"], -2000);
    assert_eq!(envelope["data"]["affectedSubscriptions"][0], sub_id);

    // Read-only: the database bytes are unchanged by the simulation.
    let db_after = std::fs::read(fixture.home.join(".agenthub/agenthub.db")).unwrap();
    assert_eq!(db_before, db_after, "simulation must not write");
}

#[test]
fn cost_report_reflects_the_shared_threshold_configured_via_cli() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Alpha");
    fixture.add_subscription(&id, "Alpha Pro");

    // No threshold configured → R2 silent.
    let output = fixture.cli(&["--json", "cost", "report"]);
    let envelope = stdout_json(&output);
    assert!(
        envelope["data"]["recommendations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["recommendationType"] != "R2_HIGH_COST"),
        "R2 must stay silent without thresholds: {envelope}"
    );

    let set = fixture.cli(&["--json", "cost", "set", "--threshold", "USD:2000"]);
    assert_exit(&set, 0);

    let output = fixture.cli(&["--json", "cost", "report"]);
    let envelope = stdout_json(&output);
    // The persisted threshold drives the same service the GUI reads.
    let r2 = envelope["data"]["recommendations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["recommendationType"] == "R2_HIGH_COST")
        .expect("R2 must fire with the persisted threshold");
    assert_eq!(
        r2["confidenceOrEvidenceLevel"], "user_configured",
        "envelope: {envelope}"
    );
    assert!(
        r2["explanation"].as_str().unwrap_or_default().contains("20.00 USD"),
        "explanation: {r2}"
    );
}
