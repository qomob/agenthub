//! Extended adapter coverage for Qwen Work (千问工作) and Doubao Work (豆包工作).
//!
//! All fixtures live in a temporary home; `/Applications` is never needed
//! because `find_app` also looks in `<home>/Applications`.

mod common;

use std::fs;

use agenthub_core::model::*;
use common::*;

fn fake_app(home: &std::path::Path, name: &str) -> std::path::PathBuf {
    let app = home.join("Applications").join(name);
    fs::create_dir_all(&app).expect("create fake app bundle");
    app
}

fn write_qwen_plugin(home: &std::path::Path, folder: &str, manifest: &str) {
    let dir = home
        .join(".qwenworkcn/plugins-custom")
        .join(folder)
        .join(".codex-plugin");
    fs::create_dir_all(&dir).expect("create plugin manifest dir");
    fs::write(dir.join("plugin.json"), manifest).expect("write plugin manifest");
}

#[test]
fn all_builtin_adapters_include_qwen_and_doubao() {
    let fixture = Fixture::with_agents(&[".codex"]);
    let hub = fixture.hub();
    let agents = hub.list_agents().expect("agents");
    assert!(agents.len() >= 11, "expected 11 builtin adapters: {agents:?}");
    for key in ["qwen-work", "doubao-work"] {
        assert!(
            agents.iter().any(|agent| agent.key == key),
            "missing adapter {key}"
        );
    }
}

#[test]
fn qwen_work_detects_skills_and_discovers_plugins() {
    let fixture = Fixture::with_agents(&[".qwenworkcn"]);
    fake_app(&fixture.home, "QwenWorkCN.app");
    let skills_root = fixture.home.join(".qwenworkcn/skills");
    write_skill(&skills_root, "qw-demo", "Qwen demo skill", "hello qwen");
    write_qwen_plugin(
        &fixture.home,
        "demo-plugin",
        r#"{"name":"demo-plugin","version":"2.1.0","description":"fixture"}"#,
    );

    let hub = fixture.hub();
    let qwen = hub
        .list_agents()
        .expect("agents")
        .into_iter()
        .find(|agent| agent.key == "qwen-work")
        .expect("qwen-work adapter");
    assert_eq!(qwen.status, AgentStatus::Installed);
    assert!(qwen
        .skill_roots
        .iter()
        .any(|root| root.path == skills_root.to_string_lossy() && root.exists));

    let report = hub.scan().expect("scan");
    let scan = report
        .agents
        .iter()
        .find(|agent| agent.agent_key == "qwen-work")
        .expect("qwen scan");
    assert!(
        scan.skills.iter().any(|skill| skill.slug == "qw-demo"),
        "qwen skill should be discovered: {scan:?}"
    );

    let plugins = hub.sync_plugins().expect("plugins");
    assert!(
        plugins
            .iter()
            .any(|plugin| plugin.agent_key == "qwen-work" && plugin.name == "demo-plugin"),
        "qwen plugin should be discovered read-only: {plugins:?}"
    );
}

#[test]
fn qwen_work_mcp_distribution_preserves_unrelated_config() {
    let fixture = Fixture::with_agents(&[".qwenworkcn"]);
    fake_app(&fixture.home, "QwenWorkCN.app");
    let mcp_path = fixture.home.join(".qwenworkcn/mcp.json");
    fs::write(
        &mcp_path,
        r#"{
  "mcpServers": {
    "existing": { "command": "existing-server", "args": ["--keep"] }
  },
  "unrelated": { "keep": true }
}
"#,
    )
    .expect("write qwen mcp config");

    let hub = fixture.hub();
    let qwen_id = agent_id(&hub, "qwen-work");
    let server = hub
        .mcp_upsert(McpUpsertRequest {
            id: None,
            name: "agenthub-qwen-smoke".to_string(),
            description: Some("fixture".to_string()),
            transport: McpTransport::Stdio,
            command: Some("/usr/bin/true".to_string()),
            args: vec!["--qwen".to_string()],
            url: None,
            headers: Default::default(),
            env_keys: Vec::new(),
            credential_refs: Vec::new(),
        })
        .expect("upsert mcp");
    let plan = hub
        .mcp_plan_distribute(McpDistributeRequest {
            server_id: server.id.clone(),
            agent_ids: vec![qwen_id.clone()],
            force: false,
        })
        .expect("plan distribute");
    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply mcp");
    assert_eq!(result.status, OperationStatus::Success, "{result:?}");

    let text = fs::read_to_string(&mcp_path).expect("read mcp config");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert!(doc.get("unrelated").is_some(), "unrelated key preserved");
    assert!(
        doc["mcpServers"].get("existing").is_some(),
        "existing server preserved"
    );
    assert_eq!(
        doc["mcpServers"]["agenthub-qwen-smoke"]["command"], "/usr/bin/true",
        "new server written: {text}"
    );

    let remove = hub
        .mcp_plan_remove(&server.id, vec![qwen_id])
        .expect("plan remove");
    hub.apply_operation(&remove.operation_id, false)
        .expect("apply remove");
    let text = fs::read_to_string(&mcp_path).expect("read mcp config after remove");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert!(doc["mcpServers"].get("agenthub-qwen-smoke").is_none());
    assert!(doc["mcpServers"].get("existing").is_some());
    assert!(doc.get("unrelated").is_some());
}

#[test]
fn doubao_work_uses_workspace_user_skills_and_never_guesses_global_roots() {
    let fixture = Fixture::with_agents(&[".doubao"]);
    fake_app(&fixture.home, "Doubao.app");
    let workspace_skills = fixture.project.join(".user_skills");
    write_skill(&workspace_skills, "db-demo", "Doubao demo skill", "hello doubao");
    hub_setup_project(&fixture);

    let hub = fixture.hub();
    hub.set_project_roots(vec![fixture.project.to_string_lossy().to_string()])
        .expect("set project roots");
    let doubao = hub
        .list_agents()
        .expect("agents")
        .into_iter()
        .find(|agent| agent.key == "doubao-work")
        .expect("doubao adapter");
    assert_eq!(doubao.status, AgentStatus::VerificationRequired);
    assert!(
        doubao.skill_roots.iter().all(|root| !root.exists || root.source == "project"),
        "no global doubao skill root is assumed: {:?}",
        doubao.skill_roots
    );

    let report = hub.scan().expect("scan");
    let scan = report
        .agents
        .iter()
        .find(|agent| agent.agent_key == "doubao-work")
        .expect("doubao scan");
    assert!(
        scan.skills.iter().any(|skill| skill.slug == "db-demo"),
        "workspace .user_skills should be discovered: {scan:?}"
    );

    // Import the workspace skill and try to distribute without a writable root.
    let imported = hub
        .import_skill(ImportRequest {
            source: ImportSource::AgentDir {
                agent_id: doubao.id.clone(),
                path: workspace_skills.join("db-demo").to_string_lossy().to_string(),
            },
            skill_path: None,
            version_label: None,
            notes: None,
        })
        .expect("import doubao skill");
    let blocked = hub
        .plan_install(InstallRequest {
            capability_id: imported.skill.id.clone(),
            version_id: None,
            agent_ids: vec![doubao.id.clone()],
            strategy: Some(InstallStrategy::Copy),
            force: false,
        })
        .expect("plan install without root");
    assert_eq!(blocked.summary.blocked, 1, "no writable doubao root: {blocked:?}");

    // Registering the workspace skill folder as a custom root makes it a real
    // install target; AgentHub must never invent a global Doubao directory.
    let install_root = fixture.project.join(".doubao/skills");
    hub.update_agent_paths(AgentPathUpdate {
        agent_id: doubao.id.clone(),
        app_path: doubao.app_path.clone(),
        home_path: doubao.home_path.clone(),
        custom_skill_roots: vec![install_root.to_string_lossy().to_string()],
    })
    .expect("register custom root");
    let plan = hub
        .plan_install(InstallRequest {
            capability_id: imported.skill.id,
            version_id: None,
            agent_ids: vec![doubao.id],
            strategy: Some(InstallStrategy::Copy),
            force: false,
        })
        .expect("plan install with custom root");
    assert_eq!(plan.summary.create, 1, "{plan:?}");
    let result = hub
        .apply_operation(&plan.operation_id, false)
        .expect("apply install");
    assert_eq!(result.status, OperationStatus::Success, "{result:?}");
    assert!(
        install_root.join("db-demo/SKILL.md").is_file(),
        "deployed into the registered workspace root"
    );
}

/// The Doubao fixture needs the hub before setting project roots; this helper
/// simply opens and drops a hub so the fixture directory layout is exercised.
fn hub_setup_project(fixture: &Fixture) {
    let hub = fixture.hub();
    drop(hub);
}
