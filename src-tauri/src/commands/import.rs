use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::OpenApiSyncService;
use rocket_environment::{EnvironmentRepository, SecretStore};
use rocket_http::SpecFetcher;
use rocket_import::{
    EnvironmentRepositoryFactory, GroupBy, ImportReport, ImportService, OpenApiImportResult,
};
use rocket_infra::{
    FsCollectionRepo, FsEnvironmentRepo, FsFileTxFactory, FsOpenApiBaselineRepo,
    FsOpenApiConsentRepo, FsOpenApiLocalRepo, FsOpenApiSourceRepo, FsRequestRenderer,
    KeyringSecretStore, SharedPathCollectionRepo,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

struct FsEnvFactory(PathBuf);
impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::with_secret_store(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
            crate::env_secret_store(),
        ))
    }
}

fn make_import_service(base: PathBuf) -> ImportService {
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
    )
}

/// Import a Bruno collection or workspace directory. Type is auto-detected from content.
#[tauri::command]
pub async fn import_bruno(
    path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_auto(
            &PathBuf::from(&path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// Import a Postman Collection JSON file (v2.0 or v2.1).
#[tauri::command]
pub async fn import_postman_collection(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_postman_collection(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Import a Postman environment JSON file into an existing collection.
#[tauri::command]
pub async fn import_postman_environment(
    json_path: String,
    collection_name: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_postman_environment(
            &PathBuf::from(&json_path),
            &collection_name,
            &target_workspace_id,
        )
        .map_err(|e| e.to_string())
}

/// Import a WSDL 1.1 file as a collection of SOAP requests.
#[tauri::command]
pub async fn import_wsdl(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_wsdl(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Extract a Bruno ZIP and import the contained collection or workspace.
#[tauri::command]
pub async fn import_bruno_zip(
    zip_path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_auto_from_zip(
            &PathBuf::from(&zip_path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// What an OpenAPI import returns to the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImportDto {
    pub report: ImportReport,
    pub collection: String,
    pub source_url: Option<String>,
    pub source_id: Option<String>,
    pub warnings: Vec<String>,
}

impl From<OpenApiImportResult> for OpenApiImportDto {
    fn from(r: OpenApiImportResult) -> Self {
        Self {
            report: r.report,
            collection: r.collection,
            source_url: r.source_url,
            source_id: r.source_id,
            warnings: r.warnings,
        }
    }
}

fn parse_group_by(text: &str) -> Result<GroupBy, String> {
    GroupBy::parse(text).ok_or_else(|| "folder layout must be tags or paths".to_string())
}

/// The machine-local approvals for OpenAPI sync, `~/.rocket-api/openapi-sync.yml`. One store
/// is shared by the whole process, so its write lock covers every service built per call.
pub(crate) fn openapi_consent_store() -> Arc<dyn rocket_collection::OpenApiConsentStore> {
    static STORE: std::sync::OnceLock<Arc<dyn rocket_collection::OpenApiConsentStore>> =
        std::sync::OnceLock::new();
    Arc::clone(STORE.get_or_init(|| {
        match rocket_shared::paths::home_dir() {
            Some(home) => Arc::new(FsOpenApiConsentRepo::new(
                home.join(".rocket-api").join("openapi-sync.yml"),
            )),
            // Startup needs a home directory, so this is not expected. Approvals then last
            // for this run only.
            None => Arc::new(rocket_collection::InMemoryConsentStore::new()),
        }
    }))
}

/// Trims a token from the UI. A blank value means no token.
fn token_arg(token: Option<String>) -> Option<String> {
    token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Shown when the user asked for update checks and they could not be turned on.
const CHECKS_NOT_ENABLED: &str =
    "Update checks could not be turned on. You can turn them on in the Spec sync settings.";
const CONSENT_NOT_SAVED: &str =
    "The link could not be approved on this machine, so it will not be checked until you approve it.";

const TOKEN_NOT_SAVED: &str =
    "The token could not be saved, so checks on this private repository will fail.";

/// Stores the token in the keychain. The error text never holds the token.
fn store_token(
    fetcher: &Arc<dyn SpecFetcher>,
    secrets: Arc<dyn SecretStore>,
    collection_root: &std::path::Path,
    token: &str,
) -> bool {
    OpenApiSyncService::new(Arc::clone(fetcher), Box::new(FsOpenApiSourceRepo), secrets)
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_consent(openapi_consent_store())
        .set_token(collection_root, token)
        .is_ok()
}

/// Builds the sync service with everything applying needs, for the active workspace.
pub(crate) fn make_sync_service(
    active_workspace_path: Arc<Mutex<PathBuf>>,
    fetcher: Arc<dyn SpecFetcher>,
) -> OpenApiSyncService {
    let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecretStore::new_openapi_sync());
    OpenApiSyncService::new(fetcher, Box::new(FsOpenApiSourceRepo), secrets)
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_consent(openapi_consent_store())
        .with_apply(rocket_app::ApplyDeps {
            collections: Box::new(SharedPathCollectionRepo::new(active_workspace_path)),
            baselines: Box::new(FsOpenApiBaselineRepo),
            locals: Box::new(FsOpenApiLocalRepo),
            tx: Arc::new(FsFileTxFactory::new()),
            renderer: Arc::new(|root: &std::path::Path| {
                Box::new(FsRequestRenderer::for_collection(root.to_path_buf()))
            }),
        })
}

/// Finishes or undoes interrupted syncs of the active workspace on a background task. The
/// log lines carry counts only.
pub(crate) fn recover_openapi_syncs(
    active_workspace_path: Arc<Mutex<PathBuf>>,
    fetcher: Arc<dyn SpecFetcher>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let service = make_sync_service(active_workspace_path, fetcher);
        for (_, outcome) in service.recover_all() {
            match outcome {
                rocket_collection::file_tx::RecoveryOutcome::RolledBack { files } => {
                    tracing::warn!(files, "an interrupted sync apply was undone");
                }
                other => tracing::info!(?other, "an interrupted sync apply was settled"),
            }
        }
    });
}

/// A running background loop that can be told to stop.
pub(crate) trait Stoppable {
    fn stop(&self);
}

impl Stoppable for rocket_app::SchedulerHandle {
    fn stop(&self) {
        rocket_app::SchedulerHandle::stop(self);
    }
}

/// Holds the handle of the running sync supervisor, so a workspace switch or the app exit can
/// stop it. Generic over `Stoppable` so tests can use a counting stand-in.
pub(crate) struct SyncSchedulerSlot<H: Stoppable = rocket_app::SchedulerHandle>(Mutex<Option<H>>);

impl<H: Stoppable> Default for SyncSchedulerSlot<H> {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

impl<H: Stoppable> SyncSchedulerSlot<H> {
    /// Stops the previous handle first, then keeps the new one.
    pub(crate) fn replace(&self, handle: H) {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = guard.take() {
            old.stop();
        }
        *guard = Some(handle);
    }

    /// Stops and drops the kept handle. Calling it again does nothing.
    pub(crate) fn stop(&self) {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = guard.take() {
            old.stop();
        }
    }
}

/// Starts the sync supervisor for the active workspace and keeps its handle in the slot, which
/// stops the previous one first. It checks only collections whose local interval is on. Recovery
/// is not awaited here: the first tick waits 30 s and `check` recovers before it fetches.
pub(crate) fn start_sync_scheduler(
    app: &AppHandle,
    active_workspace_path: Arc<Mutex<PathBuf>>,
    fetcher: Arc<dyn SpecFetcher>,
) {
    let Some(slot) = app.try_state::<SyncSchedulerSlot>() else {
        tracing::error!(
            code = "sync_scheduler_slot_missing",
            "sync supervisor not started"
        );
        return;
    };
    // Stop the old loop before building the new one.
    slot.stop();
    let factory: rocket_app::ServiceFactory = Arc::new(move || {
        make_sync_service(Arc::clone(&active_workspace_path), Arc::clone(&fetcher))
    });
    let events = Arc::new(crate::tauri_event_bus::TauriEventBus::new(app.clone()));
    let scheduler = rocket_app::OpenApiSyncScheduler::new(
        factory,
        events,
        rocket_app::SchedulerConfig::default(),
    );
    // The scheduler spawns on the current tokio runtime, so enter the app runtime first.
    let _guard = tauri::async_runtime::handle().inner().enter();
    slot.replace(scheduler.start());
}

/// Import an OpenAPI or Swagger spec file as a new collection.
#[tauri::command]
pub async fn import_openapi_file(
    path: String,
    group_by: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<OpenApiImportDto, String> {
    let group_by = parse_group_by(&group_by)?;
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_openapi_file(&PathBuf::from(&path), group_by, &target_workspace_id)
        .map(OpenApiImportDto::from)
        .map_err(|e| e.to_string())
}

/// Turns on checks every 5 minutes when the user asked for them. It runs only after the
/// source is saved and the link is approved, and a failure leaves checks off. It returns
/// false when checks were asked for and could not be turned on.
fn enable_checks(
    service: &OpenApiSyncService,
    root: &std::path::Path,
    check_for_updates: Option<bool>,
) -> bool {
    if check_for_updates != Some(true) {
        return true;
    }
    service
        .set_interval(root, rocket_collection::openapi_source::SyncInterval::Min5)
        .is_ok()
}

/// Import an OpenAPI or Swagger spec from a link as a new collection.
#[tauri::command]
pub async fn import_openapi_url(
    link: String,
    group_by: String,
    token: Option<String>,
    target_workspace_id: String,
    check_for_updates: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<OpenApiImportDto, String> {
    let group_by = parse_group_by(&group_by)?;
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    let token = token_arg(token);
    let mut dto: OpenApiImportDto = make_import_service(base.clone())
        .import_openapi_url(
            fetcher.inner().as_ref(),
            &FsOpenApiSourceRepo,
            &FsOpenApiBaselineRepo,
            &link,
            group_by,
            token.as_deref(),
            &target_workspace_id,
        )
        .await
        .map(OpenApiImportDto::from)
        .map_err(|e| e.to_string())?;
    // The user typed this link on this machine, so it is approved for the new collection.
    let root = base.join("collections").join(&dto.collection);
    let approver = OpenApiSyncService::new(
        Arc::clone(fetcher.inner()),
        Box::new(FsOpenApiSourceRepo),
        Arc::new(KeyringSecretStore::new_openapi_sync()),
    )
    .with_local(Box::new(FsOpenApiLocalRepo))
    .with_consent(openapi_consent_store());
    let approval = approver.grant_consent_for_link(&root, &link);
    if approval.is_err() {
        dto.warnings.push(CONSENT_NOT_SAVED.to_string());
        if check_for_updates == Some(true) {
            dto.warnings.push(CHECKS_NOT_ENABLED.to_string());
        }
    } else if !enable_checks(&approver, &root, check_for_updates) {
        dto.warnings.push(CHECKS_NOT_ENABLED.to_string());
    }
    if let Some(token) = token.as_deref() {
        let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecretStore::new_openapi_sync());
        if !store_token(fetcher.inner(), secrets, &root, token) {
            dto.warnings.push(TOKEN_NOT_SAVED.to_string());
        }
    }
    Ok(dto)
}

#[cfg(test)]
mod sync_scheduler_slot {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting(Arc<AtomicUsize>);
    impl Stoppable for Counting {
        fn stop(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn replace_stops_the_previous_handle() {
        let slot = SyncSchedulerSlot::<Counting>::default();
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        slot.replace(Counting(Arc::clone(&first)));
        slot.replace(Counting(Arc::clone(&second)));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn stop_is_idempotent() {
        let slot = SyncSchedulerSlot::<Counting>::default();
        let count = Arc::new(AtomicUsize::new(0));
        slot.replace(Counting(Arc::clone(&count)));
        slot.stop();
        slot.stop();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_arg_trims_and_drops_blank_values() {
        assert_eq!(token_arg(None), None);
        assert_eq!(token_arg(Some("  ".to_string())), None);
        assert_eq!(
            token_arg(Some(" abc ".to_string())),
            Some("abc".to_string())
        );
    }

    mod check_for_updates {
        use super::super::*;
        use async_trait::async_trait;
        use rocket_collection::openapi_local::OpenApiLocalRepository;
        use rocket_collection::openapi_source::{
            OpenApiSource, OpenApiSourceRepository, SpecLocation, SyncInterval,
        };
        use rocket_collection::InMemoryConsentStore;
        use rocket_http::spec_fetch::{SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest};

        struct NoFetch;

        #[async_trait]
        impl SpecFetcher for NoFetch {
            async fn fetch(
                &self,
                _: SpecFetchRequest,
            ) -> Result<SpecFetchOutcome, SpecFetchFailure> {
                Ok(SpecFetchOutcome::NotModified)
            }
        }

        const LINK: &str = "https://example.com/spec.json";

        fn service() -> OpenApiSyncService {
            OpenApiSyncService::new(
                Arc::new(NoFetch),
                Box::new(FsOpenApiSourceRepo),
                Arc::new(KeyringSecretStore::new_openapi_sync()),
            )
            .with_local(Box::new(FsOpenApiLocalRepo))
            .with_consent(Arc::new(InMemoryConsentStore::new()))
        }

        fn seed(root: &std::path::Path) {
            let source = OpenApiSource {
                version: 1,
                location: SpecLocation::Https { url: LINK.into() },
                group_by: "tags".into(),
                spec_sha256: String::new(),
                etag: None,
                last_modified: None,
                operations: Vec::new(),
                source_id: "src-1".into(),
                sync_interval: SyncInterval::Off,
                last_checked_at: None,
                last_status: None,
                pending_sha256: None,
                remote_refs: Vec::new(),
                remote_refs_incomplete: false,
            };
            FsOpenApiSourceRepo.save(root, &source).expect("source");
        }

        fn interval(root: &std::path::Path) -> SyncInterval {
            FsOpenApiLocalRepo
                .load(root)
                .expect("load")
                .map(|l| l.sync_interval)
                .unwrap_or_default()
        }

        #[test]
        fn true_turns_on_five_minutes_after_the_source_is_saved() {
            let dir = tempfile::tempdir().expect("dir");
            seed(dir.path());
            let svc = service();
            assert!(svc.grant_consent_for_link(dir.path(), LINK).is_ok());
            assert!(enable_checks(&svc, dir.path(), Some(true)));
            assert_eq!(interval(dir.path()), SyncInterval::Min5);
        }

        #[test]
        fn false_and_none_leave_checks_off() {
            let dir = tempfile::tempdir().expect("dir");
            seed(dir.path());
            let svc = service();
            assert!(svc.grant_consent_for_link(dir.path(), LINK).is_ok());
            assert!(enable_checks(&svc, dir.path(), Some(false)));
            assert!(enable_checks(&svc, dir.path(), None));
            assert_eq!(interval(dir.path()), SyncInterval::Off);
        }

        #[test]
        fn a_missing_source_or_consent_leaves_checks_off() {
            let dir = tempfile::tempdir().expect("dir");
            let svc = service();
            assert!(!enable_checks(&svc, dir.path(), Some(true)));
            assert_eq!(interval(dir.path()), SyncInterval::Off);
            // A saved source without consent is refused too.
            seed(dir.path());
            assert!(!enable_checks(&svc, dir.path(), Some(true)));
            assert_eq!(interval(dir.path()), SyncInterval::Off);
        }
    }

    #[test]
    fn parse_group_by_accepts_tags_and_paths_only() {
        assert!(parse_group_by("tags").is_ok());
        assert!(parse_group_by("paths").is_ok());
        assert_eq!(
            parse_group_by("folders").err().as_deref(),
            Some("folder layout must be tags or paths")
        );
    }
}
