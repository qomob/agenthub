//! SCHEMA_V4 migration, backup and restore-path tests (M1-A, P0-5).
//!
//! Everything runs against isolated fixture homes; no real `~/.agenthub`
//! database is ever opened. The frozen no-downgrade policy means the only
//! recovery path from v4 is restoring a pre-migration backup — which is what
//! these tests exercise, followed by a re-migration proving the path works.

mod common;

use common::Fixture;

fn user_version(hub: &agenthub_core::AgentHub) -> i64 {
    hub.db()
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version")
}

fn table_exists(hub: &agenthub_core::AgentHub, name: &str) -> bool {
    let count: i64 = hub
        .db()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .unwrap();
    count > 0
}

fn find_pre_v3_backup(home: &std::path::Path) -> Option<std::path::PathBuf> {
    let backups = home.join(".agenthub/backups");
    let mut found = None;
    if let Ok(entries) = std::fs::read_dir(&backups) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("pre-migration-v3-") {
                found = Some(entry.path().join("agenthub.db"));
            }
        }
    }
    found
}

#[test]
fn fresh_open_migrates_to_v4_with_stack_tables_and_backup() {
    let f = Fixture::new();
    let hub = f.hub();
    assert_eq!(user_version(&hub), 4, "fresh fixture migrates through v4");
    for table in ["stack_items", "stack_detections", "subscription_records"] {
        assert!(
            table_exists(&hub, table),
            "{table} must exist after migration"
        );
    }
    hub.db()
        .integrity_check()
        .expect("integrity check after v4");

    // Every migration (including the fresh-open v4 run) must leave a
    // pre-migration backup with a readable manifest behind.
    let backup = find_pre_v3_backup(&f.home).expect("v4 migration must produce a backup");
    let manifest = std::fs::read_to_string(backup.parent().unwrap().join("manifest.json")).unwrap();
    assert!(manifest.contains("\"from_version\": 3"), "got: {manifest}");
    assert!(manifest.contains("\"to_version\": 4"), "got: {manifest}");
    assert!(
        manifest.contains("sha256"),
        "manifest must record the snapshot digest"
    );
}

#[test]
fn migration_failure_rolls_back_and_keeps_version_3() {
    // Inject a mid-migration failure: a pre-existing `stack_items` table with
    // an incompatible shape. CREATE TABLE IF NOT EXISTS skips the real DDL and
    // the first dependent statement fails inside the transaction, which must
    // roll back cleanly (P0-5/E2): no partial writes, user_version stays 3.
    let f = Fixture::with_agents(&[".codex"]);
    let home = f.home.join(".agenthub");
    std::fs::create_dir_all(&home).unwrap();
    {
        let conn = rusqlite::Connection::open(home.join("agenthub.db")).unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute_batch(
            "CREATE TABLE stack_items (bogus INTEGER NOT NULL);
             INSERT INTO stack_items (bogus) VALUES (1);",
        )
        .unwrap();
    }
    match agenthub_core::AgentHub::open(agenthub_core::CoreConfig::for_home(&f.home)) {
        Err(err) => {
            let text = err.to_string();
            assert!(
                text.contains("no such column") || text.contains("merged_into"),
                "expected a schema failure, got: {text}"
            );
        }
        Ok(_) => panic!("migration must fail against the incompatible table"),
    }

    // Reopen and verify the rollback left the database untouched.
    let conn = rusqlite::Connection::open(home.join("agenthub.db")).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3, "user_version must not advance on failure");
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM stack_items", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 1, "injected row still there: transaction rolled back");
    let stack_detections: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='stack_detections'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stack_detections, 0, "no partial v4 tables");
}

#[test]
fn restore_pre_migration_backup_downgrades_and_re_migrates() {
    // 2.12 recovery path: v4 database → restore the pre-migration-v3 backup →
    // user_version 3, no stack tables → reopening migrates to v4 again.
    //
    // Note: `restore_backup()` handles library backups (manifest kind
    // "database-backup"); pre-migration snapshots are restored by replacing
    // the database file — the manual path exercised here and documented in
    // DATA_MODEL §6.
    let f = Fixture::new();
    let hub = f.hub();
    assert_eq!(user_version(&hub), 4);
    let backup_db = find_pre_v3_backup(&f.home).expect("pre-migration-v3 backup must exist");
    drop(hub); // close the connection before replacing the file

    let live_db = f.home.join(".agenthub/agenthub.db");
    for suffix in ["-wal", "-shm"] {
        let sidecar = live_db.to_string_lossy().to_string() + suffix;
        let _ = std::fs::remove_file(sidecar);
    }
    std::fs::copy(&backup_db, &live_db).expect("restore pre-migration snapshot");

    // The restored database is at v3 with no stack tables.
    let conn = rusqlite::Connection::open(&live_db).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3, "restore rolls the schema back to v3");
    let stack_items: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='stack_items'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stack_items, 0, "stack tables are gone after restore");
    drop(conn);

    // Reopening must migrate to v4 again and the module must work.
    let hub = f.hub();
    assert_eq!(user_version(&hub), 4, "reopen re-migrates v3 -> v4");
    hub.stack_refresh_detections()
        .expect("stack module works after recovery");
    assert!(table_exists(&hub, "stack_items"));
}

#[test]
fn reopening_a_v4_database_never_degrades_or_rewrites_it() {
    // P0-5 assertion 3: an application rollback to an older binary reads a
    // user_version=4 database without degrading or rewriting it — the
    // version-guarded migrate() loop skips every branch >= its schema range
    // and leaves user_version untouched.
    let f = Fixture::new();
    let hub = f.hub();
    assert_eq!(user_version(&hub), 4);
    let before = user_version(&hub);
    drop(hub);

    // Re-open (the same code path an older binary uses: version guard first).
    let hub = f.hub();
    assert_eq!(user_version(&hub), before, "no version movement on reopen");
    hub.db().integrity_check().expect("database intact");
    assert!(table_exists(&hub, "stack_items"), "v4 tables untouched");
}
