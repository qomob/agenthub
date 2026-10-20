//! Shared filesystem fixtures for the agenthub-core integration tests.
//!
//! Every test gets an isolated temporary "home" directory and a separate
//! "project" directory. Nothing in this module ever touches the real user home:
//! `CoreConfig::for_home` is always given the fixture home, and agent adapters
//! resolve `~/.<agent>` relative to it.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};

pub struct Fixture {
    _temp: tempfile::TempDir,
    pub home: PathBuf,
    pub project: PathBuf,
}

impl Fixture {
    /// Home with Codex and WorkBuddy directories (both detected as installed)
    /// plus an empty project directory.
    pub fn new() -> Self {
        Self::with_agents(&[".codex", ".workbuddy"])
    }

    /// `agent_dirs` are top-level directories created inside the fake home,
    /// which is how the built-in adapters detect an installed agent.
    pub fn with_agents(agent_dirs: &[&str]) -> Self {
        let temp = tempfile::tempdir().expect("create temp dir");
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("create fake home");
        for dir in agent_dirs {
            fs::create_dir_all(home.join(dir)).expect("create fake agent dir");
        }
        let project = temp.path().join("project");
        fs::create_dir_all(&project).expect("create project dir");
        Fixture {
            _temp: temp,
            home,
            project,
        }
    }

    pub fn hub(&self) -> AgentHub {
        AgentHub::open(CoreConfig::for_home(&self.home)).expect("open isolated AgentHub")
    }

    pub fn codex_skills(&self) -> PathBuf {
        self.home.join(".codex/skills")
    }

    pub fn workbuddy_skills(&self) -> PathBuf {
        self.home.join(".workbuddy/skills")
    }

    pub fn codex_config(&self) -> PathBuf {
        self.home.join(".codex/config.toml")
    }

    pub fn workbuddy_mcp(&self) -> PathBuf {
        self.home.join(".workbuddy/connectors/default/mcp.json")
    }
}

/// Write a skill folder `<parent>/<name>/SKILL.md` with valid frontmatter plus a
/// mutable body so tests can produce different content hashes.
pub fn write_skill(parent: &Path, name: &str, description: &str, body: &str) -> PathBuf {
    let folder = parent.join(name);
    fs::create_dir_all(&folder).expect("create skill folder");
    fs::write(folder.join("SKILL.md"), skill_md(name, description, body)).expect("write SKILL.md");
    folder
}

pub fn skill_md(name: &str, description: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\nversion: 1.0.0\n---\n\n# {name}\n\n{body}\n")
}

/// Write a standalone SKILL.md file (single-file import).
pub fn write_skill_md_file(path: &Path, name: &str, description: &str, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create SKILL.md parent");
    }
    fs::write(path, skill_md(name, description, body)).expect("write SKILL.md");
}

/// Overwrite the body of an already written skill folder, producing a new hash.
pub fn rewrite_skill(folder: &Path, name: &str, description: &str, body: &str) {
    fs::write(folder.join("SKILL.md"), skill_md(name, description, body)).expect("rewrite SKILL.md");
}

pub fn agent_id(hub: &AgentHub, key: &str) -> String {
    hub.list_agents()
        .expect("list agents")
        .into_iter()
        .find(|agent| agent.key == key)
        .unwrap_or_else(|| panic!("agent '{key}' was not registered"))
        .id
}

pub fn agent_ids(hub: &AgentHub, keys: &[&str]) -> Vec<String> {
    keys.iter().map(|key| agent_id(hub, key)).collect()
}

pub fn skill_by_slug(hub: &AgentHub, slug: &str) -> Skill {
    hub.list_skills()
        .expect("list skills")
        .into_iter()
        .find(|skill| skill.slug == slug)
        .unwrap_or_else(|| panic!("skill '{slug}' is not in the library"))
}

pub fn folder_import(path: &Path) -> ImportRequest {
    ImportRequest {
        source: ImportSource::Folder {
            path: path.to_string_lossy().to_string(),
        },
        skill_path: None,
        version_label: None,
        notes: None,
    }
}

pub fn install_request(
    capability_id: &str,
    agent_ids: Vec<String>,
    strategy: InstallStrategy,
) -> InstallRequest {
    InstallRequest {
        capability_id: capability_id.to_string(),
        version_id: None,
        agent_ids,
        strategy: Some(strategy),
        force: false,
    }
}

pub fn dir_hash(path: &Path) -> String {
    agenthub_core::util::hash_dir(path).expect("hash directory")
}

pub fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// Recursively look for a file with the given name; used to assert that
/// archive extraction never wrote outside its sandbox.
pub fn find_file_named(root: &Path, name: &str) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name() {
                if file_name == name {
                    return Some(path);
                }
            }
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push(path);
            }
        }
    }
    None
}

pub fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, contents).expect("write fixture file");
}
