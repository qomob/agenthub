//! SQLite persistence layer. WAL enabled, foreign keys enforced, migrations
//! tracked through `PRAGMA user_version`.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::*;
use crate::util::now;

const SCHEMA_V1: &str = r#"
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE agents (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  vendor TEXT NOT NULL,
  adapter_id TEXT NOT NULL,
  app_path TEXT,
  home_path TEXT,
  custom_skill_roots TEXT NOT NULL DEFAULT '[]',
  notes TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE capabilities (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('skill','mcp','plugin')),
  slug TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  current_version_id TEXT,
  tags TEXT NOT NULL DEFAULT '[]',
  license TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (kind, slug)
);

CREATE TABLE capability_versions (
  id TEXT PRIMARY KEY,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  version_label TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  storage_path TEXT NOT NULL,
  manifest TEXT NOT NULL DEFAULT '{}',
  skill_md TEXT NOT NULL DEFAULT '',
  size_bytes INTEGER NOT NULL DEFAULT 0,
  file_count INTEGER NOT NULL DEFAULT 0,
  notes TEXT,
  created_at TEXT NOT NULL,
  parent_version_id TEXT,
  UNIQUE (capability_id, content_hash)
);
CREATE INDEX idx_versions_capability ON capability_versions(capability_id);

CREATE TABLE capability_sources (
  id TEXT PRIMARY KEY,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  source_type TEXT NOT NULL,
  uri TEXT NOT NULL,
  reference TEXT,
  imported_at TEXT NOT NULL,
  last_checked_at TEXT,
  metadata TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_sources_capability ON capability_sources(capability_id);

CREATE TABLE installations (
  id TEXT PRIMARY KEY,
  capability_id TEXT REFERENCES capabilities(id) ON DELETE CASCADE,
  version_id TEXT REFERENCES capability_versions(id),
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  scope TEXT NOT NULL DEFAULT 'global',
  strategy TEXT NOT NULL,
  target_path TEXT NOT NULL,
  source_path TEXT,
  content_hash TEXT,
  state TEXT NOT NULL,
  config_key TEXT NOT NULL DEFAULT '',
  installed_at TEXT NOT NULL,
  verified_at TEXT,
  last_check_at TEXT,
  metadata TEXT NOT NULL DEFAULT '{}'
);
CREATE UNIQUE INDEX idx_installations_target ON installations(agent_id, target_path, config_key);
CREATE INDEX idx_installations_capability ON installations(capability_id);
CREATE INDEX idx_installations_agent ON installations(agent_id);

CREATE TABLE mcp_definitions (
  id TEXT PRIMARY KEY,
  capability_id TEXT REFERENCES capabilities(id) ON DELETE SET NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  transport TEXT NOT NULL,
  command TEXT,
  args TEXT NOT NULL DEFAULT '[]',
  url TEXT,
  headers TEXT NOT NULL DEFAULT '{}',
  env_keys TEXT NOT NULL DEFAULT '[]',
  credential_refs TEXT NOT NULL DEFAULT '[]',
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_mcp_name ON mcp_definitions(name);

CREATE TABLE plugin_records (
  id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  version TEXT,
  source TEXT,
  install_state TEXT NOT NULL,
  managed_by TEXT NOT NULL,
  permissions TEXT NOT NULL DEFAULT '[]',
  official_url TEXT,
  discovered_path TEXT,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (agent_id, name)
);

CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT '',
  is_builtin INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE profile_items (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  enabled INTEGER NOT NULL DEFAULT 1,
  target_agents TEXT NOT NULL DEFAULT '[]',
  UNIQUE (profile_id, capability_id)
);

CREATE TABLE operations (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  dry_run INTEGER NOT NULL DEFAULT 0,
  actor TEXT NOT NULL DEFAULT 'user',
  summary TEXT NOT NULL DEFAULT '',
  plan TEXT NOT NULL DEFAULT '{}',
  result TEXT NOT NULL DEFAULT '{}',
  error TEXT,
  rollback_of TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT
);
CREATE INDEX idx_operations_started ON operations(started_at DESC);

CREATE TABLE snapshots (
  id TEXT PRIMARY KEY,
  operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  original_path TEXT NOT NULL,
  backup_path TEXT NOT NULL,
  existed INTEGER NOT NULL,
  hash_before TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_snapshots_operation ON snapshots(operation_id);

CREATE TABLE scan_results (
  id TEXT PRIMARY KEY,
  agent_id TEXT,
  scope TEXT,
  path TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  fingerprint TEXT,
  payload TEXT NOT NULL DEFAULT '{}',
  scanned_at TEXT NOT NULL
);
CREATE INDEX idx_scan_agent ON scan_results(agent_id);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE capability_audits (
  id TEXT PRIMARY KEY,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  version_id TEXT NOT NULL REFERENCES capability_versions(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  risk TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  findings TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_audits_capability ON capability_audits(capability_id, version_id, created_at DESC);

CREATE TABLE dependency_reports (
  id TEXT PRIMARY KEY,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  version_id TEXT NOT NULL REFERENCES capability_versions(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  risk TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  items TEXT NOT NULL DEFAULT '[]',
  checked_at TEXT NOT NULL
);
CREATE INDEX idx_deps_capability ON dependency_reports(capability_id, version_id, checked_at DESC);

CREATE TABLE update_impacts (
  id TEXT PRIMARY KEY,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  from_version_id TEXT NOT NULL,
  to_version_id TEXT NOT NULL,
  risk TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_impacts_capability ON update_impacts(capability_id, created_at DESC);

CREATE TABLE validation_runs (
  id TEXT PRIMARY KEY,
  capability_id TEXT NOT NULL REFERENCES capabilities(id) ON DELETE CASCADE,
  version_id TEXT NOT NULL REFERENCES capability_versions(id) ON DELETE CASCADE,
  agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  case_name TEXT NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  evidence TEXT NOT NULL DEFAULT '',
  actor TEXT NOT NULL DEFAULT 'user',
  created_at TEXT NOT NULL
);
CREATE INDEX idx_validations_capability ON validation_runs(capability_id, created_at DESC);

CREATE TABLE agent_versions (
  id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  version TEXT,
  previous_version TEXT,
  changed INTEGER NOT NULL DEFAULT 0,
  evidence TEXT NOT NULL DEFAULT '',
  detected_at TEXT NOT NULL
);
CREATE INDEX idx_agent_versions_agent ON agent_versions(agent_id, detected_at DESC);
"#;

// ---------------------------------------------------------------------------
// Internal row structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentRow {
    pub id: String,
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub adapter_id: String,
    pub app_path: Option<String>,
    pub home_path: Option<String>,
    pub custom_skill_roots: Vec<String>,
    pub notes: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentRow {
    pub fn to_model(&self) -> AgentRecord {
        AgentRecord {
            id: self.id.clone(),
            key: self.key.clone(),
            name: self.name.clone(),
            vendor: self.vendor.clone(),
            adapter_id: self.adapter_id.clone(),
            app_path: self.app_path.clone(),
            home_path: self.home_path.clone(),
            custom_skill_roots: self.custom_skill_roots.clone(),
            notes: self.notes.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapabilityRow {
    pub id: String,
    pub kind: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub current_version_id: Option<String>,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct VersionRow {
    pub id: String,
    pub capability_id: String,
    pub version_label: String,
    pub content_hash: String,
    pub storage_path: String,
    pub manifest: String,
    pub skill_md: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationRow {
    pub id: String,
    pub capability_id: Option<String>,
    pub version_id: Option<String>,
    pub agent_id: String,
    pub kind: String,
    pub scope: String,
    pub strategy: String,
    pub target_path: String,
    pub source_path: Option<String>,
    pub content_hash: Option<String>,
    pub state: String,
    pub config_key: String,
    pub installed_at: String,
    pub verified_at: Option<String>,
    pub last_check_at: Option<String>,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct OperationRow {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub dry_run: bool,
    pub actor: String,
    pub summary: String,
    pub plan: String,
    pub result: String,
    pub error: Option<String>,
    pub rollback_of: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct McpRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub headers: Vec<(String, String)>,
    pub env_keys: Vec<String>,
    pub credential_refs: Vec<String>,
    pub capability_id: Option<String>,
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct PluginRow {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub install_state: String,
    pub managed_by: String,
    pub permissions: Vec<String>,
    pub official_url: Option<String>,
    pub discovered_path: Option<String>,
    pub metadata: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ProfileItemRow {
    pub capability_id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub target_agent_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<ProfileItemRow>,
}

#[derive(Debug, Clone)]
pub struct ScanResultRow {
    pub agent_id: Option<String>,
    pub scope: Option<String>,
    pub path: String,
    pub kind: String,
    pub status: String,
    pub fingerprint: Option<String>,
    pub payload: String,
    pub scanned_at: String,
}

#[derive(Debug, Clone)]
pub struct SnapshotRow {
    pub id: String,
    pub operation_id: String,
    pub kind: String,
    pub original_path: String,
    pub backup_path: String,
    pub existed: bool,
    pub hash_before: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

pub fn from_json<T: DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_str(value)?)
}

fn json_or_default<T: DeserializeOwned + Default>(value: Option<String>) -> T {
    value
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let db = Database {
            conn,
            path: path.to_path_buf(),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        let db = Database {
            conn,
            path: PathBuf::from(":memory:"),
        };
        db.migrate()?;
        Ok(db)
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA_V1)?;
            tx.pragma_update(None, "user_version", 1)?;
            tx.commit()?;
        }
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 2 {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA_V2)?;
            tx.pragma_update(None, "user_version", 2)?;
            tx.commit()?;
        }
        Ok(())
    }

    // -- settings ----------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now()],
        )?;
        Ok(())
    }

    // -- agents ------------------------------------------------------------

    pub fn upsert_agent(&self, agent: &AgentRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO agents (id, key, name, vendor, adapter_id, app_path, home_path, custom_skill_roots, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(key) DO UPDATE SET
               name = excluded.name,
               vendor = excluded.vendor,
               adapter_id = excluded.adapter_id,
               app_path = COALESCE(excluded.app_path, agents.app_path),
               home_path = COALESCE(excluded.home_path, agents.home_path),
               custom_skill_roots = agents.custom_skill_roots,
               notes = excluded.notes,
               updated_at = excluded.updated_at",
            params![
                agent.id,
                agent.key,
                agent.name,
                agent.vendor,
                agent.adapter_id,
                agent.app_path,
                agent.home_path,
                to_json(&agent.custom_skill_roots)?,
                to_json(&agent.notes)?,
                agent.created_at,
                agent.updated_at
            ],
        )?;
        Ok(())
    }

    fn agent_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRow> {
        Ok(AgentRow {
            id: row.get(0)?,
            key: row.get(1)?,
            name: row.get(2)?,
            vendor: row.get(3)?,
            adapter_id: row.get(4)?,
            app_path: row.get(5)?,
            home_path: row.get(6)?,
            custom_skill_roots: json_or_default(row.get(7)?),
            notes: json_or_default(row.get(8)?),
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }

    pub fn list_agents(&self) -> Result<Vec<AgentRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, key, name, vendor, adapter_id, app_path, home_path, custom_skill_roots, notes, created_at, updated_at
             FROM agents ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], Self::agent_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_agent(&self, id: &str) -> Result<Option<AgentRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, key, name, vendor, adapter_id, app_path, home_path, custom_skill_roots, notes, created_at, updated_at
                 FROM agents WHERE id = ?1",
                params![id],
                Self::agent_from_row,
            )
            .optional()?)
    }

    pub fn get_agent_by_key(&self, key: &str) -> Result<Option<AgentRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, key, name, vendor, adapter_id, app_path, home_path, custom_skill_roots, notes, created_at, updated_at
                 FROM agents WHERE key = ?1",
                params![key],
                Self::agent_from_row,
            )
            .optional()?)
    }

    pub fn update_agent_paths(&self, agent: &AgentRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE agents SET app_path = ?2, home_path = ?3, custom_skill_roots = ?4, updated_at = ?5 WHERE id = ?1",
            params![
                agent.id,
                agent.app_path,
                agent.home_path,
                to_json(&agent.custom_skill_roots)?,
                now()
            ],
        )?;
        Ok(())
    }

    // -- capabilities / versions / sources ---------------------------------

    fn capability_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CapabilityRow> {
        Ok(CapabilityRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            slug: row.get(2)?,
            name: row.get(3)?,
            description: row.get(4)?,
            current_version_id: row.get(5)?,
            tags: json_or_default(row.get(6)?),
            license: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }

    pub fn find_capability(&self, kind: &str, slug: &str) -> Result<Option<CapabilityRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, kind, slug, name, description, current_version_id, tags, license, created_at, updated_at
                 FROM capabilities WHERE kind = ?1 AND slug = ?2",
                params![kind, slug],
                Self::capability_from_row,
            )
            .optional()?)
    }

    pub fn get_capability(&self, id: &str) -> Result<Option<CapabilityRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, kind, slug, name, description, current_version_id, tags, license, created_at, updated_at
                 FROM capabilities WHERE id = ?1",
                params![id],
                Self::capability_from_row,
            )
            .optional()?)
    }

    pub fn list_capabilities(&self, kind: &str) -> Result<Vec<CapabilityRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, slug, name, description, current_version_id, tags, license, created_at, updated_at
             FROM capabilities WHERE kind = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![kind], Self::capability_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_capability(&self, row: &CapabilityRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO capabilities (id, kind, slug, name, description, current_version_id, tags, license, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.kind,
                row.slug,
                row.name,
                row.description,
                row.current_version_id,
                to_json(&row.tags)?,
                row.license,
                row.created_at,
                row.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn update_capability_meta(
        &self,
        id: &str,
        name: &str,
        description: &str,
        current_version_id: Option<&str>,
        license: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE capabilities SET name = ?2, description = ?3,
               current_version_id = COALESCE(?4, current_version_id), license = COALESCE(?5, license), updated_at = ?6
             WHERE id = ?1",
            params![id, name, description, current_version_id, license, now()],
        )?;
        Ok(())
    }

    fn version_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VersionRow> {
        Ok(VersionRow {
            id: row.get(0)?,
            capability_id: row.get(1)?,
            version_label: row.get(2)?,
            content_hash: row.get(3)?,
            storage_path: row.get(4)?,
            manifest: row.get(5)?,
            skill_md: row.get(6)?,
            size_bytes: row.get(7)?,
            file_count: row.get(8)?,
            notes: row.get(9)?,
            created_at: row.get(10)?,
        })
    }

    pub fn insert_version(&self, row: &VersionRow, parent_version_id: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO capability_versions (id, capability_id, version_label, content_hash, storage_path, manifest, skill_md, size_bytes, file_count, notes, created_at, parent_version_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                row.id,
                row.capability_id,
                row.version_label,
                row.content_hash,
                row.storage_path,
                row.manifest,
                row.skill_md,
                row.size_bytes,
                row.file_count,
                row.notes,
                row.created_at,
                parent_version_id
            ],
        )?;
        Ok(())
    }

    pub fn get_version(&self, id: &str) -> Result<Option<VersionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, capability_id, version_label, content_hash, storage_path, manifest, skill_md, size_bytes, file_count, notes, created_at
                 FROM capability_versions WHERE id = ?1",
                params![id],
                Self::version_from_row,
            )
            .optional()?)
    }

    pub fn find_version_by_hash(&self, capability_id: &str, hash: &str) -> Result<Option<VersionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, capability_id, version_label, content_hash, storage_path, manifest, skill_md, size_bytes, file_count, notes, created_at
                 FROM capability_versions WHERE capability_id = ?1 AND content_hash = ?2",
                params![capability_id, hash],
                Self::version_from_row,
            )
            .optional()?)
    }

    pub fn list_versions(&self, capability_id: &str) -> Result<Vec<VersionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, capability_id, version_label, content_hash, storage_path, manifest, skill_md, size_bytes, file_count, notes, created_at
             FROM capability_versions WHERE capability_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![capability_id], Self::version_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_source(
        &self,
        capability_id: &str,
        source_type: &str,
        uri: &str,
        reference: Option<&str>,
    ) -> Result<String> {
        let id = crate::util::new_id();
        self.conn.execute(
            "INSERT INTO capability_sources (id, capability_id, source_type, uri, reference, imported_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '{}')",
            params![id, capability_id, source_type, uri, reference, now()],
        )?;
        Ok(id)
    }

    pub fn get_source(&self, capability_id: &str) -> Result<Option<SourceInfo>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, source_type, uri, reference, imported_at, last_checked_at
                 FROM capability_sources WHERE capability_id = ?1 ORDER BY imported_at DESC LIMIT 1",
                params![capability_id],
                |row| {
                    Ok(SourceInfo {
                        id: row.get(0)?,
                        source_type: row.get(1)?,
                        uri: row.get(2)?,
                        reference: row.get(3)?,
                        imported_at: row.get(4)?,
                        last_checked_at: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    // -- installations -----------------------------------------------------

    fn installation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<InstallationRow> {
        Ok(InstallationRow {
            id: row.get(0)?,
            capability_id: row.get(1)?,
            version_id: row.get(2)?,
            agent_id: row.get(3)?,
            kind: row.get(4)?,
            scope: row.get(5)?,
            strategy: row.get(6)?,
            target_path: row.get(7)?,
            source_path: row.get(8)?,
            content_hash: row.get(9)?,
            state: row.get(10)?,
            config_key: row.get(11)?,
            installed_at: row.get(12)?,
            verified_at: row.get(13)?,
            last_check_at: row.get(14)?,
            metadata: row.get(15)?,
        })
    }

    const INSTALLATION_COLS: &'static str = "id, capability_id, version_id, agent_id, kind, scope, strategy, target_path, source_path, content_hash, state, config_key, installed_at, verified_at, last_check_at, metadata";

    pub fn find_mcp_installation(
        &self,
        agent_id: &str,
        target_path: &str,
        config_key: &str,
    ) -> Result<Option<InstallationRow>> {
        let sql = format!(
            "SELECT {} FROM installations WHERE agent_id = ?1 AND target_path = ?2 AND config_key = ?3",
            Self::INSTALLATION_COLS
        );
        Ok(self
            .conn
            .query_row(
                &sql,
                params![agent_id, target_path, config_key],
                Self::installation_from_row,
            )
            .optional()?)
    }

    pub fn get_installation(&self, id: &str) -> Result<Option<InstallationRow>> {
        let sql = format!("SELECT {} FROM installations WHERE id = ?1", Self::INSTALLATION_COLS);
        Ok(self
            .conn
            .query_row(&sql, params![id], Self::installation_from_row)
            .optional()?)
    }

    pub fn find_installation(&self, agent_id: &str, target_path: &str) -> Result<Option<InstallationRow>> {
        let sql = format!(
            "SELECT {} FROM installations WHERE agent_id = ?1 AND target_path = ?2 AND config_key = ''",
            Self::INSTALLATION_COLS
        );
        Ok(self
            .conn
            .query_row(&sql, params![agent_id, target_path], Self::installation_from_row)
            .optional()?)
    }

    pub fn list_installations_for_capability(&self, capability_id: &str) -> Result<Vec<InstallationRow>> {
        let sql = format!(
            "SELECT {} FROM installations WHERE capability_id = ?1 ORDER BY installed_at DESC",
            Self::INSTALLATION_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![capability_id], Self::installation_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_installations_for_agent(&self, agent_id: &str) -> Result<Vec<InstallationRow>> {
        let sql = format!(
            "SELECT {} FROM installations WHERE agent_id = ?1 ORDER BY installed_at DESC",
            Self::INSTALLATION_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![agent_id], Self::installation_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_installations(&self) -> Result<Vec<InstallationRow>> {
        let sql = format!(
            "SELECT {} FROM installations ORDER BY installed_at DESC",
            Self::INSTALLATION_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::installation_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_installation(&self, row: &InstallationRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO installations (id, capability_id, version_id, agent_id, kind, scope, strategy, target_path, source_path, content_hash, state, config_key, installed_at, verified_at, last_check_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(agent_id, target_path, config_key) DO UPDATE SET
               capability_id = excluded.capability_id,
               version_id = excluded.version_id,
               kind = excluded.kind,
               scope = excluded.scope,
               strategy = excluded.strategy,
               source_path = excluded.source_path,
               content_hash = excluded.content_hash,
               state = excluded.state,
               config_key = excluded.config_key,
               installed_at = excluded.installed_at,
               verified_at = excluded.verified_at,
               last_check_at = excluded.last_check_at,
               metadata = excluded.metadata",
            params![
                row.id,
                row.capability_id,
                row.version_id,
                row.agent_id,
                row.kind,
                row.scope,
                row.strategy,
                row.target_path,
                row.source_path,
                row.content_hash,
                row.state,
                row.config_key,
                row.installed_at,
                row.verified_at,
                row.last_check_at,
                row.metadata
            ],
        )?;
        Ok(())
    }

    pub fn delete_installation(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM installations WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -- mcp ---------------------------------------------------------------

    fn mcp_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpRow> {
        Ok(McpRow {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            transport: row.get(3)?,
            command: row.get(4)?,
            args: json_or_default(row.get(5)?),
            url: row.get(6)?,
            headers: json_or_default(row.get(7)?),
            env_keys: json_or_default(row.get(8)?),
            credential_refs: json_or_default(row.get(9)?),
            capability_id: row.get(10)?,
            metadata: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }

    pub fn list_mcp(&self) -> Result<Vec<McpRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, transport, command, args, url, headers, env_keys, credential_refs, capability_id, metadata, created_at, updated_at
             FROM mcp_definitions ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], Self::mcp_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_mcp(&self, id: &str) -> Result<Option<McpRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, description, transport, command, args, url, headers, env_keys, credential_refs, capability_id, metadata, created_at, updated_at
                 FROM mcp_definitions WHERE id = ?1",
                params![id],
                Self::mcp_from_row,
            )
            .optional()?)
    }

    pub fn find_mcp_by_name(&self, name: &str) -> Result<Option<McpRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, description, transport, command, args, url, headers, env_keys, credential_refs, capability_id, metadata, created_at, updated_at
                 FROM mcp_definitions WHERE name = ?1",
                params![name],
                Self::mcp_from_row,
            )
            .optional()?)
    }

    pub fn upsert_mcp(&self, row: &McpRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_definitions (id, capability_id, name, description, transport, command, args, url, headers, env_keys, credential_refs, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(name) DO UPDATE SET
               description = excluded.description,
               transport = excluded.transport,
               command = excluded.command,
               args = excluded.args,
               url = excluded.url,
               headers = excluded.headers,
               env_keys = excluded.env_keys,
               credential_refs = excluded.credential_refs,
               metadata = excluded.metadata,
               updated_at = excluded.updated_at",
            params![
                row.id,
                row.capability_id,
                row.name,
                row.description,
                row.transport,
                row.command,
                to_json(&row.args)?,
                row.url,
                to_json(&row.headers)?,
                to_json(&row.env_keys)?,
                to_json(&row.credential_refs)?,
                row.metadata,
                row.created_at,
                row.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_mcp(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM mcp_definitions WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -- plugins -----------------------------------------------------------

    pub fn list_plugins(&self) -> Result<Vec<PluginRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent_id, name, version, source, install_state, managed_by, permissions, official_url, discovered_path, metadata, updated_at
             FROM plugin_records ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PluginRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                name: row.get(2)?,
                version: row.get(3)?,
                source: row.get(4)?,
                install_state: row.get(5)?,
                managed_by: row.get(6)?,
                permissions: json_or_default(row.get(7)?),
                official_url: row.get(8)?,
                discovered_path: row.get(9)?,
                metadata: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_plugin(&self, row: &PluginRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO plugin_records (id, agent_id, name, version, source, install_state, managed_by, permissions, official_url, discovered_path, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(agent_id, name) DO UPDATE SET
               version = excluded.version,
               source = excluded.source,
               install_state = excluded.install_state,
               managed_by = excluded.managed_by,
               permissions = excluded.permissions,
               official_url = excluded.official_url,
               discovered_path = excluded.discovered_path,
               metadata = excluded.metadata,
               updated_at = excluded.updated_at",
            params![
                row.id,
                row.agent_id,
                row.name,
                row.version,
                row.source,
                row.install_state,
                row.managed_by,
                to_json(&row.permissions)?,
                row.official_url,
                row.discovered_path,
                row.metadata,
                now(),
                row.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_plugin(&self, agent_id: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM plugin_records WHERE agent_id = ?1 AND name = ?2",
            params![agent_id, name],
        )?;
        Ok(())
    }

    // -- profiles ----------------------------------------------------------

    pub fn list_profiles(&self) -> Result<Vec<ProfileRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, is_builtin, created_at, updated_at FROM profiles ORDER BY is_builtin DESC, name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut profile_rows = Vec::new();
        for row in rows {
            let (id, name, description, is_builtin, created_at, updated_at) = row?;
            let mut item_stmt = self.conn.prepare(
                "SELECT c.id, c.kind, c.name, c.description, pi.enabled, pi.target_agents
                 FROM profile_items pi JOIN capabilities c ON c.id = pi.capability_id
                 WHERE pi.profile_id = ?1 ORDER BY c.name COLLATE NOCASE",
            )?;
            let item_rows = item_stmt.query_map(params![&id], |row| {
                Ok(ProfileItemRow {
                    capability_id: row.get(0)?,
                    kind: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    target_agent_keys: json_or_default(row.get(5)?),
                })
            })?;
            let mut items = Vec::new();
            for item in item_rows {
                items.push(item?);
            }
            profile_rows.push(ProfileRow {
                id,
                name,
                description,
                is_builtin: is_builtin != 0,
                created_at,
                updated_at,
                items,
            });
        }
        Ok(profile_rows)
    }

    pub fn insert_profile(
        &self,
        id: &str,
        name: &str,
        description: &str,
        is_builtin: bool,
        created_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profiles (id, name, description, is_builtin, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, name, description, if is_builtin { 1 } else { 0 }, created_at, created_at],
        )?;
        Ok(())
    }

    pub fn update_profile(&self, id: &str, name: &str, description: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE profiles SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, name, description, now()],
        )?;
        Ok(())
    }

    pub fn delete_profile_items(&self, profile_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM profile_items WHERE profile_id = ?1", params![profile_id])?;
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM profile_items WHERE profile_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn add_profile_item(
        &self,
        profile_id: &str,
        capability_id: &str,
        enabled: bool,
        target_agents: &[String],
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profile_items (id, profile_id, capability_id, enabled, target_agents)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                crate::util::new_id(),
                profile_id,
                capability_id,
                if enabled { 1 } else { 0 },
                to_json(&target_agents.to_vec())?
            ],
        )?;
        Ok(())
    }

    // -- operations --------------------------------------------------------

    fn operation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperationRow> {
        Ok(OperationRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            status: row.get(2)?,
            dry_run: row.get::<_, i64>(3)? != 0,
            actor: row.get(4)?,
            summary: row.get(5)?,
            plan: row.get(6)?,
            result: row.get(7)?,
            error: row.get(8)?,
            rollback_of: row.get(9)?,
            started_at: row.get(10)?,
            finished_at: row.get(11)?,
        })
    }

    pub fn insert_operation(
        &self,
        id: &str,
        kind: &str,
        dry_run: bool,
        summary: &str,
        plan: &str,
        rollback_of: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO operations (id, kind, status, dry_run, actor, summary, plan, result, rollback_of, started_at)
             VALUES (?1, ?2, 'planned', ?3, 'user', ?4, ?5, '{}', ?6, ?7)",
            params![id, kind, if dry_run { 1 } else { 0 }, summary, plan, rollback_of, now()],
        )?;
        Ok(())
    }

    pub fn update_operation_status(
        &self,
        id: &str,
        status: &str,
        result: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE operations SET status = ?2, result = ?3, error = ?4, finished_at = ?5 WHERE id = ?1",
            params![id, status, result, error, now()],
        )?;
        Ok(())
    }

    pub fn get_operation(&self, id: &str) -> Result<Option<OperationRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, kind, status, dry_run, actor, summary, plan, result, error, rollback_of, started_at, finished_at
                 FROM operations WHERE id = ?1",
                params![id],
                Self::operation_from_row,
            )
            .optional()?)
    }

    pub fn list_operations(&self, limit: usize) -> Result<Vec<OperationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, status, dry_run, actor, summary, plan, result, error, rollback_of, started_at, finished_at
             FROM operations ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], Self::operation_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // -- snapshots ---------------------------------------------------------

    pub fn insert_snapshot(&self, row: &SnapshotRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snapshots (id, operation_id, kind, original_path, backup_path, existed, hash_before, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.id,
                row.operation_id,
                row.kind,
                row.original_path,
                row.backup_path,
                if row.existed { 1 } else { 0 },
                row.hash_before,
                row.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_snapshots(&self, operation_id: &str) -> Result<Vec<SnapshotRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, operation_id, kind, original_path, backup_path, existed, hash_before, created_at
             FROM snapshots WHERE operation_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![operation_id], |row| {
            Ok(SnapshotRow {
                id: row.get(0)?,
                operation_id: row.get(1)?,
                kind: row.get(2)?,
                original_path: row.get(3)?,
                backup_path: row.get(4)?,
                existed: row.get::<_, i64>(5)? != 0,
                hash_before: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_all_snapshots(&self, limit: usize) -> Result<Vec<SnapshotRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, operation_id, kind, original_path, backup_path, existed, hash_before, created_at
             FROM snapshots ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SnapshotRow {
                id: row.get(0)?,
                operation_id: row.get(1)?,
                kind: row.get(2)?,
                original_path: row.get(3)?,
                backup_path: row.get(4)?,
                existed: row.get::<_, i64>(5)? != 0,
                hash_before: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // -- scan results ------------------------------------------------------

    pub fn replace_scan_results(&self, agent_id: &str, rows: &[ScanResultRow]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM scan_results WHERE agent_id = ?1", params![agent_id])?;
        for row in rows {
            tx.execute(
                "INSERT INTO scan_results (id, agent_id, scope, path, kind, status, fingerprint, payload, scanned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    crate::util::new_id(),
                    row.agent_id,
                    row.scope,
                    row.path,
                    row.kind,
                    row.status,
                    row.fingerprint,
                    row.payload,
                    row.scanned_at
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_scan_results(&self, agent_id: &str) -> Result<Vec<ScanResultRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, scope, path, kind, status, fingerprint, payload, scanned_at
             FROM scan_results WHERE agent_id = ?1 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok(ScanResultRow {
                agent_id: row.get(0)?,
                scope: row.get(1)?,
                path: row.get(2)?,
                kind: row.get(3)?,
                status: row.get(4)?,
                fingerprint: row.get(5)?,
                payload: row.get(6)?,
                scanned_at: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // -- counts ------------------------------------------------------------

    pub fn count(&self, table: &str) -> Result<i64> {
        let allowed = ["capabilities", "installations", "agents", "mcp_definitions", "plugin_records"];
        if !allowed.contains(&table) {
            return Err(Error::msg(format!("invalid count table {table}")));
        }
        let sql = format!("SELECT COUNT(*) FROM {table}");
        Ok(self.conn.query_row(&sql, [], |row| row.get(0))?)
    }

    pub fn count_capabilities(&self, kind: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM capabilities WHERE kind = ?1",
            params![kind],
            |row| row.get(0),
        )?)
    }
}

impl Database {
    /// Latest scan timestamp recorded for an agent (used by the UI).
    pub fn latest_scan_at(&self, agent_id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MAX(scanned_at) FROM scan_results WHERE agent_id = ?1",
                params![agent_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn latest_scan_global(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT MAX(scanned_at) FROM scan_results", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten())
    }
}

// ---------------------------------------------------------------------------
// Health / audit / validation persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AuditRow {
    pub id: String,
    pub capability_id: String,
    pub version_id: String,
    pub kind: String,
    pub status: String,
    pub risk: String,
    pub summary: String,
    pub findings: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct DependencyRow {
    pub id: String,
    pub capability_id: String,
    pub version_id: String,
    pub status: String,
    pub risk: String,
    pub summary: String,
    pub items: String,
    pub checked_at: String,
}

#[derive(Debug, Clone)]
pub struct ImpactRow {
    pub id: String,
    pub capability_id: String,
    pub from_version_id: String,
    pub to_version_id: String,
    pub risk: String,
    pub summary: String,
    pub payload: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ValidationRow {
    pub id: String,
    pub capability_id: String,
    pub version_id: String,
    pub agent_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub case_name: String,
    pub notes: String,
    pub evidence: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct AgentVersionRow {
    pub agent_id: String,
    pub version: Option<String>,
    pub previous_version: Option<String>,
    pub changed: bool,
    pub evidence: String,
    pub detected_at: String,
}

impl Database {
    pub fn insert_audit(&self, row: &AuditRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO capability_audits (id, capability_id, version_id, kind, status, risk, summary, findings, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.id,
                row.capability_id,
                row.version_id,
                row.kind,
                row.status,
                row.risk,
                row.summary,
                row.findings,
                row.created_at
            ],
        )?;
        Ok(())
    }

    pub fn latest_audit(
        &self,
        capability_id: &str,
        version_id: &str,
        kind: &str,
    ) -> Result<Option<AuditRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, capability_id, version_id, kind, status, risk, summary, findings, created_at
                 FROM capability_audits
                 WHERE capability_id = ?1 AND version_id = ?2 AND kind = ?3
                 ORDER BY created_at DESC LIMIT 1",
                params![capability_id, version_id, kind],
                |row| {
                    Ok(AuditRow {
                        id: row.get(0)?,
                        capability_id: row.get(1)?,
                        version_id: row.get(2)?,
                        kind: row.get(3)?,
                        status: row.get(4)?,
                        risk: row.get(5)?,
                        summary: row.get(6)?,
                        findings: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn list_audits(&self, capability_id: &str, limit: usize) -> Result<Vec<AuditRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, capability_id, version_id, kind, status, risk, summary, findings, created_at
             FROM capability_audits WHERE capability_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![capability_id, limit as i64], |row| {
            Ok(AuditRow {
                id: row.get(0)?,
                capability_id: row.get(1)?,
                version_id: row.get(2)?,
                kind: row.get(3)?,
                status: row.get(4)?,
                risk: row.get(5)?,
                summary: row.get(6)?,
                findings: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_dependency_report(&self, row: &DependencyRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO dependency_reports (id, capability_id, version_id, status, risk, summary, items, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.id,
                row.capability_id,
                row.version_id,
                row.status,
                row.risk,
                row.summary,
                row.items,
                row.checked_at
            ],
        )?;
        Ok(())
    }

    pub fn latest_dependency_report(
        &self,
        capability_id: &str,
        version_id: &str,
    ) -> Result<Option<DependencyRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, capability_id, version_id, status, risk, summary, items, checked_at
                 FROM dependency_reports WHERE capability_id = ?1 AND version_id = ?2
                 ORDER BY checked_at DESC LIMIT 1",
                params![capability_id, version_id],
                |row| {
                    Ok(DependencyRow {
                        id: row.get(0)?,
                        capability_id: row.get(1)?,
                        version_id: row.get(2)?,
                        status: row.get(3)?,
                        risk: row.get(4)?,
                        summary: row.get(5)?,
                        items: row.get(6)?,
                        checked_at: row.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn insert_impact(&self, row: &ImpactRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO update_impacts (id, capability_id, from_version_id, to_version_id, risk, summary, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.id,
                row.capability_id,
                row.from_version_id,
                row.to_version_id,
                row.risk,
                row.summary,
                row.payload,
                row.created_at
            ],
        )?;
        Ok(())
    }

    pub fn latest_impact(
        &self,
        capability_id: &str,
        from_version_id: &str,
        to_version_id: &str,
    ) -> Result<Option<ImpactRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, capability_id, from_version_id, to_version_id, risk, summary, payload, created_at
                 FROM update_impacts
                 WHERE capability_id = ?1 AND from_version_id = ?2 AND to_version_id = ?3
                 ORDER BY created_at DESC LIMIT 1",
                params![capability_id, from_version_id, to_version_id],
                |row| {
                    Ok(ImpactRow {
                        id: row.get(0)?,
                        capability_id: row.get(1)?,
                        from_version_id: row.get(2)?,
                        to_version_id: row.get(3)?,
                        risk: row.get(4)?,
                        summary: row.get(5)?,
                        payload: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn insert_validation_run(&self, row: &ValidationRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO validation_runs (id, capability_id, version_id, agent_id, kind, status, case_name, notes, evidence, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.capability_id,
                row.version_id,
                row.agent_id,
                row.kind,
                row.status,
                row.case_name,
                row.notes,
                row.evidence,
                row.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_validation_runs(&self, capability_id: &str, limit: usize) -> Result<Vec<ValidationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, capability_id, version_id, agent_id, kind, status, case_name, notes, evidence, created_at
             FROM validation_runs WHERE capability_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![capability_id, limit as i64], |row| {
            Ok(ValidationRow {
                id: row.get(0)?,
                capability_id: row.get(1)?,
                version_id: row.get(2)?,
                agent_id: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                case_name: row.get(6)?,
                notes: row.get(7)?,
                evidence: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Record a detected agent version, keeping history. Returns the row written
    /// and whether the version differs from the most recent previous observation.
    pub fn record_agent_version(
        &self,
        agent_id: &str,
        version: Option<&str>,
        evidence: &str,
    ) -> Result<AgentVersionRow> {
        let previous: Option<(Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT version, detected_at FROM agent_versions WHERE agent_id = ?1
                 ORDER BY detected_at DESC, rowid DESC LIMIT 1",
                params![agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let previous_version = previous.as_ref().and_then(|(version, _)| version.clone());
        // The first observation is a baseline, not an upgrade.
        let changed = match (&previous_version, version) {
            (Some(old), Some(new)) => old != new,
            _ => false,
        };
        let row = AgentVersionRow {
            agent_id: agent_id.to_string(),
            version: version.map(str::to_string),
            previous_version,
            changed,
            evidence: evidence.to_string(),
            detected_at: now(),
        };
        self.conn.execute(
            "INSERT INTO agent_versions (id, agent_id, version, previous_version, changed, evidence, detected_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                crate::util::new_id(),
                row.agent_id,
                row.version,
                row.previous_version,
                if row.changed { 1 } else { 0 },
                row.evidence,
                row.detected_at
            ],
        )?;
        Ok(row)
    }

    pub fn latest_agent_version(&self, agent_id: &str) -> Result<Option<AgentVersionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT agent_id, version, previous_version, changed, evidence, detected_at
                 FROM agent_versions WHERE agent_id = ?1 ORDER BY detected_at DESC, rowid DESC LIMIT 1",
                params![agent_id],
                |row| {
                    Ok(AgentVersionRow {
                        agent_id: row.get(0)?,
                        version: row.get(1)?,
                        previous_version: row.get(2)?,
                        changed: row.get::<_, i64>(3)? != 0,
                        evidence: row.get(4)?,
                        detected_at: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn list_agent_version_changes(&self, agent_id: &str, limit: usize) -> Result<Vec<AgentVersionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, version, previous_version, changed, evidence, detected_at
             FROM agent_versions WHERE agent_id = ?1 ORDER BY detected_at DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![agent_id, limit as i64], |row| {
            Ok(AgentVersionRow {
                agent_id: row.get(0)?,
                version: row.get(1)?,
                previous_version: row.get(2)?,
                changed: row.get::<_, i64>(3)? != 0,
                evidence: row.get(4)?,
                detected_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
