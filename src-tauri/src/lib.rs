mod audit_bridge;
mod commands;
mod tauri_event_bus;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::{
    CollectionService, ContractService, CookieService, GitAppService,
    HistoryService, RequestExecutionService, SecurityAuditService, TemplateService,
    WorkspaceService,
};
use rocket_audit::publisher::SecurityAuditPublisher;
use rocket_infra::{
    FsAuditLogRepo, FsCollectionRepo, FsComplianceProfileRepo, FsContractRepo, FsCookieRepo,
    FsEnvironmentRepo, FsHistoryRepo, FsTemplateRepo, FsWorkspaceRepo, FsWorkspaceConfigRepo,
    NotifyFileWatcher, ReqwestExecutor, SharedPathCollectionRepo,
};
use rocket_workspace::WorkspaceConfigRepository;
use rocket_shared::events::NullEventPublisher;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Structured logging subscriber with reload layer for TauriTracingLayer.
    // The reload layer starts as None and gets hot-swapped in .setup() once
    // the AppHandle is available.
    use tracing_subscriber::{fmt, prelude::*, reload, EnvFilter, Registry};

    type TauriReloadLayer = reload::Layer<
        Option<rocket_infra::TauriTracingLayer>,
        Registry,
    >;
    type TauriReloadHandle = reload::Handle<
        Option<rocket_infra::TauriTracingLayer>,
        Registry,
    >;

    let env_filter = EnvFilter::try_from_env("ROCKET_LOG")
        .or_else(|_| EnvFilter::try_from_env("RUST_LOG"))
        .unwrap_or_else(|_| EnvFilter::new("info,git2=warn,reqwest=warn,hyper=warn"));

    let (tauri_layer, reload_handle): (TauriReloadLayer, TauriReloadHandle) =
        reload::Layer::new(None::<rocket_infra::TauriTracingLayer>);

    if cfg!(debug_assertions) {
        tracing_subscriber::registry()
            .with(tauri_layer)
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_file(false)
                    .with_line_number(false)
                    .pretty(),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tauri_layer)
            .with(env_filter)
            .with(
                fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_span_list(true)
                    .flatten_event(true),
            )
            .init();
    }

    // Bridge log crate to tracing for transitive deps (reqwest, notify, git2).
    let _ = tracing_log::LogTracer::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Activate the Tauri tracing layer now that we have an AppHandle.
            let tauri_tracing = rocket_infra::TauriTracingLayer::new(app_handle.clone());
            if let Err(e) = reload_handle.modify(|layer| *layer = Some(tauri_tracing)) {
                eprintln!("Failed to activate TauriTracingLayer: {e}");
            }

            // Determine the application data directory.
            let data_dir = dirs::home_dir()
                .ok_or("Home directory could not be determined")?
                .join(".rocket-api");
            std::fs::create_dir_all(&data_dir).ok();

            // Workspace service — manages workspace switching.
            let active_workspace_path: Arc<Mutex<PathBuf>> =
                Arc::new(Mutex::new(PathBuf::new()));
            let workspace_repo = Box::new(FsWorkspaceRepo::new(data_dir.clone()));
            let workspace_config_repo = Box::new(FsWorkspaceConfigRepo::new());
            let workspace_svc = WorkspaceService::new(
                workspace_repo,
                workspace_config_repo,
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Arc::clone(&active_workspace_path),
            );

            // Bootstrap the active workspace path from persisted state.
            let active_ws = workspace_svc
                .get_active()
                .map_err(|e| format!("Failed to load active workspace: {e}"))?;
            *active_workspace_path.lock().map_err(|e| format!("Workspace path lock poisoned: {e}"))? = active_ws.path.clone();

            // Ensure the default workspace has a workspace.yml on first launch.
            let ws_yml = active_ws.path.join("workspace.yml");
            if !ws_yml.exists() {
                let config_repo = FsWorkspaceConfigRepo::new();
                let config = rocket_workspace::WorkspaceConfig::new(&active_ws.name);
                let _ = config_repo.save(&active_ws.path, &config);
            }

            // Derive per-service directories from the active workspace.
            let workspace_base = active_ws.path.clone();
            let collections_dir = workspace_base.join("collections");
            let environments_dir = workspace_base.join("environments");
            let history_dir = workspace_base.join("history");
            let templates_dir = workspace_base.join("templates");
            let cookies_dir = workspace_base.join("cookies");

            for dir in [
                &collections_dir,
                &environments_dir,
                &history_dir,
                &templates_dir,
                &cookies_dir,
            ] {
                std::fs::create_dir_all(dir).ok();
            }

            // Event buses — publish domain events to the frontend.
            let watcher_bus =
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone()));

            // Security audit: tamper-evident event log + compliance profile.
            // Lives under data_dir (not workspace) so the log persists across
            // workspace switches.
            let audit_dir = data_dir.join("audit");
            std::fs::create_dir_all(&audit_dir).ok();
            let audit_log_repo = Arc::new(
                FsAuditLogRepo::new(audit_dir.join("events.jsonl"))
                    .map_err(|e| format!("Failed to initialise audit log: {e}"))?,
            );
            let profile_repo = Arc::new(
                FsComplianceProfileRepo::new(audit_dir.join("profile.yml"))
                    .map_err(|e| format!("Failed to initialise compliance profile: {e}"))?,
            );
            let audit_svc = Arc::new(
                SecurityAuditService::new(audit_log_repo.clone(), profile_repo.clone())
                    .map_err(|e| format!("Failed to initialise audit service: {e}"))?,
            );
            let audit_publisher: Arc<dyn SecurityAuditPublisher> = Arc::new(
                audit_bridge::ServiceBackedAuditPublisher::new(audit_svc.clone()),
            );

            // Application services — no event publishing.
            // The file watcher is the single source of truth for sidebar updates.
            // SharedPathCollectionRepo resolves the base directory from
            // active_workspace_path at call time, so switching workspaces
            // automatically redirects all collection reads/writes.
            let collection_svc = CollectionService::new_with_audit(
                Box::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))),
                audit_publisher.clone(),
            );
            let history_svc = HistoryService::new(
                Box::new(FsHistoryRepo::new(history_dir.clone())),
                Box::new(NullEventPublisher),
            );
            let template_svc = TemplateService::new(
                Box::new(FsTemplateRepo::new(templates_dir)),
                Box::new(NullEventPublisher),
            );
            let cookie_svc = CookieService::new(
                Box::new(FsCookieRepo::new(cookies_dir.clone())),
                Box::new(NullEventPublisher),
            );
            let exec_svc = RequestExecutionService::new_with_audit(
                Box::new(FsEnvironmentRepo::new(environments_dir.clone())),
                Arc::new(ReqwestExecutor::with_allowed_base(Arc::clone(&active_workspace_path))),
                Box::new(FsHistoryRepo::new(history_dir)),
                Box::new(FsCollectionRepo::new(collections_dir.clone())),
                Box::new(FsCookieRepo::new(cookies_dir)),
                Box::new(NullEventPublisher),
                audit_publisher.clone(),
            );

            // OAuth2Service — stand-alone service for token acquisition flows.
            // Uses its own repo instances pointed at the same paths as the exec service.
            let oauth2_svc = rocket_app::oauth2_service::OAuth2Service::new(
                Box::new(FsEnvironmentRepo::new(environments_dir)),
                Box::new(FsCollectionRepo::new(collections_dir.clone())),
            );

            let git_svc = GitAppService::new(
                Box::new(rocket_git::Git2Service::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );

            // Contract service — owns the save-hook audit log.
            // FsContractRepo is stateless; it receives the per-collection
            // directory on every call, computed as <workspace>/collections/<name>.
            // A second SharedPathCollectionRepo sharing the same workspace
            // path lets the service walk collections at contract-attach time
            // without duplicating filesystem state.
            let contract_svc = ContractService::new_with_audit(
                Arc::new(FsContractRepo),
                Arc::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))),
                audit_publisher.clone(),
            );

            // Register all services as Tauri managed state.
            app.manage(collection_svc);
            app.manage(contract_svc);
            app.manage(history_svc);
            app.manage(template_svc);
            app.manage(cookie_svc);
            app.manage(exec_svc);
            app.manage(oauth2_svc);
            app.manage(git_svc);
            app.manage(audit_svc);
            app.manage(Mutex::new(workspace_svc));
            app.manage(active_workspace_path);

            // Start filesystem watcher for the collections directory.
            let watcher = NotifyFileWatcher::new();
            let _ = watcher.start(collections_dir, watcher_bus.clone());
            app.manage(watcher);

            // Share the event bus so commands like watch_collections can reuse it.
            app.manage(watcher_bus);

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                    width: 1440,
                    height: 900,
                }));
            }

            tracing::info!(data_dir = %data_dir.display(), "RocketAPI initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::collections::list_collections,
            commands::collections::get_collection,
            commands::collections::create_collection,
            commands::collections::delete_collection,
            commands::collections::rename_collection,
            commands::collections::save_request,
            commands::collections::rename_request,
            commands::collections::delete_request,
            commands::collections::create_folder,
            commands::collections::delete_folder,
            commands::collections::move_item,
            commands::collections::reorder_items,
            commands::collections::get_collection_settings,
            commands::collections::save_collection_settings,
            commands::collections::scan_collections_in_path,
            commands::collections::detect_cloned_structure,
            commands::collections::get_folder_chain_variables,
            commands::collections::get_folder_variables,
            commands::collections::save_folder_variables,
            commands::collections::get_request_variables,
            commands::collections::save_request_variables,
            commands::collections::update_request_docs,
            commands::environments::list_environments,
            commands::environments::get_environment,
            commands::environments::save_environment,
            commands::environments::delete_environment,
            commands::environments::get_global_environment_name,
            commands::environments::set_global_environment,
            commands::environments::get_process_env_vars,
            commands::environments::list_global_environments,
            commands::environments::get_global_environment,
            commands::environments::save_global_environment,
            commands::environments::delete_global_environment,
            commands::execution::execute_request,
            commands::load_test::run_load_test_command,
            commands::history::list_history,
            commands::history::get_history_entry,
            commands::history::clear_history,
            commands::history::search_history,
            commands::templates::list_templates,
            commands::templates::get_template,
            commands::templates::save_template,
            commands::templates::delete_template,
            commands::cookies::get_cookies,
            commands::cookies::set_cookies,
            commands::cookies::clear_cookies,
            commands::app::get_app_data_dir,
            commands::app::watch_collections,
            commands::app::stop_watching,
            commands::audit::list_audit_events,
            commands::audit::list_audit_events_range,
            commands::audit::get_compliance_profile,
            commands::audit::set_compliance_profile,
            commands::audit::export_audit_evidence,
            commands::audit::save_audit_evidence_file,
            commands::oauth2::oauth2_auth_code_flow,
            commands::oauth2::oauth2_decode_jwt,
            commands::oauth2::oauth2_get_token,
            commands::oauth2::oauth2_refresh_token,
            commands::git::git_is_repo,
            commands::git::git_init,
            commands::git::git_clone,
            commands::git::git_status,
            commands::git::git_diff,
            commands::git::git_diff_staged,
            commands::git::git_diff_commit,
            commands::git::git_stage,
            commands::git::git_unstage,
            commands::git::git_discard,
            commands::git::git_commit,
            commands::git::git_log,
            commands::git::git_push,
            commands::git::git_pull,
            commands::git::git_fetch,
            commands::git::git_branches,
            commands::git::git_switch_branch,
            commands::git::git_checkout_remote_branch,
            commands::git::git_create_branch,
            commands::git::git_delete_branch,
            commands::git::git_merge_branch,
            commands::git::git_stash_list,
            commands::git::git_stash_save,
            commands::git::git_stash_pop,
            commands::git::git_stash_apply,
            commands::git::git_stash_drop,
            commands::git::git_conflicts,
            commands::git::git_resolve_conflict,
            commands::git::git_abort_merge,
            commands::git::git_list_remotes,
            commands::git::git_add_remote,
            commands::git::git_remove_remote,
            commands::git::git_set_remote_url,
            commands::git::get_default_ssh_key_path,
            commands::git::save_git_credentials,
            commands::git::load_git_credentials,
            commands::workspaces::list_workspaces,
            commands::workspaces::get_active_workspace,
            commands::workspaces::create_workspace,
            commands::workspaces::switch_workspace,
            commands::workspaces::rename_workspace,
            commands::workspaces::close_workspace,
            commands::workspaces::delete_workspace,
            commands::workspaces::pin_workspace,
            commands::workspaces::unpin_workspace,
            commands::workspaces::update_workspace_description,
            commands::workspaces::open_workspace,
            commands::workspaces::get_workspace_config,
            commands::workspaces::get_multi_workspace_mode,
            commands::workspaces::set_multi_workspace_mode,
            commands::workspaces::open_folder_picker,
            commands::workspaces::link_external_collection,
            commands::ui_state::load_ui_state,
            commands::ui_state::save_ui_state,
            commands::import::import_bruno,
            commands::import::import_bruno_zip,
            commands::contract::attach_contract,
            commands::contract::update_contract,
            commands::contract::list_contracts,
            commands::contract::get_contract,
            commands::contract::delete_contract,
            commands::contract::get_contract_changelog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
