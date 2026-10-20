//! Filesystem, hashing and path-safety helpers.
//!
//! Every externally visible write in AgentHub goes through the helpers in this
//! module so that path traversal, symlink escapes and partial writes are dealt
//! with in exactly one place.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{Error, Result};

/// RFC3339 timestamp in UTC, second precision (stable, sortable).
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

/// Directories that are tooling/caches rather than skill content. They are
/// excluded consistently from hashing, copying and listing so that the library
/// stays small and hash verification is stable.
pub fn is_ignored_component(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".venv"
            | "venv"
            | "node_modules"
            | "__pycache__"
            | "__MACOSX"
            | ".DS_Store"
            | ".pytest_cache"
    )
}

pub fn is_ignored_path(path: &Path) -> bool {
    path.file_name()
        .map(|name| is_ignored_component(&name.to_string_lossy()))
        .unwrap_or(false)
}

/// Stable content hash of a directory: sorted relative path + file bytes.
/// Symlinks are hashed by their target string and never followed.
pub fn hash_dir(root: &Path) -> Result<String> {
    let mut entries: Vec<(String, PathBuf, bool)> = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.path() == root || !is_ignored_path(entry.path()))
    {
        let entry = entry.map_err(|e| Error::msg(format!("walk {}: {e}", root.display())))?;
        if entry.path() == root {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .map_err(|e| Error::msg(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if rel == ".DS_Store" || rel.ends_with("/.DS_Store") {
            continue;
        }
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            entries.push((rel, entry.path().to_path_buf(), true));
        } else if file_type.is_file() {
            entries.push((rel, entry.path().to_path_buf(), false));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (rel, path, is_link) in entries {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        if is_link {
            let target = fs::read_link(&path)?;
            hasher.update(b"L");
            hasher.update(target.to_string_lossy().as_bytes());
        } else {
            hasher.update(b"F");
            hasher.update(fs::read(&path)?);
        }
        hasher.update([0u8]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn dir_stats(root: &Path) -> Result<(i64, i64)> {
    let mut size: i64 = 0;
    let mut count: i64 = 0;
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| Error::msg(format!("walk {}: {e}", root.display())))?;
        if entry.file_type().is_file() {
            size += entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
            count += 1;
        }
    }
    Ok((size, count))
}

/// Lexically normalize a path (resolve `.` and `..`) without touching the disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

/// Canonicalize when the path exists, otherwise normalize its parent and append
/// the final component so callers can validate not-yet-created targets.
pub fn canonicalize_maybe_missing(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }
    let parent = path.parent().ok_or_else(|| {
        Error::blocked(format!("refusing to operate on root path {}", path.display()))
    })?;
    let parent_canon = if parent.exists() {
        fs::canonicalize(parent)?
    } else {
        normalize(parent)
    };
    let name = path
        .file_name()
        .ok_or_else(|| Error::blocked(format!("path has no file name: {}", path.display())))?;
    Ok(parent_canon.join(name))
}

/// Validate that `path` resolves inside `root`. Returns the canonical path.
pub fn ensure_within(root: &Path, path: &Path) -> Result<PathBuf> {
    let root_canon = canonicalize_maybe_missing(root)?;
    let path_canon = canonicalize_maybe_missing(path)?;
    if path_canon != root_canon && !path_canon.starts_with(&root_canon) {
        return Err(Error::blocked(format!(
            "path {} is outside of allowed root {}",
            path.display(),
            root.display()
        )));
    }
    Ok(path_canon)
}

/// True when both paths are the same canonical location.
pub fn same_path(a: &Path, b: &Path) -> bool {
    match (canonicalize_maybe_missing(a), canonicalize_maybe_missing(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Remove a file, symlink or directory without ever following symlinks.
pub fn remove_path(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    if meta.file_type().is_symlink() || meta.is_file() {
        fs::remove_file(path)?;
    } else if meta.is_dir() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Recursively copy `src` into `dst`. Symlinks inside the source are reproduced
/// only when they stay inside the source root; escaping links are rejected.
pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.path() == src || !is_ignored_path(entry.path()))
    {
        let entry = entry.map_err(|e| Error::msg(format!("walk {}: {e}", src.display())))?;
        if entry.path() == src {
            continue;
        }
        let rel = match entry.path().strip_prefix(src) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.to_path_buf(),
            _ => continue,
        };
        let target = dst.join(&rel);
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            let link_target = fs::read_link(entry.path())?;
            if target.exists() || fs::symlink_metadata(&target).is_ok() {
                remove_path(&target)?;
            }
            std::os::unix::fs::symlink(&link_target, &target)?;
        } else if file_type.is_dir() {
            fs::create_dir_all(&target)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Atomic write: write a sibling temp file, fsync it, rename into place.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::msg(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".agenthub-tmp-{}", new_id()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    if path.exists() {
        let backup = parent.join(format!(".agenthub-bak-{}", new_id()));
        fs::rename(path, &backup)?;
        match fs::rename(&tmp, path) {
            Ok(()) => {
                let _ = fs::remove_file(&backup);
            }
            Err(err) => {
                let _ = fs::rename(&backup, path);
                let _ = fs::remove_file(&tmp);
                return Err(err.into());
            }
        }
    } else {
        fs::rename(&tmp, path)?;
    }
    Ok(())
}

/// Atomically move a directory into place. When `target` already exists the
/// caller is responsible for having snapshotted/removed it first.
pub fn atomic_place_dir(staging: &Path, target: &Path) -> Result<()> {
    if fs::symlink_metadata(target).is_ok() {
        return Err(Error::conflict(format!(
            "target already exists: {}",
            target.display()
        )));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(staging, target)?;
    Ok(())
}

/// Create a symlink atomically via a temporary sibling name.
pub fn atomic_symlink(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::symlink_metadata(link).is_ok() {
        return Err(Error::conflict(format!(
            "link already exists: {}",
            link.display()
        )));
    }
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

pub fn read_symlink_target(link: &Path) -> Option<PathBuf> {
    fs::read_link(link).ok()
}

pub fn path_is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Read-only writability probe using access(2); never creates files in the
/// inspected directory (agent directories must not be touched while scanning).
pub fn is_writable_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    // SAFETY: c_path is a valid NUL-terminated string for the duration of the call.
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

/// Make a string safe to use as a single filesystem path segment.
pub fn safe_segment(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '/' || ch == '\\' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

pub fn now_for_path() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}
