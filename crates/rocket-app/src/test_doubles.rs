//! In-memory test doubles shared by the Collection Runner and ACP tests.
//!
//! `execution_service.rs` predates this module and keeps its own doubles — do
//! not migrate those, their byte-for-byte stability is what proves the
//! phase-split refactor changed no behaviour.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::callback_listener::{CallbackEndpoint, CallbackListener, ReceivedCall};
use async_trait::async_trait;
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    FolderSettings, Request as CollectionRequest, RequestScriptPhase,
};
use rocket_environment::{
    Environment, EnvironmentRepository, EnvironmentRepositoryFactory, ExternalSecretRef,
    SecretManagerConnection, SecretManagerRepository, SecretStore, VaultCertificateMaterial,
    VaultCertificateSummary, VaultSecretFetcher,
};
use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Collection repo
// ---------------------------------------------------------------------------

/// Collection repo backed by one in-memory `Collection`.
pub struct InMemoryCollectionRepo {
    collection: Collection,
    folder_chain: Vec<FolderSettings>,
    root: Option<std::path::PathBuf>,
    folder_chain_reads: AtomicUsize,
}

impl InMemoryCollectionRepo {
    pub fn new(collection: Collection) -> Arc<Self> {
        Self::with_folder_chain(collection, Vec::new(), None)
    }

    /// Every request sits below `folder_chain`, outermost folder first. `root`
    /// is the collection directory scripts may `require()` from.
    pub fn with_folder_chain(
        collection: Collection,
        folder_chain: Vec<FolderSettings>,
        root: Option<std::path::PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            collection,
            folder_chain,
            root,
            folder_chain_reads: AtomicUsize::new(0),
        })
    }

    /// How many times `get_folder_chain_settings` was called.
    pub fn folder_chain_reads(&self) -> usize {
        self.folder_chain_reads.load(Ordering::SeqCst)
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
    fn collection_root_path(&self, _: &str) -> DomainResult<std::path::PathBuf> {
        self.root
            .clone()
            .ok_or_else(|| DomainError::Internal("collection root path is not available".into()))
    }
    fn get_folder_chain_settings(&self, _: &str, _: &str) -> DomainResult<Vec<FolderSettings>> {
        self.folder_chain_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.folder_chain.clone())
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
    fn save_request_script(
        &self,
        _: &str,
        _: &str,
        _: rocket_collection::RequestScriptPhase,
        _: String,
    ) -> DomainResult<()> {
        Ok(())
    }
}

/// Hands one shared `Arc` collection repo to a service expecting a `Box<dyn>`.
/// Defaults to `InMemoryCollectionRepo`, the Runner tests' original use.
pub struct SharedCollectionRepo<T: CollectionRepository = InMemoryCollectionRepo>(pub Arc<T>);

impl<T: CollectionRepository> CollectionRepository for SharedCollectionRepo<T> {
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
    fn collection_root_path(&self, n: &str) -> DomainResult<std::path::PathBuf> {
        self.0.collection_root_path(n)
    }
    fn get_folder_chain_settings(&self, a: &str, b: &str) -> DomainResult<Vec<FolderSettings>> {
        self.0.get_folder_chain_settings(a, b)
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
    fn save_request_script(
        &self,
        a: &str,
        b: &str,
        c: rocket_collection::RequestScriptPhase,
        d: String,
    ) -> DomainResult<()> {
        self.0.save_request_script(a, b, c, d)
    }
}

/// Collection repo with mutable per-collection settings, requests, and
/// summary trees, plus a record of every `save_request_script` call. Built
/// for the ACP tests (`McpToolService`, `AcpSessionService`), which need to
/// toggle `agent_autonomy_enabled` mid-test. Settings default to
/// `CollectionSettings::default()` (autonomy off) for any unconfigured
/// collection, matching the real repos' "missing settings file" fallback.
#[derive(Default)]
pub struct ConfigurableCollectionRepo {
    settings: Mutex<HashMap<String, CollectionSettings>>,
    settings_error_for: Mutex<Option<String>>,
    fail_request_vars: Mutex<bool>,
    fail_request_read: Mutex<bool>,
    request_vars: Mutex<Vec<CollectionVariable>>,
    requests: Mutex<HashMap<(String, String), CollectionRequest>>,
    summaries: Mutex<HashMap<String, Collection>>,
    saved_scripts: Mutex<Vec<(String, String, RequestScriptPhase, String)>>,
    folder_settings: Mutex<HashMap<(String, String), rocket_collection::FolderSettings>>,
}

impl ConfigurableCollectionRepo {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Shorthand for a repo with one collection's autonomy flag preset.
    pub fn with_autonomy_enabled(collection: &str, enabled: bool) -> Arc<Self> {
        let repo = Self::new();
        repo.set_autonomy(collection, enabled);
        repo
    }

    pub fn set_autonomy(&self, collection: &str, enabled: bool) {
        let settings = CollectionSettings {
            agent_autonomy_enabled: enabled,
            ..Default::default()
        };
        self.settings
            .lock()
            .expect("lock settings")
            .insert(collection.to_string(), settings);
    }

    /// Replaces one collection's whole settings, for tests that need auth,
    /// headers or variables and not only the run switch.
    /// Sets the request-level variables every request of this repo returns.
    pub fn set_request_variables(&self, vars: Vec<CollectionVariable>) {
        *self.request_vars.lock().expect("lock request_vars") = vars;
    }

    pub fn set_settings(&self, collection: &str, settings: CollectionSettings) {
        self.settings
            .lock()
            .expect("lock settings")
            .insert(collection.to_string(), settings);
    }

    /// Makes `get_settings(collection)` fail with `DomainError::Internal`.
    pub fn fail_settings_for(&self, collection: &str) {
        *self
            .settings_error_for
            .lock()
            .expect("lock settings_error_for") = Some(collection.to_string());
    }

    /// Makes `get_request_variables` fail with `DomainError::Internal`.
    pub fn fail_request_variables(&self) {
        *self.fail_request_vars.lock().expect("lock fail_request_vars") = true;
    }

    /// Makes `get_request` fail with `DomainError::Internal` instead of `NotFound`.
    pub fn fail_request_reads(&self) {
        *self.fail_request_read.lock().expect("lock fail_request_read") = true;
    }

    pub fn with_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: rocket_collection::FolderSettings,
    ) {
        self.folder_settings
            .lock()
            .expect("lock folder_settings")
            .insert((collection.to_string(), folder_path.to_string()), settings);
    }

    pub fn with_request(&self, collection: &str, path: &str, request: CollectionRequest) {
        self.requests
            .lock()
            .expect("lock requests")
            .insert((collection.to_string(), path.to_string()), request);
    }

    pub fn with_summaries(&self, collection: &str, tree: Collection) {
        self.summaries
            .lock()
            .expect("lock summaries")
            .insert(collection.to_string(), tree);
    }
}

impl CollectionRepository for ConfigurableCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        names.extend(self.settings.lock().expect("lock settings").keys().cloned());
        names.extend(self.summaries.lock().expect("lock summaries").keys().cloned());
        let requests = self.requests.lock().expect("lock requests");
        names.extend(requests.keys().map(|(collection, _)| collection.clone()));
        Ok(names
            .into_iter()
            .map(|name| {
                let count = requests
                    .keys()
                    .filter(|(collection, _)| *collection == name)
                    .count();
                CollectionSummary::new("", &name, "", count, None)
            })
            .collect())
    }
    fn get(&self, name: &str) -> DomainResult<Collection> {
        self.summaries
            .lock()
            .expect("lock summaries")
            .get(name)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(name.into()))
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
    fn get_request(&self, collection: &str, path: &str) -> DomainResult<CollectionRequest> {
        if *self.fail_request_read.lock().expect("lock fail_request_read") {
            return Err(DomainError::Internal("request read failed".into()));
        }
        self.requests
            .lock()
            .expect("lock requests")
            .get(&(collection.to_string(), path.to_string()))
            .cloned()
            .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
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
    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
        let failing = self
            .settings_error_for
            .lock()
            .expect("lock settings_error_for")
            .as_deref()
            == Some(name);
        if failing {
            return Err(DomainError::Internal("settings read failed".into()));
        }
        Ok(self
            .settings
            .lock()
            .expect("lock settings")
            .get(name)
            .cloned()
            .unwrap_or_default())
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
        if *self.fail_request_vars.lock().expect("lock fail_request_vars") {
            return Err(DomainError::Internal("request variables read failed".into()));
        }
        Ok(self.request_vars.lock().expect("lock request_vars").clone())
    }
    fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<rocket_collection::FolderSettings> {
        self.folder_settings
            .lock()
            .expect("lock folder_settings")
            .get(&(collection.to_string(), folder_path.to_string()))
            .cloned()
            .ok_or_else(|| DomainError::NotFound(folder_path.to_string()))
    }
    fn save_request_variables(
        &self,
        _: &str,
        _: &str,
        _: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        Ok(())
    }
    fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.saved_scripts
            .lock()
            .expect("lock saved_scripts")
            .push((
                collection.to_string(),
                request_path.to_string(),
                phase,
                body,
            ));
        Ok(())
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
    sent_auth: Mutex<Vec<rocket_shared::types::Auth>>,
    sent_bodies: Mutex<Vec<Option<String>>>,
    statuses: Mutex<HashMap<String, u16>>,
    bodies: Mutex<HashMap<String, String>>,
    error_messages: Mutex<HashMap<String, String>>,
}

impl RecordingExecutor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            sent_auth: Mutex::new(Vec::new()),
            sent_bodies: Mutex::new(Vec::new()),
            statuses: Mutex::new(HashMap::new()),
            bodies: Mutex::new(HashMap::new()),
            error_messages: Mutex::new(HashMap::new()),
        })
    }
    /// Registers a response body for any URL containing `url_substring`.
    pub fn set_body(&self, url_substring: &str, body: &str) {
        self.bodies
            .lock()
            .expect("lock")
            .insert(url_substring.to_string(), body.to_string());
    }
    /// The request body content of every send, in order.
    pub fn sent_bodies(&self) -> Vec<Option<String>> {
        self.sent_bodies.lock().expect("lock").clone()
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
    /// Like `set_status(url_substring, 0)`, but the resulting transport
    /// error carries `message` instead of the default "connection refused" —
    /// lets a test build an error whose text embeds something secret-shaped
    /// (e.g. a resolved URL with an API key in it), to verify a caller
    /// sanitizes it before it reaches an agent.
    pub fn set_error(&self, url_substring: &str, message: &str) {
        self.statuses
            .lock()
            .expect("lock")
            .insert(url_substring.to_string(), 0);
        self.error_messages
            .lock()
            .expect("lock")
            .insert(url_substring.to_string(), message.to_string());
    }
    pub fn sent_urls(&self) -> Vec<String> {
        self.sent.lock().expect("lock").clone()
    }
    pub fn sent_auths(&self) -> Vec<rocket_shared::types::Auth> {
        self.sent_auth.lock().expect("lock").clone()
    }
}

#[async_trait]
impl HttpExecutor for RecordingExecutor {
    async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
        self.sent.lock().expect("lock").push(req.url.clone());
        self.sent_auth.lock().expect("lock").push(req.auth.clone());
        self.sent_bodies
            .lock()
            .expect("lock")
            .push(req.body.as_ref().and_then(|b| b.content.clone()));
        let status = self
            .statuses
            .lock()
            .expect("lock")
            .iter()
            .find(|(fragment, _)| req.url.contains(fragment.as_str()))
            .map(|(_, status)| *status)
            .unwrap_or(200);
        if status == 0 {
            let message = self
                .error_messages
                .lock()
                .expect("lock")
                .iter()
                .find(|(fragment, _)| req.url.contains(fragment.as_str()))
                .map(|(_, message)| message.clone())
                .unwrap_or_else(|| "connection refused".to_string());
            return Err(DomainError::Http(message));
        }
        let body = self
            .bodies
            .lock()
            .expect("lock")
            .iter()
            .find(|(fragment, _)| req.url.contains(fragment.as_str()))
            .map(|(_, body)| body.clone())
            .unwrap_or_else(|| "{}".to_string());
        Ok(HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![],
            size_bytes: body.len(),
            body,
            duration_ms: 1,
            ttfb_ms: 1,
            ..Default::default()
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

/// What `FakeCertificateFetcher` answers for one certificate name.
#[derive(Clone, Copy)]
pub enum FakeExport {
    /// Exports fixed material in the asked format.
    Ok,
    /// Fails with `InvalidInput(message)`, as RocketVault's mapped errors do.
    Fail(&'static str),
}

pub const FAKE_CERT_PEM: &[u8] =
    b"-----BEGIN CERTIFICATE-----\nZmFrZS1jZXJ0\n-----END CERTIFICATE-----\n";
pub const FAKE_KEY_PEM: &[u8] =
    b"-----BEGIN PRIVATE KEY-----\nc2VjcmV0LWtleQ==\n-----END PRIVATE KEY-----\n";
pub const FAKE_BUNDLE: &[u8] = &[0x30, 0x82, 0x01, 0x02, 0x03];
pub const FAKE_PASSWORD: &str = "one-time-pass-123";

/// Vault fetcher that exports certificates from a script and records every export as
/// `vault/name/format`. A name with no script entry is "not found". `list_certificates` lists
/// the scripted names, with `exportable` true for `Ok`.
pub struct FakeCertificateFetcher {
    exports: HashMap<String, FakeExport>,
    calls: Mutex<Vec<String>>,
}

impl FakeCertificateFetcher {
    pub fn new(exports: &[(&str, FakeExport)]) -> Arc<Self> {
        Arc::new(Self {
            exports: exports
                .iter()
                .map(|(name, export)| (name.to_string(), *export))
                .collect(),
            calls: Mutex::new(Vec::new()),
        })
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock").clone()
    }
}

#[async_trait]
impl VaultSecretFetcher for FakeCertificateFetcher {
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
        _secret_id: &str,
    ) -> DomainResult<Option<String>> {
        Ok(None)
    }

    async fn test_connection(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        Ok(())
    }

    async fn list_certificates(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        let mut listed: Vec<VaultCertificateSummary> = self
            .exports
            .iter()
            .map(|(name, export)| VaultCertificateSummary {
                id: format!("id-{name}"),
                name: name.clone(),
                exportable: matches!(export, FakeExport::Ok),
                enabled: true,
                key_algorithm: "RSA-2048".into(),
                expires_at: None,
            })
            .collect();
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(listed)
    }

    async fn fetch_certificate(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        vault_name: &str,
        certificate_name: &str,
        export_format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        self.calls.lock().expect("lock").push(format!(
            "{vault_name}/{certificate_name}/{}",
            export_format.as_str()
        ));
        match self.exports.get(certificate_name).copied() {
            Some(FakeExport::Ok) => Ok(match export_format {
                VaultCertificateFormat::Pem => VaultCertificateMaterial::Pem {
                    certificate: Zeroizing::new(FAKE_CERT_PEM.to_vec()),
                    private_key: Zeroizing::new(FAKE_KEY_PEM.to_vec()),
                    key_algorithm: "RSA-2048".into(),
                },
                VaultCertificateFormat::Pkcs12 => VaultCertificateMaterial::Pkcs12 {
                    bundle: Zeroizing::new(FAKE_BUNDLE.to_vec()),
                    password: Zeroizing::new(FAKE_PASSWORD.to_string()),
                    key_algorithm: "EC-P256".into(),
                },
            }),
            Some(FakeExport::Fail(message)) => Err(DomainError::InvalidInput(message.to_string())),
            None => Err(DomainError::NotFound(
                "Certificate not found in this vault.".into(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Callback listener
// ---------------------------------------------------------------------------

/// Flips its flag when dropped, so a test can see that an endpoint closed.
struct FakeGuard(Arc<AtomicBool>);

impl Drop for FakeGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct FakeEndpointState {
    sender: tokio::sync::mpsc::Sender<ReceivedCall>,
    closed: Arc<AtomicBool>,
    host: Option<String>,
}

/// In-memory `CallbackListener`. Endpoint `i` gets the URL
/// `http://fake:1/cb/<i>`; tests push calls in with `sender(i)`.
pub struct FakeCallbackListener {
    endpoints: Mutex<Vec<FakeEndpointState>>,
    /// Once this many endpoints are open, every further `open` fails with
    /// the message.
    fail_with: Option<(usize, String)>,
    /// Calls put into the next endpoint the moment it opens, so a test can
    /// deliver a call before its node's turn.
    queued: Mutex<Vec<ReceivedCall>>,
    /// When set, the URL token is `tok-secret-<i>` (long enough to be masked).
    long_tokens: bool,
}

impl FakeCallbackListener {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            endpoints: Mutex::new(Vec::new()),
            fail_with: None,
            queued: Mutex::new(Vec::new()),
            long_tokens: false,
        })
    }

    /// A listener whose URLs end in `/cb/tok-secret-<i>`, like a real token.
    pub fn with_long_tokens() -> Arc<Self> {
        Arc::new(Self {
            endpoints: Mutex::new(Vec::new()),
            fail_with: None,
            queued: Mutex::new(Vec::new()),
            long_tokens: true,
        })
    }

    /// A listener whose every `open` fails with `message`.
    pub fn failing(message: &str) -> Arc<Self> {
        Self::failing_after(0, message)
    }

    /// A listener that opens `opened` endpoints, then fails every further
    /// `open` with `message`.
    pub fn failing_after(opened: usize, message: &str) -> Arc<Self> {
        Arc::new(Self {
            endpoints: Mutex::new(Vec::new()),
            fail_with: Some((opened, message.to_string())),
            queued: Mutex::new(Vec::new()),
            long_tokens: false,
        })
    }

    /// Drops the fake's sender for endpoint `index`, so the endpoint sees
    /// its channel close once the queued calls are read.
    pub fn hang_up(&self, index: usize) {
        let (sender, _calls) = tokio::sync::mpsc::channel(1);
        self.endpoints.lock().expect("lock")[index].sender = sender;
    }

    /// Delivers `call` into the next endpoint as soon as it opens.
    pub fn queue_on_open(&self, call: ReceivedCall) {
        self.queued.lock().expect("lock").push(call);
    }

    pub fn sender(&self, index: usize) -> tokio::sync::mpsc::Sender<ReceivedCall> {
        self.endpoints.lock().expect("lock")[index].sender.clone()
    }

    pub fn is_closed(&self, index: usize) -> bool {
        self.endpoints.lock().expect("lock")[index]
            .closed
            .load(Ordering::SeqCst)
    }

    pub fn opened_count(&self) -> usize {
        self.endpoints.lock().expect("lock").len()
    }

    pub fn hosts(&self) -> Vec<Option<String>> {
        self.endpoints
            .lock()
            .expect("lock")
            .iter()
            .map(|e| e.host.clone())
            .collect()
    }

    /// Waits until at least `count` endpoints are open, for up to 2 seconds.
    pub async fn wait_opened(&self, count: usize) {
        for _ in 0..400 {
            if self.opened_count() >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!(
            "expected {count} open endpoints, found {}",
            self.opened_count()
        );
    }
}

#[async_trait]
impl CallbackListener for Arc<FakeCallbackListener> {
    async fn open(&self, host: Option<&str>) -> DomainResult<CallbackEndpoint> {
        if let Some((opened, message)) = &self.fail_with {
            if self.opened_count() >= *opened {
                return Err(DomainError::Io(message.clone()));
            }
        }
        let (sender, calls) = tokio::sync::mpsc::channel(100);
        for call in self.queued.lock().expect("lock").drain(..) {
            sender.try_send(call).expect("the fake channel has room");
        }
        let closed = Arc::new(AtomicBool::new(false));
        let mut endpoints = self.endpoints.lock().expect("lock");
        let index = endpoints.len();
        endpoints.push(FakeEndpointState {
            sender,
            closed: Arc::clone(&closed),
            host: host.map(str::to_string),
        });
        Ok(CallbackEndpoint {
            url: if self.long_tokens {
                format!("http://fake:1/cb/tok-secret-{index}")
            } else {
                format!("http://fake:1/cb/{index}")
            },
            calls,
            guard: Box::new(FakeGuard(closed)),
        })
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
