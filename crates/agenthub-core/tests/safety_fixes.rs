//! Regression tests for the production-safety fixes:
//! - diagnostics export must not carry URL-query/argv secrets or home paths;
//! - profile upsert must be all-or-nothing;
//! - backups must be owner-only on disk;
//! - mutating entry points must respect the cross-process runtime lock.

mod common;

use std::collections::BTreeMap;

use agenthub_core::model::*;
use agenthub_core::runtime;

use common::*;

fn http_request(name: &str, url: &str, args: Vec<String>) -> McpUpsertRequest {
    McpUpsertRequest {
        id: None,
        name: name.to_string(),
        description: None,
        transport: McpTransport::Http,
        command: None,
        args,
        url: Some(url.to_string()),
        headers: BTreeMap::new(),
        env_keys: Vec::new(),
        credential_refs: Vec::new(),
    }
}

#[test]
fn diagnostics_export_redacts_url_and_argv_secrets_and_home_paths() {
    let fixture = Fixture::new();
    let hub = fixture.hub();
    let secret = "sk-super-secret-value-123";
    hub.mcp_upsert(http_request(
        "tokenized",
        &format!("https://mcp.example.invalid/mcp?token={secret}&x=1"),
        vec![
            "-y".to_string(),
            "some-mcp".to_string(),
            "--api-key".to_string(),
            secret.to_string(),
        ],
    ))
    .expect("upsert mcp server");

    let out = fixture.home.join("diag/report.json");
    let export = hub.export_diagnostics(&out).expect("export diagnostics");
    assert!(export.path.is_file());
    let body = std::fs::read_to_string(&out).expect("read export");

    assert!(
        !body.contains(secret),
        "diagnostics export must not contain the secret value"
    );
    assert!(
        !body.contains("token=sk"),
        "URL query must be stripped from exported MCP URLs"
    );
    assert!(
        body.contains("https://mcp.example.invalid/mcp"),
        "scheme/host/path must survive redaction"
    );
    assert!(
        body.contains("--api-key") && body.contains("[REDACTED]"),
        "argv secret must be redacted while the flag stays visible"
    );
    // Home paths are folded to ~: the fixture home path (which embeds the
    // tempdir name) must not appear in the export.
    let home_text = fixture.home.to_string_lossy();
    assert!(
        !body.contains(home_text.as_ref()),
        "absolute home paths must be folded to ~ in the export"
    );
}

#[test]
fn profile_upsert_failure_keeps_previous_items() {
    let fixture = Fixture::new();
    let hub = fixture.hub();

    let cap = agenthub_core::db::CapabilityRow {
        id: "cap-1".to_string(),
        kind: "skill".to_string(),
        slug: "alpha".to_string(),
        name: "Alpha".to_string(),
        description: String::new(),
        current_version_id: None,
        tags: Vec::new(),
        license: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    hub.db().insert_capability(&cap).expect("insert capability");

    let request = |capability_id: &str| ProfileUpsertRequest {
        id: None,
        name: "work".to_string(),
        description: None,
        items: vec![ProfileItemInput {
            capability_id: capability_id.to_string(),
            enabled: true,
            target_agent_keys: vec!["codex".to_string()],
        }],
    };

    let saved = hub.upsert_profile(request("cap-1")).expect("save profile");
    assert_eq!(saved.items.len(), 1);

    // Update with a dangling capability: the whole update must fail WITHOUT
    // deleting the previously stored item.
    let result = hub.upsert_profile(ProfileUpsertRequest {
        id: Some(saved.id.clone()),
        ..request("cap-missing")
    });
    assert!(result.is_err(), "dangling capability must fail the update");

    let reloaded = hub
        .list_profiles()
        .expect("list profiles")
        .into_iter()
        .find(|profile| profile.id == saved.id)
        .expect("profile still exists");
    assert_eq!(
        reloaded.items.len(),
        1,
        "failed profile update must keep the original items"
    );
    assert_eq!(reloaded.items[0].capability_id, "cap-1");
}

#[test]
fn backups_are_owner_only_on_disk() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let hub = fixture.hub();
    let backup = hub.create_backup().expect("create backup");
    let backup_dir = std::path::Path::new(&backup.path);
    let dir_mode = std::fs::metadata(backup_dir)
        .expect("backup dir exists")
        .permissions()
        .mode();
    assert_eq!(
        dir_mode & 0o077,
        0,
        "backup directory must not be group/world accessible"
    );
    for file in ["agenthub.db", "manifest.json"] {
        let mode = std::fs::metadata(backup_dir.join(file))
            .unwrap_or_else(|_| panic!("{file} exists"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{file} must be owner-only");
    }
}

#[test]
fn mutating_entry_points_respect_the_runtime_lock() {
    let fixture = Fixture::new();
    let hub = fixture.hub();
    let data_dir = fixture.home.join(".agenthub");
    let guard = runtime::acquire(&data_dir, runtime::RuntimeKind::Desktop)
        .expect("acquire lock as the other process");

    fn assert_busy<T>(name: &str, result: Result<T, agenthub_core::Error>) {
        let err = match result {
            Ok(_) => panic!("{name} must be blocked by the runtime lock"),
            Err(err) => err,
        };
        let err = err.to_string();
        assert!(
            err.contains("runtime is busy"),
            "{name} must be blocked by the runtime lock, got: {err}"
        );
    }
    assert_busy(
        "upsert_profile",
        hub.upsert_profile(ProfileUpsertRequest {
            id: None,
            name: "x".to_string(),
            description: None,
            items: Vec::new(),
        }),
    );
    assert_busy(
        "mcp_upsert",
        hub.mcp_upsert(http_request("blocked", "https://mcp.example.invalid", Vec::new())),
    );
    assert_busy(
        "set_project_roots",
        hub.set_project_roots(vec![fixture.project.to_string_lossy().to_string()]),
    );
    assert_busy(
        "record_validation",
        hub.record_validation(ValidationRequest {
            capability_id: "cap-x".to_string(),
            version_id: None,
            agent_id: None,
            kind: None,
            status: "passed".to_string(),
            case_name: "case".to_string(),
            notes: None,
            evidence: None,
        }),
    );

    drop(guard);
    // After release the same calls reach their normal validation paths.
    let err = hub
        .record_validation(ValidationRequest {
            capability_id: "cap-x".to_string(),
            version_id: None,
            agent_id: None,
            kind: None,
            status: "passed".to_string(),
            case_name: "case".to_string(),
            notes: None,
            evidence: None,
        })
        .expect_err("unknown capability still fails");
    assert!(
        !err.to_string().contains("runtime is busy"),
        "runtime must be free after release, got: {err}"
    );
}
