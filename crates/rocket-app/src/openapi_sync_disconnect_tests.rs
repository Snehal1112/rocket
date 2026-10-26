//! Tests for disconnecting a collection from its OpenAPI source.

use super::*;
use async_trait::async_trait;
use rocket_collection::file_tx::{CollectionFileTxFactory, OWNED_STATE_FILES};
use rocket_collection::openapi_baseline::{OpenApiBaseline, OpenApiBaselineRepository};
use rocket_collection::{CollectionRepository, InMemoryConsentStore, Request};
use rocket_infra::{
    FsCollectionRepo, FsFileTxFactory, FsOpenApiBaselineRepo, FsOpenApiLocalRepo,
    FsOpenApiSourceRepo,
};
use rocket_shared::types::HttpMethod;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

struct NoFetch;

#[async_trait]
impl SpecFetcher for NoFetch {
    async fn fetch(&self, _: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        Ok(SpecFetchOutcome::NotModified)
    }
}

#[derive(Default)]
struct Secrets(Mutex<HashMap<(String, String), String>>);

impl SecretStore for Secrets {
    fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .get(&(scope.to_string(), key.to_string()))
            .cloned())
    }
    fn get_strict(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
        self.get(scope, key)
    }
    fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
        self.0
            .lock()
            .expect("lock")
            .insert((scope.to_string(), key.to_string()), value.to_string());
        Ok(())
    }
    fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
        self.0
            .lock()
            .expect("lock")
            .remove(&(scope.to_string(), key.to_string()));
        Ok(())
    }
}

struct World {
    ws: TempDir,
    root: PathBuf,
    secrets: Arc<Secrets>,
    store: Arc<InMemoryConsentStore>,
    tx: Arc<FsFileTxFactory>,
    svc: OpenApiSyncService,
}

fn location() -> SpecLocation {
    SpecLocation::Github {
        owner: "o".into(),
        repo: "r".into(),
        git_ref: "main".into(),
        path: "api.json".into(),
    }
}

impl World {
    fn new() -> Self {
        let ws = TempDir::new().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        repo.create("c").expect("create");
        repo.save_request(
            "c",
            "Login.yml",
            &Request::new("Login", HttpMethod::Get, "http://one".to_string()),
        )
        .expect("request");
        let root = repo.collection_root_path("c").expect("root");
        let source = OpenApiSource {
            version: 1,
            location: location(),
            group_by: "tags".into(),
            spec_sha256: "abc".into(),
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
        FsOpenApiSourceRepo.save(&root, &source).expect("source");
        FsOpenApiSourceRepo
            .save_pending(&root, b"pending")
            .expect("pending");
        FsOpenApiBaselineRepo
            .save(
                &root,
                &OpenApiBaseline {
                    version: 1,
                    converter_version: 1,
                    base_url: String::new(),
                    operations: Vec::new(),
                },
            )
            .expect("baseline");
        let local = OpenApiLocalState {
            sync_interval: SyncInterval::Min5,
            ..OpenApiLocalState::default()
        };
        FsOpenApiLocalRepo.save(&root, &local).expect("local");
        // A spec copy from the sync design and a file this feature does not own.
        std::fs::write(root.join(".rocket/openapi-applied.spec"), b"applied").expect("applied");
        std::fs::write(root.join(".rocket/other.json"), b"{}").expect("other");
        let secrets = Arc::new(Secrets::default());
        secrets.set("src-1", "token", "ghp_secret").expect("token");
        let store = Arc::new(InMemoryConsentStore::new());
        store
            .grant(
                &root,
                "src-1",
                &rocket_collection::location_key(&location()),
            )
            .expect("consent");
        let tx = Arc::new(FsFileTxFactory::new());
        let deps = ApplyDeps {
            collections: Box::new(FsCollectionRepo::new_standalone(
                ws.path().join("collections"),
            )),
            baselines: Box::new(FsOpenApiBaselineRepo),
            locals: Box::new(FsOpenApiLocalRepo),
            tx: Arc::clone(&tx) as Arc<dyn CollectionFileTxFactory>,
            renderer: Arc::new(|root: &Path| {
                Box::new(rocket_infra::FsRequestRenderer::for_collection(
                    root.to_path_buf(),
                ))
            }),
        };
        let svc = OpenApiSyncService::new(
            Arc::new(NoFetch),
            Box::new(FsOpenApiSourceRepo),
            Arc::clone(&secrets) as Arc<dyn SecretStore>,
        )
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_consent(Arc::clone(&store) as Arc<dyn OpenApiConsentStore>)
        .with_apply(deps);
        Self {
            ws,
            root,
            secrets,
            store,
            tx,
            svc,
        }
    }

    fn has_token(&self) -> bool {
        self.secrets.get("src-1", "token").expect("read").is_some()
    }

    fn has_consent(&self) -> bool {
        self.store.get(&self.root, "src-1").expect("get").is_some()
    }

    fn rocket(&self, name: &str) -> bool {
        self.root.join(".rocket").join(name).exists()
    }
}

#[test]
fn disconnect_removes_only_sync_state() {
    let w = World::new();
    w.svc.disconnect(&w.root, false).expect("disconnect");
    for name in OWNED_STATE_FILES {
        assert!(!w.rocket(name), "{name} should be gone");
    }
    assert!(!w.has_consent());
    // The token stays unless the caller asked for it to go.
    assert!(w.has_token());
    // Everything else is untouched.
    assert!(w.rocket("other.json"));
    assert!(w.root.join("Login.yml").exists());
    assert!(w.root.join("opencollection.yml").exists());
    let repo = FsCollectionRepo::new_standalone(w.ws.path().join("collections"));
    assert!(repo.get("c").is_ok());
}

#[test]
fn disconnect_with_remove_token_deletes_the_token() {
    let w = World::new();
    w.svc.disconnect(&w.root, true).expect("disconnect");
    assert!(!w.has_token());
    assert!(!w.has_consent());
}

#[test]
fn a_second_disconnect_is_a_no_op() {
    let w = World::new();
    w.svc.disconnect(&w.root, true).expect("first");
    w.svc.disconnect(&w.root, true).expect("second");
    assert!(w.rocket("other.json"));
}

#[test]
fn disconnect_is_refused_while_an_apply_holds_the_root() {
    let w = World::new();
    let open = w.tx.begin(&w.root).expect("begin");
    let err = w.svc.disconnect(&w.root, true).expect_err("refused");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    // Nothing was removed.
    assert!(w.rocket("openapi-source.yml"));
    assert!(w.rocket("openapi-local.json"));
    assert!(w.has_token());
    assert!(w.has_consent());
    open.abort();
    w.svc.disconnect(&w.root, true).expect("after the apply");
    assert!(!w.rocket("openapi-source.yml"));
}

#[test]
fn status_and_due_targets_cope_with_vanished_sync_files() {
    let w = World::new();
    w.svc.disconnect(&w.root, false).expect("disconnect");
    assert!(matches!(
        w.svc.status(&w.root, true),
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        w.svc.status_of("c", false),
        Err(DomainError::NotFound(_))
    ));
    let now = Utc::now();
    assert!(w.svc.due_targets(now).is_empty());
    // A local state that outlives the source file is skipped too.
    FsOpenApiLocalRepo
        .save(
            &w.root,
            &OpenApiLocalState {
                sync_interval: SyncInterval::Min5,
                ..OpenApiLocalState::default()
            },
        )
        .expect("local");
    assert!(w.svc.due_targets(now).is_empty());
}

#[test]
fn status_reports_an_interval_it_does_not_understand() {
    let w = World::new();
    std::fs::write(
        w.root.join(".rocket/openapi-local.json"),
        r#"{"version":1,"sync_interval":"every_2_hours"}"#,
    )
    .expect("write");
    let view = w.svc.status(&w.root, false).expect("status");
    assert_eq!(view.interval, SyncInterval::Off);
    assert!(view.interval_unknown);
    assert!(w.svc.due_targets(Utc::now()).is_empty());
    let ok = World::new();
    assert!(
        !ok.svc
            .status(&ok.root, false)
            .expect("status")
            .interval_unknown
    );
}

#[test]
fn set_interval_takes_the_short_values_and_still_needs_consent() {
    let w = World::new();
    w.svc
        .set_interval(&w.root, SyncInterval::Min15)
        .expect("set");
    assert_eq!(
        w.svc.status(&w.root, false).expect("status").interval,
        SyncInterval::Min15
    );
    w.store.revoke(&w.root, "src-1").expect("revoke");
    assert!(w.svc.set_interval(&w.root, SyncInterval::Min5).is_err());
    w.svc.set_interval(&w.root, SyncInterval::Off).expect("off");
}
