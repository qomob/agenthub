//! My AI Stack (M1-B) CLI contract tests.
//!
//! Every command runs as a real process against an isolated temporary
//! `AGENTHUB_HOME`, verifying at the binary boundary:
//!
//! * the v3.1 `ApiResult` envelope for the new stack commands,
//! * the D-1 direct-write + explicit `--yes` gate for high-impact commands,
//! * the D-2 error contract (`RESOURCE_CONFLICT` → exit 4),
//! * UX-2/UX-4 semantics (amount parsing, currency kept separate, CLI data
//!   equal to the service-layer data it wraps).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use agenthub_core::{AgentHub, CoreConfig};
use serde_json::Value;
use tempfile::TempDir;

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

/// Assert the failure envelope shape and return its `error` object.
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
        let source = temp.path().join("sources/alpha");
        fs::create_dir_all(&source).expect("skill source dir");
        fs::write(
            source.join("SKILL.md"),
            "---\nname: alpha\ndescription: stack cli test skill.\nversion: 1.0.0\n---\n\nbody\n",
        )
        .expect("write SKILL.md");
        Fixture {
            _temp: temp,
            home,
            source,
        }
    }

    fn cli(&self, args: &[&str]) -> Output {
        cli(&self.home, args)
    }

    fn hub(&self) -> AgentHub {
        let config = CoreConfig::for_home(self.home.clone());
        AgentHub::open(config).expect("open hub")
    }

    /// Add a manual stack item and return its id.
    fn add_item(&self, name: &str) -> String {
        let output = self.cli(&["--json", "stack", "add", "--name", name]);
        assert_exit(&output, 0);
        let envelope = stdout_json(&output);
        assert_eq!(envelope["ok"], true, "envelope: {envelope}");
        envelope["data"]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("stack add must return the item id: {envelope}"))
            .to_string()
    }

    fn add_subscription(&self, stack: &str, product: &str) -> String {
        let output = self.cli(&[
            "--json",
            "sub",
            "add",
            "--stack",
            stack,
            "--product",
            product,
            "--billing-type",
            "subscription",
            "--amount",
            "20",
            "--currency",
            "USD",
            "--period",
            "monthly",
        ]);
        assert_exit(&output, 0);
        stdout_json(&output)["data"]["id"]
            .as_str()
            .expect("subscription id")
            .to_string()
    }
}

// ---------------------------------------------------------------------------
// Envelope shape
// ---------------------------------------------------------------------------

#[test]
fn stack_list_uses_the_v31_envelope_with_camel_case_data() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Cursor");
    fixture.add_subscription(&id, "Cursor Pro");

    let output = fixture.cli(&["--json", "stack", "list"]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], true, "envelope: {envelope}");
    assert!(
        !envelope["requestId"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "envelope must carry a requestId: {envelope}"
    );
    let items = envelope["data"].as_array().expect("data items");
    assert_eq!(items.len(), 1, "envelope: {envelope}");
    let item = &items[0];
    assert_eq!(item["id"], id, "envelope: {envelope}");
    assert_eq!(item["name"], "Cursor", "envelope: {envelope}");
    // camelCase business fields (GUI consumes the same shape).
    assert_eq!(item["detectionState"], "unknown", "envelope: {envelope}");
    assert_eq!(
        item["subscriptionState"], "registered",
        "envelope: {envelope}"
    );
    assert_eq!(item["subscriptionCount"], 1, "envelope: {envelope}");
}

// ---------------------------------------------------------------------------
// Amount parsing (UX-2: unknown stays unknown, no fake zero)
// ---------------------------------------------------------------------------

#[test]
fn sub_add_rejects_amount_without_currency_and_never_fills_zero() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Windsurf");

    let output = fixture.cli(&[
        "--json",
        "sub",
        "add",
        "--stack",
        &id,
        "--product",
        "Windsurf Pro",
        "--billing-type",
        "subscription",
        "--amount",
        "20",
    ]);
    assert_exit(&output, 3);
    assert_error_envelope(&output, "INVALID_INPUT");

    // Without an amount the subscription is still registered with unknown amount.
    let output = fixture.cli(&[
        "--json",
        "sub",
        "add",
        "--stack",
        &id,
        "--product",
        "Windsurf Pro",
        "--billing-type",
        "subscription",
    ]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    assert!(
        envelope["data"]["amountMinor"].is_null(),
        "unknown amount must stay null, never zero: {envelope}"
    );
}

#[test]
fn sub_add_converts_major_units_to_minor_exactly() {
    let fixture = Fixture::new();
    let id = fixture.add_item("ChatGPT");
    let output = fixture.cli(&[
        "--json",
        "sub",
        "add",
        "--stack",
        &id,
        "--product",
        "Plus",
        "--billing-type",
        "subscription",
        "--amount",
        "20.5",
        "--currency",
        "USD",
        "--period",
        "monthly",
    ]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    assert_eq!(
        envelope["data"]["amountMinor"], 2050,
        "USD has a 2-digit exponent: {envelope}"
    );
    assert_eq!(envelope["data"]["currency"], "USD", "envelope: {envelope}");
    // A zero-decimal currency rejects sub-minor input.
    let jpy = fixture.add_item("JPY Tool");
    let output = fixture.cli(&[
        "--json",
        "sub",
        "add",
        "--stack",
        &jpy,
        "--product",
        "JPY Plan",
        "--billing-type",
        "subscription",
        "--amount",
        "100.5",
        "--currency",
        "JPY",
    ]);
    assert_exit(&output, 3);
    assert_error_envelope(&output, "INVALID_INPUT");
}

// ---------------------------------------------------------------------------
// High-impact confirmation gate (D-1)
// ---------------------------------------------------------------------------

#[test]
fn stack_delete_without_yes_is_refused_and_changes_nothing() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Refusals");

    let output = fixture.cli(&["--json", "stack", "delete", &id]);
    assert_eq!(exit_code(&output), 1, "refusal exits 1: {output:?}");

    // The item still exists.
    let output = fixture.cli(&["--json", "stack", "list"]);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["data"].as_array().unwrap().len(), 1);
}

#[test]
fn stack_merge_without_yes_is_refused() {
    let fixture = Fixture::new();
    let a = fixture.add_item("Tool A");
    let b = fixture.add_item("Tool B");

    let output = fixture.cli(&["--json", "stack", "merge", &a, "--into", &b]);
    assert_exit(&output, 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--yes"),
        "refusal must tell the user how to confirm: {stderr}"
    );

    // A legal merge with --yes succeeds.
    let output = fixture.cli(&["--json", "stack", "merge", &a, "--into", &b, "--yes"]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    assert_eq!(envelope["data"]["mergedInto"], b, "envelope: {envelope}");
}

// ---------------------------------------------------------------------------
// D-2 error contract: RESOURCE_CONFLICT → exit 4
// ---------------------------------------------------------------------------

#[test]
fn deleting_an_item_with_subscription_is_resource_conflict_exit_4() {
    let fixture = Fixture::new();
    let id = fixture.add_item("Paid Tool");
    fixture.add_subscription(&id, "Paid Plan");

    let output = fixture.cli(&["--json", "stack", "delete", &id, "--yes"]);
    assert_exit(&output, 4);
    let error = assert_error_envelope(&output, "RESOURCE_CONFLICT");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("subscription"),
        "conflict must explain why: {message}"
    );
}

#[test]
fn merge_rejections_follow_the_d2_contract() {
    let fixture = Fixture::new();
    let a = fixture.add_item("A");
    let b = fixture.add_item("B");
    let c = fixture.add_item("C");

    // Merge A into B, then try to merge B into C: B is now a merged child.
    let output = fixture.cli(&["--json", "stack", "merge", &a, "--into", &b, "--yes"]);
    assert_exit(&output, 0);
    let output = fixture.cli(&["--json", "stack", "merge", &b, "--into", &c, "--yes"]);
    assert_exit(&output, 4);
    assert_error_envelope(&output, "RESOURCE_CONFLICT");

    // A legal delete still works for a clean item.
    let output = fixture.cli(&["--json", "stack", "delete", &c, "--yes"]);
    assert_exit(&output, 0);
}

// ---------------------------------------------------------------------------
// UX-4: CLI JSON data equals the service-layer data (requestId excluded)
// ---------------------------------------------------------------------------

#[test]
fn cli_json_data_equals_service_layer_data() {
    let fixture = Fixture::new();
    let a = fixture.add_item("A Tool");
    let b = fixture.add_item("B Tool");
    fixture.add_subscription(&a, "A Plan");
    fixture.cli(&["--json", "stack", "merge", &b, "--into", &a, "--yes"]);
    // Register one more subscription directly through the service layer.
    let hub = fixture.hub();
    let upsert = agenthub_core::model::SubscriptionUpsert {
        id: None,
        stack_item_id: a.clone(),
        product_name: "Yearly Plan".into(),
        vendor: Some("Vendor".into()),
        plan_name: None,
        billing_type: "subscription".into(),
        amount_minor: Some(12_000),
        currency: Some("USD".into()),
        billing_period: Some("yearly".into()),
        started_on: Some("2026-01-01".into()),
        renews_on: Some("2027-01-01".into()),
        status: Some("active".into()),
        notes: None,
        info_source: Some("fixture".into()),
    };
    hub.subscription_upsert(upsert).expect("service upsert");
    drop(hub);

    let output = fixture.cli(&["--json", "stack", "list"]);
    assert_exit(&output, 0);
    let envelope = stdout_json(&output);
    let cli_items = envelope["data"].as_array().expect("cli items");

    let hub = fixture.hub();
    let service_items =
        serde_json::to_value(hub.stack_items().expect("service items")).expect("serialize");
    let service_items = service_items.as_array().expect("service items").to_vec();

    assert_eq!(cli_items.len(), service_items.len());
    // Normalized comparison: sort by id, ignore the envelope (requestId).
    let mut cli_sorted: Vec<&Value> = cli_items.iter().collect();
    cli_sorted.sort_by_key(|v| v["id"].as_str().unwrap_or_default().to_string());
    let mut svc_sorted: Vec<&Value> = service_items.iter().collect();
    svc_sorted.sort_by_key(|v| v["id"].as_str().unwrap_or_default().to_string());
    assert_eq!(
        serde_json::to_value(&cli_sorted).unwrap(),
        serde_json::to_value(&svc_sorted).unwrap(),
        "CLI data must equal service data (UX-4)"
    );
}

// ---------------------------------------------------------------------------
// Spending semantics surface in the CLI (UX-2: no mixed sums, no fake zeros)
// ---------------------------------------------------------------------------

#[test]
fn stack_list_reports_per_currency_spending_and_incomplete_items() {
    let fixture = Fixture::new();
    let usd = fixture.add_item("USD Tool");
    fixture.add_subscription(&usd, "USD Plan");
    let cny = fixture.add_item("CNY Tool");
    fixture.cli(&[
        "--json",
        "sub",
        "add",
        "--stack",
        &cny,
        "--product",
        "CNY Plan",
        "--billing-type",
        "subscription",
        "--amount",
        "100",
        "--currency",
        "CNY",
        "--period",
        "monthly",
    ]);
    // Unknown-amount subscription → incomplete, never folded into a sum.
    let unknown = fixture.add_item("Unknown Tool");
    fixture.cli(&[
        "--json",
        "sub",
        "add",
        "--stack",
        &unknown,
        "--product",
        "Mystery Plan",
        "--billing-type",
        "subscription",
    ]);

    let output = fixture.cli(&["stack", "list"]);
    assert_exit(&output, 0);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USD"), "per-currency line: {stdout}");
    assert!(stdout.contains("CNY"), "per-currency line: {stdout}");
    assert!(
        stdout.contains("monthly equivalent"),
        "must be labeled as equivalent, not this month's charge: {stdout}"
    );
    assert!(
        stdout.contains("incomplete"),
        "incomplete items must be surfaced: {stdout}"
    );
    // USD 20/mo and CNY 100/mo are listed but never summed together.
    assert!(stdout.contains("20.00"), "USD line: {stdout}");
    assert!(stdout.contains("100.00"), "CNY line: {stdout}");
}
