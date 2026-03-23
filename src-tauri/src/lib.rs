mod commands;
mod tauri_event_bus;

use std::sync::Arc;

use rocket_app::{
    CollectionService, CookieService, EnvironmentService, HistoryService,
    RequestExecutionService, TemplateService,
};
use rocket_infra::{
    FsCollectionRepo, FsCookieRepo, FsEnvironmentRepo, FsHistoryRepo, FsTemplateRepo,
    NotifyFileWatcher, ReqwestExecutor,
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
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Determine data directories.
            let data_dir = dirs::home_dir()
                .expect("Home directory not found")
                .join(".rocket-api");
            let collections_dir = data_dir.join("collections");
            let environments_dir = data_dir.join("environments");
            let history_dir = data_dir.join("history");
            let templates_dir = data_dir.join("templates");
            let cookies_dir = data_dir.join("cookies");

            for dir in [
                &data_dir,
                &collections_dir,
                &environments_dir,
                &history_dir,
                &templates_dir,
                &cookies_dir,
            ] {
                std::fs::create_dir_all(dir).ok();
            }

            // Event bus for collection service — publishes domain events to the frontend.
            let event_bus =
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle));

            // Application services with injected infra repos.
            let collection_svc = CollectionService::new(
                Box::new(FsCollectionRepo::new(collections_dir.clone())),
                event_bus,
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
                Box::new(FsCookieRepo::new(cookies_dir)),
                Box::new(NullEventPublisher),
            );

            // Register all services as Tauri managed state.
            app.manage(collection_svc);
            app.manage(env_svc);
            app.manage(history_svc);
            app.manage(template_svc);
            app.manage(cookie_svc);
            app.manage(exec_svc);

            // Start filesystem watcher for the collections directory.
            let watcher = NotifyFileWatcher::new();
            let _ = watcher.start(
                collections_dir,
                Arc::new(NullEventPublisher),
            );
            app.manage(watcher);

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
            commands::collections::delete_request,
            commands::collections::create_folder,
            commands::collections::delete_folder,
            commands::collections::move_item,
            commands::environments::list_environments,
            commands::environments::get_environment,
            commands::environments::save_environment,
            commands::environments::delete_environment,
            commands::execution::execute_request,
            commands::history::list_history,
            commands::history::get_history_entry,
            commands::history::clear_history,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
