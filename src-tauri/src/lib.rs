//! AgentHub Tauri command surface.
//!
//! These commands are thin wrappers over `agenthub_core::AgentHub`, which is the
//! same engine used by the `agenthub` CLI. Command argument names are snake_case
//! (see `rename_all`); all payload fields are camelCase.

use std::sync::Mutex;

use agenthub_core::model::*;
use agenthub_core::{AgentHub, CoreConfig};
use tauri::State;

struct AppState {
    hub: Mutex<Option<AgentHub>>,
    init_error: Mutex<Option<String>>,
}

impl AppState {
    fn new() -> Self {
        match CoreConfig::resolve() {
            Ok(config) => match AgentHub::open(config) {
                Ok(hub) => AppState {
                    hub: Mutex::new(Some(hub)),
                    init_error: Mutex::new(None),
                },
                Err(err) => AppState {
                    hub: Mutex::new(None),
                    init_error: Mutex::new(Some(format!(
                        "AgentHub could not open its local database: {err}"
                    ))),
                },
            },
            Err(err) => AppState {
                hub: Mutex::new(None),
                init_error: Mutex::new(Some(format!("Cannot resolve home directory: {err}"))),
            },
        }
    }

    fn run<T>(&self, action: impl FnOnce(&AgentHub) -> agenthub_core::Result<T>) -> Result<T, String> {
        let guard = self.hub.lock().map_err(|_| "state lock poisoned".to_string())?;
        let hub = guard.as_ref().ok_or_else(|| {
            self.init_error
                .lock()
                .ok()
                .and_then(|error| error.clone())
                .unwrap_or_else(|| "AgentHub is not initialized".to_string())
        })?;
        action(hub).map_err(|err| err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Bootstrap / settings
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
fn app_bootstrap(state: State<'_, AppState>) -> Result<Bootstrap, String> {
    state.run(|hub| hub.bootstrap())
}

#[tauri::command(rename_all = "snake_case")]
fn app_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    state.run(|hub| hub.dashboard())
}

#[tauri::command(rename_all = "snake_case")]
fn app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.run(|hub| hub.settings())
}

#[tauri::command(rename_all = "snake_case")]
fn app_update_settings(
    state: State<'_, AppState>,
    update: SettingsUpdate,
) -> Result<AppSettings, String> {
    state.run(|hub| hub.update_settings(update))
}

#[tauri::command(rename_all = "snake_case")]
fn app_mark_initialized(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.run(|hub| hub.mark_initialized())
}

#[tauri::command(rename_all = "snake_case")]
fn app_doctor(state: State<'_, AppState>) -> Result<DoctorReport, String> {
    state.run(|hub| hub.doctor())
}

#[tauri::command(rename_all = "snake_case")]
fn app_project_roots(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.run(|hub| {
        hub.project_roots()
            .map(|roots| roots.iter().map(|path| path.to_string_lossy().to_string()).collect())
    })
}

#[tauri::command(rename_all = "snake_case")]
fn app_set_project_roots(
    state: State<'_, AppState>,
    roots: Vec<String>,
) -> Result<Vec<String>, String> {
    state.run(|hub| hub.set_project_roots(roots))
}

// ---------------------------------------------------------------------------
// Agents & scan
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
fn agents_list(state: State<'_, AppState>) -> Result<Vec<AgentView>, String> {
    state.run(|hub| hub.list_agents())
}

#[tauri::command(rename_all = "snake_case")]
fn agents_update_paths(
    state: State<'_, AppState>,
    update: AgentPathUpdate,
) -> Result<AgentView, String> {
    state.run(|hub| hub.update_agent_paths(update))
}

#[tauri::command(rename_all = "snake_case")]
fn app_scan(state: State<'_, AppState>) -> Result<ScanReport, String> {
    state.run(|hub| hub.scan())
}

#[tauri::command(rename_all = "snake_case")]
fn app_check_changes(state: State<'_, AppState>) -> Result<ChangeCheck, String> {
    state.run(|hub| hub.check_changes())
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
fn skills_list(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    state.run(|hub| hub.list_skills())
}

#[tauri::command(rename_all = "snake_case")]
fn skills_get(state: State<'_, AppState>, id: String) -> Result<SkillDetail, String> {
    state.run(|hub| hub.get_skill(&id))
}

#[tauri::command(rename_all = "snake_case")]
fn skills_import(
    state: State<'_, AppState>,
    request: ImportRequest,
) -> Result<ImportResult, String> {
    state.run(|hub| hub.import_skill(request))
}

#[tauri::command(rename_all = "snake_case")]
fn skills_plan_install(
    state: State<'_, AppState>,
    request: InstallRequest,
) -> Result<OperationPlan, String> {
    state.run(|hub| hub.plan_install(request))
}

#[tauri::command(rename_all = "snake_case")]
fn skills_plan_uninstall(
    state: State<'_, AppState>,
    request: UninstallRequest,
) -> Result<OperationPlan, String> {
    state.run(|hub| hub.plan_uninstall(request))
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
fn operations_apply(
    state: State<'_, AppState>,
    operation_id: String,
    force: bool,
) -> Result<OperationResult, String> {
    state.run(|hub| hub.apply_operation(&operation_id, force))
}

#[tauri::command(rename_all = "snake_case")]
fn operations_rollback(
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<OperationResult, String> {
    state.run(|hub| hub.rollback_operation(&operation_id))
}

#[tauri::command(rename_all = "snake_case")]
fn operations_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<OperationView>, String> {
    state.run(|hub| hub.list_operations(limit.unwrap_or(60)))
}

#[tauri::command(rename_all = "snake_case")]
fn operations_get(
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<OperationView, String> {
    state.run(|hub| hub.get_operation(&operation_id))
}

#[tauri::command(rename_all = "snake_case")]
fn snapshots_list(
    state: State<'_, AppState>,
    operation_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<SnapshotView>, String> {
    state.run(|hub| match operation_id {
        Some(id) => hub.list_snapshots(&id),
        None => hub.list_snapshots_all(limit.unwrap_or(100)),
    })
}

#[tauri::command(rename_all = "snake_case")]
fn backups_create(state: State<'_, AppState>) -> Result<BackupResult, String> {
    state.run(|hub| hub.create_backup())
}

#[tauri::command(rename_all = "snake_case")]
fn backups_list(state: State<'_, AppState>) -> Result<Vec<BackupView>, String> {
    state.run(|hub| hub.list_backups())
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
fn mcp_list(state: State<'_, AppState>) -> Result<Vec<McpServerView>, String> {
    state.run(|hub| hub.mcp_servers())
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_discover(
    state: State<'_, AppState>,
    agent_ids: Option<Vec<String>>,
) -> Result<Vec<DiscoveredMcp>, String> {
    state.run(|hub| hub.mcp_discover(agent_ids))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_upsert(
    state: State<'_, AppState>,
    request: McpUpsertRequest,
) -> Result<McpServerView, String> {
    state.run(|hub| hub.mcp_upsert(request))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_import(
    state: State<'_, AppState>,
    request: McpImportRequest,
) -> Result<McpServerView, String> {
    state.run(|hub| hub.mcp_import(request))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_plan_distribute(
    state: State<'_, AppState>,
    request: McpDistributeRequest,
) -> Result<OperationPlan, String> {
    state.run(|hub| hub.mcp_plan_distribute(request))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_plan_remove(
    state: State<'_, AppState>,
    server_id: String,
    agent_ids: Vec<String>,
) -> Result<OperationPlan, String> {
    state.run(|hub| hub.mcp_plan_remove(&server_id, agent_ids))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_secret_states(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<SecretState>, String> {
    state.run(|hub| hub.mcp_secret_states(&server_id))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_set_secret(
    state: State<'_, AppState>,
    server_id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    state.run(|hub| hub.mcp_set_secret(&server_id, &key, &value))
}

#[tauri::command(rename_all = "snake_case")]
fn mcp_delete_secret(
    state: State<'_, AppState>,
    server_id: String,
    key: String,
) -> Result<(), String> {
    state.run(|hub| hub.mcp_delete_secret(&server_id, &key))
}

// ---------------------------------------------------------------------------
// Plugins & profiles
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
fn plugins_sync(state: State<'_, AppState>) -> Result<Vec<PluginRecordView>, String> {
    state.run(|hub| hub.sync_plugins())
}

#[tauri::command(rename_all = "snake_case")]
fn profiles_list(state: State<'_, AppState>) -> Result<Vec<ProfileView>, String> {
    state.run(|hub| hub.list_profiles())
}

#[tauri::command(rename_all = "snake_case")]
fn profiles_upsert(
    state: State<'_, AppState>,
    request: ProfileUpsertRequest,
) -> Result<ProfileView, String> {
    state.run(|hub| hub.upsert_profile(request))
}

#[tauri::command(rename_all = "snake_case")]
fn profiles_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.run(|hub| hub.delete_profile(&id))
}

#[tauri::command(rename_all = "snake_case")]
fn profiles_plan_apply(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<OperationPlan, String> {
    state.run(|hub| hub.plan_profile_apply(&profile_id))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            app_bootstrap,
            app_dashboard,
            app_settings,
            app_update_settings,
            app_mark_initialized,
            app_doctor,
            app_project_roots,
            app_set_project_roots,
            agents_list,
            agents_update_paths,
            app_scan,
            app_check_changes,
            skills_list,
            skills_get,
            skills_import,
            skills_plan_install,
            skills_plan_uninstall,
            operations_apply,
            operations_rollback,
            operations_list,
            operations_get,
            snapshots_list,
            backups_create,
            backups_list,
            mcp_list,
            mcp_discover,
            mcp_upsert,
            mcp_import,
            mcp_plan_distribute,
            mcp_plan_remove,
            mcp_secret_states,
            mcp_set_secret,
            mcp_delete_secret,
            plugins_sync,
            profiles_list,
            profiles_upsert,
            profiles_delete,
            profiles_plan_apply
        ])
        .run(tauri::generate_context!())
        .expect("error while running AgentHub");
}
