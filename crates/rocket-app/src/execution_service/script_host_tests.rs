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

    let lines: Vec<&str> = out
        .console_entries
        .iter()
        .map(|e| e.message.as_str())
        .collect();
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
        lines
            .iter()
            .any(|l| l.starts_with("caught rok.sendRequest:")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn real_server_echo_of_a_secret_is_masked_in_the_console() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/echo"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "echo": SECRET })),
        )
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

// ── rok.runRequest ───────────────────────────────────────────────────────────

use rocket_collection::{CollectionItem, Folder, Request, WebSocketRequest};
use rocket_environment::EnvironmentRepository;

use super::{ExecuteRequestInput, ExecuteRequestOutput};

/// Environment repo that keeps one environment in memory and saves into it.
struct MemoryEnvRepo(Arc<Mutex<Environment>>);

impl EnvironmentRepository for MemoryEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        Ok(vec![self.0.lock().expect("lock").clone()])
    }
    fn get(&self, _name: &str) -> DomainResult<Environment> {
        Ok(self.0.lock().expect("lock").clone())
    }
    fn save(&self, env: &Environment) -> DomainResult<()> {
        *self.0.lock().expect("lock") = env.clone();
        Ok(())
    }
    fn delete(&self, _name: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// A saved GET request whose URL ends in its file name.
fn saved(name: &str, file: &str, pre_request: &str) -> Request {
    let mut request = Request::new(name, HttpMethod::Get, format!("https://api.test/{file}"));
    request.file_name = Some(file.to_string());
    if !pre_request.is_empty() {
        request.pre_request_script = Some(pre_request.to_string());
    }
    request
}

/// A service over `collection` with the real engine and environment `dev` (`E` = `old`).
fn run_service(collection: Collection) -> (RequestExecutionService, Arc<KeepingExecutor>) {
    run_service_with_engine(collection, rocket_infra::scripting::DenoScriptEngine::new())
}

/// Like `run_service`, with an engine that has the given limits.
fn run_service_with_engine(
    collection: Collection,
    engine: rocket_infra::scripting::DenoScriptEngine,
) -> (RequestExecutionService, Arc<KeepingExecutor>) {
    let executor = Arc::new(KeepingExecutor::default());
    let mut env = Environment::new("dev");
    env.set_variable(Variable::new("E", "old"));
    let svc = RequestExecutionService::new(
        Box::new(MemoryEnvRepo(Arc::new(Mutex::new(env)))),
        Arc::new(SharedKeeping(Arc::clone(&executor))),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            collection,
        ))),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(engine));
    (svc, executor)
}

/// The input a single send of the request at `path` gets, in environment `dev`.
fn send_input(collection: &Collection, path: &str) -> ExecuteRequestInput {
    let item = crate::runner_sequence::flatten_run_set(collection, None)
        .expect("run set")
        .into_iter()
        .find(|item| item.request_path == path)
        .expect("request in the collection");
    crate::runner_sequence::build_step_input(
        &item,
        "api",
        Some("dev"),
        None,
        Default::default(),
        rocket_shared::RunSource::Manual,
    )
}

fn console(out: &ExecuteRequestOutput) -> Vec<String> {
    out.console_entries
        .iter()
        .map(|e| e.message.clone())
        .collect()
}

fn urls(executor: &KeepingExecutor) -> Vec<String> {
    executor.seen().into_iter().map(|r| r.url).collect()
}

/// root: [main.yml with `main_script`], auth/ [login.yml with `login_pre`, `login_post`]
fn main_and_login(main_script: &str, login_pre: &str, login_post: &str) -> Collection {
    let mut collection = Collection::new("api");
    collection
        .root
        .add_request(saved("Main", "main.yml", main_script));
    let mut login = saved("Login", "login.yml", login_pre);
    if !login_post.is_empty() {
        login.post_response_script = Some(login_post.to_string());
    }
    let mut auth = Folder::new("auth");
    auth.add_request(login);
    collection.root.add_subfolder(auth);
    collection
}

#[tokio::test]
async fn run_request_e2e_runs_the_saved_request() {
    let collection = main_and_login(
        "const r = await rok.runRequest('auth/login'); console.log('login', r.status, r.data.ok);",
        "",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert_eq!(
        urls(&executor),
        vec![
            "https://api.test/login.yml".to_string(),
            "https://api.test/main.yml".to_string()
        ]
    );
    let lines = console(&out);
    assert!(lines.contains(&"login 200 true".to_string()), "{lines:?}");
    assert!(
        lines.contains(&"rok.runRequest auth/login -> 200".to_string()),
        "{lines:?}"
    );
}

#[tokio::test]
async fn run_request_e2e_nested_runtime_writes_reach_the_caller_and_later_phases() {
    let mut collection = main_and_login(
        "rok.setVar('token', 'old'); await rok.runRequest('auth/login'); \
         console.log('now', rok.getVar('token'));",
        "",
        "rok.setVar('token', 'from-login');",
    );
    if let Some(CollectionItem::Request(main)) = collection.root.items.first_mut() {
        main.tests = Some("console.log('later', rok.getVar('token'));".into());
    }
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out);
    assert!(lines.contains(&"now from-login".to_string()), "{lines:?}");
    assert!(lines.contains(&"later from-login".to_string()), "{lines:?}");
}

#[tokio::test]
async fn run_request_e2e_the_nested_run_sees_the_callers_runtime_vars() {
    let collection = main_and_login(
        "rok.setVar('who', 'main'); await rok.runRequest('auth/login'); \
         console.log('seen', rok.getVar('seen'));",
        "rok.setVar('seen', rok.getVar('who'));",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"seen main".to_string()),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn run_request_e2e_nested_env_writes_are_visible_after_the_call() {
    let collection = main_and_login(
        "await rok.runRequest('auth/login'); console.log('E', rok.getEnvVar('E'));",
        "rok.setEnvVar('E', 'new');",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"E new".to_string()),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn run_request_e2e_a_pending_env_write_survives_an_unrelated_nested_run() {
    // An earlier phase saved E=a. The post-response script then writes E=b and
    // runs a request that never touches E, so E must stay b.
    let mut collection = main_and_login("rok.setEnvVar('E', 'a');", "", "");
    if let Some(CollectionItem::Request(main)) = collection.root.items.first_mut() {
        main.post_response_script = Some(
            "rok.setEnvVar('E', 'b'); await rok.runRequest('auth/login'); \
             console.log('E', rok.getEnvVar('E'));"
                .into(),
        );
    }
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"E b".to_string()),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn run_request_e2e_other_protocols_are_skipped() {
    let mut collection = main_and_login(
        "const r = await rok.runRequest('socket'); console.log('ws', r.status);",
        "",
        "",
    );
    let mut socket = WebSocketRequest::new("Socket", "wss://api.test/ws");
    socket.file_name = Some("socket.yml".into());
    collection
        .root
        .items
        .push(CollectionItem::WebSocket(Box::new(socket)));
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"ws skipped".to_string()),
        "{:?}",
        console(&out)
    );
    assert_eq!(
        urls(&executor),
        vec!["https://api.test/main.yml".to_string()]
    );
}

#[tokio::test]
async fn run_request_e2e_an_unknown_path_rejects() {
    let collection = main_and_login(
        "try { await rok.runRequest('nope/missing'); } catch (e) { console.log('err', e.message); }",
        "",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out)
            .contains(&"err rok.runRequest: invalid request path - nope/missing".to_string()),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn run_request_e2e_a_self_call_rejects() {
    let collection = main_and_login(
        "try { await rok.runRequest('main'); } catch (e) { console.log('err', e.message); }",
        "",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"err rok.runRequest: recursive call to main".to_string()),
        "{:?}",
        console(&out)
    );
    assert_eq!(
        urls(&executor),
        vec!["https://api.test/main.yml".to_string()]
    );
}

#[tokio::test]
async fn run_request_e2e_a_cycle_rejects_inside_the_nested_run() {
    let mut collection = Collection::new("api");
    collection.root.add_request(saved(
        "A",
        "a.yml",
        "await rok.runRequest('b'); console.log('cycle', rok.getVar('cycle'));",
    ));
    collection.root.add_request(saved(
        "B",
        "b.yml",
        "try { await rok.runRequest('a'); } catch (e) { rok.setVar('cycle', e.message); }",
    ));
    let input = send_input(&collection, "a.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"cycle rok.runRequest: recursive call to a".to_string()),
        "{:?}",
        console(&out)
    );
    assert_eq!(
        urls(&executor),
        vec![
            "https://api.test/b.yml".to_string(),
            "https://api.test/a.yml".to_string()
        ]
    );
}

#[tokio::test]
async fn run_request_e2e_nesting_stops_after_five_levels() {
    let mut collection = Collection::new("api");
    for i in 1..=7 {
        let script = if i < 7 {
            format!("await rok.runRequest('r{}');", i + 1)
        } else {
            String::new()
        };
        collection
            .root
            .add_request(saved(&format!("R{i}"), &format!("r{i}.yml"), &script));
    }
    let input = send_input(&collection, "r1.yml");
    let (svc, executor) = run_service(collection);
    svc.execute(input).await.expect("execute");
    let sent = urls(&executor);
    assert_eq!(sent.len(), 6, "{sent:?}");
    assert!(sent.iter().all(|url| !url.ends_with("r7.yml")), "{sent:?}");
}

#[tokio::test]
async fn run_request_rejects_past_the_active_nested_run_limit() {
    let mut collection = Collection::new("api");
    collection.root.add_request(saved(
        "Main",
        "main.yml",
        "const r = await Promise.allSettled(Array.from({length: 30}, () => rok.runRequest('slow'))); \
         const bad = r.filter(x => x.status === 'rejected'); \
         console.log('rejected:' + bad.length); \
         console.log(bad[0].reason.message);",
    ));
    collection
        .root
        .add_request(saved("Slow", "slow.yml", "await rok.sleep(200);"));
    let input = send_input(&collection, "main.yml");
    let limits = rocket_infra::scripting::ScriptLimits {
        max_host_calls: 40,
        ..rocket_infra::scripting::ScriptLimits::DEFAULT
    };
    let engine = rocket_infra::scripting::DenoScriptEngine::with_limits(limits);
    let (svc, _executor) = run_service_with_engine(collection, engine);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out);
    assert!(lines.iter().any(|l| l == "rejected:14"), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("rok.runRequest: too many nested requests running (limit 16)")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn run_request_releases_its_slot_after_a_nested_script_error() {
    let mut collection = Collection::new("api");
    collection.root.add_request(saved(
        "Main",
        "main.yml",
        "let limited = 0; \
         for (let i = 0; i < 20; i++) { \
           try { await rok.runRequest('bad'); } \
           catch (e) { if (e.message.includes('too many nested')) limited++; } \
         } \
         console.log('limited:' + limited);",
    ));
    collection
        .root
        .add_request(saved("Bad", "bad.yml", "throw new Error('boom');"));
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out);
    assert!(lines.iter().any(|l| l == "limited:0"), "{lines:?}");
}

// ── the request guard ────────────────────────────────────────────────────────

fn guard_on() -> rocket_workspace::RequestGuardPolicy {
    rocket_workspace::RequestGuardPolicy {
        block_script_redirects_to_internal_hosts: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn send_request_blocks_an_internal_host_when_the_policy_is_on() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("https://main.test/x");
    req.method = HttpMethod::Get;
    req.request_guard_policy = guard_on();
    req.pre_request_script =
        Some("await rok.sendRequest({ url: 'http://169.254.169.254/latest' });".into());
    let out = svc.execute(req).await.expect("execute");
    let error = out.script_error.expect("script error");
    assert!(error.contains("rok.sendRequest"), "{error}");
    assert!(
        error.contains("blocked: script redirected request to internal host '169.254.169.254'"),
        "{error}"
    );
    assert!(
        !executor.seen().iter().any(|r| r.url.contains("169.254")),
        "{:?}",
        executor.seen()
    );
}

#[tokio::test]
async fn send_request_to_the_calling_requests_own_host_is_allowed_with_the_policy_on() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("http://127.0.0.1:9/main");
    req.method = HttpMethod::Get;
    req.request_guard_policy = guard_on();
    req.pre_request_script =
        Some("await rok.sendRequest({ url: 'http://127.0.0.1:9/side' });".into());
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert!(urls(&executor).contains(&"http://127.0.0.1:9/side".to_string()));
}

#[tokio::test]
async fn send_request_to_the_host_of_a_templated_request_url_is_allowed_with_the_policy_on() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut env = Environment::new("dev");
    env.set_variable(Variable::new("base", &server.uri()));
    let svc = RequestExecutionService::new(
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
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()));
    let mut req = input("{{base}}/x");
    req.method = HttpMethod::Get;
    req.environment_name = Some("dev".into());
    req.request_guard_policy = guard_on();
    req.pre_request_script = Some(format!(
        "await rok.sendRequest({{ url: '{}/side' }});",
        server.uri()
    ));
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
}

#[tokio::test]
async fn send_request_to_an_internal_host_is_allowed_when_the_policy_is_off() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("https://main.test/x");
    req.method = HttpMethod::Get;
    req.pre_request_script =
        Some("await rok.sendRequest({ url: 'http://169.254.169.254/latest' });".into());
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert!(urls(&executor).contains(&"http://169.254.169.254/latest".to_string()));
}

#[tokio::test]
async fn run_request_nested_set_url_is_guarded_by_the_policy() {
    let collection = main_and_login(
        "try { await rok.runRequest('auth/login'); } catch (e) { console.log('err', e.message); }",
        "req.setUrl('http://169.254.169.254/x');",
        "",
    );
    let mut input = send_input(&collection, "main.yml");
    input.request_guard_policy = guard_on();
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out).join("\n");
    assert!(
        lines.contains("blocked: script redirected request to internal host '169.254.169.254'"),
        "{lines}"
    );
    assert!(
        !urls(&executor).iter().any(|u| u.contains("169.254")),
        "{:?}",
        urls(&executor)
    );
}

// ── nested runs and an unresolved RocketVault alias ──────────────────────────

use rocket_environment::{
    ExternalSecretBinding, ExternalSecretRef, SecretManagerConnection, VaultSecretFetcher,
};
use rocket_shared::error::DomainError;
use rocket_shared::types::Header;

use crate::test_doubles::{FakeSecretManagerRepo, FakeSecretStore};

/// Every secret fetch fails, as when the vault rejects the credentials.
struct RejectingFetcher;

#[async_trait]
impl VaultSecretFetcher for RejectingFetcher {
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
        Err(DomainError::Internal(
            "vault rejected the credentials".into(),
        ))
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

/// A service over `collection` whose environment `dev` binds the alias `vault`, which cannot be fetched.
fn failing_vault_service(
    collection: Collection,
) -> (RequestExecutionService, Arc<KeepingExecutor>) {
    let executor = Arc::new(KeepingExecutor::default());
    let mut env = Environment::new("dev");
    env.external_secrets.push(ExternalSecretBinding {
        alias: "vault".to_string(),
        connection_id: "conn-1".to_string(),
        vault_name: "dev-vault".to_string(),
        secret_names: vec![ExternalSecretRef {
            name: "key".to_string(),
            secret_id: "sec-1".to_string(),
        }],
    });
    let connection = SecretManagerConnection {
        id: "conn-1".to_string(),
        label: "Test".to_string(),
        base_url: "https://vault.internal:8774".to_string(),
        client_id: "rocketapi".to_string(),
        verify_ssl: true,
        allow_insecure_http: false,
        provider: Default::default(),
        config: None,
    };
    let svc = RequestExecutionService::new(
        Box::new(StaticEnvRepo(env)),
        Arc::new(SharedKeeping(Arc::clone(&executor))),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            collection,
        ))),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(FakeSecretManagerRepo(connection)),
        Arc::new(FakeSecretStore("client-secret".into())),
        Arc::new(RejectingFetcher),
    )
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()));
    (svc, executor)
}

/// Makes `auth/login` send the header `X-Key: {{vault.key}}`.
fn login_uses_the_alias(collection: &mut Collection) {
    for item in collection.root.items.iter_mut() {
        if let CollectionItem::Folder(auth) = item {
            for inner in auth.items.iter_mut() {
                if let CollectionItem::Request(login) = inner {
                    login.headers.push(Header::new("X-Key", "{{vault.key}}"));
                }
            }
        }
    }
}

const CATCH_RUN: &str = "try { await rok.runRequest('auth/login'); console.log('ran'); } \
                         catch (e) { console.log('err', e.message); }";

#[tokio::test]
async fn run_request_refuses_a_nested_request_that_uses_a_failed_alias() {
    let mut collection = main_and_login(CATCH_RUN, "", "");
    login_uses_the_alias(&mut collection);
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = failing_vault_service(collection);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out).join("\n");
    assert!(lines.contains("rok.runRequest:"), "{lines}");
    assert!(lines.contains("vault rejected the credentials"), "{lines}");
    assert!(
        !urls(&executor).iter().any(|u| u.contains("login.yml")),
        "{:?}",
        urls(&executor)
    );
}

#[tokio::test]
async fn run_request_runs_a_nested_request_that_does_not_use_the_failed_alias() {
    let collection = main_and_login(CATCH_RUN, "", "");
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = failing_vault_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"ran".to_string()),
        "{:?}",
        console(&out)
    );
    assert!(urls(&executor).iter().any(|u| u.contains("login.yml")));
}

#[tokio::test]
async fn run_request_inherits_failed_aliases_two_levels_deep() {
    // Main runs mid, which does not use the alias, and mid runs login, which does.
    let mut collection = main_and_login(
        "await rok.runRequest('mid'); console.log('mid said', rok.getVar('mid'));",
        "",
        "",
    );
    collection.root.add_request(saved(
        "Mid",
        "mid.yml",
        "try { await rok.runRequest('auth/login'); } catch (e) { rok.setVar('mid', e.message); }",
    ));
    login_uses_the_alias(&mut collection);
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = failing_vault_service(collection);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out).join("\n");
    assert!(lines.contains("rok.runRequest:"), "{lines}");
    assert!(lines.contains("vault rejected the credentials"), "{lines}");
    assert!(
        !urls(&executor).iter().any(|u| u.contains("login.yml")),
        "{:?}",
        urls(&executor)
    );
}

// ── rok.cookies against a real jar ───────────────────────────────────────────

use rocket_http::{CookieJarLock, CookieRepository};

/// A service whose executor, service and test share one jar folder and one jar lock.
fn cookie_service(dir: &std::path::Path, collection: Collection) -> RequestExecutionService {
    let lock = CookieJarLock::default();
    let repo: Arc<dyn CookieRepository> =
        Arc::new(rocket_infra::FsCookieRepo::new(dir.to_path_buf()));
    let executor =
        rocket_infra::ReqwestExecutor::new().with_cookie_repo_and_lock(repo, lock.clone());
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(executor),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            collection,
        ))),
        Box::new(rocket_infra::FsCookieRepo::new(dir.to_path_buf())),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_cookie_lock(lock)
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()))
}

/// Every cookie in the jars under `dir`, as sorted `name=value` texts.
fn jar_pairs(dir: &std::path::Path) -> Vec<String> {
    let mut pairs: Vec<String> = rocket_infra::FsCookieRepo::new(dir.to_path_buf())
        .get_all()
        .expect("read jars")
        .into_iter()
        .flat_map(|j| j.cookies)
        .map(|c| format!("{}={}", c.name, c.value))
        .collect();
    pairs.sort();
    pairs
}

/// A GET send of `<server>/main` with `pre` and `post` scripts.
fn cookie_input(server: &MockServer, pre: &str, post: &str) -> ExecuteRequestInput {
    let mut req = input(&format!("{}/main", server.uri()));
    req.method = HttpMethod::Get;
    if !pre.is_empty() {
        req.pre_request_script = Some(pre.to_string());
    }
    if !post.is_empty() {
        req.post_response_script = Some(post.to_string());
    }
    req
}

#[tokio::test]
async fn pre_request_script_cookie_is_sent_with_the_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/main"))
        .and(header("cookie", "sid=abc"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let req = cookie_input(&server, "await rok.cookies.upsert('sid', 'abc');", "");
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert_eq!(out.response.status, 200);
}

#[tokio::test]
async fn set_cookie_from_the_response_is_visible_to_the_post_response_script() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/main"))
        .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "tok=xyz; Path=/"))
        .mount(&server)
        .await;
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let req = cookie_input(&server, "", "console.log('tok', rok.cookies.get('tok'));");
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert!(
        console(&out).iter().any(|l| l == "tok xyz"),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn jar_reads_and_writes_cross_domains() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = server_with_main().await;
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let req = cookie_input(
        &server,
        "const jar = rok.cookies.jar(); \
         await jar.setCookie('https://other.test/', 'k', 'v'); \
         const c = await jar.getCookie('https://other.test/', 'k'); \
         console.log('got', c.key, c.value, c.domain); \
         console.log('own', rok.cookies.count());",
        "",
    );
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    let lines = console(&out);
    assert!(lines.iter().any(|l| l == "got k v other.test"), "{lines:?}");
    assert!(lines.iter().any(|l| l == "own 0"), "{lines:?}");
    assert_eq!(jar_pairs(dir.path()), vec!["k=v".to_string()]);
}

#[tokio::test]
async fn jar_clear_empties_the_whole_jar() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = server_with_main().await;
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let req = cookie_input(
        &server,
        "const jar = rok.cookies.jar(); \
         await jar.setCookie('https://a.test/', 'a', '1'); \
         await jar.setCookie('https://b.test/', 'b', '2'); \
         await rok.cookies.upsert('c', '3'); \
         await jar.clear(); \
         console.log('left', rok.cookies.count());",
        "",
    );
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert!(
        console(&out).iter().any(|l| l == "left 0"),
        "{:?}",
        console(&out)
    );
    assert!(jar_pairs(dir.path()).is_empty());
}

#[tokio::test]
async fn host_cookie_error_does_not_leak_the_value() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = server_with_main().await;
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let req = cookie_input(
        &server,
        "try { await rok.cookies.upsert('__Host-sid', 'topsecretvalue99', { domain: 'example.com' }); } \
         catch (e) { console.log('caught', e.message); }",
        "",
    );
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    let lines = console(&out);
    let caught = lines
        .iter()
        .find(|l| l.starts_with("caught"))
        .expect("caught line");
    assert!(caught.contains("__Host-"), "{caught}");
    assert!(!caught.contains("topsecretvalue99"), "{caught}");
    assert!(jar_pairs(dir.path()).is_empty());
}

#[tokio::test]
async fn cookies_are_unavailable_in_a_flow_transform_node() {
    use rocket_scripting::{ScriptContext, ScriptEngine};
    let engine = rocket_infra::scripting::DenoScriptEngine::new();
    let ctx = ScriptContext::before_request(
        "try { await rok.cookies.upsert('a', 'b'); } catch (e) { console.log(e.message); }".into(),
        Default::default(),
        HttpRequest::new(HttpMethod::Get, "https://example.com"),
        None,
        String::new(),
        vec![],
        vec![],
    );
    let result = engine.execute(ctx).await.expect("execute");
    let lines: Vec<String> = result
        .console_entries
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l == "rok.cookies is not available here"),
        "{lines:?} {:?}",
        result.error
    );
}

#[tokio::test]
async fn concurrent_script_and_request_writes_keep_all_cookies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = MockServer::start().await;
    for p in ["/main", "/side"] {
        Mock::given(method("GET"))
            .and(path(p))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "srv=1; Path=/"))
            .mount(&server)
            .await;
    }
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let req = cookie_input(
        &server,
        &format!(
            "const jobs = []; \
             for (let i = 0; i < 20; i++) jobs.push(rok.cookies.upsert('c' + i, String(i))); \
             jobs.push(rok.sendRequest({{ url: '{}/side' }})); \
             await Promise.all(jobs);",
            server.uri()
        ),
        "",
    );
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert_eq!(
        jar_pairs(dir.path()).len(),
        21,
        "{:?}",
        jar_pairs(dir.path())
    );
}

#[tokio::test]
async fn nested_run_request_sees_cookies_set_by_the_caller() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/child"))
        .and(header("cookie", "sid=abc"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/main"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut collection = Collection::new("api");
    let mut child = Request::new("Child", HttpMethod::Get, format!("{}/child", server.uri()));
    child.file_name = Some("child.yml".to_string());
    collection.root.add_request(child);
    let svc = cookie_service(dir.path(), collection);
    let mut req = cookie_input(
        &server,
        "await rok.cookies.upsert('sid', 'abc'); \
         const r = await rok.runRequest('child'); \
         console.log('child', r.status);",
        "",
    );
    req.collection = Some("api".into());
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert!(
        console(&out).iter().any(|l| l == "child 200"),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn a_cookie_with_a_newline_is_rejected_and_the_good_cookie_is_still_sent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/second"))
        .and(header("cookie", "good=1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let svc = cookie_service(dir.path(), Collection::new("api"));
    let script = format!(
        "await rok.cookies.upsert('good', '1'); \
         try {{ await rok.cookies.upsert('t', 'abc\\n'); console.log('no error'); }} \
         catch (e) {{ console.log('caught', e.message); }} \
         const r = await rok.sendRequest({{ url: '{}/second' }}); \
         console.log('status', r.status);",
        server.uri()
    );
    let out = svc
        .execute(cookie_input(&server, &script, ""))
        .await
        .expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    let lines = console(&out);
    let caught = lines
        .iter()
        .find(|l| l.starts_with("caught"))
        .unwrap_or_else(|| panic!("no caught line: {lines:?}"));
    assert!(!caught.contains("abc"), "{caught}");
    assert!(lines.iter().any(|l| l == "status 200"), "{lines:?}");
    assert_eq!(jar_pairs(dir.path()), vec!["good=1".to_string()]);
}
