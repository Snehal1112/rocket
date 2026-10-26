//! Tests for the due list and the supervisor, on real temporary collections.

use super::*;
use crate::openapi_sync_apply::ApplyDeps;
use crate::openapi_sync_service::SyncTarget;
use async_trait::async_trait;
use rocket_collection::file_tx::{CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome};
use rocket_collection::openapi_local::{OpenApiLocalRepository, OpenApiLocalState};
use rocket_collection::openapi_source::{
    OpenApiSource, OpenApiSourceRepository, SpecLocation, SyncInterval, SyncStatus,
};
use rocket_collection::{CollectionRepository, OpenApiConsentStore};
use rocket_environment::SecretStore;
use rocket_http::spec_fetch::{SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher};
use rocket_infra::{FsFileTxFactory, FsOpenApiLocalRepo, FsOpenApiSourceRepo};
use rocket_shared::error::{DomainError, DomainResult};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tempfile::TempDir;

fn t0() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-26T12:00:00Z")
        .expect("date")
        .with_timezone(&Utc)
}

fn fixture() -> Vec<u8> {
    let path = format!(
        "{}/../rocket-import/tests/fixtures/openapi/orders-3.0.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path).expect("fixture")
}

fn changed_body() -> Vec<u8> {
    let mut v: serde_json::Value = serde_json::from_slice(&fixture()).expect("json");
    v["paths"]["/extra"] = serde_json::json!({
        "get": { "operationId": "extra", "responses": { "200": { "description": "ok" } } }
    });
    serde_json::to_vec(&v).expect("json")
}

#[derive(Clone, Copy)]
enum Answer {
    Same,
    Changed,
    NotFound,
}

struct Gate {
    started: tokio::sync::Notify,
    go: tokio::sync::Notify,
}

struct FakeFetcher {
    answer: Mutex<Answer>,
    calls: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
    gate: Mutex<Option<Arc<Gate>>>,
    panic_once: std::sync::atomic::AtomicBool,
}

impl FakeFetcher {
    fn new(answer: Answer) -> Arc<Self> {
        Arc::new(Self {
            answer: Mutex::new(answer),
            calls: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
            gate: Mutex::new(None),
            panic_once: std::sync::atomic::AtomicBool::new(false),
        })
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

struct SharedFetcher(Arc<FakeFetcher>);

#[async_trait]
impl SpecFetcher for SharedFetcher {
    async fn fetch(
        &self,
        _request: SpecFetchRequest,
    ) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        let f = &self.0;
        f.calls.fetch_add(1, Ordering::SeqCst);
        if f.panic_once.swap(false, Ordering::SeqCst) {
            panic!("fetcher panic for a test");
        }
        let now = f.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        f.max_in_flight.fetch_max(now, Ordering::SeqCst);
        let gate = f.gate.lock().expect("lock").clone();
        if let Some(gate) = gate {
            gate.started.notify_one();
            gate.go.notified().await;
        }
        // A short pause lets a second concurrent check show up if the loop allowed one.
        tokio::time::sleep(Duration::from_millis(5)).await;
        f.in_flight.fetch_sub(1, Ordering::SeqCst);
        let answer = *f.answer.lock().expect("lock");
        match answer {
            Answer::Same => Ok(SpecFetchOutcome::Fetched {
                body: fixture(),
                etag: None,
                last_modified: None,
                final_url: "https://x".into(),
            }),
            Answer::Changed => Ok(SpecFetchOutcome::Fetched {
                body: changed_body(),
                etag: None,
                last_modified: None,
                final_url: "https://x".into(),
            }),
            Answer::NotFound => Err(SpecFetchFailure::NotFound),
        }
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

/// A transaction factory whose recovery fails for the named collections and counts the tries.
struct PickyTx {
    inner: FsFileTxFactory,
    bad: Mutex<HashSet<String>>,
    bad_tries: AtomicUsize,
}

impl CollectionFileTxFactory for PickyTx {
    fn begin(&self, root: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        self.inner.begin(root)
    }
    fn recover(&self, root: &Path) -> DomainResult<RecoveryOutcome> {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if self.bad.lock().expect("lock").contains(name) {
            self.bad_tries.fetch_add(1, Ordering::SeqCst);
            return Err(DomainError::Internal("recovery blocked".into()));
        }
        self.inner.recover(root)
    }
    fn list_files(&self, root: &Path) -> DomainResult<HashSet<String>> {
        self.inner.list_files(root)
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, String)>>);

impl EventPublisher for Recorder {
    fn publish(&self, event: DomainEvent) {
        if let DomainEvent::OpenApiSyncChecked { collection, status } = event {
            self.0.lock().expect("lock").push((collection, status));
        }
    }
}

impl Recorder {
    fn all(&self) -> Vec<(String, String)> {
        self.0.lock().expect("lock").clone()
    }
}

struct World {
    ws: TempDir,
    fetcher: Arc<FakeFetcher>,
    tx: Arc<PickyTx>,
    now: Arc<Mutex<DateTime<Utc>>>,
    consent: Arc<rocket_collection::InMemoryConsentStore>,
}

impl World {
    fn new(answer: Answer, bad: &[&str]) -> Self {
        Self {
            ws: TempDir::new().expect("tempdir"),
            fetcher: FakeFetcher::new(answer),
            tx: Arc::new(PickyTx {
                inner: FsFileTxFactory::new(),
                bad: Mutex::new(bad.iter().map(|s| s.to_string()).collect()),
                bad_tries: AtomicUsize::new(0),
            }),
            now: Arc::new(Mutex::new(t0())),
            consent: Arc::new(rocket_collection::InMemoryConsentStore::new()),
        }
    }

    fn repo(&self) -> rocket_infra::FsCollectionRepo {
        rocket_infra::FsCollectionRepo::new_standalone(self.ws.path().join("collections"))
    }

    fn set_now(&self, at: DateTime<Utc>) {
        *self.now.lock().expect("lock") = at;
    }

    fn clock(&self) -> Arc<dyn Fn() -> DateTime<Utc> + Send + Sync> {
        let now = Arc::clone(&self.now);
        Arc::new(move || *now.lock().expect("lock"))
    }

    fn root(&self, name: &str) -> PathBuf {
        self.repo().collection_root_path(name).expect("root")
    }

    /// Creates a collection with a source file and, when given, a local state.
    fn seed(
        &self,
        name: &str,
        interval: Option<SyncInterval>,
        last: Option<DateTime<Utc>>,
    ) -> PathBuf {
        self.repo().create(name).expect("create");
        let root = self.root(name);
        let source = OpenApiSource {
            version: 1,
            location: SpecLocation::Https {
                url: "https://example.com/spec.json".into(),
            },
            group_by: "tags".into(),
            spec_sha256: crate::openapi_sync_service::sha256_hex(&fixture()),
            etag: None,
            last_modified: None,
            operations: Vec::new(),
            source_id: format!("src-{name}"),
            // The legacy interval stays hourly: the scheduler must ignore it.
            sync_interval: SyncInterval::Hourly,
            last_checked_at: None,
            last_status: None,
            pending_sha256: None,
            remote_refs: Vec::new(),
            remote_refs_incomplete: false,
        };
        FsOpenApiSourceRepo.save(&root, &source).expect("source");
        if let Some(interval) = interval {
            let local = OpenApiLocalState {
                sync_interval: interval,
                last_checked_at: last.map(|t| t.to_rfc3339()),
                last_status: last.map(|_| SyncStatus::Unchanged),
                ..OpenApiLocalState::default()
            };
            FsOpenApiLocalRepo.save(&root, &local).expect("local");
            // A collection with an interval was set up on this machine, so it has consent.
            self.consent
                .grant(
                    &root,
                    &source.source_id,
                    &rocket_collection::location_key(&source.location),
                )
                .expect("consent");
        }
        root
    }

    fn deps(&self) -> ApplyDeps {
        ApplyDeps {
            collections: Box::new(self.repo()),
            baselines: Box::new(rocket_infra::FsOpenApiBaselineRepo),
            locals: Box::new(FsOpenApiLocalRepo),
            tx: Arc::clone(&self.tx) as Arc<dyn CollectionFileTxFactory>,
            renderer: Arc::new(|root: &Path| {
                Box::new(rocket_infra::FsRequestRenderer::for_collection(
                    root.to_path_buf(),
                ))
            }),
        }
    }

    fn service(&self) -> OpenApiSyncService {
        OpenApiSyncService::new(
            Arc::new(SharedFetcher(Arc::clone(&self.fetcher))),
            Box::new(FsOpenApiSourceRepo),
            Arc::new(NoSecrets),
        )
        .with_local(Box::new(FsOpenApiLocalRepo))
        .with_consent(Arc::clone(&self.consent) as Arc<dyn OpenApiConsentStore>)
        .with_apply(self.deps())
        .with_clock(self.clock())
    }

    fn factory(&self) -> ServiceFactory {
        let fetcher = Arc::clone(&self.fetcher);
        let tx = Arc::clone(&self.tx);
        let ws = self.ws.path().to_path_buf();
        let clock = self.clock();
        let consent = Arc::clone(&self.consent);
        Arc::new(move || {
            let deps = ApplyDeps {
                collections: Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                    ws.join("collections"),
                )),
                baselines: Box::new(rocket_infra::FsOpenApiBaselineRepo),
                locals: Box::new(FsOpenApiLocalRepo),
                tx: Arc::clone(&tx) as Arc<dyn CollectionFileTxFactory>,
                renderer: Arc::new(|root: &Path| {
                    Box::new(rocket_infra::FsRequestRenderer::for_collection(
                        root.to_path_buf(),
                    ))
                }),
            };
            OpenApiSyncService::new(
                Arc::new(SharedFetcher(Arc::clone(&fetcher))),
                Box::new(FsOpenApiSourceRepo),
                Arc::new(NoSecrets),
            )
            .with_local(Box::new(FsOpenApiLocalRepo))
            .with_consent(Arc::clone(&consent) as Arc<dyn OpenApiConsentStore>)
            .with_apply(deps)
            .with_clock(Arc::clone(&clock))
        })
    }

    fn scheduler(&self, events: &Arc<Recorder>) -> OpenApiSyncScheduler {
        OpenApiSyncScheduler::new(
            self.factory(),
            Arc::clone(events) as Arc<dyn EventPublisher>,
            SchedulerConfig::default(),
        )
        .with_clock(self.clock())
    }

    fn local(&self, name: &str) -> OpenApiLocalState {
        FsOpenApiLocalRepo
            .load(&self.root(name))
            .expect("load")
            .unwrap_or_default()
    }
}

fn names(targets: &[SyncTarget]) -> Vec<String> {
    targets.iter().map(|t| t.collection.clone()).collect()
}

fn ago(secs: i64) -> DateTime<Utc> {
    t0() - chrono::Duration::seconds(secs)
}

/// Waits until the condition holds, with a real-time deadline of 10 s. Blocking work runs on
/// real threads, so each round also gives it a little real time.
async fn wait_until(cond: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    panic!("condition not reached");
}

/// Lets the loop run for about 100 ms of real time so an unwanted action would show.
async fn settle() {
    let end = std::time::Instant::now() + std::time::Duration::from_millis(100);
    while std::time::Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(1)).await;
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
}

// Due list.

#[test]
fn lists_only_collections_with_an_interval_that_are_due() {
    let w = World::new(Answer::Same, &[]);
    w.seed("never", None, None);
    w.seed("off", Some(SyncInterval::Off), Some(ago(10 * 3600)));
    w.seed("fresh", Some(SyncInterval::Hourly), Some(ago(10)));
    w.seed("overdue", Some(SyncInterval::Hourly), Some(ago(3 * 3600)));
    w.seed("first", Some(SyncInterval::Daily), None);
    let mut got = names(&w.service().due_targets(t0()));
    got.sort();
    assert_eq!(got, vec!["first".to_string(), "overdue".to_string()]);
}

#[test]
fn a_collection_without_local_state_is_not_listed() {
    let w = World::new(Answer::Same, &[]);
    // The source file says hourly, yet there is no local state.
    w.seed("plain", None, None);
    assert!(w.service().due_targets(t0()).is_empty());
}

#[test]
fn due_targets_skips_collections_without_consent() {
    let w = World::new(Answer::Same, &[]);
    let root = w.seed("moved", Some(SyncInterval::Hourly), Some(ago(3 * 3600)));
    assert_eq!(names(&w.service().due_targets(t0())), vec!["moved"]);
    let id = format!("src-{}", "moved");
    w.consent.revoke(&root, &id).expect("revoke");
    assert!(w.service().due_targets(t0()).is_empty());
    let status = w.service().status(&root, false).expect("status");
    assert_eq!(status.interval, SyncInterval::Off);
    assert_eq!(status.consent, rocket_collection::ConsentState::Missing);
}

#[test]
fn opt_in_root_alone_no_longer_turns_checks_on() {
    let w = World::new(Answer::Same, &[]);
    let root = w.seed("copied", Some(SyncInterval::Hourly), Some(ago(3 * 3600)));
    w.consent.revoke(&root, "src-copied").expect("revoke");
    // A state that names exactly this folder in the legacy field changes nothing.
    let mut local = w.local("copied");
    local.opt_in_root = Some(
        std::fs::canonicalize(&root)
            .expect("canonical")
            .to_string_lossy()
            .into_owned(),
    );
    FsOpenApiLocalRepo.save(&root, &local).expect("save");
    assert!(w.service().due_targets(t0()).is_empty());
    assert_eq!(
        w.service().status(&root, false).expect("status").interval,
        SyncInterval::Off
    );
}

#[test]
fn set_interval_turns_checks_on_and_off() {
    let w = World::new(Answer::Same, &[]);
    let root = w.seed("mine", None, None);
    let svc = w.service();
    // Turning checks on needs an approval first.
    assert!(svc.set_interval(&root, SyncInterval::Hourly).is_err());
    svc.grant_current(&root).expect("grant");
    svc.set_interval(&root, SyncInterval::Hourly).expect("on");
    assert_eq!(w.local("mine").opt_in_root, None);
    assert_eq!(names(&svc.due_targets(t0())), vec!["mine"]);
    assert_eq!(
        svc.status(&root, false).expect("status").interval,
        SyncInterval::Hourly
    );
    svc.set_interval(&root, SyncInterval::Off).expect("off");
    assert!(svc.due_targets(t0()).is_empty());
}

#[test]
fn without_apply_deps_nothing_is_listed() {
    let w = World::new(Answer::Same, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let svc = OpenApiSyncService::new(
        Arc::new(SharedFetcher(Arc::clone(&w.fetcher))),
        Box::new(FsOpenApiSourceRepo),
        Arc::new(NoSecrets),
    )
    .with_local(Box::new(FsOpenApiLocalRepo));
    assert!(svc.due_targets(t0()).is_empty());
}

#[test]
fn oldest_due_first() {
    let w = World::new(Answer::Same, &[]);
    w.seed("x", Some(SyncInterval::Daily), Some(ago(30 * 3600)));
    w.seed("y", Some(SyncInterval::Daily), Some(ago(90 * 3600)));
    w.seed("z", Some(SyncInterval::Daily), Some(ago(26 * 3600)));
    assert_eq!(
        names(&w.service().due_targets(t0())),
        vec!["y".to_string(), "x".to_string(), "z".to_string()]
    );
}

#[test]
fn an_unreadable_local_file_is_listed_first_not_skipped() {
    let w = World::new(Answer::Same, &[]);
    let bad = w.seed("broken", Some(SyncInterval::Hourly), None);
    w.seed("fine", Some(SyncInterval::Hourly), None);
    std::fs::write(bad.join(".rocket/openapi-local.json"), "{ not json").expect("write");
    assert_eq!(
        names(&w.service().due_targets(t0())),
        vec!["broken".to_string(), "fine".to_string()]
    );
}

// Supervisor.

#[tokio::test(start_paused = true)]
async fn first_check_waits_for_the_launch_delay() {
    let w = World::new(Answer::Same, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(29)).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 0);
    tokio::time::sleep(Duration::from_secs(2)).await;
    wait_until(|| w.fetcher.calls() == 1).await;
}

#[tokio::test(start_paused = true)]
async fn checks_run_one_at_a_time_and_at_most_the_per_tick_limit() {
    let w = World::new(Answer::Same, &[]);
    for i in 0..8 {
        w.seed(&format!("c{i}"), Some(SyncInterval::Hourly), None);
    }
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| w.fetcher.calls() >= 5).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 5);
    assert_eq!(w.fetcher.max_in_flight.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_failed_check_is_not_retried_before_its_backoff_even_after_a_restart() {
    let w = World::new(Answer::NotFound, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let first = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| w.fetcher.calls() == 1).await;
    wait_until(|| events.all().len() == 1).await;
    first.stop();
    assert_eq!(w.local("a").consecutive_failures, 1);
    // A second scheduler over the same collection sees the stored failure.
    let _second = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(600)).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 1);
    // Past the 15 minute backoff the check is allowed again.
    w.set_now(t0() + chrono::Duration::minutes(16));
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| w.fetcher.calls() == 2).await;
}

#[tokio::test(start_paused = true)]
async fn a_changed_result_publishes_the_event_with_status_changed() {
    let w = World::new(Answer::Changed, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| !events.all().is_empty()).await;
    assert_eq!(events.all(), vec![("a".to_string(), "changed".to_string())]);
}

#[tokio::test(start_paused = true)]
async fn stop_ends_the_loop_during_a_sleep() {
    let w = World::new(Answer::Same, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(10)).await;
    handle.stop();
    tokio::time::sleep(Duration::from_secs(3600)).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn stop_does_not_cancel_a_running_check() {
    let w = World::new(Answer::Changed, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    w.seed("b", Some(SyncInterval::Hourly), None);
    let gate = Arc::new(Gate {
        started: tokio::sync::Notify::new(),
        go: tokio::sync::Notify::new(),
    });
    *w.fetcher.gate.lock().expect("lock") = Some(Arc::clone(&gate));
    let events = Arc::new(Recorder::default());
    let handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    gate.started.notified().await;
    handle.stop();
    gate.go.notify_one();
    wait_until(|| events.all().len() == 1).await;
    tokio::time::sleep(Duration::from_secs(300)).await;
    settle().await;
    // The running check finished and published, and the second one never started.
    assert_eq!(events.all().len(), 1);
    assert_eq!(w.fetcher.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_check_error_for_one_collection_does_not_stop_the_next() {
    let w = World::new(Answer::Same, &["a-bad"]);
    w.seed("a-bad", Some(SyncInterval::Hourly), None);
    w.seed("b-good", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| events.all().len() == 2).await;
    let mut got = events.all();
    got.sort();
    assert_eq!(
        got,
        vec![
            ("a-bad".to_string(), "failed".to_string()),
            ("b-good".to_string(), "unchanged".to_string())
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_check_that_errors_is_delayed_in_memory_and_doubles() {
    let w = World::new(Answer::Same, &["a-bad"]);
    w.seed("a-bad", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| w.tx.bad_tries.load(Ordering::SeqCst) >= 1).await;
    // Many ticks later, still inside the first 15 minute delay, it is not tried again.
    // The tries count settle too, so the first check's own try is the baseline.
    let after_first = w.tx.bad_tries.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(600)).await;
    settle().await;
    assert_eq!(w.tx.bad_tries.load(Ordering::SeqCst), after_first);
    // Past 15 minutes it runs again.
    w.set_now(t0() + chrono::Duration::minutes(16));
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| w.tx.bad_tries.load(Ordering::SeqCst) > after_first).await;
    let second = w.tx.bad_tries.load(Ordering::SeqCst);
    // The second delay is 30 minutes, so 20 minutes later it is still held back.
    w.set_now(t0() + chrono::Duration::minutes(36));
    tokio::time::sleep(Duration::from_secs(300)).await;
    settle().await;
    assert_eq!(w.tx.bad_tries.load(Ordering::SeqCst), second);
    // Every failure published `failed`.
    assert!(events.all().iter().all(|(_, s)| s == "failed"));
}

#[tokio::test(start_paused = true)]
async fn after_a_long_pause_checks_run_in_batches_not_all_at_once() {
    let w = World::new(Answer::Same, &[]);
    for i in 0..12 {
        w.seed(&format!("c{i:02}"), Some(SyncInterval::Hourly), Some(t0()));
    }
    // The clock is one month ahead, so all twelve are overdue.
    w.set_now(t0() + chrono::Duration::days(30));
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| w.fetcher.calls() >= 5).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 5);
    tokio::time::sleep(Duration::from_secs(60)).await;
    wait_until(|| w.fetcher.calls() >= 10).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 10);
    tokio::time::sleep(Duration::from_secs(60)).await;
    wait_until(|| w.fetcher.calls() >= 12).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 12);
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_local_file_gets_failed_and_is_then_held_back() {
    let w = World::new(Answer::Same, &[]);
    let bad = w.seed("broken", Some(SyncInterval::Hourly), None);
    std::fs::write(bad.join(".rocket/openapi-local.json"), "{ not json").expect("write");
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| events.all().len() == 1).await;
    assert_eq!(
        events.all(),
        vec![("broken".to_string(), "failed".to_string())]
    );
    tokio::time::sleep(Duration::from_secs(600)).await;
    settle().await;
    assert_eq!(events.all().len(), 1);
    assert_eq!(w.fetcher.calls(), 0);
    w.set_now(t0() + chrono::Duration::minutes(16));
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| events.all().len() == 2).await;
}

#[tokio::test(start_paused = true)]
async fn a_panic_in_a_check_fails_that_collection_and_the_next_still_runs() {
    let w = World::new(Answer::Same, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    w.seed("b", Some(SyncInterval::Hourly), None);
    w.fetcher.panic_once.store(true, Ordering::SeqCst);
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| events.all().len() == 2).await;
    let mut statuses: Vec<String> = events.all().into_iter().map(|(_, s)| s).collect();
    statuses.sort();
    assert_eq!(
        statuses,
        vec!["failed".to_string(), "unchanged".to_string()]
    );
    // The loop is still alive: a later collection is checked on a following tick.
    w.seed("c", Some(SyncInterval::Hourly), None);
    tokio::time::sleep(Duration::from_secs(61)).await;
    wait_until(|| events.all().len() == 3).await;
}

#[tokio::test(start_paused = true)]
async fn stop_during_the_sleep_between_ticks_ends_the_loop() {
    let w = World::new(Answer::Same, &[]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let handle = w.scheduler(&events).start();
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| events.all().len() == 1).await;
    handle.stop();
    w.seed("late", Some(SyncInterval::Hourly), None);
    w.set_now(t0() + chrono::Duration::minutes(5));
    tokio::time::sleep(Duration::from_secs(300)).await;
    settle().await;
    assert_eq!(w.fetcher.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_success_clears_the_error_backoff() {
    let w = World::new(Answer::Same, &["a"]);
    w.seed("a", Some(SyncInterval::Hourly), None);
    let events = Arc::new(Recorder::default());
    let _handle = w.scheduler(&events).start();
    let tries = || w.tx.bad_tries.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(31)).await;
    wait_until(|| tries() == 1).await;
    // Second error after 15 minutes, so the delay is now 30 minutes.
    w.set_now(t0() + chrono::Duration::minutes(16));
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| tries() == 2).await;
    // The collection heals and a check past the 30 minute delay succeeds.
    w.tx.bad.lock().expect("lock").clear();
    w.set_now(t0() + chrono::Duration::minutes(50));
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| events.all().last().is_some_and(|(_, s)| s == "unchanged")).await;
    // It breaks again once it is due. The delay starts over at 15 minutes.
    w.tx.bad.lock().expect("lock").insert("a".to_string());
    let broken_at = t0() + chrono::Duration::minutes(50 + 70);
    w.set_now(broken_at);
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| tries() == 3).await;
    w.set_now(broken_at + chrono::Duration::minutes(16));
    tokio::time::sleep(Duration::from_secs(120)).await;
    wait_until(|| tries() == 4).await;
}
