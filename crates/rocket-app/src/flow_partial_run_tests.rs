//! End-to-end tests for partial runs ("Run this node", "Run from here").

use super::*;

use async_trait::async_trait;
use rocket_collection::Collection;
use rocket_flow::{FlowRepository, NodePosition};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::events::FlowPartialMode;

use crate::flow_auth::SuppliedToken;
use crate::flow_partial::PartialRun;
use crate::test_doubles::{
    EmptySecretManagerRepo, FakeCallbackListener, InMemoryCollectionRepo, InMemoryHistoryRepo,
    NullCookieRepo, NullEnvRepo, RecordingExecutor, RecordingPublisher, SharedCollectionRepo,
    SharedExecutor, SharedHistoryRepo, SharedPublisher,
};

const FLOW: &str = "partial";

/// A flow repo the test can edit between runs.
#[derive(Clone, Default)]
struct SharedFlowRepo(Arc<Mutex<HashMap<String, rocket_flow::Flow>>>);

impl SharedFlowRepo {
    fn put(&self, flow: rocket_flow::Flow) {
        self.0.lock().expect("lock").insert(flow.name.clone(), flow);
    }
    fn edit(&self, change: impl FnOnce(&mut rocket_flow::Flow)) {
        if let Some(flow) = self.0.lock().expect("lock").get_mut(FLOW) {
            change(flow);
        }
    }
}

impl FlowRepository for SharedFlowRepo {
    fn list(&self, _collection: &str) -> DomainResult<Vec<String>> {
        Ok(self.0.lock().expect("lock").keys().cloned().collect())
    }
    fn get(&self, _collection: &str, name: &str) -> DomainResult<rocket_flow::Flow> {
        self.0
            .lock()
            .expect("lock")
            .get(name)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(name.to_string()))
    }
    fn save(&self, _collection: &str, flow: &rocket_flow::Flow) -> DomainResult<()> {
        self.put(flow.clone());
        Ok(())
    }
    fn delete(&self, _collection: &str, name: &str) -> DomainResult<()> {
        self.0.lock().expect("lock").remove(name);
        Ok(())
    }
}

/// Answers every wire expression with the body of the response it reads.
struct BodyEngine;

#[async_trait]
impl ScriptEngine for BodyEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        let body = ctx
            .response
            .as_ref()
            .map(|r| r.body.clone())
            .unwrap_or_default();
        let mut vars = HashMap::new();
        vars.insert("__jsonq_result__".to_string(), serde_json::json!(body));
        Ok(ScriptResult {
            runtime_vars: vars,
            ..Default::default()
        })
    }
}

struct Harness {
    flows: SharedFlowRepo,
    http: Arc<RecordingExecutor>,
    events: Arc<RecordingPublisher>,
    callbacks: Arc<FakeCallbackListener>,
    service: FlowExecutionService,
    exec: RequestExecutionService,
}

fn harness(flow: rocket_flow::Flow) -> Harness {
    let flows = SharedFlowRepo::default();
    flows.put(flow);
    let http = RecordingExecutor::new();
    let events = RecordingPublisher::new();
    let callbacks = FakeCallbackListener::new();
    let service = FlowExecutionService::new(
        Box::new(flows.clone()),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("my-api"),
        ))),
        Box::new(SharedPublisher(Arc::clone(&events))),
    )
    .with_callback_listener(Box::new(Arc::clone(&callbacks)));
    let exec = RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(SharedExecutor(Arc::clone(&http))),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("my-api"),
        ))),
        Box::new(NullCookieRepo),
        Box::new(SharedPublisher(RecordingPublisher::new())),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(BodyEngine));
    Harness {
        flows,
        http,
        events,
        callbacks,
        service,
        exec,
    }
}

fn at(id: &str, kind: FlowNodeKind) -> FlowNode {
    FlowNode {
        id: id.to_string(),
        kind,
        position: NodePosition { x: 0.0, y: 0.0 },
    }
}

fn request(id: &str, url: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::Request {
            debug: false,
            repeat_until: None,
            label: id.to_string(),
            source: RequestSource::Inline {
                request: InlineRequestData {
                    method: "get".to_string(),
                    url: url.to_string(),
                    headers: Vec::new(),
                    body: None,
                },
            },
        },
    )
}

fn output(id: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::Output {
            label: id.to_string(),
        },
    )
}

fn wait(id: &str, name: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::WaitForCallback {
            label: id.to_string(),
            name: name.to_string(),
            timeout_ms: 60_000,
            accept_when: None,
        },
    )
}

fn sign_in(id: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::Auth {
            label: id.to_string(),
            auth: rocket_shared::types::Auth::OAuth2(Box::new(
                crate::flow_auth::test_support::authorization_code(),
            )),
            apply_to_inherit: true,
        },
    )
}

fn wire(id: &str, from: &str, to: &str, field: &str) -> FlowEdge {
    FlowEdge {
        id: id.to_string(),
        source_node_id: from.to_string(),
        target_node_id: to.to_string(),
        target_field: field.to_string(),
        expression: "response.body".to_string(),
        source_handle: handle::RESULT.to_string(),
    }
}

fn trigger(id: &str, from: &str, to: &str) -> FlowEdge {
    FlowEdge {
        id: id.to_string(),
        source_node_id: from.to_string(),
        target_node_id: to.to_string(),
        target_field: handle::TRIGGER.to_string(),
        expression: String::new(),
        source_handle: handle::RESULT.to_string(),
    }
}

fn flow(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> rocket_flow::Flow {
    rocket_flow::Flow {
        name: FLOW.to_string(),
        nodes,
        edges,
        callback_host: None,
    }
}

fn input() -> RunFlowInput {
    RunFlowInput {
        collection: "my-api".to_string(),
        flow_name: FLOW.to_string(),
        environment_name: None,
        global_env_name: None,
    }
}

fn partial(base: &str, start: &str, mode: FlowPartialMode) -> PartialRun {
    PartialRun {
        base_run_id: base.to_string(),
        start_node_id: start.to_string(),
        mode,
    }
}

fn tokens(token: &str) -> FlowAuthTokens {
    HashMap::from([(
        "au".to_string(),
        SuppliedToken {
            access_token: token.to_string(),
        },
    )])
}

fn node_ids(summary: &FlowRunSummary) -> Vec<&str> {
    summary.steps.iter().map(|s| s.node_id.as_str()).collect()
}

fn step<'s>(summary: &'s FlowRunSummary, id: &str) -> &'s FlowStepResult {
    summary
        .steps
        .iter()
        .find(|s| s.node_id == id)
        .expect("step recorded")
}

/// login -url-> b, where login answers with the URL b should call.
fn login_then_b() -> rocket_flow::Flow {
    flow(
        vec![
            request("a", "https://api.example.com/login"),
            request("b", "https://api.example.com/placeholder"),
        ],
        vec![wire("e1", "a", "b", "url")],
    )
}

#[tokio::test]
async fn run_this_node_reuses_the_cached_upstream_response() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");

    let summary = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect("partial run");

    let sent = h.http.sent_urls();
    assert_eq!(
        sent.iter().filter(|u| u.contains("/login")).count(),
        1,
        "the cached request is not sent again: {sent:?}"
    );
    assert!(sent.last().is_some_and(|u| u.contains("/profile")), "{sent:?}");
    assert_eq!(node_ids(&summary), vec!["b"]);
    assert_ne!(summary.run_id, base.run_id);
    let info = summary.partial.clone().expect("partial info");
    assert_eq!(info.base_run_id, base.run_id);
    assert_eq!(info.node_ids, vec!["b".to_string()]);
    let started = h
        .events
        .events()
        .into_iter()
        .find_map(|e| match e {
            DomainEvent::FlowRunStarted {
                run_id,
                total_nodes,
                partial,
                ..
            } if run_id == summary.run_id => Some((total_nodes, partial)),
            _ => None,
        })
        .expect("started event");
    assert_eq!(started, (1, Some(info)));
    let finished = h
        .events
        .events()
        .into_iter()
        .find_map(|e| match e {
            DomainEvent::FlowRunFinished {
                run_id, node_count, ..
            } if run_id == summary.run_id => Some(node_count),
            _ => None,
        })
        .expect("finished event");
    assert_eq!(finished, 1);
}

#[tokio::test]
async fn an_upstream_edit_since_the_base_run_refuses_with_no_events() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    h.flows.edit(|f| f.nodes[0] = request("a", "https://api.example.com/login?v=2"));
    let events_before = h.events.events().len();
    let sent_before = h.http.sent_urls().len();

    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("a changed");

    let text = err.to_string();
    assert!(text.contains("'a' changed since the earlier run"), "{text}");
    assert!(text.ends_with("node(s): a; edge(s): "), "{text}");
    assert_eq!(h.events.events().len(), events_before, "a refused run sends no events");
    assert_eq!(h.http.sent_urls().len(), sent_before);
}

#[tokio::test]
async fn editing_the_start_node_itself_is_allowed() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    let mut edited = request("b", "https://api.example.com/other");
    if let FlowNodeKind::Request {
        source: RequestSource::Inline { request },
        ..
    } = &mut edited.kind
    {
        request.method = "post".to_string();
        request.body = Some("edited-body".to_string());
    }
    h.flows.edit(|f| f.nodes[1] = edited);
    let sent_before = h.http.sent_urls().len();

    let summary = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect("only the start node changed");

    // The edited node ran with its new body, still fed by the cached login.
    assert_eq!(node_ids(&summary), vec!["b"]);
    assert_eq!(step(&summary, "b").status, FlowNodeStatus::Success);
    let sent = h.http.sent_urls();
    assert_eq!(sent.len(), sent_before + 1, "{sent:?}");
    assert!(sent.last().is_some_and(|u| u.contains("/profile")), "{sent:?}");
    assert_eq!(
        h.http.sent_bodies().last().cloned().flatten().as_deref(),
        Some("edited-body")
    );
}

#[tokio::test]
async fn a_rotated_token_in_a_cached_response_stays_masked() {
    // The login response echoes the token of the base run. The token has
    // rotated by the time the partial run reads that cached response.
    let h = harness(flow(
        vec![
            sign_in("au"),
            request("login", "https://api.example.com/login"),
            output("out"),
        ],
        vec![wire("e1", "login", "out", "value")],
    ));
    h.http.set_body("/login", r#"{"echo":"token-AAAA-123456"}"#);
    let base = h
        .service
        .run_with_auth(&h.exec, input(), tokens("token-AAAA-123456"))
        .await
        .expect("base run");
    let base_value = step(&base, "out").value.clone().expect("base value");
    assert!(!base_value.contains("token-AAAA-123456"), "{base_value}");

    let summary = h
        .service
        .run_partial(
            &h.exec,
            input(),
            tokens("token-BBBB-654321"),
            partial(&base.run_id, "out", FlowPartialMode::Node),
        )
        .await
        .expect("partial run");

    let value = step(&summary, "out").value.clone().expect("value");
    assert!(!value.contains("token-AAAA-123456"), "{value}");
    assert!(value.contains(crate::redaction::REDACTED), "{value}");
}

/// a -> b -> c -> d, each answering with the next URL.
fn four_chain() -> Harness {
    let h = harness(flow(
        vec![
            request("a", "https://api.example.com/n1"),
            request("b", "https://api.example.com/placeholder"),
            request("c", "https://api.example.com/placeholder"),
            request("d", "https://api.example.com/placeholder"),
        ],
        vec![
            wire("e1", "a", "b", "url"),
            wire("e2", "b", "c", "url"),
            wire("e3", "c", "d", "url"),
        ],
    ));
    h.http.set_body("/n1", "https://api.example.com/n2");
    h.http.set_body("/n2", "https://api.example.com/n3");
    h.http.set_body("/n3", "https://api.example.com/n4");
    h
}

#[tokio::test]
async fn two_partial_runs_from_one_base_both_work_and_leave_it_intact() {
    let h = four_chain();
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    let run_b = |base_id: String| {
        h.service.run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base_id, "b", FlowPartialMode::Node),
        )
    };
    let first = run_b(base.run_id.clone()).await.expect("first");
    let second = run_b(base.run_id.clone()).await.expect("second");
    assert_ne!(first.run_id, second.run_id);

    // Neither partial run changed the base, so c still has fresh inputs there.
    h.service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "d", FlowPartialMode::Node),
        )
        .await
        .expect("d from the base");

    // In the first partial run, c was not re-run after b changed.
    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&first.run_id, "d", FlowPartialMode::Node),
        )
        .await
        .expect_err("c is out of date there");
    let text = err.to_string();
    assert!(text.contains("is out of date"), "{text}");
    assert!(text.ends_with("node(s): c; edge(s): "), "{text}");
}

fn callback_call() -> crate::callback_listener::ReceivedCall {
    crate::callback_listener::ReceivedCall {
        method: "POST".to_string(),
        path: "/cb/0".to_string(),
        query: Vec::new(),
        headers: Vec::new(),
        body: "{}".to_string(),
    }
}

/// reg (sends {{callback.pay}}) -trigger-> w -value-> out.
fn callback_flow() -> rocket_flow::Flow {
    flow(
        vec![
            request("reg", "https://api.example.com/register?cb={{callback.pay}}"),
            wait("w", "pay"),
            output("out"),
        ],
        vec![trigger("e1", "reg", "w"), wire("e2", "w", "out", "value")],
    )
}

#[tokio::test]
async fn a_cancelled_partial_run_marks_unreached_nodes_stale() {
    let h = harness(callback_flow());
    h.callbacks.queue_on_open(callback_call());
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    assert_eq!(base.stopped_reason, "completed");

    let run = h.service.run_partial(
        &h.exec,
        input(),
        FlowAuthTokens::new(),
        partial(&base.run_id, "reg", FlowPartialMode::FromHere),
    );
    let base_id = base.run_id.clone();
    let stop = async {
        loop {
            let events = h.events.events();
            let partial_id = events.iter().find_map(|e| match e {
                DomainEvent::FlowRunStarted { run_id, .. } if *run_id != base_id => {
                    Some(run_id.clone())
                }
                _ => None,
            });
            if let Some(id) = partial_id {
                let waiting = events.iter().any(|e| {
                    matches!(e, DomainEvent::FlowStepStarted { run_id, node_id } if *run_id == id && node_id == "w")
                });
                if waiting {
                    h.service.cancel(&id);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let (summary, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(run, stop)
    })
    .await
    .expect("Stop must end the wait");
    let summary = summary.expect("partial run");

    assert_eq!(summary.stopped_reason, "cancelled");
    assert_eq!(node_ids(&summary), vec!["reg", "w"]);
    assert_eq!(step(&summary, "w").error.as_deref(), Some("cancelled"));

    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&summary.run_id, "out", FlowPartialMode::Node),
        )
        .await
        .expect_err("w never finished in that run");
    let text = err.to_string();
    assert!(text.contains("is out of date"), "{text}");
    assert!(text.ends_with("node(s): w; edge(s): "), "{text}");
}

#[tokio::test]
async fn run_from_a_wait_with_an_upstream_sender_is_refused() {
    let h = harness(callback_flow());
    h.callbacks.queue_on_open(callback_call());
    let base = h.service.run(&h.exec, input()).await.expect("base run");

    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "w", FlowPartialMode::FromHere),
        )
        .await
        .expect_err("reg would not send the new URL");
    let text = err.to_string();
    assert!(text.contains("is not part of this run"), "{text}");
    assert!(text.ends_with("node(s): w, reg; edge(s): "), "{text}");
}

#[tokio::test]
async fn an_unknown_base_another_environment_and_a_cleared_cache_are_refused() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");

    let unknown = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial("no-such-run", "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("unknown base");
    assert!(unknown.to_string().contains("no longer kept"), "{unknown}");

    let mut staging = input();
    staging.environment_name = Some("staging".to_string());
    let other_env = h
        .service
        .run_partial(
            &h.exec,
            staging,
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("another environment");
    assert!(other_env.to_string().contains("environment"), "{other_env}");

    h.service.clear_run_cache();
    let cleared = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("cache cleared");
    assert!(cleared.to_string().contains("no longer kept"), "{cleared}");
}
