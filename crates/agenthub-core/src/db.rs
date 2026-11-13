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

/// v3.1 Phase 1 safety baseline (additive; existing v1/v2 tables are kept).
/// Adds the approval/journal/evidence/audit primitives required by
/// F-02/F-06/F-07 without rewriting the 0.4.0 registry tables.
const SCHEMA_V3: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  checksum TEXT NOT NULL DEFAULT '',
  applied_at TEXT NOT NULL
);
INSERT OR IGNORE INTO schema_migrations(version, checksum, applied_at)
  VALUES (1, 'legacy-v1', datetime('now')), (2, 'legacy-v2', datetime('now'));

CREATE TABLE IF NOT EXISTS operation_steps (
  id TEXT PRIMARY KEY,
  operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
  sequence_no INTEGER NOT NULL,
  target_path TEXT NOT NULL,
  before_hash TEXT,
  after_hash TEXT,
  snapshot_ref TEXT,
  state TEXT NOT NULL CHECK (state IN ('pending','applying','applied','failed','skipped','recovered','blocked')),
  error_code TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  UNIQUE (operation_id, sequence_no)
);
CREATE INDEX IF NOT EXISTS idx_operation_steps_op ON operation_steps(operation_id, sequence_no);

CREATE TABLE IF NOT EXISTS adapter_evidence (
  id TEXT PRIMARY KEY,
  agent_key TEXT NOT NULL,
  feature_key TEXT NOT NULL,
  support_state TEXT NOT NULL CHECK (support_state IN
    ('verified_runtime','verified_config','documented','community','assumed',
     'unsupported','manual','unverified')),
  evidence_level TEXT NOT NULL,
  source_url TEXT,
  tested_at TEXT,
  agent_version TEXT,
  adapter_version TEXT NOT NULL,
  notes TEXT,
  UNIQUE (agent_key, feature_key, adapter_version)
);
CREATE INDEX IF NOT EXISTS idx_adapter_evidence_agent ON adapter_evidence(agent_key, feature_key);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  actor TEXT NOT NULL,
  event_key TEXT NOT NULL,
  operation_id TEXT REFERENCES operations(id),
  event_json_redacted TEXT NOT NULL,
  occurred_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_events_time ON audit_events(occurred_at);

INSERT OR IGNORE INTO schema_migrations(version, checksum, applied_at)
  VALUES (3, 'v31-safety-v1', datetime('now'));
"#;

/// SCHEMA_V4 (My AI Stack, additive-only). Detection facts and subscription
/// facts live in separate tables; subscription FKs never move on merge
/// (`stack_items.merged_into` is the only merge mechanism). Detections are
/// keyed by global-unique `adapter_key` and UPSERTed on refresh so row
/// identity is stable (P0-3). Amounts are integer minor units.
const SCHEMA_V4: &str = r#"
CREATE TABLE IF NOT EXISTS stack_items (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  vendor TEXT,
  source TEXT NOT NULL CHECK (source IN ('detected','manual')),
  detection_state TEXT NOT NULL DEFAULT 'unknown'
    CHECK (detection_state IN ('installed','not_installed','unknown')),
  confirmed INTEGER NOT NULL DEFAULT 0,
  last_confirmed_at TEXT,
  merged_into TEXT REFERENCES stack_items(id) ON DELETE SET NULL,
  overlap_group TEXT,
  notes TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_stack_items_merged ON stack_items(merged_into);
CREATE INDEX IF NOT EXISTS idx_stack_items_group ON stack_items(overlap_group);

CREATE TABLE IF NOT EXISTS stack_detections (
  id TEXT PRIMARY KEY,
  stack_item_id TEXT NOT NULL REFERENCES stack_items(id) ON DELETE CASCADE,
  adapter_key TEXT NOT NULL,
  agent_id TEXT REFERENCES agents(id) ON DELETE SET NULL,
  detected_status TEXT NOT NULL,
  evidence_json TEXT NOT NULL DEFAULT '[]',
  detected_at TEXT NOT NULL,
  UNIQUE (adapter_key)
);
CREATE INDEX IF NOT EXISTS idx_stack_detections_item ON stack_detections(stack_item_id);

CREATE TABLE IF NOT EXISTS subscription_records (
  id TEXT PRIMARY KEY,
  stack_item_id TEXT NOT NULL REFERENCES stack_items(id) ON DELETE RESTRICT,
  product_name TEXT NOT NULL,
  vendor TEXT,
  plan_name TEXT,
  billing_type TEXT NOT NULL CHECK (billing_type IN ('free','subscription','usage','hybrid','unknown')),
  amount_minor INTEGER,
  currency TEXT,
  billing_period TEXT CHECK (billing_period IN ('monthly','quarterly','semiannual','yearly','usage','unknown')),
  started_on TEXT,
  renews_on TEXT,
  status TEXT NOT NULL CHECK (status IN ('active','paused','cancelled','expired','unknown')),
  notes TEXT,
  info_source TEXT,
  last_confirmed_at TEXT,
  superseded_by TEXT REFERENCES subscription_records(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_subscriptions_item ON subscription_records(stack_item_id);
CREATE INDEX IF NOT EXISTS idx_subscriptions_status ON subscription_records(status);

INSERT OR IGNORE INTO schema_migrations(version, checksum, applied_at)
  VALUES (4, 'my-ai-stack-v1', datetime('now'));
"#;

/// `operations` columns added by v3. `ALTER TABLE ADD COLUMN` is not
/// idempotent, so each is applied only when absent (supports downgrade tests
/// and interrupted migrations).
const V3_OPERATION_COLUMNS: &[(&str, &str)] = &[
    ("plan_hash", "ALTER TABLE operations ADD COLUMN plan_hash TEXT"),
    (
        "approval_token_hash",
        "ALTER TABLE operations ADD COLUMN approval_token_hash TEXT",
    ),
    (
        "approval_expires_at",
        "ALTER TABLE operations ADD COLUMN approval_expires_at TEXT",
    ),
    (
        "approved_by",
        "ALTER TABLE operations ADD COLUMN approved_by TEXT",
    ),
    (
        "approved_at",
        "ALTER TABLE operations ADD COLUMN approved_at TEXT",
    ),
    (
        "idempotency_key",
        "ALTER TABLE operations ADD COLUMN idempotency_key TEXT",
    ),
    (
        "recovery_state",
        "ALTER TABLE operations ADD COLUMN recovery_state TEXT",
    ),
    (
        "approval_state",
        "ALTER TABLE operations ADD COLUMN approval_state TEXT NOT NULL DEFAULT 'none'",
    ),
];

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
    pub plan_hash: Option<String>,
    pub approval_token_hash: Option<String>,
    pub approval_expires_at: Option<String>,
    pub approved_by: Option<String>,
    pub approved_at: Option<String>,
    pub idempotency_key: Option<String>,
    pub recovery_state: Option<String>,
    pub approval_state: String,
}

#[derive(Debug, Clone)]
pub struct AdapterEvidenceRow {
    pub id: String,
    pub agent_key: String,
    pub feature_key: String,
    pub support_state: String,
    pub evidence_level: String,
    pub source_url: Option<String>,
    pub tested_at: Option<String>,
    pub agent_version: Option<String>,
    pub adapter_version: String,
    pub notes: Option<String>,
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
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 3 {
            // v3.1 rule: migration must be preceded by a recoverable DB copy.
            self.backup_before_migration(version, 3)?;
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA_V3)?;
            for (column, ddl) in V3_OPERATION_COLUMNS {
                let exists = tx
                    .prepare("PRAGMA table_info(operations)")?
                    .query_map([], |row| row.get::<_, String>(1))?
                    .flatten()
                    .any(|name| name == *column);
                if !exists {
                    tx.execute_batch(ddl)?;
                }
            }
            tx.pragma_update(None, "user_version", 3)?;
            tx.commit()?;
            let integrity = self.integrity_check()?;
            if !integrity.ok {
                return Err(Error::Verification(format!(
                    "database integrity check failed after v3 migration: {}",
                    integrity.details.join("; ")
                )));
            }
        }
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 4 {
            // v3.1 rule: migration must be preceded by a recoverable DB copy
            // (P0-5). The pre-migration backup is the only sanctioned recovery
            // path; git-style downgrade of the schema is not supported.
            self.backup_before_migration(version, 4)?;
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA_V4)?;
            tx.pragma_update(None, "user_version", 4)?;
            tx.commit()?;
            let integrity = self.integrity_check()?;
            if !integrity.ok {
                return Err(Error::Verification(format!(
                    "database integrity check failed after v4 migration: {}",
                    integrity.details.join("; ")
                )));
            }
        }
        Ok(())
    }

    /// Best-effort but mandatory-before-migration snapshot. The backup lives in
    /// `<data>/backups/pre-migration-v<from>-<timestamp>/` and includes a
    /// manifest with the source/target schema version and sha256.
    fn backup_before_migration(&self, from_version: i64, to_version: i64) -> Result<PathBuf> {
        if self.path == Path::new(":memory:") {
            return Ok(PathBuf::new());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Error::msg("database path has no parent directory"))?;
        let unique = crate::util::new_id();
        let dir = parent.join("backups").join(format!(
            "pre-migration-v{}-{}-{}",
            from_version,
            crate::util::now_for_path(),
            &unique[..8]
        ));
        std::fs::create_dir_all(&dir)?;
        let target = dir.join("agenthub.db");
        let sql = format!(
            "VACUUM INTO '{}'",
            target.to_string_lossy().replace('\'', "''")
        );
        self.conn.execute_batch(&sql)?;
        let sha256 = crate::util::hash_file(&target)?;
        let manifest = serde_json::json!({
            "kind": "pre-migration-backup",
            "from_version": from_version,
            "to_version": to_version,
            "created_at": crate::util::now(),
            "database": target.to_string_lossy(),
            "sha256": sha256,
            "app_version": env!("CARGO_PKG_VERSION"),
        });
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(dir)
    }

    /// `PRAGMA integrity_check` + `PRAGMA foreign_key_check`.
    pub fn integrity_check(&self) -> Result<IntegrityReport> {
        let mut details = Vec::new();
        let mut ok = true;
        let integrity: String =
            self.conn
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            ok = false;
            details.push(format!("integrity_check: {integrity}"));
        }
        let mut stmt = self.conn.prepare("PRAGMA foreign_key_check")?;
        let rows = stmt.query_map([], |row| {
            Ok(format!(
                "table={} rowid={:?} parent={}",
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?
            ))
        })?;
        for row in rows {
            ok = false;
            details.push(format!("foreign_key_check: {}", row?));
        }
        let schema_version: i64 =
            self.conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(IntegrityReport {
            ok,
            schema_version,
            details,
        })
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

    /// Run `body` inside one SQLite transaction with `&Self`-style helpers:
    /// `body` receives a callable that writes settings through the same
    /// statement as [`Self::set_setting`], and everything commits atomically
    /// or not at all (used by the optimizer settings save, M2-B).
    pub fn with_settings_transaction<F>(&self, body: F) -> Result<()>
    where
        F: FnOnce(&dyn Fn(&str, &str) -> Result<()>) -> Result<()>,
    {
        let tx = self.conn.unchecked_transaction()?;
        let writer = |key: &str, value: &str| -> Result<()> {
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now()],
            )?;
            Ok(())
        };
        body(&writer)?;
        tx.commit()?;
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
            plan_hash: row.get(12)?,
            approval_token_hash: row.get(13)?,
            approval_expires_at: row.get(14)?,
            approved_by: row.get(15)?,
            approved_at: row.get(16)?,
            idempotency_key: row.get(17)?,
            recovery_state: row.get(18)?,
            approval_state: row.get(19)?,
        })
    }

    const OPERATION_COLUMNS: &'static str = "id, kind, status, dry_run, actor, summary, plan, result, error, rollback_of, started_at, finished_at, plan_hash, approval_token_hash, approval_expires_at, approved_by, approved_at, idempotency_key, recovery_state, approval_state";

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
        let sql = format!(
            "SELECT {} FROM operations WHERE id = ?1",
            Self::OPERATION_COLUMNS
        );
        Ok(self
            .conn
            .query_row(&sql, params![id], Self::operation_from_row)
            .optional()?)
    }

    pub fn list_operations(&self, limit: usize) -> Result<Vec<OperationRow>> {
        let sql = format!(
            "SELECT {} FROM operations ORDER BY started_at DESC LIMIT ?1",
            Self::OPERATION_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], Self::operation_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Persist the semantic plan hash and approval deadline on creation.
    pub fn set_operation_plan_meta(
        &self,
        id: &str,
        plan_hash: &str,
        expires_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE operations SET plan_hash = ?2, approval_expires_at = ?3 WHERE id = ?1",
            params![id, plan_hash, expires_at],
        )?;
        Ok(())
    }

    /// Store a hashed one-time approval token. The clear-text token is never
    /// written to the database or to logs.
    pub fn set_operation_approval(
        &self,
        id: &str,
        token_hash: &str,
        approved_by: &str,
        approved_at: &str,
        expires_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE operations SET approval_token_hash = ?2, approved_by = ?3,
             approved_at = ?4, approval_expires_at = ?5, approval_state = 'approved'
             WHERE id = ?1",
            params![id, token_hash, approved_by, approved_at, expires_at],
        )?;
        Ok(())
    }

    pub fn set_operation_state(
        &self,
        id: &str,
        status: &str,
        recovery_state: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE operations SET status = ?2,
             recovery_state = COALESCE(?3, recovery_state),
             idempotency_key = COALESCE(?4, idempotency_key)
             WHERE id = ?1",
            params![id, status, recovery_state, idempotency_key],
        )?;
        Ok(())
    }

    pub fn find_operation_by_idempotency(&self, key: &str) -> Result<Option<OperationRow>> {
        let sql = format!(
            "SELECT {} FROM operations WHERE idempotency_key = ?1 ORDER BY started_at DESC LIMIT 1",
            Self::OPERATION_COLUMNS
        );
        Ok(self
            .conn
            .query_row(&sql, params![key], Self::operation_from_row)
            .optional()?)
    }

    // -- operation journal ------------------------------------------------

    pub fn insert_operation_step(
        &self,
        operation_id: &str,
        sequence_no: i64,
        target_path: &str,
        before_hash: Option<&str>,
        state: &str,
    ) -> Result<String> {
        let id = crate::util::new_id();
        self.conn.execute(
            "INSERT INTO operation_steps
             (id, operation_id, sequence_no, target_path, before_hash, state, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                operation_id,
                sequence_no,
                target_path,
                before_hash,
                state,
                crate::util::now()
            ],
        )?;
        Ok(id)
    }

    pub fn finish_operation_step(
        &self,
        id: &str,
        after_hash: Option<&str>,
        snapshot_ref: Option<&str>,
        state: &str,
        error_code: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE operation_steps SET after_hash = ?2, snapshot_ref = ?3, state = ?4,
             error_code = ?5, finished_at = ?6 WHERE id = ?1",
            params![id, after_hash, snapshot_ref, state, error_code, crate::util::now()],
        )?;
        Ok(())
    }

    pub fn list_operation_steps(&self, operation_id: &str) -> Result<Vec<OperationStepView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, operation_id, sequence_no, target_path, before_hash, after_hash,
                    snapshot_ref, state, error_code, started_at, finished_at
             FROM operation_steps WHERE operation_id = ?1 ORDER BY sequence_no",
        )?;
        let rows = stmt.query_map(params![operation_id], |row| {
            Ok(OperationStepView {
                id: row.get(0)?,
                operation_id: row.get(1)?,
                sequence_no: row.get(2)?,
                target_path: row.get(3)?,
                before_hash: row.get(4)?,
                after_hash: row.get(5)?,
                snapshot_ref: row.get(6)?,
                state: row.get(7)?,
                error_code: row.get(8)?,
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // -- adapter evidence -------------------------------------------------

    pub fn upsert_adapter_evidence(&self, row: &AdapterEvidenceRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO adapter_evidence
             (id, agent_key, feature_key, support_state, evidence_level, source_url,
              tested_at, agent_version, adapter_version, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(agent_key, feature_key, adapter_version) DO UPDATE SET
               support_state = excluded.support_state,
               evidence_level = excluded.evidence_level,
               source_url = excluded.source_url,
               tested_at = excluded.tested_at,
               agent_version = excluded.agent_version,
               notes = excluded.notes",
            params![
                row.id,
                row.agent_key,
                row.feature_key,
                row.support_state,
                row.evidence_level,
                row.source_url,
                row.tested_at,
                row.agent_version,
                row.adapter_version,
                row.notes
            ],
        )?;
        Ok(())
    }

    pub fn list_adapter_evidence(&self, agent_key: &str) -> Result<Vec<AdapterEvidenceView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent_key, feature_key, support_state, evidence_level, source_url,
                    tested_at, agent_version, adapter_version, notes
             FROM adapter_evidence WHERE agent_key = ?1 ORDER BY feature_key",
        )?;
        let rows = stmt.query_map(params![agent_key], |row| {
            Ok(AdapterEvidenceView {
                id: row.get(0)?,
                agent_key: row.get(1)?,
                feature_key: row.get(2)?,
                support_state: row.get(3)?,
                evidence_level: row.get(4)?,
                source_url: row.get(5)?,
                tested_at: row.get(6)?,
                agent_version: row.get(7)?,
                adapter_version: row.get(8)?,
                notes: row.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_audit_event(
        &self,
        actor: &str,
        event_key: &str,
        operation_id: Option<&str>,
        detail: &serde_json::Value,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_events (id, actor, event_key, operation_id, event_json_redacted, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                crate::util::new_id(),
                actor,
                event_key,
                operation_id,
                detail.to_string(),
                crate::util::now()
            ],
        )?;
        Ok(())
    }

    /// Read audit events newest-first, newest `limit` records. Used by the My
    /// AI Stack tests and later by the audit trail UI.
    pub fn list_audit_events(&self, limit: usize) -> Result<Vec<AuditEventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, actor, event_key, operation_id, event_json_redacted, occurred_at
             FROM audit_events ORDER BY occurred_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(AuditEventRow {
                id: row.get(0)?,
                actor: row.get(1)?,
                event_key: row.get(2)?,
                operation_id: row.get(3)?,
                event_json_redacted: row.get(4)?,
                occurred_at: row.get(5)?,
            })
        })?;
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

// ---------------------------------------------------------------------------
// My AI Stack (M1-A): stack_items / stack_detections / subscription_records.
// Storage structs are snake_case column mirrors; views live in `model.rs`.
// ---------------------------------------------------------------------------

/// One row of the `audit_events` table (D-1 audit trail carrier).
#[derive(Debug, Clone)]
pub struct AuditEventRow {
    pub id: String,
    pub actor: String,
    pub event_key: String,
    pub operation_id: Option<String>,
    pub event_json_redacted: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone)]
pub struct StackItemRow {
    pub id: String,
    pub name: String,
    pub vendor: Option<String>,
    pub source: String,
    pub detection_state: String,
    pub confirmed: bool,
    pub last_confirmed_at: Option<String>,
    pub merged_into: Option<String>,
    pub overlap_group: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StackDetectionRow {
    pub id: String,
    pub stack_item_id: String,
    pub adapter_key: String,
    pub agent_id: Option<String>,
    pub detected_status: String,
    pub evidence_json: String,
    pub detected_at: String,
}

#[derive(Debug, Clone)]
pub struct SubscriptionRow {
    pub id: String,
    pub stack_item_id: String,
    pub product_name: String,
    pub vendor: Option<String>,
    pub plan_name: Option<String>,
    pub billing_type: String,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub billing_period: Option<String>,
    pub started_on: Option<String>,
    pub renews_on: Option<String>,
    pub status: String,
    pub notes: Option<String>,
    pub info_source: Option<String>,
    pub last_confirmed_at: Option<String>,
    pub superseded_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const STACK_ITEM_COLUMNS: &str = "id, name, vendor, source, detection_state, confirmed, last_confirmed_at, merged_into, overlap_group, notes, created_at, updated_at";

/// Shared SQL for the refresh transaction and the standalone accessors, so
/// the two paths can never drift.
const SQL_INSERT_STACK_ITEM: &str =
    "INSERT INTO stack_items (id, name, vendor, source, detection_state, confirmed, last_confirmed_at, merged_into, overlap_group, notes, created_at, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";
const SQL_UPSERT_STACK_DETECTION: &str =
    "INSERT INTO stack_detections (id, stack_item_id, adapter_key, agent_id, detected_status, evidence_json, detected_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
     ON CONFLICT(adapter_key) DO UPDATE SET
       stack_item_id = excluded.stack_item_id,
       agent_id = excluded.agent_id,
       detected_status = excluded.detected_status,
       evidence_json = excluded.evidence_json,
       detected_at = excluded.detected_at";
const SQL_SET_STACK_ITEM_DETECTION_STATE: &str =
    "UPDATE stack_items SET detection_state = ?2, updated_at = ?3 WHERE id = ?1";

/// One detection-refresh write plan applied atomically (frozen I2a rule:
/// refresh runs inside a single transaction).
pub struct StackRefreshPlan {
    pub item_inserts: Vec<StackItemRow>,
    pub detection_upserts: Vec<StackDetectionRow>,
    /// Detection-state re-derivations for existing items (id, state).
    pub state_updates: Vec<(String, String)>,
}

impl Database {
    /// Applies the refresh plan inside one transaction. Called by
    /// `AgentHub::stack_refresh_detections` only.
    pub fn apply_stack_refresh(&self, plan: &StackRefreshPlan) -> Result<()> {
        let now = crate::util::now();
        let tx = self.conn.unchecked_transaction()?;
        for item in &plan.item_inserts {
            tx.execute(
                SQL_INSERT_STACK_ITEM,
                params![
                    item.id,
                    item.name,
                    item.vendor,
                    item.source,
                    item.detection_state,
                    item.confirmed as i64,
                    item.last_confirmed_at,
                    item.merged_into,
                    item.overlap_group,
                    item.notes,
                    item.created_at,
                    item.updated_at,
                ],
            )?;
        }
        for detection in &plan.detection_upserts {
            tx.execute(
                SQL_UPSERT_STACK_DETECTION,
                params![
                    detection.id,
                    detection.stack_item_id,
                    detection.adapter_key,
                    detection.agent_id,
                    detection.detected_status,
                    detection.evidence_json,
                    detection.detected_at,
                ],
            )?;
        }
        for (id, state) in &plan.state_updates {
            tx.execute(SQL_SET_STACK_ITEM_DETECTION_STATE, params![id, state, now])?;
        }
        tx.commit()?;
        Ok(())
    }
    fn stack_item_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<StackItemRow> {
        Ok(StackItemRow {
            id: row.get(0)?,
            name: row.get(1)?,
            vendor: row.get(2)?,
            source: row.get(3)?,
            detection_state: row.get(4)?,
            confirmed: row.get::<_, i64>(5)? != 0,
            last_confirmed_at: row.get(6)?,
            merged_into: row.get(7)?,
            overlap_group: row.get(8)?,
            notes: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }

    pub fn insert_stack_item(&self, item: &StackItemRow) -> Result<()> {
        self.conn.execute(
            SQL_INSERT_STACK_ITEM,
            params![
                item.id,
                item.name,
                item.vendor,
                item.source,
                item.detection_state,
                item.confirmed as i64,
                item.last_confirmed_at,
                item.merged_into,
                item.overlap_group,
                item.notes,
                item.created_at,
                item.updated_at,
            ],
        )?;
        Ok(())
    }

    /// User-editable fields only: source, detection_state and confirmed are
    /// system-owned and never written here.
    pub fn update_stack_item_fields(
        &self,
        id: &str,
        name: &str,
        vendor: Option<&str>,
        overlap_group: Option<&str>,
        notes: Option<&str>,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE stack_items SET name = ?2, vendor = ?3, overlap_group = ?4, notes = ?5, updated_at = ?6 WHERE id = ?1",
            params![id, name, vendor, overlap_group, notes, crate::util::now()],
        )?;
        Ok(changed > 0)
    }

    pub fn set_stack_item_confirmed(&self, id: &str, confirmed_at: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE stack_items SET confirmed = 1, last_confirmed_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, confirmed_at],
        )?;
        Ok(changed > 0)
    }

    /// Merge/unmerge: the single, reversible mechanism. Subscription FKs never
    /// move (P0-2).
    pub fn set_stack_item_merged_into(&self, id: &str, target: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE stack_items SET merged_into = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, target, crate::util::now()],
        )?;
        Ok(())
    }

    /// System-owned detection state re-derivation during refresh.
    pub fn set_stack_item_detection_state(&self, id: &str, state: &str) -> Result<()> {
        self.conn.execute(
            SQL_SET_STACK_ITEM_DETECTION_STATE,
            params![id, state, crate::util::now()],
        )?;
        Ok(())
    }

    pub fn delete_stack_item(&self, id: &str) -> Result<bool> {
        let changed = self.conn.execute("DELETE FROM stack_items WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn get_stack_item(&self, id: &str) -> Result<Option<StackItemRow>> {
        let sql = format!("SELECT {STACK_ITEM_COLUMNS} FROM stack_items WHERE id = ?1");
        Ok(self
            .conn
            .query_row(&sql, params![id], |row| self.stack_item_from_row(row))
            .optional()?)
    }

    pub fn list_stack_items(&self) -> Result<Vec<StackItemRow>> {
        let sql = format!(
            "SELECT {STACK_ITEM_COLUMNS} FROM stack_items ORDER BY created_at ASC, id ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| self.stack_item_from_row(row))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn count_stack_item_children(&self, id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM stack_items WHERE merged_into = ?1",
                params![id],
                |row| row.get(0),
            )?)
    }

    pub fn count_subscriptions_for_item(&self, id: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM subscription_records WHERE stack_item_id = ?1",
                params![id],
                |row| row.get(0),
            )?)
    }

    fn detection_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<StackDetectionRow> {
        Ok(StackDetectionRow {
            id: row.get(0)?,
            stack_item_id: row.get(1)?,
            adapter_key: row.get(2)?,
            agent_id: row.get(3)?,
            detected_status: row.get(4)?,
            evidence_json: row.get(5)?,
            detected_at: row.get(6)?,
        })
    }

    /// Stable-identity upsert keyed by the globally unique adapter_key
    /// (P0-3/I2a). On conflict only the attach target, agent link, observed
    /// status and evidence are refreshed; row id and detected row identity
    /// stay stable across refreshes.
    pub fn upsert_stack_detection(&self, detection: &StackDetectionRow) -> Result<()> {
        self.conn.execute(
            SQL_UPSERT_STACK_DETECTION,
            params![
                detection.id,
                detection.stack_item_id,
                detection.adapter_key,
                detection.agent_id,
                detection.detected_status,
                detection.evidence_json,
                detection.detected_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_stack_detection_by_adapter(&self, adapter_key: &str) -> Result<Option<StackDetectionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, stack_item_id, adapter_key, agent_id, detected_status, evidence_json, detected_at
                 FROM stack_detections WHERE adapter_key = ?1",
                params![adapter_key],
                |row| self.detection_from_row(row),
            )
            .optional()?)
    }

    pub fn list_stack_detections(&self) -> Result<Vec<StackDetectionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, stack_item_id, adapter_key, agent_id, detected_status, evidence_json, detected_at
             FROM stack_detections ORDER BY adapter_key ASC",
        )?;
        let rows = stmt.query_map([], |row| self.detection_from_row(row))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_stack_detections_for_item(&self, stack_item_id: &str) -> Result<Vec<StackDetectionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, stack_item_id, adapter_key, agent_id, detected_status, evidence_json, detected_at
             FROM stack_detections WHERE stack_item_id = ?1 ORDER BY adapter_key ASC",
        )?;
        let rows = stmt.query_map(params![stack_item_id], |row| self.detection_from_row(row))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn subscription_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<SubscriptionRow> {
        Ok(SubscriptionRow {
            id: row.get(0)?,
            stack_item_id: row.get(1)?,
            product_name: row.get(2)?,
            vendor: row.get(3)?,
            plan_name: row.get(4)?,
            billing_type: row.get(5)?,
            amount_minor: row.get(6)?,
            currency: row.get(7)?,
            billing_period: row.get(8)?,
            started_on: row.get(9)?,
            renews_on: row.get(10)?,
            status: row.get(11)?,
            notes: row.get(12)?,
            info_source: row.get(13)?,
            last_confirmed_at: row.get(14)?,
            superseded_by: row.get(15)?,
            created_at: row.get(16)?,
            updated_at: row.get(17)?,
        })
    }

    pub fn insert_subscription(&self, sub: &SubscriptionRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO subscription_records (id, stack_item_id, product_name, vendor, plan_name, billing_type, amount_minor, currency, billing_period, started_on, renews_on, status, notes, info_source, last_confirmed_at, superseded_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                sub.id,
                sub.stack_item_id,
                sub.product_name,
                sub.vendor,
                sub.plan_name,
                sub.billing_type,
                sub.amount_minor,
                sub.currency,
                sub.billing_period,
                sub.started_on,
                sub.renews_on,
                sub.status,
                sub.notes,
                sub.info_source,
                sub.last_confirmed_at,
                sub.superseded_by,
                sub.created_at,
                sub.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Business-column update; created_at is preserved. superseded_by is
    /// system-owned (M2 change chain) and never written here.
    pub fn update_subscription(&self, sub: &SubscriptionRow) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE subscription_records SET stack_item_id = ?2, product_name = ?3, vendor = ?4, plan_name = ?5, billing_type = ?6, amount_minor = ?7, currency = ?8, billing_period = ?9, started_on = ?10, renews_on = ?11, status = ?12, notes = ?13, info_source = ?14, last_confirmed_at = ?15, updated_at = ?16 WHERE id = ?1",
            params![
                sub.id,
                sub.stack_item_id,
                sub.product_name,
                sub.vendor,
                sub.plan_name,
                sub.billing_type,
                sub.amount_minor,
                sub.currency,
                sub.billing_period,
                sub.started_on,
                sub.renews_on,
                sub.status,
                sub.notes,
                sub.info_source,
                sub.last_confirmed_at,
                sub.updated_at,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_subscription(&self, id: &str) -> Result<bool> {
        let changed =
            self.conn
                .execute("DELETE FROM subscription_records WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn get_subscription(&self, id: &str) -> Result<Option<SubscriptionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, stack_item_id, product_name, vendor, plan_name, billing_type, amount_minor, currency, billing_period, started_on, renews_on, status, notes, info_source, last_confirmed_at, superseded_by, created_at, updated_at
                 FROM subscription_records WHERE id = ?1",
                params![id],
                |row| self.subscription_from_row(row),
            )
            .optional()?)
    }

    pub fn list_subscriptions(&self) -> Result<Vec<SubscriptionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, stack_item_id, product_name, vendor, plan_name, billing_type, amount_minor, currency, billing_period, started_on, renews_on, status, notes, info_source, last_confirmed_at, superseded_by, created_at, updated_at
             FROM subscription_records ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| self.subscription_from_row(row))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
