//! v3.1 API contract primitives: unified result envelope, stable error codes
//! and request identifiers. Shared by the IPC layer and the CLI so both expose
//! the same machine-readable surface.

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Generate a fresh request id for every command invocation.
pub fn request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Stable error codes from `docs/03-architecture/API_CONTRACTS.md` §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    UnsupportedCapability,
    UnverifiedAdapter,
    PermissionDenied,
    PathEscape,
    InvalidSkill,
    McpVersionMismatch,
    SecretLocked,
    PlanStale,
    ExternalModification,
    PartialApply,
    RollbackBlocked,
    ExecutorUnavailable,
    EvalInconclusive,
    BudgetExceeded,
    UpstreamTimeout,
    RateLimited,
    /// Internal failures that do not map to a public v3.1 code yet.
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidInput => "INVALID_INPUT",
            ErrorCode::UnsupportedCapability => "UNSUPPORTED_CAPABILITY",
            ErrorCode::UnverifiedAdapter => "UNVERIFIED_ADAPTER",
            ErrorCode::PermissionDenied => "PERMISSION_DENIED",
            ErrorCode::PathEscape => "PATH_ESCAPE",
            ErrorCode::InvalidSkill => "INVALID_SKILL",
            ErrorCode::McpVersionMismatch => "MCP_VERSION_MISMATCH",
            ErrorCode::SecretLocked => "SECRET_LOCKED",
            ErrorCode::PlanStale => "PLAN_STALE",
            ErrorCode::ExternalModification => "EXTERNAL_MODIFICATION",
            ErrorCode::PartialApply => "PARTIAL_APPLY",
            ErrorCode::RollbackBlocked => "ROLLBACK_BLOCKED",
            ErrorCode::ExecutorUnavailable => "EXECUTOR_UNAVAILABLE",
            ErrorCode::EvalInconclusive => "EVAL_INCONCLUSIVE",
            ErrorCode::BudgetExceeded => "BUDGET_EXCEEDED",
            ErrorCode::UpstreamTimeout => "UPSTREAM_TIMEOUT",
            ErrorCode::RateLimited => "RATE_LIMITED",
            ErrorCode::Internal => "INTERNAL",
        }
    }

    /// Normalized CLI exit code (`API_CONTRACTS.md` §6).
    pub fn exit_code(self) -> u8 {
        match self {
            ErrorCode::PartialApply => 2,
            ErrorCode::InvalidInput
            | ErrorCode::UnsupportedCapability
            | ErrorCode::UnverifiedAdapter
            | ErrorCode::PermissionDenied
            | ErrorCode::PathEscape
            | ErrorCode::InvalidSkill
            | ErrorCode::McpVersionMismatch => 3,
            ErrorCode::PlanStale => 4,
            ErrorCode::ExternalModification
            | ErrorCode::RollbackBlocked
            | ErrorCode::SecretLocked
            | ErrorCode::ExecutorUnavailable => 5,
            _ => 5,
        }
    }

    /// Whether retrying the same request could succeed.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::ExecutorUnavailable
                | ErrorCode::UpstreamTimeout
                | ErrorCode::RateLimited
                | ErrorCode::Internal
        )
    }
}

impl From<&Error> for ErrorCode {
    fn from(error: &Error) -> Self {
        match error {
            Error::Io(_) => ErrorCode::Internal,
            Error::Database(_) => ErrorCode::Internal,
            Error::Json(_) => ErrorCode::InvalidInput,
            Error::Yaml(_) => ErrorCode::InvalidInput,
            Error::Toml(_) => ErrorCode::InvalidInput,
            Error::Zip(_) => ErrorCode::InvalidSkill,
            Error::InvalidSkill(_) => ErrorCode::InvalidSkill,
            Error::Conflict(_) => ErrorCode::PlanStale,
            Error::NotFound(_) => ErrorCode::InvalidInput,
            Error::Unsupported(_) => ErrorCode::UnsupportedCapability,
            Error::Verification(_) => ErrorCode::PartialApply,
            Error::Blocked(_) => ErrorCode::PermissionDenied,
            Error::ExternalModification(_) => ErrorCode::ExternalModification,
            Error::Message(_) => ErrorCode::Internal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn from_error(error: &Error) -> Self {
        let code = ErrorCode::from(error);
        ApiError {
            code,
            message: error.to_string(),
            retryable: code.retryable(),
            details: None,
        }
    }

    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            retryable: code.retryable(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Unified success/error envelope. `data` and `error` are mutually exclusive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResult<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    pub request_id: String,
}

impl<T> ApiResult<T> {
    pub fn success(data: T) -> Self {
        ApiResult {
            ok: true,
            data: Some(data),
            error: None,
            request_id: request_id(),
        }
    }

    pub fn failure(error: ApiError) -> Self {
        ApiResult {
            ok: false,
            data: None,
            error: Some(error),
            request_id: request_id(),
        }
    }

    pub fn from_result(result: Result<T, Error>) -> Self {
        match result {
            Ok(data) => ApiResult::success(data),
            Err(error) => ApiResult::failure(ApiError::from_error(&error)),
        }
    }
}

/// Deterministic JSON serialization with object keys sorted recursively. Used
/// for plan hashes and content-addressed manifests so the same semantic plan
/// always yields the same digest regardless of map ordering.
pub fn canonical_json(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_else(|_| "\"\"".into()));
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => {
            out.push_str(
                &serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
            );
        }
    }
}

/// SHA-256 hex digest of arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

/// SHA-256 of the canonical JSON representation of a value.
pub fn canonical_sha256(value: &serde_json::Value) -> String {
    sha256_hex(canonical_json(value).as_bytes())
}
