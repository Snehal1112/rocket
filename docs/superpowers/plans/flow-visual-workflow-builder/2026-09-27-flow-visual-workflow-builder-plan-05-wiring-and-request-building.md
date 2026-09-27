# Flow Plan 05: Wiring Resolution + Request Building — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `FlowExecutionService` (Plan 06) everything it needs to turn one
`FlowNode` into a real `ExecuteRequestInput` and to resolve a wire's value
from an upstream node's captured output, without inventing any new HTTP
request model or expression language.

**Architecture:** Three small, independently-testable pieces added to a new
`crates/rocket-app/src/flow_execution_service.rs` module: (1) evaluating a
wiring expression against a captured node output, by wrapping the *existing*
`RequestExecutionService::evaluate_var_expression` jsonq mechanism — not a new
sandbox path; (2) building an `ExecuteRequestInput` for a `FlowNodeKind::Request`
node by reusing the *existing* `crate::runner_sequence::build_step_input`
function (the same one the Collection Runner uses) — not a new mapping; (3)
applying resolved wire values onto specific fields of that input.

**Tech Stack:** Rust, `serde_json`, the existing `rocket_scripting`/`ScriptEngine`
machinery (no new dependency).

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§6 Wiring semantics, §7 Execution, §10 Error handling). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md` — **this
plan corrects two of the index's claims; both corrections are called out
inline below and the index has been updated to match.**

## Corrections to the plan index (read before starting)

1. **`resolve_flow_wire_expression` takes an extra `collection: &str` parameter**
   not present in the index's original sketch. `RequestExecutionService::evaluate_var_expression`
   (`crates/rocket-app/src/execution_service.rs:1474`) — the exact mechanism
   this method wraps — requires a `collection_root: &str` to load
   collection-scope variables before evaluating the expression. There is no
   way to reuse that mechanism without also taking a collection name. Task 1
   below reflects the corrected signature; Plan 06 must pass `input.collection`
   when calling it.
2. **The index's claim that "there is no existing Rust Request → ExecuteRequestInput
   mapping to reuse" is wrong — it exists and Task 2 reuses it.**
   `crate::runner_sequence::build_step_input(item: &RunItem, collection: &str,
   environment_name: Option<&str>, global_env_name: Option<&str>,
   request_guard_policy: RequestGuardPolicy) -> ExecuteRequestInput`
   (`crates/rocket-app/src/runner_sequence.rs:112`) is exactly this mapping —
   it is what the Collection Runner already uses for the identical problem
   (turn a saved `Request` into an `ExecuteRequestInput`). Task 2 below reuses
   it directly for both `Saved` and `Inline` sources, instead of writing a
   second, parallel field-by-field mapping.

## Global Constraints

- This plan creates `crates/rocket-app/src/flow_execution_service.rs`. Plan 06
  **modifies** (not creates) this same file to add `FlowExecutionService`
  itself — say so explicitly in Plan 06 so its tasks use `Modify`, not `Create`.
- Never a panicking `unwrap` or bare `expect` call outside test code, per this
  repo's hard rule. Every fallible step returns `DomainResult`.
- `InlineRequestData` (from `rocket-flow`, Plan 01) is deliberately **not**
  `rocket_http::HttpRequest` — `rocket-flow` has zero cross-domain-crate
  dependencies. Task 2 is where the translation from `InlineRequestData`'s
  plain-string shape into `rocket_shared::types::{HttpMethod, Header, Body,
  BodyMode}` happens; do not push that translation down into `rocket-flow`.
- Flow nodes carry no per-request scripts/assertions/actions in Phase 1 (the
  spec's non-goals, §3, do not mention them, but nothing in the Goals section
  asks for them either — Flow nodes are plain HTTP calls plus wiring, not
  scriptable steps). `build_execute_request_input` gets these fields for free
  as empty/default because `Request::new(...)` and a freshly-constructed
  synthetic `Request` for the `Inline` case both default them to empty — no
  extra code is needed to suppress them, and no task here should add any.
- 📖 Before starting Task 2, read `docs/superpowers/specs/opencollection-spec-reference.md`
  — Task 2 resolves a `Saved` node against the live collection tree and Task 3
  mutates URL/header/body fields derived from it.

## Review Focus

- An expression that makes the script engine return `ScriptResult.error`
  (e.g. a `ReferenceError`) must surface as `DomainResult::Err`, not panic and
  not silently return an empty string.
- A `Saved` node whose `request_path` does not resolve (`get_request` returns
  `Err`) must propagate that error out of `build_execute_request_input`
  unchanged, not be swallowed into a default/empty request.
- An `Inline` node with an unparseable `method` string (e.g. `"FETCH"`) must
  be a clear `DomainError::InvalidInput`, not a panic — `HttpMethod::from_str`
  already returns exactly this error type; do not wrap it in another error.
- `apply_wired_overrides` called with empty `resolved`/`edges` must be a
  complete no-op — the `ExecuteRequestInput` passed in comes back byte-for-byte
  unchanged.
- A `target_field` of `"headers[N].value"` where `N == headers.len()` (one
  past the last real header) is an out-of-range `DomainError`, not a panic
  from indexing past the end of the `Vec`.

---

## Task 1: `CapturedOutput` + `resolve_flow_wire_expression`

**Files:**
- Create: `crates/rocket-app/src/flow_execution_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Modify: `crates/rocket-app/Cargo.toml` (add the `rocket-flow` dependency)

**Interfaces:**
- Produces: `CapturedOutput` enum, `RequestExecutionService::resolve_flow_wire_expression(&self, collection: &str, output: &CapturedOutput, expression: &str) -> DomainResult<String>` — consumed by Plan 06 Task 2.

- [ ] **Step 1: Add the `rocket-flow` dependency**

`crates/rocket-app/Cargo.toml` does not yet depend on `rocket-flow` (Plan 01)
— this task's imports need it. Add to `[dependencies]`:

```toml
rocket-flow.workspace = true
```

(The workspace root `Cargo.toml` already declares this path dependency from
Plan 01 — this is just adding it to `rocket-app`'s own dependency list.)

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-app/src/flow_execution_service.rs
use std::collections::HashMap;

use rocket_flow::{FlowEdge, FlowNode};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Body, BodyMode, Header, HttpMethod, QueryParam};
use rocket_shared::VariableValue;

use crate::execution_service::{ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService};
use crate::runner_sequence::{build_step_input, RunItem};

/// One node's fully-executed result, kept around so a downstream edge's
/// wiring expression can be evaluated against it.
#[derive(Debug, Clone)]
pub enum CapturedOutput {
    Request(Box<ExecuteRequestOutput>),
    Value(VariableValue),
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_collection::{Collection, CollectionRepository, CollectionSettings, Request};
    use rocket_environment::{Environment, EnvironmentRepository};
    use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
    use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
    use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
    use rocket_shared::events::{DomainEvent, EventPublisher};
    use std::sync::Arc;

    // ---- Minimal fakes shared by every task in this file ---------------
    // Only the methods this file's code paths actually call return real
    // values; everything else is `unimplemented!()` — standard practice for
    // a narrow test double (this crate's convention is inline, per-module
    // mocks; see `rocket-app/CLAUDE.md`).

    struct FakeCollectionRepo {
        requests: std::sync::Mutex<HashMap<(String, String), Request>>,
    }
    impl FakeCollectionRepo {
        fn new() -> Self {
            Self {
                requests: std::sync::Mutex::new(HashMap::new()),
            }
        }
        fn with_request(self, collection: &str, path: &str, request: Request) -> Self {
            self.requests
                .lock()
                .expect("lock FakeCollectionRepo")
                .insert((collection.to_string(), path.to_string()), request);
            self
        }
    }
    impl CollectionRepository for FakeCollectionRepo {
        fn list(&self) -> DomainResult<Vec<rocket_collection::CollectionSummary>> {
            unimplemented!()
        }
        fn get(&self, _name: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn get_summaries(&self, _name: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn create(&self, _name: &str) -> DomainResult<Collection> {
            unimplemented!()
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn rename(&self, _old_name: &str, _new_name: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
            self.requests
                .lock()
                .expect("lock FakeCollectionRepo")
                .get(&(collection.to_string(), path.to_string()))
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
        }
        fn save_request(&self, _c: &str, _p: &str, _r: &Request) -> DomainResult<String> {
            unimplemented!()
        }
        fn rename_request(&self, _c: &str, _o: &str, _n: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_request(&self, _c: &str, _p: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn create_folder(&self, _c: &str, _p: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_folder(&self, _c: &str, _p: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn move_item(&self, _sc: &str, _sp: &str, _dc: &str, _dp: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn reorder_items(&self, _c: &str, _p: &str, _order: &[String]) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_settings(&self, _name: &str) -> DomainResult<CollectionSettings> {
            Ok(CollectionSettings::default())
        }
        fn save_settings(&self, _name: &str, _settings: &CollectionSettings) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_folder_chain_variables(
            &self,
            _c: &str,
            _p: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(Vec::new())
        }
        fn get_folder_variables(
            &self,
            _c: &str,
            _p: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            unimplemented!()
        }
        fn save_folder_variables(
            &self,
            _c: &str,
            _p: &str,
            _vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unimplemented!()
        }
        fn get_request_variables(
            &self,
            _c: &str,
            _p: &str,
        ) -> DomainResult<Vec<rocket_collection::CollectionVariable>> {
            Ok(Vec::new())
        }
        fn save_request_variables(
            &self,
            _c: &str,
            _p: &str,
            _vars: Vec<rocket_collection::CollectionVariable>,
        ) -> DomainResult<()> {
            unimplemented!()
        }
    }

    struct NullEnvRepo;
    impl EnvironmentRepository for NullEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(Vec::new())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            Err(DomainError::NotFound(name.to_string()))
        }
        fn save(&self, _env: &Environment) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            unimplemented!()
        }
    }

    struct NullCookieRepo;
    impl CookieRepository for NullCookieRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(Vec::new())
        }
        fn get_by_domain(&self, _domain: &str) -> DomainResult<Option<CookieJar>> {
            Ok(None)
        }
        fn save(&self, _jar: &CookieJar) -> DomainResult<()> {
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            Ok(())
        }
    }

    struct NullHistoryRepo;
    impl HistoryRepository for NullHistoryRepo {
        fn list(&self, _limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
            Ok(Vec::new())
        }
        fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
            Err(DomainError::NotFound(id.to_string()))
        }
        fn save(&self, _entry: &HistoryEntry) -> DomainResult<()> {
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            Ok(())
        }
        fn search(&self, _filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
            Ok(Vec::new())
        }
    }

    struct NullExecutor;
    #[async_trait]
    impl HttpExecutor for NullExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            unimplemented!("this test never dispatches a real HTTP call")
        }
    }

    struct NullEventPublisher;
    impl EventPublisher for NullEventPublisher {
        fn publish(&self, _event: DomainEvent) {}
    }

    struct EmptySecretManagerRepo;
    impl rocket_environment::SecretManagerRepository for EmptySecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<rocket_environment::SecretManagerConnection>> {
            Ok(Vec::new())
        }
        fn get(&self, _id: &str) -> DomainResult<Option<rocket_environment::SecretManagerConnection>> {
            Ok(None)
        }
        fn save(&self, _c: &rocket_environment::SecretManagerConnection) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete(&self, _id: &str) -> DomainResult<()> {
            unimplemented!()
        }
    }

    /// Script engine stub that always resolves the jsonq snippet to a fixed
    /// value — mirrors `FixedJsonqEngine` in `execution_service.rs`'s own
    /// tests (this file cannot import that one, it's private to that
    /// module's `#[cfg(test)]`, so it is re-declared here per this crate's
    /// existing "each module owns its own inline mocks" convention).
    struct FixedJsonqEngine {
        value: serde_json::Value,
    }
    #[async_trait]
    impl ScriptEngine for FixedJsonqEngine {
        async fn execute(&self, _ctx: ScriptContext) -> DomainResult<ScriptResult> {
            let mut vars = HashMap::new();
            vars.insert("__jsonq_result__".to_string(), self.value.clone());
            Ok(ScriptResult {
                runtime_vars: vars,
                ..Default::default()
            })
        }
    }

    struct ErrorJsonqEngine;
    #[async_trait]
    impl ScriptEngine for ErrorJsonqEngine {
        async fn execute(&self, _ctx: ScriptContext) -> DomainResult<ScriptResult> {
            Ok(ScriptResult {
                error: Some("ReferenceError: nope".into()),
                ..Default::default()
            })
        }
    }

    fn service_with_engine(
        collection_repo: FakeCollectionRepo,
        engine: Box<dyn ScriptEngine>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(NullExecutor),
            Box::new(NullHistoryRepo),
            Box::new(collection_repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    fn sample_response_output() -> ExecuteRequestOutput {
        ExecuteRequestOutput {
            response: HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: r#"{"token":"abc123"}"#.into(),
                duration_ms: 10,
                ttfb_ms: 5,
                size_bytes: 20,
            },
            test_results: Vec::new(),
            console_entries: Vec::new(),
            script_error: None,
        }
    }

    #[tokio::test]
    async fn resolves_expression_against_request_output() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("abc123"),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body")
            .await
            .expect("expression should resolve");

        assert_eq!(value, "abc123");
    }

    #[tokio::test]
    async fn resolves_expression_against_input_node_value() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("hello"),
            }),
        );
        let output = CapturedOutput::Value(VariableValue::simple("hello"));

        let value = svc
            .resolve_flow_wire_expression("my-api", &output, "response.body")
            .await
            .expect("expression should resolve");

        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn script_error_surfaces_as_domain_error_not_panic() {
        let svc = service_with_engine(FakeCollectionRepo::new(), Box::new(ErrorJsonqEngine));
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .resolve_flow_wire_expression("my-api", &output, "response.nope.nope")
            .await
            .expect_err("a throwing expression must be an Err, not a panic");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: FAIL — `resolve_flow_wire_expression` does not exist yet (compile
error).

- [ ] **Step 4: Implement `resolve_flow_wire_expression`**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add above the tests module)

impl RequestExecutionService {
    /// Evaluates `expression` (a jsonq/JS snippet such as `"response.body"` or
    /// `"response.body.token"`) against `output`, reusing the same
    /// script-engine mechanism `evaluate_var_expression` uses for the Vars
    /// tab's preview — not a second sandbox invocation path. `Value` outputs
    /// (Input/Output nodes) are normalized into a synthetic `HttpResponse`
    /// whose `body` is that value's raw string, so a single expression
    /// convention ("response.xxx") works uniformly regardless of which kind
    /// of node produced the output.
    pub async fn resolve_flow_wire_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        expression: &str,
    ) -> DomainResult<String> {
        let response = match output {
            CapturedOutput::Request(out) => out.response.clone(),
            CapturedOutput::Value(value) => rocket_http::HttpResponse {
                status: 200,
                status_text: "OK".to_string(),
                headers: Vec::new(),
                body: value.data().to_string(),
                duration_ms: 0,
                ttfb_ms: 0,
                size_bytes: value.data().len(),
            },
        };
        let response_json = serde_json::to_string(&response).map_err(|e| {
            DomainError::Internal(format!("failed to serialize captured output: {e}"))
        })?;
        let result = self
            .evaluate_var_expression(collection, expression, &response_json)
            .await?;
        Ok(match result {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        })
    }
}
```

- [ ] **Step 5: Register the module**

In `crates/rocket-app/src/lib.rs`, add alongside the existing `pub mod
execution_service;` declaration:

```rust
pub mod flow_execution_service;
pub use flow_execution_service::CapturedOutput;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: PASS — 3 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/lib.rs crates/rocket-app/Cargo.toml Cargo.lock
git commit -m "feat(app): add Flow wiring expression resolution"
```

---

## Task 2: `build_execute_request_input`

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `rocket_flow::{FlowNode, FlowNodeKind, RequestSource, InlineRequestData}` (Plan 01); `crate::runner_sequence::{build_step_input, RunItem}` (existing).
- Produces: `build_execute_request_input(collection_repo, collection, environment_name, node) -> DomainResult<ExecuteRequestInput>` — consumed by Plan 06 Task 2.

- [ ] **Step 1: Write the failing tests**

📖 Before starting this task, read `docs/superpowers/specs/opencollection-spec-reference.md`.

```rust
// crates/rocket-app/src/flow_execution_service.rs (add to the existing tests module)

use rocket_flow::{FlowNodeKind, InlineHeader, InlineRequestData, NodePosition, RequestSource};

fn saved_flow_node(id: &str, request_path: &str) -> FlowNode {
    FlowNode {
        id: id.to_string(),
        kind: FlowNodeKind::Request {
            label: "Get Auth Token".to_string(),
            source: RequestSource::Saved {
                request_path: request_path.to_string(),
            },
        },
        position: NodePosition { x: 0.0, y: 0.0 },
    }
}

fn inline_flow_node(id: &str) -> FlowNode {
    FlowNode {
        id: id.to_string(),
        kind: FlowNodeKind::Request {
            label: "Ping".to_string(),
            source: RequestSource::Inline {
                request: InlineRequestData {
                    method: "post".to_string(),
                    url: "https://api.example.com/ping".to_string(),
                    headers: vec![InlineHeader {
                        name: "X-Test".to_string(),
                        value: "1".to_string(),
                    }],
                    body: Some(r#"{"ok":true}"#.to_string()),
                },
            },
        },
        position: NodePosition { x: 0.0, y: 0.0 },
    }
}

#[test]
fn saved_source_resolves_via_collection_repo_and_reuses_build_step_input() {
    let mut saved = Request::new("Get Auth Token", HttpMethod::Get, "https://api.example.com/login");
    saved.tags = vec!["auth".to_string()];
    let repo = FakeCollectionRepo::new().with_request("my-api", "auth/login.yml", saved);

    let node = saved_flow_node("n1", "auth/login.yml");
    let input = build_execute_request_input(&repo, "my-api", Some("dev"), &node)
        .expect("saved source must resolve");

    assert_eq!(input.method, HttpMethod::Get);
    assert_eq!(input.url, "https://api.example.com/login");
    assert_eq!(input.collection.as_deref(), Some("my-api"));
    assert_eq!(input.environment_name.as_deref(), Some("dev"));
    assert_eq!(input.request_path.as_deref(), Some("auth/login.yml"));
    assert_eq!(input.tags, vec!["auth".to_string()]);
}

#[test]
fn saved_source_propagates_not_found_instead_of_defaulting() {
    let repo = FakeCollectionRepo::new();
    let node = saved_flow_node("n1", "does/not/exist.yml");

    let err = build_execute_request_input(&repo, "my-api", None, &node)
        .expect_err("a missing saved request must error, not silently build an empty request");

    assert!(matches!(err, DomainError::NotFound(_)));
}

#[test]
fn inline_source_builds_request_from_embedded_fields() {
    let repo = FakeCollectionRepo::new();
    let node = inline_flow_node("n2");

    let input = build_execute_request_input(&repo, "my-api", None, &node)
        .expect("inline source must build");

    assert_eq!(input.method, HttpMethod::Post);
    assert_eq!(input.url, "https://api.example.com/ping");
    assert_eq!(input.headers.len(), 1);
    assert_eq!(input.headers[0].key, "X-Test");
    assert_eq!(input.headers[0].value, "1");
    let body = input.body.expect("inline body must be set");
    assert_eq!(body.content.as_deref(), Some(r#"{"ok":true}"#));
    assert_eq!(body.mode, BodyMode::Json);
}

#[test]
fn inline_source_with_unparseable_method_is_invalid_input_not_a_panic() {
    let repo = FakeCollectionRepo::new();
    let mut node = inline_flow_node("n2");
    if let FlowNodeKind::Request { source: RequestSource::Inline { request }, .. } = &mut node.kind {
        request.method = "FETCH".to_string();
    }

    let err = build_execute_request_input(&repo, "my-api", None, &node)
        .expect_err("an invalid method string must be InvalidInput");

    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[test]
fn non_request_node_is_rejected() {
    let repo = FakeCollectionRepo::new();
    let node = FlowNode {
        id: "n3".to_string(),
        kind: FlowNodeKind::Output {
            label: "Result".to_string(),
        },
        position: NodePosition { x: 0.0, y: 0.0 },
    };

    let err = build_execute_request_input(&repo, "my-api", None, &node)
        .expect_err("an Output node has no request to build");

    assert!(matches!(err, DomainError::InvalidInput(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: FAIL — `build_execute_request_input` does not exist yet (compile
error).

- [ ] **Step 3: Implement it, reusing `build_step_input`**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add above the tests module)

/// Builds an `ExecuteRequestInput` for a `FlowNodeKind::Request` node, before
/// any wire overrides (see `apply_wired_overrides`) are applied.
///
/// Both `Saved` and `Inline` sources resolve down to a `rocket_collection::Request`
/// value, then reuse `crate::runner_sequence::build_step_input` — the exact
/// function the Collection Runner already uses for the same "saved request →
/// ExecuteRequestInput" problem — rather than a second, parallel mapping.
pub fn build_execute_request_input(
    collection_repo: &dyn rocket_collection::CollectionRepository,
    collection: &str,
    environment_name: Option<&str>,
    node: &FlowNode,
) -> DomainResult<ExecuteRequestInput> {
    let FlowNodeKind::Request { label, source } = &node.kind else {
        return Err(DomainError::InvalidInput(format!(
            "node '{}' is not a Request node",
            node.id
        )));
    };

    let (request, request_path) = match source {
        RequestSource::Saved { request_path } => {
            let request = collection_repo.get_request(collection, request_path)?;
            (request, request_path.clone())
        }
        RequestSource::Inline { request: inline } => {
            (build_inline_request(label, inline)?, format!("__flow_inline__/{}", node.id))
        }
    };

    let item = RunItem {
        name: request.name.clone(),
        request_path,
        request,
    };
    Ok(build_step_input(
        &item,
        collection,
        environment_name,
        None,
        rocket_workspace::RequestGuardPolicy::default(),
    ))
}

/// Turns an ad hoc `InlineRequestData` into a `rocket_collection::Request`
/// value object so it can flow through the same `build_step_input` path a
/// saved request uses. `request_path` for an inline node is a synthetic,
/// never-resolves-to-a-real-file sentinel (`"__flow_inline__/<node id>"`) —
/// `RequestExecutionService::build_variable_scopes` already treats a failed
/// `get_folder_chain_variables`/`get_request_variables` lookup as "no
/// variables at this scope" (`if let Ok(...)`), which is exactly correct
/// here: an inline request isn't part of the collection tree and should not
/// inherit folder-chain variables.
fn build_inline_request(label: &str, inline: &InlineRequestData) -> DomainResult<Request> {
    let method: HttpMethod = inline.method.parse()?;
    let mut request = Request::new(label, method, inline.url.clone());
    request.headers = inline
        .headers
        .iter()
        .map(|h| Header {
            key: h.name.clone(),
            value: h.value.clone(),
            enabled: true,
            description: None,
        })
        .collect();
    request.body = inline.body.as_ref().map(|content| Body {
        mode: BodyMode::Json,
        content: Some(content.clone()),
        form_data: None,
        file_path: None,
    });
    Ok(request)
}
```

Add `use rocket_collection::Request;` and `use rocket_flow::{FlowNodeKind, InlineRequestData, RequestSource};`
to this file's top-level imports (alongside the ones Task 1 already added).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: PASS — 8 tests total (3 from Task 1, 5 from this task).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(app): build ExecuteRequestInput for Flow request nodes"
```

---

## Task 3: `apply_wired_overrides`

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `rocket_flow::FlowEdge` (Plan 01); `ExecuteRequestInput` (Task 2 of this plan).
- Produces: `apply_wired_overrides(input, resolved, edges) -> DomainResult<()>` — consumed by Plan 06 Task 2.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add to the existing tests module)

fn edge(id: &str, target_node: &str, target_field: &str) -> FlowEdge {
    FlowEdge {
        id: id.to_string(),
        source_node_id: "src".to_string(),
        target_node_id: target_node.to_string(),
        target_field: target_field.to_string(),
        expression: "response.body".to_string(),
    }
}

fn sample_execute_input() -> ExecuteRequestInput {
    let repo = FakeCollectionRepo::new();
    build_execute_request_input(&repo, "my-api", None, &inline_flow_node("n2"))
        .expect("build sample input")
}

#[test]
fn empty_overrides_leave_input_unchanged() {
    let input = sample_execute_input();
    let mut mutated = input.clone();
    apply_wired_overrides(&mut mutated, &HashMap::new(), &[]).expect("no-op must succeed");
    assert_eq!(mutated.url, input.url);
    assert_eq!(mutated.headers, input.headers);
    assert_eq!(mutated.body, input.body);
}

#[test]
fn url_override_replaces_url() {
    let mut input = sample_execute_input();
    let edges = vec![edge("e1", "n2", "url")];
    let mut resolved = HashMap::new();
    resolved.insert("e1".to_string(), "https://api.example.com/v2/ping".to_string());

    apply_wired_overrides(&mut input, &resolved, &edges).expect("url override must apply");

    assert_eq!(input.url, "https://api.example.com/v2/ping");
}

#[test]
fn header_value_override_replaces_the_named_index() {
    let mut input = sample_execute_input();
    let edges = vec![edge("e1", "n2", "headers[0].value")];
    let mut resolved = HashMap::new();
    resolved.insert("e1".to_string(), "42".to_string());

    apply_wired_overrides(&mut input, &resolved, &edges).expect("header override must apply");

    assert_eq!(input.headers[0].value, "42");
}

#[test]
fn header_value_override_out_of_range_is_an_error_not_a_panic() {
    let mut input = sample_execute_input();
    let out_of_range = input.headers.len();
    let edges = vec![edge("e1", "n2", &format!("headers[{out_of_range}].value"))];
    let mut resolved = HashMap::new();
    resolved.insert("e1".to_string(), "42".to_string());

    let err = apply_wired_overrides(&mut input, &resolved, &edges)
        .expect_err("an out-of-range header index must error");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[test]
fn body_override_replaces_body_content() {
    let mut input = sample_execute_input();
    let edges = vec![edge("e1", "n2", "body")];
    let mut resolved = HashMap::new();
    resolved.insert("e1".to_string(), r#"{"replaced":true}"#.to_string());

    apply_wired_overrides(&mut input, &resolved, &edges).expect("body override must apply");

    assert_eq!(
        input.body.expect("body must be set").content.as_deref(),
        Some(r#"{"replaced":true}"#)
    );
}

#[test]
fn unrecognized_target_field_is_an_error_not_a_silent_noop() {
    let mut input = sample_execute_input();
    let edges = vec![edge("e1", "n2", "auth.token")];
    let mut resolved = HashMap::new();
    resolved.insert("e1".to_string(), "x".to_string());

    let err = apply_wired_overrides(&mut input, &resolved, &edges)
        .expect_err("an unrecognized target_field must error, not silently do nothing");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: FAIL — `apply_wired_overrides` does not exist yet (compile error).

- [ ] **Step 3: Implement it**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add above the tests module)

/// Mutates `input` in place, applying each edge in `edges` whose id is a key
/// in `resolved` onto the field its `target_field` path names. Supported
/// paths for Phase 1: `"url"`, `"headers[N].value"`, `"body"`. Any other
/// path, or an out-of-range header index, is a `DomainError` — never a
/// silent no-op, since a wire the user drew that quietly does nothing would
/// be far more confusing than a run that fails with a clear reason.
pub fn apply_wired_overrides(
    input: &mut ExecuteRequestInput,
    resolved: &HashMap<String, String>,
    edges: &[FlowEdge],
) -> DomainResult<()> {
    for e in edges {
        let Some(value) = resolved.get(&e.id) else {
            continue;
        };
        match e.target_field.as_str() {
            "url" => input.url = value.clone(),
            "body" => {
                let body = input.body.get_or_insert(Body {
                    mode: BodyMode::Json,
                    content: None,
                    form_data: None,
                    file_path: None,
                });
                body.content = Some(value.clone());
            }
            field => {
                if let Some(index_str) = field
                    .strip_prefix("headers[")
                    .and_then(|rest| rest.strip_suffix("].value"))
                {
                    let index: usize = index_str.parse().map_err(|_| {
                        DomainError::InvalidInput(format!(
                            "edge '{}': malformed target_field '{}'",
                            e.id, e.target_field
                        ))
                    })?;
                    // `headers.len()` is read into a local before the mutable
                    // borrow below — reading it inline inside `get_mut`'s
                    // `ok_or_else` closure is a real E0502 borrow-checker
                    // conflict (the immutable borrow for `.len()` overlaps
                    // the mutable borrow `get_mut` holds), caught during
                    // Task 3's implementation.
                    let headers_len = input.headers.len();
                    let header = input.headers.get_mut(index).ok_or_else(|| {
                        DomainError::InvalidInput(format!(
                            "edge '{}': header index {} out of range (request has {} headers)",
                            e.id, index, headers_len
                        ))
                    })?;
                    header.value = value.clone();
                } else {
                    return Err(DomainError::InvalidInput(format!(
                        "edge '{}': unrecognized target_field '{}'",
                        e.id, e.target_field
                    )));
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: PASS — 14 tests total (3 from Task 1, 5 from Task 2, 6 from this task).

- [ ] **Step 5: Run the full crate test suite**

Run: `cargo test -p rocket-app -j4`
Expected: PASS — confirms nothing in `execution_service.rs` or
`runner_sequence.rs` broke.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(app): apply Flow wire overrides onto ExecuteRequestInput"
```

---

## Next Plan

[Plan 06: FlowExecutionService](2026-09-27-flow-visual-workflow-builder-plan-06-execution-service.md) —
adds the orchestration loop (`FlowExecutionService`) that drives these three
pieces in topological order over a whole `Flow`.

## Post-Implementation Review

Before starting Plan 06, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-app/src/flow_execution_service.rs`,
> `crates/rocket-app/src/lib.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does `CapturedOutput`,
>    `resolve_flow_wire_expression`, `build_execute_request_input`, and
>    `apply_wired_overrides` match exactly what the plan index's locked
>    interface contract (as corrected by this plan's "Corrections to the plan
>    index" section) promises Plan 06 will consume?
> 2. Code quality — naming, doc comments, test coverage versus this plan's
>    Review Focus section (script-error propagation, Saved-not-found
>    propagation, Inline invalid-method handling, empty-override no-op,
>    out-of-range header index).
> 3. Duplication — confirm `build_execute_request_input` actually calls
>    `crate::runner_sequence::build_step_input` rather than reimplementing
>    field-by-field mapping logic that already exists there.
> 4. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    `rocket-app` orchestrates via the `CollectionRepository` trait only (no
>    concrete `rocket-infra` type appears here), and `InlineRequestData`'s
>    translation stays inside `rocket-app`, not pushed into `rocket-flow`.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-app -j4` and
> `cargo check -p rocket-app -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 06 once this review comes back clean (or its fixes are
applied and re-verified).
