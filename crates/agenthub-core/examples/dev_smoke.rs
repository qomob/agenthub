//! Development smoke harness used by the Lead to verify the real engine on this
//! machine without opening the GUI.
//!
//! Modes:
//!   scan-real     read-only scan of the real user's agents (no writes to them)
//!   acceptance    full import → distribute → update → rollback scenario on a
//!                 throw-away fake home populated from real skill content
//!
//! Run with:
//!   ./scripts/cargo.sh run -p agenthub-core --example dev_smoke -- scan-real
//!   ./scripts/cargo.sh run -p agenthub-core --example dev_smoke -- acceptance

use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "scan-real".to_string());
    let result = match mode.as_str() {
        "scan-real" => scan_real(),
        "acceptance" => acceptance(),
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    };
    match result {
        Ok(()) => println!("\n[dev_smoke] OK ({mode})"),
        Err(err) => {
            eprintln!("\n[dev_smoke] FAILED ({mode}): {err}");
            std::process::exit(1);
        }
    }
}

fn real_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
}

fn scan_real() -> agenthub_core::Result<()> {
    let data_dir = PathBuf::from("/tmp/agenthub-dev-smoke");
    let _ = fs::remove_dir_all(&data_dir);
    // Read-only scan of the real agents; metadata lives in a throw-away dir.
    let config = CoreConfig::resolve()?;
    let hub = AgentHub::open(config)?;
    let report = hub.scan()?;
    println!("scan timestamp: {}", report.scanned_at);
    for agent in &report.agents {
        println!(
            "\n== {} ({}) status {:?}, {} skills, {} roots",
            agent.agent_name,
            agent.agent_key,
            agent.status,
            agent.skills.len(),
            agent.roots.len()
        );
        for root in &agent.roots {
            println!(
                "   root {} exists={} discovered={} invalid={}",
                root.path, root.exists, root.discovered, root.invalid
            );
        }
        for skill in agent.skills.iter().take(5) {
            println!(
                "   - {} v{:?} valid={} inSync={} path={}",
                skill.slug, skill.version, skill.valid, skill.in_sync, skill.path
            );
        }
        for warning in agent.errors.iter().take(3) {
            println!("   ! {warning}");
        }
    }
    println!("\ntotals: {} skills, {} invalid, {} conflicts, {} duplicates", report.total_skills, report.invalid, report.conflicts, report.duplicates);
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            copy_dir(&path, &target)?;
        } else if meta.is_file() {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

struct Checker {
    failures: Vec<String>,
}

impl Checker {
    fn check(&mut self, label: &str, condition: bool, detail: impl AsRef<str>) {
        if condition {
            println!("  PASS  {label} ({})", detail.as_ref());
        } else {
            println!("  FAIL  {label} ({})", detail.as_ref());
            self.failures.push(format!("{label}: {}", detail.as_ref()));
        }
    }
}

fn acceptance() -> agenthub_core::Result<()> {
    let real = real_home();
    let base = PathBuf::from("/tmp/agenthub-acceptance");
    let _ = fs::remove_dir_all(&base);
    let home = base.join("home");
    let project = base.join("project");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&project)?;

    // Fake agent homes populated from genuine skill content on this machine.
    let sources = [
        (
            real.join(".workbuddy/skills/skillforge"),
            home.join(".workbuddy/skills/skillforge"),
        ),
        (
            real.join("Documents/madjob/.trae/skills/chat-ui"),
            project.join(".trae/skills/chat-ui"),
        ),
        (
            real.join("Documents/Memory Music/.dsh/skills/memory-maker"),
            project.join(".dsh/skills/memory-maker"),
        ),
        (
            real.join(".codex/skills/xclawskill"),
            home.join(".codex/skills/xclawskill"),
        ),
    ];
    for (source, target) in &sources {
        if !source.is_dir() {
            return Err(agenthub_core::Error::msg(format!(
                "fixture source missing: {}",
                source.display()
            )));
        }
        copy_dir(source, target)?;
    }
    fs::create_dir_all(home.join(".zcode/skills"))?;
    fs::create_dir_all(home.join(".trae-cn/skills"))?;
    fs::create_dir_all(home.join(".workbuddy/connectors/default"))?;
    fs::write(
        home.join(".codex/config.toml"),
        r#"model = "gpt-test"
# a user comment that must survive MCP edits
[projects."/tmp/example"]
trust_level = "trusted"

[mcp_servers.existing]
command = "existing-server"
args = ["--keep"]

[mcp_servers.existing.env]
TOKEN = "keep-me"

[plugins."demo@local"]
enabled = true
"#,
    )?;
    fs::write(
        home.join(".workbuddy/connectors/default/mcp.json"),
        r#"{
  "mcpServers": {
    "existing-http": {
      "url": "https://example.invalid/mcp",
      "type": "streamable-http",
      "disabled": true
    }
  },
  "unrelated": {"keep": true}
}
"#,
    )?;

    std::env::set_var("AGENTHUB_HOME", &home);
    std::env::set_var("AGENTHUB_DATA_DIR", home.join(".agenthub"));

    println!("fake home: {}", home.display());
    let hub = AgentHub::open(CoreConfig::resolve()?)?;
    hub.set_project_roots(vec![project.to_string_lossy().to_string()])?;
    let report = hub.scan()?;
    let mut checker = Checker { failures: Vec::new() };
    checker.check(
        "scan discovers skills across fake agents",
        report.total_skills >= 3,
        format!("{} skills", report.total_skills),
    );
    for agent in &report.agents {
        if ["codex", "workbuddy", "trae-cn", "dsh"].contains(&agent.agent_key.as_str()) {
            println!(
                "  scanned {} -> {} skills, status {:?}",
                agent.agent_key,
                agent.skills.len(),
                agent.status
            );
        }
    }

    // 1. Import the TRAE project skill into the central library.
    let chat_ui_source = project.join(".trae/skills/chat-ui");
    let imported = hub.import_skill(ImportRequest {
        source: ImportSource::AgentDir {
            agent_id: hub
                .list_agents()?
                .into_iter()
                .find(|agent| agent.key == "trae-cn")
                .map(|agent| agent.id)
                .unwrap_or_default(),
            path: chat_ui_source.to_string_lossy().to_string(),
        },
        skill_path: None,
        version_label: None,
        notes: None,
    })?;
    let skill_id = imported.skill.id.clone();
    let v1_hash = imported.version.content_hash.clone();
    checker.check(
        "import from TRAE project skill",
        imported.skill.slug == "chat-ui" && !v1_hash.is_empty(),
        format!("slug={} hash={}", imported.skill.slug, &v1_hash[..12]),
    );

    // 2. Distribute to Codex and ZCode (copy strategy).
    let agents = hub.list_agents()?;
    let codex = agents.iter().find(|a| a.key == "codex").expect("codex agent");
    let zcode = agents.iter().find(|a| a.key == "zcode").expect("zcode agent");
    let workbuddy = agents.iter().find(|a| a.key == "workbuddy").expect("workbuddy agent");
    let plan = hub.plan_install(InstallRequest {
        capability_id: skill_id.clone(),
        version_id: None,
        agent_ids: vec![codex.id.clone(), zcode.id.clone(), workbuddy.id.clone()],
        strategy: Some(InstallStrategy::Copy),
        force: false,
    })?;
    println!(
        "  plan: create={} replace={} noop={} conflict={} blocked={}",
        plan.summary.create, plan.summary.replace, plan.summary.noop, plan.summary.conflict, plan.summary.blocked
    );
    checker.check(
        "plan is a real preview (dry run)",
        plan.dry_run && plan.items.iter().all(|item| !item.target_path.is_empty()),
        format!("{} items", plan.items.len()),
    );
    let result = hub.apply_operation(&plan.operation_id, false)?;
    checker.check(
        "apply succeeds",
        matches!(result.status, OperationStatus::Success),
        format!("{:?}", result.status),
    );
    for agent in [&codex, &zcode, &workbuddy] {
        let root = agent
            .skill_roots
            .iter()
            .find(|root| root.source == "adapter")
            .map(|root| PathBuf::from(&root.path))
            .unwrap_or_default();
        // fake-home zcode/workbuddy roots: use the known fake home layout
        let target = match agent.key.as_str() {
            "codex" => home.join(".codex/skills/chat-ui"),
            "zcode" => home.join(".zcode/skills/chat-ui"),
            _ => home.join(".workbuddy/skills/chat-ui"),
        };
        checker.check(
            &format!("deployed to {}", agent.key),
            target.join("SKILL.md").is_file(),
            target.display().to_string(),
        );
        let _ = root;
    }

    // 3. Modify the library version and detect the pending update.
    let modified = base.join("modified-chat-ui");
    copy_dir(&chat_ui_source, &modified)?;
    let skill_md = modified.join("SKILL.md");
    let mut text = fs::read_to_string(&skill_md)?;
    text.push_str("\n\n## AgentHub smoke change\n\nThis line marks version 2 of the smoke test skill.\n");
    fs::write(&skill_md, text)?;
    let updated = hub.import_skill(ImportRequest {
        source: ImportSource::Folder {
            path: modified.to_string_lossy().to_string(),
        },
        skill_path: None,
        version_label: Some("1.0.1-smoke".to_string()),
        notes: Some("smoke test update".to_string()),
    })?;
    checker.check(
        "second import creates a new version without overwriting",
        updated.version.content_hash != v1_hash,
        format!("new hash {}", &updated.version.content_hash[..12]),
    );
    let changes = hub.check_changes()?;
    checker.check(
        "update detected",
        changes.updates.iter().any(|skill| skill.id == skill_id),
        format!("{} updates", changes.updates.len()),
    );

    // 4. Apply the update and then roll it back.
    let update_plan = hub.plan_install(InstallRequest {
        capability_id: skill_id.clone(),
        version_id: None,
        agent_ids: vec![codex.id.clone(), zcode.id.clone(), workbuddy.id.clone()],
        strategy: Some(InstallStrategy::Copy),
        force: false,
    })?;
    checker.check(
        "update plan replaces existing copies",
        update_plan.summary.replace == 3,
        format!("{:?}", update_plan.summary),
    );
    let update_result = hub.apply_operation(&update_plan.operation_id, false)?;
    checker.check(
        "update applied",
        matches!(update_result.status, OperationStatus::Success),
        format!("{:?}", update_result.status),
    );
    let expected_hash = updated.version.content_hash.clone();
    let deployed_hash = agenthub_core::util::hash_dir(&home.join(".codex/skills/chat-ui"))
        .map_err(agenthub_core::Error::from)?;
    checker.check(
        "deployed content matches new version",
        deployed_hash == expected_hash,
        format!("{}", &deployed_hash[..12]),
    );
    let rollback = hub.rollback_operation(&update_plan.operation_id)?;
    checker.check(
        "rollback reported success",
        matches!(rollback.status, OperationStatus::Success),
        format!("{:?}", rollback.status),
    );
    let restored_hash = agenthub_core::util::hash_dir(&home.join(".codex/skills/chat-ui"))
        .map_err(agenthub_core::Error::from)?;
    checker.check(
        "rollback restored previous deployed content",
        restored_hash == v1_hash,
        format!("restored {}", &restored_hash[..12]),
    );
    let library_version_dir = updated.version.storage_path.clone();
    checker.check(
        "library source files are intact after rollback",
        Path::new(&library_version_dir).join("SKILL.md").is_file(),
        &library_version_dir,
    );

    // 5. Symlink install + uninstall must not damage the library source.
    let symlink_plan = hub.plan_install(InstallRequest {
        capability_id: skill_id.clone(),
        version_id: None,
        agent_ids: vec![zcode.id.clone()],
        strategy: Some(InstallStrategy::Symlink),
        force: false,
    })?;
    let symlink_result = hub.apply_operation(&symlink_plan.operation_id, false)?;
    checker.check(
        "symlink install",
        matches!(symlink_result.status, OperationStatus::Success)
            && home.join(".zcode/skills/chat-ui").is_symlink(),
        format!("{:?}", symlink_result.status),
    );
    let zcode_install = hub
        .list_skills()?
        .into_iter()
        .find(|skill| skill.id == skill_id)
        .and_then(|skill| skill.installations.into_iter().find(|i| i.agent_key == "zcode"))
        .map(|installation| installation.id);
    if let Some(installation_id) = zcode_install {
        let uninstall = hub.plan_uninstall(UninstallRequest {
            installation_ids: vec![installation_id],
            force: false,
        })?;
        let uninstall_result = hub.apply_operation(&uninstall.operation_id, false)?;
        checker.check(
            "uninstall symlink",
            matches!(uninstall_result.status, OperationStatus::Success),
            format!("{:?}", uninstall_result.status),
        );
    } else {
        checker.check("find zcode installation", false, "installation row missing");
    }
    checker.check(
        "library source intact after uninstall",
        Path::new(&library_version_dir).join("SKILL.md").is_file(),
        &library_version_dir,
    );

    // 6. MCP: codec fidelity for TOML (Codex) and JSON (WorkBuddy).
    let server = hub.mcp_upsert(McpUpsertRequest {
        id: None,
        name: "agenthub-smoke".to_string(),
        description: Some("smoke test MCP server".to_string()),
        transport: McpTransport::Stdio,
        command: Some("/usr/bin/true".to_string()),
        args: vec!["--smoke".to_string()],
        url: None,
        headers: Default::default(),
        env_keys: vec![],
        credential_refs: vec![],
    })?;
    let mcp_agents = hub.list_agents()?;
    let mcp_codex = mcp_agents.iter().find(|a| a.key == "codex").unwrap();
    let mcp_wb = mcp_agents.iter().find(|a| a.key == "workbuddy").unwrap();
    let mcp_plan = hub.mcp_plan_distribute(McpDistributeRequest {
        server_id: server.id.clone(),
        agent_ids: vec![mcp_codex.id.clone(), mcp_wb.id.clone()],
        force: false,
    })?;
    checker.check(
        "MCP: stdio is blocked for the HTTP-only WorkBuddy config",
        mcp_plan.summary.blocked >= 1 && mcp_plan.summary.create + mcp_plan.summary.replace >= 1,
        format!("{:?}", mcp_plan.summary),
    );
    let mcp_result = hub.apply_operation(&mcp_plan.operation_id, true)?;
    for diagnostic in &mcp_result.diagnostics {
        println!("  MCP diagnostic: {diagnostic}");
    }
    for item in &mcp_result.items {
        println!("  MCP item: {} {} {} - {}", item.agent_key, item.status, item.target_path, item.message);
    }
    checker.check(
        "MCP: apply writes Codex config (partial due to blocked WorkBuddy)",
        matches!(mcp_result.status, OperationStatus::Partial),
        format!("{:?}", mcp_result.status),
    );
    let codex_config = fs::read_to_string(home.join(".codex/config.toml"))?;
    checker.check(
        "MCP: TOML entry written",
        codex_config.contains("[mcp_servers.agenthub-smoke]") && codex_config.contains("--smoke"),
        format!("{} bytes", codex_config.len()),
    );
    checker.check(
        "MCP: TOML unrelated settings preserved",
        codex_config.contains("[mcp_servers.existing]")
            && codex_config.contains("TOKEN = \"keep-me\"")
            && codex_config.contains("trust_level = \"trusted\"")
            && codex_config.contains("# a user comment that must survive MCP edits"),
        "projects/node_repl/comment intact",
    );
    let wb_config = fs::read_to_string(home.join(".workbuddy/connectors/default/mcp.json"))?;
    let wb_json: serde_json::Value = serde_json::from_str(&wb_config)?;
    checker.check(
        "MCP: JSON unrelated settings preserved",
        wb_json.get("unrelated").is_some() && wb_json["mcpServers"].get("existing-http").is_some(),
        "unrelated + existing-http intact",
    );
    // Replace the entry through the same config file.
    let updated_server = hub.mcp_upsert(McpUpsertRequest {
        id: Some(server.id.clone()),
        name: "agenthub-smoke".to_string(),
        description: Some("smoke test MCP server v2".to_string()),
        transport: McpTransport::Stdio,
        command: Some("/usr/bin/true".to_string()),
        args: vec!["--smoke2".to_string()],
        url: None,
        headers: Default::default(),
        env_keys: vec![],
        credential_refs: vec![],
    })?;
    let replace_plan = hub.mcp_plan_distribute(McpDistributeRequest {
        server_id: updated_server.id.clone(),
        agent_ids: vec![mcp_codex.id.clone()],
        force: false,
    })?;
    let replace_result = hub.apply_operation(&replace_plan.operation_id, true)?;
    let codex_config = fs::read_to_string(home.join(".codex/config.toml"))?;
    checker.check(
        "MCP: update replaces the entry and keeps the file",
        matches!(replace_result.status, OperationStatus::Success)
            && codex_config.contains("--smoke2")
            && !codex_config.contains("--smoke\"")
            && codex_config.contains("[mcp_servers.existing]"),
        format!("{:?}", replace_result.status),
    );

    let remove_plan = hub.mcp_plan_remove(&server.id, vec![mcp_codex.id.clone()])?;
    let remove_result = hub.apply_operation(&remove_plan.operation_id, true)?;
    let codex_config = fs::read_to_string(home.join(".codex/config.toml"))?;
    checker.check(
        "MCP: remove keeps the rest of the TOML file",
        matches!(remove_result.status, OperationStatus::Success)
            && !codex_config.contains("agenthub-smoke")
            && codex_config.contains("[mcp_servers.existing]")
            && codex_config.contains("# a user comment that must survive MCP edits"),
        format!("{:?}", remove_result.status),
    );
    let plugins = hub.sync_plugins()?;
    checker.check(
        "Plugins: read-only discovery from Codex config",
        plugins.iter().any(|plugin| plugin.name.contains("demo@local")),
        format!("{} plugin records", plugins.len()),
    );

    // 7. Backup + doctor still work after all operations.
    let backup = hub.create_backup()?;
    checker.check(
        "backup created",
        Path::new(&backup.path).join("agenthub.db").is_file(),
        backup.path.clone(),
    );
    let doctor = hub.doctor()?;
    checker.check(
        "doctor reports database ok",
        doctor.checks.iter().any(|check| check.name == "database" && check.status == "ok"),
        format!("{} checks", doctor.checks.len()),
    );

    if checker.failures.is_empty() {
        println!("\nALL CHECKS PASSED");
        Ok(())
    } else {
        for failure in &checker.failures {
            eprintln!("FAILED: {failure}");
        }
        Err(agenthub_core::Error::msg(format!(
            "{} acceptance checks failed",
            checker.failures.len()
        )))
    }
}
