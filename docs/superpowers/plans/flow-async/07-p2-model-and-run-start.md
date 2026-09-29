# Flow Async P2 — Plan 07: Wait for Callback Model and Run-Start Endpoints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `WaitForCallback` node kind and `Flow.callback_host` to the model, validation, persistence and IPC. Then open one callback endpoint per Wait for callback node when a run starts and expose each URL as `{{callback.<name>}}` to every request in the run.

**Architecture:** `rocket-flow` gains the new variant, three timeout constants, `CALLBACK_VAR_PREFIX` and the flow-level `callback_host`, with save-time rules in `validate.rs`. `ExecuteRequestInput` gains `flow_vars`, which `resolve_request` and `begin_phases` merge into the runtime scope, so `{{callback.<name>}}` resolves in URL, headers, body and scripts. A new `RunCallbacks` value in `rocket-app` opens every endpoint before the run starts, and `run` owns it, so every endpoint closes when `run` returns on any path. The node's own waiting logic comes in plan 08; until then its arm fails with a clear message.

**Tech Stack:** Rust (serde, serde_yaml, tokio), Tauri IPC DTOs.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §7.1, §7.2, §7.4 (run start). Interfaces are locked in `docs/superpowers/plans/flow-async/00-index.md` ("P2 — model", "P2 — IPC and TS").

## Global Constraints

- `name` matches `^[A-Za-z0-9_]+$` and is unique among Wait for callback nodes in a flow.
- `timeout_ms` is in `1000..=3_600_000`; the default is `60000`.
- The node accepts a `trigger` input and no data inputs. It has one exit, `result`.
- The run-scoped variable is `callback.<name>` = the endpoint URL. Values set by scripts still win, because runtime merging already works that way.
- Endpoints open before the first node runs, and close when the run ends on every path. A listener failure fails the whole run before any node runs, with `could not open callback listener: …`.
- `callback_host: None` means auto-detect the LAN IP.
- A flow without `callback_host` and without Wait for callback nodes saves byte-identically (`skip_serializing_if`).
- Persistence structs never get `#[serde(rename_all = "camelCase")]`; only IPC DTOs do.
- No panicking-unwrap calls in production paths. Cargo always `-j4`, one crate at a time.

## Review Focus

1. A flow with no Wait for callback node must open no listener and bind no port. → Task 3 test `a_flow_without_wait_nodes_opens_no_listener`.
2. Two Wait for callback nodes in one flow must get two different URLs, and each name must map to its own URL. → Task 3 test `two_wait_nodes_get_their_own_urls`.
3. A listener that cannot open must fail the run before `FlowRunStarted`, and leave nothing registered in `in_flight`. → Task 3 test `a_listener_failure_fails_the_run_before_it_starts`.
4. A script that sets `callback.payment` itself must keep its own value over the injected one (existing runtime precedence). → Task 2 test `a_pre_request_script_sees_flow_vars_as_runtime_vars` checks the value reaches the runtime scope; precedence is the existing `ScriptResult.runtime_vars` merge, unchanged.
5. An existing flow file (no `callback_host`, no new node kinds) must load and re-save byte-identically. → Task 1 test `flow_without_callback_host_saves_without_the_key`.

---

### Task 1: `WaitForCallback` kind, `callback_host`, validation, persistence and DTOs

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/node.rs`
- Modify: `crates/rocket-flow/src/flow.rs`
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `crates/rocket-flow/src/validate.rs`
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (interim `execute_node` arm)
- Modify: `src-tauri/src/commands/flow.rs`
- Modify: every `Flow { … }` struct literal the compiler reports (see Step 5)

**Interfaces:**
- Consumes: `FlowNodeKind::Request { …, repeat_until }` from plan 03 (patterns already use `..`).
- Produces:
  - `FlowNodeKind::WaitForCallback { label: String, name: String, timeout_ms: u64, accept_when: Option<String> }`.
  - `rocket_flow::{CALLBACK_DEFAULT_TIMEOUT_MS, CALLBACK_MIN_TIMEOUT_MS, CALLBACK_MAX_TIMEOUT_MS, CALLBACK_VAR_PREFIX}`.
  - `Flow.callback_host: Option<String>`.
  - `FlowNodeKindDto::WaitForCallback { label, name, timeout_ms, accept_when }` (JSON `timeoutMs`, `acceptWhen`) and `FlowDto.callback_host` (JSON `callbackHost`).

- [ ] **Step 1: Write the failing model tests**

Append to the `tests` module in `crates/rocket-flow/src/node.rs`:

```rust
    #[test]
    fn flow_node_kind_wait_for_callback_roundtrips_in_yaml() {
        let kind = FlowNodeKind::WaitForCallback {
            label: "Payment done".to_string(),
            name: "payment".to_string(),
            timeout_ms: 60_000,
            accept_when: Some("request.body.event === \"payment.completed\"".to_string()),
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(yaml.contains("kind: WaitForCallback"), "got:\n{yaml}");
        assert!(yaml.contains("timeout_ms: 60000"), "snake_case on disk, got:\n{yaml}");
        assert!(yaml.contains("accept_when:"), "got:\n{yaml}");
        let back: FlowNodeKind = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(kind, back);
    }

    #[test]
    fn wait_for_callback_without_accept_when_omits_the_key() {
        let kind = FlowNodeKind::WaitForCallback {
            label: "Hook".to_string(),
            name: "hook".to_string(),
            timeout_ms: CALLBACK_DEFAULT_TIMEOUT_MS,
            accept_when: None,
        };
        let yaml = serde_yaml::to_string(&kind).expect("serialize");
        assert!(!yaml.contains("accept_when"), "got:\n{yaml}");
        let back: FlowNodeKind = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(kind, back);
    }
```

`serde_yaml` is already a dev-dependency of `rocket-flow`.

Append to the existing `tests` module in `crates/rocket-flow/src/flow.rs`:

```rust
    #[test]
    fn flow_without_callback_host_saves_without_the_key() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: None,
        };
        let yaml = serde_yaml::to_string(&flow).expect("serialize");
        assert!(!yaml.contains("callback_host"), "got:\n{yaml}");
        let old_file = "name: f\nnodes: []\nedges: []\n";
        let loaded: Flow = serde_yaml::from_str(old_file).expect("an old file still loads");
        assert_eq!(loaded.callback_host, None);
    }

    #[test]
    fn flow_with_callback_host_roundtrips() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: Some("host.docker.internal".to_string()),
        };
        let yaml = serde_yaml::to_string(&flow).expect("serialize");
        assert!(yaml.contains("callback_host: host.docker.internal"), "got:\n{yaml}");
        let back: Flow = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(back, flow);
    }
```

- [ ] **Step 2: Write the failing validation tests**

In `crates/rocket-flow/src/validate.rs` tests, add a helper next to `if_node`:

```rust
    fn wait_node(id: &str, name: &str, timeout_ms: u64) -> FlowNode {
        node(
            id,
            FlowNodeKind::WaitForCallback {
                label: id.to_string(),
                name: name.to_string(),
                timeout_ms,
                accept_when: None,
            },
        )
    }
```

and these tests:

```rust
    #[test]
    fn a_wait_node_with_a_run_when_wire_is_valid() {
        let f = flow(
            vec![request("register"), wait_node("w", "payment", 60_000), request("use")],
            vec![
                edge("e1", "register", handle::RESULT, "w", handle::TRIGGER),
                edge("e2", "w", handle::RESULT, "use", "url"),
            ],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn a_wait_node_rejects_a_data_input() {
        let f = flow(
            vec![request("a"), wait_node("w", "payment", 60_000)],
            vec![edge("e1", "a", handle::RESULT, "w", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn a_wait_node_has_only_a_result_exit() {
        let f = flow(
            vec![wait_node("w", "payment", 60_000), request("b")],
            vec![edge("e1", "w", handle::TRUE, "b", handle::TRIGGER)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn a_wait_node_name_must_use_letters_digits_and_underscores() {
        for bad in ["", "pay ment", "pay-ment", "päy", "a.b"] {
            let f = flow(vec![wait_node("w", bad, 60_000)], Vec::new());
            assert_eq!(invalid_node_id(validate(&f)), "w", "name {bad:?} must be rejected");
        }
        let f = flow(vec![wait_node("w", "Payment_2", 60_000)], Vec::new());
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn wait_node_names_must_be_unique() {
        let f = flow(
            vec![wait_node("w1", "payment", 60_000), wait_node("w2", "payment", 60_000)],
            Vec::new(),
        );
        assert_eq!(invalid_node_id(validate(&f)), "w2");
    }

    #[test]
    fn a_wait_node_timeout_must_be_between_one_second_and_one_hour() {
        for bad in [0, 999, 3_600_001] {
            let f = flow(vec![wait_node("w", "payment", bad)], Vec::new());
            assert_eq!(invalid_node_id(validate(&f)), "w", "timeout {bad} must be rejected");
        }
        for good in [1000, 3_600_000] {
            let f = flow(vec![wait_node("w", "payment", good)], Vec::new());
            assert!(validate(&f).is_ok(), "timeout {good} is allowed");
        }
    }

    #[test]
    fn a_blank_accept_when_is_rejected() {
        let f = flow(
            vec![node(
                "w",
                FlowNodeKind::WaitForCallback {
                    label: "w".to_string(),
                    name: "payment".to_string(),
                    timeout_ms: 60_000,
                    accept_when: Some("   ".to_string()),
                },
            )],
            Vec::new(),
        );
        assert_eq!(invalid_node_id(validate(&f)), "w");
    }
```

The `flow(...)` helper in this test module builds a `Flow` literal; Step 5 adds `callback_host: None` to it.

- [ ] **Step 3: Write the failing persistence test**

Append to the `tests` module in `crates/rocket-infra/src/fs_flow_repo.rs`:

```rust
    #[test]
    fn wait_for_callback_node_and_callback_host_roundtrip_on_disk() {
        let (dir, repo) = setup();
        let mut flow = sample("Callback Flow");
        flow.callback_host = Some("host.docker.internal".to_string());
        flow.nodes.push(FlowNode {
            id: "wait-1".to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: "Payment done".to_string(),
                name: "payment".to_string(),
                timeout_ms: 60_000,
                accept_when: Some("request.body.ok".to_string()),
            },
            position: NodePosition { x: 300.0, y: 200.0 },
        });
        repo.save("acme", &flow).expect("save");

        let loaded = repo.get("acme", "Callback Flow").expect("get");
        assert_eq!(loaded, flow);
        let raw = fs::read_to_string(dir.path().join("acme").join("flows").join("callback-flow.yml"))
            .expect("read saved flow file");
        assert!(raw.contains("callback_host: host.docker.internal"), "got:\n{raw}");
        assert!(raw.contains("kind: WaitForCallback"), "got:\n{raw}");
        assert!(!raw.contains("timeoutMs"), "no camelCase on disk, got:\n{raw}");
    }
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow wait`
Expected: FAIL to compile — `no variant named WaitForCallback`, `cannot find value CALLBACK_DEFAULT_TIMEOUT_MS`, `struct Flow has no field named callback_host`.

- [ ] **Step 5: Implement the model**

In `crates/rocket-flow/src/node.rs`, add the variant after `Switch`:

```rust
    /// Waits for an inbound HTTP call on a run-scoped local URL. Requests
    /// use the URL as `{{callback.<name>}}`. Its output is the received call,
    /// shaped like a response.
    WaitForCallback {
        label: String,
        /// Letters, digits and `_`. Unique within the flow.
        name: String,
        timeout_ms: u64,
        /// Optional script condition over `request`. Calls that do not
        /// match are answered and ignored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accept_when: Option<String>,
    },
```

and the constants after the `is_false` helper:

```rust
/// Default, minimum and maximum `timeout_ms` of a Wait for callback node.
pub const CALLBACK_DEFAULT_TIMEOUT_MS: u64 = 60_000;
pub const CALLBACK_MIN_TIMEOUT_MS: u64 = 1000;
pub const CALLBACK_MAX_TIMEOUT_MS: u64 = 3_600_000;

/// Prefix of the run-scoped variable that holds a callback URL:
/// `callback.<name>`.
pub const CALLBACK_VAR_PREFIX: &str = "callback.";
```

Update the doc comment of `FlowNodeKind` to mention the new kind: add the sentence "`WaitForCallback` nodes wait for an inbound call on a local URL."

In `crates/rocket-flow/src/flow.rs`, add the field to `Flow`:

```rust
    /// Host used in callback URLs. `None` means this machine's LAN IP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_host: Option<String>,
```

In `crates/rocket-flow/src/lib.rs`, extend the `node` re-export:

```rust
pub use node::{
    FlowNodeKind, InlineHeader, InlineRequestData, NodePosition, RepeatUntil, RequestSource,
    SwitchCase, CALLBACK_DEFAULT_TIMEOUT_MS, CALLBACK_MAX_TIMEOUT_MS, CALLBACK_MIN_TIMEOUT_MS,
    CALLBACK_VAR_PREFIX,
};
```

(Keep whatever plan 03 already added to this list, such as `RepeatUntil`.)

Every `Flow { name, nodes, edges }` struct literal in the workspace now fails with `E0063 missing field callback_host`. There are about 70, all but two in tests. Fix them compiler-first, crate by crate, adding `callback_host: None,` as the last field of each literal:

```bash
cargo check -j4 -p rocket-flow --tests 2>&1 | grep -A3 "E0063"
cargo check -j4 -p rocket-infra --tests 2>&1 | grep -A3 "E0063"
cargo check -j4 -p rocket-app --tests 2>&1 | grep -A3 "E0063"
cargo check -j4 -p rocket --tests 2>&1 | grep -A3 "E0063"
```

Repeat each command until it prints nothing. The two production sites are `From<FlowDto> for Flow` (Step 7) and any `Flow { … }` built in `crates/rocket-app/src/flow_service.rs`; give the latter `callback_host: None` as well.

- [ ] **Step 6: Implement validation**

In `crates/rocket-flow/src/validate.rs`:

Import the constants at the top:

```rust
use crate::node::{CALLBACK_MAX_TIMEOUT_MS, CALLBACK_MIN_TIMEOUT_MS};
```

Extend `accepts_trigger` in `validate`:

```rust
        let accepts_trigger = matches!(
            target,
            FlowNodeKind::Request { .. }
                | FlowNodeKind::Output { .. }
                | FlowNodeKind::WaitForCallback { .. }
        );
        (edge.target_field == handle::TRIGGER && !accepts_trigger).then(|| {
            format!(
                "only Request, Output and Wait for callback nodes have a '{}' input",
                handle::TRIGGER
            )
        })
```

Add a new edge rule right after the "Input nodes cannot receive wires" rule:

```rust
    check_edges(flow, &kinds, |edge, target, _| {
        (matches!(target, FlowNodeKind::WaitForCallback { .. })
            && edge.target_field != handle::TRIGGER)
            .then(|| {
                format!(
                    "Wait for callback nodes only have a '{}' input",
                    handle::TRIGGER
                )
            })
    })?;
```

Add after `check_expressions(flow)?;`:

```rust
    check_wait_nodes(flow)?;
```

Extend `kind_name`:

```rust
        FlowNodeKind::WaitForCallback { .. } => "Wait for callback",
```

Extend `source_handle_exists` (the first arm):

```rust
        FlowNodeKind::Request { .. }
        | FlowNodeKind::Input { .. }
        | FlowNodeKind::WaitForCallback { .. } => source_handle == handle::RESULT,
```

Add the new rule function at the end of the non-test code:

```rust
/// V10: a Wait for callback node has a usable, unique name, a timeout in
/// range, and no blank `accept_when`.
fn check_wait_nodes(flow: &Flow) -> Result<(), FlowGraphError> {
    let mut seen = HashSet::new();
    for node in &flow.nodes {
        let FlowNodeKind::WaitForCallback {
            name,
            timeout_ms,
            accept_when,
            ..
        } = &node.kind
        else {
            continue;
        };
        let valid_name =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid_name {
            return Err(invalid_node(
                node,
                format!("the Wait for callback name '{name}' must use only letters, digits and _"),
            ));
        }
        if !seen.insert(name.as_str()) {
            return Err(invalid_node(
                node,
                format!("more than one Wait for callback node is named '{name}'"),
            ));
        }
        if !(CALLBACK_MIN_TIMEOUT_MS..=CALLBACK_MAX_TIMEOUT_MS).contains(timeout_ms) {
            return Err(invalid_node(
                node,
                format!(
                    "the Wait for callback timeout must be between {CALLBACK_MIN_TIMEOUT_MS} and {CALLBACK_MAX_TIMEOUT_MS} ms"
                ),
            ));
        }
        if accept_when.as_deref().is_some_and(|s| s.trim().is_empty()) {
            return Err(invalid_node(
                node,
                "the Wait for callback node's accept_when is empty".to_string(),
            ));
        }
    }
    Ok(())
}
```

Update the module doc comment's rule range from "V1-V10" (after plan 03) to "V1-V10".

- [ ] **Step 7: Keep the other crates compiling**

`execute_node` in `crates/rocket-app/src/flow_execution_service.rs` matches `FlowNodeKind` exhaustively. Add this arm after the `Switch` arm. Plan 08 replaces it with the real behaviour and a test proves the replacement:

```rust
            // Plan 08 implements waiting. Until then a run fails this node
            // with a clear reason instead of doing nothing.
            FlowNodeKind::WaitForCallback { .. } => Err(DomainError::InvalidInput(format!(
                "node '{}': Wait for callback nodes cannot run yet",
                node.id
            ))),
```

In `src-tauri/src/commands/flow.rs`, add the DTO variant after `Switch` in `FlowNodeKindDto`:

```rust
    WaitForCallback {
        label: String,
        name: String,
        timeout_ms: u64,
        #[serde(default)]
        accept_when: Option<String>,
    },
```

and the two conversion arms. In `From<FlowNodeKind> for FlowNodeKindDto`:

```rust
            FlowNodeKind::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            } => FlowNodeKindDto::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            },
```

In `From<FlowNodeKindDto> for FlowNodeKind`:

```rust
            FlowNodeKindDto::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            } => FlowNodeKind::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            },
```

Add to `FlowDto`:

```rust
    #[serde(default)]
    pub callback_host: Option<String>,
```

and carry it in both conversions: `callback_host: f.callback_host,` in `From<Flow> for FlowDto` and in `From<FlowDto> for Flow`.

Add a DTO test to the `tests` module of `src-tauri/src/commands/flow.rs`:

```rust
    #[test]
    fn wait_for_callback_dto_uses_camel_case_json_and_roundtrips() {
        let json = r#"{
            "name": "cb",
            "callbackHost": "host.docker.internal",
            "nodes": [{
                "id": "w",
                "kind": { "kind": "WaitForCallback", "label": "Hook", "name": "payment",
                          "timeoutMs": 60000, "acceptWhen": "request.body.ok" },
                "position": { "x": 0, "y": 0 }
            }],
            "edges": []
        }"#;
        let dto: FlowDto = serde_json::from_str(json).expect("parse");
        let flow: Flow = dto.into();
        assert_eq!(flow.callback_host.as_deref(), Some("host.docker.internal"));
        assert_eq!(
            flow.nodes[0].kind,
            FlowNodeKind::WaitForCallback {
                label: "Hook".to_string(),
                name: "payment".to_string(),
                timeout_ms: 60_000,
                accept_when: Some("request.body.ok".to_string()),
            }
        );
        let back = serde_json::to_string(&FlowDto::from(flow)).expect("serialize");
        assert!(back.contains("\"timeoutMs\":60000"), "got: {back}");
        assert!(back.contains("\"callbackHost\""), "got: {back}");
    }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS, including the 2 node tests, 2 flow tests and 7 validation tests above.

Run: `cargo test -j4 -p rocket-infra wait_for_callback_node_and_callback_host_roundtrip_on_disk`
Expected: PASS.

Run: `cargo test -j4 -p rocket wait_for_callback_dto_uses_camel_case_json_and_roundtrips`
Expected: PASS.

Run: `cargo check -j4 -p rocket-app --tests` and `cargo check -j4 -p rocket --tests`
Expected: no errors.

- [ ] **Step 9: Commit**

Stage every changed file (the `E0063` fixes span many test modules), then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add Wait for callback node model`.

---

### Task 2: `flow_vars` on `ExecuteRequestInput`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (struct, `resolve_request`, `begin_phases`, tests)
- Modify: `crates/rocket-app/src/runner_sequence.rs`, `crates/rocket-app/src/load_test_service.rs`, `crates/rocket-app/src/flow_execution_service.rs` (struct literals)

**Interfaces:**
- Consumes: `ExecuteRequestInput.skip_history` from plan 03/04 (struct literals already carry it).
- Produces: `ExecuteRequestInput.flow_vars: HashMap<String, String>` (`#[serde(default)]`). Values resolve `{{name}}` in URL, auth, headers and body, and appear in `VariableContext.runtime` for every script phase.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module of `crates/rocket-app/src/execution_service.rs`, after `resolve_request_populates_global_env_scope_for_url_resolution`:

```rust
    /// Records the whole request the executor was handed.
    struct CapturingExecutor {
        sent: Mutex<Option<HttpRequest>>,
    }

    #[async_trait]
    impl HttpExecutor for CapturingExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.sent.lock().expect("lock") = Some(req.clone());
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
            })
        }
    }

    fn callback_vars() -> std::collections::HashMap<String, String> {
        std::collections::HashMap::from([(
            "callback.payment".to_string(),
            "http://10.0.0.5:4000/cb/abc".to_string(),
        )])
    }

    #[tokio::test]
    async fn flow_vars_resolve_in_url_header_and_body() {
        let executor = Arc::new(CapturingExecutor {
            sent: Mutex::new(None),
        });
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("unused"))),
            Arc::clone(&executor) as Arc<dyn HttpExecutor>,
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let mut input = sample_input("https://api.example.com/register?cb={{callback.payment}}", None);
        input.headers = vec![rocket_shared::types::Header {
            key: "X-Callback".to_string(),
            value: "{{callback.payment}}".to_string(),
            enabled: true,
            description: None,
        }];
        input.body = Some(rocket_shared::types::Body {
            mode: rocket_shared::types::BodyMode::Json,
            content: Some(r#"{"callbackUrl":"{{callback.payment}}"}"#.to_string()),
            form_data: None,
            file_path: None,
        });
        input.flow_vars = callback_vars();

        svc.execute(input).await.expect("execute");

        let sent = executor.sent.lock().expect("lock").clone().expect("a sent request");
        assert_eq!(sent.url, "https://api.example.com/register?cb=http://10.0.0.5:4000/cb/abc");
        assert_eq!(sent.headers[0].value, "http://10.0.0.5:4000/cb/abc");
        assert_eq!(
            sent.body.and_then(|b| b.content).as_deref(),
            Some(r#"{"callbackUrl":"http://10.0.0.5:4000/cb/abc"}"#)
        );
    }

    /// Records the runtime scope the pre-request script was given.
    struct RuntimeCapturingEngine {
        runtime: Mutex<Option<std::collections::HashMap<String, String>>>,
    }

    #[async_trait]
    impl ScriptEngine for RuntimeCapturingEngine {
        async fn execute(
            &self,
            ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            if ctx.phase == rocket_scripting::ScriptPhase::BeforeRequest {
                *self.runtime.lock().expect("lock") = Some(ctx.variables.runtime.clone());
            }
            Ok(ScriptResult::default())
        }
    }

    #[tokio::test]
    async fn a_pre_request_script_sees_flow_vars_as_runtime_vars() {
        let engine = Arc::new(RuntimeCapturingEngine {
            runtime: Mutex::new(None),
        });
        struct SharedEngine(Arc<RuntimeCapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedEngine {
            async fn execute(
                &self,
                ctx: ScriptContext,
            ) -> rocket_shared::error::DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("unused"))),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));
        let mut input = sample_input("https://api.example.com", None);
        input.pre_request_script = Some("console.log(1)".to_string());
        input.flow_vars = callback_vars();

        svc.execute(input).await.expect("execute");

        let runtime = engine.runtime.lock().expect("lock").clone().expect("pre-request ran");
        assert_eq!(
            runtime.get("callback.payment").map(String::as_str),
            Some("http://10.0.0.5:4000/cb/abc")
        );
    }
```

`ScriptContext`, `ScriptEngine` and `ScriptResult` come from the `use rocket_scripting::{…}` line near `MockScriptEngine` in the same test module; `use` items apply to the whole module, so the position of the new tests does not matter.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_vars`
Expected: FAIL to compile — `no field flow_vars on type ExecuteRequestInput`.

- [ ] **Step 3: Implement**

Add the field at the end of `ExecuteRequestInput` in `crates/rocket-app/src/execution_service.rs`:

```rust
    /// Run-scoped variables a Flow run adds, such as `callback.<name>`.
    /// They resolve like runtime variables. Empty for every other caller.
    #[serde(default)]
    pub flow_vars: std::collections::HashMap<String, String>,
```

In `resolve_request`, make the variable map mutable and add the flow vars last, so they win over every stored scope like runtime variables do:

```rust
        let mut vars = self.build_variable_context(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            external_secrets,
        );
        vars.extend(input.flow_vars.clone());
```

In `begin_phases`, make `var_ctx` mutable and merge right after it is built:

```rust
        let mut var_ctx = self.build_variable_scopes(
            input.global_env_name.as_deref(),
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
            external_secrets,
        );
        // Flow run variables (e.g. `callback.<name>`) behave like runtime
        // variables. A script that sets the same key later still wins.
        var_ctx.runtime.extend(input.flow_vars.clone());
```

Add `flow_vars: std::collections::HashMap::new(),` (or `flow_vars: Default::default(),`) to every other `ExecuteRequestInput { … }` literal. The compiler lists them:

```bash
cargo check -j4 -p rocket-app --tests 2>&1 | grep -A3 "E0063"
```

Today they are in `execution_service.rs` (3), `runner_sequence.rs` (2), `load_test_service.rs` (1) and `flow_execution_service.rs` (1). Repeat until the command prints nothing, then run `cargo check -j4 -p rocket --tests` for `src-tauri`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_vars`
Expected: PASS — `flow_vars_resolve_in_url_header_and_body`, `a_pre_request_script_sees_flow_vars_as_runtime_vars`.

Run: `cargo test -j4 -p rocket-app execution_service`
Expected: PASS (no regressions in the existing resolution tests).

- [ ] **Step 5: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): resolve run-scoped flow variables in requests`.

---

### Task 3: Open callback endpoints at run start

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/flow_callbacks.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`run`, `execute_node` signature and Request arm, tests)

**Interfaces:**
- Consumes: `CallbackListener`, `CallbackEndpoint`, `FakeCallbackListener` (plan 06); `Flow.callback_host`, `CALLBACK_VAR_PREFIX` (Task 1); `ExecuteRequestInput.flow_vars` (Task 2); `NodeRunContext` and the `_ctx` parameter of `execute_node` (plan 01).
- Produces (contract extension, recorded in `00-index.md`):

```rust
// crates/rocket-app/src/flow_callbacks.rs
pub(crate) struct RunCallbacks { /* vars + endpoints by node id */ }
impl RunCallbacks {
    pub(crate) async fn open_all(listener: &dyn CallbackListener, flow: &Flow) -> DomainResult<Self>;
    pub(crate) fn vars(&self) -> &HashMap<String, String>;
    pub(crate) fn endpoint_mut(&mut self, node_id: &str) -> Option<&mut CallbackEndpoint>;
}
```

  `execute_node` gains the parameter `callbacks: &mut RunCallbacks`, placed after the plan 01 context parameter. `run` opens endpoints before inserting into `in_flight` and before `FlowRunStarted`.

- [ ] **Step 1: Write the failing tests**

Add helpers to the `tests` module of `crates/rocket-app/src/flow_execution_service.rs` (next to `request_flow_node`):

```rust
    fn wait_node(id: &str, name: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: format!("Wait {id}"),
                name: name.to_string(),
                timeout_ms: 1000,
                accept_when: None,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn service_with_listener(
        flow: Flow,
        fake: &Arc<crate::test_doubles::FakeCallbackListener>,
    ) -> FlowExecutionService {
        service_with_flow(flow).with_callback_listener(Box::new(Arc::clone(fake)))
    }
```

and the tests:

```rust
    #[tokio::test]
    async fn a_request_url_resolves_the_callback_variable() {
        let flow = Flow {
            name: "cb".to_string(),
            nodes: vec![
                request_flow_node("reg", "https://api.example.com/register?cb={{callback.payment}}"),
                wait_node("w", "payment"),
            ],
            edges: vec![trigger_edge("e1", "reg", handle::RESULT, "w")],
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/register?cb=http://fake:1/cb/0".to_string()]
        );
    }

    #[tokio::test]
    async fn two_wait_nodes_get_their_own_urls() {
        let flow = Flow {
            name: "two".to_string(),
            nodes: vec![
                wait_node("w1", "first"),
                wait_node("w2", "second"),
                request_flow_node(
                    "reg",
                    "https://api.example.com/r?a={{callback.first}}&b={{callback.second}}",
                ),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("two"))
            .await
            .expect("run");

        assert_eq!(fake.opened_count(), 2);
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/r?a=http://fake:1/cb/0&b=http://fake:1/cb/1".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn a_flow_without_wait_nodes_opens_no_listener() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(linear_flow(), &fake)
            .run(&exec, run_input("auth-flow"))
            .await
            .expect("run");

        assert_eq!(fake.opened_count(), 0);
    }

    #[tokio::test]
    async fn the_flow_callback_host_is_passed_to_the_listener() {
        let flow = Flow {
            name: "host".to_string(),
            nodes: vec![wait_node("w", "payment")],
            edges: Vec::new(),
            callback_host: Some("host.docker.internal".to_string()),
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("host"))
            .await
            .expect("run");

        assert_eq!(fake.hosts(), vec![Some("host.docker.internal".to_string())]);
    }

    #[tokio::test]
    async fn every_endpoint_is_closed_when_the_run_ends() {
        let flow = Flow {
            name: "close".to_string(),
            nodes: vec![wait_node("w", "payment")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        service_with_listener(flow, &fake)
            .run(&exec, run_input("close"))
            .await
            .expect("run");

        assert!(fake.is_closed(0));
    }

    #[tokio::test]
    async fn a_listener_failure_fails_the_run_before_it_starts() {
        let flow = Flow {
            name: "fail".to_string(),
            nodes: vec![wait_node("w", "payment")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = crate::test_doubles::FakeCallbackListener::failing("port in use");
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher)
            .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let err = service
            .run(&exec, run_input("fail"))
            .await
            .err()
            .expect("the run must fail");

        let message = err.to_string();
        assert!(message.contains("could not open callback listener"), "got: {message}");
        assert!(message.contains("port in use"), "got: {message}");
        assert!(
            publisher.events().is_empty(),
            "no FlowRunStarted or any other event for a run that never started"
        );
        assert!(service.in_flight.lock().expect("lock").is_empty());
    }
```

`RecordingPublisher`, `service_with_publisher`, `trigger_edge` and `linear_flow` already exist in this test module.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app callback`
Expected: the new tests FAIL — `a_request_url_resolves_the_callback_variable` sends the literal `{{callback.payment}}`, `a_listener_failure_fails_the_run_before_it_starts` returns `Ok`, `opened_count()` is 0 where 2 is expected.

- [ ] **Step 3: Write `RunCallbacks`**

Create `crates/rocket-app/src/flow_callbacks.rs`:

```rust
//! Callback endpoints for one Flow run. `run` owns a `RunCallbacks`, so
//! every endpoint closes when the run ends, on any path.

use std::collections::HashMap;

use rocket_flow::{Flow, FlowNodeKind, CALLBACK_VAR_PREFIX};
use rocket_shared::error::{DomainError, DomainResult};

use crate::callback_listener::{CallbackEndpoint, CallbackListener};

pub(crate) struct RunCallbacks {
    /// `callback.<name>` → endpoint URL, for every request in the run.
    vars: HashMap<String, String>,
    /// Open endpoints by Wait for callback node id.
    endpoints: HashMap<String, CallbackEndpoint>,
}

impl RunCallbacks {
    /// Opens one endpoint per Wait for callback node, in node order. A flow
    /// without such nodes opens nothing.
    pub(crate) async fn open_all(listener: &dyn CallbackListener, flow: &Flow) -> DomainResult<Self> {
        let mut vars = HashMap::new();
        let mut endpoints = HashMap::new();
        for node in &flow.nodes {
            let FlowNodeKind::WaitForCallback { name, .. } = &node.kind else {
                continue;
            };
            let endpoint = listener
                .open(flow.callback_host.as_deref())
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("could not open callback listener: {e}"))
                })?;
            vars.insert(format!("{CALLBACK_VAR_PREFIX}{name}"), endpoint.url.clone());
            endpoints.insert(node.id.clone(), endpoint);
        }
        Ok(Self { vars, endpoints })
    }

    pub(crate) fn vars(&self) -> &HashMap<String, String> {
        &self.vars
    }

    pub(crate) fn endpoint_mut(&mut self, node_id: &str) -> Option<&mut CallbackEndpoint> {
        self.endpoints.get_mut(node_id)
    }
}
```

`endpoint_mut` is first used in plan 08. Add `#[cfg_attr(not(test), allow(dead_code))]` above it now; plan 08 removes the attribute.

Register the module in `crates/rocket-app/src/lib.rs`, after `pub(crate) mod flow_debug;`:

```rust
pub(crate) mod flow_callbacks;
```

- [ ] **Step 4: Open endpoints in `run` and pass variables to requests**

In `FlowExecutionService::run`, right after `nodes_by_id` is built and **before** `let run_id = Ulid::new().to_string();`:

```rust
        // Open every callback endpoint before the run is registered or
        // announced. A failure here ends the call with no events and nothing
        // left in `in_flight`. `callbacks` lives until `run` returns, so
        // every endpoint closes on every exit path.
        let mut callbacks = crate::flow_callbacks::RunCallbacks::open_all(
            self.callback_listener.as_ref(),
            &flow,
        )
        .await?;
```

Add the parameter to `execute_node`, after the plan 01 context parameter:

```rust
        callbacks: &mut crate::flow_callbacks::RunCallbacks,
```

and pass `&mut callbacks` at the single call site in `run`, after the context argument.

In the `FlowNodeKind::Request` arm of `execute_node`, right after `build_execute_request_input(…)?` returns `request_input`:

```rust
                // Callback URLs (`{{callback.<name>}}`) resolve in every
                // field and script of every request in the run.
                request_input.flow_vars = callbacks.vars().clone();
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app callback`
Expected: PASS, including the six tests from Step 1 and plan 06's `with_callback_listener_replaces_the_default_listener`.

Run: `cargo test -j4 -p rocket-app flow_execution_service`
Expected: PASS (existing run tests unchanged).

Run: `cargo check -j4 -p rocket --tests`
Expected: no errors.

- [ ] **Step 6: Commit**

Stage `crates/rocket-app/src/flow_callbacks.rs`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/src/flow_execution_service.rs`, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): open callback endpoints when a run starts`.
