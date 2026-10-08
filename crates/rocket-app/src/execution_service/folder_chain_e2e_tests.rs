//! Runs a request two folders deep through the real `FsCollectionRepo`, the real
//! `ReqwestExecutor` and a wiremock server. Scripts go to a recording engine, because only the
//! order in which code reaches the engine is under test.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocket_collection::{
    CollectionRepository, CollectionSettings, FolderSettings, Request, ScriptFlow,
};
use rocket_infra::{FsCollectionRepo, ReqwestExecutor};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::DomainResult;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::{Auth, Header, HttpMethod};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::RequestExecutionService;
use crate::graphql_request::test_support::input;
use crate::test_doubles::{
    EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo, SharedHistoryRepo,
};

/// Records `(phase, code)` for every engine call and returns an empty result.
#[derive(Default)]
struct CodeRecorder {
    calls: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl ScriptEngine for CodeRecorder {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        self.calls
            .lock()
            .expect("lock")
            .push((ctx.phase.as_str().to_string(), ctx.code));
        Ok(ScriptResult::default())
    }
}

/// Hands one `Arc<CodeRecorder>` to a service expecting a `Box<dyn ScriptEngine>`.
struct SharedRecorder(Arc<CodeRecorder>);

#[async_trait]
impl ScriptEngine for SharedRecorder {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        self.0.execute(ctx).await
    }
}

/// Builds the collection `api` with folders `outer` and `outer/inner`, sends `outer/inner/ping.yml`
/// to a wiremock server, and returns the request the server saw plus the recorded script calls.
async fn run_ping(flow: ScriptFlow) -> (wiremock::Request, Vec<(String, String)>) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.create("api").expect("create collection");
    repo.save_settings(
        "api",
        &CollectionSettings {
            headers: vec![Header::new("X-Col", "col"), Header::new("X-Shared", "col")],
            auth: Some(Auth::Bearer {
                token: "col-token".into(),
            }),
            script_flow: flow,
            ..Default::default()
        },
    )
    .expect("save collection settings");
    repo.create_folder("api", "outer").expect("create outer");
    repo.create_folder("api", "outer/inner")
        .expect("create inner");
    repo.save_folder_settings(
        "api",
        "outer",
        &FolderSettings {
            headers: vec![
                Header::new("X-Outer", "outer"),
                Header::new("X-Shared", "outer"),
                // A disabled folder header must not shadow the collection's X-Col.
                Header::disabled("X-Col", "shadow"),
            ],
            auth: Some(Auth::Basic {
                username: "u".into(),
                password: "p".into(),
            }),
            pre_request_script: Some("// outer-pre".into()),
            post_response_script: Some("// outer-post".into()),
            tests_script: Some("// outer-tests".into()),
            ..Default::default()
        },
    )
    .expect("save outer settings");
    repo.save_folder_settings(
        "api",
        "outer/inner",
        &FolderSettings {
            headers: vec![
                Header::new("X-Inner", "inner"),
                Header::new("X-Shared", "inner"),
            ],
            auth: Some(Auth::Bearer {
                token: "inner-token".into(),
            }),
            pre_request_script: Some("// inner-pre".into()),
            post_response_script: Some("// inner-post".into()),
            tests_script: Some("// inner-tests".into()),
            ..Default::default()
        },
    )
    .expect("save inner settings");

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let url = format!("{}/ping", server.uri());
    let rel = repo
        .save_request(
            "api",
            "outer/inner/ping.yml",
            &Request::new("Ping", HttpMethod::Get, url.clone()),
        )
        .expect("save request");

    let engine = Arc::new(CodeRecorder::default());
    let svc = RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(ReqwestExecutor::new()),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(repo),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(SharedRecorder(Arc::clone(&engine))));

    let mut req = input(&url);
    req.method = HttpMethod::Get;
    req.headers = vec![
        Header::new("X-Req", "req"),
        // The request replaces the inner folder's X-Inner.
        Header::new("X-Inner", "request"),
    ];
    req.auth = Auth::Inherit;
    req.collection = Some("api".into());
    req.request_path = Some(rel);
    req.request_name = Some("Ping".into());
    req.pre_request_script = Some("// req-pre".into());
    req.post_response_script = Some("// req-post".into());
    req.tests_script = Some("// req-tests".into());
    svc.execute(req).await.expect("execute");

    let mut seen = server
        .received_requests()
        .await
        .expect("request recording is on");
    assert_eq!(seen.len(), 1, "exactly one request reaches the server");
    let calls = engine.calls.lock().expect("lock").clone();
    (seen.remove(0), calls)
}

fn header<'r>(request: &'r wiremock::Request, name: &str) -> Option<&'r str> {
    request.headers.get(name).and_then(|v| v.to_str().ok())
}

/// The markers found in the code handed over for `phase`, in the order they appear.
fn order(calls: &[(String, String)], phase: &str, markers: &[&str]) -> Vec<String> {
    let text = calls
        .iter()
        .filter(|(p, _)| p == phase)
        .map(|(_, code)| code.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut found: Vec<(usize, &str)> = markers
        .iter()
        .filter_map(|m| text.find(m).map(|at| (at, *m)))
        .collect();
    found.sort_unstable();
    found.into_iter().map(|(_, m)| m.to_string()).collect()
}

#[tokio::test]
async fn nested_request_gets_merged_headers_inherited_auth_and_sandwich_scripts() {
    let (received, calls) = run_ping(ScriptFlow::Sandwich).await;

    assert_eq!(
        header(&received, "X-Col"),
        Some("col"),
        "a disabled folder header must not shadow the collection header"
    );
    assert_eq!(header(&received, "X-Outer"), Some("outer"));
    assert_eq!(
        header(&received, "X-Shared"),
        Some("inner"),
        "inner beats outer"
    );
    assert_eq!(
        header(&received, "X-Inner"),
        Some("request"),
        "request wins"
    );
    assert_eq!(header(&received, "X-Req"), Some("req"));
    assert_eq!(
        header(&received, "Authorization"),
        Some("Bearer inner-token"),
        "the innermost folder auth beats the outer folder and the collection"
    );

    assert_eq!(
        order(
            &calls,
            "before-request",
            &["outer-pre", "inner-pre", "req-pre"]
        ),
        vec!["outer-pre", "inner-pre", "req-pre"]
    );
    assert_eq!(
        order(
            &calls,
            "after-response",
            &["outer-post", "inner-post", "req-post"]
        ),
        vec!["req-post", "inner-post", "outer-post"]
    );
    assert_eq!(
        order(
            &calls,
            "tests",
            &["outer-tests", "inner-tests", "req-tests"]
        ),
        vec!["req-tests", "inner-tests", "outer-tests"]
    );
}

#[tokio::test]
async fn sequential_flow_runs_every_phase_outer_to_inner() {
    let (_received, calls) = run_ping(ScriptFlow::Sequential).await;

    assert_eq!(
        order(
            &calls,
            "before-request",
            &["outer-pre", "inner-pre", "req-pre"]
        ),
        vec!["outer-pre", "inner-pre", "req-pre"]
    );
    assert_eq!(
        order(
            &calls,
            "after-response",
            &["outer-post", "inner-post", "req-post"]
        ),
        vec!["outer-post", "inner-post", "req-post"]
    );
    assert_eq!(
        order(
            &calls,
            "tests",
            &["outer-tests", "inner-tests", "req-tests"]
        ),
        vec!["outer-tests", "inner-tests", "req-tests"]
    );
}
