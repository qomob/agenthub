//! Agent version detection and health signals.
//!
//! Closed desktop agents do not expose a test interface; AgentHub only records
//! what the local app bundle / CLI actually reports and never fabricates a
//! version or an "OK" state.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Hard timeout for version probes. A misbehaving CLI must never hang a scan.
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

/// Directories probed in addition to `PATH` (Apple Silicon + Intel Homebrew).
const EXTRA_BIN_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Read the short version from a macOS `.app` bundle.
///
/// Order of evidence: `plutil` on `Contents/Info.plist`, then a plain XML parse
/// of the same file, then Spotlight metadata (`mdls`). Returns `None` when the
/// bundle does not exist or no real version is reported. Never guesses.
pub fn detect_app_version(app_path: &Path) -> Option<String> {
    let plist = app_info_plist(app_path)?;
    if !plist.is_file() {
        return None;
    }

    if let Some(version) = plutil_short_version(&plist) {
        return Some(version);
    }
    if let Ok(text) = fs::read_to_string(&plist) {
        if let Some(version) = version_from_plist_xml(&text) {
            return Some(version);
        }
    }
    mdls_version(app_path)
}

/// Locate an executable by name, or accept an explicit path containing `/`.
///
/// Search order: each `PATH` entry, then `/opt/homebrew/bin`, then
/// `/usr/local/bin`. Only regular executable files are returned.
pub fn find_binary(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let path = PathBuf::from(name);
        return is_executable_file(&path).then_some(path);
    }

    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    dirs.extend(EXTRA_BIN_DIRS.iter().map(PathBuf::from));

    let mut seen: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if dir.as_os_str().is_empty() || seen.iter().any(|known| known == &dir) {
            continue;
        }
        seen.push(dir.clone());
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Detect the version reported by a local binary (`--version`) or app bundle.
pub fn detect_binary_version(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let as_path = PathBuf::from(trimmed);
    if is_app_bundle_path(&as_path) {
        return detect_app_version(&as_path);
    }
    let resolved = find_binary(trimmed)?;
    detect_binary_version_at(&resolved)
}

/// Version probe for an already-resolved executable path.
///
/// Kept separate from the public name-based lookup so tests never mutate the
/// process-wide `PATH`. Runs the process with a hard 2 second timeout and reads
/// both stdout and stderr; a non-zero exit is tolerated when a version was
/// still printed.
pub(crate) fn detect_binary_version_at(path: &Path) -> Option<String> {
    if !is_executable_file(path) {
        return None;
    }
    let output = run_with_timeout(path, &["--version"], VERSION_TIMEOUT)?;
    version_from_output(&output)
}

fn app_info_plist(app_path: &Path) -> Option<PathBuf> {
    let name = app_path.file_name()?.to_string_lossy();
    if name == "Info.plist" {
        return Some(app_path.to_path_buf());
    }
    Some(app_path.join("Contents").join("Info.plist"))
}

fn is_app_bundle_path(path: &Path) -> bool {
    if path.extension().map(|ext| ext == "app").unwrap_or(false) {
        return true;
    }
    let is_plist = path
        .file_name()
        .map(|name| name == "Info.plist")
        .unwrap_or(false);
    is_plist
        && path
            .components()
            .any(|component| component.as_os_str().to_string_lossy().ends_with(".app"))
}

fn plutil_short_version(plist: &Path) -> Option<String> {
    let output = run_with_timeout(
        Path::new("plutil"),
        &[
            "-extract",
            "CFBundleShortVersionString",
            "raw",
            "-o",
            "-",
            &plist.to_string_lossy(),
        ],
        VERSION_TIMEOUT,
    )?;
    if !output.status.success() {
        return None;
    }
    clean_version(&String::from_utf8_lossy(&output.stdout))
}

/// Fallback XML parse for environments where `plutil` is unavailable.
pub(crate) fn version_from_plist_xml(text: &str) -> Option<String> {
    const KEY: &str = "CFBundleShortVersionString";
    let key_marker = format!("<key>{KEY}</key>");
    let key_pos = text.find(&key_marker)?;
    let after_key = &text[key_pos + key_marker.len()..];
    let string_start = after_key.find("<string>")? + "<string>".len();
    let rest = &after_key[string_start..];
    let string_end = rest.find("</string>")?;
    clean_version(&rest[..string_end])
}

fn mdls_version(app_path: &Path) -> Option<String> {
    let output = run_with_timeout(
        Path::new("mdls"),
        &[
            "-name",
            "kMDItemVersion",
            "-raw",
            &app_path.to_string_lossy(),
        ],
        VERSION_TIMEOUT,
    )?;
    let raw = String::from_utf8_lossy(&output.stdout);
    let cleaned = raw.trim().trim_matches('"').trim();
    if cleaned.is_empty() || cleaned == "(null)" || !cleaned.chars().any(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn clean_version(raw: &str) -> Option<String> {
    let cleaned = raw.trim().trim_matches('"').trim();
    if cleaned.is_empty() || !cleaned.chars().any(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn version_from_output(output: &Output) -> Option<String> {
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.lines().find_map(version_token)
}

/// Extract the first version-looking token from a single line.
fn version_token(line: &str) -> Option<String> {
    for raw in line.split_whitespace() {
        let word = raw.trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+' && c != '_'
        });
        if word.is_empty() {
            continue;
        }
        let candidate = word
            .strip_prefix('v')
            .or_else(|| word.strip_prefix('V'))
            .unwrap_or(word);
        let starts_with_digit = candidate
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
        if !starts_with_digit {
            continue;
        }
        if candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+' || c == '_')
        {
            return Some(candidate.to_string());
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(_) => return false,
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Spawn a probe with a hard deadline; returns `None` on spawn failure or timeout.
fn run_with_timeout(program: &Path, args: &[&str], timeout: Duration) -> Option<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    child.wait_with_output().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn make_executable(path: &Path) {
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).unwrap();
        }
    }

    #[test]
    fn parses_version_from_info_plist_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Fake Agent</string>
    <key>CFBundleShortVersionString</key>
    <string>2.3.4</string>
    <key>CFBundleVersion</key>
    <string>99</string>
</dict>
</plist>
"#;
        assert_eq!(version_from_plist_xml(xml), Some("2.3.4".to_string()));
        assert_eq!(version_from_plist_xml("<dict></dict>"), None);
    }

    #[test]
    fn detect_app_version_reads_fake_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Fake Agent.app");
        let plist = app.join("Contents").join("Info.plist");
        write_file(
            &plist,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>CFBundleShortVersionString</key>
    <string>7.8.9</string>
</dict>
</plist>
"#,
        );
        assert_eq!(detect_app_version(&app), Some("7.8.9".to_string()));
    }

    #[test]
    fn detect_app_version_returns_none_without_plist() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Missing.app");
        fs::create_dir_all(app.join("Contents")).unwrap();
        assert_eq!(detect_app_version(&app), None);
    }

    #[test]
    fn find_binary_accepts_absolute_executable_path() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("fake-tool");
        write_file(&tool, "#!/bin/sh\necho fake-tool 9.8.7\n");
        make_executable(&tool);

        assert!(find_binary(&tool.to_string_lossy()).is_some());
        assert_eq!(detect_binary_version_at(&tool), Some("9.8.7".to_string()));
    }

    #[test]
    fn find_binary_rejects_non_executable_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("not-executable");
        write_file(&tool, "#!/bin/sh\necho nope\n");
        assert_eq!(find_binary(&tool.to_string_lossy()), None);

        let missing = dir.path().join("nope");
        assert_eq!(find_binary(&missing.to_string_lossy()), None);
        assert_eq!(
            detect_binary_version("definitely-not-an-agenthub-binary-xyz"),
            None
        );
    }

    #[test]
    fn extracts_version_tokens_from_common_outputs() {
        assert_eq!(version_token("fake-tool 9.8.7"), Some("9.8.7".to_string()));
        assert_eq!(version_token("node v20.1.0"), Some("20.1.0".to_string()));
        assert_eq!(version_token("Python 3.11.5"), Some("3.11.5".to_string()));
        assert_eq!(
            version_token("git version 2.39.3 (Apple Git-145)"),
            Some("2.39.3".to_string())
        );
        assert_eq!(version_token("no digits here"), None);
    }
}
