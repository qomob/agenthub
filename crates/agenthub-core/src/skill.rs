//! SKILL.md parsing, validation, directory discovery and safe import staging.
//!
//! A skill is a directory containing a `SKILL.md` with YAML frontmatter. The
//! frontmatter requires `name` and `description`; `version`, `license`,
//! `allowed-tools`, `tags` and `metadata` are optional. Unknown fields are
//! preserved in the stored manifest.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::model::FileEntry;
use crate::util;

pub const MAX_SKILL_MD_BYTES: u64 = 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: usize = 4000;

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub license: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedSkill {
    pub path: PathBuf,
    pub frontmatter: Frontmatter,
    pub manifest: Value,
    pub skill_md: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub files: Vec<FileEntry>,
    /// Non-fatal validation findings.
    pub issues: Vec<String>,
}

impl ParsedSkill {
    pub fn slug(&self) -> String {
        util::safe_segment(&self.frontmatter.name)
    }

    pub fn version_label(&self) -> String {
        self.frontmatter
            .version
            .clone()
            .unwrap_or_else(|| format!("0.0.0+{}", util::short_hash(&self.content_hash)))
    }
}

/// Parse the YAML frontmatter block of a SKILL.md file.
pub fn parse_frontmatter(text: &str) -> Result<(Value, String)> {
    let stripped = text.trim_start_matches('\u{feff}');
    if !stripped.starts_with("---") {
        return Err(Error::invalid(
            "SKILL.md must start with a YAML frontmatter block delimited by '---'",
        ));
    }
    let rest = &stripped[3..];
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let mut offset = 0usize;
    let mut found = false;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.trim() == "---" {
            found = true;
            break;
        }
        offset += line.len();
    }
    if !found {
        return Err(Error::invalid(
            "SKILL.md frontmatter is missing the closing '---' delimiter",
        ));
    }
    let yaml = &rest[..offset];
    let body = &rest[offset..];
    let body = body
        .strip_prefix("---")
        .unwrap_or(body)
        .trim_start_matches(['\r', '\n']);
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|err| Error::invalid(format!("invalid YAML frontmatter: {err}")))?;
    let json: Value = serde_json::to_value(yaml_value)?;
    Ok((json, body.to_string()))
}

/// Parse and validate a skill directory.
pub fn read_skill_dir(path: &Path) -> Result<ParsedSkill> {
    let skill_md_path = path.join("SKILL.md");
    let meta = fs::metadata(&skill_md_path)
        .map_err(|_| Error::invalid(format!("{} has no SKILL.md", path.display())))?;
    if !meta.is_file() {
        return Err(Error::invalid(format!(
            "{} is not a regular file",
            skill_md_path.display()
        )));
    }
    if meta.len() > MAX_SKILL_MD_BYTES {
        return Err(Error::invalid(format!(
            "SKILL.md is larger than {} bytes",
            MAX_SKILL_MD_BYTES
        )));
    }
    let text = fs::read_to_string(&skill_md_path)
        .map_err(|err| Error::invalid(format!("cannot read SKILL.md: {err}")))?;
    let (manifest, _body) = parse_frontmatter(&text)?;

    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::invalid("frontmatter is missing a non-empty 'name'"))?
        .to_string();
    let description = manifest
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::invalid("frontmatter is missing a non-empty 'description'"))?
        .to_string();

    let version = manifest
        .get("version")
        .map(value_to_string)
        .or_else(|| {
            manifest
                .get("metadata")
                .and_then(|meta| meta.get("version"))
                .map(value_to_string)
        })
        .flatten();
    let license = manifest.get("license").map(value_to_string).flatten();
    let tags = manifest
        .get("tags")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut issues = Vec::new();
    if name.len() > 64 {
        issues.push("name is longer than 64 characters (Agent Skills guideline)".to_string());
    }
    if description.len() > 1024 {
        issues.push("description is longer than 1024 characters".to_string());
    }
    if util::safe_segment(&name) != name.to_ascii_lowercase() {
        issues.push(format!(
            "name '{}' is not a strict slug; AgentHub uses '{}' for the library path",
            name,
            util::safe_segment(&name)
        ));
    }
    if text.len() > 100 * 1024 {
        issues.push("SKILL.md is larger than 100KB; agents may truncate it".to_string());
    }
    if let Some(dir_name) = path.file_name().map(|name| name.to_string_lossy().to_string()) {
        if dir_name != util::safe_segment(&name) && dir_name != name {
            issues.push(format!(
                "Directory name '{}' differs from frontmatter name '{}' (the spec requires them to match; AgentHub deploys using the normalized name).",
                dir_name, name
            ));
        }
    }

    if let Ok(entries) = fs::read_dir(path) {
        let _ = entries;
    }
    for link in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_symlink())
    {
        if let Ok(target) = fs::read_link(link.path()) {
            let resolved = if target.is_absolute() {
                util::normalize(&target)
            } else {
                util::normalize(&link.path().parent().unwrap_or(path).join(&target))
            };
            let root_norm = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if !resolved.starts_with(&root_norm) {
                issues.push(format!(
                    "Contains a symlink pointing outside the skill directory ({}); it is preserved as-is and never followed by AgentHub.",
                    link.path()
                        .strip_prefix(path)
                        .unwrap_or(link.path())
                        .display()
                ));
            }
        }
    }
    let (size_bytes, file_count, files) = collect_files(path)?;
    let content_hash = util::hash_dir(path)?;
    let frontmatter = Frontmatter {
        name,
        description,
        version,
        license,
        tags,
    };
    Ok(ParsedSkill {
        path: path.to_path_buf(),
        frontmatter,
        manifest,
        skill_md: text,
        content_hash,
        size_bytes,
        file_count,
        files,
        issues,
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn collect_files(root: &Path) -> Result<(i64, i64, Vec<FileEntry>)> {
    let mut size = 0i64;
    let mut count = 0i64;
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.path() == root || !util::is_ignored_path(entry.path()))
    {
        let entry = entry.map_err(|err| Error::msg(format!("walk error: {err}")))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        size += bytes;
        count += 1;
        files.push(FileEntry {
            path: rel,
            size_bytes: bytes,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((size, count, files))
}

/// Find skill directories directly under `root`.
///
/// A directory qualifies only when it contains a `SKILL.md`; arbitrary markdown
/// files are never treated as skills. Hidden directories and `node_modules` are
/// skipped. When `root` itself contains a `SKILL.md` it is treated as a single
/// skill directory.
pub fn find_skill_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.join("SKILL.md").is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let is_dir_like = entry
            .file_type()
            .map(|kind| kind.is_dir() || kind.is_symlink())
            .unwrap_or(false);
        if is_dir_like {
            children.push(entry.path());
        }
    }
    children.sort();
    for child in children {
        if child.join("SKILL.md").is_file() {
            out.push(child);
        }
    }
    out
}

/// Walk a staging directory (bounded depth) to locate skill directories.
pub fn find_skill_dirs_recursive(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.join("SKILL.md").is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_dir() && entry.path().join("SKILL.md").is_file() {
            let depth = entry.path().components().count() - root.components().count();
            let is_nested_skill = entry
                .path()
                .ancestors()
                .skip(1)
                .take_while(|ancestor| *ancestor != root)
                .any(|ancestor| ancestor.join("SKILL.md").is_file());
            if !is_nested_skill && depth <= max_depth {
                out.push(entry.path().to_path_buf());
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Staging (safe import)
// ---------------------------------------------------------------------------

pub struct StagingDir {
    path: PathBuf,
    keep: bool,
}

impl StagingDir {
    pub fn create(base: &Path) -> Result<Self> {
        fs::create_dir_all(base)?;
        let path = base.join(format!("staging-{}", util::new_id()));
        fs::create_dir_all(&path)?;
        Ok(StagingDir { path, keep: false })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = util::remove_path(&self.path);
        }
    }
}

/// Extract a ZIP archive with path-traversal, symlink, size and entry-count
/// protections. Returns the directory that contains the extracted content.
pub fn extract_archive(archive: &Path, staging: &Path) -> Result<PathBuf> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|err| Error::Zip(err.to_string()))?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(Error::blocked(format!(
            "archive has {} entries, exceeding the {} entry limit",
            zip.len(),
            MAX_ARCHIVE_ENTRIES
        )));
    }
    let destination = staging.join("archive");
    fs::create_dir_all(&destination)?;
    let mut total: u64 = 0;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| Error::Zip(err.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if name.contains('\0') {
            return Err(Error::blocked(format!(
                "archive entry contains a NUL byte: {name}"
            )));
        }
        if entry
            .unix_mode()
            .map(|mode| mode & 0o170000 == 0o120000)
            .unwrap_or(false)
        {
            return Err(Error::blocked(format!(
                "archive entry is a symlink, refusing to extract: {name}"
            )));
        }
        let relative = safe_relative_path(&name)?;
        let target = destination.join(&relative);
        if !util::normalize(&target).starts_with(&destination) {
            return Err(Error::blocked(format!(
                "archive entry escapes extraction root: {name}"
            )));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&target)?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = entry.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > MAX_ARCHIVE_BYTES {
                return Err(Error::blocked(format!(
                    "archive expands beyond {} bytes",
                    MAX_ARCHIVE_BYTES
                )));
            }
            std::io::Write::write_all(&mut out, &buffer[..read])?;
        }
    }
    Ok(destination)
}

fn safe_relative_path(name: &str) -> Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.starts_with("//")
        || normalized
            .chars()
            .nth(1)
            .map(|ch| ch == ':')
            .unwrap_or(false)
    {
        return Err(Error::blocked(format!(
            "archive entry uses an absolute path: {name}"
        )));
    }
    let mut path = PathBuf::new();
    for component in normalized.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return Err(Error::blocked(format!(
                "archive entry contains path traversal: {name}"
            )));
        }
        if component == "__MACOSX" {
            continue;
        }
        path.push(component);
    }
    if path.as_os_str().is_empty() {
        return Err(Error::blocked(format!("archive entry has empty path: {name}")));
    }
    Ok(path)
}

/// Clone a git repository into staging (data only, never execute its scripts).
pub fn clone_git(url: &str, reference: Option<&str>, staging: &Path) -> Result<PathBuf> {
    if !(url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git@")
        || url.starts_with("file://")
        || url.starts_with('/'))
    {
        return Err(Error::blocked(
            "only https/ssh/git/file URLs or absolute local paths are accepted for git import",
        ));
    }
    let destination = staging.join("repo");
    let mut command = std::process::Command::new("git");
    command
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("--quiet")
        .arg("--no-tags");
    if let Some(reference) = reference {
        if !reference.is_empty() {
            command.arg("--branch").arg(reference);
        }
    }
    command
        .arg(url)
        .arg(&destination)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "true");
    let output = command
        .output()
        .map_err(|err| Error::unsupported(format!("git is not available: {err}")))?;
    if !output.status.success() {
        return Err(Error::msg(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    // Never import the .git directory payload: it is metadata, not skill content.
    let _ = util::remove_path(&destination.join(".git"));
    Ok(destination)
}

/// Compute a skill hash without parsing (used for drift detection).
pub fn skill_hash(path: &Path) -> Option<String> {
    if !path.join("SKILL.md").is_file() {
        return None;
    }
    util::hash_dir(path).ok()
}
