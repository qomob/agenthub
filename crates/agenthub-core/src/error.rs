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
