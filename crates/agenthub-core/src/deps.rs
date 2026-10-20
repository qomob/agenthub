//! Environment and dependency diagnostics for skill scripts and manifests.
//!
//! Everything here is pure local inspection: binaries are looked up on `PATH`
//! and probed with `--version`, manifests are parsed as text. Install commands
//! are only *suggested* as strings; nothing is executed or installed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;
use toml_edit::DocumentMut;

use crate::agent_health::{detect_binary_version, find_binary};
use crate::audit::{discover_scripts, is_manifest_file_name, read_text_limited, MAX_SCAN_BYTES};
use crate::model::DependencyItem;

const REQUIREMENTS_FILE: &str = "requirements.txt";
const PYPROJECT_FILE: &str = "pyproject.toml";
const PACKAGE_JSON_FILE: &str = "package.json";

#[derive(Debug, Clone)]
pub struct DepsResult {
    /// healthy | needs_attention | unknown
    pub status: String,
    pub risk: String,
    pub summary: String,
    pub items: Vec<DependencyItem>,
}

#[derive(Debug, Clone, Default)]
struct RuntimeUse {
    scripts: Vec<String>,
    required_version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PyProjectInfo {
    pub name: Option<String>,
    pub version: Option<String>,
    pub dependency_names: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PackageJsonInfo {
    pub name: Option<String>,
    pub engines_node: Option<String>,
    pub dependency_names: BTreeSet<String>,
    pub dependency_count: usize,
    pub dev_dependency_count: usize,
}

pub fn check_skill_dependencies(root: &Path) -> DepsResult {
    let mut items: BTreeMap<(String, String), DependencyItem> = BTreeMap::new();
    let scripts = discover_scripts(root);

    let mut runtimes: BTreeMap<String, RuntimeUse> = BTreeMap::new();
    for script in &scripts {
        let Some(shebang) = script.shebang.as_ref() else {
            continue;
        };
        let usage = runtimes.entry(shebang.name.clone()).or_default();
        if !usage.scripts.contains(&script.rel) {
            usage.scripts.push(script.rel.clone());
        }
        if usage.required_version.is_none() {
            usage.required_version = shebang.required_version.clone();
        }
    }
    for (name, usage) in &runtimes {
        let item = runtime_item(
            name,
            usage.required_version.clone(),
            usage.scripts.first().cloned(),
            &usage.scripts,
            true,
        );
        insert_item(&mut items, item);
    }

    let requirements_path = root.join(REQUIREMENTS_FILE);
    if requirements_path.is_file() {
        if let Some(text) = read_text_limited(&requirements_path, MAX_SCAN_BYTES) {
            for (name, required_version) in parse_requirements_text(&text) {
                insert_item(
                    &mut items,
                    DependencyItem {
                        name,
                        kind: "package".to_string(),
                        required_version,
                        found: None,
                        status: "unknown".to_string(),
                        detail: "declared, not installed by AgentHub".to_string(),
                        source_path: Some(REQUIREMENTS_FILE.to_string()),
                        suggestion: Some(
                            "Install declared packages separately (for example `pip install -r requirements.txt`) if the skill needs them."
                                .to_string(),
                        ),
                    },
                );
            }
        }
        ensure_runtime(
            &mut items,
            &runtimes,
            "python3",
            Some(REQUIREMENTS_FILE.to_string()),
            "required by requirements.txt",
            false,
        );
    }

    let pyproject_path = root.join(PYPROJECT_FILE);
    if pyproject_path.is_file() {
        let text = read_text_limited(&pyproject_path, MAX_SCAN_BYTES);
        let info = text.as_deref().and_then(parse_pyproject_info);
        let (name, required_version, detail) = match &info {
            Some(info) => (
                info.name.clone().unwrap_or_else(|| PYPROJECT_FILE.to_string()),
                info.version.clone(),
                format!(
                    "Python project manifest detected ({} declared dependencies).",
                    info.dependency_names.len()
                ),
            ),
            None => (
                PYPROJECT_FILE.to_string(),
                None,
                "Python project manifest detected (dependency list could not be parsed)."
                    .to_string(),
            ),
        };
        insert_item(
            &mut items,
            DependencyItem {
                name,
                kind: "manifest".to_string(),
                required_version,
                found: None,
                status: "ok".to_string(),
                detail,
                source_path: Some(PYPROJECT_FILE.to_string()),
                suggestion: None,
            },
        );
        ensure_runtime(
            &mut items,
            &runtimes,
            "python3",
            Some(PYPROJECT_FILE.to_string()),
            "required by pyproject.toml",
            false,
        );
    }

    let package_json_path = root.join(PACKAGE_JSON_FILE);
    if package_json_path.is_file() {
        let text = read_text_limited(&package_json_path, MAX_SCAN_BYTES);
        let info = text.as_deref().and_then(parse_package_json_info);
        let (name, required_version, detail) = match &info {
            Some(info) => (
                info.name.clone().unwrap_or_else(|| PACKAGE_JSON_FILE.to_string()),
                info.engines_node.clone(),
                format!(
                    "Node project manifest detected ({} dependencies, {} devDependencies).",
                    info.dependency_count, info.dev_dependency_count
                ),
            ),
            None => (
                PACKAGE_JSON_FILE.to_string(),
                None,
                "Node project manifest detected (dependency list could not be parsed).".to_string(),
            ),
        };
        insert_item(
            &mut items,
            DependencyItem {
                name,
                kind: "manifest".to_string(),
                required_version,
                found: None,
                status: "ok".to_string(),
                detail,
                source_path: Some(PACKAGE_JSON_FILE.to_string()),
                suggestion: None,
            },
        );
        ensure_runtime(
            &mut items,
            &runtimes,
            "node",
            Some(PACKAGE_JSON_FILE.to_string()),
            "required by package.json",
            false,
        );
    }

    if let Some(skill_md) = read_text_limited(&root.join("SKILL.md"), MAX_SCAN_BYTES) {
        if skill_md.to_ascii_lowercase().contains("mcp") {
            insert_item(
                &mut items,
                DependencyItem {
                    name: "mcp".to_string(),
                    kind: "mcp".to_string(),
                    required_version: None,
                    found: None,
                    status: "unknown".to_string(),
                    detail: "MCP dependency cannot be verified statically".to_string(),
                    source_path: Some("SKILL.md".to_string()),
                    suggestion: Some(
                        "Verify the referenced MCP server in the MCP manager before deployment."
                            .to_string(),
                    ),
                },
            );
        }
    }

    let mut items: Vec<DependencyItem> = items.into_values().collect();
    items.sort_by(|left, right| {
        kind_rank(&left.kind)
            .cmp(&kind_rank(&right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });

    let missing: Vec<&DependencyItem> = items
        .iter()
        .filter(|item| item.status == "missing")
        .collect();
    let warnings: Vec<&DependencyItem> = items
        .iter()
        .filter(|item| item.status == "warning")
        .collect();
    let checkable = items
        .iter()
        .filter(|item| matches!(item.kind.as_str(), "runtime" | "binary" | "manifest"))
        .count();
    let package_count = items.iter().filter(|item| item.kind == "package").count();

    let (status, risk, summary) = if items.is_empty() {
        (
            "unknown".to_string(),
            "low".to_string(),
            "no scripts, dependency manifests or MCP references were found; the environment cannot be verified statically".to_string(),
        )
    } else if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().take(5).map(|item| item.name.as_str()).collect();
        (
            "needs_attention".to_string(),
            "medium".to_string(),
            format!(
                "{} missing dependency item(s): {}{}; {} other check(s) passed",
                missing.len(),
                names.join(", "),
                if missing.len() > names.len() {
                    " (+more)"
                } else {
                    ""
                },
                checkable.saturating_sub(missing.len())
            ),
        )
    } else if checkable == 0 {
        (
            "unknown".to_string(),
            "low".to_string(),
            "only unverifiable MCP/package declarations were found; the environment cannot be verified statically"
                .to_string(),
        )
    } else if !warnings.is_empty() {
        let names: Vec<&str> = warnings.iter().take(5).map(|item| item.name.as_str()).collect();
        (
            "healthy".to_string(),
            "medium".to_string(),
            format!(
                "environment ready with {} warning(s): {}",
                warnings.len(),
                names.join(", ")
            ),
        )
    } else {
        (
            "healthy".to_string(),
            "low".to_string(),
            format!(
                "environment healthy: {} runtime/manifest check(s) passed; {} package(s) declared (installation not checked)",
                checkable, package_count
            ),
        )
    };

    DepsResult {
        status,
        risk,
        summary,
        items,
    }
}

fn kind_rank(kind: &str) -> u8 {
    match kind {
        "runtime" => 0,
        "binary" => 1,
        "manifest" => 2,
        "mcp" => 3,
        "package" => 4,
        _ => 5,
    }
}

fn insert_item(items: &mut BTreeMap<(String, String), DependencyItem>, item: DependencyItem) {
    let key = (item.kind.clone(), item.name.clone());
    items.entry(key).or_insert(item);
}

fn ensure_runtime(
    items: &mut BTreeMap<(String, String), DependencyItem>,
    runtimes: &BTreeMap<String, RuntimeUse>,
    name: &str,
    source: Option<String>,
    reason: &str,
    allow_alternative: bool,
) {
    if runtimes.contains_key(name) {
        return;
    }
    if items.contains_key(&("runtime".to_string(), name.to_string())) {
        return;
    }
    let item = runtime_item(name, None, source, &[], allow_alternative);
    let mut item = item;
    item.detail = format!("{reason}; {}", item.detail);
    insert_item(items, item);
}

fn runtime_item(
    name: &str,
    required_version: Option<String>,
    source: Option<String>,
    scripts: &[String],
    allow_alternative: bool,
) -> DependencyItem {
    let script_suffix = if scripts.is_empty() {
        String::new()
    } else {
        format!(
            " (used by {}{})",
            scripts.first().cloned().unwrap_or_default(),
            if scripts.len() > 1 {
                format!(" and {} more script(s)", scripts.len() - 1)
            } else {
                String::new()
            }
        )
    };

    match find_binary(name) {
        Some(path) => {
            let version = detect_binary_version(name);
            let version_display = version.clone().unwrap_or_else(|| "version unknown".to_string());
            DependencyItem {
                name: name.to_string(),
                kind: "runtime".to_string(),
                required_version,
                found: version,
                status: "ok".to_string(),
                detail: format!(
                    "found at {} ({version_display}){script_suffix}",
                    path.display()
                ),
                source_path: source,
                suggestion: None,
            }
        }
        None => {
            let alternative = if allow_alternative {
                match name {
                    "python3" => Some("python"),
                    "python" => Some("python3"),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(alternative) = alternative {
                if find_binary(alternative).is_some() {
                    let version = detect_binary_version(alternative);
                    let version_display =
                        version.clone().unwrap_or_else(|| "version unknown".to_string());
                    return DependencyItem {
                        name: name.to_string(),
                        kind: "runtime".to_string(),
                        required_version,
                        found: version,
                        status: "warning".to_string(),
                        detail: format!(
                            "scripts require '{name}', but only '{alternative}' ({version_display}) was found; shebang compatibility is not guaranteed{script_suffix}"
                        ),
                        source_path: source,
                        suggestion: Some(format!(
                            "Install '{name}' or change the shebang to '{alternative}'."
                        )),
                    };
                }
            }
            if name == "python3" {
                if let Some(python_version) = detect_binary_version("python") {
                    return DependencyItem {
                        name: name.to_string(),
                        kind: "runtime".to_string(),
                        required_version,
                        found: Some(python_version.clone()),
                        status: "missing".to_string(),
                        detail: format!(
                            "'python3' was not found on PATH (only 'python' {python_version} is available){script_suffix}"
                        ),
                        source_path: source,
                        suggestion: Some(
                            "Install Python 3 (`brew install python3`) or change the requirement to use the available `python`."
                                .to_string(),
                        ),
                    };
                }
            }
            DependencyItem {
                name: name.to_string(),
                kind: "runtime".to_string(),
                required_version,
                found: None,
                status: "missing".to_string(),
                detail: format!(
                    "not found on PATH (also checked /opt/homebrew/bin and /usr/local/bin){script_suffix}"
                ),
                source_path: source,
                suggestion: Some(install_suggestion(name)),
            }
        }
    }
}

fn install_suggestion(name: &str) -> String {
    match name {
        "python3" | "python" => {
            "Install Python 3 (for example `brew install python3`) or use a shebang that matches the installed interpreter."
                .to_string()
        }
        "node" => "Install Node.js (for example `brew install node`) and ensure it is on PATH."
            .to_string(),
        "ruby" => "Install Ruby (for example `brew install ruby`) and ensure it is on PATH."
            .to_string(),
        "uv" => "Install uv (for example `brew install uv`) and ensure it is on PATH.".to_string(),
        "bun" => "Install Bun (for example `brew install oven-sh/bun/bun`) and ensure it is on PATH."
            .to_string(),
        "deno" => "Install Deno (for example `brew install deno`) and ensure it is on PATH."
            .to_string(),
        "bash" | "sh" | "zsh" => {
            format!("The shell '{name}' is normally present on macOS; check PATH and the shebang.")
        }
        _ => format!("Install `{name}` and ensure it is on PATH."),
    }
}

// ---------------------------------------------------------------------------
// Manifest parsing (also reused by impact analysis)
// ---------------------------------------------------------------------------

/// Parse `requirements.txt` style lines into `(package, version constraint)`.
pub(crate) fn parse_requirements_text(text: &str) -> Vec<(String, Option<String>)> {
    let mut declarations = Vec::new();
    for raw in text.lines() {
        let without_comment = raw.split('#').next().unwrap_or("").trim();
        if without_comment.is_empty() || without_comment.starts_with('-') {
            continue;
        }
        if without_comment.starts_with("http://")
            || without_comment.starts_with("https://")
            || without_comment.starts_with("git+")
            || without_comment.contains("://")
        {
            continue;
        }
        if let Some(declaration) = parse_requirement_line(without_comment) {
            declarations.push(declaration);
        }
    }
    declarations
}

fn parse_requirement_line(line: &str) -> Option<(String, Option<String>)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut name_end = trimmed.len();
    for (index, ch) in trimmed.char_indices() {
        if matches!(ch, '[' | '<' | '>' | '=' | '!' | '~' | ';' | ' ' | '\t') {
            name_end = index;
            break;
        }
    }
    let name = trimmed[..name_end].trim();
    if name.is_empty() {
        return None;
    }
    let mut rest = trimmed[name_end..].trim();
    if let Some(after_open) = rest.strip_prefix('[') {
        if let Some((_, tail)) = after_open.split_once(']') {
            rest = tail.trim();
        }
    }
    let required_version = if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    };
    Some((name.to_ascii_lowercase(), required_version))
}

pub(crate) fn parse_pyproject_info(text: &str) -> Option<PyProjectInfo> {
    let doc: DocumentMut = text.parse().ok()?;
    let project = doc.get("project").and_then(|item| item.as_table_like());
    let name = project
        .and_then(|table| table.get("name"))
        .and_then(|item| item.as_str())
        .map(str::to_string);
    let version = project
        .and_then(|table| table.get("version"))
        .and_then(|item| item.as_str())
        .map(str::to_string);

    let mut dependency_names = BTreeSet::new();
    if let Some(dependencies) = project
        .and_then(|table| table.get("dependencies"))
        .and_then(|item| item.as_array())
    {
        for value in dependencies.iter() {
            if let Some(text) = value.as_str() {
                if let Some(name) = dependency_name(text) {
                    dependency_names.insert(name);
                }
            }
        }
    }
    if let Some(optional) = project
        .and_then(|table| table.get("optional-dependencies"))
        .and_then(|item| item.as_table_like())
    {
        for (_, value) in optional.iter() {
            if let Some(dependencies) = value.as_array() {
                for dependency in dependencies.iter() {
                    if let Some(text) = dependency.as_str() {
                        if let Some(name) = dependency_name(text) {
                            dependency_names.insert(name);
                        }
                    }
                }
            }
        }
    }
    // Poetry-style dependencies.
    if let Some(dependencies) = doc
        .get("tool")
        .and_then(|item| item.as_table_like())
        .and_then(|table| table.get("poetry"))
        .and_then(|item| item.as_table_like())
        .and_then(|table| table.get("dependencies"))
        .and_then(|item| item.as_table_like())
    {
        for (name, _) in dependencies.iter() {
            if !name.eq_ignore_ascii_case("python") {
                dependency_names.insert(normalize_dependency_name(name));
            }
        }
    }

    Some(PyProjectInfo {
        name,
        version,
        dependency_names,
    })
}

pub(crate) fn parse_package_json_info(text: &str) -> Option<PackageJsonInfo> {
    let root: Value = serde_json::from_str(text).ok()?;
    let object = root.as_object()?;
    let name = object.get("name").and_then(Value::as_str).map(str::to_string);
    let engines_node = object
        .get("engines")
        .and_then(Value::as_object)
        .and_then(|engines| engines.get("node"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut dependency_names = BTreeSet::new();
    let mut dependency_count = 0usize;
    if let Some(dependencies) = object.get("dependencies").and_then(Value::as_object) {
        dependency_count = dependencies.len();
        for name in dependencies.keys() {
            dependency_names.insert(normalize_dependency_name(name));
        }
    }
    let mut dev_dependency_count = 0usize;
    if let Some(dev_dependencies) = object.get("devDependencies").and_then(Value::as_object) {
        dev_dependency_count = dev_dependencies.len();
        for name in dev_dependencies.keys() {
            dependency_names.insert(normalize_dependency_name(name));
        }
    }

    Some(PackageJsonInfo {
        name,
        engines_node,
        dependency_names,
        dependency_count,
        dev_dependency_count,
    })
}

fn dependency_name(declaration: &str) -> Option<String> {
    let trimmed = declaration.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut end = trimmed.len();
    for (index, ch) in trimmed.char_indices() {
        if matches!(ch, '[' | '<' | '>' | '=' | '!' | '~' | ';' | ' ' | '\t') {
            end = index;
            break;
        }
    }
    let name = trimmed[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(normalize_dependency_name(name))
    }
}

fn normalize_dependency_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}

/// Union of dependency names declared by requirements.txt, pyproject.toml and
/// package.json in `root`. Used by update impact analysis.
pub(crate) fn manifest_dependency_names(root: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let requirements = root.join(REQUIREMENTS_FILE);
    if requirements.is_file() {
        if let Some(text) = read_text_limited(&requirements, MAX_SCAN_BYTES) {
            for (name, _) in parse_requirements_text(&text) {
                names.insert(normalize_dependency_name(&name));
            }
        }
    }
    let pyproject = root.join(PYPROJECT_FILE);
    if pyproject.is_file() {
        if let Some(text) = read_text_limited(&pyproject, MAX_SCAN_BYTES) {
            if let Some(info) = parse_pyproject_info(&text) {
                names.extend(info.dependency_names);
            }
        }
    }
    let package_json = root.join(PACKAGE_JSON_FILE);
    if package_json.is_file() {
        if let Some(text) = read_text_limited(&package_json, MAX_SCAN_BYTES) {
            if let Some(info) = parse_package_json_info(&text) {
                names.extend(info.dependency_names);
            }
        }
    }
    names
}

/// Manifest file names present in a directory (used to detect newly declared
/// dependency surfaces during an update).
pub(crate) fn manifest_file_names(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_manifest_file_name(&name) {
            found.insert(name);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn make_skill(root: &Path, body: &str) {
        write_file(
            &root.join("SKILL.md"),
            &format!(
                "---\nname: deps-test\ndescription: A deterministic test skill used to exercise dependency diagnostics.\nlicense: MIT\n---\n\n{body}\n"
            ),
        );
    }

    fn make_script(root: &Path, rel: &str, contents: &str) -> PathBuf {
        let path = root.join(rel);
        write_file(&path, contents);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    fn item<'a>(result: &'a DepsResult, kind: &str, name: &str) -> &'a DependencyItem {
        result
            .items
            .iter()
            .find(|item| item.kind == kind && item.name == name)
            .unwrap_or_else(|| panic!("expected item {kind}:{name}, got {:?}", result.items))
    }

    #[test]
    fn missing_runtime_marks_needs_attention() {
        let dir = tempfile::tempdir().unwrap();
        make_skill(dir.path(), "Runs a fake runtime.");
        make_script(
            dir.path(),
            "scripts/run.xyz",
            "#!/usr/bin/env agenthub-missing-runtime-xyz\nprint('hi')\n",
        );

        let result = check_skill_dependencies(dir.path());
        assert_eq!(result.status, "needs_attention");
        let runtime = item(&result, "runtime", "agenthub-missing-runtime-xyz");
        assert_eq!(runtime.status, "missing");
        assert!(runtime.found.is_none());
        assert!(runtime.suggestion.is_some());
        assert!(runtime.detail.contains("not found on PATH"));
    }

    #[test]
    fn requirements_are_declared_but_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        make_skill(dir.path(), "Uses Python packages.");
        write_file(
            &dir.path().join("requirements.txt"),
            "# direct dependencies\nrequests>=2.31\nflask==3.0.0  # web server\n-r other.txt\n",
        );

        let result = check_skill_dependencies(dir.path());
        let requests = item(&result, "package", "requests");
        assert_eq!(requests.status, "unknown");
        assert_eq!(requests.required_version.as_deref(), Some(">=2.31"));
        assert!(requests.detail.contains("declared, not installed by AgentHub"));
        assert_eq!(
            requests.source_path.as_deref(),
            Some(REQUIREMENTS_FILE)
        );

        let flask = item(&result, "package", "flask");
        assert_eq!(flask.required_version.as_deref(), Some("==3.0.0"));
        // requirements.txt implies a Python runtime probe.
        assert!(result
            .items
            .iter()
            .any(|item| item.kind == "runtime" && item.name == "python3"));
    }

    #[test]
    fn pyproject_manifest_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        make_skill(dir.path(), "Python project.");
        write_file(
            &dir.path().join("pyproject.toml"),
            r#"[project]
name = "fixture-skill"
version = "1.2.3"
dependencies = ["requests>=2.0", "click"]
"#,
        );

        let result = check_skill_dependencies(dir.path());
        let manifest = item(&result, "manifest", "fixture-skill");
        assert_eq!(manifest.status, "ok");
        assert_eq!(manifest.required_version.as_deref(), Some("1.2.3"));
        assert!(manifest.detail.contains("2 declared dependencies"));
        assert!(result
            .items
            .iter()
            .any(|item| item.kind == "runtime" && item.name == "python3"));
    }

    #[test]
    fn package_json_manifest_counts_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        make_skill(dir.path(), "Node project.");
        write_file(
            &dir.path().join("package.json"),
            r#"{"name":"node-fixture","version":"0.5.0","dependencies":{"left-pad":"1.3.0","undici":"6.0.0"},"devDependencies":{"typescript":"5.0.0"},"engines":{"node":">=20"}}"#,
        );

        let result = check_skill_dependencies(dir.path());
        let manifest = item(&result, "manifest", "node-fixture");
        assert_eq!(manifest.status, "ok");
        assert!(manifest.detail.contains("2 dependencies"));
        assert!(manifest.detail.contains("1 devDependencies"));
        assert_eq!(manifest.required_version.as_deref(), Some(">=20"));
        assert!(result
            .items
            .iter()
            .any(|item| item.kind == "runtime" && item.name == "node"));
    }

    #[test]
    fn mcp_mention_is_unknown_without_false_positive() {
        let dir = tempfile::tempdir().unwrap();
        make_skill(dir.path(), "Calls an MCP server for data.");

        let result = check_skill_dependencies(dir.path());
        let mcp = item(&result, "mcp", "mcp");
        assert_eq!(mcp.status, "unknown");
        assert!(mcp.detail.contains("cannot be verified statically"));
        assert_eq!(result.status, "unknown");
        assert!(result.summary.contains("MCP"));
        // No fabricated missing binaries.
        assert!(result.items.iter().all(|item| item.status != "missing"));
    }

    #[test]
    fn empty_skill_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        make_skill(dir.path(), "Nothing to check.");
        let result = check_skill_dependencies(dir.path());
        assert_eq!(result.status, "unknown");
        assert!(result.items.is_empty());
        assert!(result.summary.contains("no scripts"));
    }

    #[test]
    fn parses_requirement_constraints_and_extras() {
        let declarations = parse_requirements_text(
            "requests[socks]>=2.0,<3\nflask==1.0\n# comment\n-e .\nhttps://example.com/pkg.whl\n",
        );
        assert_eq!(
            declarations,
            vec![
                ("requests".to_string(), Some(">=2.0,<3".to_string())),
                ("flask".to_string(), Some("==1.0".to_string())),
            ]
        );
    }

    #[test]
    fn manifest_dependency_names_are_normalized() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("requirements.txt"), "Pillow>=10\n");
        write_file(
            &dir.path().join("package.json"),
            r#"{"dependencies":{"left_pad":"1.0.0"}}"#,
        );
        let names = manifest_dependency_names(dir.path());
        assert!(names.contains("pillow"));
        assert!(names.contains("left-pad"));
    }
}
