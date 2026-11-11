//! Error type used across the core engine.

use std::fmt;

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("toml error: {0}")]
    Toml(String),
    #[error("zip error: {0}")]
    Zip(String),
    #[error("invalid skill: {0}")]
    InvalidSkill(String),
    /// A user-supplied parameter failed validation or refers to a non-existent
    /// entity (My AI Stack module; D-2). Kept distinct from `InvalidSkill`,
    /// which carries the legacy INVALID_SKILL semantics for skill imports.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The target resource exists, but the requested state change is not
    /// allowed in its current state — e.g. deleting a stack item that still
    /// holds subscription records, or merging an already-merged item (D-2).
    /// Distinct from `Conflict`, which means a stale operation plan.
    #[error("resource conflict: {0}")]
    ResourceConflict(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("blocked: {0}")]
    Blocked(String),
    #[error("external modification: {0}")]
    ExternalModification(String),
    #[error("{0}")]
    Message(String),
}

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Error::Message(message.into())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Error::InvalidSkill(message.into())
    }

    /// D-2: parameter-level validation failure (maps to INVALID_INPUT).
    /// Note: `invalid()` deliberately maps to InvalidSkill for legacy
    /// compatibility; new My AI Stack code must use this constructor instead.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Error::InvalidInput(message.into())
    }

    /// D-2: resource exists but the state change is not allowed
    /// (maps to RESOURCE_CONFLICT / exit code 4).
    pub fn resource_conflict(message: impl Into<String>) -> Self {
        Error::ResourceConflict(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Error::Conflict(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Error::NotFound(message.into())
    }

    pub fn blocked(message: impl Into<String>) -> Self {
        Error::Blocked(message.into())
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Error::Unsupported(message.into())
    }
}

impl From<toml_edit::TomlError> for Error {
    fn from(value: toml_edit::TomlError) -> Self {
        Error::Toml(value.to_string())
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(value: zip::result::ZipError) -> Self {
        Error::Zip(value.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl fmt::Display for HumanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Wrapper used when an operation should keep going and record a non-fatal error.
#[derive(Debug, Clone, Serialize)]
pub struct HumanError(pub String);

pub type Result<T, E = Error> = std::result::Result<T, E>;
