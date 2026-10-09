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

// ── against a real server ────────────────────────────────────────────────────

use rocket_environment::{Environment, Variable};
use std::time::Duration;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::test_doubles::StaticEnvRepo;

const SECRET: &str = "sk-live-abcdef123";

/// A service with the real engine and the real `ReqwestExecutor`. The
/// environment `dev` holds the secret variable `API_KEY`.
fn real_service() -> RequestExecutionService {
    let mut env = Environment::new("dev");
    env.set_variable(Variable::secret("API_KEY", SECRET));
    RequestExecutionService::new(
        Box::new(StaticEnvRepo(env)),
        Arc::new(rocket_infra::ReqwestExecutor::new()),
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

/// Starts a server whose `/main` answers 200, and returns it.
async fn server_with_main() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/main"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

/// Sends `GET <server>/main` in environment `dev` with `script` as its
/// pre-request script and returns the Console lines.
async fn run_pre_request(server: &MockServer, script: String) -> Vec<String> {
    let svc = real_service();
    let mut req = input(&format!("{}/main", server.uri()));
    req.method = HttpMethod::Get;
    req.environment_name = Some("dev".into());
    req.pre_request_script = Some(script);
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    out.console_entries.into_iter().map(|e| e.message).collect()
}

#[tokio::test]
async fn real_server_json_reply_is_parsed() {
    let server = server_with_main().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("content-type", "application/json"))
        .and(body_json(serde_json::json!({ "user": "a" })))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("X-Trace", "t-1")
                .set_body_json(serde_json::json!({ "token": "abc" })),
        )
        .mount(&server)
        .await;
    let lines = run_pre_request(
        &server,
        format!(
            "const r = await rok.sendRequest({{ method: 'POST', url: '{}/token', data: {{ user: 'a' }} }}); \
             console.log(r.status, r.headers['x-trace'], r.data.token);",
            server.uri()
        ),
    )
    .await;
    assert!(lines.iter().any(|l| l == "201 t-1 abc"), "{lines:?}");
}

#[tokio::test]
async fn real_server_404_resolves() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let lines = run_pre_request(
        &server,
        format!(
            "const r = await rok.sendRequest({{ url: '{}/missing' }}); console.log('status', r.status);",
            server.uri()
        ),
    )
    .await;
    assert!(lines.iter().any(|l| l == "status 404"), "{lines:?}");
}

#[tokio::test]
async fn real_server_slower_than_the_timeout_rejects() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let lines = run_pre_request(
        &server,
        format!(
            "try {{ await rok.sendRequest({{ url: '{}/slow', timeout: 200 }}); }} \
             catch (e) {{ console.log('caught', e.message); }}",
            server.uri()
        ),
    )
    .await;
    assert!(
        lines
            .iter()
            .any(|l| l == "caught rok.sendRequest: timed out after 200 ms"),
        "{lines:?}"
    );
    assert!(started.elapsed() < Duration::from_millis(1_800));
}

#[tokio::test]
async fn real_network_error_is_masked() {
    let server = server_with_main().await;
    // A port that was just free: connecting to it is refused.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("free port");
    let lines = run_pre_request(
        &server,
        format!(
            "try {{ await rok.sendRequest({{ url: 'http://127.0.0.1:{port}/?key=' + rok.getEnvVar('API_KEY') }}); }} \
             catch (e) {{ console.log('caught', e.message); }}"
        ),
    )
    .await;
    assert!(lines.iter().all(|l| !l.contains(SECRET)), "{lines:?}");
    assert!(
        lines.iter().any(|l| l.starts_with("caught rok.sendRequest:")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn real_server_echo_of_a_secret_is_masked_in_the_console() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "echo": SECRET })))
        .mount(&server)
        .await;
    let lines = run_pre_request(
        &server,
        format!(
            "const r = await rok.sendRequest({{ url: '{}/echo' }}); console.log(JSON.stringify(r.data));",
            server.uri()
        ),
    )
    .await;
    assert!(lines.iter().all(|l| !l.contains(SECRET)), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("••••••")), "{lines:?}");
}
