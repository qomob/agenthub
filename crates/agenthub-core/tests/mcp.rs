//! Integration tests: MCP discovery and config writes must preserve every
//! unrelated setting/comment/entry in the target files.

mod common;

use std::collections::BTreeMap;

use agenthub_core::model::*;

use common::*;

fn stdio_request(name: &str, env_keys: Vec<String>) -> McpUpsertRequest {
    McpUpsertRequest {
        id: None,
        name: name.to_string(),
        description: Some("created by integration test".to_string()),
        transport: McpTransport::Stdio,
        command: Some("npx".to_string()),
        args: vec!["-y".to_string(), "@example/mcp-server".to_string()],
        url: None,
        headers: BTreeMap::new(),
        env_keys,
        credential_refs: Vec::new(),
    }
}

fn http_request(name: &str) -> McpUpsertRequest {
    McpUpsertRequest {
        id: None,
        name: name.to_string(),
        description: Some("remote server created by integration test".to_string()),
        transport: McpTransport::Http,
        command: None,
        args: Vec::new(),
        url: Some("https://mcp.example.invalid/mcp".to_string()),
        headers: BTreeMap::new(),
        env_keys: Vec::new(),
        credential_refs: Vec::new(),
    }
}

const CODEX_CONFIG: &str = r#"# keep this comment: unrelated Codex settings
model = "gpt-5"
approval_policy = "on-request"

[projects.foo]
trust_level = "trusted"

# upstream MCP server that AgentHub must not touch
[mcp_servers.legacy]
command = "legacy-server"
args = ["--flag", "value"]
"#;

const WORKBUDDY_MCP: &str = r#"{
  "mcpServers": {
    "legacy-wb": {
      "url": "http://localhost:9999/mcp",
      "type": "http"
    }
  },
  "unrelated": {
    "a": 1,
    "keep": ["x"]
  }
}
"#;

fn toml_str<'a>(item: Option<&'a toml_edit::Item>, key: &str) -> Option<&'a str> {
    item.and_then(|item| item.as_table_like())
        .and_then(|table| table.get(key))
        .and_then(|item| item.as_str())
}

#[test]
fn mcp_discover_reads_existing_entries_without_writing() {
    let fixture = Fixture::new();
    write(&fixture.codex_config(), CODEX_CONFIG);
    write(&fixture.workbuddy_mcp(), WORKBUDDY_MCP);

    let hub = fixture.hub();
    let discovered = hub.mcp_discover(None).expect("mcp discover");

    let codex_entry = discovered
        .iter()
        .find(|entry| entry.name == "legacy" && entry.agent_key == "codex")
        .expect("codex TOML MCP entry must be discovered");
    assert_eq!(codex_entry.transport, McpTransport::Stdio);
    assert_eq!(codex_entry.command.as_deref(), Some("legacy-server"));
    assert_eq!(codex_entry.target_path, fixture.codex_config().to_string_lossy());

    let workbuddy_entry = discovered
        .iter()
        .find(|entry| entry.name == "legacy-wb" && entry.agent_key == "workbuddy")
        .expect("workbuddy JSON MCP entry must be discovered");
    assert_eq!(workbuddy_entry.transport, McpTransport::Http);
    assert_eq!(
        workbuddy_entry.url.as_deref(),
        Some("http://localhost:9999/mcp")
    );

    // Discovery is read-only.
    assert_eq!(read(&fixture.codex_config()), CODEX_CONFIG);
    assert_eq!(read(&fixture.workbuddy_mcp()), WORKBUDDY_MCP);
}

#[test]
fn mcp_distribution_and_removal_preserve_unrelated_configuration() {
    let fixture = Fixture::new();
    write(&fixture.codex_config(), CODEX_CONFIG);
    write(&fixture.workbuddy_mcp(), WORKBUDDY_MCP);

    let hub = fixture.hub();
    let codex = agent_id(&hub, "codex");
    let workbuddy = agent_id(&hub, "workbuddy");

    // --- stdio server into the Codex TOML file ------------------------------
    let stdio = hub
        .mcp_upsert(stdio_request("hub-stdio", Vec::new()))
        .expect("upsert stdio server");
    assert!(stdio.issues.is_empty(), "unexpected issues: {:?}", stdio.issues);

    let plan = hub
        .mcp_plan_distribute(McpDistributeRequest {
            server_id: stdio.id.clone(),
            agent_ids: vec![codex.clone()],
            force: false,
        })
        .expect("plan codex distribution");
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].action, PlanAction::Create);
    assert_eq!(plan.items[0].install_kind, "mcp_config");
    assert_eq!(
        plan.items[0].target_path,
        fixture.codex_config().to_string_lossy()
    );
    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply codex distribution");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");

    let text = read(&fixture.codex_config());
    assert!(
        text.contains("# keep this comment: unrelated Codex settings"),
        "the top-level comment must survive:\n{text}"
    );
    assert!(
        text.contains("# upstream MCP server that AgentHub must not touch"),
        "comments attached to existing MCP entries must survive:\n{text}"
    );
    let doc: toml_edit::DocumentMut = text.parse().expect("config.toml must stay valid TOML");
    assert_eq!(
        doc.get("model").and_then(|item| item.as_str()),
        Some("gpt-5")
    );
    assert_eq!(
        doc.get("approval_policy").and_then(|item| item.as_str()),
        Some("on-request")
    );
    let projects = doc
        .get("projects")
        .and_then(|item| item.as_table_like())
        .expect("projects table must survive");
    assert_eq!(
        toml_str(projects.get("foo"), "trust_level"),
        Some("trusted")
    );
    let servers = doc
        .get("mcp_servers")
        .and_then(|item| item.as_table_like())
        .expect("mcp_servers table");
    assert_eq!(toml_str(servers.get("legacy"), "command"), Some("legacy-server"));
    assert_eq!(toml_str(servers.get("hub-stdio"), "command"), Some("npx"));
    let args = servers
        .get("hub-stdio")
        .and_then(|item| item.as_table_like())
        .and_then(|table| table.get("args"))
        .and_then(|item| item.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
        });
    assert_eq!(args, Some(vec!["-y", "@example/mcp-server"]));

    // The distribution is recorded.
    let servers_view = hub.mcp_servers().expect("list mcp servers");
    let view = servers_view
        .iter()
        .find(|server| server.id == stdio.id)
        .expect("server view");
    assert_eq!(view.distributions.len(), 1);
    assert_eq!(view.distributions[0].agent_key, "codex");
    assert_eq!(view.distributions[0].state, InstallationState::Installed);

    // --- updating an existing managed entry rewrites only that entry ---------
    let updated = hub
        .mcp_upsert(McpUpsertRequest {
            id: Some(stdio.id.clone()),
            name: "hub-stdio".to_string(),
            description: Some("updated definition".to_string()),
            transport: McpTransport::Stdio,
            command: Some("uvx".to_string()),
            args: vec!["new-server".to_string(), "--fast".to_string()],
            url: None,
            headers: BTreeMap::new(),
            env_keys: Vec::new(),
            credential_refs: Vec::new(),
        })
        .expect("update MCP definition");
    assert_eq!(updated.id, stdio.id);

    let update_plan = hub
        .mcp_plan_distribute(McpDistributeRequest {
            server_id: stdio.id.clone(),
            agent_ids: vec![codex.clone()],
            force: false,
        })
        .expect("plan MCP update");
    assert_eq!(update_plan.items[0].action, PlanAction::Replace);
    let result = hub
        .apply_operation(&update_plan.operation_id, false)
        .expect("apply MCP update");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");

    let text = read(&fixture.codex_config());
    assert!(text.contains("# keep this comment: unrelated Codex settings"));
    let doc: toml_edit::DocumentMut = text.parse().expect("config.toml must stay valid TOML");
    let servers = doc
        .get("mcp_servers")
        .and_then(|item| item.as_table_like())
        .expect("mcp_servers table");
    assert_eq!(toml_str(servers.get("legacy"), "command"), Some("legacy-server"));
    assert_eq!(toml_str(servers.get("hub-stdio"), "command"), Some("uvx"));
    let updated_args: Vec<&str> = servers
        .get("hub-stdio")
        .and_then(|item| item.as_table_like())
        .and_then(|table| table.get("args"))
        .and_then(|item| item.as_array())
        .map(|array| array.iter().filter_map(|value| value.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(updated_args, vec!["new-server", "--fast"]);
    let projects = doc
        .get("projects")
        .and_then(|item| item.as_table_like())
        .expect("projects table");
    assert_eq!(toml_str(projects.get("foo"), "trust_level"), Some("trusted"));

    // --- HTTP server into the WorkBuddy JSON file ---------------------------
    let http = hub
        .mcp_upsert(http_request("hub-http"))
        .expect("upsert http server");
    let plan = hub
        .mcp_plan_distribute(McpDistributeRequest {
            server_id: http.id.clone(),
            agent_ids: vec![workbuddy.clone()],
            force: false,
        })
        .expect("plan workbuddy distribution");
    assert_eq!(plan.items[0].action, PlanAction::Create);
    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply workbuddy distribution");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");

    let value: serde_json::Value =
        serde_json::from_str(&read(&fixture.workbuddy_mcp())).expect("mcp.json must stay valid JSON");
    assert_eq!(value["unrelated"]["a"], 1);
    assert_eq!(value["unrelated"]["keep"][0], "x");
    assert_eq!(
        value["mcpServers"]["legacy-wb"]["url"],
        "http://localhost:9999/mcp"
    );
    assert_eq!(
        value["mcpServers"]["hub-http"]["url"],
        "https://mcp.example.invalid/mcp"
    );
    assert_eq!(value["mcpServers"]["hub-http"]["type"], "http");

    // --- removal must delete only the managed entries -----------------------
    let remove = hub
        .mcp_plan_remove(&stdio.id, vec![codex.clone()])
        .expect("plan codex removal");
    assert_eq!(remove.items.len(), 1);
    assert_eq!(remove.items[0].action, PlanAction::Remove);
    let result = hub
        .apply_operation(&remove.operation_id, false)
        .expect("apply codex removal");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");

    let text = read(&fixture.codex_config());
    assert!(
        !text.contains("hub-stdio"),
        "the removed MCP entry must disappear from config.toml:\n{text}"
    );
    assert!(
        text.contains("# keep this comment: unrelated Codex settings")
            && text.contains("legacy-server")
            && text.contains("[projects.foo]"),
        "removal must preserve unrelated config/comments/entries:\n{text}"
    );

    let remove = hub
        .mcp_plan_remove(&http.id, vec![workbuddy])
        .expect("plan workbuddy removal");
    let result = hub
        .apply_operation(&remove.operation_id, false)
        .expect("apply workbuddy removal");
    assert_eq!(result.status, OperationStatus::Success, "result: {result:?}");
    let value: serde_json::Value = serde_json::from_str(&read(&fixture.workbuddy_mcp()))
        .expect("mcp.json must stay valid JSON");
    assert!(
        value["mcpServers"].get("hub-http").is_none(),
        "removed entry must disappear: {value}"
    );
    assert_eq!(
        value["mcpServers"]["legacy-wb"]["url"],
        "http://localhost:9999/mcp"
    );
    assert_eq!(value["unrelated"]["a"], 1);

    // No distribution records remain for the removed entries.
    let servers_view = hub.mcp_servers().expect("list mcp servers");
    for server_id in [&stdio.id, &http.id] {
        let view = servers_view
            .iter()
            .find(|server| &server.id == server_id)
            .expect("server view");
        assert!(
            view.distributions.is_empty(),
            "removal must delete installation records"
        );
    }
}

#[test]
fn mcp_env_keys_without_keychain_values_are_blocked() {
    let fixture = Fixture::new();
    write(&fixture.codex_config(), CODEX_CONFIG);
    let hub = fixture.hub();
    let codex = agent_id(&hub, "codex");

    let server = hub
        .mcp_upsert(stdio_request("hub-env", vec!["AGENTHUB_TEST_API_TOKEN".to_string()]))
        .expect("upsert env server");

    let plan = hub
        .mcp_plan_distribute(McpDistributeRequest {
            server_id: server.id.clone(),
            agent_ids: vec![codex],
            force: false,
        })
        .expect("plan distribution with missing secret");
    assert_eq!(plan.summary.blocked, 1, "summary: {:?}", plan.summary);
    assert_eq!(plan.summary.create, 0);
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.contains("AGENTHUB_TEST_API_TOKEN")),
        "missing env value warning expected: {:?}",
        plan.warnings
    );
    // The plan item is blocked and points at no target.
    assert_eq!(plan.items[0].action, PlanAction::Blocked);
    assert!(plan.items[0].target_path.is_empty());

    // Applying a plan whose only item is blocked skips it (the executor treats
    // unsupported/blocked items as skipped) and writes nothing.
    let before = read(&fixture.codex_config());
    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("blocked items are skipped, not fatal");
    assert_eq!(result.status, OperationStatus::Partial, "result: {result:?}");
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].status, "skipped");
    assert!(
        result.items[0].message.contains("Keychain"),
        "skip reason must mention the missing Keychain value: {}",
        result.items[0].message
    );
    assert!(
        result.diagnostics.iter().any(|line| line.contains("skipped")),
        "diagnostics must record the skip: {:?}",
        result.diagnostics
    );
    assert_eq!(read(&fixture.codex_config()), before);
}

#[test]
fn mcp_header_values_go_to_keychain_and_never_into_the_db_row() {
    let security_available = std::path::Path::new("/usr/bin/security").exists();
    if !security_available {
        // Keychain storage is macOS-only; on other platforms the upsert path
        // is covered by the empty-header tests above.
        return;
    }
    let fixture = Fixture::new();
    let hub = fixture.hub();

    let mut request = http_request("keychain-headers");
    request.headers.insert(
        "Authorization".to_string(),
        "Bearer tokHeaderValue".to_string(),
    );
    let view = agenthub_core::mcp::upsert(&hub, request).expect("upsert with header");
    let server_id = view.id.clone();

    // The view must not carry the raw value.
    assert_eq!(
        view.headers.get("Authorization").map(String::as_str),
        Some("••••••"),
        "view must mask header values"
    );

    // The DB row keeps the name with a blanked value.
    let row = hub
        .db()
        .list_mcp()
        .expect("list rows")
        .into_iter()
        .find(|row| row.id == server_id)
        .expect("row exists");
    assert_eq!(
        row.headers,
        vec![("Authorization".to_string(), String::new())],
        "DB row must not store the header value"
    );

    // The value itself lives in the Keychain and round-trips.
    assert_eq!(
        agenthub_core::mcp::get_header_secret(&server_id, "Authorization").as_deref(),
        Some("Bearer tokHeaderValue")
    );

    // Cleanup: remove the test secret from the user keychain.
    let _ = agenthub_core::mcp::delete_header_secret(&server_id, "Authorization");
}
