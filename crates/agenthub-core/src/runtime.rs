//! Cross-process runtime mutex for mutating operations.
//!
//! Design notes (method learned from studying juejin-cn/juejin-usage's
//! `tud.pid {pid, kind}` owner model; this is an independent implementation):
//!
//! - CLI and GUI share one `agenthub-core` engine against one SQLite database
//!   and the same real agent directories. The in-process `write_lock` in
//!   `service.rs` only serializes writes within one process; two processes
//!   (e.g. the desktop app and a CLI `skills sync --yes`) can still interleave.
//! - Instead of juejin-usage's "desktop steals ownership, CLI degrades to
//!   observer" owner model, AgentHub uses a narrower contract: the lock is held
//!   **only for the duration of a single mutating operation** (apply, rollback,
//!   backup, restore). Read-only commands never block, and short-lived CLI
//!   calls stay usable while the GUI is open.
//! - Implementation is an advisory `flock` (via `fs2`) on
//!   `<data_dir>/runtime.lock`. The OS releases the lock automatically if the
//!   process dies, so there are no stale lock files to clean up — unlike a
//!   bare pid file, which needs liveness probing.
//! - The lock file is **never deleted** while the runtime is in use. Deleting
//!   it on release would open an unlink race: a process that opened the file
//!   before the unlink could later flock the orphaned inode while a fresh
//!   process creates and locks a replacement file — two simultaneous owners.
//!   A leftover zero-length lock file is harmless.
//! - The file body records `{pid, kind, acquiredAt}` purely as diagnostics so
//!   the "busy" error can tell the user *who* holds the runtime.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const LOCK_FILE_NAME: &str = "runtime.lock";

/// Which kind of front end holds the runtime. Detected from the
/// `AGENTHUB_PROCESS` environment variable so the diagnostics in the lock file
/// can name the holder; it plays no role in locking itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Cli,
    Desktop,
    Process,
}

impl RuntimeKind {
    pub fn detect() -> Self {
        match std::env::var("AGENTHUB_PROCESS").as_deref() {
            Ok("cli") => RuntimeKind::Cli,
            Ok("desktop") => RuntimeKind::Desktop,
            _ => RuntimeKind::Process,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeKind::Cli => "cli",
            RuntimeKind::Desktop => "desktop",
            RuntimeKind::Process => "process",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LockMeta {
    pid: u32,
    kind: String,
    acquired_at: String,
}

/// Held for the duration of one mutating operation. Dropping the guard
/// releases the advisory lock; the lock file itself stays on disk (deleting it
/// would break mutual exclusion — see the module docs).
#[derive(Debug)]
pub struct RuntimeGuard {
    #[allow(dead_code)] // diagnostics only; kept so holders can log the path
    path: PathBuf,
    file: File,
}

/// Try to acquire the cross-process runtime lock under `data_dir`.
///
/// Returns [`Error::Blocked`] with a human-readable holder description when
/// another process is mid-mutation.
pub fn acquire(data_dir: &Path, kind: RuntimeKind) -> Result<RuntimeGuard> {
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join(LOCK_FILE_NAME);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    file.try_lock_exclusive().map_err(|_| {
        let holder = fs::read_to_string(&path)
            .ok()
            .and_then(|body| serde_json::from_str::<LockMeta>(&body).ok())
            .map(|meta| format!("{} (pid {})", meta.kind, meta.pid))
            .unwrap_or_else(|| "another AgentHub process".to_string());
        Error::blocked(format!(
            "AgentHub runtime is busy: a mutating operation is in progress by {holder}; \
             retry once it finishes"
        ))
    })?;
    let meta = LockMeta {
        pid: std::process::id(),
        kind: kind.as_str().to_string(),
        acquired_at: crate::util::now(),
    };
    file.set_len(0)?;
    writeln!(file, "{}", serde_json::to_string(&meta)?)?;
    file.flush()?;
    Ok(RuntimeGuard { path, file })
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        // Release only the flock; keep the file. See module docs for why the
        // unlink-on-release variant is a correctness bug.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let guard = acquire(dir.path(), RuntimeKind::Cli).expect("first acquire must succeed");
        assert!(dir.path().join(LOCK_FILE_NAME).exists());
        drop(guard);
        // The lock file is intentionally kept on disk (unlink-on-release is a
        // race; see module docs). Only the flock is released.
        assert!(dir.path().join(LOCK_FILE_NAME).exists());
    }

    #[test]
    fn second_acquire_is_busy_and_names_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = acquire(dir.path(), RuntimeKind::Desktop).unwrap();
        let err = acquire(dir.path(), RuntimeKind::Cli).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("runtime is busy"), "unexpected error: {text}");
        assert!(text.contains("desktop"), "holder kind missing: {text}");
    }

    #[test]
    fn lock_is_reacquirable_after_release() {
        let dir = tempfile::tempdir().unwrap();
        drop(acquire(dir.path(), RuntimeKind::Process).unwrap());
        let guard = acquire(dir.path(), RuntimeKind::Process);
        assert!(guard.is_ok(), "release must free the runtime");
    }
}
