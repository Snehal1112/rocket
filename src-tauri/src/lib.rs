mod audit_bridge;
mod callback_adapter;
// `pub` so integration tests in `src-tauri/tests/` can drive command
// orchestration directly (e.g. `commands::acp_sessions::
// start_workspace_assistant_inner`) — mirrors why `mcp` below is already `pub`.
pub mod agent_session;
pub mod commands;
pub mod mcp;
mod tauri_event_bus;
mod tauri_tracing_layer;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::{
    CollectionRunnerService, CollectionService, ContractService, CookieService, GitAppService,
    HistoryService, RequestExecutionService, SecurityAuditService, TemplateService,
    WorkspaceService,
};
use rocket_audit::publisher::SecurityAuditPublisher;
use rocket_environment::secret_store::SecretStore;
use rocket_infra::{
    scripting::DenoScriptEngine, CloneDestinationCapabilities, FsAuditLogRepo, FsCollectionRepo,
    FsComplianceProfileRepo, FsContractRepo, FsCookieRepo, FsEnvironmentRepo,
    FsRepositoryPathResolver, FsTemplateRepo, FsWorkspaceConfigRepo, FsWorkspaceRepo,
    KeyringSecretStore, NotifyFileWatcher, ReqwestExecutor, SharedCollectionEnvironmentRepo,
    SharedPathCollectionRepo,
};
use rocket_shared::events::NullEventPublisher;
use rocket_workspace::WorkspaceConfigRepository;
use tauri::Manager;

/// OS-keychain backend for environment secret values.
///
/// `KeyringSecretStore::new_env_secrets()` builds a handle scoped to the
/// environment-secrets keychain namespace; this helper keeps the concrete
/// type and that namespace choice in one place. Every `FsEnvironmentRepo`
/// that serves user-facing environments must be built with it —
/// `FsEnvironmentRepo::new` silently drops secret values.
pub(crate) fn env_secret_store() -> Arc<dyn SecretStore> {
    Arc::new(KeyringSecretStore::new_env_secrets())
}

/// Memory-pressure settings for WebKit's network process.
///
/// WebKitGTK only lets these reach the network process from here. The web
/// process takes them from a construct-only `WebContext` property, which wry
/// creates internally and does not expose. The network process disables its
/// periodic memory check by default and only turns it on when custom
/// settings are set, so this enables proactive trimming there.
///
/// WebKit's defaults are a 3072 MB limit, thresholds of 0.33 and 0.5, no
/// kill threshold, and a 30 s poll. Rocket's network process idles around
/// 15 MB and only carries its own app assets and IPC traffic, so a 128 MB
/// limit starts releasing non-critical memory at 32 MB and critical memory
/// at about 51 MB. That leaves idle usage well below the first threshold,
/// so WebKit does not keep trimming in a loop. The kill threshold stays at
/// 0, which disables killing, because a restarted process is worse than
/// the memory it would save.
#[cfg(target_os = "linux")]
fn network_memory_pressure_settings() -> webkit2gtk::MemoryPressureSettings {
    use webkit2gtk::glib::translate::from_glib_full;
    // SAFETY: `MemoryPressureSettings::new()` asserts that GTK is initialized,
    // but the C constructor only allocates a plain settings struct. It
    // returns a new owned box, which `from_glib_full` takes ownership of.
    let mut settings: webkit2gtk::MemoryPressureSettings =
        unsafe { from_glib_full(webkit2gtk::ffi::webkit_memory_pressure_settings_new()) };
    settings.set_memory_limit(128);
    // Conservative must stay below strict, so set it first.
    settings.set_conservative_threshold(0.25);
    settings.set_strict_threshold(0.4);
    settings.set_kill_threshold(0.0);
    settings.set_poll_interval(30.0);
    settings
}

/// Resolves on the next delivery of `stream`'s signal. A missing or closed
/// stream never resolves, so it cannot trigger a spurious exit.
#[cfg(unix)]
async fn next_signal(stream: &mut Option<tokio::signal::unix::Signal>) {
    if let Some(stream) = stream {
        if stream.recv().await.is_some() {
            return;
        }
    }
    std::future::pending::<()>().await
}

/// Turns SIGINT, SIGTERM, and SIGHUP into a normal app exit.
///
/// Without this, those signals end Rocket without `RunEvent::Exit`, and any
/// running ACP agent (with its API key in its environment) is orphaned. The
/// first signal kills all agent sessions and requests a normal exit. A second
/// signal exits at once, in case the event loop is stuck.
#[cfg(unix)]
fn spawn_exit_signal_listener(app_handle: tauri::AppHandle) {
    use tokio::signal::unix::{signal, SignalKind};

    tauri::async_runtime::spawn(async move {
        let mut interrupt = signal(SignalKind::interrupt()).ok();
        let mut terminate = signal(SignalKind::terminate()).ok();
        let mut hangup = signal(SignalKind::hangup()).ok();
        if interrupt.is_none() && terminate.is_none() && hangup.is_none() {
            return;
        }

        for attempt in 0.. {
            tokio::select! {
                _ = next_signal(&mut interrupt) => {}
                _ = next_signal(&mut terminate) => {}
                _ = next_signal(&mut hangup) => {}
            }
            if attempt > 0 {
                std::process::exit(1);
            }
            if let Some(acp_session_svc) = app_handle.try_state::<rocket_app::AcpSessionService>() {
                let _ = acp_session_svc.end_all_sessions().await;
            }
            if let Some(websocket_svc) = app_handle.try_state::<rocket_app::WebSocketService>() {
                websocket_svc.end_all_sessions().await;
            }
            if let Some(svc) = app_handle.try_state::<rocket_app::GraphQlSubscriptionService>() {
                svc.end_all().await;
            }
            if let Some(grpc_svc) = app_handle.try_state::<rocket_app::GrpcService>() {
                grpc_svc.end_all();
            }
            if let Some(mcp_registry) =
                app_handle.try_state::<Arc<mcp::registry::McpServerRegistry>>()
            {
                mcp_registry.shutdown_all();
            }
            if let Some(resources) =
                app_handle.try_state::<Arc<agent_session::cleanup::SessionResourceRegistry>>()
            {
                // Backstop for a session that started while end_all_sessions ran.
                resources.clear_all();
            }
            app_handle.exit(0);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Work around a WebKitGTK GPU-compositing bug that renders the Scripts tab
    // Monaco editors transparent. Set before any thread or webview exists.
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    // Must run before any WebsiteDataManager (and so any webview) exists.
    #[cfg(target_os = "linux")]
    {
        use webkit2gtk::glib::translate::ToGlibPtrMut;
        let mut settings = network_memory_pressure_settings();
        // SAFETY: The safe binding asserts that GTK is initialized, which
        // tao only does later. The C function just copies the settings into
        // process-global state and needs no GTK, and we are on the main
        // thread before any WebsiteDataManager exists.
        unsafe {
            webkit2gtk::ffi::webkit_website_data_manager_set_memory_pressure_settings(
                settings.to_glib_none_mut().0,
            );
        }
    }

    // Bound libgit2's network operations before anything else runs. These
    // write process-global C state with no synchronization, so they are only
    // safe here, before any other thread exists. Without this, a stalled
    // connection (e.g. an unreachable remote) blocks the calling thread
    // forever instead of failing — this only bounds the HTTPS transport;
    // libgit2 has no equivalent timeout for its SSH (libssh2) transport.
    unsafe {
        let _ = git2::opts::set_server_connect_timeout_in_milliseconds(10_000);
        let _ = git2::opts::set_server_timeout_in_milliseconds(60_000);
    }

    // Structured logging subscriber with reload layer for TauriTracingLayer.
    // The reload layer starts as None and gets hot-swapped in .setup() once
    // the AppHandle is available.
    use tracing_subscriber::{fmt, prelude::*, reload, EnvFilter, Registry};

    type TauriReloadLayer = reload::Layer<Option<tauri_tracing_layer::TauriTracingLayer>, Registry>;
    type TauriReloadHandle =
        reload::Handle<Option<tauri_tracing_layer::TauriTracingLayer>, Registry>;

    let env_filter = EnvFilter::try_from_env("ROCKET_LOG")
        .or_else(|_| EnvFilter::try_from_env("RUST_LOG"))
        .unwrap_or_else(|_| EnvFilter::new("info,git2=warn,reqwest=warn,hyper=warn"))
        // agent-client-protocol logs raw JSON-RPC messages (including the MCP
        // bearer token) at debug/trace. Cap this target at info regardless of
        // RUST_LOG/ROCKET_LOG so raising verbosity elsewhere never leaks it.
        .add_directive(
            "agent_client_protocol=info"
                .parse()
                .expect("static directive is valid"),
        );

    let (tauri_layer, reload_handle): (TauriReloadLayer, TauriReloadHandle) =
        reload::Layer::new(None::<tauri_tracing_layer::TauriTracingLayer>);

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
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Activate the Tauri tracing layer now that we have an AppHandle.
            let tauri_tracing = tauri_tracing_layer::TauriTracingLayer::new(app_handle.clone());
            if let Err(e) = reload_handle.modify(|layer| *layer = Some(tauri_tracing)) {
                eprintln!("Failed to activate TauriTracingLayer: {e}");
            }

            // Determine the application data directory.
            let data_dir = dirs::home_dir()
                .ok_or("Home directory could not be determined")?
                .join(".rocket-api");
            std::fs::create_dir_all(&data_dir).ok();

            // Workspace service — manages workspace switching.
            let active_workspace_path: Arc<Mutex<PathBuf>> = Arc::new(Mutex::new(PathBuf::new()));
            let workspace_repo = Box::new(FsWorkspaceRepo::new(data_dir.clone()));
            let workspace_config_repo = Box::new(FsWorkspaceConfigRepo::new());
            let workspace_svc = WorkspaceService::new_with_repository_locator(
                workspace_repo,
                workspace_config_repo,
                Box::new(FsRepositoryPathResolver::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Arc::clone(&active_workspace_path),
            );

            // Bootstrap the active workspace path from persisted state.
            let active_ws = workspace_svc
                .get_active()
                .map_err(|e| format!("Failed to load active workspace: {e}"))?;
            *active_workspace_path
                .lock()
                .map_err(|e| format!("Workspace path lock poisoned: {e}"))? =
                active_ws.path.clone();

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
            let watcher_bus = Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone()));

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

            // What the user allowed per collection on this computer. One store is
            // shared by every service, so a grant is seen by all of them at once.
            let trust_store: Arc<dyn rocket_collection::CollectionTrustStore> =
                Arc::new(rocket_infra::FsCollectionTrustStore::new(data_dir.join("trust.yml")));
            let trust_svc = rocket_app::CollectionTrustService::new(
                Arc::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Arc::clone(&trust_store),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );
            // One-time grandfathering of the collections that exist before the trust gate.
            // It runs before any command is served.
            let migration = workspace_svc.list().and_then(|workspaces| {
                let paths: Vec<PathBuf> = workspaces.into_iter().map(|w| w.path).collect();
                let found = rocket_infra::discover_legacy_collections(
                    &paths,
                    &FsWorkspaceConfigRepo::new(),
                );
                trust_svc.migrate_legacy(found)
            });
            if let Err(e) = migration {
                // Fail closed: mark the store migrated with nothing grandfathered, so a
                // later start cannot grandfather collections cloned after the upgrade.
                tracing::warn!(error = %e, "trust migration failed, no collection was grandfathered");
                if let Err(e) = trust_store.complete_migration(Vec::new(), Vec::new()) {
                    tracing::warn!(error = %e, "could not mark the trust store as migrated");
                }
            }

            // SharedPathCollectionRepo resolves the base directory from
            // active_workspace_path at call time, so switching workspaces
            // automatically redirects all collection reads/writes. The file
            // watcher (started further below) remains a fallback for changes
            // made outside the app; this service publishes its own events for
            // deterministic, immediate sidebar/tree refresh on success.
            let collection_svc = CollectionService::new_with_audit(
                Box::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit_publisher.clone(),
            )
            .with_trust_store(Arc::clone(&trust_store));
            // Holds the assistant's proposed changes until the user accepts
            // them. It applies them through its own CollectionService, built
            // like collection_svc above, so accepted changes follow workspace
            // switches and publish the same events as manual edits. The
            // environment factory keeps secret values on a write-back.
            let proposal_svc = Arc::new(rocket_app::ProposalService::new(
                CollectionService::new_with_audit(
                    Box::new(SharedPathCollectionRepo::new(Arc::clone(
                        &active_workspace_path,
                    ))),
                    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                    audit_publisher.clone(),
                ),
                Arc::new(SharedCollectionEnvironmentRepo::with_secret_store(
                    Arc::clone(&active_workspace_path),
                    env_secret_store(),
                )),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            )
            .with_workspace_identity({
                // A proposal is stale once another workspace is active.
                let active = Arc::clone(&active_workspace_path);
                Arc::new(move || match active.lock() {
                    Ok(path) => path.display().to_string(),
                    // Fail closed: a poisoned lock gives a value that never
                    // equals a stored identity, so proposals turn stale.
                    Err(_) => {
                        use std::sync::atomic::{AtomicU64, Ordering};
                        static UNREADABLE: AtomicU64 = AtomicU64::new(0);
                        format!(
                            "unreadable-workspace-{}",
                            UNREADABLE.fetch_add(1, Ordering::Relaxed)
                        )
                    }
                })
            }));
            let history_svc = HistoryService::new(
                Box::new(rocket_infra::SharedPathHistoryRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
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
            // App-level proxy: the service validates and stores it, the executor reads it.
            let shared_proxy = rocket_http::new_shared_proxy();
            let proxy_svc = rocket_app::ProxySettingsService::new(
                Box::new(rocket_infra::FsProxySettingsRepo::new(
                    data_dir.join("proxy.yml"),
                )),
                Arc::new(rocket_infra::KeyringSecretStore::new_proxy()),
                Arc::clone(&shared_proxy),
            );
            let executor: Arc<dyn rocket_http::HttpExecutor> = Arc::new(
                ReqwestExecutor::with_allowed_base(Arc::clone(&active_workspace_path))
                    .with_cookie_repo(Arc::new(FsCookieRepo::new(cookies_dir.clone())))
                    .with_proxy(Arc::clone(&shared_proxy)),
            );

            // RocketVault external secrets stack — shared Arcs used by both
            // secret_manager_svc and exec_svc below.
            let vault_connection_secret_store: Arc<
                dyn rocket_environment::secret_store::SecretStore,
            > = Arc::new(rocket_infra::KeyringSecretStore::new_vault_connections());
            let vault_fetcher: Arc<
                dyn rocket_environment::vault_secret_fetcher::VaultSecretFetcher,
            > = Arc::new(rocket_infra::DispatchingSecretFetcher::with_providers());

            let secret_manager_svc = rocket_app::SecretManagerService::new(
                Box::new(rocket_infra::FsSecretManagerRepo::new(
                    data_dir.join("secret_managers.yml"),
                )),
                Arc::clone(&vault_connection_secret_store),
                Arc::clone(&vault_fetcher),
            );

            // A second SecretManagerService instance, dedicated to
            // AgentConfigService, sharing the same
            // vault_connection_secret_store/vault_fetcher Arcs as
            // secret_manager_svc/exec_svc above — see Plan 04's Global
            // Constraints for why this isn't a shared
            // Arc<SecretManagerService> instead.
            let agent_config_secret_manager = Arc::new(rocket_app::SecretManagerService::new(
                Box::new(rocket_infra::FsSecretManagerRepo::new(
                    data_dir.join("secret_managers.yml"),
                )),
                Arc::clone(&vault_connection_secret_store),
                Arc::clone(&vault_fetcher),
            ));

            let agent_config_svc = rocket_app::AgentConfigService::new(
                Box::new(rocket_infra::FsAgentConfigRepo::new(
                    data_dir.join("agent_configs.yml"),
                )),
                agent_config_secret_manager,
            );

            // A dedicated SecretManagerService + AgentConfigService pair for
            // AcpSessionService, mirroring agent_config_svc above. It must
            // share the same vault_connection_secret_store/vault_fetcher Arcs,
            // so connection secrets and the vault token cache stay shared.
            let acp_agent_config_secret_manager = Arc::new(rocket_app::SecretManagerService::new(
                Box::new(rocket_infra::FsSecretManagerRepo::new(
                    data_dir.join("secret_managers.yml"),
                )),
                Arc::clone(&vault_connection_secret_store),
                Arc::clone(&vault_fetcher),
            ));
            let acp_agent_config_svc = Arc::new(rocket_app::AgentConfigService::new(
                Box::new(rocket_infra::FsAgentConfigRepo::new(
                    data_dir.join("agent_configs.yml"),
                )),
                acp_agent_config_secret_manager,
            ));

            // Reads agent_autonomy_enabled for the session's collection. This
            // must be SharedPathCollectionRepo, like runner_svc/flow_exec_svc:
            // the collection name comes from the active workspace's sidebar,
            // so a path-pinned repo would read the startup workspace's
            // settings after a workspace switch.
            let acp_collection_repo: Arc<dyn rocket_collection::CollectionRepository> = Arc::new(
                SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path)),
            );

            let websocket_svc = rocket_app::WebSocketService::new(
                Arc::new(rocket_infra::TungsteniteWebSocketClient::new()),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );

            let graphql_subscription_svc = rocket_app::GraphQlSubscriptionService::new(
                Arc::new(rocket_infra::TungsteniteWebSocketClient::new()),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );

            let exec_svc = RequestExecutionService::new_with_audit(
                Box::new(FsEnvironmentRepo::with_secret_store(
                    environments_dir.clone(),
                    env_secret_store(),
                )),
                Arc::clone(&executor),
                Box::new(rocket_infra::SharedPathHistoryRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                // Sends read the folder chain, so the repo must follow workspace switches.
                Box::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(FsCookieRepo::new(cookies_dir.clone())),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                audit_publisher.clone(),
                Box::new(rocket_infra::FsSecretManagerRepo::new(
                    data_dir.join("secret_managers.yml"),
                )),
                Arc::clone(&vault_connection_secret_store),
                Arc::clone(&vault_fetcher),
            )
            .with_script_engine(Box::new(DenoScriptEngine::new()))
            .with_trust_store(Arc::clone(&trust_store))
            .with_collection_env_repo_factory(Box::new(
                SharedCollectionEnvironmentRepo::new(Arc::clone(&active_workspace_path)),
            ));

            // OAuth2Service — stand-alone service for token acquisition flows.
            // Uses its own repo instances pointed at the same paths as the exec service.
            // A factory, because the Flow runner needs a second instance for its
            // token fetcher and `OAuth2Service` is not `Clone`.
            let make_oauth2_service = || {
                rocket_app::oauth2_service::OAuth2Service::new(
                    Box::new(FsEnvironmentRepo::with_secret_store(
                        environments_dir.clone(),
                        env_secret_store(),
                    )),
                    Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
                )
                // Client certificates live on a collection's own environment, and a token
                // endpoint that needs mutual TLS gets the matching one.
                .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(
                    Arc::clone(&active_workspace_path),
                )))
                .with_token_client_provider(Arc::new(
                    rocket_infra::ReqwestTokenClientProvider::new()
                        .with_proxy(Arc::clone(&shared_proxy)),
                ))
                // A RocketVault certificate selected for a token URL is fetched at send time. It
                // reads the same connection file and shares the fetcher (and so the token and id
                // caches) above.
                .with_vault_access(
                    Box::new(rocket_infra::FsSecretManagerRepo::new(
                        data_dir.join("secret_managers.yml"),
                    )),
                    Arc::clone(&vault_connection_secret_store),
                    Arc::clone(&vault_fetcher),
                )
            };
            let oauth2_svc = make_oauth2_service();

            // MCP tool server backend (Plan 05, Post-Plan-03 review caveats
            // (a)-(b)): a dedicated Arc<RequestExecutionService> for
            // agent-driven tool calls, rather than switching exec_svc's
            // managed state to Arc (caveat (a)'s other option) — exec_svc is
            // `app.manage`d by value and consumed by several commands'
            // `State<'_, RequestExecutionService>` today. This mirrors
            // exec_svc's own construction, except: (1) SharedPathCollectionRepo
            // instead of a path-pinned FsCollectionRepo for collection_repo,
            // reusing acp_collection_repo, since the agent's target collection
            // is resolved from the sidebar and must follow workspace switches
            // (same reasoning as acp_collection_repo's own doc comment); (2)
            // SharedCollectionEnvironmentRepo::with_secret_store (caveat (b))
            // for the per-collection env factory, never the secret-dropping
            // `::new()`, because a read-modify-write of an environment would
            // otherwise permanently erase any legacy plaintext secret sharing
            // the environment file.
            let mcp_exec_svc = Arc::new(
                RequestExecutionService::new_with_audit(
                    Box::new(FsEnvironmentRepo::with_secret_store(
                        environments_dir.clone(),
                        env_secret_store(),
                    )),
                    Arc::clone(&executor),
                    Box::new(rocket_infra::SharedPathHistoryRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                    Box::new(SharedPathCollectionRepo::new(Arc::clone(
                        &active_workspace_path,
                    ))),
                    Box::new(FsCookieRepo::new(cookies_dir.clone())),
                    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                    audit_publisher.clone(),
                    Box::new(rocket_infra::FsSecretManagerRepo::new(
                        data_dir.join("secret_managers.yml"),
                    )),
                    Arc::clone(&vault_connection_secret_store),
                    Arc::clone(&vault_fetcher),
                )
                .with_script_engine(Box::new(DenoScriptEngine::new()))
                .with_trust_store(Arc::clone(&trust_store))
                .with_collection_env_repo_factory(Box::new(
                    SharedCollectionEnvironmentRepo::with_secret_store(
                        Arc::clone(&active_workspace_path),
                        env_secret_store(),
                    ),
                )),
            );

            // The production Arc<McpToolService> — no earlier plan
            // constructs or `app.manage`s this (Plan 04's "Next Plan" left it
            // to this one); every MCP tool call fails in
            // `mcp_tool_service()`'s `try_state` lookup without it. Its
            // `config_repo`/`active_workspace_path` pair (caveat (d)) lets
            // `run_request` resolve the active workspace's
            // `RequestGuardPolicy` fresh on every call, instead of always
            // running with the fully-permissive default.
            let mcp_tool_svc = Arc::new(rocket_app::McpToolService::new(
                Arc::clone(&acp_collection_repo),
                Arc::new(SharedCollectionEnvironmentRepo::with_secret_store(
                    Arc::clone(&active_workspace_path),
                    env_secret_store(),
                )),
                Arc::clone(&mcp_exec_svc),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Box::new(FsWorkspaceConfigRepo::new()),
                Arc::clone(&active_workspace_path),
                // Follows the live workspace, like the scope check does, so
                // get_history never returns another workspace's runs.
                Box::new(rocket_infra::SharedPathHistoryRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
            )
            .with_trust_store(Arc::clone(&trust_store)));

            // Flow CRUD and Flow execution both need to follow workspace switches, the
            // same reasoning CollectionRunnerService's collection_repo already follows
            // — see SharedPathFlowRepo's doc comment.
            let flow_svc = rocket_app::FlowService::new(Box::new(
                rocket_infra::SharedPathFlowRepo::new(Arc::clone(&active_workspace_path)),
            ));

            // Collection Runner — SharedPathCollectionRepo, not a path-pinned
            // FsCollectionRepo: the run set is the entire content of a run (URLs,
            // scripts, auth), so it must follow workspace switches the same way
            // collection_svc's sidebar reads do, not read whatever workspace was
            // active at process startup.
            let runner_svc = CollectionRunnerService::new(
                Box::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );

            // Flow execution — both repos are workspace-following (SharedPathFlowRepo,
            // SharedPathCollectionRepo), matching runner_svc's own collection_repo
            // exactly and for the same reason: a run must follow workspace switches,
            // not read whatever workspace was active at process startup.
            let flow_exec_svc = rocket_app::FlowExecutionService::new(
                Box::new(rocket_infra::SharedPathFlowRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            )
            .with_callback_listener(Box::new(callback_adapter::HyperCallbackAdapter(
                rocket_infra::HyperCallbackListener::new(),
            )))
            // Auth nodes fetch client-credentials and password tokens through the
            // same OAuth2 stack as the Authentication tab.
            .with_token_fetcher(Box::new(
                rocket_app::flow_auth::OAuth2ServiceFetcher::new(make_oauth2_service()),
            ));

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
                Arc::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                audit_publisher.clone(),
            );

            // gRPC calls. The workspace path is shared with the collection repo, so relative
            // proto paths follow a workspace switch.
            let grpc_svc = rocket_app::GrpcService::new(
                Arc::new(rocket_infra::TonicGrpcExecutor),
                Arc::new(rocket_infra::FsProtoLoader),
                Arc::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Arc::clone(&active_workspace_path),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );

            // Tracks every live per-session MCP HTTP server so it can be
            // swept on app exit and on individual session end. Shared with
            // TauriSessionCleanup below, which ends one session's server.
            let mcp_server_registry = Arc::new(mcp::registry::McpServerRegistry::new());

            // Remove scratch left by a crashed run, before any session can
            // start. Failures are logged inside and never abort startup.
            let swept = agent_session::scratch::sweep_stale_scratch();
            if swept > 0 {
                tracing::info!(swept, "removed stale agent scratch directories");
            }

            // Per-session scratch directories and pre-handshake MCP ids.
            let session_resources =
                Arc::new(agent_session::cleanup::SessionResourceRegistry::new());

            // Built here, after mcp_tool_svc and the registries exist, because
            // its SessionCleanup needs all three.
            let acp_session_svc = rocket_app::AcpSessionService::new(
                Box::new(rocket_infra::AcpAgentClient::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Arc::new(agent_session::cleanup::TauriSessionCleanup::new(
                    Arc::clone(&mcp_server_registry),
                    Arc::clone(&mcp_tool_svc),
                    Arc::clone(&session_resources),
                    Arc::clone(&proposal_svc),
                )),
                acp_agent_config_svc,
                Arc::clone(&acp_collection_repo),
            )
            .with_trust_store(Arc::clone(&trust_store));

            // Register all services as Tauri managed state.
            app.manage(collection_svc);
            app.manage(trust_svc);
            app.manage(contract_svc);
            app.manage(grpc_svc);
            app.manage(history_svc);
            app.manage(template_svc);
            app.manage(cookie_svc);
            app.manage(proxy_svc);
            app.manage(exec_svc);
            app.manage(rocket_app::graphql_schema::GraphQlSchemaCache::default());
            app.manage(secret_manager_svc);
            app.manage(agent_config_svc);
            app.manage(acp_session_svc);
            app.manage(websocket_svc);
            app.manage(graphql_subscription_svc);
            app.manage(runner_svc);
            app.manage(flow_exec_svc);
            app.manage(executor);
            app.manage(oauth2_svc);
            app.manage(flow_svc);
            app.manage(git_svc);
            app.manage(CloneDestinationCapabilities::default());
            app.manage(audit_svc);
            app.manage(Mutex::new(workspace_svc));
            app.manage(active_workspace_path);
            app.manage(Arc::clone(&mcp_server_registry));
            app.manage(Arc::clone(&session_resources));
            app.manage(mcp_tool_svc);
            app.manage(Arc::clone(&proposal_svc));

            // Agent processes run in their own process groups, so a signal
            // sent to Rocket alone never reaches them. Route those signals
            // through the normal exit path, which kills every agent session.
            #[cfg(unix)]
            spawn_exit_signal_listener(app.handle().clone());

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

                // WebKitGTK defaults its cache model to WebBrowser, sized for
                // navigating many sites. Rocket's webview only ever loads its
                // own single-page app, so the browser-sized page/object caches
                // are pure overhead.
                #[cfg(target_os = "linux")]
                win.with_webview(|webview| {
                    use webkit2gtk::{CacheModel, SettingsExt, WebContextExt, WebViewExt};
                    if let Some(ctx) = webview.inner().context() {
                        ctx.set_cache_model(CacheModel::DocumentViewer);
                    }
                    // Rocket has no camera or microphone features, draws its
                    // charts as SVG, and never navigates between documents.
                    // Media capture, WebGL and the back/forward page cache
                    // are therefore unused overhead.
                    if let Some(settings) = webview.inner().settings() {
                        settings.set_enable_media_stream(false);
                        settings.set_enable_webgl(false);
                        settings.set_enable_page_cache(false);
                    }
                })
                .ok();
            }

            tracing::info!(data_dir = %data_dir.display(), "RocketAPI initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::collections::list_collections,
            commands::collections::get_collection,
            commands::collections::get_collection_summaries,
            commands::collections::get_request,
            commands::collections::get_grpc_request,
            commands::collections::get_graphql_request,
            commands::collections::create_collection,
            commands::collections::delete_collection,
            commands::collections::rename_collection,
            commands::collections::save_request,
            commands::collections::save_grpc_request,
            commands::collections::get_websocket_request,
            commands::collections::save_websocket_request,
            commands::collections::save_graphql_request,
            commands::collections::rename_request,
            commands::collections::delete_request,
            commands::collections::create_folder,
            commands::collections::delete_folder,
            commands::collections::create_script_file,
            commands::collections::read_script_file,
            commands::collections::save_script_file,
            commands::collections::rename_script_file,
            commands::collections::delete_script_file,
            commands::collections::move_item,
            commands::collections::reorder_items,
            commands::collections::get_collection_settings,
            commands::collections::save_collection_settings,
            commands::collection_trust::set_collection_capability,
            commands::collection_trust::get_trust_migration_notice,
            commands::collection_trust::dismiss_trust_migration_notice,
            commands::collections::scan_collections_in_path,
            commands::collections::detect_cloned_structure,
            commands::collections::get_folder_chain_variables,
            commands::collections::get_folder_variables,
            commands::collections::save_folder_variables,
            commands::collections::get_folder_settings,
            commands::collections::save_folder_settings,
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
            commands::execution::execute_graphql_request,
            commands::execution::list_graphql_operations,
            commands::graphql_schema::fetch_graphql_schema,
            commands::graphql_schema::get_cached_graphql_schema,
            commands::graphql_schema::clear_graphql_schema,
            commands::execution::evaluate_var_expression,
            commands::runner::run_collection,
            commands::runner::stop_collection_run,
            commands::flow::list_flows,
            commands::flow::get_flow,
            commands::flow::delete_flow,
            commands::flow::rename_flow,
            commands::flow::save_flow,
            commands::flow::lint_flow,
            commands::flow::run_flow,
            commands::flow::cancel_flow_run,
            commands::load_test::run_load_test_command,
            commands::load_test::run_load_test_v2_command,
            commands::load_test::export_load_test,
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
            commands::proxy::get_proxy_settings,
            commands::proxy::save_proxy_settings,
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
            commands::git::select_clone_destination,
            commands::git::git_clone,
            commands::git::get_default_ssh_key_path,
            commands::git::list_ssh_key_paths,
            commands::git::git_is_repo_v2,
            commands::git::git_init_v2,
            commands::git::git_status_v2,
            commands::git::git_diff_v2,
            commands::git::git_diff_staged_v2,
            commands::git::git_diff_commit_v2,
            commands::git::git_stage_v2,
            commands::git::git_unstage_v2,
            commands::git::git_discard_v2,
            commands::git::git_commit_v2,
            commands::git::git_log_v2,
            commands::git::git_push_v2,
            commands::git::git_pull_v2,
            commands::grpc::grpc_unary_call,
            commands::grpc::grpc_start_session,
            commands::grpc::grpc_send_message,
            commands::grpc::grpc_end_requests,
            commands::grpc::grpc_cancel_session,
            commands::grpc::grpc_list_services,
            commands::git::git_fetch_v2,
            commands::git::git_branches_v2,
            commands::git::git_switch_branch_v2,
            commands::git::git_checkout_remote_branch_v2,
            commands::git::git_create_branch_v2,
            commands::git::git_delete_branch_v2,
            commands::git::git_merge_branch_v2,
            commands::git::git_stash_list_v2,
            commands::git::git_stash_save_v2,
            commands::git::git_stash_pop_v2,
            commands::git::git_stash_apply_v2,
            commands::git::git_stash_drop_v2,
            commands::git::git_stash_diff_v2,
            commands::git::git_conflicts_v2,
            commands::git::git_resolve_conflict_v2,
            commands::git::git_abort_merge_v2,
            commands::git::git_list_remotes_v2,
            commands::git::git_add_remote_v2,
            commands::git::git_remove_remote_v2,
            commands::git::git_set_remote_url_v2,
            commands::git::save_git_credentials_v2,
            commands::git::load_git_credentials_v2,
            commands::git::git_get_identity_v2,
            commands::git::git_set_identity_v2,
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
            commands::workspaces::update_request_guard_policy,
            commands::workspaces::open_folder_picker,
            commands::workspaces::link_external_collection,
            commands::ui_state::load_ui_state,
            commands::ui_state::save_ui_state,
            commands::import::import_bruno,
            commands::import::import_bruno_zip,
            commands::import::import_postman_collection,
            commands::import::import_postman_environment,
            commands::import::import_wsdl,
            commands::contract::attach_contract,
            commands::contract::update_contract,
            commands::contract::list_contracts,
            commands::contract::get_contract,
            commands::contract::delete_contract,
            commands::contract::get_contract_changelog,
            commands::contract::publish_contract,
            commands::contract::accept_drift,
            commands::contract::pause_contract,
            commands::contract::resume_contract,
            commands::contract::renew_contract,
            commands::contract::send_for_review,
            commands::contract::approve_contract,
            commands::contract::reject_contract,
            commands::contract::duplicate_contract,
            commands::contract::recompute_drift,
            commands::contract::get_contract_summary,
            commands::contract::export_contract_openapi,
            commands::contract::archive_contract,
            commands::contract::unarchive_contract,
            commands::secret_managers::list_secret_manager_connections,
            commands::secret_managers::save_secret_manager_connection,
            commands::secret_managers::delete_secret_manager_connection,
            commands::secret_managers::test_secret_manager_connection,
            commands::secret_managers::fetch_external_secret_names,
            commands::secret_managers::list_vault_certificates,
            commands::agent_configs::list_agent_configs,
            commands::agent_configs::save_agent_config,
            commands::agent_configs::delete_agent_config,
            commands::agent_configs::test_agent_config,
            commands::assistant_chips::build_assistant_chip_resource,
            commands::assistant_chips::mask_assistant_response,
            commands::acp_sessions::send_agent_prompt,
            commands::acp_sessions::cancel_agent_prompt,
            commands::acp_sessions::set_agent_config_option,
            commands::acp_sessions::end_agent_session,
            commands::agent_proposals::list_agent_proposals,
            commands::agent_proposals::accept_agent_proposal,
            commands::agent_proposals::reject_agent_proposal,
            commands::acp_sessions::set_assistant_mode,
            commands::acp_sessions::start_workspace_assistant,
            commands::acp_sessions::end_stale_assistant_sessions,
            commands::websocket::ws_connect,
            commands::websocket::ws_send,
            commands::websocket::ws_disconnect,
            commands::graphql_subscription::graphql_subscribe,
            commands::graphql_subscription::graphql_unsubscribe,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(acp_session_svc) =
                    app_handle.try_state::<rocket_app::AcpSessionService>()
                {
                    // Best-effort on app exit. The process is about to tear
                    // down regardless, so there is no caller left to report
                    // a kill failure to.
                    let _ = tauri::async_runtime::block_on(acp_session_svc.end_all_sessions());
                }
                if let Some(websocket_svc) = app_handle.try_state::<rocket_app::WebSocketService>() {
                    tauri::async_runtime::block_on(websocket_svc.end_all_sessions());
                }
                if let Some(svc) = app_handle.try_state::<rocket_app::GraphQlSubscriptionService>() {
                    tauri::async_runtime::block_on(svc.end_all());
                }
                if let Some(grpc_svc) = app_handle.try_state::<rocket_app::GrpcService>() {
                    // Best-effort, like the ACP cleanup: the process is about to end.
                    grpc_svc.end_all();
                }
                if let Some(mcp_registry) =
                    app_handle.try_state::<Arc<mcp::registry::McpServerRegistry>>()
                {
                    mcp_registry.shutdown_all();
                }
                if let Some(resources) =
                    app_handle.try_state::<Arc<agent_session::cleanup::SessionResourceRegistry>>()
                {
                    // Backstop for a session that started while end_all_sessions ran.
                    resources.clear_all();
                }
            }
        });
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::network_memory_pressure_settings;

    // WebKit's setters silently ignore out-of-range or misordered values, so
    // read every field back to prove each one was accepted.
    #[test]
    fn network_memory_pressure_settings_are_accepted_by_webkit() {
        let mut settings = network_memory_pressure_settings();
        assert_eq!(settings.memory_limit(), 128);
        assert_eq!(settings.conservative_threshold(), 0.25);
        assert_eq!(settings.strict_threshold(), 0.4);
        assert_eq!(settings.kill_threshold(), 0.0);
        assert_eq!(settings.poll_interval(), 30.0);
    }
}
