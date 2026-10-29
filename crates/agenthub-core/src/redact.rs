//! Redaction helpers for user-facing diagnostic exports.
//!
//! The diagnostics export promises to be secret-free (see
//! `AgentHub::export_diagnostics`). MCP server records can still carry
//! credentials inside data that is *not* a stored secret value:
//!
//! - `url` values may embed tokens as query parameters
//!   (`https://host/mcp?token=...`);
//! - stdio `args` may pass keys as command-line arguments
//!   (`--api-key <value>`);
//! - absolute paths reveal the user's home directory layout.
//!
//! None of these are guessed at the storage layer, so redaction happens at
//! the export boundary: URLs keep only `scheme://host[:port]/path`, argv
//! values that follow secret-bearing flags are replaced, and the home
//! directory prefix is folded to `~`. Distribution writes (plans, applies)
//! are untouched — redaction is export-only.

/// Redact an MCP URL: strip query string and fragment, keep scheme/host/port.
/// Implemented without a URL-parsing dependency: split after the `scheme://`
/// prefix, then cut at the first `?` or `#` so userinfo-less authority and
/// path survive intact.
pub fn redact_url(url: &str) -> String {
    let (prefix, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (format!("{scheme}://"), rest),
        // Not a scheme-qualified URL: treat the whole string as path-like and
        // cut at the first query/fragment separator.
        None => (String::new(), url),
    };
    let rest = match rest.find(['?', '#']) {
        Some(index) => &rest[..index],
        None => rest,
    };
    // Drop userinfo (`user:pass@host`) — credentials in the authority.
    let rest = match rest.find('/') {
        Some(slash) if rest[..slash].contains('@') => &rest[rest.find('@').unwrap() + 1..],
        _ => match rest.find('@') {
            Some(at) => &rest[at + 1..],
            None => rest,
        },
    };
    format!("{prefix}{rest}")
}

/// Flags whose *following* argv value is considered a secret.
const SECRET_VALUE_FLAGS: &[&str] = &[
    "--api-key",
    "--apikey",
    "--token",
    "--access-token",
    "--auth-token",
    "-t",
    "-k",
    "--key",
    "--password",
    "--passwd",
    "--secret",
    "--header",
    "-h",
    "-H",
    "--authorization",
];

/// Flags whose `flag=value` form is considered a secret.
const SECRET_EQ_FLAGS: &[&str] = &[
    "api_key", "api-key", "apikey", "token", "access_token", "access-token", "auth_token",
    "auth-token", "password", "passwd", "secret", "authorization",
];

/// Redact a single argv token: either a secret-bearing `flag=value` token or
/// the value that follows a secret-bearing flag. Returns `None` when the
/// token passes through unchanged.
fn redact_one(token: &str, next_is_value: bool) -> Option<String> {
    // `--api-key=sk-...` inline form.
    if let Some((flag, _value)) = token.split_once('=') {
        let bare = flag.trim_start_matches('-');
        if SECRET_EQ_FLAGS.contains(&bare.to_ascii_lowercase().as_str()) {
            return Some(format!("{flag}=[REDACTED]"));
        }
    }
    // `--api-key sk-...` separate-value form: redact this token if the
    // previous position expected a value.
    let bare = token.trim_start_matches('-');
    if next_is_value {
        return Some("[REDACTED]".to_string());
    }
    let _ = bare;
    None
}

/// Redact an argv list used by a stdio MCP server for export purposes.
pub fn redact_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut redact_next = false;
    for token in args {
        if redact_next {
            out.push("[REDACTED]".to_string());
            redact_next = false;
            continue;
        }
        match redact_one(token, false) {
            Some(redacted) => out.push(redacted),
            None => {
                let lower = token.to_ascii_lowercase();
                if SECRET_VALUE_FLAGS.contains(&lower.as_str()) {
                    out.push(token.clone());
                    redact_next = true;
                } else if let Some((flag, _)) = token.split_once('=') {
                    let bare = flag.trim_start_matches('-').to_ascii_lowercase();
                    if SECRET_EQ_FLAGS.contains(&bare.as_str()) {
                        out.push(format!("{flag}=[REDACTED]"));
                    } else {
                        out.push(token.clone());
                    }
                } else {
                    out.push(token.clone());
                }
            }
        }
    }
    out
}

/// Fold the user's home directory prefix to `~` in a path-like string.
pub fn fold_home(path: &str, home: &str) -> String {
    if home.is_empty() || home == "/" {
        return path.to_string();
    }
    let home = home.trim_end_matches('/');
    if let Some(rest) = path.strip_prefix(home) {
        if rest.is_empty() {
            return "~".to_string();
        }
        if rest.starts_with('/') {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_query_and_fragment_are_stripped() {
        assert_eq!(
            redact_url("https://host.example/mcp?token=abc123&x=1"),
            "https://host.example/mcp"
        );
        assert_eq!(
            redact_url("http://127.0.0.1:8080/sse#frag"),
            "http://127.0.0.1:8080/sse"
        );
        assert_eq!(redact_url("https://host.example/mcp"), "https://host.example/mcp");
        assert_eq!(redact_url("not a url?token=x"), "not a url");
        // Userinfo with a query separator must not leak the credential part.
        assert_eq!(
            redact_url("https://user:pass@host.example/mcp?token=abc"),
            "https://host.example/mcp"
        );
    }

    #[test]
    fn argv_secrets_are_redacted() {
        let args: Vec<String> = ["-y", "some-server", "--api-key", "sk-live-xyz", "--port", "8080"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            redact_args(&args),
            vec!["-y", "some-server", "--api-key", "[REDACTED]", "--port", "8080"]
        );
        let inline: Vec<String> = vec!["--token=abc".to_string(), "run".to_string()];
        assert_eq!(redact_args(&inline), vec!["--token=[REDACTED]", "run"]);
        let header: Vec<String> = vec!["--header".to_string(), "Authorization: Bearer zzz".to_string()];
        assert_eq!(redact_args(&header), vec!["--header", "[REDACTED]"]);
        // Non-secret flags keep their values.
        let plain: Vec<String> = vec!["--verbose".to_string(), "value".to_string()];
        assert_eq!(redact_args(&plain), plain);
    }

    #[test]
    fn home_prefix_folds_to_tilde() {
        assert_eq!(fold_home("/Users/example/.codex", "/Users/example"), "~/.codex");
        assert_eq!(fold_home("/Users/example", "/Users/example"), "~");
        assert_eq!(fold_home("/usr/local", "/Users/example"), "/usr/local");
        // Prefix must not match a longer sibling path.
        assert_eq!(
            fold_home("/Users/exampleson/x", "/Users/example"),
            "/Users/exampleson/x"
        );
    }
}
