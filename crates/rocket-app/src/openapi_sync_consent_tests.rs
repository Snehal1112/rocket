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
    w.svc.grant_consent(w.root()).expect("grant");
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
    let status = w.svc.status(w.root()).expect("status");
    assert_eq!(status.consent, ConsentState::LocationChanged);
    assert!(!status.has_token);
    assert_eq!(w.reads(), 0);
}

#[tokio::test]
async fn check_with_consent_sends_the_token_to_the_approved_location() {
    let w = World::new();
    w.svc.grant_consent(w.root()).expect("grant");
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
    w.svc.grant_consent(w.root()).expect("grant");
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
    let status = w.svc.status(copy.path()).expect("status");
    assert!(!status.has_token);
    assert_eq!(w.reads(), 0);
    // Approving the copy does not touch the original, and revoking it leaves the original.
    w.svc.grant_consent(copy.path()).expect("grant copy");
    w.store.revoke(copy.path(), "src-1").expect("revoke");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
}

#[test]
fn set_interval_grants_consent_for_the_current_location() {
    let w = World::new();
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Missing
    );
    w.svc
        .set_interval(w.root(), SyncInterval::Daily)
        .expect("interval");
    assert_eq!(
        w.svc.consent_state(w.root()).expect("state"),
        ConsentState::Granted
    );
    let got = w
        .store
        .get(w.root(), "src-1")
        .expect("get")
        .expect("record");
    assert_eq!(got.location, github("r").display_url());
    // Turning it off does not approve anything new and keeps the record.
    w.svc
        .set_interval(w.root(), SyncInterval::Off)
        .expect("off");
    assert!(w.store.get(w.root(), "src-1").expect("get").is_some());
}

#[test]
fn set_token_grants_consent_for_the_current_location() {
    let w = World::new();
    w.svc.set_token(w.root(), "ghp_new").expect("token");
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
    assert!(!svc.status(w.root()).expect("status").has_token);
    w.assert_untouched();
}

#[tokio::test]
async fn revoking_consent_stops_the_next_check() {
    let w = World::new();
    w.svc.grant_consent(w.root()).expect("grant");
    w.store.revoke(w.root(), "src-1").expect("revoke");
    let outcome = w.svc.check(w.root()).await.expect("check");
    assert_eq!(failed_code(outcome), "consent_required");
    w.assert_untouched();
}
