//! In-memory test doubles shared by the Collection Runner tests.
//!
//! `execution_service.rs` predates this module and keeps its own doubles — do
//! not migrate those, their byte-for-byte stability is what proves the
//! phase-split refactor changed no behaviour.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    Request as CollectionRequest,
};
use rocket_environment::{
    Environment, EnvironmentRepository, EnvironmentRepositoryFactory, ExternalSecretRef,
    SecretManagerConnection, SecretManagerRepository, SecretStore, VaultSecretFetcher,
};
use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

// ---------------------------------------------------------------------------
// Collection repo
// ---------------------------------------------------------------------------

/// Collection repo backed by one in-memory `Collection`.
pub struct InMemoryCollectionRepo {
    collection: Collection,
}

impl InMemoryCollectionRepo {
    pub fn new(collection: Collection) -> Arc<Self> {
        Arc::new(Self { collection })
    }
}

impl CollectionRepository for InMemoryCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        Ok(vec![])
    }
    fn get(&self, name: &str) -> DomainResult<Collection> {
        if name == self.collection.name {
            Ok(self.collection.clone())
        } else {
            Err(DomainError::NotFound(name.into()))
        }
    }
    fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
        self.get(name)
    }
    fn create(&self, _: &str) -> DomainResult<Collection> {
        Err(DomainError::NotFound("stub".into()))
    }
    fn delete(&self, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
        Err(DomainError::NotFound("stub".into()))
    }
    fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> {
        Ok(path.to_string())
    }
    fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
    fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
        Ok(())
    }
    fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
        Ok(self.collection.settings.clone())
    }
    fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
        Ok(())
    }
    fn get_folder_chain_variables(
        &self,
        _: &str,
        _: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        Ok(vec![])
    }
    fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
        Ok(vec![])
    }
    fn save_folder_variables(
        &self,
        _: &str,
        _: &str,
        _: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        Ok(())
    }
    fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
        Ok(vec![])
    }
    fn save_request_variables(
        &self,
        _: &str,
        _: &str,
        _: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        Ok(())
    }
}

/// Hands one `Arc<InMemoryCollectionRepo>` to a service expecting a `Box<dyn>`.
pub struct SharedCollectionRepo(pub Arc<InMemoryCollectionRepo>);

impl CollectionRepository for SharedCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.0.list()
    }
    fn get(&self, n: &str) -> DomainResult<Collection> {
        self.0.get(n)
    }
    fn get_summaries(&self, n: &str) -> DomainResult<Collection> {
        self.0.get_summaries(n)
    }
    fn create(&self, n: &str) -> DomainResult<Collection> {
        self.0.create(n)
    }
    fn delete(&self, n: &str) -> DomainResult<()> {
        self.0.delete(n)
    }
    fn rename(&self, a: &str, b: &str) -> DomainResult<()> {
        self.0.rename(a, b)
    }
    fn get_request(&self, a: &str, b: &str) -> DomainResult<CollectionRequest> {
        self.0.get_request(a, b)
    }
    fn save_request(&self, a: &str, b: &str, c: &CollectionRequest) -> DomainResult<String> {
        self.0.save_request(a, b, c)
    }
    fn rename_request(&self, a: &str, b: &str, c: &str) -> DomainResult<()> {
        self.0.rename_request(a, b, c)
    }
    fn delete_request(&self, a: &str, b: &str) -> DomainResult<()> {
        self.0.delete_request(a, b)
    }
    fn create_folder(&self, a: &str, b: &str) -> DomainResult<()> {
        self.0.create_folder(a, b)
    }
    fn delete_folder(&self, a: &str, b: &str) -> DomainResult<()> {
        self.0.delete_folder(a, b)
    }
    fn move_item(&self, a: &str, b: &str, c: &str, d: &str) -> DomainResult<()> {
        self.0.move_item(a, b, c, d)
    }
    fn reorder_items(&self, a: &str, b: &str, c: &[String]) -> DomainResult<()> {
        self.0.reorder_items(a, b, c)
    }
    fn get_settings(&self, n: &str) -> DomainResult<CollectionSettings> {
        self.0.get_settings(n)
    }
    fn save_settings(&self, n: &str, s: &CollectionSettings) -> DomainResult<()> {
        self.0.save_settings(n, s)
    }
    fn get_folder_chain_variables(
        &self,
        a: &str,
        b: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.0.get_folder_chain_variables(a, b)
    }
    fn get_folder_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
        self.0.get_folder_variables(a, b)
    }
    fn save_folder_variables(
        &self,
        a: &str,
        b: &str,
        c: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.0.save_folder_variables(a, b, c)
    }
    fn get_request_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
        self.0.get_request_variables(a, b)
    }
    fn save_request_variables(
        &self,
        a: &str,
        b: &str,
        c: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.0.save_request_variables(a, b, c)
    }
}

// ---------------------------------------------------------------------------
// Environment, history, cookies
// ---------------------------------------------------------------------------

/// Environment repo with no environments.
pub struct NullEnvRepo;

impl EnvironmentRepository for NullEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        Ok(vec![])
    }
    fn get(&self, name: &str) -> DomainResult<Environment> {
        Err(DomainError::NotFound(name.into()))
    }
    fn save(&self, _: &Environment) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// History repo that records everything saved to it.
pub struct InMemoryHistoryRepo {
    pub entries: Mutex<Vec<HistoryEntry>>,
}

impl InMemoryHistoryRepo {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(Vec::new()),
        })
    }
    pub fn saved_count(&self) -> usize {
        self.entries.lock().expect("lock").len()
    }
}

impl HistoryRepository for InMemoryHistoryRepo {
    fn list(&self, _: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        Ok(self.entries.lock().expect("lock").clone())
    }
    fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        self.entries
            .lock()
            .expect("lock")
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(id.into()))
    }
    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
        self.entries.lock().expect("lock").push(entry.clone());
        Ok(())
    }
    fn clear(&self) -> DomainResult<()> {
        self.entries.lock().expect("lock").clear();
        Ok(())
    }
    fn search(&self, _: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
        Ok(self.entries.lock().expect("lock").clone())
    }
}

/// Hands one `Arc<InMemoryHistoryRepo>` to a service expecting a `Box<dyn>`.
pub struct SharedHistoryRepo(pub Arc<InMemoryHistoryRepo>);

impl HistoryRepository for SharedHistoryRepo {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        self.0.list(limit)
    }
    fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        self.0.get(id)
    }
    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
        self.0.save(entry)
    }
    fn clear(&self) -> DomainResult<()> {
        self.0.clear()
    }
    fn search(&self, f: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
        self.0.search(f)
    }
}

/// No connections configured — every lookup misses. Used by every
/// `CollectionRunnerService` test that doesn't exercise RocketVault
/// resolution itself.
pub struct EmptySecretManagerRepo;

impl SecretManagerRepository for EmptySecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        Ok(vec![])
    }
    fn get(&self, _id: &str) -> DomainResult<Option<SecretManagerConnection>> {
        Ok(None)
    }
    fn save(&self, _connection: &SecretManagerConnection) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _id: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Cookie repo that stores nothing.
pub struct NullCookieRepo;

impl CookieRepository for NullCookieRepo {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
        Ok(vec![])
    }
    fn get_by_domain(&self, _: &str) -> DomainResult<Option<CookieJar>> {
        Ok(None)
    }
    fn save(&self, _: &CookieJar) -> DomainResult<()> {
        Ok(())
    }
    fn clear(&self) -> DomainResult<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

/// Executor that records every URL it was asked to send and answers with a
/// per-URL status code (default 200). A URL whose status is registered as 0
/// fails with a transport error instead.
pub struct RecordingExecutor {
    sent: Mutex<Vec<String>>,
    statuses: Mutex<HashMap<String, u16>>,
}

impl RecordingExecutor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            statuses: Mutex::new(HashMap::new()),
        })
    }
    /// Registers a status for any URL containing `url_substring`. A status of
    /// `0` makes the send fail with a transport error instead.
    /// Takes `&self` so it can be called straight through the `Arc`.
    pub fn set_status(&self, url_substring: &str, status: u16) {
        self.statuses
            .lock()
            .expect("lock")
            .insert(url_substring.to_string(), status);
    }
    pub fn sent_urls(&self) -> Vec<String> {
        self.sent.lock().expect("lock").clone()
    }
}

#[async_trait]
impl HttpExecutor for RecordingExecutor {
    async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
        self.sent.lock().expect("lock").push(req.url.clone());
        let status = self
            .statuses
            .lock()
            .expect("lock")
            .iter()
            .find(|(fragment, _)| req.url.contains(fragment.as_str()))
            .map(|(_, status)| *status)
            .unwrap_or(200);
        if status == 0 {
            return Err(DomainError::Http("connection refused".into()));
        }
        Ok(HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![],
            body: "{}".into(),
            duration_ms: 1,
            ttfb_ms: 1,
            size_bytes: 2,
        })
    }
}

/// Hands one `Arc<RecordingExecutor>` to a service expecting `Arc<dyn>`.
pub struct SharedExecutor(pub Arc<RecordingExecutor>);

#[async_trait]
impl HttpExecutor for SharedExecutor {
    async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
        self.0.execute(req).await
    }
}

// ---------------------------------------------------------------------------
// Script engine
// ---------------------------------------------------------------------------

/// Script engine that returns a canned `ScriptResult` per (request name, phase)
/// and records the execution mode every call carried.
pub struct ProgrammableEngine {
    results: Mutex<HashMap<String, ScriptResult>>,
    modes: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
    runtime_reads: Mutex<Vec<HashMap<String, String>>>,
}

impl ProgrammableEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            results: Mutex::new(HashMap::new()),
            modes: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            runtime_reads: Mutex::new(Vec::new()),
        })
    }
    /// Registers a canned result. `phase` is `"before-request"`,
    /// `"after-response"`, or `"tests"`. Takes `&self` so it can be called
    /// straight through the `Arc`.
    pub fn on(&self, request_name: &str, phase: &str, result: ScriptResult) {
        self.results
            .lock()
            .expect("lock")
            .insert(format!("{request_name}|{phase}"), result);
    }
    pub fn modes(&self) -> Vec<String> {
        self.modes.lock().expect("lock").clone()
    }
    /// Keys of the form `"<request name>|<phase>"`, in call order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock").clone()
    }
    /// The runtime variable scope each call saw, in call order.
    pub fn runtime_reads(&self) -> Vec<HashMap<String, String>> {
        self.runtime_reads.lock().expect("lock").clone()
    }
}

#[async_trait]
impl ScriptEngine for ProgrammableEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        let key = format!("{}|{}", ctx.request_name, ctx.phase.as_str());
        self.modes
            .lock()
            .expect("lock")
            .push(ctx.execution_mode.clone());
        self.calls.lock().expect("lock").push(key.clone());
        self.runtime_reads
            .lock()
            .expect("lock")
            .push(ctx.variables.runtime.clone());
        Ok(self
            .results
            .lock()
            .expect("lock")
            .get(&key)
            .cloned()
            .unwrap_or_default())
    }
}

/// Hands one `Arc<ProgrammableEngine>` to a service expecting a `Box<dyn>`.
pub struct SharedEngine(pub Arc<ProgrammableEngine>);

#[async_trait]
impl ScriptEngine for SharedEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        self.0.execute(ctx).await
    }
}

// ---------------------------------------------------------------------------
// Event publisher
// ---------------------------------------------------------------------------

/// Publisher that records every event it is handed.
pub struct RecordingPublisher {
    events: Mutex<Vec<DomainEvent>>,
}

impl RecordingPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
        })
    }
    pub fn events(&self) -> Vec<DomainEvent> {
        self.events.lock().expect("lock").clone()
    }
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: DomainEvent) {
        self.events.lock().expect("lock").push(event);
    }
}

/// Hands one `Arc<RecordingPublisher>` to a service expecting a `Box<dyn>`.
pub struct SharedPublisher(pub Arc<RecordingPublisher>);

impl EventPublisher for SharedPublisher {
    fn publish(&self, event: DomainEvent) {
        self.0.publish(event);
    }
}

// ---------------------------------------------------------------------------
// External Secrets: environment, vault fetcher, secret manager repo, store
// ---------------------------------------------------------------------------

/// Environment repo that always returns one fixed `Environment`, regardless
/// of the name asked for. Enough for a test that only needs one environment
/// with a known `external_secrets` binding — mirrors `NullEnvRepo` above but
/// answers instead of always erroring.
pub struct StaticEnvRepo(pub Environment);

impl EnvironmentRepository for StaticEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        Ok(vec![self.0.clone()])
    }
    fn get(&self, _name: &str) -> DomainResult<Environment> {
        Ok(self.0.clone())
    }
    fn save(&self, _: &Environment) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Returns the same pre-loaded `Environment` for any collection name asked —
/// used to prove a caller routes an environment lookup through
/// `regular_env_repo(collection)` rather than the global `env_repo`. Pair
/// with an empty/erroring global `env_repo` (e.g. `NullEnvRepo`) so a test
/// using this factory fails if the caller's collection-routing regresses.
pub struct StaticCollectionEnvRepoFactory(pub Environment);

impl EnvironmentRepositoryFactory for StaticCollectionEnvRepoFactory {
    fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(StaticEnvRepo(self.0.clone()))
    }
}

/// Secret Manager connection repo that always answers one fixed connection.
pub struct FakeSecretManagerRepo(pub SecretManagerConnection);

impl SecretManagerRepository for FakeSecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        Ok(vec![self.0.clone()])
    }
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
        Ok(if id == self.0.id {
            Some(self.0.clone())
        } else {
            None
        })
    }
    fn save(&self, _: &SecretManagerConnection) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Secret store that always answers one fixed client secret, regardless of
/// scope/key. Enough for a test that only needs the vault-connection client
/// secret lookup to succeed.
pub struct FakeSecretStore(pub String);

impl SecretStore for FakeSecretStore {
    fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
        Ok(Some(self.0.clone()))
    }
    fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Vault fetcher that answers one canned value per secret id and counts how
/// many times `get_secret_value` was called. That count is the assertion
/// this plan's test makes to enforce spec acceptance criterion 7 (one
/// resolution pass per run, not one per request).
pub struct FakeVaultSecretFetcher {
    values: HashMap<String, String>, // secret_id -> value
    get_secret_value_calls: AtomicUsize,
}

impl FakeVaultSecretFetcher {
    pub fn new(values: HashMap<String, String>) -> Arc<Self> {
        Arc::new(Self {
            values,
            get_secret_value_calls: AtomicUsize::new(0),
        })
    }

    pub fn call_count(&self) -> usize {
        self.get_secret_value_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl VaultSecretFetcher for FakeVaultSecretFetcher {
    async fn list_secrets(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        Ok(Vec::new())
    }

    async fn get_secret_value(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        self.get_secret_value_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.values.get(secret_id).cloned())
    }

    async fn test_connection(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::HttpMethod;

    #[tokio::test]
    async fn recording_executor_reports_registered_status_and_records_urls() {
        let executor = RecordingExecutor::new();
        executor.set_status("/boom", 500);

        let ok = executor
            .execute(&HttpRequest::new(HttpMethod::Get, "https://api.test/ok"))
            .await
            .expect("send");
        let boom = executor
            .execute(&HttpRequest::new(HttpMethod::Get, "https://api.test/boom"))
            .await
            .expect("send");

        assert_eq!(ok.status, 200);
        assert_eq!(boom.status, 500);
        assert_eq!(executor.sent_urls().len(), 2);
    }
}
