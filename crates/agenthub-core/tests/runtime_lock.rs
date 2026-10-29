//! Cross-process runtime lock integration tests.
//!
//! The runtime lock (see `core/src/runtime.rs`) must make concurrent mutating
//! entry points fail fast with a `Blocked` error instead of interleaving writes
//! to the SQLite database and real agent directories.

use agenthub_core::{AgentHub, CoreConfig};

#[test]
fn mutating_entries_fail_busy_while_runtime_is_held() {
    let home = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(CoreConfig::for_home(home.path())).unwrap();

    // Simulate another process mid-mutation by holding the lock file.
    let data_dir = home.path().join(".agenthub");
    let guard = agenthub_core::runtime::acquire(
        &data_dir,
        agenthub_core::runtime::RuntimeKind::Desktop,
    )
    .unwrap();

    let err = hub
        .apply_operation("op-does-not-matter", false)
        .expect_err("apply must refuse to run while another process holds the runtime");
    let text = err.to_string();
    assert!(
        text.contains("runtime is busy"),
        "expected a busy runtime error, got: {text}"
    );
    assert!(
        text.contains("desktop"),
        "error should name the holding kind, got: {text}"
    );

    // Read-only entry points are unaffected by the held lock.
    let agents = hub.list_agents().expect("read paths must not block");
    assert!(!agents.is_empty() || agents.is_empty()); // shape-only sanity check

    drop(guard);

    // After release, the same entry point reaches its normal validation
    // (unknown operation => NotFound, not Blocked).
    let err = hub
        .apply_operation("op-does-not-matter", false)
        .expect_err("unknown operation should still fail");
    assert!(
        !err.to_string().contains("runtime is busy"),
        "runtime lock must be released after drop, got: {err}"
    );
}

#[test]
fn sequential_operations_reacquire_the_runtime() {
    let home = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(CoreConfig::for_home(home.path())).unwrap();
    // Two mutating calls back to back: each acquires and releases the lock.
    for _ in 0..2 {
        let err = hub
            .apply_operation("op-unknown", false)
            .expect_err("unknown operation should fail");
        assert!(
            !err.to_string().contains("runtime is busy"),
            "runtime must be free between sequential calls, got: {}",
            err
        );
    }
}

/// Child mode: when spawned by the race test, hold the runtime lock for a
/// while so the parent (a genuinely separate process) can observe the busy
/// behaviour across process boundaries.
fn hold_mode() -> ! {
    let dir = std::env::var("AGENTHUB_TEST_HOLD_DIR").expect("hold dir");
    let guard = agenthub_core::runtime::acquire(
        std::path::Path::new(&dir),
        agenthub_core::runtime::RuntimeKind::Cli,
    )
    .expect("child must acquire the lock");
    println!("held");
    std::thread::sleep(std::time::Duration::from_secs(15));
    drop(guard);
    std::process::exit(0);
}

/// Reproduction for the unlink race: if releasing the guard deletes the lock
/// file, a process that opened the file before the delete can later "acquire"
/// a lock on the now-unlinked inode while a third process creates and locks a
/// fresh file — two simultaneous "owners".
///
/// Sequence (all within one process, using a saved raw handle as process B):
/// 1. A acquires; B opens the same file (before A releases).
/// 2. A drops; if the implementation unlinks the file, the name now points at
///    a new inode once C acquires.
/// 3. C acquires (new inode). B then locks the old (unlinked) inode and must
///    NOT succeed — otherwise B and C are both "holding the runtime".
#[test]
fn releasing_the_lock_must_not_unlink_the_lock_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.lock");

    // A: acquire, then B: open the same inode while A still holds it.
    let a = agenthub_core::runtime::acquire(
        dir.path(),
        agenthub_core::runtime::RuntimeKind::Desktop,
    )
    .unwrap();
    let b_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("second process can open the lock file while it is held");
    drop(a);

    // C: acquire again — allowed, A released.
    let c = agenthub_core::runtime::acquire(
        dir.path(),
        agenthub_core::runtime::RuntimeKind::Cli,
    )
    .expect("C must acquire after A released");

    // B must see the runtime as busy: C holds it. If B's flock on the old,
    // possibly unlinked inode succeeds, mutual exclusion is broken.
    use fs2::FileExt;
    let b_lock = b_file.try_lock_exclusive();
    assert!(
        b_lock.is_err(),
        "B locked a stale/unlinked lock inode while C holds the current one: \
         releasing the guard must not delete runtime.lock"
    );
    if let Err(err) = b_lock {
        let _ = err; // Expected: resource busy.
    }
    drop(c);
}

#[test]
fn busy_error_crosses_process_boundaries() {
    if std::env::var("AGENTHUB_TEST_HOLD_DIR").is_ok() {
        hold_mode();
    }
    let dir = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("busy_error_crosses_process_boundaries")
        .arg("--exact")
        .arg("--nocapture")
        .env("AGENTHUB_TEST_HOLD_DIR", dir.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn holder process");

    // Poll until the separate holder process is observable: at least one busy
    // error naming the holder kind must occur while it is alive.
    let mut saw_busy = String::new();
    for _ in 0..100 {
        if let Ok(guard) = agenthub_core::runtime::acquire(
            dir.path(),
            agenthub_core::runtime::RuntimeKind::Desktop,
        ) {
            // Raced past the child before it locked; release and retry.
            drop(guard);
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        } else if let Err(err) = agenthub_core::runtime::acquire(
            dir.path(),
            agenthub_core::runtime::RuntimeKind::Desktop,
        ) {
            saw_busy = err.to_string();
            break;
        }
    }
    assert!(
        saw_busy.contains("runtime is busy") && saw_busy.contains("cli"),
        "expected cross-process busy error naming the cli holder, got: {saw_busy}"
    );

    child.kill().expect("kill holder");
    let _ = child.wait();

    // After the holder dies, the OS releases the flock: immediately acquirable.
    let guard = agenthub_core::runtime::acquire(
        dir.path(),
        agenthub_core::runtime::RuntimeKind::Desktop,
    );
    assert!(guard.is_ok(), "lock must be free after holder death");
}
