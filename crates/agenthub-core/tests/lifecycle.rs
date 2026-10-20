//! Integration tests: first-run initialisation, repeated migrations and
//! defensive parsing of legacy operation rows.

mod common;

use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};

#[test]
fn fresh_home_initialises_library_profiles_and_migrations() {
    let temp = tempfile::tempdir().expect("temp dir");
    let home = temp.path().join("fresh-home");
    let config = CoreConfig::for_home(&home);
    assert!(!config.data_dir.exists(), "fixture must start empty");

    let hub = AgentHub::open(config.clone()).expect("first open on a fresh home");
    assert!(config.db_path.is_file(), "SQLite database must be created");
    assert!(
        home.join(".agenthub").is_dir(),
        "data dir must live under the fake home"
    );
    for dir in ["library/skills", "snapshots", "staging", "backups", "logs"] {
        assert!(
            config.data_dir.join(dir).is_dir(),
            "library layout directory {dir} is missing"
        );
    }

    // Seeded defaults.
    let settings = hub.settings().expect("settings");
    assert!(!settings.initialized);
    assert_eq!(settings.default_strategy, InstallStrategy::Copy);
    assert_eq!(settings.library_root, config.data_dir.to_string_lossy());
    assert_eq!(settings.database_path, config.db_path.to_string_lossy());

    // Built-in profiles.
    let profiles = hub.list_profiles().expect("profiles");
    assert_eq!(profiles.len(), 4, "four built-in profiles expected");
    assert!(profiles.iter().all(|profile| profile.is_builtin));
    let names: Vec<&str> = profiles.iter().map(|profile| profile.name.as_str()).collect();
    for expected in ["Coding", "Research", "Writing", "Marketing"] {
        assert!(names.contains(&expected), "missing profile {expected}: {names:?}");
    }

    // All known adapters are registered even when not installed.
    let agents = hub.list_agents().expect("agents");
    assert!(agents.len() >= 9, "expected all built-in adapters: {agents:?}");
    assert!(agents.iter().any(|agent| agent.key == "codex"));

    // Schema version is tracked.
    let schema_version: i64 = hub
        .db()
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");
    assert_eq!(schema_version, 3);

    drop(hub);

    // Reopening the same home must not fail or duplicate seeded rows.
    let hub = AgentHub::open(config.clone()).expect("second open on existing home");
    assert_eq!(hub.list_profiles().expect("profiles").len(), 4);
    assert_eq!(hub.list_agents().expect("agents").len(), agents.len());
    let schema_version: i64 = hub
        .db()
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");
    assert_eq!(schema_version, 3, "migrations must be idempotent");
}

#[test]
fn legacy_operation_rows_are_parsed_defensively() {
    let temp = tempfile::tempdir().expect("temp dir");
    let home = temp.path().join("legacy-home");
    let hub = AgentHub::open(CoreConfig::for_home(&home)).expect("open hub");

    let insert = |id: &str, kind: &str, status: &str| {
        hub.db()
            .conn()
            .execute_batch(&format!(
                "INSERT INTO operations \
                 (id, kind, status, dry_run, actor, summary, plan, result, error, rollback_of, started_at, finished_at) \
                 VALUES ('{id}', '{kind}', '{status}', 1, 'user', 'legacy row', '{{}}', '{{}}', NULL, NULL, '2024-01-01T00:00:00Z', NULL);"
            ))
            .expect("insert legacy operation row");
    };

    insert("op-success", "install", "success");
    insert("op-partial", "install", "partial");
    insert("op-rolled-back", "install", "rolled_back");
    insert("op-planned", "install", "planned");
    insert("op-unknown", "install", "some-legacy-status");

    let expected = [
        ("op-success", OperationStatus::Success),
        ("op-partial", OperationStatus::Partial),
        ("op-rolled-back", OperationStatus::RolledBack),
        ("op-planned", OperationStatus::Planned),
        // Unknown legacy values must not panic; they degrade to Failed.
        ("op-unknown", OperationStatus::Failed),
    ];
    for (id, status) in expected {
        let view = hub.get_operation(id).expect("operation view");
        assert_eq!(view.status, status, "wrong status for {id}");
        assert!(
            view.plan.is_none(),
            "an unparseable legacy plan must degrade to None"
        );
        assert!(
            view.result.is_none(),
            "an unparseable legacy result must degrade to None"
        );
        assert_eq!(view.item_count, 0);
        assert!(!view.can_rollback, "legacy rows without items are not rollbackable");
    }

    // The operation list includes every legacy row.
    let listed = hub.list_operations(50).expect("list operations");
    assert!(listed.len() >= 5);
}
