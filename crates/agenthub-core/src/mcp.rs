//! MCP configuration engine.
//!
//! AgentHub keeps an agent-neutral MCP definition and converts it per target
//! client. JSON files are edited in place with `serde_json` (ordering
//! preserved); TOML files are edited with `toml_edit` so unrelated settings,
//! comments and formatting survive. Every write is preceded by a snapshot in
//! `ops`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use toml_edit::{DocumentMut, Item, Table};

use crate::adapters::{find_adapter, McpTargetSpec};
use crate::db::{Database, InstallationRow, McpRow};
use crate::error::{Error, Result};
use crate::model::*;
use crate::service::AgentHub;
use crate::util;

const KEYCHAIN_SERVICE: &str = "com.agenthub.mcp";
const SUPPORTED_FORMATS: [&str; 2] = ["json:", "toml:mcp_servers"];

// ---------------------------------------------------------------------------
// Secret storage (macOS Keychain via the `security` CLI)
// ---------------------------------------------------------------------------

fn security() -> Result<PathBuf> {
    let path = PathBuf::from("/usr/bin/security");
    if !path.exists() {
        return Err(Error::unsupported(
            "macOS Keychain CLI (/usr/bin/security) is not available",
        ));
    }
    Ok(path)
}

fn account(server_id: &str, key: &str) -> String {
    format!("{server_id}/{key}")
}

pub fn set_secret(server_id: &str, key: &str, value: &str) -> Result<()> {
    let security = security()?;
    let status = std::process::Command::new(security)
        .args([
            "add-generic-password",
            "-U",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            &account(server_id, key),
            "-w",
            value,
        ])
        .status()?;
    if !status.success() {
        return Err(Error::msg(
            "could not store the value in macOS Keychain",
        ));
    }
    Ok(())
}

pub fn get_secret(server_id: &str, key: &str) -> Option<String> {
    let security = security().ok()?;
    let output = std::process::Command::new(security)
        .args([
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            &account(server_id, key),
            "-w",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub fn delete_secret(server_id: &str, key: &str) -> Result<()> {
    let security = security()?;
    let _ = std::process::Command::new(security)
        .args([
            "delete-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            &account(server_id, key),
        ])
        .status()?;
    Ok(())
}

pub fn secret_present(server_id: &str, key: &str) -> bool {
    get_secret(server_id, key).is_some()
}

// ---------------------------------------------------------------------------
// Configuration file parsing / writing
// ---------------------------------------------------------------------------

fn supported_format(format: &str) -> bool {
    SUPPORTED_FORMATS.contains(&format) || format.starts_with("json:")
}

fn json_path_segments(format: &str) -> Vec<&str> {
    format
        .strip_prefix("json:")
        .unwrap_or("mcpServers")
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn nested_object_mut<'a>(
    root: &'a mut Value,
    segments: &[&str],
) -> Result<&'a mut serde_json::Map<String, Value>> {
    let mut current = root
        .as_object_mut()
        .ok_or_else(|| Error::msg("MCP config root is not a JSON object"))?;
    for segment in segments {
        let entry = current
            .entry(*segment)
            .or_insert_with(|| json!({}));
        current = entry
            .as_object_mut()
            .ok_or_else(|| Error::msg(format!("'{segment}' is not an object")))?;
    }
    Ok(current)
}

pub fn read_entries(path: &Path, format: &str) -> Result<Vec<(String, Value)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| Error::msg(format!("cannot read {}: {err}", path.display())))?;
    match format {
        format if format.starts_with("json:") => {
            if text.trim().is_empty() {
                return Ok(Vec::new());
            }
            let root: Value = serde_json::from_str(&text)
                .map_err(|err| Error::msg(format!("invalid JSON in {}: {err}", path.display())))?;
            let mut out = Vec::new();
            let mut current = &root;
            let mut missing = false;
            for segment in json_path_segments(format) {
                match current.get(segment) {
                    Some(value) => current = value,
                    None => {
                        missing = true;
                        break;
                    }
                }
            }
            if !missing {
                if let Some(servers) = current.as_object() {
                    for (name, entry) in servers {
                        out.push((name.clone(), entry.clone()));
                    }
                }
            }
            out.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(out)
        }
        "toml:mcp_servers" => {
            let doc: DocumentMut = text
                .parse()
                .map_err(|err| Error::msg(format!("invalid TOML in {}: {err}", path.display())))?;
            let mut out = Vec::new();
            if let Some(table) = doc.get("mcp_servers").and_then(Item::as_table_like) {
                for (name, item) in table.iter() {
                    out.push((name.to_string(), item_to_json(item)));
                }
            }
            out.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(out)
        }
        other => Err(Error::unsupported(format!(
            "unsupported MCP config format '{other}'"
        ))),
    }
}

pub fn read_entry(path: &Path, format: &str, name: &str) -> Result<Option<Value>> {
    Ok(read_entries(path, format)?
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value))
}

fn write_entries(path: &Path, format: &str, entries: &[(String, Value)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match format {
        format if format.starts_with("json:") => {
            let mut root = if path.exists() {
                let text = std::fs::read_to_string(path)?;
                if text.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&text).map_err(|err| {
                        Error::msg(format!("refusing to overwrite invalid JSON {}: {err}", path.display()))
                    })?
                }
            } else {
                json!({})
            };
            let segments = json_path_segments(format);
            let servers = nested_object_mut(&mut root, &segments)?;
            let desired: std::collections::HashSet<&str> =
                entries.iter().map(|(name, _)| name.as_str()).collect();
            servers.retain(|name, _| desired.contains(name.as_str()));
            for (name, entry) in entries {
                servers.insert(name.clone(), entry.clone());
            }
            let mut text = serde_json::to_string_pretty(&root)?;
            text.push('\n');
            util::atomic_write(path, text.as_bytes())
        }
        "toml:mcp_servers" => {
            let mut doc: DocumentMut = if path.exists() {
                let text = std::fs::read_to_string(path)?;
                if text.trim().is_empty() {
                    DocumentMut::new()
                } else {
                    text.parse().map_err(|err| {
                        Error::msg(format!("refusing to overwrite invalid TOML {}: {err}", path.display()))
                    })?
                }
            } else {
                DocumentMut::new()
            };
            if !doc.contains_key("mcp_servers") {
                doc["mcp_servers"] = Item::Table(Table::new());
            }
            let desired: std::collections::HashSet<&str> =
                entries.iter().map(|(name, _)| name.as_str()).collect();
            let table = doc["mcp_servers"]
                .as_table_like_mut()
                .ok_or_else(|| Error::msg("mcp_servers is not a table"))?;
            let stale: Vec<String> = table
                .iter()
                .map(|(name, _)| name.to_string())
                .filter(|name| !desired.contains(name.as_str()))
                .collect();
            for name in stale {
                table.remove(&name);
            }
            for (name, entry) in entries {
                let unchanged = table
                    .get(name)
                    .map(|item| entry_hash(&item_to_json(item)) == entry_hash(entry))
                    .unwrap_or(false);
                if !unchanged {
                    table.insert(name, json_to_item(entry));
                }
            }
            util::atomic_write(path, doc.to_string().as_bytes())
        }
        other => Err(Error::unsupported(format!(
            "unsupported MCP config format '{other}'"
        ))),
    }
}

pub fn remove_entry(path: &Path, format: &str, name: &str) -> Result<()> {
    let entries: Vec<(String, Value)> = read_entries(path, format)?
        .into_iter()
        .filter(|(key, _)| key != name)
        .collect();
    write_entries(path, format, &entries)
}

fn item_to_json(item: &Item) -> Value {
    if let Some(value) = item.as_str() {
        json!(value)
    } else if let Some(value) = item.as_integer() {
        json!(value)
    } else if let Some(value) = item.as_bool() {
        json!(value)
    } else if let Some(value) = item.as_float() {
        json!(value)
    } else if let Some(array) = item.as_array() {
        Value::Array(array.iter().map(toml_value_to_json).collect())
    } else if let Some(table) = item.as_table_like() {
        let mut map = serde_json::Map::new();
        for (key, value) in table.iter() {
            map.insert(key.to_string(), item_to_json(value));
        }
        Value::Object(map)
    } else {
        Value::Null
    }
}

fn toml_value_to_json(value: &toml_edit::Value) -> Value {
    if let Some(value) = value.as_str() {
        json!(value)
    } else if let Some(value) = value.as_integer() {
        json!(value)
    } else if let Some(value) = value.as_bool() {
        json!(value)
    } else if let Some(value) = value.as_float() {
        json!(value)
    } else if let Some(array) = value.as_array() {
        Value::Array(array.iter().map(toml_value_to_json).collect())
    } else if let Some(table) = value.as_inline_table() {
        let mut map = serde_json::Map::new();
        for (key, value) in table.iter() {
            map.insert(key.to_string(), toml_value_to_json(value));
        }
        Value::Object(map)
    } else {
        Value::Null
    }
}

fn json_to_item(value: &Value) -> Item {
    match value {
        Value::String(text) => Item::Value(text.clone().into()),
        Value::Bool(flag) => Item::Value((*flag).into()),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                Item::Value(integer.into())
            } else if let Some(float) = number.as_f64() {
                Item::Value(float.into())
            } else {
                Item::Value(number.to_string().into())
            }
        }
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                match json_to_item(item) {
                    Item::Value(value) => array.push(value),
                    _ => {}
                }
            }
            Item::Value(toml_edit::Value::Array(array))
        }
        Value::Object(map) => {
            let mut table = Table::new();
            for (key, value) in map {
                table[key.as_str()] = json_to_item(value);
            }
            Item::Table(table)
        }
        Value::Null => Item::None,
    }
}

// ---------------------------------------------------------------------------
// Entry model
// ---------------------------------------------------------------------------

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for key in keys {
                out.insert(key.clone(), canonical_json(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

pub fn entry_hash(value: &Value) -> String {
    let text = serde_json::to_string(&canonical_json(value)).unwrap_or_default();
    util::sha256_hex(text.as_bytes())
}

fn infer_transport(entry: &Value) -> McpTransport {
    if entry.get("command").and_then(Value::as_str).is_some() {
        return McpTransport::Stdio;
    }
    let kind = entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if kind.contains("sse") {
        McpTransport::Sse
    } else if kind.contains("streamable") {
        McpTransport::StreamableHttp
    } else {
        McpTransport::Http
    }
}

fn entry_strings(entry: &Value, key: &str) -> Vec<String> {
    entry
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn entry_map(entry: &Value, key: &str) -> BTreeMap<String, String> {
    entry
        .get(key)
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn type_for(format: &str, transport: McpTransport) -> &'static str {
    match (format, transport) {
        ("toml:mcp_servers", _) => "streamable-http",
        (_, McpTransport::Sse) => "sse",
        (_, McpTransport::StreamableHttp) => "streamable-http",
        _ => "http",
    }
}

fn build_entry(row: &McpRow, env_values: &BTreeMap<String, String>, format: &str) -> Result<Value> {
    let transport = McpTransport::parse(&row.transport);
    let mut entry = serde_json::Map::new();
    match transport {
        McpTransport::Stdio => {
            let command = row
                .command
                .as_deref()
                .filter(|command| !command.trim().is_empty())
                .ok_or_else(|| Error::msg("stdio MCP server has no command"))?;
            entry.insert("command".to_string(), json!(command));
            if !row.args.is_empty() {
                entry.insert("args".to_string(), json!(row.args));
            }
        }
        McpTransport::Http | McpTransport::Sse | McpTransport::StreamableHttp => {
            let url = row
                .url
                .as_deref()
                .filter(|url| !url.trim().is_empty())
                .ok_or_else(|| Error::msg("HTTP MCP server has no URL"))?;
            entry.insert("url".to_string(), json!(url));
            entry.insert("type".to_string(), json!(type_for(format, transport)));
            if !row.headers.is_empty() {
                let headers: serde_json::Map<String, Value> = row
                    .headers
                    .iter()
                    .map(|(key, value)| (key.clone(), json!(value)))
                    .collect();
                entry.insert("headers".to_string(), Value::Object(headers));
            }
        }
    }
    if !row.env_keys.is_empty() {
        let mut env = serde_json::Map::new();
        for key in &row.env_keys {
            if let Some(value) = env_values.get(key) {
                env.insert(key.clone(), json!(value));
            }
        }
        if !env.is_empty() {
            entry.insert("env".to_string(), Value::Object(env));
        }
    }
    Ok(Value::Object(entry))
}

// ---------------------------------------------------------------------------
// AgentHub service integration
// ---------------------------------------------------------------------------

fn agent_map(hub: &AgentHub) -> Result<std::collections::HashMap<String, AgentView>> {
    Ok(hub
        .list_agents()?
        .into_iter()
        .map(|agent| (agent.id.clone(), agent))
        .collect())
}

pub fn list_servers(hub: &AgentHub) -> Result<Vec<McpServerView>> {
    let rows = hub.db_ref().list_mcp()?;
    let agents = agent_map(hub)?;
    let installations = hub
        .db_ref()
        .list_installations()?
        .into_iter()
        .filter(|row| row.kind == "mcp")
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for row in &rows {
        let mut distributions = Vec::new();
        for installation in &installations {
            let metadata: Value =
                serde_json::from_str(&installation.metadata).unwrap_or_else(|_| json!({}));
            if metadata.get("serverId").and_then(Value::as_str) != Some(row.id.as_str()) {
                continue;
            }
            let agent = agents.get(&installation.agent_id);
            distributions.push(McpDistribution {
                installation_id: Some(installation.id.clone()),
                agent_id: installation.agent_id.clone(),
                agent_key: agent.map(|agent| agent.key.clone()).unwrap_or_default(),
                agent_name: agent.map(|agent| agent.name.clone()).unwrap_or_default(),
                target_path: installation.target_path.clone(),
                config_key: installation.config_key.clone(),
                state: InstallationState::parse(&installation.state),
                content_hash: installation.content_hash.clone(),
                installed_at: Some(installation.installed_at.clone()),
            });
        }
        let mut view = hub.mcp_row_to_view(row, distributions);
        if !row.env_keys.is_empty() {
            let missing: Vec<String> = row
                .env_keys
                .iter()
                .filter(|key| !secret_present(&row.id, key))
                .cloned()
                .collect();
            if !missing.is_empty() {
                view.issues.push(format!(
                    "Environment values are not stored in Keychain yet: {}",
                    missing.join(", ")
                ));
            }
        }
        if row.transport == McpTransport::Stdio.as_str()
            && row.command.as_deref().unwrap_or_default().trim().is_empty()
        {
            view.issues.push("stdio server has no command".to_string());
        }
        out.push(view);
    }
    Ok(out)
}

pub fn discover(hub: &AgentHub, filter: Option<Vec<String>>) -> Result<Vec<DiscoveredMcp>> {
    let agents = hub.list_agents()?;
    let project_roots = hub.project_roots()?;
    let home = hub.config_ref().home.clone();
    let mut out = Vec::new();
    for agent in &agents {
        if let Some(filter) = &filter {
            if !filter.iter().any(|id| id == &agent.id) {
                continue;
            }
        }
        let Some(adapter) = find_adapter(&agent.adapter_id) else {
            continue;
        };
        let mut targets: Vec<McpTargetSpec> = adapter.resolved_mcp_targets(&home);
        for project in &project_roots {
            for spec in adapter.project_mcp_target_specs(project) {
                if !targets.iter().any(|item| item.path == spec.path) {
                    targets.push(spec);
                }
            }
        }
        for target in targets {
            if !supported_format(target.format) || !target.path.is_file() {
                continue;
            }
            let entries = match read_entries(&target.path, target.format) {
                Ok(entries) => entries,
                Err(err) => {
                    out.push(DiscoveredMcp {
                        name: "(unreadable config)".to_string(),
                        agent_id: agent.id.clone(),
                        agent_key: agent.key.clone(),
                        agent_name: agent.name.clone(),
                        target_path: target.path.to_string_lossy().to_string(),
                        format: target.format.to_string(),
                        transport: McpTransport::Stdio,
                        command: None,
                        args: Vec::new(),
                        url: None,
                        headers: BTreeMap::new(),
                        env_keys: Vec::new(),
                        raw: Value::Null,
                        issues: vec![err.to_string()],
                        already_known: false,
                    });
                    continue;
                }
            };
            for (name, entry) in entries {
                let transport = infer_transport(&entry);
                let already_known = hub.db_ref().find_mcp_by_name(&name)?.is_some();
                out.push(DiscoveredMcp {
                    name,
                    agent_id: agent.id.clone(),
                    agent_key: agent.key.clone(),
                    agent_name: agent.name.clone(),
                    target_path: target.path.to_string_lossy().to_string(),
                    format: target.format.to_string(),
                    transport,
                    command: entry
                        .get("command")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    args: entry_strings(&entry, "args"),
                    url: entry.get("url").and_then(Value::as_str).map(str::to_string),
                    headers: entry_map(&entry, "headers"),
                    env_keys: entry_map(&entry, "env").keys().cloned().collect(),
                    raw: entry,
                    issues: Vec::new(),
                    already_known,
                });
            }
        }
    }
    out.sort_by(|a, b| (a.agent_key.clone(), a.name.clone()).cmp(&(b.agent_key.clone(), b.name.clone())));
    Ok(out)
}

pub fn upsert(hub: &AgentHub, request: McpUpsertRequest) -> Result<McpServerView> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(Error::msg("MCP server name cannot be empty"));
    }
    if name.len() > 128 {
        return Err(Error::msg("MCP server name is too long"));
    }
    let transport = request.transport;
    match transport {
        McpTransport::Stdio => {
            if request
                .command
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(Error::msg("stdio MCP server requires a command"));
            }
            if request.url.as_deref().unwrap_or_default().trim().len() > 0 {
                return Err(Error::msg("stdio MCP server cannot have a URL"));
            }
        }
        _ => {
            if request.url.as_deref().unwrap_or_default().trim().is_empty() {
                return Err(Error::msg("HTTP/SSE MCP server requires a URL"));
            }
            if request
                .command
                .as_deref()
                .unwrap_or_default()
                .trim()
                .len()
                > 0
            {
                return Err(Error::msg("HTTP/SSE MCP server cannot have a command"));
            }
        }
    }
    let existing_by_name = hub.db_ref().find_mcp_by_name(name)?;
    let id = match &request.id {
        Some(id) => {
            if let Some(conflict) = &existing_by_name {
                if &conflict.id != id {
                    return Err(Error::conflict(format!(
                        "another MCP server named '{name}' already exists"
                    )));
                }
            }
            id.clone()
        }
        None => {
            if existing_by_name.is_some() {
                return Err(Error::conflict(format!(
                    "MCP server '{name}' already exists; edit it instead of creating a duplicate"
                )));
            }
            util::new_id()
        }
    };
    let existing = if request.id.is_some() {
        hub.db_ref().find_mcp_by_name(name)?
    } else {
        None
    };
    let now = util::now();
    let row = McpRow {
        id,
        name: name.to_string(),
        description: request.description.unwrap_or_default(),
        transport: transport.as_str().to_string(),
        command: request.command,
        args: request.args,
        url: request.url,
        headers: request.headers.into_iter().collect(),
        env_keys: request.env_keys,
        credential_refs: request.credential_refs,
        capability_id: None,
        metadata: serde_json::json!({
            "source": "manual",
            "managedByAgentHub": true
        })
        .to_string(),
        created_at: existing
            .as_ref()
            .map(|row| row.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    hub.db_ref().upsert_mcp(&row)?;
    let view = list_servers(hub)?
        .into_iter()
        .find(|server| server.id == row.id)
        .ok_or_else(|| Error::not_found("MCP server disappeared after saving"))?;
    Ok(view)
}

pub fn import(hub: &AgentHub, request: McpImportRequest) -> Result<McpServerView> {
    let agent = hub
        .db_ref()
        .get_agent(&request.agent_id)?
        .ok_or_else(|| Error::not_found("agent does not exist"))?;
    let adapter = find_adapter(&agent.adapter_id)
        .or_else(|| find_adapter(&agent.key))
        .ok_or_else(|| Error::msg("unknown adapter"))?;
    let mut targets = adapter.resolved_mcp_targets(&hub.config_ref().home);
    for project in hub.project_roots()? {
        for spec in adapter.project_mcp_target_specs(&project) {
            if !targets.iter().any(|item| item.path == spec.path) {
                targets.push(spec);
            }
        }
    }
    let target = targets
        .into_iter()
        .find(|spec| spec.path.to_string_lossy() == request.target_path)
        .ok_or_else(|| Error::not_found("MCP config target is not registered for this agent"))?;
    let entry = read_entry(&target.path, target.format, &request.name)?
        .ok_or_else(|| Error::not_found(format!("MCP server '{}' not found in {}", request.name, target.path.display())))?;
    let transport = infer_transport(&entry);
    let env_keys: Vec<String> = entry_map(&entry, "env").keys().cloned().collect();
    let headers: BTreeMap<String, String> = entry_map(&entry, "headers");
    let existing = hub.db_ref().find_mcp_by_name(&request.name)?;
    if let Some(existing) = &existing {
        let canonical_existing = entry_hash(&build_entry(
            existing,
            &BTreeMap::new(),
            target.format,
        )?);
        if canonical_existing == entry_hash(&entry) {
            return list_servers(hub)?
                .into_iter()
                .find(|server| server.id == existing.id)
                .ok_or_else(|| Error::not_found("MCP server is missing"));
        }
        return Err(Error::conflict(format!(
            "MCP server '{}' already exists with different settings; edit or remove it first",
            request.name
        )));
    }
    let now = util::now();
    let row = McpRow {
        id: util::new_id(),
        name: request.name.clone(),
        description: format!("Imported from {}", agent.name),
        transport: transport.as_str().to_string(),
        command: entry.get("command").and_then(Value::as_str).map(str::to_string),
        args: entry_strings(&entry, "args"),
        url: entry.get("url").and_then(Value::as_str).map(str::to_string),
        headers: headers.into_iter().collect(),
        env_keys,
        credential_refs: Vec::new(),
        capability_id: None,
        metadata: serde_json::json!({
            "source": "imported",
            "importedFrom": target.path.to_string_lossy(),
            "importedAgent": agent.key,
            "format": target.format,
            "managedByAgentHub": false,
        })
        .to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    hub.db_ref().upsert_mcp(&row)?;
    list_servers(hub)?
        .into_iter()
        .find(|server| server.id == row.id)
        .ok_or_else(|| Error::not_found("MCP server disappeared after import"))
}

pub fn plan_distribute(hub: &AgentHub, request: McpDistributeRequest) -> Result<OperationPlan> {
    let server = hub
        .db_ref()
        .list_mcp()?
        .into_iter()
        .find(|row| row.id == request.server_id)
        .ok_or_else(|| Error::not_found("MCP server does not exist"))?;
    let transport = McpTransport::parse(&server.transport);
    let agents = agent_map(hub)?;
    let home = hub.config_ref().home.clone();
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut restarts = Vec::new();
    let mut env_values: BTreeMap<String, String> = BTreeMap::new();
    for key in &server.env_keys {
        match get_secret(&server.id, key) {
            Some(value) => {
                env_values.insert(key.clone(), value);
            }
            None => {
                warnings.push(format!(
                    "Environment value '{key}' is missing from Keychain; distribution is blocked for agents that need it."
                ));
            }
        }
    }
    let secrets_ready = warnings.is_empty();

    for agent_id in &request.agent_ids {
        let Some(agent) = agents.get(agent_id) else {
            warnings.push(format!("Unknown agent id {agent_id} was skipped."));
            continue;
        };
        let Some(adapter) = find_adapter(&agent.adapter_id) else {
            continue;
        };
        let mut targets = adapter.resolved_mcp_targets(&home);
        for project in hub.project_roots()? {
            for spec in adapter.project_mcp_target_specs(&project) {
                if !targets.iter().any(|item| item.path == spec.path) {
                    targets.push(spec);
                }
            }
        }
        let target = targets.into_iter().find(|spec| {
            supported_format(spec.format)
                && spec.transports.contains(&transport)
                && spec
                    .path
                    .parent()
                    .map(|parent| parent.is_dir() && util::is_writable_dir(parent))
                    .unwrap_or(false)
        });
        let Some(target) = target else {
            items.push(mcp_blocked_item(
                &server,
                agent,
                format!(
                    "No writable MCP configuration for {} supports {:?} transport.",
                    agent.name, transport
                ),
            ));
            continue;
        };
        if !secrets_ready {
            items.push(mcp_blocked_item(
                &server,
                agent,
                "Required environment values are not stored in Keychain yet.".to_string(),
            ));
            continue;
        }
        let full_entry = build_entry(&server, &env_values, target.format)?;
        // The persisted plan must never contain secret values: strip the env
        // block from the payload; the executor rebuilds it from Keychain.
        let plan_entry = build_entry(&server, &BTreeMap::new(), target.format)?;
        if entry_hash(&full_entry) == entry_hash(&json!({})) {
            items.push(mcp_blocked_item(&server, agent, "MCP server definition is empty.".to_string()));
            continue;
        }
        let current = read_entry(&target.path, target.format, &server.name)
            .ok()
            .flatten();
        let previous = hub.db_ref().find_mcp_installation(
            &agent.id,
            &target.path.to_string_lossy(),
            &server.name,
        )?;
        let (action, diff) = match (&current, &previous) {
            (None, _) => (PlanAction::Create, None),
            (Some(current), Some(previous))
                if previous.content_hash.as_deref() == Some(entry_hash(current).as_str()) =>
            {
                if entry_hash(current) == entry_hash(&full_entry) {
                    (PlanAction::Noop, None)
                } else {
                    (
                        PlanAction::Replace,
                        Some("Replace the existing MCP entry managed by AgentHub.".to_string()),
                    )
                }
            }
            (Some(current), _) => {
                if entry_hash(current) == entry_hash(&full_entry) {
                    (PlanAction::Noop, None)
                } else {
                    (
                        PlanAction::Conflict,
                        Some(
                            "This config already contains a different entry with the same name; it is not managed by AgentHub."
                                .to_string(),
                        ),
                    )
                }
            }
        };
        if let Some(note) = adapter.requires_restart_note() {
            restarts.push(format!("{}: {}", agent.name, note));
        }
        let mut item_warnings = Vec::new();
        if !server.env_keys.is_empty() {
            item_warnings.push(
                "Environment values from Keychain will be written into the agent's config file. This exposes them to anything that can read that file."
                    .to_string(),
            );
        }
        if let Some(diff) = &diff {
            item_warnings.push(diff.clone());
        }
        items.push(PlanItem {
            id: util::new_id(),
            capability_id: None,
            capability_name: server.name.clone(),
            version_id: None,
            version_label: None,
            agent_id: agent.id.clone(),
            agent_key: agent.key.clone(),
            agent_name: agent.name.clone(),
            install_kind: "mcp_config".to_string(),
            action,
            strategy: InstallStrategy::Copy,
            target_path: target.path.to_string_lossy().to_string(),
            source_path: None,
            current_hash: current.as_ref().map(entry_hash),
            desired_hash: Some(entry_hash(&full_entry)),
            config_key: Some(server.name.clone()),
            scope: Scope::Global,
            target_format: Some(target.format.to_string()),
            payload: Some(plan_entry),
            warnings: item_warnings,
            requires_restart: adapter.requires_restart_note().is_some(),
            diff,
            previous_installation: previous
                .as_ref()
                .map(serde_json::to_value)
                .transpose()?,
        });
    }
    if items.is_empty() {
        return Err(Error::conflict("no distributable MCP target was found"));
    }
    hub.persist_mcp_plan(
        OperationKind::McpDistribute,
        format!("Distribute MCP server {}", server.name),
        items,
        warnings,
        restarts,
    )
}

fn mcp_blocked_item(server: &McpRow, agent: &AgentView, reason: String) -> PlanItem {
    PlanItem {
        id: util::new_id(),
        capability_id: None,
        capability_name: server.name.clone(),
        version_id: None,
        version_label: None,
        agent_id: agent.id.clone(),
        agent_key: agent.key.clone(),
        agent_name: agent.name.clone(),
        install_kind: "mcp_config".to_string(),
        action: PlanAction::Blocked,
        strategy: InstallStrategy::Copy,
        target_path: String::new(),
        source_path: None,
        current_hash: None,
        desired_hash: None,
        config_key: Some(server.name.clone()),
        scope: Scope::Global,
        target_format: None,
        payload: None,
        warnings: vec![reason],
        requires_restart: false,
        diff: None,
        previous_installation: None,
    }
}

pub fn plan_remove(hub: &AgentHub, server_id: &str, agent_ids: Vec<String>) -> Result<OperationPlan> {
    let server = hub
        .db_ref()
        .list_mcp()?
        .into_iter()
        .find(|row| row.id == server_id)
        .ok_or_else(|| Error::not_found("MCP server does not exist"))?;
    let agents = agent_map(hub)?;
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    for installation in hub.db_ref().list_installations()? {
        if installation.kind != "mcp" {
            continue;
        }
        let metadata: Value =
            serde_json::from_str(&installation.metadata).unwrap_or_else(|_| json!({}));
        if metadata.get("serverId").and_then(Value::as_str) != Some(server.id.as_str()) {
            continue;
        }
        if !agent_ids.is_empty() && !agent_ids.contains(&installation.agent_id) {
            continue;
        }
        let Some(agent) = agents.get(&installation.agent_id) else {
            continue;
        };
        let format = metadata
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("json:mcpServers")
            .to_string();
        // Record the entry that is on disk now so preflight can detect changes
        // made between planning and applying (MCP entries are entry-level).
        let current = read_entry(
            Path::new(&installation.target_path),
            &format,
            &installation.config_key,
        )
        .ok()
        .flatten();
        items.push(PlanItem {
            id: util::new_id(),
            capability_id: None,
            capability_name: server.name.clone(),
            version_id: None,
            version_label: None,
            agent_id: agent.id.clone(),
            agent_key: agent.key.clone(),
            agent_name: agent.name.clone(),
            install_kind: "mcp_config".to_string(),
            action: PlanAction::Remove,
            strategy: InstallStrategy::Copy,
            target_path: installation.target_path.clone(),
            source_path: None,
            current_hash: current.as_ref().map(entry_hash),
            desired_hash: installation.content_hash.clone(),
            config_key: Some(installation.config_key.clone()),
            scope: Scope::Global,
            target_format: Some(format),
            payload: None,
            warnings: Vec::new(),
            requires_restart: false,
            diff: Some("Remove this MCP entry from the agent config; a snapshot is kept.".to_string()),
            previous_installation: Some(serde_json::to_value(&installation)?),
        });
    }
    if items.is_empty() {
        warnings.push("No matching MCP installations were found.".to_string());
        return Err(Error::not_found("no MCP installations to remove"));
    }
    hub.persist_mcp_plan(
        OperationKind::McpRemove,
        format!("Remove MCP server {}", server.name),
        items,
        warnings,
        Vec::new(),
    )
}

/// Execute a single MCP plan item. Called by the transactional executor after a
/// snapshot has been taken. Returns the installation id (if one remains).
pub fn apply_config_item(db: &Database, item: &PlanItem, remove: bool) -> Result<Option<String>> {
    let target_path = PathBuf::from(&item.target_path);
    let format = item
        .target_format
        .as_deref()
        .ok_or_else(|| Error::msg("MCP plan item is missing a target format"))?;
    let key = item
        .config_key
        .as_deref()
        .ok_or_else(|| Error::msg("MCP plan item is missing a config key"))?;
    if remove {
        remove_entry(&target_path, format, key)?;
        if let Some(existing) =
            db.find_mcp_installation(&item.agent_id, &item.target_path, key)?
        {
            db.delete_installation(&existing.id)?;
        }
        return Ok(None);
    }
    let server = db
        .find_mcp_by_name(key)?
        .ok_or_else(|| Error::not_found("MCP definition is missing"))?;
    let mut env_values = BTreeMap::new();
    for env_key in &server.env_keys {
        let value = get_secret(&server.id, env_key).ok_or_else(|| {
            Error::blocked(format!(
                "environment value '{env_key}' is not stored in Keychain"
            ))
        })?;
        env_values.insert(env_key.clone(), value);
    }
    // Rebuild the entry with secret values at apply time only, so secrets are
    // never persisted inside operations.plan or logs.
    let entry = build_entry(&server, &env_values, format)?;
    let mut entries = read_entries(&target_path, format)?;
    entries.retain(|(name, _)| name != key);
    entries.push((key.to_string(), entry.clone()));
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    write_entries(&target_path, format, &entries)?;
    let written = read_entry(&target_path, format, key)?
        .ok_or_else(|| Error::Verification("MCP entry was not written".to_string()))?;
    if entry_hash(&written) != entry_hash(&entry) {
        return Err(Error::Verification(
            "MCP config verification failed: entry content differs after write".to_string(),
        ));
    }
    let file_hash = util::hash_file(&target_path)?;
    let server_id = server.id.clone();
    let existing = db.find_mcp_installation(&item.agent_id, &item.target_path, key)?;
    let id = existing
        .as_ref()
        .map(|row| row.id.clone())
        .unwrap_or_else(util::new_id);
    let now = util::now();
    db.upsert_installation(&InstallationRow {
        id: id.clone(),
        capability_id: None,
        version_id: None,
        agent_id: item.agent_id.clone(),
        kind: "mcp".to_string(),
        scope: "global".to_string(),
        strategy: "native".to_string(),
        target_path: item.target_path.clone(),
        source_path: None,
        content_hash: Some(entry_hash(&entry)),
        state: InstallationState::Installed.as_str().to_string(),
        config_key: key.to_string(),
        installed_at: now.clone(),
        verified_at: Some(now.clone()),
        last_check_at: Some(now),
        metadata: json!({
            "serverId": server_id,
            "format": format,
            "fileHash": file_hash
        })
        .to_string(),
    })?;
    Ok(Some(id))
}
