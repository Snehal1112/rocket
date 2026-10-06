# Protocol parity, Plan 06: GraphQL execution, editor, responses and runner

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GraphQL request can be edited and sent. The query and variables have real editors, the operation is picked from a multi-operation document, `errors[]` in the response are surfaced, sends land in History, and the Collection Runner runs GraphQL items.

**Architecture:** Execution reuses the HTTP executor. A pure builder in `rocket-app` turns `(method, query, variables, operationName)` into an HTTP body (POST, `application/json`) or query-string parameters (GET). `RequestExecutionService::execute_graphql` applies that to an `ExecuteRequestInput` and calls the existing `execute`, so variables, auth, scripts, assertions and History run unchanged. A small lexical scanner lists a document's operations; the frontend asks for the list over IPC instead of porting the scanner. The frontend keeps the HTTP panels for headers, auth, scripts, settings and docs, and swaps only the Body tab for a `GraphQlEditor`. No GraphQL package is added in this plan.

**Tech Stack:** Rust (serde_json, wiremock for tests), Tauri 2 IPC, React + TypeScript, Monaco (built-in `graphql` Monarch tokenizer), Zustand, Vitest. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md`, sections 2.4 `GraphQLRequest` and `GraphQLBody`. GraphQL over HTTP: POST `application/json` with `query`, `variables`, `operationName`; GET with the same names as query-string parameters.

## Global Constraints

- Never `unwrap()` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill, staging by explicit path.
- `ExecuteRequestInput` is itself the IPC payload (camelCase, `Deserialize`). `ExecuteGraphQlInput` nests it; do not copy its fields.
- Frontend: shadcn/ui primitives and `lucide-react` only. Single-line variable-aware fields use `SingleLineEditor`; multi-line editors use Monaco only. Zustand: narrow selectors only.
- No new npm package in this plan. Plan 07 adds `graphql` and `graphql-language-service`.
- This plan assumes Plan 05 is merged: `GraphQlRequest`, `CollectionItem::GraphQl`, `RequestState.graphql`, `mapGraphQlToState`, `saveTabRequest`.

## Review Focus

1. A document with several operations and no chosen name must fail with a clear error from the UI send path, and must not silently pick one. Braces, quotes, comments and `"""` block strings inside the document must not create phantom operations (Task 1 tests `list_operations_*`, `select_operation_*`).
2. Variables that are not a JSON object must be rejected before any network call, but variables that contain an unquoted `{{placeholder}}` (invalid JSON until resolved) must be allowed (Task 1 tests `validate_variables_*`, `execute_graphql_rejects_bad_variables_before_sending`).
3. A query with quotes, newlines and unicode must reach the server intact, and a `{{var}}` inside it must still resolve (Task 1 test `execute_graphql_resolves_placeholders_in_query_and_variables`).
4. An HTTP 200 response whose body has a non-empty `errors` array must read as a failure in the Errors tab and in the Collection Runner (Task 3 tests `graphql_errors_fail_a_runner_step`, `parseGraphQlResponse`).
5. A GraphQL tab must never send through `executeRequest`, and an HTTP tab must never send through `executeGraphQlRequest` (Task 2 test `dispatchSend`).

---

## Task 1: Execution path in `rocket-app`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/graphql_document.rs`
- Create: `crates/rocket-app/src/graphql_request.rs`
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Produces:
  - `rocket_app::graphql_document::{GraphQlOperation, GraphQlOperationKind, list_operations, select_operation}`.
    - `GraphQlOperation { name: Option<String>, kind: GraphQlOperationKind }`, serde camelCase, kind lowercase.
    - `list_operations(document: &str) -> Vec<GraphQlOperation>`.
    - `select_operation(document: &str, requested: Option<&str>, fallback_first: bool) -> DomainResult<Option<String>>`.
  - `rocket_app::graphql_request::{GraphQlWire, build_wire, validate_variables, apply_graphql_payload}` and `ExecuteGraphQlInput`.
    - `build_wire(method: HttpMethod, query: &str, variables: Option<&str>, operation_name: Option<&str>) -> DomainResult<GraphQlWire>`.
    - `ExecuteGraphQlInput { request: ExecuteRequestInput, query: String, variables: Option<String>, operation_name: Option<String> }`.
    - `RequestExecutionService::execute_graphql(&self, input: ExecuteGraphQlInput) -> DomainResult<ExecuteRequestOutput>`.
  - `rocket_app::ExecuteGraphQlInput` re-export.
- Consumes: `ExecuteRequestInput`, `RequestExecutionService::execute`, `rocket_shared::types::{Body, BodyMode, HttpMethod, QueryParam}`, `crate::test_doubles::*` (tests only).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing scanner tests**

Create `crates/rocket-app/src/graphql_document.rs` containing only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn names(doc: &str) -> Vec<(Option<String>, GraphQlOperationKind)> {
        list_operations(doc)
            .into_iter()
            .map(|o| (o.name, o.kind))
            .collect()
    }

    #[test]
    fn list_operations_finds_a_shorthand_query() {
        assert_eq!(
            names("{ users { id } }"),
            vec![(None, GraphQlOperationKind::Query)]
        );
    }

    #[test]
    fn list_operations_finds_named_operations_of_every_kind() {
        let doc = "query A { a }\nmutation B($x: Int = 1) @dir { b }\nsubscription C { c }";
        assert_eq!(
            names(doc),
            vec![
                (Some("A".into()), GraphQlOperationKind::Query),
                (Some("B".into()), GraphQlOperationKind::Mutation),
                (Some("C".into()), GraphQlOperationKind::Subscription),
            ]
        );
    }

    #[test]
    fn list_operations_finds_an_anonymous_keyword_operation() {
        assert_eq!(
            names("query { a }"),
            vec![(None, GraphQlOperationKind::Query)]
        );
        assert_eq!(
            names("mutation($x: Int) { a(x: $x) }"),
            vec![(None, GraphQlOperationKind::Mutation)]
        );
    }

    #[test]
    fn list_operations_ignores_fragments() {
        let doc = "fragment F on User { id }\nquery Q { user { ...F } }";
        assert_eq!(
            names(doc),
            vec![(Some("Q".into()), GraphQlOperationKind::Query)]
        );
    }

    #[test]
    fn list_operations_ignores_comments_strings_and_block_strings() {
        let doc = "# query Fake { x }\nquery Real {\n  a(s: \"query Nope { }\", t: \"\"\"mutation Also { } \\\"\"\" still\"\"\")\n}\n";
        assert_eq!(
            names(doc),
            vec![(Some("Real".into()), GraphQlOperationKind::Query)]
        );
    }

    #[test]
    fn list_operations_handles_object_defaults_in_variable_definitions() {
        let doc = "query A($f: Filter = {a: 1, b: {c: 2}}) { x }\nquery B { y }";
        assert_eq!(
            names(doc),
            vec![
                (Some("A".into()), GraphQlOperationKind::Query),
                (Some("B".into()), GraphQlOperationKind::Query),
            ]
        );
    }

    #[test]
    fn list_operations_of_an_empty_or_broken_document_is_empty() {
        assert!(list_operations("").is_empty());
        assert!(list_operations("   # nothing\n").is_empty());
        // An unterminated string must not hang or panic.
        assert!(list_operations("query A { a(s: \"oops").len() <= 1);
    }

    #[test]
    fn select_operation_uses_the_only_operation() {
        assert_eq!(
            select_operation("query A { a }", None, false).expect("select"),
            Some("A".to_string())
        );
        assert_eq!(
            select_operation("{ a }", None, false).expect("select"),
            None
        );
    }

    #[test]
    fn select_operation_requires_a_choice_when_there_are_several() {
        let doc = "query A { a } query B { b }";
        let err = select_operation(doc, None, false).expect_err("must choose");
        assert!(err.to_string().contains("2 operations"), "got: {err}");
    }

    #[test]
    fn select_operation_can_fall_back_to_the_first_for_the_runner() {
        let doc = "query A { a } query B { b }";
        assert_eq!(
            select_operation(doc, None, true).expect("select"),
            Some("A".to_string())
        );
    }

    #[test]
    fn select_operation_validates_a_requested_name() {
        let doc = "query A { a } query B { b }";
        assert_eq!(
            select_operation(doc, Some("B"), false).expect("select"),
            Some("B".to_string())
        );
        let err = select_operation(doc, Some("Z"), false).expect_err("unknown");
        assert!(err.to_string().contains("'Z'"), "got: {err}");
    }

    #[test]
    fn select_operation_rejects_a_document_with_no_operation() {
        let err = select_operation("fragment F on U { id }", None, false).expect_err("none");
        assert!(err.to_string().contains("no operation"), "got: {err}");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app graphql_document`
Expected: FAIL to compile (module not registered, types missing). Register the module first: add `pub mod graphql_document;` and `pub mod graphql_request;` to the module list in `crates/rocket-app/src/lib.rs` (after `pub mod git_service;`) and create an empty `graphql_request.rs`. The run then fails with `cannot find ... GraphQlOperationKind`.

- [ ] **Step 4: Implement the scanner**

Put this above the test module in `crates/rocket-app/src/graphql_document.rs`:

```rust
//! A lexical scan of a GraphQL document, just enough to list its operations.
//!
//! It is not a parser. It tracks nesting and skips comments, strings and block
//! strings, so a brace or a keyword inside those never counts. A document that
//! is not valid GraphQL gives a best-effort answer and never panics.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphQlOperationKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlOperation {
    /// `None` for an anonymous operation.
    pub name: Option<String>,
    pub kind: GraphQlOperationKind,
}

fn is_name_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_name_char(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

const TRIPLE: [char; 3] = ['"', '"', '"'];

/// Returns the index just past the string that starts at `start`.
fn skip_string(chars: &[char], start: usize) -> usize {
    if chars.get(start..start + 3) == Some(&TRIPLE[..]) {
        let mut i = start + 3;
        while i < chars.len() {
            // `\"""` is an escaped delimiter inside a block string.
            if chars[i] == '\\' && chars.get(i + 1..i + 4) == Some(&TRIPLE[..]) {
                i += 4;
                continue;
            }
            if chars.get(i..i + 3) == Some(&TRIPLE[..]) {
                return i + 3;
            }
            i += 1;
        }
        return chars.len();
    }
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' | '\n' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// Lists the operations a document defines, in order.
pub fn list_operations(document: &str) -> Vec<GraphQlOperation> {
    let chars: Vec<char> = document.chars().collect();
    let mut ops = Vec::new();
    let mut depth: usize = 0;
    // True from an operation or fragment keyword until its selection set opens,
    // so that body is not mistaken for a shorthand query.
    let mut awaiting_body = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
        } else if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '"' {
            i = skip_string(&chars, i);
        } else if c == '{' {
            if depth == 0 {
                if !awaiting_body {
                    ops.push(GraphQlOperation {
                        name: None,
                        kind: GraphQlOperationKind::Query,
                    });
                }
                awaiting_body = false;
            }
            depth += 1;
            i += 1;
        } else if c == '(' || c == '[' {
            depth += 1;
            i += 1;
        } else if c == '}' || c == ')' || c == ']' {
            depth = depth.saturating_sub(1);
            i += 1;
        } else if is_name_start(c) {
            let start = i;
            while i < chars.len() && is_name_char(chars[i]) {
                i += 1;
            }
            if depth != 0 {
                continue;
            }
            let word: String = chars[start..i].iter().collect();
            let kind = match word.as_str() {
                "query" => Some(GraphQlOperationKind::Query),
                "mutation" => Some(GraphQlOperationKind::Mutation),
                "subscription" => Some(GraphQlOperationKind::Subscription),
                "fragment" => {
                    awaiting_body = true;
                    None
                }
                _ => None,
            };
            if let Some(kind) = kind {
                // The operation name, when there is one, is the next name.
                let mut j = i;
                while j < chars.len() && (chars[j].is_whitespace() || chars[j] == ',') {
                    j += 1;
                }
                let name = if j < chars.len() && is_name_start(chars[j]) {
                    let name_start = j;
                    while j < chars.len() && is_name_char(chars[j]) {
                        j += 1;
                    }
                    i = j;
                    Some(chars[name_start..j].iter().collect::<String>())
                } else {
                    None
                };
                ops.push(GraphQlOperation { name, kind });
                awaiting_body = true;
            }
        } else {
            i += 1;
        }
    }
    ops
}

/// Picks the operation name to send.
///
/// - A requested name must exist in the document.
/// - With no request, a document with one operation sends that one.
/// - With several operations and no request, the send path (`fallback_first == false`)
///   fails so the user must choose, and the runner (`fallback_first == true`) runs the first.
///
/// `Ok(None)` means the chosen operation is anonymous, so no `operationName` is sent.
pub fn select_operation(
    document: &str,
    requested: Option<&str>,
    fallback_first: bool,
) -> DomainResult<Option<String>> {
    let ops = list_operations(document);
    if ops.is_empty() {
        return Err(DomainError::InvalidInput(
            "the document has no operation to run".into(),
        ));
    }
    if let Some(name) = requested.map(str::trim).filter(|n| !n.is_empty()) {
        return if ops.iter().any(|o| o.name.as_deref() == Some(name)) {
            Ok(Some(name.to_string()))
        } else {
            Err(DomainError::InvalidInput(format!(
                "the document has no operation named '{name}'"
            )))
        };
    }
    if ops.len() == 1 || fallback_first {
        return Ok(ops[0].name.clone());
    }
    Err(DomainError::InvalidInput(format!(
        "the document defines {} operations; choose one to run",
        ops.len()
    )))
}
```

- [ ] **Step 5: Run the scanner tests to verify they pass**

Run: `cargo test -j4 -p rocket-app graphql_document`
Expected: PASS (11 tests). If `list_operations_ignores_comments_strings_and_block_strings` fails, re-check the Rust escape in the test string: it must contain the sequence `\"""` inside a `"""` block string.

- [ ] **Step 6: Write the failing wire-builder and execution tests**

Create `crates/rocket-app/src/graphql_request.rs` containing only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
        SharedCollectionRepo, SharedHistoryRepo, StaticEnvRepo,
    };
    use rocket_collection::Collection;
    use rocket_environment::environment::Environment;
    use rocket_environment::variable::Variable;
    use rocket_http::RequestOptions;
    use rocket_shared::types::Auth;
    use std::sync::Arc;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn post_body(wire: &GraphQlWire) -> serde_json::Value {
        let content = wire
            .body
            .as_ref()
            .and_then(|b| b.content.as_deref())
            .expect("a POST has a JSON body");
        serde_json::from_str(content).expect("body is valid JSON")
    }

    #[test]
    fn validate_variables_accepts_objects_blank_and_null() {
        assert!(validate_variables("").is_ok());
        assert!(validate_variables("  \n").is_ok());
        assert!(validate_variables("{}").is_ok());
        assert!(validate_variables("{\"a\": [1, 2]}").is_ok());
        assert!(validate_variables("null").is_ok());
    }

    #[test]
    fn validate_variables_rejects_non_objects_and_bad_json() {
        let err = validate_variables("[1, 2]").expect_err("array");
        assert!(err.to_string().contains("JSON object"), "got: {err}");
        let err = validate_variables("{\"a\": ").expect_err("truncated");
        assert!(err.to_string().contains("not valid JSON"), "got: {err}");
    }

    #[test]
    fn validate_variables_allows_an_unquoted_placeholder() {
        // Not valid JSON until the placeholder is resolved.
        assert!(validate_variables("{\"n\": {{count}}}").is_ok());
    }

    #[test]
    fn build_wire_post_encodes_query_variables_and_operation_name() {
        let wire = build_wire(
            HttpMethod::Post,
            "query A($n: Int) {\n  a(n: $n) # \"quoted\"\n}\n",
            Some("{\"n\": 5}"),
            Some("A"),
        )
        .expect("wire");
        assert_eq!(wire.method, HttpMethod::Post);
        assert!(wire.query_params.is_empty());
        let body = post_body(&wire);
        assert_eq!(
            body["query"],
            "query A($n: Int) {\n  a(n: $n) # \"quoted\"\n}\n"
        );
        assert_eq!(body["operationName"], "A");
        assert_eq!(body["variables"]["n"], 5);
        assert_eq!(wire.body.as_ref().map(|b| b.mode.clone()), Some(BodyMode::Json));
    }

    #[test]
    fn build_wire_post_omits_blank_variables_and_anonymous_operation() {
        let wire = build_wire(HttpMethod::Post, "{ a }", Some("  "), None).expect("wire");
        let body = post_body(&wire);
        assert_eq!(body["query"], "{ a }");
        assert!(body.get("variables").is_none());
        assert!(body.get("operationName").is_none());
    }

    #[test]
    fn build_wire_keeps_unicode_intact() {
        let wire = build_wire(HttpMethod::Post, "{ a(s: \"héllo ✓\") }", None, None).expect("wire");
        assert_eq!(post_body(&wire)["query"], "{ a(s: \"héllo ✓\") }");
    }

    #[test]
    fn build_wire_get_uses_query_parameters_and_no_body() {
        let wire = build_wire(
            HttpMethod::Get,
            "query A { a }",
            Some("{\"n\": 1}"),
            Some("A"),
        )
        .expect("wire");
        assert_eq!(wire.method, HttpMethod::Get);
        assert!(wire.body.is_none());
        let pairs: Vec<(&str, &str)> = wire
            .query_params
            .iter()
            .map(|p| (p.key.as_str(), p.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("query", "query A { a }"),
                ("operationName", "A"),
                ("variables", "{\"n\": 1}"),
            ]
        );
    }

    #[test]
    fn build_wire_rejects_other_methods_and_an_empty_query() {
        let err = build_wire(HttpMethod::Put, "{ a }", None, None).expect_err("PUT");
        assert!(err.to_string().contains("GET or POST"), "got: {err}");
        let err = build_wire(HttpMethod::Post, "  \n", None, None).expect_err("empty");
        assert!(err.to_string().contains("query is empty"), "got: {err}");
    }

    fn input(url: &str) -> ExecuteRequestInput {
        ExecuteRequestInput {
            skip_history: false,
            flow_vars: std::collections::HashMap::new(),
            method: HttpMethod::Post,
            url: url.to_string(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
            environment_name: None,
            collection: None,
            request_name: Some("Users".into()),
            pre_request_script: None,
            post_response_script: None,
            tests_script: None,
            request_path: None,
            global_env_name: None,
            assertions: vec![],
            tags: vec![],
            path_params: vec![],
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }

    fn service(env: Environment) -> (RequestExecutionService, Arc<InMemoryHistoryRepo>) {
        let history = InMemoryHistoryRepo::new();
        let repo = InMemoryCollectionRepo::new(Collection::new("api"));
        let svc = RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            Arc::new(rocket_infra::ReqwestExecutor::new()),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(repo)),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        (svc, history)
    }

    #[tokio::test]
    async fn execute_graphql_posts_json_with_the_chosen_operation() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("content-type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data": {"users": []}})),
            )
            .mount(&server)
            .await;
        let (svc, history) = service(Environment::new("dev"));

        let out = svc
            .execute_graphql(ExecuteGraphQlInput {
                request: input(&format!("{}/graphql", server.uri())),
                query: "query A { a } query B { b }".into(),
                variables: Some("{\"n\": 1}".into()),
                operation_name: Some("B".into()),
            })
            .await
            .expect("execute");

        assert_eq!(out.response.status, 200);
        let seen = server.received_requests().await.expect("recording is on");
        assert_eq!(seen.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json body");
        assert_eq!(body["query"], "query A { a } query B { b }");
        assert_eq!(body["operationName"], "B");
        assert_eq!(body["variables"]["n"], 1);
        assert_eq!(history.saved_count(), 1, "a GraphQL send is recorded in History");
    }

    #[tokio::test]
    async fn execute_graphql_get_sends_query_string_parameters() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/graphql"))
            .and(query_param("query", "query A { a }"))
            .and(query_param("operationName", "A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let mut request = input(&format!("{}/graphql", server.uri()));
        request.method = HttpMethod::Get;

        let out = svc
            .execute_graphql(ExecuteGraphQlInput {
                request,
                query: "query A { a }".into(),
                variables: None,
                operation_name: None,
            })
            .await
            .expect("execute");
        assert_eq!(out.response.status, 200);
    }

    #[tokio::test]
    async fn execute_graphql_rejects_bad_variables_before_sending() {
        let server = MockServer::start().await;
        let (svc, history) = service(Environment::new("dev"));

        let err = svc
            .execute_graphql(ExecuteGraphQlInput {
                request: input(&format!("{}/graphql", server.uri())),
                query: "{ a }".into(),
                variables: Some("[1]".into()),
                operation_name: None,
            })
            .await
            .expect_err("bad variables");
        assert!(err.to_string().contains("variables"), "got: {err}");
        assert!(server.received_requests().await.expect("recording").is_empty());
        assert_eq!(history.saved_count(), 0);
    }

    #[tokio::test]
    async fn execute_graphql_asks_for_a_choice_when_the_document_has_several_operations() {
        let server = MockServer::start().await;
        let (svc, _history) = service(Environment::new("dev"));
        let err = svc
            .execute_graphql(ExecuteGraphQlInput {
                request: input(&format!("{}/graphql", server.uri())),
                query: "query A { a } query B { b }".into(),
                variables: None,
                operation_name: None,
            })
            .await
            .expect_err("must choose");
        assert!(err.to_string().contains("choose one"), "got: {err}");
        assert!(server.received_requests().await.expect("recording").is_empty());
    }

    #[tokio::test]
    async fn execute_graphql_resolves_placeholders_in_query_and_variables() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("id", "42"));
        env.set_variable(Variable::new("count", "7"));
        let (svc, _history) = service(env);
        let mut request = input(&format!("{}/graphql", server.uri()));
        request.environment_name = Some("dev".into());

        svc.execute_graphql(ExecuteGraphQlInput {
            request,
            // Quotes and the compact `}}` must survive; the placeholders must resolve.
            query: "query Q($n: Int) {a{b(id: \"{{id}}\", n: $n)}}".into(),
            variables: Some("{\"n\": {{count}}}".into()),
            operation_name: None,
        })
        .await
        .expect("execute");

        let seen = server.received_requests().await.expect("recording");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json body");
        assert_eq!(body["query"], "query Q($n: Int) {a{b(id: \"42\", n: $n)}}");
        assert_eq!(body["variables"]["n"], 7);
    }
}
```

- [ ] **Step 7: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app graphql_request`
Expected: FAIL to compile (`build_wire`, `validate_variables`, `ExecuteGraphQlInput` not found).

- [ ] **Step 8: Implement the builder and the service method**

Put this above the test module in `crates/rocket-app/src/graphql_request.rs`:

```rust
//! Builds the HTTP request for a GraphQL operation and sends it through the
//! existing HTTP execution path, so variables, auth, scripts, assertions and
//! History work exactly as they do for an HTTP request.

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Body, BodyMode, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};

use crate::execution_service::{ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService};
use crate::graphql_document::select_operation;

/// What a GraphQL operation looks like on the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphQlWire {
    pub method: HttpMethod,
    /// A JSON body for POST. `None` for GET.
    pub body: Option<Body>,
    /// `query`, `operationName` and `variables` for GET. Empty for POST.
    pub query_params: Vec<QueryParam>,
}

/// Checks the variables text. Blank is fine and means no variables. Text with a
/// `{{placeholder}}` is not parsed, because it is not valid JSON until the
/// placeholder is resolved. Anything else must be a JSON object.
pub fn validate_variables(variables: &str) -> DomainResult<()> {
    let text = variables.trim();
    if text.is_empty() || text.contains("{{") {
        return Ok(());
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(_)) | Ok(serde_json::Value::Null) => Ok(()),
        Ok(_) => Err(DomainError::InvalidInput(
            "variables must be a JSON object".into(),
        )),
        Err(e) => Err(DomainError::InvalidInput(format!(
            "variables are not valid JSON: {e}"
        ))),
    }
}

fn json_string(text: &str) -> DomainResult<String> {
    serde_json::to_string(text)
        .map_err(|e| DomainError::Internal(format!("could not encode GraphQL text: {e}")))
}

fn param(key: &str, value: &str) -> QueryParam {
    QueryParam {
        key: key.into(),
        value: value.into(),
        enabled: true,
        description: None,
    }
}

/// Builds the wire form of one operation.
///
/// The POST body is assembled as text so the variables stay exactly as the
/// user wrote them, including an unresolved `{{placeholder}}`. The query and
/// the operation name are JSON-encoded, so quotes, newlines and unicode survive.
pub fn build_wire(
    method: HttpMethod,
    query: &str,
    variables: Option<&str>,
    operation_name: Option<&str>,
) -> DomainResult<GraphQlWire> {
    if query.trim().is_empty() {
        return Err(DomainError::InvalidInput("the query is empty".into()));
    }
    let variables = variables.map(str::trim).filter(|v| !v.is_empty());
    if let Some(v) = variables {
        validate_variables(v)?;
    }
    let operation_name = operation_name.map(str::trim).filter(|n| !n.is_empty());

    match method {
        HttpMethod::Post => {
            let mut text = String::from("{\"query\":");
            text.push_str(&json_string(query)?);
            if let Some(name) = operation_name {
                text.push_str(",\"operationName\":");
                text.push_str(&json_string(name)?);
            }
            if let Some(vars) = variables {
                text.push_str(",\"variables\":");
                text.push_str(vars);
            }
            text.push('}');
            Ok(GraphQlWire {
                method,
                body: Some(Body {
                    mode: BodyMode::Json,
                    content: Some(text),
                    form_data: None,
                    file_path: None,
                }),
                query_params: Vec::new(),
            })
        }
        HttpMethod::Get => {
            let mut params = vec![param("query", query)];
            if let Some(name) = operation_name {
                params.push(param("operationName", name));
            }
            if let Some(vars) = variables {
                params.push(param("variables", vars));
            }
            Ok(GraphQlWire {
                method,
                body: None,
                query_params: params,
            })
        }
        other => Err(DomainError::InvalidInput(format!(
            "GraphQL requests use GET or POST, not {other}"
        ))),
    }
}

/// A GraphQL send: the HTTP side of the request plus the GraphQL payload.
/// `request.body` is ignored; the payload replaces it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteGraphQlInput {
    pub request: ExecuteRequestInput,
    pub query: String,
    #[serde(default)]
    pub variables: Option<String>,
    /// The operation to run. Required when the document defines several.
    #[serde(default)]
    pub operation_name: Option<String>,
}

/// Applies the GraphQL payload to an HTTP input: method, body and query parameters.
pub fn apply_graphql_payload(
    input: &mut ExecuteRequestInput,
    query: &str,
    variables: Option<&str>,
    operation_name: Option<&str>,
) -> DomainResult<()> {
    let chosen = select_operation(query, operation_name, false)?;
    let wire = build_wire(input.method, query, variables, chosen.as_deref())?;
    input.method = wire.method;
    input.body = wire.body;
    input.query_params.extend(wire.query_params);
    Ok(())
}

impl RequestExecutionService {
    /// Sends a GraphQL operation through the HTTP execution path.
    pub async fn execute_graphql(
        &self,
        input: ExecuteGraphQlInput,
    ) -> DomainResult<ExecuteRequestOutput> {
        let mut request = input.request;
        apply_graphql_payload(
            &mut request,
            &input.query,
            input.variables.as_deref(),
            input.operation_name.as_deref(),
        )?;
        self.execute(request).await
    }
}
```

In `crates/rocket-app/src/lib.rs`, add to the re-exports:

```rust
pub use graphql_request::ExecuteGraphQlInput;
```

- [ ] **Step 9: Run the tests to verify they pass**

Run:
- `cargo test -j4 -p rocket-app graphql_request`
- `cargo test -j4 -p rocket-app graphql_document`

Expected: PASS. If `execute_graphql_resolves_placeholders_in_query_and_variables` shows the placeholder unresolved, the environment lookup is not reaching `StaticEnvRepo`: confirm `request.environment_name` is `Some("dev")` and `collection` is `None`, which is the path `execute_resolves_variables_in_url` in `execution_service.rs` uses.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-app/src/graphql_document.rs crates/rocket-app/src/graphql_request.rs crates/rocket-app/src/lib.rs
```

Suggested subject: `feat(app): execute GraphQL operations over the HTTP path`.

---

## Task 2: IPC commands and the GraphQL request tab

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/execution.rs`, `src-tauri/src/lib.rs`
- Modify: `src/lib/tauri-api.ts`, `src/lib/execute-request.ts`
- Create: `src/lib/dispatch-send.ts`, `src/lib/graphql-variables.ts`, `src/lib/request-profile.ts`
- Create: `src/components/request/GraphQlEditor.tsx`
- Modify: `src/components/request/RequestPanel.tsx`
- Create tests: `src/lib/__tests__/dispatch-send.test.ts`, `src/lib/__tests__/graphql-variables.test.ts`, `src/lib/__tests__/request-profile.test.ts`, `src/components/request/__tests__/GraphQlEditor.test.tsx`

**Interfaces:**
- Consumes: `ExecuteGraphQlInput`, `rocket_app::graphql_document::{GraphQlOperation, list_operations}` (Task 1), `RequestState.graphql`, `GraphQlState` (Plan 05).
- Produces:
  - Tauri commands `execute_graphql_request(input: ExecuteGraphQlInput) -> ExecuteRequestResponse` and `list_graphql_operations(document: String) -> Vec<GraphQlOperation>`.
  - TS: `ExecuteGraphQlInput`, `GraphQlOperation`, `executeGraphQlRequest`, `listGraphQlOperations`.
  - `dispatchSend(request, input, graphql)` and `ResolvedGraphQl` in `src/lib/dispatch-send.ts`.
  - `validateVariablesText(text: string): string | null` in `src/lib/graphql-variables.ts`.
  - `requestProfile(kind): { methods; bodyTabLabel; showLoadTest; showCopyAsCurl; initialSection }` in `src/lib/request-profile.ts`.
  - `<GraphQlEditor state onChange variableContext />`.

Justification for adding no package here: Monaco 0.55 already registers a `graphql` Monarch tokenizer (`node_modules/monaco-editor/esm/vs/basic-languages/graphql`), and `MonacoWrapper` already registers its variable hover for the `graphql` language id (`MonacoWrapper.tsx`, the `langIds` array). Syntax highlighting needs nothing more. Schema-aware completion is Plan 07's job and is the only thing that justifies `graphql` and `graphql-language-service`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Add the Tauri commands**

In `src-tauri/src/commands/execution.rs`, change the first import line to:

```rust
use rocket_app::graphql_document::{list_operations, GraphQlOperation};
use rocket_app::{
    ExecuteGraphQlInput, ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService,
};
```

and add after `execute_request`:

```rust
/// Sends a GraphQL operation through the HTTP execution path.
#[tauri::command]
pub async fn execute_graphql_request(
    input: ExecuteGraphQlInput,
    svc: State<'_, RequestExecutionService>,
) -> Result<ExecuteRequestResponse, DomainError> {
    svc.execute_graphql(input)
        .await
        .map(ExecuteRequestResponse::from)
}

/// Lists the operations a GraphQL document defines, for the operation picker.
#[tauri::command]
pub fn list_graphql_operations(document: String) -> Vec<GraphQlOperation> {
    list_operations(&document)
}
```

In `src-tauri/src/lib.rs`, add after `commands::execution::execute_request,`:

```rust
            commands::execution::execute_graphql_request,
            commands::execution::list_graphql_operations,
```

Run: `cargo check -j4 -p rocket`
Expected: PASS.

- [ ] **Step 3: Write the failing frontend library tests**

Create `src/lib/__tests__/graphql-variables.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { validateVariablesText } from '../graphql-variables';

describe('validateVariablesText', () => {
  it('accepts blank text, objects and null', () => {
    expect(validateVariablesText('')).toBeNull();
    expect(validateVariablesText('  \n')).toBeNull();
    expect(validateVariablesText('{"a": [1, 2]}')).toBeNull();
    expect(validateVariablesText('null')).toBeNull();
  });

  it('rejects arrays and scalars', () => {
    expect(validateVariablesText('[1]')).toBe('Variables must be a JSON object.');
    expect(validateVariablesText('5')).toBe('Variables must be a JSON object.');
  });

  it('rejects invalid JSON with the parser message', () => {
    expect(validateVariablesText('{"a": ')).toMatch(/^Variables are not valid JSON/);
  });

  it('does not judge text that holds a placeholder, which is not JSON until resolved', () => {
    expect(validateVariablesText('{"n": {{count}}}')).toBeNull();
  });
});
```

Create `src/lib/__tests__/request-profile.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { requestProfile } from '../request-profile';

describe('requestProfile', () => {
  it('keeps the full HTTP surface for http', () => {
    const p = requestProfile('http');
    expect(p.methods).toEqual(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD']);
    expect(p.bodyTabLabel).toBe('Body');
    expect(p.showLoadTest).toBe(true);
    expect(p.showCopyAsCurl).toBe(true);
    expect(p.initialSection).toBe('params');
  });

  it('limits graphql to POST and GET and hides what only fits HTTP bodies', () => {
    const p = requestProfile('graphql');
    expect(p.methods).toEqual(['POST', 'GET']);
    expect(p.bodyTabLabel).toBe('Query');
    expect(p.showLoadTest).toBe(false);
    expect(p.showCopyAsCurl).toBe(false);
    expect(p.initialSection).toBe('body');
  });
});
```

Create `src/lib/__tests__/dispatch-send.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  executeRequest: vi.fn().mockResolvedValue({ status: 200 }),
  executeGraphQlRequest: vi.fn().mockResolvedValue({ status: 200 }),
}));

import { executeGraphQlRequest, executeRequest } from '@/lib/tauri-api';
import { createDefaultRequestFor } from '../pane-utils';
import { dispatchSend } from '../dispatch-send';

const baseInput = {
  method: 'POST' as const,
  url: 'https://api.example.com/graphql',
  headers: [],
  queryParams: [],
  body: { mode: 'json' as const, content: 'stale' },
  auth: { authType: 'none' as const },
  options: { followRedirects: true, timeoutMs: 1000, verifySsl: true },
};

describe('dispatchSend', () => {
  beforeEach(() => vi.clearAllMocks());

  it('sends a graphql tab through executeGraphQlRequest and drops any http body', async () => {
    const request = createDefaultRequestFor('graphql');
    request.graphql = { query: '{ a }', variables: '', operationName: 'A' };
    await dispatchSend(request, baseInput, { query: '{ a }', variables: undefined });

    expect(executeRequest).not.toHaveBeenCalled();
    expect(executeGraphQlRequest).toHaveBeenCalledWith({
      request: { ...baseInput, body: undefined },
      query: '{ a }',
      variables: undefined,
      operationName: 'A',
    });
  });

  it('sends an http tab through executeRequest', async () => {
    await dispatchSend(createDefaultRequestFor('http'), baseInput, undefined);
    expect(executeGraphQlRequest).not.toHaveBeenCalled();
    expect(executeRequest).toHaveBeenCalledWith(baseInput);
  });
});
```

Create `src/components/request/__tests__/GraphQlEditor.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GraphQlEditor } from '../GraphQlEditor';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ language, value }: { language?: string; value: string }) => (
    <div data-testid={`monaco-${language}`}>{value}</div>
  ),
}));

vi.mock('@/lib/tauri-api', () => ({
  listGraphQlOperations: vi.fn(),
}));

import { listGraphQlOperations } from '@/lib/tauri-api';

describe('GraphQlEditor', () => {
  beforeEach(() => {
    vi.mocked(listGraphQlOperations).mockReset();
  });

  it('shows the query in a graphql editor and the variables in a json editor', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([{ name: 'A', kind: 'query' }]);
    render(
      <GraphQlEditor
        state={{ query: 'query A { a }', variables: '{"n": 1}' }}
        onChange={vi.fn()}
      />,
    );
    expect(await screen.findByTestId('monaco-graphql')).toHaveTextContent('query A { a }');
    expect(await screen.findByTestId('monaco-json')).toHaveTextContent('{"n": 1}');
  });

  it('selects the first named operation when the document has several', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([
      { name: 'A', kind: 'query' },
      { name: 'B', kind: 'mutation' },
    ]);
    const onChange = vi.fn();
    render(
      <GraphQlEditor
        state={{ query: 'query A { a } mutation B { b }', variables: '' }}
        onChange={onChange}
      />,
    );
    await waitFor(() => expect(onChange).toHaveBeenCalledWith({ operationName: 'A' }));
  });

  it('does not touch the operation for a single-operation document', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([{ name: 'A', kind: 'query' }]);
    const onChange = vi.fn();
    render(<GraphQlEditor state={{ query: 'query A { a }', variables: '' }} onChange={onChange} />);
    await screen.findByTestId('monaco-graphql');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('shows an inline error for variables that are not a JSON object', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([]);
    render(<GraphQlEditor state={{ query: '{ a }', variables: '[1]' }} onChange={vi.fn()} />);
    expect(await screen.findByText('Variables must be a JSON object.')).toBeTruthy();
  });
});
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `yarn test graphql-variables request-profile dispatch-send GraphQlEditor`
Expected: FAIL (modules do not exist).

- [ ] **Step 5: Implement the library pieces**

In `src/lib/tauri-api.ts`, add after `executeRequest`:

```ts
/** A GraphQL send: the HTTP side plus the GraphQL payload. `request.body` is ignored. */
export interface ExecuteGraphQlInput {
  request: ExecuteRequestInput;
  query: string;
  variables?: string;
  /** Required when the document defines several operations. */
  operationName?: string;
}

export interface GraphQlOperation {
  /** `null` for an anonymous operation. */
  name: string | null;
  kind: 'query' | 'mutation' | 'subscription';
}

export const executeGraphQlRequest = (input: ExecuteGraphQlInput) =>
  invoke<ExecuteRequestResponse>('execute_graphql_request', { input });

export const listGraphQlOperations = (document: string) =>
  invoke<GraphQlOperation[]>('list_graphql_operations', { document });
```

Create `src/lib/graphql-variables.ts`:

```ts
// Mirrors validate_variables in crates/rocket-app/src/graphql_request.rs, so the
// editor warns about the same text the backend would reject.
export function validateVariablesText(text: string): string | null {
  const trimmed = text.trim();
  // Text with a placeholder is not valid JSON until the placeholder is resolved.
  if (trimmed === '' || trimmed.includes('{{')) return null;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed === null || (typeof parsed === 'object' && !Array.isArray(parsed))) return null;
    return 'Variables must be a JSON object.';
  } catch (err) {
    return `Variables are not valid JSON: ${err instanceof Error ? err.message : String(err)}`;
  }
}
```

Create `src/lib/request-profile.ts`:

```ts
import type { HttpMethod, RequestState } from '@/types/pane-types';

export interface RequestProfile {
  methods: HttpMethod[];
  bodyTabLabel: 'Body' | 'Query';
  showLoadTest: boolean;
  showCopyAsCurl: boolean;
  initialSection: 'params' | 'body';
}

const HTTP_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD'];

// What the request panel offers for each protocol. Load test and copy-as-cURL
// read the HTTP body, which a GraphQL tab does not have.
export function requestProfile(kind: RequestState['requestType']): RequestProfile {
  if (kind === 'graphql') {
    return {
      methods: ['POST', 'GET'],
      bodyTabLabel: 'Query',
      showLoadTest: false,
      showCopyAsCurl: false,
      initialSection: 'body',
    };
  }
  return {
    methods: HTTP_METHODS,
    bodyTabLabel: 'Body',
    showLoadTest: true,
    showCopyAsCurl: true,
    initialSection: 'params',
  };
}
```

Create `src/lib/dispatch-send.ts`:

```ts
import {
  type ExecuteRequestInput,
  type ExecuteRequestResponse,
  executeGraphQlRequest,
  executeRequest,
} from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

/** The query and variables after `{{variable}}` resolution. */
export interface ResolvedGraphQl {
  query: string;
  variables?: string;
}

// Sends through the command that matches the tab's protocol. A GraphQL tab must
// never reach executeRequest: its HTTP body is empty and the backend builds the real one.
export function dispatchSend(
  request: RequestState,
  input: ExecuteRequestInput,
  graphql: ResolvedGraphQl | undefined,
): Promise<ExecuteRequestResponse> {
  if (request.requestType !== 'graphql') return executeRequest(input);
  return executeGraphQlRequest({
    request: { ...input, body: undefined },
    query: graphql?.query ?? '',
    variables: graphql?.variables,
    operationName: request.graphql?.operationName,
  });
}
```

In `src/lib/execute-request.ts`:

1. Add `import type { ResolvedGraphQl } from '@/lib/dispatch-send';` and `import { dispatchSend } from '@/lib/dispatch-send';` (one import statement with both).
2. Add `graphql?: ResolvedGraphQl;` to `ResolvedRequestFields`.
3. In `resolveRequestFieldsForPath`, after the `resolvedAssertions` constant, add:

```ts
  const resolvedGraphql: ResolvedGraphQl | undefined =
    request.requestType === 'graphql' && request.graphql
      ? {
          query: resolve(request.graphql.query),
          variables:
            request.graphql.variables.trim() === ''
              ? undefined
              : resolve(request.graphql.variables),
        }
      : undefined;
```

   and add `graphql: resolvedGraphql,` to the returned object (after `requestPath`).
4. In `sendRequest`, add `graphql: resolvedGraphql,` to the destructuring of `await resolveRequestFields(...)`, and replace

```ts
    const result = await executeRequest({
      method: effectiveRequest.method,
      ...
      requestGuardPolicy,
    });
```

   with the same object assigned first and dispatched through the helper:

```ts
    const requestInput: ExecuteRequestInput = {
      method: effectiveRequest.method,
      ...
      requestGuardPolicy,
    };
    const result = await dispatchSend(effectiveRequest, requestInput, resolvedGraphql);
```

   (keep every field of the original object literal; only the call wrapper changes). Import `type ExecuteRequestInput` from `@/lib/tauri-api` if it is not already imported there, and remove `executeRequest` from that import if nothing else in the file uses it (`yarn tsc --noEmit` will say).

- [ ] **Step 6: Implement `GraphQlEditor`**

Create `src/components/request/GraphQlEditor.tsx`:

```tsx
import { AlertTriangle } from 'lucide-react';
import { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { validateVariablesText } from '@/lib/graphql-variables';
import { type GraphQlOperation, listGraphQlOperations } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import type { GraphQlState } from '@/types/pane-types';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

interface GraphQlEditorProps {
  state: GraphQlState;
  onChange: (patch: Partial<GraphQlState>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

// Query on top, variables below, operation picker in the toolbar when the
// document defines several operations.
export function GraphQlEditor({ state, onChange, variableContext }: GraphQlEditorProps) {
  const [operations, setOperations] = useState<GraphQlOperation[]>([]);

  // Ask the backend scanner for the operation list, debounced while typing.
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      listGraphQlOperations(state.query)
        .then((ops) => {
          if (!cancelled) setOperations(ops);
        })
        .catch(() => {
          if (!cancelled) setOperations([]);
        });
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [state.query]);

  const names = useMemo(() => operations.flatMap((o) => (o.name ? [o.name] : [])), [operations]);
  const hasSeveral = operations.length > 1;

  // Keep the chosen operation valid as the document changes.
  useEffect(() => {
    if (!hasSeveral) {
      if (state.operationName !== undefined) onChange({ operationName: undefined });
      return;
    }
    if (!state.operationName || !names.includes(state.operationName)) {
      onChange({ operationName: names[0] });
    }
  }, [hasSeveral, names, state.operationName, onChange]);

  const variablesError = validateVariablesText(state.variables);

  return (
    <div className='flex h-full min-h-0 flex-col'>
      {hasSeveral && (
        <div className='flex items-center gap-2 border-b border-border px-3 py-1.5 shrink-0'>
          <span className='text-xs text-muted-foreground'>Operation</span>
          <Select
            value={state.operationName ?? ''}
            onValueChange={(v) => onChange({ operationName: v })}
          >
            <SelectTrigger className='h-7 w-56 text-xs' aria-label='Operation'>
              <SelectValue placeholder='Choose an operation' />
            </SelectTrigger>
            <SelectContent>
              {operations.map((op) =>
                op.name ? (
                  <SelectItem key={op.name} value={op.name} className='text-xs'>
                    {op.kind} {op.name}
                  </SelectItem>
                ) : null,
              )}
            </SelectContent>
          </Select>
        </div>
      )}

      <div className='flex-1 min-h-0'>
        <Suspense fallback={<EditorSkeleton />}>
          <MonacoWrapper
            value={state.query}
            onChange={(query) => onChange({ query })}
            language='graphql'
            height='100%'
            variableContext={variableContext}
          />
        </Suspense>
      </div>

      <div className='flex h-44 shrink-0 flex-col border-t border-border'>
        <div className='flex items-center justify-between px-3 py-1 shrink-0'>
          <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
            Variables (JSON)
          </span>
          {variablesError && (
            <span
              role='alert'
              className='flex items-center gap-1 text-xs text-destructive'
            >
              <AlertTriangle className='h-3 w-3' aria-hidden='true' />
              {variablesError}
            </span>
          )}
        </div>
        <div className='flex-1 min-h-0'>
          <Suspense fallback={<EditorSkeleton />}>
            <MonacoWrapper
              value={state.variables}
              onChange={(variables) => onChange({ variables })}
              language='json'
              height='100%'
              variableContext={variableContext}
            />
          </Suspense>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Wire `GraphQlEditor` into `RequestPanel`**

In `src/components/request/RequestPanel.tsx`:

1. Imports: `import { GraphQlEditor } from './GraphQlEditor';`, `import { requestProfile } from '@/lib/request-profile';`, and `GraphQlState` from `@/types/pane-types` (add to the existing type import).
2. Right after `const { request, response } = tab;`:

```tsx
  const profile = requestProfile(request.requestType);
  const isGraphQl = request.requestType === 'graphql';
```

3. Change the section state initialiser to `useState<SectionTab>(profile.initialSection)`.
4. Add the handler next to `handleBodyChange`. It depends on `request.graphql` only:

```tsx
  const handleGraphQlChange = useCallback(
    (patch: Partial<GraphQlState>) =>
      updateRequest(tab.id, {
        graphql: { ...(request.graphql ?? { query: '', variables: '' }), ...patch },
      }),
    [tab.id, updateRequest, request.graphql],
  );
```

5. In `tabDefs`, change the body tab label from the literal `Body` to `{profile.bodyTabLabel}`, base its dot indicator on `isGraphQl ? request.graphql?.query.trim() !== '' : request.body.mode !== 'none'`, and filter the array before returning: `.filter((t) => profile.showLoadTest || t.value !== 'load-test')`. Add `profile.bodyTabLabel`, `profile.showLoadTest`, `isGraphQl` and `request.graphql?.query` to the `useMemo` dependency list.
6. In `urlBar`, replace `METHODS.map((m) => ...)` with `profile.methods.map((m) => ...)`. Wrap the Load test button and the Copy as cURL button each in `{profile.showLoadTest && (...)}` and `{profile.showCopyAsCurl && (...)}`.
7. Render the editor. Add this block before the generic section container (next to the `activeSection === 'scripts'` block):

```tsx
      {activeSection === 'body' && isGraphQl ? (
        <div className='flex-1 min-h-0 overflow-hidden'>
          <GraphQlEditor
            state={request.graphql ?? { query: '', variables: '' }}
            onChange={handleGraphQlChange}
            variableContext={scopedContext}
          />
        </div>
      ) : null}
```

   Add `(activeSection === 'body' && isGraphQl) ||` to the condition that applies the `hidden` class to the generic container, and change the generic `{activeSection === 'body' && (<BodyEditor ... />)}` to `{activeSection === 'body' && !isGraphQl && (...)}`. The body-mode selector that `tabRightContent` shows for the `body` section must return `undefined` for GraphQL: add `if (isGraphQl) return undefined;` at the top of the `activeSection === 'body'` branch and add `isGraphQl` to that memo's dependencies.

- [ ] **Step 8: Run the checks**

Run:
- `yarn test graphql-variables request-profile dispatch-send GraphQlEditor execute-request`
- `yarn tsc --noEmit`
- `yarn check`
- `cargo check -j4 -p rocket`

Expected: PASS. If `GraphQlEditor` re-runs the operation effect forever in the test with `vi.fn()` as `onChange`, check that the effect only calls `onChange` when the chosen operation is wrong, which is the case in the code above.

- [ ] **Step 9: Verify the language support and try it in the app**

Run: `ls node_modules/monaco-editor/esm/vs/basic-languages/graphql` and `grep -n "graphql" node_modules/monaco-editor/esm/vs/basic-languages/monaco.contribution.js`
Expected: the directory exists and the contribution registers `graphql`.

Run `yarn tauri dev`. Open a GraphQL request from Plan 05, type a query against a public GraphQL endpoint, press Send, and confirm: the query is highlighted, a two-operation document shows the Operation picker and sends only the chosen one, a `[1]` in Variables shows the inline error and Send returns a clear error without a network call, and switching the method to GET sends a query-string request.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add src-tauri/src/commands/execution.rs src-tauri/src/lib.rs \
  src/lib/tauri-api.ts src/lib/execute-request.ts src/lib/dispatch-send.ts \
  src/lib/graphql-variables.ts src/lib/request-profile.ts \
  src/lib/__tests__/dispatch-send.test.ts src/lib/__tests__/graphql-variables.test.ts \
  src/lib/__tests__/request-profile.test.ts \
  src/components/request/GraphQlEditor.tsx src/components/request/RequestPanel.tsx \
  src/components/request/__tests__/GraphQlEditor.test.tsx
```

Suggested subject: `feat(ui): edit and send GraphQL requests from a request tab`.

---

## Task 3: Response handling, History and the Collection Runner

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/graphql_request.rs`, `runner_sequence.rs`, `collection_runner_service.rs`, `flow_execution_service.rs`, `test_doubles.rs`
- Create: `src/lib/graphql-response.ts`, `src/components/response/GraphQlErrorsPanel.tsx`
- Modify: `src/types/pane-types.ts`, `src/lib/execute-request.ts`, `src/lib/runner-flatten.ts`, `src/lib/runner-execute.ts`, `src/stores/pane-store.ts`
- Modify: `src/components/response/ResponseBodyViewer.tsx`, `src/components/request/runner/RunnerRequestList.tsx`, `RunnerResultsList.tsx`
- Create tests: `src/lib/__tests__/graphql-response.test.ts`, `src/components/response/__tests__/ResponseBodyViewer.graphql.test.tsx`
- Modify tests: `src/lib/__tests__/runner-flatten.test.ts`, `src/lib/__tests__/runner-execute.test.ts`, `src/stores/__tests__/pane-store.test.ts`

**Interfaces:**
- Consumes: `build_wire`, `select_operation` (Task 1), `CollectionItem::GraphQl`, `RequestKind` (Plan 05), `dispatchSend` (Task 2).
- Produces:
  - `rocket_app::graphql_request::{to_http_request, response_error_summary}`:
    - `to_http_request(g: &GraphQlRequest, operation_name: Option<&str>) -> DomainResult<Request>`: the HTTP form of a saved GraphQL request, using the first operation when the document has several.
    - `response_error_summary(body: &str) -> Option<String>`: `Some("2 GraphQL errors: first message")` when the body is a JSON object with a non-empty `errors` array.
  - `RunItem { kind: RequestKind, prepare_error: Option<String> }` with constructors `RunItem::http(name, request_path, request)` and `RunItem::graphql(name, request_path, request)`.
  - `RecordingExecutor::{set_body, sent_bodies}` (test double).
  - TS: `parseGraphQlResponse(body)`, `ResponseState.protocol?: 'graphql'`, `ResponseState.activeView` gains `'data' | 'errors'`, `RunnerRequestEntry.graphql?: GraphQlRequest`, `executeRunnerEntry(collection, requestPath, request, environmentName, graphql?)`.

Decisions this task fixes, so the implementer does not have to choose:
- A GraphQL response counts as failed when `errors` is non-empty, even with partial `data` and HTTP 200.
- The runner picks the first operation of a multi-operation document, because a run has no operation picker. Anything that makes the request unbuildable (empty query, invalid variables) becomes an errored step, never a silent skip.
- History stores method, URL, status and timing only (`HistoryEntry` has no body or protocol field), so a GraphQL send appears as a `POST` of the endpoint URL. Adding a protocol label to `HistoryEntry` is out of scope.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing Rust tests**

Append to the `tests` module of `crates/rocket-app/src/graphql_request.rs`:

```rust
    #[test]
    fn response_error_summary_reports_a_non_empty_errors_array() {
        assert_eq!(
            response_error_summary(r#"{"data":null,"errors":[{"message":"boom"},{"message":"x"}]}"#),
            Some("2 GraphQL errors: boom".to_string())
        );
        assert_eq!(
            response_error_summary(r#"{"errors":[{"message":"only"}]}"#),
            Some("1 GraphQL error: only".to_string())
        );
    }

    #[test]
    fn response_error_summary_is_none_for_clean_or_foreign_bodies() {
        assert_eq!(response_error_summary(r#"{"data":{"a":1}}"#), None);
        assert_eq!(response_error_summary(r#"{"data":{},"errors":[]}"#), None);
        assert_eq!(response_error_summary("not json"), None);
        assert_eq!(response_error_summary("[1,2]"), None);
        assert_eq!(response_error_summary(r#"{"errors":"a string"}"#), None);
    }

    #[test]
    fn to_http_request_projects_a_saved_graphql_request() {
        use rocket_collection::GraphQlRequest;
        let mut g = GraphQlRequest::new("Users", "https://api.example.com/graphql")
            .with_query("query A { a } query B { b }");
        g.body.variables = Some("{\"n\": 1}".into());
        g.headers.push(rocket_shared::types::Header::new("X-Trace", "1"));
        g.pre_request_script = Some("// pre".into());
        g.tags = vec!["smoke".into()];
        g.file_name = Some("users.yml".into());

        let r = to_http_request(&g, None).expect("request");
        assert_eq!(r.method, HttpMethod::Post);
        assert_eq!(r.uid, g.uid);
        assert_eq!(r.url, "https://api.example.com/graphql");
        assert_eq!(r.headers.len(), 1);
        assert_eq!(r.pre_request_script.as_deref(), Some("// pre"));
        assert_eq!(r.tags, vec!["smoke".to_string()]);
        assert_eq!(r.file_name.as_deref(), Some("users.yml"));
        let body: serde_json::Value = serde_json::from_str(
            r.body.as_ref().and_then(|b| b.content.as_deref()).expect("body"),
        )
        .expect("json");
        assert_eq!(body["operationName"], "A", "the runner runs the first operation");
        assert_eq!(body["variables"]["n"], 1);
    }

    #[test]
    fn to_http_request_fails_for_an_unbuildable_request() {
        use rocket_collection::GraphQlRequest;
        let empty = GraphQlRequest::new("Empty", "https://x/graphql");
        assert!(to_http_request(&empty, None).is_err());
        let mut bad = GraphQlRequest::new("Bad", "https://x/graphql").with_query("{ a }");
        bad.body.variables = Some("[1]".into());
        assert!(to_http_request(&bad, None).is_err());
    }
```

Add to the `tests` module of `crates/rocket-app/src/runner_sequence.rs`:

```rust
    #[test]
    fn graphql_items_become_run_steps_with_a_json_body() {
        use rocket_collection::{CollectionItem, GraphQlRequest, RequestKind};
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("Login", "login.yml"));
        let mut g = GraphQlRequest::new("Search", "https://api.test/graphql").with_query("{ a }");
        g.file_name = Some("search.yml".into());
        collection.root.items.push(CollectionItem::GraphQl(Box::new(g)));

        let items = flatten_run_set(&collection, None).expect("flatten");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].name, "Search");
        assert_eq!(items[1].request_path, "search.yml");
        assert_eq!(items[1].kind, RequestKind::GraphQl);
        assert!(items[1].prepare_error.is_none());
        assert_eq!(items[1].request.method, rocket_shared::types::HttpMethod::Post);
        assert!(items[1].request.body.is_some());
    }

    #[test]
    fn an_unbuildable_graphql_item_is_kept_as_an_errored_step_not_dropped() {
        use rocket_collection::{CollectionItem, GraphQlRequest};
        let mut collection = Collection::new("my-api");
        let mut g = GraphQlRequest::new("Broken", "https://api.test/graphql");
        g.file_name = Some("broken.yml".into()); // no query
        collection.root.items.push(CollectionItem::GraphQl(Box::new(g)));

        let items = flatten_run_set(&collection, None).expect("flatten");
        assert_eq!(items.len(), 1);
        assert!(items[0]
            .prepare_error
            .as_deref()
            .expect("error")
            .contains("query is empty"));
    }
```

Add to the `tests` module of `crates/rocket-app/src/collection_runner_service.rs`:

```rust
    fn graphql_collection() -> Collection {
        use rocket_collection::{CollectionItem, GraphQlRequest};
        let mut collection = Collection::new("my-api");
        let mut g = GraphQlRequest::new("Search", "https://api.test/gql").with_query("{ a }");
        g.file_name = Some("search.yml".into());
        collection
            .root
            .items
            .push(CollectionItem::GraphQl(Box::new(g)));
        collection
    }

    #[tokio::test]
    async fn graphql_errors_fail_a_runner_step() {
        let executor = RecordingExecutor::new();
        executor.set_body(
            "api.test/gql",
            r#"{"data":null,"errors":[{"message":"boom"}]}"#,
        );
        let h = harness(graphql_collection(), ProgrammableEngine::new(), executor);
        let summary = h
            .runner
            .run(&h.exec, sample_run_input())
            .await
            .expect("run");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].status_code, Some(200));
        assert!(summary.steps[0].is_failure(), "HTTP 200 with errors[] is a failure");
        assert!(summary.steps[0]
            .error
            .as_deref()
            .expect("error text")
            .contains("boom"));
    }

    #[tokio::test]
    async fn a_clean_graphql_response_passes_and_sends_a_json_body() {
        let executor = RecordingExecutor::new();
        executor.set_body("api.test/gql", r#"{"data":{"a":1}}"#);
        let h = harness(
            graphql_collection(),
            ProgrammableEngine::new(),
            Arc::clone(&executor),
        );
        let summary = h
            .runner
            .run(&h.exec, sample_run_input())
            .await
            .expect("run");

        assert!(!summary.steps[0].is_failure());
        let bodies = executor.sent_bodies();
        assert_eq!(bodies.len(), 1);
        assert!(bodies[0].as_deref().expect("body").contains("\"query\""));
    }

    #[tokio::test]
    async fn an_unbuildable_graphql_item_errors_the_step_without_sending() {
        use rocket_collection::{CollectionItem, GraphQlRequest};
        let mut collection = Collection::new("my-api");
        let mut g = GraphQlRequest::new("Broken", "https://api.test/gql");
        g.file_name = Some("broken.yml".into());
        collection
            .root
            .items
            .push(CollectionItem::GraphQl(Box::new(g)));
        let executor = RecordingExecutor::new();
        let h = harness(collection, ProgrammableEngine::new(), Arc::clone(&executor));
        let summary = h
            .runner
            .run(&h.exec, sample_run_input())
            .await
            .expect("run");

        assert_eq!(summary.steps[0].status, RunStepStatus::Error);
        assert!(executor.sent_urls().is_empty());
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app graphql`
Expected: FAIL to compile (`to_http_request`, `response_error_summary`, `RunItem::kind`, `set_body` not found).

- [ ] **Step 4: Extend the test double**

In `crates/rocket-app/src/test_doubles.rs`, change `RecordingExecutor` to record bodies and serve canned bodies:

```rust
pub struct RecordingExecutor {
    sent: Mutex<Vec<String>>,
    sent_auth: Mutex<Vec<rocket_shared::types::Auth>>,
    sent_bodies: Mutex<Vec<Option<String>>>,
    statuses: Mutex<HashMap<String, u16>>,
    bodies: Mutex<HashMap<String, String>>,
}
```

In `RecordingExecutor::new`, initialise the two new fields with `Mutex::new(Vec::new())` and `Mutex::new(HashMap::new())`. Add the methods:

```rust
    /// Registers a response body for any URL containing `url_substring`.
    pub fn set_body(&self, url_substring: &str, body: &str) {
        self.bodies
            .lock()
            .expect("lock")
            .insert(url_substring.to_string(), body.to_string());
    }
    /// The request body content of every send, in order.
    pub fn sent_bodies(&self) -> Vec<Option<String>> {
        self.sent_bodies.lock().expect("lock").clone()
    }
```

In `impl HttpExecutor for RecordingExecutor`, record the body next to the auth line, and use the canned body in the response:

```rust
        self.sent_bodies
            .lock()
            .expect("lock")
            .push(req.body.as_ref().and_then(|b| b.content.clone()));
```

```rust
        let body = self
            .bodies
            .lock()
            .expect("lock")
            .iter()
            .find(|(fragment, _)| req.url.contains(fragment.as_str()))
            .map(|(_, body)| body.clone())
            .unwrap_or_else(|| "{}".to_string());
        Ok(HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![],
            size_bytes: body.len(),
            body,
            duration_ms: 1,
            ttfb_ms: 1,
        })
```

(this replaces the existing `Ok(HttpResponse { ... body: "{}".into(), ... size_bytes: 2 })`).

- [ ] **Step 5: Implement the projection and the error summary**

Append to the non-test part of `crates/rocket-app/src/graphql_request.rs` (above `#[cfg(test)]`):

```rust
use rocket_collection::{GraphQlRequest, Request};

/// The HTTP form of a saved GraphQL request, for the Collection Runner.
///
/// A document with several operations runs its first one unless `operation_name`
/// names another, because a run has no operation picker.
pub fn to_http_request(
    g: &GraphQlRequest,
    operation_name: Option<&str>,
) -> DomainResult<Request> {
    let chosen = select_operation(&g.body.query, operation_name, true)?;
    let wire = build_wire(
        g.method,
        &g.body.query,
        g.body.variables.as_deref(),
        chosen.as_deref(),
    )?;
    let mut r = Request::new(g.name.clone(), wire.method, g.url.clone());
    r.uid = g.uid.clone();
    r.headers = g.headers.clone();
    r.query_params = g.query_params.iter().cloned().chain(wire.query_params).collect();
    r.path_params = g.path_params.clone();
    r.body = wire.body;
    r.auth = g.auth.clone();
    r.file_name = g.file_name.clone();
    r.seq = g.seq;
    r.tags = g.tags.clone();
    r.description = g.description.clone();
    r.pre_request_script = g.pre_request_script.clone();
    r.post_response_script = g.post_response_script.clone();
    r.tests = g.tests.clone();
    r.assertions = g.assertions.clone();
    r.actions = g.actions.clone();
    r.docs = g.docs.clone();
    r.variables = g.variables.clone();
    r.runtime_auth = g.runtime_auth.clone();
    r.settings = g.settings.clone();
    Ok(r)
}

/// A one-line summary when a GraphQL response body carries a non-empty `errors`
/// array, such as `2 GraphQL errors: boom`. `None` for a clean response or a
/// body that is not a GraphQL response.
pub fn response_error_summary(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let errors = value.get("errors")?.as_array()?;
    let first = errors.first()?;
    let message = first
        .get("message")
        .and_then(|m| m.as_str())
        .or_else(|| first.as_str())
        .unwrap_or("unknown error");
    let noun = if errors.len() == 1 { "error" } else { "errors" };
    Some(format!("{} GraphQL {noun}: {message}", errors.len()))
}
```

Move the `use rocket_collection::{GraphQlRequest, Request};` line to join the other `use` lines at the top of the file.

- [ ] **Step 6: Implement `RunItem` kind, the flatten arm and the failure rule**

In `crates/rocket-app/src/runner_sequence.rs`:

1. Extend the imports: `use rocket_collection::{Collection, CollectionItem, Folder, Request, RequestKind};`.
2. Replace the `RunItem` struct and add constructors:

```rust
/// One executable step in a run set.
#[derive(Debug, Clone)]
pub struct RunItem {
    /// Display name. This is what `rok.runner.setNextRequest(name)` matches on.
    pub name: String,
    /// Path relative to the collection root, e.g. `"auth/login.yml"`.
    pub request_path: String,
    /// The saved request definition. For GraphQL it is the HTTP form of the item.
    pub request: Request,
    /// Which protocol the item came from. GraphQL steps also fail on `errors[]`.
    pub kind: RequestKind,
    /// Set when the item could not be turned into a request, so the step errors
    /// instead of being skipped silently.
    pub prepare_error: Option<String>,
}

impl RunItem {
    pub fn http(name: String, request_path: String, request: Request) -> Self {
        Self {
            name,
            request_path,
            request,
            kind: RequestKind::Http,
            prepare_error: None,
        }
    }

    pub fn graphql(
        name: String,
        request_path: String,
        request: Request,
        prepare_error: Option<String>,
    ) -> Self {
        Self {
            name,
            request_path,
            request,
            kind: RequestKind::GraphQl,
            prepare_error,
        }
    }
}
```

3. In `collect_items`, replace the `out.push(RunItem { ... })` of the HTTP arm with `out.push(RunItem::http(request.name.clone(), format!("{prefix}{file_name}"), request.as_ref().clone()));`, and add this arm before the skip arm (and drop `CollectionItem::GraphQl(_) |` from the skip arm, restoring the comment to mention only non-HTTP protocols and summaries):

```rust
            CollectionItem::GraphQl(gql) => {
                let Some(file_name) = gql.file_name.as_ref() else {
                    tracing::warn!(
                        request = %gql.name,
                        "run set: GraphQL request has no on-disk file name, skipping"
                    );
                    continue;
                };
                let request_path = format!("{prefix}{file_name}");
                match crate::graphql_request::to_http_request(gql, None) {
                    Ok(request) => out.push(RunItem::graphql(
                        gql.name.clone(),
                        request_path,
                        request,
                        None,
                    )),
                    Err(e) => out.push(RunItem::graphql(
                        gql.name.clone(),
                        request_path,
                        Request::new(gql.name.clone(), gql.method, gql.url.clone()),
                        Some(e.to_string()),
                    )),
                }
            }
```

4. Replace every other `RunItem { name: ..., request_path: ..., request }` literal with `RunItem::http(name, request_path, request)`: the four in this file's tests (around lines 278, 308, 337, 364) and the one in `crates/rocket-app/src/flow_execution_service.rs` (around line 411). For example the Flow one becomes:

```rust
    let item = RunItem::http(request.name.clone(), request_path, request);
```

   Update the doc comment of `flatten_run_set` to say "opaque protocol items (gRPC/WebSocket) and sidebar summaries are not executable" and that GraphQL items become steps.

In `crates/rocket-app/src/collection_runner_service.rs`:

1. At the top of `run_step`, before building `step_input`, add:

```rust
        // A GraphQL item that could not be built is an errored step, never a silent skip.
        if let Some(message) = &item.prepare_error {
            return StepOutcome {
                result: error_step(index, item, message.clone()),
                next_request: None,
            };
        }
```

2. Replace the `passed` / `failed` / `StepOutcome` tail of `run_step` with:

```rust
        let passed = output
            .test_results
            .iter()
            .filter(|t| t.status == rocket_scripting::TestStatus::Passed)
            .count();
        let mut failed = output.test_results.len() - passed;

        // HTTP 200 with a non-empty `errors` array is a failed GraphQL operation.
        let graphql_error = if item.kind == rocket_collection::RequestKind::GraphQl {
            crate::graphql_request::response_error_summary(&output.response.body)
        } else {
            None
        };
        if graphql_error.is_some() {
            failed += 1;
        }

        StepOutcome {
            next_request: state.next_request.clone(),
            result: RunStepResult {
                index,
                item_name: item.name.clone(),
                request_path: item.request_path.clone(),
                status: RunStepStatus::Completed,
                status_code: Some(output.response.status),
                duration_ms: output.response.duration_ms,
                test_pass_count: passed,
                test_fail_count: failed,
                script_error: output.script_error.clone(),
                error: graphql_error,
            },
        }
```

3. Update the `error` field doc on `RunStepResult` to "Transport or sequencing error, or the summary of a GraphQL `errors` array."

- [ ] **Step 7: Run the Rust tests to verify they pass**

Run:
- `cargo test -j4 -p rocket-app graphql`
- `cargo test -j4 -p rocket-app runner_sequence`
- `cargo test -j4 -p rocket-app collection_runner_service`
- `cargo test -j4 -p rocket-app flow_execution_service`

Expected: PASS. Every pre-existing runner and Flow test keeps passing, because `RunItem::http` builds the same value the literals did.

- [ ] **Step 8: Write the failing frontend tests**

Create `src/lib/__tests__/graphql-response.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { parseGraphQlResponse } from '../graphql-response';

describe('parseGraphQlResponse', () => {
  it('reads data and no errors from a clean response', () => {
    const r = parseGraphQlResponse('{"data":{"a":1}}');
    expect(r.isGraphQl).toBe(true);
    expect(r.data).toEqual({ a: 1 });
    expect(r.errors).toEqual([]);
  });

  it('reads errors with path, locations and extensions', () => {
    const r = parseGraphQlResponse(
      JSON.stringify({
        data: null,
        errors: [
          {
            message: 'boom',
            path: ['user', 0, 'name'],
            locations: [{ line: 2, column: 3 }],
            extensions: { code: 'FORBIDDEN' },
          },
        ],
      }),
    );
    expect(r.errors).toHaveLength(1);
    expect(r.errors[0]).toMatchObject({
      message: 'boom',
      path: ['user', 0, 'name'],
      locations: [{ line: 2, column: 3 }],
      extensions: { code: 'FORBIDDEN' },
    });
  });

  it('treats a string error as a message', () => {
    expect(parseGraphQlResponse('{"errors":["plain"]}').errors[0].message).toBe('plain');
  });

  it('is not a graphql response for other bodies', () => {
    expect(parseGraphQlResponse('<html></html>').isGraphQl).toBe(false);
    expect(parseGraphQlResponse('[1,2]').isGraphQl).toBe(false);
    expect(parseGraphQlResponse('{"status":"ok"}').isGraphQl).toBe(false);
  });
});
```

Create `src/components/response/__tests__/ResponseBodyViewer.graphql.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ResponseState } from '@/types/pane-types';
import { ResponseBodyViewer } from '../ResponseBodyViewer';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value }: { value: string }) => <pre data-testid='monaco'>{value}</pre>,
}));

function response(body: string, extra: Partial<ResponseState> = {}): ResponseState {
  return {
    status: 200,
    statusText: 'OK',
    headers: [{ id: 'h', key: 'content-type', value: 'application/json', enabled: true }],
    body,
    durationMs: 5,
    ttfbMs: 3,
    sizeBytes: body.length,
    activeView: 'pretty',
    ...extra,
  };
}

describe('ResponseBodyViewer for GraphQL', () => {
  it('shows Data and Errors tabs only for a graphql response', () => {
    const { rerender } = render(<ResponseBodyViewer response={response('{"data":{"a":1}}')} />);
    expect(screen.queryByRole('tab', { name: /^Data/ })).toBeNull();

    rerender(
      <ResponseBodyViewer response={response('{"data":{"a":1}}', { protocol: 'graphql' })} />,
    );
    expect(screen.getByRole('tab', { name: /^Data/ })).toBeTruthy();
    expect(screen.getByRole('tab', { name: /^Errors/ })).toBeTruthy();
  });

  it('puts the error count on the Errors tab and lists each message', async () => {
    const body = JSON.stringify({
      data: null,
      errors: [{ message: 'boom', path: ['a', 'b'] }, { message: 'second' }],
    });
    render(<ResponseBodyViewer response={response(body, { protocol: 'graphql', activeView: 'errors' })} />);
    expect(screen.getByRole('tab', { name: /Errors\s*\(2\)/ })).toBeTruthy();
    expect(screen.getByText('boom')).toBeTruthy();
    expect(screen.getByText('a.b')).toBeTruthy();
    expect(screen.getByText('second')).toBeTruthy();
  });

  it('shows only the data member in the Data tab', async () => {
    const body = JSON.stringify({ data: { a: 1 }, extensions: { cost: 3 } });
    render(<ResponseBodyViewer response={response(body, { protocol: 'graphql' })} />);
    await userEvent.click(screen.getByRole('tab', { name: /^Data/ }));
    const shown = screen.getByTestId('monaco').textContent ?? '';
    expect(JSON.parse(shown)).toEqual({ a: 1 });
  });
});
```

Add to `src/lib/__tests__/runner-flatten.test.ts` a case (inside the existing `describe` for `flattenRunnerEntries`):

```ts
  it('includes typed graphql items as runnable entries that keep their graphql payload', () => {
    const collection = makeCollection();
    collection.root.items.push({
      type: 'graphql',
      uid: 'g1',
      name: 'Search',
      method: 'POST',
      url: 'https://example.com/graphql',
      headers: [],
      auth: { authType: 'none' },
      body: { query: '{ a }' },
      fileName: 'search.yml',
    });
    const entries = flattenRunnerEntries(collection);
    const gql = entries.find((e) => e.requestPath === 'search.yml');
    expect(gql?.graphql?.body.query).toBe('{ a }');
    expect(gql?.request.name).toBe('Search');
    expect(gql?.request.method).toBe('POST');
    expect(gql?.included).toBe(true);
  });
```

Add to `src/lib/__tests__/runner-execute.test.ts`: extend the `vi.mock('@/lib/tauri-api', ...)` factory with `executeGraphQlRequest: vi.fn()`, import it, and add:

```ts
  it('sends a graphql entry through executeGraphQlRequest and fails it on errors[]', async () => {
    vi.mocked(executeGraphQlRequest).mockResolvedValue({
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '{"data":null,"errors":[{"message":"boom"}]}',
      durationMs: 10,
      ttfbMs: 5,
      sizeBytes: 40,
      testResults: [],
      consoleEntries: [],
      scriptError: null,
    });
    const graphql = {
      uid: 'g1',
      name: 'Search',
      method: 'POST' as const,
      url: 'https://example.com/graphql',
      headers: [],
      auth: { authType: 'none' as const },
      body: { query: '{ a }' },
    };
    const outcome = await executeRunnerEntry(
      'demo',
      'search.yml',
      { ...baseRequest(), name: 'Search' },
      undefined,
      graphql,
    );

    expect(executeRequest).not.toHaveBeenCalled();
    expect(executeGraphQlRequest).toHaveBeenCalledWith(
      expect.objectContaining({ query: '{ a }', request: expect.objectContaining({ body: undefined }) }),
    );
    expect(outcome.status).toBe('failed');
  });
```

The `resolveRequestFieldsForPath` mock in that file returns no `graphql` member, so the runner code must resolve the query itself when `graphql` is passed (see Step 9): use `resolved.graphql ?? { query: graphql.body.query, variables: graphql.body.variables ?? undefined }`.

In `src/stores/__tests__/pane-store.test.ts`, find the assertion around line 714 that checks `executeRunnerEntry` was called with four arguments and append `undefined` as the fifth expected argument (the store now passes `entry.graphql`).

- [ ] **Step 9: Run the tests to verify they fail**

Run: `yarn test graphql-response ResponseBodyViewer.graphql runner-flatten runner-execute pane-store`
Expected: FAIL (module and props missing).

- [ ] **Step 10: Implement the frontend response and runner pieces**

Create `src/lib/graphql-response.ts`:

```ts
export interface GraphQlError {
  message: string;
  path?: (string | number)[];
  locations?: { line: number; column: number }[];
  extensions?: Record<string, unknown>;
}

export interface ParsedGraphQlResponse {
  /** True when the body is a JSON object with a `data` or `errors` member. */
  isGraphQl: boolean;
  data: unknown;
  errors: GraphQlError[];
}

function toError(raw: unknown): GraphQlError {
  if (typeof raw === 'string') return { message: raw };
  if (raw && typeof raw === 'object') {
    const e = raw as Record<string, unknown>;
    return {
      message: typeof e.message === 'string' ? e.message : 'Unknown error',
      path: Array.isArray(e.path) ? (e.path as (string | number)[]) : undefined,
      locations: Array.isArray(e.locations)
        ? (e.locations as { line: number; column: number }[])
        : undefined,
      extensions:
        e.extensions && typeof e.extensions === 'object'
          ? (e.extensions as Record<string, unknown>)
          : undefined,
    };
  }
  return { message: 'Unknown error' };
}

// Reads a GraphQL-over-HTTP response body. Anything that is not a JSON object
// with `data` or `errors` is reported as not GraphQL, so the Data and Errors
// tabs fall back to the plain body view.
export function parseGraphQlResponse(body: string): ParsedGraphQlResponse {
  const empty: ParsedGraphQlResponse = { isGraphQl: false, data: undefined, errors: [] };
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return empty;
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return empty;
  const obj = parsed as Record<string, unknown>;
  if (!('data' in obj) && !('errors' in obj)) return empty;
  return {
    isGraphQl: true,
    data: obj.data,
    errors: Array.isArray(obj.errors) ? obj.errors.map(toError) : [],
  };
}
```

Create `src/components/response/GraphQlErrorsPanel.tsx`:

```tsx
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import type { GraphQlError } from '@/lib/graphql-response';

interface GraphQlErrorsPanelProps {
  errors: GraphQlError[];
}

// Lists the entries of a GraphQL `errors` array with their path and location.
export function GraphQlErrorsPanel({ errors }: GraphQlErrorsPanelProps) {
  if (errors.length === 0) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-2 text-muted-foreground'>
        <CheckCircle2 className='h-8 w-8 opacity-20' />
        <span className='text-xs'>No errors in the response</span>
      </div>
    );
  }
  return (
    <ul className='h-full overflow-auto p-3 space-y-2'>
      {errors.map((err, i) => (
        // The array has no stable id, and entries never reorder.
        // biome-ignore lint/suspicious/noArrayIndexKey: errors are rendered once per response
        <li key={i} className='rounded-md border border-destructive/30 bg-destructive/5 p-2.5'>
          <div className='flex items-start gap-2'>
            <AlertTriangle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-destructive' />
            <span className='text-xs font-medium text-foreground break-words'>{err.message}</span>
          </div>
          {err.path && err.path.length > 0 && (
            <div className='mt-1 pl-5 font-mono text-2xs text-muted-foreground'>
              {err.path.join('.')}
            </div>
          )}
          {err.locations && err.locations.length > 0 && (
            <div className='pl-5 font-mono text-2xs text-muted-foreground'>
              {err.locations.map((l) => `line ${l.line}, column ${l.column}`).join('; ')}
            </div>
          )}
          {err.extensions && (
            <pre className='mt-1 pl-5 font-mono text-2xs text-muted-foreground whitespace-pre-wrap'>
              {JSON.stringify(err.extensions, null, 2)}
            </pre>
          )}
        </li>
      ))}
    </ul>
  );
}
```

In `src/types/pane-types.ts`, change `ResponseState`:

```ts
  activeView: 'pretty' | 'raw' | 'preview' | 'headers' | 'tests' | 'data' | 'errors';
  /** Set for a response to a GraphQL request, which adds the Data and Errors tabs. */
  protocol?: 'graphql';
```

and add to `RunnerRequestEntry`:

```ts
  /** Present for a GraphQL item; `request` is then only its display and HTTP-shaped form. */
  graphql?: import('@/lib/tauri-api').GraphQlRequest;
```

In `src/components/response/ResponseBodyViewer.tsx`:

1. Import `GraphQlErrorsPanel` and `parseGraphQlResponse`.
2. After `prettyBody` is computed, add:

```tsx
  const graphqlView = useMemo(
    () => (response.protocol === 'graphql' ? parseGraphQlResponse(response.body) : null),
    [response.protocol, response.body],
  );
  const dataBody = useMemo(
    () =>
      graphqlView?.isGraphQl && graphqlView.data !== undefined
        ? JSON.stringify(graphqlView.data, null, 2)
        : '',
    [graphqlView],
  );
```

3. In the tab bar, before the Pretty `TabButton`, add the two tabs (matching the existing `TabButton` usage):

```tsx
          {graphqlView && (
            <>
              <TabButton
                active={activeView === 'data'}
                onClick={() => setActiveView('data')}
                role='tab'
                ariaSelected={activeView === 'data'}
              >
                Data
              </TabButton>
              <TabButton
                active={activeView === 'errors'}
                onClick={() => setActiveView('errors')}
                role='tab'
                ariaSelected={activeView === 'errors'}
              >
                Errors
                <span
                  className={`ml-1 text-2xs ${
                    graphqlView.errors.length > 0 ? 'text-red-500' : 'text-muted-foreground'
                  }`}
                >
                  ({graphqlView.errors.length})
                </span>
              </TabButton>
            </>
          )}
```

4. In the tab content area, add:

```tsx
        {activeView === 'data' &&
          (dataBody ? (
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper value={dataBody} language='json' readOnly height='100%' />
            </Suspense>
          ) : (
            <EmptyBody label='No data in the response' />
          ))}
        {activeView === 'errors' && graphqlView && <GraphQlErrorsPanel errors={graphqlView.errors} />}
```

In `src/lib/execute-request.ts`, `sendRequest`: where `responseState` is built, set the protocol and the default view:

```ts
    const isGraphQl = effectiveRequest.requestType === 'graphql';
    const graphqlErrors = isGraphQl ? parseGraphQlResponse(result.body).errors.length : 0;
```

   and in the `ResponseState` literal add `protocol: isGraphQl ? 'graphql' : undefined,` and change `activeView` to:

```ts
      activeView:
        graphqlErrors > 0
          ? 'errors'
          : result.testResults.length > 0
            ? 'tests'
            : isGraphQl
              ? 'data'
              : 'pretty',
```

   Import `parseGraphQlResponse` from `@/lib/graphql-response`. In the same function, for the console entry replace `requestBody: resolvedBody?.content ?? '',` (success branch only) with:

```ts
      requestBody: isGraphQl
        ? JSON.stringify({
            query: resolvedGraphql?.query,
            operationName: effectiveRequest.graphql?.operationName,
            variables: resolvedGraphql?.variables,
          })
        : (resolvedBody?.content ?? ''),
```

In `src/lib/runner-flatten.ts`, add an arm to `collect` after the `request` arm:

```ts
    } else if (item.type === 'graphql') {
      const requestPath = basePath
        ? `${basePath}/${item.fileName ?? item.name}`
        : (item.fileName ?? item.name);
      // The HTTP-shaped form is only for display and the shared request fields.
      const { body: _body, bodyVariants: _variants, ...asRequest } = item;
      out.push({
        requestPath,
        request: asRequest,
        graphql: item,
        included: true,
        status: 'pending',
      });
    }
```

   (the `else if` chain currently ends with the `request` branch; `asRequest` needs `type` removed, so destructure `type: _type` as well: `const { type: _type, body: _body, bodyVariants: _variants, ...asRequest } = item;`).

In `src/lib/runner-execute.ts`:

1. Signature: `executeRunnerEntry(collection, requestPath, request, environmentName, graphql?: GraphQlRequest)`; import `type GraphQlRequest` and `executeGraphQlRequest` from tauri-api, and `mapGraphQlToState` from pane-utils.
2. Build the state from the right mapper: `const requestState = graphql ? mapGraphQlToState(graphql) : mapApiRequestToState(request, true);`.
3. After `const input: ExecuteRequestInput = {...}` is built, replace `const result = await executeRequest(input);` with:

```ts
    const result = graphql
      ? await executeGraphQlRequest({
          request: { ...input, body: undefined },
          query: resolved.graphql?.query ?? graphql.body.query,
          variables: resolved.graphql?.variables ?? (graphql.body.variables || undefined),
          operationName: undefined,
        })
      : await executeRequest(input);
```

4. Make the failure rule include GraphQL errors:

```ts
    const hasGraphQlErrors = graphql ? parseGraphQlResponse(result.body).errors.length > 0 : false;
    const failed = isErrorStatus || hasFailingTest || Boolean(result.scriptError) || hasGraphQlErrors;
```

   Import `parseGraphQlResponse` from `@/lib/graphql-response`.

In `src/stores/pane-store.ts`, pass the payload at the `executeRunnerEntry` call (around line 712): add `entry.graphql,` as the fifth argument.

In `src/components/request/runner/RunnerRequestList.tsx` and `RunnerResultsList.tsx`, show `GQL` for a GraphQL entry: replace `{entry.request.method}` with `{entry.graphql ? 'GQL' : entry.request.method}` and the colour lookup `METHOD_TEXT_COLOR[entry.request.method]` with `METHOD_TEXT_COLOR[entry.graphql ? 'POST' : entry.request.method]`.

- [ ] **Step 11: Run the checks**

Run:
- `yarn test graphql-response ResponseBodyViewer runner-flatten runner-execute pane-store execute-request`
- `yarn tsc --noEmit`
- `yarn check`
- `cargo check -j4 -p rocket-app -p rocket`
- `cargo test -j4 -p rocket-app collection_runner_service`

Expected: PASS. `yarn tsc --noEmit` is the net for `RunnerRequestEntry` consumers; every place that builds an entry must still type-check because `graphql` is optional.

- [ ] **Step 12: Manual check in the real app**

Run `yarn tauri dev`. Send a GraphQL request that returns an `errors` array (a field that does not exist). Confirm the Errors tab opens by default with the count, the Data tab shows only `data`, and the Console panel shows the JSON payload. Run the Collection Runner over a collection holding a GraphQL item and an HTTP item; the errored GraphQL step shows as failed and the HTTP step is unaffected. Check History: the GraphQL send is listed as a `POST` of the endpoint URL.

- [ ] **Step 13: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-app/src/graphql_request.rs crates/rocket-app/src/runner_sequence.rs \
  crates/rocket-app/src/collection_runner_service.rs crates/rocket-app/src/flow_execution_service.rs \
  crates/rocket-app/src/test_doubles.rs \
  src/lib/graphql-response.ts src/lib/execute-request.ts src/lib/runner-flatten.ts \
  src/lib/runner-execute.ts src/lib/__tests__ src/types/pane-types.ts src/stores/pane-store.ts \
  src/stores/__tests__/pane-store.test.ts \
  src/components/response src/components/request/runner
```

Suggested subject: `feat(graphql): surface errors and run GraphQL items in the runner`.

---

## Next Plan

[Plan 07: GraphQL schema, docs explorer and query builder](2026-10-05-protocol-parity-plan-07-graphql-schema-and-builder.md). It depends on this plan (`execute_graphql`, `GraphQlEditor`, `dispatchSend`). Chain to it automatically when this one finishes.
