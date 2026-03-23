use std::collections::HashMap;

use rocket_environment::{resolve, EnvironmentRepository};
use rocket_history::{HistoryEntry, HistoryRepository};
use rocket_http::{CookieRepository, HttpExecutor, HttpRequest, HttpResponse, RequestOptions};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::{Auth, Body, Header, HttpMethod};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequestInput {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<Body>,
    pub auth: Auth,
    pub options: RequestOptions,
    pub environment_name: Option<String>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
}

pub struct RequestExecutionService {
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Box<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
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
        cookie_repo: Box<dyn CookieRepository>,
        events: Box<dyn EventPublisher>,
    ) -> Self {
        Self { env_repo, executor, history_repo, cookie_repo, events }
    }

    pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<HttpResponse> {
        // Step 1: Build variable map from the selected environment.
        let vars = self.build_variable_map(&input.environment_name)?;

        // Step 2: Resolve {{placeholders}} in URL and headers.
        let resolved_url = resolve(&input.url, &vars).output;
        let resolved_headers: Vec<Header> = input
            .headers
            .iter()
            .map(|h| Header {
                key: resolve(&h.key, &vars).output,
                value: resolve(&h.value, &vars).output,
                enabled: h.enabled,
            })
            .collect();

        // Step 3: Build and execute the HTTP request.
        let http_request = HttpRequest {
            method: input.method,
            url: resolved_url.clone(),
            headers: resolved_headers,
            body: input.body,
            auth: input.auth,
            options: input.options,
        };
        let response = self.executor.execute(&http_request).await?;

        // Step 4: Persist history (non-fatal — a save failure won't cancel the response).
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

        // Step 5: Publish domain event.
        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: resolved_url,
            status: response.status,
            duration_ms: response.duration_ms,
        });

        Ok(response)
    }

    fn build_variable_map(
        &self,
        env_name: &Option<String>,
    ) -> DomainResult<HashMap<String, String>> {
        let mut vars = HashMap::new();
        if let Some(name) = env_name {
            if let Ok(env) = self.env_repo.get(name) {
                for (k, v) in env.enabled_variables() {
                    vars.insert(k.to_string(), v.to_string());
                }
            }
        }
        Ok(vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_environment::{Environment, Variable};
    use std::sync::Mutex;
    use rocket_http::{CookieJar, HttpResponse};
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::Arc;

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

    fn sample_input(url: &str, env_name: Option<&str>) -> ExecuteRequestInput {
        ExecuteRequestInput {
            method: HttpMethod::Get,
            url: url.to_string(),
            headers: vec![],
            body: None,
            auth: rocket_shared::types::Auth::None,
            options: RequestOptions::default(),
            environment_name: env_name.map(str::to_string),
            collection: None,
            request_name: None,
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
        }

        let history_arc = Arc::clone(&history);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Box::new(MockExecutor::new(200)),
            Box::new(SharedHistoryRepo(history)),
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
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(publisher)),
        );

        svc.execute(sample_input("https://example.com/items", None)).await.unwrap();

        let events = pub_arc.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DomainEvent::RequestExecuted { status: 201, .. }));
    }
}
