use std::collections::HashMap;

use rocket_collection::CollectionRepository;
use rocket_environment::{resolve, EnvironmentRepository};
use rocket_history::{HistoryEntry, HistoryRepository};
use rocket_http::{CookieRepository, HttpExecutor, HttpRequest, HttpResponse, RequestOptions};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::{Auth, Body, Header, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequestInput {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub auth: Auth,
    pub options: RequestOptions,
    pub environment_name: Option<String>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
    /// Path of the request file relative to the collection root (e.g. "auth/login.yml").
    /// Used to load folder-chain and request-level variables.
    #[serde(default)]
    pub request_path: Option<String>,
}

pub struct RequestExecutionService {
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Box<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    // Reserved for automatic cookie persistence in future requests.
    #[allow(dead_code)]
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
}

impl RequestExecutionService {
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        executor: Box<dyn HttpExecutor>,
        history_repo: Box<dyn HistoryRepository>,
        collection_repo: Box<dyn CollectionRepository>,
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
    ) -> Self {
        Self { env_repo, executor, history_repo, collection_repo, cookie_repo, events }
    }

    pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<HttpResponse> {
        // Step 1: Build variable map — collection variables first, then env overrides.
        let mut vars = HashMap::new();
        if let Some(col) = &input.collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            // Load collection variables (lower priority).
            for cv in &settings.variables {
                if cv.enabled {
                    let val = if cv.value.is_empty() { &cv.initial_value } else { &cv.value };
                    vars.insert(cv.key.clone(), val.clone());
                }
            }
        }
        // Layer environment variables on top (higher priority, overrides collection vars).
        if let Some(name) = &input.environment_name {
            if let Ok(env) = self.env_repo.get(name) {
                for (k, v) in env.enabled_variables() {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }

        // Step 2: Load collection settings and merge with request-level values.
        let (effective_auth, effective_headers) = if let Some(col) = &input.collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            let auth = merge_auth(input.auth.clone(), settings.auth);
            let headers = merge_headers(&settings.headers, &input.headers);
            (auth, headers)
        } else {
            (input.auth.clone(), input.headers.clone())
        };

        // Step 3: Resolve {{placeholders}} in URL and headers.
        let resolved_url = resolve(&input.url, &vars).output;
        let resolved_headers: Vec<Header> = effective_headers
            .iter()
            .map(|h| Header {
                key: resolve(&h.key, &vars).output,
                value: resolve(&h.value, &vars).output,
                enabled: h.enabled,
                description: None,
            })
            .collect();

        // Step 4: Build and execute the HTTP request.
        let http_request = HttpRequest {
            method: input.method,
            url: resolved_url.clone(),
            headers: resolved_headers,
            query_params: input.query_params,
            body: input.body,
            auth: effective_auth,
            options: input.options,
        };
        let response = self.executor.execute(&http_request).await?;

        // Step 5: Persist history (non-fatal — a save failure won't cancel the response).
        let mut entry = HistoryEntry::new(
            input.method.to_string(),
            &resolved_url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        );
        if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry = entry.with_collection(col, name);
        }
        let _ = self.history_repo.save(&entry);

        // Step 6: Publish domain event.
        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: resolved_url,
            status: response.status,
            duration_ms: response.duration_ms,
        });

        Ok(response)
    }

}

/// Use the collection auth when the request carries no auth of its own.
fn merge_auth(request_auth: Auth, collection_auth: Option<Auth>) -> Auth {
    match request_auth {
        Auth::None => collection_auth.unwrap_or(Auth::None),
        explicit => explicit,
    }
}

/// Merge collection-level headers with request-level headers.
/// Request headers override collection headers when they share the same key.
fn merge_headers(collection_headers: &[Header], request_headers: &[Header]) -> Vec<Header> {
    let request_keys: std::collections::HashSet<&str> = request_headers
        .iter()
        .filter(|h| h.enabled)
        .map(|h| h.key.as_str())
        .collect();
    let mut merged: Vec<Header> = collection_headers
        .iter()
        .filter(|h| !request_keys.contains(h.key.as_str()))
        .cloned()
        .collect();
    merged.extend(request_headers.iter().cloned());
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_collection::{Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable, Request as CollectionRequest};
    use rocket_environment::{Environment, Variable};
    use rocket_http::{CookieJar, HttpResponse};
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::{Arc, Mutex};

    // Fixed-response mock executor that records the last URL it received.
    struct MockExecutor {
        last_url: Mutex<Option<String>>,
        response: HttpResponse,
    }

    impl MockExecutor {
        fn new(status: u16) -> Self {
            Self {
                last_url: Mutex::new(None),
                response: HttpResponse {
                    status,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 50,
                    size_bytes: 2,
                },
            }
        }
    }

    #[async_trait]
    impl HttpExecutor for MockExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.last_url.lock().unwrap() = Some(req.url.clone());
            Ok(self.response.clone())
        }
    }

    // Mock environment repo with one pre-loaded environment.
    struct MockEnvRepo {
        env: Option<Environment>,
    }

    impl MockEnvRepo {
        fn with_env(env: Environment) -> Self {
            Self { env: Some(env) }
        }
        fn empty() -> Self {
            Self { env: None }
        }
    }

    impl rocket_environment::EnvironmentRepository for MockEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.env.iter().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.env
                .as_ref()
                .filter(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    // In-memory history repo.
    struct MockHistoryRepo {
        entries: Mutex<Vec<HistoryEntry>>,
    }

    impl MockHistoryRepo {
        fn new() -> Self {
            Self { entries: Mutex::new(Vec::new()) }
        }
    }

    impl HistoryRepository for MockHistoryRepo {
        fn list(&self, _: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }
        fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
            self.entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(id.into()))
        }
        fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            self.entries.lock().unwrap().clear();
            Ok(())
        }
        fn search(&self, _: &rocket_history::HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }
    }

    // No-op cookie repo.
    struct NullCookieRepo;

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

    // Collection repo with configurable per-collection settings.
    struct StubCollectionRepo {
        settings: CollectionSettings,
    }

    impl StubCollectionRepo {
        fn empty() -> Self {
            Self { settings: CollectionSettings::default() }
        }

        fn with_settings(settings: CollectionSettings) -> Self {
            Self { settings }
        }
    }

    impl CollectionRepository for StubCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> { Ok(vec![]) }
        fn get(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> { Ok(path.to_string()) }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> { Ok(()) }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(self.settings.clone())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> { Ok(()) }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
    }

    fn sample_input(url: &str, env_name: Option<&str>) -> ExecuteRequestInput {
        ExecuteRequestInput {
            method: HttpMethod::Get,
            url: url.to_string(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: rocket_shared::types::Auth::None,
            options: RequestOptions::default(),
            environment_name: env_name.map(str::to_string),
            collection: None,
            request_name: None,
            request_path: None,
        }
    }

    #[tokio::test]
    async fn execute_resolves_variables_in_url() {
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("BASE_URL", "https://api.example.com"));

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Box::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );

        let resp = svc
            .execute(sample_input("{{BASE_URL}}/users", Some("prod")))
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn execute_saves_history() {
        // Share history repo via Arc so we can assert on it after the service runs.
        let history = Arc::new(MockHistoryRepo::new());

        struct SharedHistoryRepo(Arc<MockHistoryRepo>);

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
            fn search(&self, filter: &rocket_history::HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
                self.0.search(filter)
            }
        }

        let history_arc = Arc::clone(&history);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Box::new(MockExecutor::new(200)),
            Box::new(SharedHistoryRepo(history)),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );

        svc.execute(sample_input("https://example.com", None)).await.unwrap();

        assert_eq!(history_arc.entries.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn execute_publishes_event() {
        use rocket_shared::events::DomainEvent;
        use std::sync::Mutex;

        struct RecordingPublisher {
            events: Mutex<Vec<DomainEvent>>,
        }

        impl rocket_shared::events::EventPublisher for RecordingPublisher {
            fn publish(&self, event: DomainEvent) {
                self.events.lock().unwrap().push(event);
            }
        }

        let publisher = Arc::new(RecordingPublisher { events: Mutex::new(vec![]) });

        struct SharedPublisher(Arc<RecordingPublisher>);
        impl rocket_shared::events::EventPublisher for SharedPublisher {
            fn publish(&self, event: DomainEvent) {
                self.0.publish(event);
            }
        }

        let pub_arc = Arc::clone(&publisher);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Box::new(MockExecutor::new(201)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(publisher)),
        );

        svc.execute(sample_input("https://example.com/items", None)).await.unwrap();

        let events = pub_arc.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DomainEvent::RequestExecuted { status: 201, .. }));
    }

    // -------------------------------------------------------------------------
    // merge_headers unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn merge_headers_request_overrides_collection_by_key() {
        let col = vec![
            Header::new("X-Tenant", "acme"),
            Header::new("Accept", "application/json"),
        ];
        let req = vec![Header::new("Accept", "text/plain")];
        let merged = merge_headers(&col, &req);

        // Accept from request wins; X-Tenant from collection is preserved.
        assert_eq!(merged.len(), 2);
        let accept = merged.iter().find(|h| h.key == "Accept").unwrap();
        assert_eq!(accept.value, "text/plain");
        assert!(merged.iter().any(|h| h.key == "X-Tenant" && h.value == "acme"));
    }

    #[test]
    fn merge_headers_disabled_request_header_does_not_override_collection() {
        let col = vec![Header::new("Accept", "application/json")];
        // Disabled request header should not shadow the collection header.
        let req = vec![Header::disabled("Accept", "text/plain")];
        let merged = merge_headers(&col, &req);

        // Collection header is kept because request header is disabled.
        let accept = merged.iter().find(|h| h.key == "Accept").unwrap();
        assert_eq!(accept.value, "application/json");
    }

    #[test]
    fn merge_headers_empty_collection_returns_request_headers() {
        let req = vec![Header::new("Authorization", "Bearer tok")];
        let merged = merge_headers(&[], &req);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, "Authorization");
    }

    // -------------------------------------------------------------------------
    // merge_auth unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn merge_auth_uses_collection_when_request_is_none() {
        let collection_auth = Some(Auth::Bearer { token: "col_tok".into() });
        let result = merge_auth(Auth::None, collection_auth);
        assert_eq!(result, Auth::Bearer { token: "col_tok".into() });
    }

    #[test]
    fn merge_auth_request_takes_precedence_over_collection() {
        let collection_auth = Some(Auth::Bearer { token: "col_tok".into() });
        let request_auth = Auth::Basic { username: "user".into(), password: "pass".into() };
        let result = merge_auth(request_auth.clone(), collection_auth);
        assert_eq!(result, request_auth);
    }

    #[test]
    fn merge_auth_none_collection_returns_none() {
        let result = merge_auth(Auth::None, None);
        assert_eq!(result, Auth::None);
    }

    // -------------------------------------------------------------------------
    // Integration test: collection settings applied during execute
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn execute_uses_collection_auth_when_request_auth_is_none() {
        use rocket_shared::types::Auth;

        let settings = CollectionSettings {
            description: None,
            readme: None,
            auth: Some(Auth::Bearer { token: "col_tok".into() }),
            headers: vec![],
            variables: vec![],
        };

        // Use a mock executor that captures the request auth.
        struct CapturingExecutor {
            last_auth: Mutex<Option<Auth>>,
        }

        #[async_trait]
        impl HttpExecutor for CapturingExecutor {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                *self.last_auth.lock().unwrap() = Some(req.auth.clone());
                Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 1,
                    size_bytes: 2,
                })
            }
        }

        let executor = Arc::new(CapturingExecutor { last_auth: Mutex::new(None) });

        struct SharedExecutor(Arc<CapturingExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExecutor {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }

        let exec_arc = Arc::clone(&executor);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Box::new(SharedExecutor(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::with_settings(settings)),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );

        let mut input = sample_input("https://api.example.com", None);
        input.collection = Some("my-api".into());
        svc.execute(input).await.unwrap();

        let captured = exec_arc.last_auth.lock().unwrap().clone().unwrap();
        assert_eq!(captured, Auth::Bearer { token: "col_tok".into() });
    }
}
