//! Tests for applying a spec through the file transaction, on a real temporary workspace.

use super::*;
use crate::openapi_sync_service::{sha256_hex, CheckOutcome};
use async_trait::async_trait;
use rocket_collection::openapi_source::{OpenApiSourceRepository, SyncInterval, SyncStatus};
use rocket_environment::SecretStore;
use rocket_http::spec_fetch::{SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher};
use rocket_import::{
    fingerprint, item_digest, plan_with_links, variable_digest, ApprovedItem, Choice, GroupBy,
    ImportService, SkipRequest, SyncItem, SyncPlan, VariableApproval,
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

struct Gate {
    started: tokio::sync::Notify,
    go: tokio::sync::Notify,
}

struct FakeFetcher {
    body: Mutex<String>,
    etag: Mutex<Option<String>>,
    honor_304: AtomicBool,
    gate: Mutex<Option<Arc<Gate>>>,
    requests: Mutex<Vec<SpecFetchRequest>>,
    /// Extra files by address: body and etag. They answer 304 to their own etag.
    files: Mutex<BTreeMap<String, (String, String)>>,
}

impl FakeFetcher {
    fn new(body: String) -> Self {
        Self {
            body: Mutex::new(body),
            etag: Mutex::new(None),
            honor_304: AtomicBool::new(false),
            gate: Mutex::new(None),
            requests: Mutex::new(Vec::new()),
            files: Mutex::new(BTreeMap::new()),
        }
    }
}

struct SharedFetcher(Arc<FakeFetcher>);

#[async_trait]
impl SpecFetcher for SharedFetcher {
    async fn fetch(&self, request: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        let has_validator = request.if_none_match.is_some();
        let file = self
            .0
            .files
            .lock()
            .expect("lock")
            .get(&request.url)
            .cloned();
        if let Some((body, etag)) = file {
            if request.if_none_match.as_deref() == Some(etag.as_str()) {
                return Ok(SpecFetchOutcome::NotModified);
            }
            return Ok(SpecFetchOutcome::Fetched {
                body: body.into_bytes(),
                etag: Some(etag),
                last_modified: None,
                final_url: request.url.clone(),
            });
        }
        self.0.requests.lock().expect("lock").push(request);
        let gate = self.0.gate.lock().expect("lock").clone();
        if let Some(gate) = gate {
            gate.started.notify_one();
            gate.go.notified().await;
        }
        if has_validator && self.0.honor_304.load(Ordering::SeqCst) {
            return Ok(SpecFetchOutcome::NotModified);
        }
        Ok(SpecFetchOutcome::Fetched {
            body: self.0.body.lock().expect("lock").clone().into_bytes(),
            etag: self.0.etag.lock().expect("lock").clone(),
            last_modified: None,
            final_url: LINK.to_string(),
        })
    }
}

/// Holds no secret and counts the keychain reads.
struct NoSecrets(Arc<std::sync::atomic::AtomicUsize>);

impl SecretStore for NoSecrets {
    fn get(&self, _: &str, _: &str) -> DomainResult<Option<String>> {
        self.0.fetch_add(1, Ordering::SeqCst);
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
    secret_reads: Arc<std::sync::atomic::AtomicUsize>,
    /// How many spec files the service read, to show that nothing is read without approval.
    reader_reads: Arc<std::sync::atomic::AtomicUsize>,
}

/// The real file reader, counting its reads.
struct CountingReader(Arc<std::sync::atomic::AtomicUsize>);

impl rocket_http::LocalSpecReader for CountingReader {
    fn read(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> Result<Vec<u8>, rocket_http::LocalReadFailure> {
        self.0.fetch_add(1, Ordering::SeqCst);
        rocket_http::LocalSpecReader::read(&rocket_infra::FsLocalSpecReader, path, max_bytes)
    }
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
    world_from(tx, v1(), |_| {}).await
}

async fn world_from(
    tx: Arc<dyn CollectionFileTxFactory>,
    body: String,
    setup: impl FnOnce(&FakeFetcher),
) -> World {
    let ws = TempDir::new().expect("tempdir");
    let fetcher = Arc::new(FakeFetcher::new(body));
    setup(&fetcher);
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
    let secret_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let reader_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let svc = OpenApiSyncService::new(
        Arc::new(SharedFetcher(Arc::clone(&fetcher))),
        Box::new(rocket_infra::FsOpenApiSourceRepo),
        Arc::new(NoSecrets(Arc::clone(&secret_reads))),
    )
    .with_consent(Arc::new(rocket_collection::InMemoryConsentStore::new()))
    .with_reader(Arc::new(CountingReader(Arc::clone(&reader_reads))))
    .with_backups(Arc::new(rocket_infra::FsOpenApiBackupRepo))
    .with_apply(deps(&ws_path, tx));
    svc.grant_current(&root).expect("consent");
    World {
        repo: rocket_infra::FsCollectionRepo::new_standalone(ws_path.join("collections")),
        _ws: ws,
        ws_path,
        root,
        name: imported.collection,
        svc,
        fetcher,
        secret_reads,
        reader_reads,
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
async fn apply_keeps_the_timing_fields() {
    let w = world().await;
    w.svc
        .set_interval(&w.root, SyncInterval::Daily)
        .expect("interval");
    w.check_with(v2()).await;
    // Seed failure fields that are not at their defaults, as a failed check would leave them.
    let mut seeded = w.svc.local_state(&w.name).expect("local");
    seeded.consecutive_failures = 2;
    seeded.not_before = Some("2026-10-26T18:00:00+00:00".to_string());
    rocket_infra::FsOpenApiLocalRepo
        .save(&w.root, &seeded)
        .expect("seed");
    let before = w.svc.local_state(&w.name).expect("local");
    assert_eq!(before.consecutive_failures, 2);
    assert_eq!(before.sync_interval, SyncInterval::Daily);
    assert!(before.opt_in_root.is_none());
    assert!(before.last_checked_at.is_some());
    // A partial apply keeps the failure counters and the status.
    let plan = w.plan();
    let first = plan.items[0].key().to_string();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[first.as_str()]))
        .await
        .expect("partial apply");
    let mid = w.svc.local_state(&w.name).expect("local");
    assert_eq!(mid.sync_interval, SyncInterval::Daily);
    assert_eq!(mid.opt_in_root, before.opt_in_root);
    assert_eq!(mid.last_checked_at, before.last_checked_at);
    assert_eq!(mid.consecutive_failures, 2);
    assert_eq!(mid.not_before, before.not_before);
    assert_eq!(mid.last_status, before.last_status);
    // A finalized apply resets the failure state and sets the status.
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect("apply");
    let after = w.svc.local_state(&w.name).expect("local");
    assert_eq!(after.sync_interval, SyncInterval::Daily);
    assert_eq!(after.opt_in_root, before.opt_in_root);
    assert_eq!(after.last_checked_at, before.last_checked_at);
    assert_eq!(after.consecutive_failures, 0);
    assert_eq!(after.not_before, None);
    assert_eq!(after.last_status, Some(SyncStatus::Unchanged));
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
async fn a_finalized_apply_that_fails_at_any_step_keeps_the_previous_applied_copy() {
    let mut failed_at_least_once = false;
    let mut succeeded = false;
    for step in 1..=16 {
        let factory = Wrapper {
            inner: rocket_infra::FsFileTxFactory::new(),
            fail_stage_at: Some(step),
            recover_blocked: Arc::new(AtomicBool::new(false)),
        };
        let w = world_with(Arc::new(factory)).await;
        w.check_with(v2()).await;
        let before = w.snapshot();
        let plan = w.plan();
        let result = w.svc.apply(&w.name, &w.approve(&plan, &[])).await;
        if result.is_ok() {
            // The apply needs fewer than `step` staging calls, so the loop has covered them all.
            assert_eq!(
                std::fs::read_to_string(w.root.join(".rocket/openapi-applied.spec")).expect("read"),
                v2()
            );
            succeeded = true;
            break;
        }
        failed_at_least_once = true;
        assert_eq!(w.snapshot(), before, "step {step}");
        assert_eq!(
            std::fs::read_to_string(w.root.join(".rocket/openapi-applied.spec")).expect("read"),
            v1(),
            "step {step}"
        );
        w.no_transaction_leftovers();
    }
    assert!(failed_at_least_once);
    // Every staging step was tried: the apply finally went through within the loop.
    assert!(succeeded);
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
    let err = w
        .svc
        .apply(&w.name, &selection)
        .await
        .expect_err("recovery is blocked");
    assert!(err.to_string().contains("next time Rocket starts"), "{err}");
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
async fn a_failed_commit_that_recovery_finishes_reports_success() {
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::new(AtomicBool::new(false)),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let report = w
        .svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect("every change landed");
    assert_eq!(report.applied.len(), 2);
    w.assert_fully_applied();
    w.no_transaction_leftovers();
}

#[tokio::test]
async fn a_failed_commit_that_rolls_back_says_nothing_changed() {
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_fault(1),
        fail_stage_at: None,
        recover_blocked: Arc::new(AtomicBool::new(false)),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let before = w.snapshot();
    let plan = w.plan();
    let err = w
        .svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect_err("fails");
    assert!(err.to_string().contains("nothing was changed"), "{err}");
    assert_eq!(w.snapshot(), before);
    w.no_transaction_leftovers();
}

#[tokio::test]
async fn a_check_finishes_an_interrupted_apply_first() {
    let blocked = Arc::new(AtomicBool::new(true));
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::clone(&blocked),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let plan = w.plan();
    assert!(w.svc.apply(&w.name, &w.approve(&plan, &[])).await.is_err());
    assert!(w.root.join(".rocket/apply-journal.json").is_file());
    blocked.store(false, Ordering::SeqCst);
    w.svc.check(&w.root).await.expect("check");
    w.no_transaction_leftovers();
    w.assert_fully_applied();
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
    w.repo.create("Other").expect("create");
    let other = w.ws_path.join("collections").join("Other");
    std::fs::create_dir_all(other.join(".rocket/staging/x")).expect("dir");
    std::fs::write(
        other.join(".rocket/apply-journal.json"),
        r#"{"version":1,"id":"x","phase":"staging","ops":[]}"#,
    )
    .expect("journal");
    blocked.store(false, Ordering::SeqCst);
    let found = w.svc.recover_all();
    assert_eq!(found.len(), 2);
    let first = found.iter().find(|(n, _)| *n == w.name).expect("first");
    assert!(matches!(first.1, RecoveryOutcome::Finished { .. }));
    let second = found.iter().find(|(n, _)| n == "Other").expect("second");
    assert_eq!(second.1, RecoveryOutcome::Discarded);
    assert!(!other.join(".rocket/apply-journal.json").exists());
    w.no_transaction_leftovers();
}

fn approve_vars(plan: &SyncPlan, names: &[&str]) -> Vec<VariableApproval> {
    names
        .iter()
        .filter_map(|n| plan.variable_changes.iter().find(|c| c.name == *n))
        .map(|c| VariableApproval {
            name: c.name.clone(),
            digest: variable_digest(c),
        })
        .collect()
}

fn v4() -> String {
    // Same operations as v1 but another server.
    v1().replace("https://api.example.com", "https://api2.example.com")
}

fn v5() -> String {
    let mut doc: serde_json::Value = serde_json::from_str(&v1()).expect("json");
    doc["components"] =
        json!({"securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}}});
    doc["security"] = json!([{"bearerAuth": []}]);
    doc.to_string()
}

/// Adds an `extensions` key to the real settings file and returns its text.
fn mark_settings(w: &World) -> String {
    let path = w.root.join("opencollection.yml");
    let text = std::fs::read_to_string(&path).expect("read");
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("yaml");
    let ext = serde_yaml::from_str::<serde_yaml::Value>("rocket-test: {keep: 1}").expect("yaml");
    doc.as_mapping_mut()
        .expect("map")
        .insert("extensions".into(), ext);
    std::fs::write(&path, serde_yaml::to_string(&doc).expect("yaml")).expect("write");
    std::fs::read_to_string(&path).expect("read")
}

fn assert_identity_kept(w: &World, before: &str) {
    let old: serde_yaml::Value = serde_yaml::from_str(before).expect("yaml");
    let new: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(w.root.join("opencollection.yml")).expect("read"),
    )
    .expect("yaml");
    assert_eq!(old["uid"], new["uid"]);
    assert_eq!(old["info"]["name"], new["info"]["name"]);
    assert_eq!(
        old["extensions"]["rocket-test"],
        new["extensions"]["rocket-test"]
    );
}

#[tokio::test]
async fn a_base_url_change_keeps_the_collection_identity() {
    let w = world().await;
    let before = mark_settings(&w);
    w.check_with(v4()).await;
    let plan = w.plan();
    assert_eq!(plan.variable_changes.len(), 1);
    let selection = Selection {
        variables: approve_vars(&plan, &["baseUrl"]),
        ..Selection::default()
    };
    w.svc.apply(&w.name, &selection).await.expect("apply");
    assert_identity_kept(&w, &before);
    let settings = w.repo.get_settings(&w.name).expect("settings");
    let base = settings
        .variables
        .iter()
        .find(|v| v.key == "baseUrl")
        .expect("baseUrl");
    assert_eq!(base.value, "https://api2.example.com");
}

#[tokio::test]
async fn an_added_security_variable_keeps_the_collection_identity() {
    let w = world().await;
    let before = mark_settings(&w);
    w.check_with(v5()).await;
    let plan = w.plan();
    let names: Vec<String> = plan
        .variable_changes
        .iter()
        .filter(|c| c.kind == rocket_import::VariableKind::AddSecurity)
        .map(|c| c.name.clone())
        .collect();
    assert!(!names.is_empty(), "{:?}", plan.variable_changes);
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let selection = Selection {
        variables: approve_vars(&plan, &name_refs),
        ..Selection::default()
    };
    w.svc.apply(&w.name, &selection).await.expect("apply");
    assert_identity_kept(&w, &before);
    let settings = w.repo.get_settings(&w.name).expect("settings");
    assert!(settings.variables.iter().any(|v| v.key == names[0]));
}

#[tokio::test]
async fn a_spec_url_that_moves_after_the_review_is_not_applied() {
    let w = world().await;
    w.check_with(v4()).await;
    let plan = w.plan();
    let selection = Selection {
        variables: approve_vars(&plan, &["baseUrl"]),
        ..Selection::default()
    };
    // The spec changes its host again before the user applies.
    w.check_with(v1().replace("https://api.example.com", "https://api3.example.com"))
        .await;
    let report = w.svc.apply(&w.name, &selection).await.expect("apply");
    assert_eq!(report.stale, vec!["baseUrl".to_string()]);
    let settings = w.repo.get_settings(&w.name).expect("settings");
    let base = settings
        .variables
        .iter()
        .find(|v| v.key == "baseUrl")
        .expect("baseUrl");
    assert_eq!(base.value, "https://api.example.com");
    assert!(w.source().pending_sha256.is_some());
}

#[test]
fn two_applies_on_one_collection_serialize() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let w = rt.block_on(world());
    rt.block_on(w.check_with(v2()));
    let plan = w.plan();
    let selection = w.approve(&plan, &[]);
    let results: Vec<DomainResult<ApplyReport>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .build()
                        .expect("runtime")
                        .block_on(w.svc.apply(&w.name, &selection))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect()
    });
    let landed: Vec<&ApplyReport> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
    assert!(!landed.is_empty(), "{results:?}");
    // The second run sees fresh state: it either refuses (the pending copy is gone) or has
    // nothing left to apply. Nothing is lost either way.
    for r in &results {
        match r {
            Ok(report) => assert!(report.applied.len() <= 2),
            Err(e) => assert!(matches!(e, DomainError::Conflict(_)), "{e}"),
        }
    }
    w.assert_fully_applied();
    w.no_transaction_leftovers();
}

#[tokio::test]
async fn a_check_finishing_during_an_apply_does_not_undo_it() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let selection = w.approve(&plan, &[]);
    let gate = Arc::new(Gate {
        started: tokio::sync::Notify::new(),
        go: tokio::sync::Notify::new(),
    });
    *w.fetcher.gate.lock().expect("lock") = Some(Arc::clone(&gate));
    let check = w.svc.check(&w.root);
    let apply = async {
        gate.started.notified().await;
        let report = w.svc.apply(&w.name, &selection).await.expect("apply");
        gate.go.notify_one();
        report
    };
    let (outcome, report) = tokio::join!(check, apply);
    assert_eq!(report.applied.len(), 2);
    assert_eq!(outcome.expect("check"), CheckOutcome::Unchanged);
    w.assert_fully_applied();
}

#[tokio::test]
async fn a_pending_hash_without_its_file_is_fetched_again_after_a_304() {
    let w = world().await;
    *w.fetcher.etag.lock().expect("lock") = Some("\"e1\"".to_string());
    w.fetcher.honor_304.store(true, Ordering::SeqCst);
    w.check_with(v2()).await;
    assert!(w.source().etag.is_some());
    // A teammate's checkout has the source file but not the git-ignored pending copy.
    rocket_infra::FsOpenApiSourceRepo
        .clear_pending(&w.root)
        .expect("clear");
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert!(matches!(outcome, CheckOutcome::Changed { .. }));
    assert!(rocket_infra::FsOpenApiSourceRepo
        .load_pending(&w.root)
        .expect("load")
        .is_some());
    let last = w
        .fetcher
        .requests
        .lock()
        .expect("lock")
        .last()
        .cloned()
        .expect("request");
    assert!(last.if_none_match.is_none());
}

#[tokio::test]
async fn the_same_body_rewrites_a_missing_pending_copy() {
    let w = world().await;
    w.check_with(v2()).await;
    rocket_infra::FsOpenApiSourceRepo
        .clear_pending(&w.root)
        .expect("clear");
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert!(matches!(outcome, CheckOutcome::Changed { .. }));
    let pending = rocket_infra::FsOpenApiSourceRepo
        .load_pending(&w.root)
        .expect("load")
        .expect("pending is back");
    assert_eq!(sha256_hex(&pending), sha256_hex(v2().as_bytes()));
}

/// Replaces the journal with a foreign one just before the commit, like another process would.
struct TakeOverFactory(rocket_infra::FsFileTxFactory);

struct TakeOverTx {
    inner: Box<dyn CollectionFileTx>,
    root: PathBuf,
}

impl CollectionFileTx for TakeOverTx {
    fn stage_put(&mut self, rel: &str, bytes: &[u8]) -> DomainResult<()> {
        self.inner.stage_put(rel, bytes)
    }
    fn stage_delete(&mut self, rel: &str) -> DomainResult<()> {
        self.inner.stage_delete(rel)
    }
    fn commit(self: Box<Self>) -> DomainResult<()> {
        std::fs::write(
            self.root.join(".rocket/apply-journal.json"),
            r#"{"version":1,"id":"foreign","phase":"staging","ops":[]}"#,
        )
        .expect("journal");
        self.inner.commit()
    }
    fn abort(self: Box<Self>) {
        self.inner.abort()
    }
}

impl CollectionFileTxFactory for TakeOverFactory {
    fn begin(&self, root: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        Ok(Box::new(TakeOverTx {
            inner: self.0.begin(root)?,
            root: root.to_path_buf(),
        }))
    }
    fn recover(&self, root: &Path) -> DomainResult<RecoveryOutcome> {
        self.0.recover(root)
    }
    fn list_files(&self, root: &Path) -> DomainResult<std::collections::HashSet<String>> {
        self.0.list_files(root)
    }
}

#[tokio::test]
async fn a_lost_staging_area_is_reported_and_the_foreign_journal_is_left_alone() {
    let w = world_with(Arc::new(TakeOverFactory(
        rocket_infra::FsFileTxFactory::new(),
    )))
    .await;
    w.check_with(v2()).await;
    let before = w.source();
    let plan = w.plan();
    let err = w
        .svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect_err("conflict");
    assert!(matches!(err, DomainError::Conflict(_)), "{err}");
    // No recovery ran, so the other process's journal is still there.
    assert!(w.root.join(".rocket/apply-journal.json").is_file());
    assert_eq!(w.source().spec_sha256, before.spec_sha256);
}

const DEFS: &str = "https://example.com/specs/defs.yaml";

fn defs_body(extra: bool) -> String {
    let mut text =
        "parameters:\n  limit: { name: limit, in: query, schema: { type: integer } }\n".to_string();
    if extra {
        text.push_str("  offset: { name: offset, in: query, schema: { type: integer } }\n");
    }
    text
}

fn spec_with_refs(names: &[&str]) -> String {
    let params: Vec<serde_json::Value> = names
        .iter()
        .map(|n| json!({"$ref": format!("{DEFS}#/parameters/{n}")}))
        .collect();
    spec(json!({
        "/pets": {"get": op("listPets", "pets", json!({"parameters": params}))},
    }))
}

#[tokio::test]
async fn a_changed_referenced_file_applies_the_new_parameter_and_keeps_user_edits() {
    let w = world_from(
        Arc::new(rocket_infra::FsFileTxFactory::new()),
        spec_with_refs(&["limit", "offset"]),
        |f| {
            *f.etag.lock().expect("lock") = Some("\"m1\"".to_string());
            f.honor_304.store(true, Ordering::SeqCst);
            f.files
                .lock()
                .expect("lock")
                .insert(DEFS.to_string(), (defs_body(true), "\"r1\"".to_string()));
        },
    )
    .await;
    let imported = w.source();
    assert_eq!(imported.remote_refs.len(), 1);
    // The user edits the request before the referenced file changes.
    let path = imported
        .operations
        .iter()
        .find(|o| o.key == "listPets")
        .expect("entry")
        .request_path
        .clone();
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = "https://edited.example.com/pets".to_string();
    w.repo.save_request(&w.name, &path, &request).expect("save");
    // Only the file changes: `limit` moves from the query to a header. The main body stays
    // the same and still answers 304.
    w.fetcher.files.lock().expect("lock").insert(
        DEFS.to_string(),
        (
            defs_body(true).replace("name: limit, in: query", "name: limit, in: header"),
            "\"r2\"".to_string(),
        ),
    );
    let out = w.svc.check(&w.root).await.expect("check");
    assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect("apply");
    let applied = w.repo.get_request(&w.name, &path).expect("get");
    assert_eq!(applied.url, "https://edited.example.com/pets");
    let names: Vec<&str> = applied
        .query_params
        .iter()
        .map(|q| q.key.as_str())
        .collect();
    assert!(!names.contains(&"limit"), "{names:?}");
    assert!(names.contains(&"offset"), "{names:?}");
    assert!(applied.headers.iter().any(|h| h.key == "limit"));
    assert_eq!(w.source().remote_refs[0].etag.as_deref(), Some("\"r2\""));
}

#[path = "openapi_sync_plan_tests.rs"]
mod plan_tests;

#[tokio::test]
async fn a_finalized_apply_clears_the_failure_message() {
    let w = world().await;
    w.check_with(v2()).await;
    let mut local = w.svc.local_state(&w.name).expect("local");
    local.last_message = Some("The server could not be reached.".to_string());
    rocket_infra::FsOpenApiLocalRepo
        .save(&w.root, &local)
        .expect("save");
    let plan = w.plan();
    let selection = w.approve(&plan, &[]);
    let report = w.svc.apply(&w.name, &selection).await.expect("apply");
    assert_eq!(report.remaining, 0);
    assert_eq!(
        w.svc.local_state(&w.name).expect("local").last_message,
        None
    );
}

#[cfg(test)]
#[path = "openapi_sync_changes_tests.rs"]
mod changes_tests;

#[cfg(test)]
#[path = "openapi_sync_spec_view_tests.rs"]
mod spec_view_tests;

#[cfg(test)]
#[path = "openapi_sync_reset_tests.rs"]
mod reset_tests;

#[path = "openapi_sync_skips_tests.rs"]
mod skips_tests;

#[path = "openapi_file_source_tests.rs"]
mod file_source_tests;

#[path = "openapi_sync_delete_tests.rs"]
mod delete_tests;

#[cfg(test)]
#[path = "openapi_connect_tests.rs"]
mod connect_tests;

#[cfg(test)]
#[path = "openapi_change_link_tests.rs"]
mod change_link_tests;
