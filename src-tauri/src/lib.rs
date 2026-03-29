mod commands;
mod tauri_event_bus;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::{
    CollectionService, CookieService, EnvironmentService, GitAppService,
    HistoryService, RequestExecutionService, TemplateService, WorkspaceService,
};
use rocket_infra::{
    FsCollectionRepo, FsCookieRepo, FsEnvironmentRepo, FsHistoryRepo, FsTemplateRepo,
    FsWorkspaceRepo, NotifyFileWatcher, ReqwestExecutor, SharedPathCollectionRepo,
};
use rocket_shared::events::NullEventPublisher;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Determine the application data directory.
            let data_dir = dirs::home_dir()
                .expect("Home directory not found")
                .join(".rocket-api");
            std::fs::create_dir_all(&data_dir).ok();

            // Workspace service — manages workspace switching.
            let active_workspace_path: Arc<Mutex<PathBuf>> =
                Arc::new(Mutex::new(PathBuf::new()));
            let workspace_repo = Box::new(FsWorkspaceRepo::new(data_dir.clone()));
            let workspace_svc = WorkspaceService::new(
                workspace_repo,
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Arc::clone(&active_workspace_path),
            );

            // Bootstrap the active workspace path from persisted state.
            let active_ws = workspace_svc
                .get_active()
                .expect("failed to load active workspace on startup");
            *active_workspace_path.lock().unwrap() = active_ws.path.clone();

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
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle));

            // Application services — no event publishing.
            // The file watcher is the single source of truth for sidebar updates.
            // SharedPathCollectionRepo resolves the base directory from
            // active_workspace_path at call time, so switching workspaces
            // automatically redirects all collection reads/writes.
            let collection_svc = CollectionService::new(
                Box::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))),
            );
            let env_svc = EnvironmentService::new(
                Box::new(FsEnvironmentRepo::new(environments_dir.clone())),
                Box::new(NullEventPublisher),
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
            let exec_svc = RequestExecutionService::new(
                Box::new(FsEnvironmentRepo::new(environments_dir)),
                Box::new(ReqwestExecutor::new()),
                Box::new(FsHistoryRepo::new(history_dir)),
                Box::new(FsCollectionRepo::new(collections_dir.clone())),
                Box::new(FsCookieRepo::new(cookies_dir)),
                Box::new(NullEventPublisher),
            );

            let git_svc = GitAppService::new(
                Box::new(rocket_git::Git2Service::new()),
                Box::new(NullEventPublisher),
            );

            // Register all services as Tauri managed state.
            app.manage(collection_svc);
            app.manage(env_svc);
            app.manage(history_svc);
            app.manage(template_svc);
            app.manage(cookie_svc);
            app.manage(exec_svc);
            app.manage(git_svc);
            app.manage(Mutex::new(workspace_svc));
            app.manage(active_workspace_path);

            // Start filesystem watcher for the collections directory.
            let watcher = NotifyFileWatcher::new();
            let _ = watcher.start(collections_dir, watcher_bus.clone());
            app.manage(watcher);

            // Share the event bus so commands like watch_collections can reuse it.
            app.manage(watcher_bus);

            log::info!("RocketAPI initialized at {:?}", data_dir);
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
            commands::environments::list_environments,
            commands::environments::get_environment,
            commands::environments::save_environment,
            commands::environments::delete_environment,
            commands::execution::execute_request,
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
            commands::oauth2::oauth2_auth_code_flow,
            commands::git::git_is_repo,
            commands::git::git_init,
            commands::git::git_clone,
            commands::git::git_status,
            commands::git::git_diff,
            commands::git::git_diff_staged,
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
            commands::workspaces::list_workspaces,
            commands::workspaces::get_active_workspace,
            commands::workspaces::create_workspace,
            commands::workspaces::switch_workspace,
            commands::workspaces::rename_workspace,
            commands::workspaces::close_workspace,
            commands::workspaces::delete_workspace,
            commands::workspaces::open_folder_picker,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
