//! Tests for applying a spec through the file transaction, on a real temporary workspace.

use super::*;
use async_trait::async_trait;
use rocket_collection::openapi_source::OpenApiSourceRepository;
use rocket_environment::SecretStore;
use rocket_http::spec_fetch::{SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher};
use rocket_import::{
    fingerprint, item_digest, plan_with_links, ApprovedItem, Choice, GroupBy, ImportService,
    SkipRequest, SyncItem, SyncPlan,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tempfile::TempDir;

const LINK: &str = "https://example.com/specs/pets.json";
const COLLECTION: &str = "Pets API";

fn op(op_id: &str, tag: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut o = json!({"operationId": op_id, "tags": [tag], "summary": op_id,
        "responses": {"200": {"description": "ok"}}});
    if let Some(map) = extra.as_object() {
        for (k, v) in map {
            o[k] = v.clone();
        }
    }
    o
}

fn spec(paths: serde_json::Value) -> String {
    json!({"openapi": "3.0.3", "info": {"title": COLLECTION, "version": "1"},
        "servers": [{"url": "https://api.example.com"}], "paths": paths})
    .to_string()
}

fn v1() -> String {
    spec(json!({
        "/pets": {"get": op("listPets", "pets", json!({}))},
        "/pets/{id}": {"get": op("getPet", "pets", json!({"parameters": [
            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}]}))},
        "/inventory": {"get": op("getInventory", "store", json!({}))},
    }))
}

fn v2() -> String {
    spec(json!({
        "/pets": {
            "get": op("listPets", "pets", json!({"parameters": [
                {"name": "limit", "in": "query", "schema": {"type": "integer"}}]})),
            "post": op("createPet", "pets", json!({})),
        },
        "/pets/{id}": {"get": op("getPet", "pets", json!({"parameters": [
            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}]}))},
    }))
}

fn v3() -> String {
    spec(json!({
        "/pets": {
            "get": op("listPets", "pets", json!({"parameters": [
                {"name": "offset", "in": "query", "schema": {"type": "integer"}}]})),
            "post": op("createPet", "pets", json!({})),
        },
        "/pets/{id}": {"get": op("getPet", "pets", json!({"parameters": [
            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}]}))},
    }))
}

struct EnvFactory(PathBuf);

impl rocket_import::EnvironmentRepositoryFactory for EnvFactory {
    fn make(&self, name: &str) -> Box<dyn rocket_environment::EnvironmentRepository> {
        Box::new(rocket_infra::FsEnvironmentRepo::new(
            self.0.join("collections").join(name).join("environments"),
        ))
    }
}

fn import_service(ws: &Path) -> ImportService {
    ImportService::new(
        ws.to_path_buf(),
        Box::new(rocket_infra::FsCollectionRepo::new_standalone(
            ws.join("collections"),
        )),
        Box::new(EnvFactory(ws.to_path_buf())),
    )
}

struct FakeFetcher {
    body: Mutex<String>,
}

struct SharedFetcher(Arc<FakeFetcher>);

#[async_trait]
impl SpecFetcher for SharedFetcher {
    async fn fetch(&self, _: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        Ok(SpecFetchOutcome::Fetched {
            body: self.0.body.lock().expect("lock").clone().into_bytes(),
            etag: None,
            last_modified: None,
            final_url: LINK.to_string(),
        })
    }
}

struct NoSecrets;

impl SecretStore for NoSecrets {
    fn get(&self, _: &str, _: &str) -> DomainResult<Option<String>> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Fails the n-th staging call, or blocks recovery, to test the service around the factory.
struct Wrapper {
    inner: rocket_infra::FsFileTxFactory,
    fail_stage_at: Option<usize>,
    recover_blocked: Arc<AtomicBool>,
}

struct WrappedTx {
    inner: Box<dyn CollectionFileTx>,
    count: usize,
    fail_at: Option<usize>,
}

impl WrappedTx {
    fn tick(&mut self) -> DomainResult<()> {
        self.count += 1;
        if self.fail_at == Some(self.count) {
            return Err(DomainError::Internal("injected staging failure".into()));
        }
        Ok(())
    }
}

impl CollectionFileTx for WrappedTx {
    fn stage_put(&mut self, rel: &str, bytes: &[u8]) -> DomainResult<()> {
        self.tick()?;
        self.inner.stage_put(rel, bytes)
    }
    fn stage_delete(&mut self, rel: &str) -> DomainResult<()> {
        self.tick()?;
        self.inner.stage_delete(rel)
    }
    fn commit(self: Box<Self>) -> DomainResult<()> {
        self.inner.commit()
    }
    fn abort(self: Box<Self>) {
        self.inner.abort()
    }
}

impl CollectionFileTxFactory for Wrapper {
    fn begin(&self, root: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        Ok(Box::new(WrappedTx {
            inner: self.inner.begin(root)?,
            count: 0,
            fail_at: self.fail_stage_at,
        }))
    }
    fn recover(&self, root: &Path) -> DomainResult<RecoveryOutcome> {
        if self.recover_blocked.load(Ordering::SeqCst)
            && root.join(".rocket/apply-journal.json").exists()
        {
            return Err(DomainError::Internal("recovery blocked".into()));
        }
        self.inner.recover(root)
    }
    fn list_files(&self, root: &Path) -> DomainResult<std::collections::HashSet<String>> {
        self.inner.list_files(root)
    }
}

struct World {
    _ws: TempDir,
    ws_path: PathBuf,
    root: PathBuf,
    name: String,
    svc: OpenApiSyncService,
    fetcher: Arc<FakeFetcher>,
    repo: rocket_infra::FsCollectionRepo,
}

fn deps(ws: &Path, tx: Arc<dyn CollectionFileTxFactory>) -> ApplyDeps {
    ApplyDeps {
        collections: Box::new(rocket_infra::FsCollectionRepo::new_standalone(
            ws.join("collections"),
        )),
        baselines: Box::new(rocket_infra::FsOpenApiBaselineRepo),
        locals: Box::new(rocket_infra::FsOpenApiLocalRepo),
        tx,
        renderer: Arc::new(|root: &Path| {
            Box::new(rocket_infra::FsRequestRenderer::for_collection(
                root.to_path_buf(),
            ))
        }),
    }
}

async fn world_with(tx: Arc<dyn CollectionFileTxFactory>) -> World {
    let ws = TempDir::new().expect("tempdir");
    let fetcher = Arc::new(FakeFetcher {
        body: Mutex::new(v1()),
    });
    let imported = import_service(ws.path())
        .import_openapi_url(
            &SharedFetcher(Arc::clone(&fetcher)),
            &rocket_infra::FsOpenApiSourceRepo,
            &rocket_infra::FsOpenApiBaselineRepo,
            LINK,
            GroupBy::Tags,
            None,
            "ws",
        )
        .await
        .expect("import");
    let ws_path = ws.path().to_path_buf();
    let root = ws_path.join("collections").join(&imported.collection);
    let svc = OpenApiSyncService::new(
        Arc::new(SharedFetcher(Arc::clone(&fetcher))),
        Box::new(rocket_infra::FsOpenApiSourceRepo),
        Arc::new(NoSecrets),
    )
    .with_apply(deps(&ws_path, tx));
    World {
        repo: rocket_infra::FsCollectionRepo::new_standalone(ws_path.join("collections")),
        _ws: ws,
        ws_path,
        root,
        name: imported.collection,
        svc,
        fetcher,
    }
}

async fn world() -> World {
    world_with(Arc::new(rocket_infra::FsFileTxFactory::new())).await
}

impl World {
    async fn check_with(&self, body: String) {
        *self.fetcher.body.lock().expect("lock") = body;
        self.svc.check(&self.root).await.expect("check");
    }

    fn plan(&self) -> SyncPlan {
        let source = rocket_infra::FsOpenApiSourceRepo
            .load(&self.root)
            .expect("load")
            .expect("source");
        let pending = rocket_infra::FsOpenApiSourceRepo
            .load_pending(&self.root)
            .expect("load")
            .expect("pending");
        let incoming = ParsedSpec::parse(&String::from_utf8(pending).expect("utf8")).expect("spec");
        let baseline = rocket_infra::FsOpenApiBaselineRepo
            .load(&self.root)
            .expect("load");
        let local = self.svc.local_state(&self.name).expect("local");
        let collection = self.repo.get(&self.name).expect("get");
        let mut current = Vec::new();
        flatten(&collection.root, "", &mut current);
        let settings = self.repo.get_settings(&self.name).expect("settings");
        let vars: Vec<String> = settings.variables.iter().map(|v| v.key.clone()).collect();
        let base = settings
            .variables
            .iter()
            .find(|v| v.key == "baseUrl")
            .map(|v| v.value.clone());
        plan_with_links(
            &SyncInputs {
                source: &source,
                baseline: baseline.as_ref(),
                incoming: &incoming,
                current: &current,
                current_base_url: base.as_deref(),
                existing_variables: &vars,
                skips: &local.skips,
            },
            &[],
        )
        .expect("plan")
    }

    fn approve(&self, plan: &SyncPlan, only: &[&str]) -> Selection {
        let items = plan
            .items
            .iter()
            .filter(|i| only.is_empty() || only.contains(&i.key()))
            .map(|i| {
                let choices = match i {
                    SyncItem::Changed { merge, .. } => merge
                        .changes
                        .iter()
                        .map(|c| (c.field.clone(), Choice::Spec))
                        .collect(),
                    _ => BTreeMap::new(),
                };
                ApprovedItem {
                    key: i.key().to_string(),
                    digest: item_digest(i),
                    choices,
                }
            })
            .collect();
        Selection {
            items,
            ..Selection::default()
        }
    }

    /// Every file under the collection root with its bytes.
    fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for e in std::fs::read_dir(dir).expect("dir") {
                let p = e.expect("entry").path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else {
                    let rel = p
                        .strip_prefix(root)
                        .expect("rel")
                        .to_string_lossy()
                        .to_string();
                    out.insert(rel, std::fs::read(&p).expect("read"));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.root, &self.root, &mut out);
        out
    }

    fn source(&self) -> OpenApiSource {
        rocket_infra::FsOpenApiSourceRepo
            .load(&self.root)
            .expect("load")
            .expect("source")
    }

    fn keys(&self) -> BTreeSet<String> {
        rocket_infra::FsOpenApiBaselineRepo
            .load(&self.root)
            .expect("load")
            .expect("baseline")
            .operations
            .into_iter()
            .map(|o| o.key)
            .collect()
    }

    fn no_transaction_leftovers(&self) {
        assert!(!self.root.join(".rocket/staging").exists());
        assert!(!self.root.join(".rocket/apply-journal.json").exists());
    }

    fn assert_fully_applied(&self) {
        let source = self.source();
        assert_eq!(source.spec_sha256, sha256_hex(v2().as_bytes()));
        assert_eq!(source.pending_sha256, None);
        assert!(rocket_infra::FsOpenApiSourceRepo
            .load_pending(&self.root)
            .expect("pending")
            .is_none());
        let keys: Vec<&str> = vec!["createPet", "getPet", "listPets"];
        assert_eq!(
            self.keys().iter().map(String::as_str).collect::<Vec<_>>(),
            keys
        );
        let created = source
            .operations
            .iter()
            .find(|o| o.key == "createPet")
            .expect("createPet entry");
        assert!(self
            .root
            .join(format!("{}.yml", created.request_path))
            .is_file());
        let left: Vec<String> = std::fs::read_dir(self.root.join("store"))
            .map(|d| {
                d.map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            left.iter().all(|n| n == "folder.yml"),
            "the unedited removed request is deleted, found {left:?}"
        );
    }
}

#[tokio::test]
async fn apply_changes_the_files_and_the_state_together() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    assert_eq!(plan.items.len(), 3);
    let report = w
        .svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect("apply");
    assert_eq!(report.applied.len(), 2);
    assert_eq!(report.deleted.len(), 1);
    w.assert_fully_applied();
    w.no_transaction_leftovers();
    assert!(w.root.join(".rocket/openapi-local.json").is_file());
}

#[tokio::test]
async fn apply_with_a_failing_step_changes_nothing() {
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::new(),
        fail_stage_at: Some(3),
        recover_blocked: Arc::new(AtomicBool::new(false)),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let before = w.snapshot();
    let plan = w.plan();
    let err = w.svc.apply(&w.name, &w.approve(&plan, &[])).await;
    assert!(err.is_err());
    assert_eq!(w.snapshot(), before);
    w.no_transaction_leftovers();
}

#[tokio::test]
async fn crash_mid_commit_is_finished_on_recovery() {
    let blocked = Arc::new(AtomicBool::new(true));
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::clone(&blocked),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let selection = w.approve(&plan, &[]);
    assert!(w.svc.apply(&w.name, &selection).await.is_err());
    assert!(w.root.join(".rocket/apply-journal.json").is_file());
    // Another apply refuses while the journal exists.
    let again = w.svc.apply(&w.name, &selection).await;
    assert!(matches!(again, Err(DomainError::Conflict(_))));
    blocked.store(false, Ordering::SeqCst);
    let outcome = w.svc.recover(&w.name).expect("recover");
    assert!(matches!(outcome, RecoveryOutcome::Finished { .. }));
    w.assert_fully_applied();
    w.no_transaction_leftovers();
}

#[tokio::test]
async fn a_failed_commit_is_followed_by_recovery_inside_apply() {
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::new(AtomicBool::new(false)),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let plan = w.plan();
    assert!(w.svc.apply(&w.name, &w.approve(&plan, &[])).await.is_err());
    w.assert_fully_applied();
    w.no_transaction_leftovers();
}

#[tokio::test]
async fn stale_approval_after_a_user_edit_is_reported_and_not_applied() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let selection = w.approve(&plan, &["listPets"]);
    let path = w
        .source()
        .operations
        .iter()
        .find(|o| o.key == "listPets")
        .expect("entry")
        .request_path
        .clone();
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = "https://edited.example.com/pets".to_string();
    w.repo.save_request(&w.name, &path, &request).expect("save");
    let before = w.snapshot();
    let report = w.svc.apply(&w.name, &selection).await.expect("apply");
    assert_eq!(report.stale, vec!["listPets".to_string()]);
    assert!(report.applied.is_empty());
    let after = w.snapshot();
    let file = format!("{path}.yml");
    assert_eq!(before.get(&file), after.get(&file));
}

#[tokio::test]
async fn changed_pending_hash_refuses_to_apply() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let selection = w.approve(&plan, &[]);
    rocket_infra::FsOpenApiSourceRepo
        .save_pending(&w.root, v3().as_bytes())
        .expect("pending");
    let before = w.snapshot();
    let err = w.svc.apply(&w.name, &selection).await;
    assert!(matches!(err, Err(DomainError::Conflict(_))));
    assert_eq!(w.snapshot(), before);
}

#[tokio::test]
async fn partial_apply_then_check_keeps_the_rest_visible() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let report = w
        .svc
        .apply(&w.name, &w.approve(&plan, &["createPet"]))
        .await
        .expect("apply");
    assert_eq!(report.applied.len(), 1);
    assert!(report.remaining >= 2);
    assert!(w.source().pending_sha256.is_some());
    w.check_with(v2()).await;
    let keys: BTreeSet<String> = w.plan().items.iter().map(|i| i.key().to_string()).collect();
    assert_eq!(
        keys,
        BTreeSet::from(["listPets".to_string(), "getInventory".to_string()])
    );
    // Skip the change of listPets at what the user saw.
    let plan = w.plan();
    let fp = match plan.items.iter().find(|i| i.key() == "listPets") {
        Some(SyncItem::Changed { spec, .. }) => fingerprint(spec),
        other => panic!("unexpected item {other:?}"),
    };
    let selection = Selection {
        skips: vec![SkipRequest {
            key: "listPets".to_string(),
            fingerprint: fp,
        }],
        ..Selection::default()
    };
    w.svc.apply(&w.name, &selection).await.expect("skip");
    assert!(w
        .svc
        .local_state(&w.name)
        .expect("local")
        .skips
        .contains_key("listPets"));
    assert!(w.plan().items.iter().all(|i| i.key() != "listPets"));
    // The spec changes that endpoint again, so it comes back.
    w.check_with(v3()).await;
    assert!(w.plan().items.iter().any(|i| i.key() == "listPets"));
}

#[tokio::test]
async fn moved_request_is_found_by_uid_and_the_source_path_is_updated() {
    let w = world().await;
    w.check_with(v2()).await;
    let old = w
        .source()
        .operations
        .iter()
        .find(|o| o.key == "listPets")
        .expect("entry")
        .request_path
        .clone();
    w.repo.create_folder(&w.name, "archive").expect("folder");
    let stem = old.rsplit('/').next().expect("stem").to_string();
    let new = format!("archive/{stem}");
    w.repo
        .move_item(
            &w.name,
            &format!("{old}.yml"),
            &w.name,
            &format!("{new}.yml"),
        )
        .expect("move");
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &["listPets"]))
        .await
        .expect("apply");
    let entry = w
        .source()
        .operations
        .into_iter()
        .find(|o| o.key == "listPets")
        .expect("entry");
    assert_eq!(entry.request_path, new);
    assert!(!w.root.join(format!("{old}.yml")).exists());
    let moved = w.repo.get_request(&w.name, &new).expect("get");
    assert!(moved.query_params.iter().any(|q| q.key == "limit"));
}

#[tokio::test]
async fn recover_all_finds_journals_in_every_collection() {
    let blocked = Arc::new(AtomicBool::new(true));
    let factory = Arc::new(Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::clone(&blocked),
    });
    let w = world_with(factory).await;
    w.check_with(v2()).await;
    let plan = w.plan();
    assert!(w.svc.apply(&w.name, &w.approve(&plan, &[])).await.is_err());
    // A second collection with a journal that was cut off while staging.
    let other = w.ws_path.join("collections").join("Other");
    w.repo.create("Other").expect("create");
    std::fs::create_dir_all(other.join(".rocket")).expect("dir");
    blocked.store(false, Ordering::SeqCst);
    let found = w.svc.recover_all();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, w.name);
    assert!(matches!(found[0].1, RecoveryOutcome::Finished { .. }));
    w.no_transaction_leftovers();
}
