//! The script host of a request, run with the real script engine.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocket_collection::Collection;
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_shared::error::DomainResult;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::{BodyMode, HttpMethod};

use super::RequestExecutionService;
use crate::graphql_request::test_support::input;
use crate::test_doubles::{
    EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
    NullEnvRepo, SharedCollectionRepo, SharedHistoryRepo,
};

/// Executor that keeps every request it gets and answers 200 with `{"ok":true}`.
#[derive(Default)]
struct KeepingExecutor {
    seen: Mutex<Vec<HttpRequest>>,
}

impl KeepingExecutor {
    fn seen(&self) -> Vec<HttpRequest> {
        self.seen.lock().expect("lock").clone()
    }
}

#[async_trait]
impl HttpExecutor for KeepingExecutor {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        self.seen.lock().expect("lock").push(request.clone());
        Ok(HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: "{\"ok\":true}".into(),
            duration_ms: 3,
            ttfb_ms: 1,
            size_bytes: 11,
            ..Default::default()
        })
    }
}

/// Hands one `Arc<KeepingExecutor>` to a service expecting `Arc<dyn HttpExecutor>`.
struct SharedKeeping(Arc<KeepingExecutor>);

#[async_trait]
impl HttpExecutor for SharedKeeping {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        self.0.execute(request).await
    }
}

/// A service with the real script engine, sending through `executor`.
fn service(executor: Arc<KeepingExecutor>) -> RequestExecutionService {
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(SharedKeeping(executor)),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("api"),
        ))),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()))
}

#[tokio::test]
async fn a_pre_request_script_sends_with_the_request_options() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("https://main.test/x");
    req.method = HttpMethod::Get;
    req.options.verify_ssl = false;
    req.options.follow_redirects = false;
    req.pre_request_script = Some(
        "const r = await rok.sendRequest({ method: 'POST', url: 'https://side.test/token', \
         data: { a: 1 } }); console.log('side', r.status, r.data.ok);"
            .into(),
    );
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);

    let seen = executor.seen();
    assert_eq!(seen.len(), 2, "the side request, then the main request");
    assert_eq!(seen[0].url, "https://side.test/token");
    assert_eq!(seen[0].method, HttpMethod::Post);
    assert!(!seen[0].options.verify_ssl);
    assert!(!seen[0].options.follow_redirects);
    assert_eq!(seen[0].options.timeout_ms, 31_000);
    let body = seen[0].body.as_ref().expect("side body");
    assert_eq!(body.mode, BodyMode::Json);
    assert_eq!(body.content.as_deref(), Some("{\"a\":1}"));
    assert_eq!(seen[1].url, "https://main.test/x");

    let lines: Vec<&str> = out.console_entries.iter().map(|e| e.message.as_str()).collect();
    assert!(lines.contains(&"side 200 true"), "{lines:?}");
    assert!(
        lines.contains(&"rok.sendRequest POST https://side.test/token -> 200 (3 ms)"),
        "{lines:?}"
    );
}

#[tokio::test]
async fn post_response_and_tests_scripts_can_send_too() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("https://main.test/x");
    req.method = HttpMethod::Get;
    req.post_response_script =
        Some("await rok.sendRequest({ url: 'https://side.test/after' });".into());
    req.tests_script = Some("await rok.sendRequest({ url: 'https://side.test/tests' });".into());
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    let urls: Vec<String> = executor.seen().into_iter().map(|r| r.url).collect();
    assert_eq!(
        urls,
        vec![
            "https://main.test/x".to_string(),
            "https://side.test/after".to_string(),
            "https://side.test/tests".to_string(),
        ]
    );
}
