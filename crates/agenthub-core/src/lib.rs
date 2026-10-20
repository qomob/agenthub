//! AgentHub — local multi-agent capability manager.
//!
//! Phase 1 MVP: real agent detection, skill discovery, central library,
//! transactional distribution with snapshots and rollback.

pub mod adapters;
pub mod db;
pub mod error;
pub mod library;
pub mod mcp;
pub mod model;
pub mod ops;
pub mod scan;
pub mod service;
pub mod skill;
pub mod util;

pub use error::{Error, Result};
pub use service::{AgentHub, CoreConfig};
