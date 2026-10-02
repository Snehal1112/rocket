# Flow Auth Node — Plan 3: Executor behavior

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 3 of 7.** Previous plan: `docs/superpowers/plans/2026-10-02-flow-auth-node-02-credentials.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-04-ipc.md`**
**Recommended model: Opus** (touches the run loop and secret masking).

**Goal:** Make Auth nodes actually do something at run time: the node reports success without leaking its token, requests set to `inherit` use the flow's credential, and an explicit `auth` wire sets a request's auth.

**Architecture:** Plan 2 left a `credentials: FlowCredentials` local in `run_with_auth`. This plan passes it to `execute_node`, replaces the placeholder `Auth` arm, applies the credential in the `Request` arm (inherit substitution first, then explicit `auth` wires), and masks Auth tokens in an Output node's reported value.

**Tech Stack:** Rust, tokio tests, the existing `RecordingExecutor` test double.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- A request with its own auth (not `inherit`) is never replaced by auto-apply; an explicit `auth` wire overrides everything.
- Tokens never appear in any step `value`, `debug_request` or `exchange`.
- No `unwrap()` in production Rust paths; never shell out to `git`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs.
- Conventional commits.
- Before each commit run `cargo fmt` (the plan's code is not rustfmt-checked) and re-run the task's tests.
- Verification per task: `cargo check -j4`, `cargo test -p rocket-app -j4 flow_execution_service`.

## Preconditions

- The uncommitted `inherit` fix (`runner_sequence.rs::build_step_input` preferring `runtime_auth`) must be committed. Task 2's tests rely on a saved request with `runtime_auth = Some(Auth::Inherit)` reaching `execute_node` as `Auth::Inherit`.
- Plan 2 is merged: `FlowCredentials`, `run_with_auth`, `with_token_fetcher` exist.

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/test_doubles.rs` | `RecordingExecutor` also records each request's `Auth` |
| `crates/rocket-app/src/flow_execution_service.rs` | `execute_node` gets `credentials`; `Auth` arm; Output masking; inherit substitution; `auth` wire; tests |

---

### Task 1: Auth node execution and Output masking

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `FlowCredentials::{auth_for_node, wire_value, secret_forms}` (Plan 2), local `credentials` in `run_with_auth`.
- Produces: `execute_node(..., external_secrets, credentials: &FlowCredentials, logs, ...)` — Task 2 and 3 use `credentials` inside the `Request` arm.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing test**

In `flow_execution_service.rs` `mod tests`, below the Plan 2 test `a_non_interactive_auth_node_is_fetched_by_the_fetcher`, add:

```rust
    #[tokio::test]
    async fn an_auth_node_succeeds_without_reporting_its_token_and_an_output_masks_it() {
        use rocket_shared::types::Auth;

        let flow = Flow {
            name: "auth-output".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Auth {
                        label: "Sign in".to_string(),
                        auth: Auth::Bearer {
                            token: "static-token-123456".to_string(),
                        },
                        apply_to_inherit: false,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "b".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Token".to_string(),
                    },
                    position: NodePosition { x: 100.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdge {
                id: "e1".to_string(),
                source_node_id: "a".to_string(),
                target_node_id: "b".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        // The engine answers every wire expression with the token, as
        // `response.body` would for the Auth node's wire value.
        let exec = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("static-token-123456"),
            }),
        );

        let summary = service
            .run(&exec, run_input("auth-output"))
            .await
            .expect("run must succeed");

        let step = |id: &str| {
            summary
                .steps
                .iter()
                .find(|s| s.node_id == id)
                .expect("step recorded")
        };
        assert_eq!(step("a").status, FlowNodeStatus::Success);
        assert_eq!(step("a").value, None, "an Auth node never reports its token");
        assert_eq!(
            step("b").value.as_deref(),
            Some(crate::redaction::REDACTED),
            "an Output wired to an Auth node shows the token masked"
        );
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -p rocket-app -j4 an_auth_node_succeeds`
Expected: FAIL — the Auth step is `Failed` with "cannot run yet".

- [ ] **Step 4: Pass `credentials` into `execute_node`**

Add the import: extend the `use crate::flow_auth::{...}` line from Plan 2 with `FlowCredentials`.

`FlowCredentials` is `pub(crate)`; `execute_node` is a private method, so no visibility change is needed.

In the `execute_node` signature, after `external_secrets: &HashMap<String, String>,` add `credentials: &FlowCredentials,`.

At the call site in `run_with_auth` (the `self.execute_node(` call), after the `&external_secrets,` argument add `&credentials,`.

- [ ] **Step 5: Replace the placeholder `Auth` arm**

Replace the Plan 1 placeholder (`// Replaced by flow-auth-node plan 03 (executor).` and its arm) with:

```rust
            FlowNodeKind::Auth { label, .. } => {
                // The credential was resolved at run start. The node only
                // publishes its plain wire value, which is empty for types
                // without a token. The step reports no value, so a token
                // never shows in step output.
                if credentials.auth_for_node(&node.id).is_none() {
                    return Err(DomainError::Internal(format!(
                        "Auth node '{label}' has no resolved credential"
                    )));
                }
                let value = credentials.wire_value(&node.id).unwrap_or_default();
                Ok(ExecutedNode::plain(CapturedOutput::Value(
                    VariableValue::simple(value),
                )))
            }
```

- [ ] **Step 6: Mask Auth tokens in an Output node's report**

In the `FlowNodeKind::Output { .. }` arm of `execute_node`, replace the final `Ok(ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(value))))` with:

```rust
                // Wires get the raw value. The step shows Auth tokens masked.
                let reported =
                    crate::redaction::redact_secrets(&value, &credentials.secret_forms());
                Ok(ExecutedNode {
                    reported_value: Some(reported),
                    ..ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(value)))
                })
```

In `result_to_step`, change the match inside the `CapturedOutput::Value` arm so an Output node uses the reported value too. Replace:

```rust
                Some(FlowNodeKind::Input { .. }) | Some(FlowNodeKind::Transform { .. }) => {
                    reported_value
                        .clone()
                        .or_else(|| Some(v.data().to_string()))
                }
                Some(FlowNodeKind::Output { .. }) => Some(v.data().to_string()),
```

with:

```rust
                Some(FlowNodeKind::Input { .. })
                | Some(FlowNodeKind::Transform { .. })
                | Some(FlowNodeKind::Output { .. }) => reported_value
                    .clone()
                    .or_else(|| Some(v.data().to_string())),
```

Update the comment above it to: `// An Input, Transform or Output node reports its masked value.`

- [ ] **Step 7: Run the tests**

Run: `cargo test -p rocket-app -j4 flow_execution_service`
Expected: the new test PASSES and every existing test still PASSES (an Output with no Auth tokens reports exactly its captured value, as before).

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(flow): run Auth nodes and mask their token in Output steps"
```

---

### Task 2: Inherit substitution in Request nodes

**Files:**
- Modify: `crates/rocket-app/src/test_doubles.rs`
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `credentials.apply_to_inherit(&mut Auth)` (Plan 2), `credentials` param (Task 1).
- Produces: `RecordingExecutor::sent_auths() -> Vec<rocket_shared::types::Auth>` (also used by Task 3 and Plan 7).

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section 3.1, `inherit`).

- [ ] **Step 2: Make `RecordingExecutor` record the auth it sends**

In `crates/rocket-app/src/test_doubles.rs`, in `struct RecordingExecutor` add a field:

```rust
    sent_auth: Mutex<Vec<rocket_shared::types::Auth>>,
```

In `RecordingExecutor::new`, add `sent_auth: Mutex::new(Vec::new()),` to the struct literal.

Add the accessor after `sent_urls`:

```rust
    pub fn sent_auths(&self) -> Vec<rocket_shared::types::Auth> {
        self.sent_auth.lock().expect("lock").clone()
    }
```

In `impl HttpExecutor for RecordingExecutor`, at the start of `execute`, after the existing `self.sent...push(req.url.clone());` line, add:

```rust
        self.sent_auth.lock().expect("lock").push(req.auth.clone());
```

- [ ] **Step 3: Write the failing tests**

In `flow_execution_service.rs` `mod tests`, after the Task 1 test, add a helper and three tests:

```rust
    /// An exec service whose HTTP layer records every request it sends.
    fn recording_http_exec(executor: &Arc<crate::test_doubles::RecordingExecutor>) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(executor))),
            Box::new(NullHistoryRepo),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
    }

    /// A flow of one Auth node (Bearer, `apply_to_inherit` as given) and one
    /// saved Request node `r` that reads `req.yml`.
    fn auth_and_request_flow(apply_to_inherit: bool, edges: Vec<FlowEdge>) -> Flow {
        Flow {
            name: "auth-req".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Auth {
                        label: "Sign in".to_string(),
                        auth: rocket_shared::types::Auth::Bearer {
                            token: "flow-token-123456".to_string(),
                        },
                        apply_to_inherit,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                saved_flow_node("r", "req.yml"),
            ],
            edges,
            callback_host: None,
        }
    }

    /// A service whose collection holds `req.yml` with the given auth.
    fn service_with_saved_request(
        flow: Flow,
        request_auth: rocket_shared::types::Auth,
    ) -> FlowExecutionService {
        let mut saved = Request::new("Get", HttpMethod::Get, "https://api.example.com/x");
        saved.runtime_auth = Some(request_auth);
        FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new().with_request("my-api", "req.yml", saved)),
            Box::new(NullEventPublisher),
        )
    }

    #[tokio::test]
    async fn an_inherit_request_uses_the_flows_auto_apply_credential() {
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let service =
            service_with_saved_request(auth_and_request_flow(true, Vec::new()), Auth::Inherit);

        service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        assert_eq!(
            executor.sent_auths(),
            vec![Auth::Bearer {
                token: "flow-token-123456".to_string()
            }]
        );
    }

    #[tokio::test]
    async fn an_inherit_request_is_left_alone_when_the_node_does_not_apply() {
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let service =
            service_with_saved_request(auth_and_request_flow(false, Vec::new()), Auth::Inherit);

        service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        assert_eq!(
            executor.sent_auths(),
            vec![Auth::None],
            "inherit with no collection auth resolves to none"
        );
    }

    #[tokio::test]
    async fn a_request_with_its_own_auth_is_never_replaced_by_auto_apply() {
        use rocket_shared::types::Auth;

        let own = Auth::Basic {
            username: "mine".to_string(),
            password: "mine".to_string(),
        };
        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let service =
            service_with_saved_request(auth_and_request_flow(true, Vec::new()), own.clone());

        service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        assert_eq!(executor.sent_auths(), vec![own]);
    }
```

If `NullEnvRepo`, `NullHistoryRepo`, `NullCookieRepo`, `EmptySecretManagerRepo` or `SharedExecutor` are not in scope, they are the same names used by `a_run_resolves_a_global_env_placeholder_in_an_inline_requests_url`; copy that test's imports.

- [ ] **Step 4: Run to verify the first test fails**

Run: `cargo test -p rocket-app -j4 inherit_request`
Expected: `an_inherit_request_uses_the_flows_auto_apply_credential` FAILS (sent auth is `None`, not the bearer token).

- [ ] **Step 5: Substitute inherited auth in the `Request` arm**

In the `FlowNodeKind::Request { .. }` arm of `execute_node`, directly after `request_input.flow_vars = callbacks.vars().clone();`, add:

```rust
                // A request set to inherit uses the flow's Auth node, when one
                // applies. A request with its own auth keeps it.
                credentials.apply_to_inherit(&mut request_input.auth);
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p rocket-app -j4 flow_execution_service`
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/test_doubles.rs crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(flow): apply the Auth node credential to inherit-auth requests"
```

---

### Task 3: The `auth` wire target

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `credentials.auth_for_node`, `rocket_flow::handle::AUTH` (Plan 1), the helpers from Task 2.
- Produces: an Auth→Request edge with `target_field == "auth"` sets that request's `auth` (and wins over `inherit` and the request's own auth).

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

After the Task 2 tests, add:

```rust
    fn auth_wire() -> FlowEdge {
        FlowEdge {
            id: "e1".to_string(),
            source_node_id: "a".to_string(),
            target_node_id: "r".to_string(),
            target_field: rocket_flow::handle::AUTH.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        }
    }

    #[tokio::test]
    async fn an_auth_wire_sets_the_requests_auth_over_its_own() {
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        // The node does not auto-apply, so only the wire can supply the credential.
        let service = service_with_saved_request(
            auth_and_request_flow(false, vec![auth_wire()]),
            Auth::Basic {
                username: "own".to_string(),
                password: "own".to_string(),
            },
        );

        service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        assert_eq!(
            executor.sent_auths(),
            vec![Auth::Bearer {
                token: "flow-token-123456".to_string()
            }]
        );
    }

    #[tokio::test]
    async fn an_auth_wire_also_beats_inherit() {
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let service = service_with_saved_request(
            auth_and_request_flow(false, vec![auth_wire()]),
            Auth::Inherit,
        );

        service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        assert_eq!(
            executor.sent_auths(),
            vec![Auth::Bearer {
                token: "flow-token-123456".to_string()
            }]
        );
    }
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p rocket-app -j4 an_auth_wire`
Expected: FAIL — the Request arm tries to evaluate the empty wire expression, so the step errors (or the auth stays `Basic`).

- [ ] **Step 4: Apply `auth` wires in the `Request` arm**

In the `Request` arm, replace the wire-resolution loop

```rust
                let mut resolved = HashMap::new();
                for edge in data_edges {
                    let source_output = captured_source(node, edge, captured)?;
```

with a version that handles `auth` edges first:

```rust
                let mut resolved = HashMap::new();
                for edge in data_edges {
                    // An auth wire carries a credential, not a value, so it has
                    // no expression to evaluate. It wins over inherit and over
                    // the request's own auth.
                    if edge.target_field == handle::AUTH {
                        request_input.auth = credentials
                            .auth_for_node(&edge.source_node_id)
                            .cloned()
                            .ok_or_else(|| {
                                DomainError::Internal(format!(
                                    "edge '{}': node '{}' has no credential",
                                    edge.id, edge.source_node_id
                                ))
                            })?;
                        continue;
                    }
                    let source_output = captured_source(node, edge, captured)?;
```

`apply_wired_overrides` is called afterwards with every data edge, but it only acts on edges that have a value in `resolved`, so the `auth` edges (never inserted) are skipped. Update its doc comment (the list of supported paths) by appending:

```rust
/// - `"auth"` wires are applied by the `Request` arm before this function runs
///   and are skipped here, because they carry a credential rather than a value.
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p rocket-app -j4 flow_execution_service && cargo check -j4`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(flow): let an Auth node wire its credential into a request"
```

---

**End of Plan 3.** Verify: `cargo test -p rocket-app -j4` and `cargo check -j4`.

**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-04-ipc.md`** (IPC `authTokens`, startup wiring, `runFlow` signature).
