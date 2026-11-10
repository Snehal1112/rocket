//! Tests for the machine-local consent gate: no request and no keychain read without it.

use super::*;
use async_trait::async_trait;
use rocket_collection::InMemoryConsentStore;
use rocket_infra::{FsOpenApiLocalRepo, FsOpenApiSourceRepo};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tempfile::TempDir;

const TOKEN: &str = "ghp_consent_test_token";

/// Records every request and answers "not modified".
#[derive(Default)]
struct Recorder {
    requests: Mutex<Vec<SpecFetchRequest>>,
}

struct SharedRecorder(Arc<Recorder>);

#[async_trait]
impl SpecFetcher for SharedRecorder {
    async fn fetch(&self, request: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        self.0.requests.lock().expect("lock").push(request);
        Ok(SpecFetchOutcome::NotModified)
    }
}

/// Counts every keychain read, strict or not.
#[derive(Default)]
struct CountingSecrets {
    map: Mutex<HashMap<(String, String), String>>,
    reads: AtomicUsize,
    writes: AtomicUsize,
}

impl SecretStore for CountingSecrets {
    fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .map
            .lock()
            .expect("lock")
            .get(&(scope.to_string(), key.to_string()))
            .cloned())
    }
    fn get_strict(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
        self.get(scope, key)
    }
    fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.map
            .lock()
            .expect("lock")
            .insert((scope.to_string(), key.to_string()), value.to_string());
        Ok(())
    }
    fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
        self.map
            .lock()
            .expect("lock")
            .remove(&(scope.to_string(), key.to_string()));
        Ok(())
    }
}

fn github(repo: &str) -> SpecLocation {
    SpecLocation::Github {
        owner: "o".into(),
        repo: repo.into(),
        git_ref: "main".into(),
        path: "api.json".into(),
    }
}

fn source(location: SpecLocation) -> OpenApiSource {
    OpenApiSource {
        version: 1,
        location,
        group_by: "tags".into(),
        spec_sha256: "abc".into(),
        etag: Some("\"v1\"".into()),
        last_modified: None,
        operations: Vec::new(),
        source_id: "src-1".into(),
        sync_interval: SyncInterval::Off,
        last_checked_at: None,
        last_status: None,
        pending_sha256: None,
        remote_refs: Vec::new(),
        remote_refs_incomplete: false,
    }
}

struct World {
    dir: TempDir,
    recorder: Arc<Recorder>,
    secrets: Arc<CountingSecrets>,
    store: Arc<InMemoryConsentStore>,
    svc: OpenApiSyncService,
}

impl World {
    /// A GitHub source with a stored token and no consent yet.
    fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        FsOpenApiSourceRepo
            .save(dir.path(), &source(github("r")))
            .expect("seed");
        let recorder = Arc::new(Recorder::default());
        let secrets = Arc::new(CountingSecrets::default());
        secrets.set("src-1", "token", TOKEN).expect("token");
        secrets.writes.store(0, Ordering::SeqCst);
        let store = Arc::new(InMemoryConsentStore::new());
        let svc = OpenApiSyncService::new(
            Arc::new(SharedRecorder(Arc::clone(&recorder))),
            Box::new(FsOpenApiSourceRepo),
            Arc::clone(&secrets) as Arc<dyn SecretStore>,
        )
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_consent(Arc::clone(&store) as Arc<dyn OpenApiConsentStore>);
        Self {
            dir,
            recorder,
            secrets,
            store,
            svc,
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn request_count(&self) -> usize {
        self.recorder.requests.lock().expect("lock").len()
    }

    fn reads(&self) -> usize {
        self.secrets.reads.load(Ordering::SeqCst)
    }

    fn assert_untouched(&self) {
        assert_eq!(self.request_count(), 0, "no request may go out");
        assert_eq!(self.reads(), 0, "the keychain may not be read");
    }
}

fn failed_code(outcome: CheckOutcome) -> String {
    match outcome {
        CheckOutcome::Failed { code, .. } => code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[tokio::test]
async fn check_without_consent_makes_no_request_and_reads_no_token() {
    let w = World::new();
    let outcome = w.svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "consent_required");
    w.assert_untouched();
    // The refusal also leaves no check record behind.
    assert!(FsOpenApiLocalRepo.load(w.root()).expect("load").is_none());
}

#[tokio::test]
async fn check_with_location_changed_makes_no_request_and_reads_no_token() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    // The committed source file is edited to point at another repository.
    FsOpenApiSourceRepo
        .save(w.root(), &source(github("attacker-repo")))
        .expect("edit");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::LocationChanged
    );
    let outcome = w.svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "location_changed");
    w.assert_untouched();
    // Status does not read the keychain either, and says what happened.
    let status = w.svc.status(w.root(), true).expect("status");
    assert_eq!(status.consent, ConsentState::LocationChanged);
    assert_eq!(status.has_token, Some(false));
    assert_eq!(w.reads(), 0);
}

#[tokio::test]
async fn check_with_consent_sends_the_token_to_the_approved_location() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    let outcome = w.svc.check(w.root()).await.expect("check");
    assert_eq!(outcome, CheckOutcome::Unchanged);
    let requests = w.recorder.requests.lock().expect("lock");
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].url.contains("/repos/o/r/contents/api.json"),
        "{}",
        requests[0].url
    );
    assert_eq!(requests[0].bearer_token.as_deref(), Some(TOKEN));
    assert!(w.reads() > 0);
}

#[test]
fn consent_is_per_root_so_a_copied_folder_is_missing() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    let copy = TempDir::new().expect("copy");
    FsOpenApiSourceRepo
        .save(copy.path(), &source(github("r")))
        .expect("copy source");
    // Same source id and same location, but another folder.
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
    assert_eq!(
        w.svc.consent_state(copy.path()).expect("state"),
        ConsentState::Missing
    );
    let status = w.svc.status(copy.path(), true).expect("status");
    assert_eq!(status.has_token, Some(false));
    assert_eq!(w.reads(), 0);
    // Approving the copy does not touch the original, and revoking it leaves the original.
    w.svc.grant_current(copy.path()).expect("grant copy");
    w.store.revoke(copy.path(), "src-1").expect("revoke");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
}

#[tokio::test]
async fn a_service_without_a_consent_store_never_checks_or_reads_the_keychain() {
    let w = World::new();
    let svc = OpenApiSyncService::new(
        Arc::new(SharedRecorder(Arc::clone(&w.recorder))),
        Box::new(FsOpenApiSourceRepo),
        Arc::clone(&w.secrets) as Arc<dyn SecretStore>,
    )
    .with_local(Box::new(FsOpenApiLocalRepo));
    let outcome = svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "consent_required");
    assert_eq!(
        svc.status(w.root(), true).expect("status").has_token,
        Some(false)
    );
    w.assert_untouched();
}

#[tokio::test]
async fn revoking_consent_stops_the_next_check() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    w.store.revoke(w.root(), "src-1").expect("revoke");
    let outcome = w.svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "consent_required");
    w.assert_untouched();
}

#[tokio::test]
async fn revoke_consent_stops_checks_and_the_interval() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    w.svc.revoke_consent(w.root()).expect("revoke");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Missing
    );
    let outcome = w.svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "consent_required");
    w.assert_untouched();
}

/// A factory whose recovery rewrites the source file, like a journal replay would.
struct RewritingTx {
    inner: rocket_infra::FsFileTxFactory,
    location: SpecLocation,
}

impl rocket_collection::CollectionFileTxFactory for RewritingTx {
    fn begin(&self, root: &Path) -> DomainResult<Box<dyn rocket_collection::CollectionFileTx>> {
        self.inner.begin(root)
    }
    fn recover(&self, root: &Path) -> DomainResult<rocket_collection::RecoveryOutcome> {
        FsOpenApiSourceRepo.save(root, &source(self.location.clone()))?;
        self.inner.recover(root)
    }
    fn list_files(&self, root: &Path) -> DomainResult<std::collections::HashSet<String>> {
        self.inner.list_files(root)
    }
}

#[tokio::test]
async fn a_recovery_that_rewrites_the_location_makes_no_request_and_reads_no_token() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    let tx = RewritingTx {
        inner: rocket_infra::FsFileTxFactory::new(),
        location: github("attacker-repo"),
    };
    let deps = ApplyDeps {
        collections: Box::new(rocket_infra::FsCollectionRepo::new_standalone(
            w.root().join("collections"),
        )),
        baselines: Box::new(rocket_infra::FsOpenApiBaselineRepo),
        locals: Box::new(FsOpenApiLocalRepo),
        tx: Arc::new(tx),
        renderer: Arc::new(|root: &Path| {
            Box::new(rocket_infra::FsRequestRenderer::for_collection(
                root.to_path_buf(),
            ))
        }),
    };
    let svc = OpenApiSyncService::new(
        Arc::new(SharedRecorder(Arc::clone(&w.recorder))),
        Box::new(FsOpenApiSourceRepo),
        Arc::clone(&w.secrets) as Arc<dyn SecretStore>,
    )
    .with_local(Box::new(FsOpenApiLocalRepo))
    .with_consent(Arc::clone(&w.store) as Arc<dyn OpenApiConsentStore>)
    .with_apply(deps);
    let outcome = svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "location_changed");
    w.assert_untouched();
}

fn colliding(a: SpecLocation, b: SpecLocation) {
    assert_eq!(a.display_url(), b.display_url(), "the forms must collide");
    let w = World::new();
    FsOpenApiSourceRepo.save(w.root(), &source(a)).expect("a");
    w.svc.grant_current(w.root()).expect("grant");
    FsOpenApiSourceRepo.save(w.root(), &source(b)).expect("b");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::LocationChanged
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt");
    let outcome = rt.block_on(w.svc.check(w.root())).expect("check");
    assert_eq!(failed_code(outcome), "location_changed");
    w.assert_untouched();
}

fn gh(owner: &str, repo: &str, git_ref: &str, path: &str) -> SpecLocation {
    SpecLocation::Github {
        owner: owner.into(),
        repo: repo.into(),
        git_ref: git_ref.into(),
        path: path.into(),
    }
}

#[test]
fn a_ref_and_path_that_display_alike_are_not_the_same_location() {
    colliding(gh("o", "r", "a", "b/c.json"), gh("o", "r", "a/b", "c.json"));
}

#[test]
fn an_owner_shift_that_displays_alike_is_not_the_same_location() {
    colliding(
        gh("o", "r", "blob", "main/api.json"),
        gh("o/r", "blob", "main", "api.json"),
    );
}

#[test]
fn an_https_location_that_displays_like_a_github_one_is_not_the_same() {
    let https = SpecLocation::Https {
        url: github("r").display_url(),
    };
    colliding(github("r"), https);
}

#[test]
fn set_interval_and_set_token_need_consent_and_write_nothing_without_it() {
    for changed in [false, true] {
        let w = World::new();
        if changed {
            w.svc.grant_current(w.root()).expect("grant");
            FsOpenApiSourceRepo
                .save(w.root(), &source(github("other")))
                .expect("edit");
        }
        assert!(w.svc.set_interval(w.root(), SyncInterval::Daily).is_err());
        assert!(w.svc.set_token(w.root(), "ghp_new").is_err());
        assert_eq!(w.secrets.writes.load(Ordering::SeqCst), 0);
        assert!(FsOpenApiLocalRepo.load(w.root()).expect("load").is_none());
        // Turning checks off is always allowed.
        w.svc
            .set_interval(w.root(), SyncInterval::Off)
            .expect("off");
    }
}

#[test]
fn grant_consent_refuses_a_location_other_than_the_one_in_the_file() {
    let w = World::new();
    let shown = github("r").display_url();
    let err = w
        .svc
        .grant_consent(w.root(), &github("other").display_url())
        .expect_err("mismatch");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Missing
    );
    w.svc.grant_consent(w.root(), &shown).expect("grant");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
    // The record holds the structural key, not the display text.
    let got = w
        .store
        .get(w.root(), "src-1")
        .expect("get")
        .expect("record");
    assert_eq!(got.location, rocket_collection::location_key(&github("r")));
}

#[test]
fn grant_consent_refuses_a_location_that_does_not_round_trip() {
    for loc in [
        gh("o", "r", "a/b", "c.json"),
        gh("o/r", "blob", "main", "api.json"),
        SpecLocation::Https {
            url: github("r").display_url(),
        },
    ] {
        let w = World::new();
        FsOpenApiSourceRepo
            .save(w.root(), &source(loc.clone()))
            .expect("seed");
        let err = w
            .svc
            .grant_consent(w.root(), &loc.display_url())
            .expect_err("ambiguous");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert!(w.store.get(w.root(), "src-1").expect("get").is_none());
    }
}

#[test]
fn an_import_by_link_grants_the_typed_link_only() {
    let w = World::new();
    // A link for another file does not approve this source.
    assert!(w
        .svc
        .grant_consent_for_link(w.root(), "https://github.com/o/other/blob/main/api.json")
        .is_err());
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Missing
    );
    assert!(w
        .svc
        .grant_consent_for_link(w.root(), "not a link")
        .is_err());
    w.svc
        .grant_consent_for_link(w.root(), "https://github.com/o/r/blob/main/api.json")
        .expect("grant");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
}

#[test]
fn with_consent_set_interval_and_set_token_work_and_keep_the_approval() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    w.svc
        .set_interval(w.root(), SyncInterval::Daily)
        .expect("interval");
    w.svc.set_token(w.root(), "ghp_new").expect("token");
    assert_eq!(w.secrets.writes.load(Ordering::SeqCst), 1);
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
}

/// Answers with a valid spec, but first rewrites the source file, like an editor or a
/// git operation could while the fetch is in flight.
struct RewriteOnFetch {
    root: std::path::PathBuf,
    rewritten: OpenApiSource,
}

#[async_trait]
impl SpecFetcher for RewriteOnFetch {
    async fn fetch(&self, _: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        FsOpenApiSourceRepo
            .save(&self.root, &self.rewritten)
            .expect("rewrite");
        let path = format!(
            "{}/../rocket-import/tests/fixtures/openapi/orders-3.0.json",
            env!("CARGO_MANIFEST_DIR")
        );
        Ok(SpecFetchOutcome::Fetched {
            body: std::fs::read(path).expect("fixture"),
            etag: Some("\"new\"".into()),
            last_modified: None,
            final_url: "https://x".into(),
        })
    }
}

#[tokio::test]
async fn a_source_rewritten_during_the_fetch_saves_nothing_and_fails() {
    let mut other_id = source(github("r"));
    other_id.source_id = "src-2".into();
    for rewritten in [source(github("attacker-repo")), other_id] {
        let w = World::new();
        w.svc.grant_current(w.root()).expect("grant");
        let svc = OpenApiSyncService::new(
            Arc::new(RewriteOnFetch {
                root: w.root().to_path_buf(),
                rewritten: rewritten.clone(),
            }),
            Box::new(FsOpenApiSourceRepo),
            Arc::clone(&w.secrets) as Arc<dyn SecretStore>,
        )
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_consent(Arc::clone(&w.store) as Arc<dyn OpenApiConsentStore>);
        let outcome = svc.check(w.root()).await.expect("check");
        assert_eq!(failed_code(outcome), "location_changed");
        let on_disk = FsOpenApiSourceRepo
            .load(w.root())
            .expect("load")
            .expect("source");
        // The rewritten file is untouched: no etag, refs or pending copy were saved.
        assert_eq!(on_disk, rewritten);
        assert!(FsOpenApiSourceRepo
            .load_pending(w.root())
            .expect("pending")
            .is_none());
        let local = FsOpenApiLocalRepo
            .load(w.root())
            .expect("load")
            .expect("local");
        assert_eq!(
            local.last_status,
            Some(SyncStatus::Failed {
                code: "location_changed".into()
            })
        );
    }
}

#[test]
fn a_link_that_does_not_round_trip_through_display_text_can_still_be_granted() {
    let w = World::new();
    let loc = gh("o", "r", "feature/x", "api.json");
    FsOpenApiSourceRepo
        .save(w.root(), &source(loc.clone()))
        .expect("seed");
    let link = "https://api.github.com/repos/o/r/contents/api.json?ref=feature/x";
    assert_eq!(rocket_import::parse_spec_url(link).expect("parse"), loc);
    w.svc
        .grant_consent_for_link(w.root(), link)
        .expect("grant by structure");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
    // A link for another ref is still refused.
    let other = "https://api.github.com/repos/o/r/contents/api.json?ref=feature/y";
    let w2 = World::new();
    FsOpenApiSourceRepo
        .save(w2.root(), &source(loc))
        .expect("seed");
    assert!(w2.svc.grant_consent_for_link(w2.root(), other).is_err());
}

#[test]
fn status_without_include_token_never_reads_the_keychain() {
    let w = World::new();
    w.svc.grant_current(w.root()).expect("grant");
    let before = w.reads();
    let status = w.svc.status(w.root(), false).expect("status");
    assert_eq!(status.consent, ConsentState::Granted);
    assert_eq!(status.has_token, None);
    assert_eq!(w.reads(), before, "no keychain read");
    // Asking for the token reads it once, and only with consent.
    let status = w.svc.status(w.root(), true).expect("status");
    assert_eq!(status.has_token, Some(true));
    assert_eq!(w.reads(), before + 1);
}
