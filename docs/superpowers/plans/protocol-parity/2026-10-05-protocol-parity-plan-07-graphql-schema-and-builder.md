# Protocol parity, Plan 07: GraphQL schema, docs explorer, autocomplete and query builder

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GraphQL request tab can fetch its endpoint's schema by introspection, browse it in a docs explorer, get schema-aware autocomplete and validation in the query editor, and assemble a query from a field tree.

**Architecture:** The schema is fetched in `rocket-app` through the same HTTP execution path as a send, so auth, headers, variables and environments apply. The introspection request carries none of the tab's scripts, assertions or actions and never writes History. The result is cached in memory per `(collection, environment, resolved URL)` and handed to the frontend as raw introspection JSON. The frontend builds a `GraphQLSchema` with `buildClientSchema` from the `graphql` package, and `graphql-language-service` supplies completion and diagnostics to a Monaco provider that is registered per editor. The docs explorer and the query builder are plain React components over that `GraphQLSchema`.

**Tech Stack:** Rust (serde_json, wiremock), Tauri 2 IPC, React + TypeScript, Monaco, `graphql` 16 and `graphql-language-service` 5 (added with Yarn in Task 2), Zustand, Vitest. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md` (GraphQL request shape only; schema handling is not part of the OpenCollection format, and nothing from this plan is written to a collection file). Introspection is the standard `__schema` query.

## Global Constraints

- Never `unwrap()` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill, staging by explicit path.
- Frontend: shadcn/ui primitives and `lucide-react` only; no raw `<button>`, `<input>`, `<select>`, `<dialog>`, `<form>`. Multi-line editors are Monaco only. Zustand: narrow selectors only, never destructure the whole store.
- Monaco must stay out of the initial bundle: import `monaco-editor` only from modules that are themselves loaded through `import()` or `lazy()`.
- Use Yarn. This repo has `yarn.lock`.
- The schema is never persisted to a collection file in this plan. The in-memory cache is lost on restart by design.
- This plan assumes Plans 05 and 06 are merged (`GraphQlEditor`, `dispatchSend`, `RequestState.graphql`, `execute_graphql`, `apply_graphql_payload`).

## Review Focus

1. The introspection request must not run the tab's pre-request script, tests, assertions or actions, and must not add a History entry (Task 1 tests `introspection_input_*` and `fetch_graphql_schema_does_not_write_history`).
2. A server that rejects the newer introspection fields (`isRepeatable`, `specifiedByURL`, deprecated arguments) must still work through the compatibility query, and a server with introspection disabled must produce a clear error rather than an empty schema (Task 1 tests `fetch_graphql_schema_falls_back_to_the_compat_query`, `fetch_graphql_schema_reports_disabled_introspection`).
3. The cache key must separate environments and URLs, and a cache hit must not touch the network (Task 1 tests `cache_key_separates_*`, `fetch_graphql_schema_serves_a_cache_hit_without_a_request`).
4. The editor must offer completion and diagnostics only for its own model, must dispose its provider and markers on unmount, and must not pull Monaco into the main bundle (Task 2 tests `attach` mapping tests, and the manual bundle check).
5. A query builder over a schema with required arguments must emit valid variable definitions without name collisions, and must never emit a selection set that is empty (Task 3 tests `buildOperation_*`).

---

## Task 1: Introspection fetch and schema cache in `rocket-app`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/graphql_schema.rs`
- Modify: `crates/rocket-app/src/graphql_request.rs` (move two test helpers into a shared `test_support` module)
- Modify: `crates/rocket-app/src/lib.rs`
- Create: `src-tauri/src/commands/graphql_schema.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`
- Modify: `src/lib/tauri-api.ts`

**Interfaces:**
- Consumes: `apply_graphql_payload`, `ExecuteRequestInput`, `RequestExecutionService::execute` (Plan 06 and earlier).
- Produces:
  - `rocket_app::graphql_schema::{INTROSPECTION_QUERY, INTROSPECTION_QUERY_COMPAT, GraphQlSchemaCache, GraphQlSchemaDto, FetchGraphQlSchemaInput, introspection_input, extract_introspection}`.
    - `GraphQlSchemaCache::key(collection: Option<&str>, environment: Option<&str>, url: &str) -> String`, `get(&self, key: &str) -> Option<GraphQlSchemaDto>`, `put(&self, dto: GraphQlSchemaDto)`, `clear(&self, key: &str)`.
    - `GraphQlSchemaDto { key: String, fetched_at: String, introspection: serde_json::Value }` (camelCase on the wire). `introspection` is `{"__schema": {...}}`, the exact input `buildClientSchema` takes.
    - `FetchGraphQlSchemaInput { request: ExecuteRequestInput, refresh: bool }`.
    - `RequestExecutionService::fetch_graphql_schema(&self, cache: &GraphQlSchemaCache, input: FetchGraphQlSchemaInput) -> DomainResult<GraphQlSchemaDto>`.
  - Tauri commands `fetch_graphql_schema(input) -> GraphQlSchemaDto`, `get_cached_graphql_schema(collection, environmentName, url) -> Option<GraphQlSchemaDto>`, `clear_graphql_schema(collection, environmentName, url)`.
  - TS: `GraphQlSchemaResult`, `FetchGraphQlSchemaInput`, `fetchGraphQlSchema`, `getCachedGraphQlSchema`, `clearGraphQlSchema`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Share the Plan 06 test helpers**

`input(url)` and `service(env)` in the `tests` module of `crates/rocket-app/src/graphql_request.rs` are needed by the new schema tests too. Move them into a `cfg(test)` module. In `graphql_request.rs`, add this just above `#[cfg(test)] mod tests`:

```rust
/// Helpers shared by the GraphQL tests in this crate.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
        SharedCollectionRepo, SharedHistoryRepo, StaticEnvRepo,
    };
    use rocket_collection::Collection;
    use rocket_environment::environment::Environment;
    use rocket_http::RequestOptions;
    use rocket_shared::types::Auth;
    use std::sync::Arc;

    pub(crate) fn input(url: &str) -> ExecuteRequestInput {
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

    pub(crate) fn service(env: Environment) -> (RequestExecutionService, Arc<InMemoryHistoryRepo>) {
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
}
```

In the `tests` module of the same file, delete the two functions `fn input(...)` and `fn service(...)`, change the `use` block at the top of the module to:

```rust
    use super::test_support::{input, service};
    use super::*;
    use rocket_environment::environment::Environment;
    use rocket_environment::variable::Variable;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
```

and keep `use std::sync::Arc;` only if a remaining test still names `Arc` (the Plan 06 tests do not).

Run: `cargo test -j4 -p rocket-app graphql_request`
Expected: PASS, with the same tests as before. This step changes no behaviour.

- [ ] **Step 3: Write the failing schema tests**

Add `pub mod graphql_schema;` to `crates/rocket-app/src/lib.rs` next to `pub mod graphql_request;`. Create `crates/rocket-app/src/graphql_schema.rs` containing only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql_request::test_support::{input, service};
    use rocket_environment::environment::Environment;
    use wiremock::matchers::{body_string_contains, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn schema_body() -> serde_json::Value {
        serde_json::json!({
            "data": { "__schema": {
                "queryType": { "name": "Query" },
                "mutationType": null,
                "subscriptionType": null,
                "types": [],
                "directives": []
            } }
        })
    }

    #[test]
    fn the_full_query_asks_for_the_newer_fields_and_the_compat_query_does_not() {
        assert!(INTROSPECTION_QUERY.contains("isRepeatable"));
        assert!(INTROSPECTION_QUERY.contains("specifiedByURL"));
        assert!(!INTROSPECTION_QUERY_COMPAT.contains("isRepeatable"));
        assert!(!INTROSPECTION_QUERY_COMPAT.contains("specifiedByURL"));
        assert!(INTROSPECTION_QUERY.contains("query IntrospectionQuery"));
        assert!(INTROSPECTION_QUERY_COMPAT.contains("query IntrospectionQuery"));
    }

    #[test]
    fn both_queries_define_exactly_one_operation() {
        use crate::graphql_document::list_operations;
        assert_eq!(list_operations(INTROSPECTION_QUERY).len(), 1);
        assert_eq!(list_operations(INTROSPECTION_QUERY_COMPAT).len(), 1);
    }

    #[test]
    fn cache_key_separates_environments_and_urls() {
        let a = GraphQlSchemaCache::key(Some("api"), Some("dev"), "https://x/graphql");
        let b = GraphQlSchemaCache::key(Some("api"), Some("prod"), "https://x/graphql");
        let c = GraphQlSchemaCache::key(Some("api"), Some("dev"), "https://y/graphql");
        let d = GraphQlSchemaCache::key(None, None, "https://x/graphql");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_eq!(a, GraphQlSchemaCache::key(Some("api"), Some("dev"), "https://x/graphql"));
    }

    #[test]
    fn introspection_input_drops_scripts_assertions_actions_and_history() {
        let mut base = input("https://x/graphql");
        base.pre_request_script = Some("// pre".into());
        base.post_response_script = Some("// post".into());
        base.tests_script = Some("// tests".into());
        base.tags = vec!["smoke".into()];
        base.assertions = vec![rocket_shared::Assertion::new(
            "res.status",
            "eq",
            Some("200".to_string()),
        )];
        base.request_name = Some("Users".into());

        let out = introspection_input(&base);
        assert!(out.pre_request_script.is_none());
        assert!(out.post_response_script.is_none());
        assert!(out.tests_script.is_none());
        assert!(out.assertions.is_empty());
        assert!(out.actions.is_empty());
        assert!(out.tags.is_empty());
        assert!(out.skip_history);
        assert_eq!(out.request_name.as_deref(), Some("GraphQL introspection"));
        assert_eq!(out.url, base.url, "the endpoint, auth and headers are kept");
    }

    #[test]
    fn extract_introspection_wraps_the_schema_member() {
        let body = schema_body().to_string();
        let got = extract_introspection(&body).expect("a schema");
        assert_eq!(got["__schema"]["queryType"]["name"], "Query");
        assert!(extract_introspection("{\"data\":null}").is_none());
        assert!(extract_introspection("not json").is_none());
        assert!(extract_introspection("{\"data\":{\"__schema\":5}}").is_none());
    }

    #[tokio::test]
    async fn fetch_graphql_schema_returns_and_caches_the_schema() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();
        let url = format!("{}/graphql", server.uri());

        let dto = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput { request: input(&url), refresh: false },
            )
            .await
            .expect("fetch");

        assert_eq!(dto.introspection["__schema"]["queryType"]["name"], "Query");
        assert!(!dto.fetched_at.is_empty());
        assert!(cache.get(&dto.key).is_some());
        let seen = server.received_requests().await.expect("recording");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json");
        assert_eq!(body["operationName"], "IntrospectionQuery");
    }

    #[tokio::test]
    async fn fetch_graphql_schema_serves_a_cache_hit_without_a_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();
        let url = format!("{}/graphql", server.uri());
        let call = |refresh: bool| FetchGraphQlSchemaInput { request: input(&url), refresh };

        svc.fetch_graphql_schema(&cache, call(false)).await.expect("first");
        svc.fetch_graphql_schema(&cache, call(false)).await.expect("second");
        assert_eq!(server.received_requests().await.expect("recording").len(), 1);

        svc.fetch_graphql_schema(&cache, call(true)).await.expect("refresh");
        assert_eq!(server.received_requests().await.expect("recording").len(), 2);
    }

    #[tokio::test]
    async fn fetch_graphql_schema_falls_back_to_the_compat_query() {
        let server = MockServer::start().await;
        // A server that predates `isRepeatable` rejects the full query.
        Mock::given(method("POST"))
            .and(body_string_contains("isRepeatable"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "message": "Cannot query field \"isRepeatable\" on type \"__Directive\"." }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(body_string_contains("IntrospectionQuery"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        let dto = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&format!("{}/graphql", server.uri())),
                    refresh: false,
                },
            )
            .await
            .expect("fetch");
        assert_eq!(dto.introspection["__schema"]["queryType"]["name"], "Query");
        assert_eq!(server.received_requests().await.expect("recording").len(), 2);
    }

    #[tokio::test]
    async fn fetch_graphql_schema_reports_disabled_introspection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "message": "GraphQL introspection is not allowed by Apollo Server" }]
            })))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        let err = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&format!("{}/graphql", server.uri())),
                    refresh: false,
                },
            )
            .await
            .expect_err("disabled");
        assert!(err.to_string().contains("introspection is disabled"), "got: {err}");
        assert!(cache.get(&GraphQlSchemaCache::key(None, None, &format!("{}/graphql", server.uri()))).is_none());
    }

    #[tokio::test]
    async fn fetch_graphql_schema_does_not_retry_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        let err = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&format!("{}/graphql", server.uri())),
                    refresh: false,
                },
            )
            .await
            .expect_err("401");
        assert!(err.to_string().contains("401"), "got: {err}");
        assert_eq!(server.received_requests().await.expect("recording").len(), 1);
    }

    #[tokio::test]
    async fn fetch_graphql_schema_does_not_write_history() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        svc.fetch_graphql_schema(
            &cache,
            FetchGraphQlSchemaInput {
                request: input(&format!("{}/graphql", server.uri())),
                refresh: false,
            },
        )
        .await
        .expect("fetch");
        assert_eq!(history.saved_count(), 0);
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app graphql_schema`
Expected: FAIL to compile (`INTROSPECTION_QUERY`, `GraphQlSchemaCache`, `introspection_input` not found).

- [ ] **Step 5: Implement the schema module**

Put this above the test module in `crates/rocket-app/src/graphql_schema.rs`:

```rust
//! Fetches a GraphQL schema by introspection and caches it per endpoint.
//!
//! The introspection request goes through the normal HTTP execution path, so
//! auth, headers, variables and the selected environment apply. It carries none
//! of the tab's scripts, assertions or actions, and it never writes History.

use std::collections::HashMap;
use std::sync::Mutex;

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::graphql_request::apply_graphql_payload;

/// The standard introspection query, as `graphql-js` 16 builds it with every option on.
pub const INTROSPECTION_QUERY: &str = r#"query IntrospectionQuery {
  __schema {
    description
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
    directives {
      name
      description
      isRepeatable
      locations
      args(includeDeprecated: true) { ...InputValue }
    }
  }
}

fragment FullType on __Type {
  kind
  name
  description
  specifiedByURL
  fields(includeDeprecated: true) {
    name
    description
    args(includeDeprecated: true) { ...InputValue }
    type { ...TypeRef }
    isDeprecated
    deprecationReason
  }
  inputFields(includeDeprecated: true) { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) {
    name
    description
    isDeprecated
    deprecationReason
  }
  possibleTypes { ...TypeRef }
}

fragment InputValue on __InputValue {
  name
  description
  type { ...TypeRef }
  defaultValue
  isDeprecated
  deprecationReason
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType { kind name }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// The same query without the fields older servers reject: `isRepeatable`,
/// `specifiedByURL`, schema description and deprecated arguments.
pub const INTROSPECTION_QUERY_COMPAT: &str = r#"query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
    directives {
      name
      description
      locations
      args { ...InputValue }
    }
  }
}

fragment FullType on __Type {
  kind
  name
  description
  fields(includeDeprecated: true) {
    name
    description
    args { ...InputValue }
    type { ...TypeRef }
    isDeprecated
    deprecationReason
  }
  inputFields { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) {
    name
    description
    isDeprecated
    deprecationReason
  }
  possibleTypes { ...TypeRef }
}

fragment InputValue on __InputValue {
  name
  description
  type { ...TypeRef }
  defaultValue
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType { kind name }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// A fetched schema as the frontend receives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlSchemaDto {
    pub key: String,
    /// RFC 3339 time of the fetch.
    pub fetched_at: String,
    /// `{"__schema": {...}}`, which is what `buildClientSchema` takes.
    pub introspection: serde_json::Value,
}

/// A schema fetch: the HTTP side of the tab and whether to bypass the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchGraphQlSchemaInput {
    pub request: ExecuteRequestInput,
    #[serde(default)]
    pub refresh: bool,
}

/// In-memory schema cache. Lost on restart by design.
#[derive(Default)]
pub struct GraphQlSchemaCache {
    entries: Mutex<HashMap<String, GraphQlSchemaDto>>,
}

impl GraphQlSchemaCache {
    /// One entry per collection, environment and resolved endpoint URL, so a
    /// schema from a staging server is never shown for production.
    pub fn key(collection: Option<&str>, environment: Option<&str>, url: &str) -> String {
        format!(
            "{}|{}|{}",
            collection.unwrap_or(""),
            environment.unwrap_or(""),
            url
        )
    }

    pub fn get(&self, key: &str) -> Option<GraphQlSchemaDto> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    pub fn put(&self, dto: GraphQlSchemaDto) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(dto.key.clone(), dto);
    }

    pub fn clear(&self, key: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
}

/// The HTTP input for an introspection request: the tab's endpoint, headers,
/// auth and options, and nothing that belongs to a normal send.
pub fn introspection_input(base: &ExecuteRequestInput) -> ExecuteRequestInput {
    let mut input = base.clone();
    input.pre_request_script = None;
    input.post_response_script = None;
    input.tests_script = None;
    input.assertions = Vec::new();
    input.actions = Vec::new();
    input.tags = Vec::new();
    input.body = None;
    input.skip_history = true;
    input.request_name = Some("GraphQL introspection".to_string());
    input
}

/// Pulls `{"__schema": ...}` out of a response body, if it holds a schema.
pub fn extract_introspection(body: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    if !schema.is_object() {
        return None;
    }
    Some(serde_json::json!({ "__schema": schema }))
}

/// The first `errors[].message` of a GraphQL error body, with the number of errors.
fn first_error(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let errors = value.get("errors")?.as_array()?;
    let first = errors.first()?;
    Some(
        first
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error")
            .to_string(),
    )
}

fn introspection_error(status: u16, body: &str) -> DomainError {
    match first_error(body) {
        Some(message) if message.to_lowercase().contains("introspection") => {
            DomainError::InvalidInput(format!("introspection is disabled on this server ({message})"))
        }
        Some(message) => DomainError::InvalidInput(format!(
            "the server rejected the introspection query: {message}"
        )),
        None => DomainError::Http(format!(
            "introspection failed: HTTP {status} returned no schema"
        )),
    }
}

impl RequestExecutionService {
    /// Fetches the endpoint's schema, or returns the cached one.
    ///
    /// The full query is tried first. A response that carries a GraphQL `errors`
    /// array without a schema triggers one retry with the compatibility query,
    /// because older servers reject the newer fields. Any other failure, such
    /// as a 401, is reported at once.
    pub async fn fetch_graphql_schema(
        &self,
        cache: &GraphQlSchemaCache,
        input: FetchGraphQlSchemaInput,
    ) -> DomainResult<GraphQlSchemaDto> {
        let key = GraphQlSchemaCache::key(
            input.request.collection.as_deref(),
            input.request.environment_name.as_deref(),
            &input.request.url,
        );
        if !input.refresh {
            if let Some(hit) = cache.get(&key) {
                return Ok(hit);
            }
        }

        let mut last: Option<(u16, String)> = None;
        for query in [INTROSPECTION_QUERY, INTROSPECTION_QUERY_COMPAT] {
            let mut request = introspection_input(&input.request);
            apply_graphql_payload(&mut request, query, None, None)?;
            let out = self.execute(request).await?;
            if let Some(introspection) = extract_introspection(&out.response.body) {
                let dto = GraphQlSchemaDto {
                    key: key.clone(),
                    fetched_at: chrono::Utc::now().to_rfc3339(),
                    introspection,
                };
                cache.put(dto.clone());
                return Ok(dto);
            }
            let retry = first_error(&out.response.body).is_some();
            last = Some((out.response.status, out.response.body));
            if !retry {
                break;
            }
        }
        let (status, body) = last.unwrap_or((0, String::new()));
        Err(introspection_error(status, &body))
    }
}
```

- [ ] **Step 6: Run the schema tests to verify they pass**

Run: `cargo test -j4 -p rocket-app graphql_schema`
Expected: PASS (10 tests). If `fetch_graphql_schema_falls_back_to_the_compat_query` sees only one request, check that the first mock matches `isRepeatable` in the body: `apply_graphql_payload` embeds the query as a JSON string, so the substring is present.

- [ ] **Step 7: Add the Tauri commands and the TS wrappers**

Create `src-tauri/src/commands/graphql_schema.rs`:

```rust
use rocket_app::graphql_schema::{FetchGraphQlSchemaInput, GraphQlSchemaCache, GraphQlSchemaDto};
use rocket_app::RequestExecutionService;
use rocket_shared::error::DomainError;
use tauri::State;

/// Fetches the endpoint's schema by introspection, or returns the cached one.
#[tauri::command]
pub async fn fetch_graphql_schema(
    input: FetchGraphQlSchemaInput,
    svc: State<'_, RequestExecutionService>,
    cache: State<'_, GraphQlSchemaCache>,
) -> Result<GraphQlSchemaDto, DomainError> {
    svc.fetch_graphql_schema(&cache, input).await
}

/// Returns the cached schema for an endpoint without touching the network.
#[tauri::command]
pub fn get_cached_graphql_schema(
    collection: Option<String>,
    environment_name: Option<String>,
    url: String,
    cache: State<'_, GraphQlSchemaCache>,
) -> Option<GraphQlSchemaDto> {
    cache.get(&GraphQlSchemaCache::key(
        collection.as_deref(),
        environment_name.as_deref(),
        &url,
    ))
}

/// Forgets the cached schema for an endpoint.
#[tauri::command]
pub fn clear_graphql_schema(
    collection: Option<String>,
    environment_name: Option<String>,
    url: String,
    cache: State<'_, GraphQlSchemaCache>,
) {
    cache.clear(&GraphQlSchemaCache::key(
        collection.as_deref(),
        environment_name.as_deref(),
        &url,
    ));
}
```

In `src-tauri/src/commands/mod.rs`, add `pub mod graphql_schema;` after `pub mod git;`. In `src-tauri/src/lib.rs`, add `app.manage(rocket_app::graphql_schema::GraphQlSchemaCache::default());` after `app.manage(exec_svc);`, and add to the handler list after `commands::execution::list_graphql_operations,`:

```rust
            commands::graphql_schema::fetch_graphql_schema,
            commands::graphql_schema::get_cached_graphql_schema,
            commands::graphql_schema::clear_graphql_schema,
```

In `src/lib/tauri-api.ts`, add after `listGraphQlOperations`:

```ts
/** A schema as `buildClientSchema` takes it: `{ __schema: ... }`. */
export interface GraphQlSchemaResult {
  key: string;
  /** RFC 3339 time of the fetch. */
  fetchedAt: string;
  introspection: unknown;
}

export interface FetchGraphQlSchemaInput {
  /** The tab's endpoint, headers, auth and options. Its scripts and body are ignored. */
  request: ExecuteRequestInput;
  /** Bypass the cache and fetch again. */
  refresh?: boolean;
}

export const fetchGraphQlSchema = (input: FetchGraphQlSchemaInput) =>
  invoke<GraphQlSchemaResult>('fetch_graphql_schema', { input });

export const getCachedGraphQlSchema = (
  collection: string | undefined,
  environmentName: string | undefined,
  url: string,
) =>
  invoke<GraphQlSchemaResult | null>('get_cached_graphql_schema', {
    collection: collection ?? null,
    environmentName: environmentName ?? null,
    url,
  });

export const clearGraphQlSchema = (
  collection: string | undefined,
  environmentName: string | undefined,
  url: string,
) =>
  invoke<void>('clear_graphql_schema', {
    collection: collection ?? null,
    environmentName: environmentName ?? null,
    url,
  });
```

- [ ] **Step 8: Run the checks**

Run:
- `cargo check -j4 -p rocket-app -p rocket`
- `cargo test -j4 -p rocket-app graphql`
- `yarn tsc --noEmit`

Expected: PASS.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-app/src/graphql_schema.rs crates/rocket-app/src/graphql_request.rs \
  crates/rocket-app/src/lib.rs src-tauri/src/commands/graphql_schema.rs \
  src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/tauri-api.ts
```

Suggested subject: `feat(graphql): fetch and cache a schema by introspection`.

---

## Task 2: Docs explorer and schema-aware editor

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `package.json`, `yarn.lock`
- Create: `src/lib/graphql-schema.ts`, `src/lib/graphql-docs.ts`, `src/lib/graphql-language-mapping.ts`, `src/lib/graphql-schema-input.ts`
- Create: `src/components/editor/graphql-language.ts`
- Create: `src/stores/graphql-schema-store.ts`
- Create: `src/components/request/GraphQlDocsExplorer.tsx`
- Modify: `src/components/request/GraphQlEditor.tsx`, `src/components/request/RequestPanel.tsx`, `src/lib/request-profile.ts`
- Create tests: `src/lib/__tests__/graphql-schema.test.ts`, `graphql-docs.test.ts`, `graphql-language-mapping.test.ts`, `src/stores/__tests__/graphql-schema-store.test.ts`, `src/components/request/__tests__/GraphQlDocsExplorer.test.tsx`
- Modify tests: `src/components/request/__tests__/GraphQlEditor.test.tsx`, `src/lib/__tests__/request-profile.test.ts`

**Interfaces:**
- Consumes: `fetchGraphQlSchema`, `getCachedGraphQlSchema` (Task 1), `GraphQlEditor` (Plan 06), `resolveRequestFields`, `getActiveGlobalEnvName`, `getActiveWorkspaceRequestGuardPolicy` (`src/lib/execute-request.ts`).
- Produces:
  - `buildSchemaFromIntrospection(introspection: unknown): GraphQLSchema`.
  - `rootTypes(schema): DocsRoot[]`, `listTypeNames(schema): string[]`, `describeType(schema, name): DocsType | null` in `graphql-docs.ts`.
  - `mapCompletionKind(kind, kinds)`, `completionDocumentation(doc)`, `diagnosticToMarker(d, severities)` in `graphql-language-mapping.ts`.
  - `attachGraphQlSupport(editor, getSchema): { revalidate(): void; dispose(): void }` in `src/components/editor/graphql-language.ts` (Monaco-bound; load it only through `import()`).
  - `buildSchemaRequestInput(tabId, request): Promise<ExecuteRequestInput>`.
  - `useGraphQlSchemaStore` with `entries: Record<string, SchemaEntry>`, `loadCached(tabId, request)`, `fetchSchema(tabId, request, refresh)`, `clear(tabId)`.
  - `<GraphQlDocsExplorer schema status error fetchedAt onFetch />`.
  - `requestProfile(kind).showSchema: boolean`.
  - `GraphQlEditor` gains optional prop `schema?: GraphQLSchema`.

Why `graphql` and `graphql-language-service` rather than `monaco-graphql`: `monaco-graphql` needs its own web worker wired through Vite and Monaco's loader, which this app configures for the JS worker only. `graphql-language-service` is the engine `monaco-graphql` uses, it runs on the main thread, and it needs only a `GraphQLSchema`. The `graphql` package is also needed for `buildClientSchema`, so the second dependency is the only extra one.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Add the packages and confirm their exports**

Run: `yarn add graphql@16 graphql-language-service@5`
Expected: both appear in `package.json` `dependencies`; `yarn.lock` changes.

Run: `yarn why graphql`
Expected: exactly one resolved version of `graphql` (16.x). If two versions appear, the peer range is not satisfied: stop and resolve it before going on.

Run: `node -e "const m=require('graphql-language-service'); console.log(typeof m.getAutocompleteSuggestions, typeof m.getDiagnostics, typeof m.Position)"`
Expected: `function function function`. If a name is missing, read `node_modules/graphql-language-service/package.json` and the package's `esm/index.d.ts` for the replacement name, and use that name everywhere this task says `getAutocompleteSuggestions`, `getDiagnostics` or `Position`.

- [ ] **Step 3: Write the failing library tests**

Create `src/lib/__tests__/graphql-schema.test.ts`:

```ts
import { buildSchema, introspectionFromSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { buildSchemaFromIntrospection } from '../graphql-schema';

describe('buildSchemaFromIntrospection', () => {
  it('rebuilds a schema from an introspection result', () => {
    const source = buildSchema('type Query { user(id: ID!): User } type User { id: ID! name: String }');
    const schema = buildSchemaFromIntrospection(introspectionFromSchema(source));
    expect(schema.getQueryType()?.name).toBe('Query');
    expect(schema.getType('User')).toBeDefined();
  });

  it('throws a readable error for something that is not an introspection result', () => {
    expect(() => buildSchemaFromIntrospection({ nope: true })).toThrow();
  });
});
```

Create `src/lib/__tests__/graphql-docs.test.ts`:

```ts
import { buildSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { describeType, listTypeNames, rootTypes } from '../graphql-docs';

const schema = buildSchema(`
  """The entry point."""
  type Query {
    user(id: ID!, active: Boolean = true): User
    search(term: String!): [SearchResult!]!
    old: String @deprecated(reason: "use user")
  }
  type Mutation { rename(id: ID!, name: String!): User }
  interface Node { id: ID! }
  type User implements Node { id: ID! name: String role: Role }
  type Post implements Node { id: ID! title: String }
  union SearchResult = User | Post
  enum Role { ADMIN @deprecated(reason: "gone") MEMBER }
  input Filter { term: String! limit: Int = 10 }
  scalar DateTime
`);

describe('rootTypes', () => {
  it('lists the root operation types that exist', () => {
    expect(rootTypes(schema)).toEqual([
      { operation: 'query', typeName: 'Query' },
      { operation: 'mutation', typeName: 'Mutation' },
    ]);
  });
});

describe('listTypeNames', () => {
  it('lists named types sorted, without introspection types', () => {
    const names = listTypeNames(schema);
    expect(names).toContain('User');
    expect(names).toContain('DateTime');
    expect(names.some((n) => n.startsWith('__'))).toBe(false);
    expect([...names].sort()).toEqual(names);
  });
});

describe('describeType', () => {
  it('describes an object type with arguments, defaults and deprecation', () => {
    const q = describeType(schema, 'Query');
    expect(q?.kind).toBe('object');
    expect(q?.description).toBe('The entry point.');
    const user = q?.fields.find((f) => f.name === 'user');
    expect(user?.type).toBe('User');
    expect(user?.namedType).toBe('User');
    expect(user?.args).toEqual([
      { name: 'id', type: 'ID!', description: undefined, defaultValue: undefined },
      { name: 'active', type: 'Boolean', description: undefined, defaultValue: 'true' },
    ]);
    expect(q?.fields.find((f) => f.name === 'search')?.type).toBe('[SearchResult!]!');
    expect(q?.fields.find((f) => f.name === 'old')?.deprecation).toBe('use user');
  });

  it('describes interfaces, unions, enums, inputs and scalars', () => {
    expect(describeType(schema, 'Node')?.possibleTypes.sort()).toEqual(['Post', 'User']);
    expect(describeType(schema, 'User')?.interfaces).toEqual(['Node']);
    expect(describeType(schema, 'SearchResult')?.possibleTypes.sort()).toEqual(['Post', 'User']);
    const role = describeType(schema, 'Role');
    expect(role?.enumValues.map((v) => v.name)).toEqual(['ADMIN', 'MEMBER']);
    expect(role?.enumValues[0].deprecation).toBe('gone');
    const filter = describeType(schema, 'Filter');
    expect(filter?.kind).toBe('input');
    expect(filter?.fields.map((f) => f.name)).toEqual(['term', 'limit']);
    expect(describeType(schema, 'DateTime')?.kind).toBe('scalar');
  });

  it('returns null for an unknown type', () => {
    expect(describeType(schema, 'Nope')).toBeNull();
  });
});
```

Create `src/lib/__tests__/graphql-language-mapping.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  completionDocumentation,
  diagnosticToMarker,
  mapCompletionKind,
} from '../graphql-language-mapping';

const kinds = {
  Field: 3,
  Variable: 4,
  Class: 5,
  Interface: 7,
  Property: 9,
  Value: 13,
  Enum: 15,
  EnumMember: 16,
  Keyword: 17,
  Text: 18,
  Constant: 14,
  Struct: 6,
};

describe('mapCompletionKind', () => {
  it('maps LSP kinds to Monaco kinds', () => {
    expect(mapCompletionKind(5, kinds)).toBe(kinds.Field);
    expect(mapCompletionKind(6, kinds)).toBe(kinds.Variable);
    expect(mapCompletionKind(7, kinds)).toBe(kinds.Class);
    expect(mapCompletionKind(13, kinds)).toBe(kinds.Enum);
    expect(mapCompletionKind(14, kinds)).toBe(kinds.Keyword);
    expect(mapCompletionKind(20, kinds)).toBe(kinds.EnumMember);
  });

  it('falls back to Text for an unknown or missing kind', () => {
    expect(mapCompletionKind(undefined, kinds)).toBe(kinds.Text);
    expect(mapCompletionKind(999, kinds)).toBe(kinds.Text);
  });
});

describe('completionDocumentation', () => {
  it('accepts a string, a markup object or nothing', () => {
    expect(completionDocumentation('plain')).toBe('plain');
    expect(completionDocumentation({ kind: 'markdown', value: 'md' })).toBe('md');
    expect(completionDocumentation(undefined)).toBeUndefined();
    expect(completionDocumentation(null)).toBeUndefined();
  });
});

describe('diagnosticToMarker', () => {
  const severities = { Hint: 1, Info: 2, Warning: 4, Error: 8 };

  it('moves zero-based LSP positions to one-based Monaco positions', () => {
    const marker = diagnosticToMarker(
      {
        message: 'Unknown field',
        severity: 1,
        range: { start: { line: 0, character: 2 }, end: { line: 0, character: 6 } },
      },
      severities,
    );
    expect(marker).toEqual({
      message: 'Unknown field',
      severity: 8,
      startLineNumber: 1,
      startColumn: 3,
      endLineNumber: 1,
      endColumn: 7,
    });
  });

  it('maps warning, info and hint severities and defaults to error', () => {
    const base = {
      message: 'm',
      range: { start: { line: 1, character: 0 }, end: { line: 1, character: 1 } },
    };
    expect(diagnosticToMarker({ ...base, severity: 2 }, severities).severity).toBe(4);
    expect(diagnosticToMarker({ ...base, severity: 3 }, severities).severity).toBe(2);
    expect(diagnosticToMarker({ ...base, severity: 4 }, severities).severity).toBe(1);
    expect(diagnosticToMarker(base, severities).severity).toBe(8);
  });
});
```

Create `src/stores/__tests__/graphql-schema-store.test.ts`:

```ts
import { buildSchema, introspectionFromSchema } from 'graphql';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/graphql-schema-input', () => ({
  buildSchemaRequestInput: vi.fn(async () => ({
    method: 'POST',
    url: 'https://x/graphql',
    headers: [],
    queryParams: [],
    auth: { authType: 'none' },
    options: { followRedirects: true, timeoutMs: 1000, verifySsl: true },
    collection: 'api',
    environmentName: 'dev',
  })),
}));

vi.mock('@/lib/tauri-api', () => ({
  fetchGraphQlSchema: vi.fn(),
  getCachedGraphQlSchema: vi.fn(),
}));

import { fetchGraphQlSchema, getCachedGraphQlSchema } from '@/lib/tauri-api';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import { useGraphQlSchemaStore } from '../graphql-schema-store';

const introspection = introspectionFromSchema(buildSchema('type Query { a: String }'));
const result = { key: 'k', fetchedAt: '2026-10-05T10:00:00Z', introspection };

describe('graphql schema store', () => {
  beforeEach(() => {
    useGraphQlSchemaStore.setState({ entries: {} });
    vi.mocked(fetchGraphQlSchema).mockReset();
    vi.mocked(getCachedGraphQlSchema).mockReset();
  });

  it('fetchSchema goes loading then ready and keeps a built schema', async () => {
    vi.mocked(fetchGraphQlSchema).mockResolvedValue(result);
    const p = useGraphQlSchemaStore.getState().fetchSchema('t1', createDefaultRequestFor('graphql'), false);
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('loading');
    await p;
    const entry = useGraphQlSchemaStore.getState().entries.t1;
    expect(entry?.status).toBe('ready');
    expect(entry?.schema?.getQueryType()?.name).toBe('Query');
    expect(entry?.fetchedAt).toBe('2026-10-05T10:00:00Z');
    expect(fetchGraphQlSchema).toHaveBeenCalledWith(
      expect.objectContaining({ refresh: false }),
    );
  });

  it('fetchSchema records a readable error and keeps no schema', async () => {
    vi.mocked(fetchGraphQlSchema).mockRejectedValue('Invalid input: introspection is disabled');
    await useGraphQlSchemaStore.getState().fetchSchema('t1', createDefaultRequestFor('graphql'), true);
    const entry = useGraphQlSchemaStore.getState().entries.t1;
    expect(entry?.status).toBe('error');
    expect(entry?.error).toContain('introspection is disabled');
    expect(entry?.schema).toBeUndefined();
  });

  it('loadCached uses the backend cache and never calls fetch', async () => {
    vi.mocked(getCachedGraphQlSchema).mockResolvedValue(result);
    await useGraphQlSchemaStore.getState().loadCached('t1', createDefaultRequestFor('graphql'));
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('ready');
    expect(fetchGraphQlSchema).not.toHaveBeenCalled();
  });

  it('loadCached leaves the entry idle when nothing is cached', async () => {
    vi.mocked(getCachedGraphQlSchema).mockResolvedValue(null);
    await useGraphQlSchemaStore.getState().loadCached('t1', createDefaultRequestFor('graphql'));
    expect(useGraphQlSchemaStore.getState().entries.t1?.status ?? 'idle').toBe('idle');
  });

  it('a broken introspection result becomes an error, not a crash', async () => {
    vi.mocked(fetchGraphQlSchema).mockResolvedValue({ ...result, introspection: { nope: true } });
    await useGraphQlSchemaStore.getState().fetchSchema('t1', createDefaultRequestFor('graphql'), false);
    expect(useGraphQlSchemaStore.getState().entries.t1?.status).toBe('error');
  });
});
```

Create `src/components/request/__tests__/GraphQlDocsExplorer.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { buildSchema } from 'graphql';
import { describe, expect, it, vi } from 'vitest';
import { GraphQlDocsExplorer } from '../GraphQlDocsExplorer';

const schema = buildSchema(`
  type Query { user(id: ID!): User }
  type User { id: ID! name: String posts: [Post!]! }
  type Post { id: ID! title: String }
`);

describe('GraphQlDocsExplorer', () => {
  it('asks the user to fetch the schema when there is none', async () => {
    const onFetch = vi.fn();
    render(
      <GraphQlDocsExplorer schema={undefined} status='idle' onFetch={onFetch} />,
    );
    await userEvent.click(screen.getByRole('button', { name: /fetch schema/i }));
    expect(onFetch).toHaveBeenCalledWith(false);
  });

  it('shows the fetch error', () => {
    render(
      <GraphQlDocsExplorer
        schema={undefined}
        status='error'
        error='introspection is disabled on this server'
        onFetch={vi.fn()}
      />,
    );
    expect(screen.getByText(/introspection is disabled/)).toBeTruthy();
  });

  it('lists the root types and navigates into a field type and back', async () => {
    render(<GraphQlDocsExplorer schema={schema} status='ready' onFetch={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: /query/i }));
    expect(screen.getByText('user')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'User' }));
    expect(screen.getByText('posts')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Post' }));
    expect(screen.getByText('title')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(screen.getByText('posts')).toBeTruthy();
  });

  it('filters the type list by the search text', async () => {
    render(<GraphQlDocsExplorer schema={schema} status='ready' onFetch={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText('Search types'), 'Pos');
    expect(screen.getByRole('button', { name: 'Post' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'User' })).toBeNull();
  });

  it('refreshes with the refresh flag set', async () => {
    const onFetch = vi.fn();
    render(<GraphQlDocsExplorer schema={schema} status='ready' onFetch={onFetch} />);
    await userEvent.click(screen.getByRole('button', { name: /refresh/i }));
    expect(onFetch).toHaveBeenCalledWith(true);
  });
});
```

Add to `src/lib/__tests__/request-profile.test.ts` (inside the two existing `it` blocks, extra assertions):

```ts
    expect(requestProfile('http').showSchema).toBe(false);
    expect(requestProfile('graphql').showSchema).toBe(true);
```

Add to `src/components/request/__tests__/GraphQlEditor.test.tsx`:

```tsx
  it('tells the user that a subscription needs the WebSocket client', async () => {
    vi.mocked(listGraphQlOperations).mockResolvedValue([{ name: 'OnEvent', kind: 'subscription' }]);
    render(
      <GraphQlEditor
        state={{ query: 'subscription OnEvent { event }', variables: '' }}
        onChange={vi.fn()}
      />,
    );
    expect(await screen.findByText(/Subscriptions need the WebSocket client/)).toBeTruthy();
  });
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `yarn test graphql-schema graphql-docs graphql-language-mapping GraphQlDocsExplorer request-profile GraphQlEditor`
Expected: FAIL (modules do not exist).

- [ ] **Step 5: Implement the library modules**

Create `src/lib/graphql-schema.ts`:

```ts
import { buildClientSchema, type GraphQLSchema, type IntrospectionQuery } from 'graphql';

// Builds a schema from the `{ __schema }` object the backend returns.
// `buildClientSchema` throws on anything that is not an introspection result.
export function buildSchemaFromIntrospection(introspection: unknown): GraphQLSchema {
  return buildClientSchema(introspection as IntrospectionQuery);
}
```

Create `src/lib/graphql-docs.ts`:

```ts
import {
  type GraphQLArgument,
  type GraphQLField,
  type GraphQLInputField,
  type GraphQLNamedType,
  type GraphQLSchema,
  getNamedType,
  isEnumType,
  isInputObjectType,
  isInterfaceType,
  isObjectType,
  isScalarType,
  isUnionType,
} from 'graphql';

export interface DocsArg {
  name: string;
  type: string;
  description?: string;
  defaultValue?: string;
}

export interface DocsField {
  name: string;
  /** The full type, such as `[Post!]!`. */
  type: string;
  /** The type without list and non-null wrappers, such as `Post`. */
  namedType: string;
  description?: string;
  deprecation?: string;
  args: DocsArg[];
}

export interface DocsType {
  kind: 'object' | 'interface' | 'union' | 'enum' | 'input' | 'scalar';
  name: string;
  description?: string;
  fields: DocsField[];
  enumValues: { name: string; description?: string; deprecation?: string }[];
  /** Union members, or the implementations of an interface. */
  possibleTypes: string[];
  interfaces: string[];
}

export interface DocsRoot {
  operation: 'query' | 'mutation' | 'subscription';
  typeName: string;
}

export function rootTypes(schema: GraphQLSchema): DocsRoot[] {
  const roots: DocsRoot[] = [];
  const query = schema.getQueryType();
  const mutation = schema.getMutationType();
  const subscription = schema.getSubscriptionType();
  if (query) roots.push({ operation: 'query', typeName: query.name });
  if (mutation) roots.push({ operation: 'mutation', typeName: mutation.name });
  if (subscription) roots.push({ operation: 'subscription', typeName: subscription.name });
  return roots;
}

// Named types sorted by name. Introspection types (`__Schema` and friends) are left out.
export function listTypeNames(schema: GraphQLSchema): string[] {
  return Object.keys(schema.getTypeMap())
    .filter((n) => !n.startsWith('__'))
    .sort();
}

function toArg(a: GraphQLArgument): DocsArg {
  return {
    name: a.name,
    type: String(a.type),
    description: a.description ?? undefined,
    defaultValue: a.defaultValue !== undefined ? JSON.stringify(a.defaultValue) : undefined,
  };
}

function toField(f: GraphQLField<unknown, unknown> | GraphQLInputField): DocsField {
  const args = 'args' in f ? f.args.map(toArg) : [];
  return {
    name: f.name,
    type: String(f.type),
    namedType: getNamedType(f.type).name,
    description: f.description ?? undefined,
    deprecation: f.deprecationReason ?? undefined,
    args,
  };
}

export function describeType(schema: GraphQLSchema, name: string): DocsType | null {
  const type: GraphQLNamedType | undefined = schema.getType(name) ?? undefined;
  if (!type) return null;
  const base = {
    name: type.name,
    description: type.description ?? undefined,
    fields: [] as DocsField[],
    enumValues: [] as DocsType['enumValues'],
    possibleTypes: [] as string[],
    interfaces: [] as string[],
  };
  if (isObjectType(type)) {
    return {
      ...base,
      kind: 'object',
      fields: Object.values(type.getFields()).map(toField),
      interfaces: type.getInterfaces().map((i) => i.name),
    };
  }
  if (isInterfaceType(type)) {
    return {
      ...base,
      kind: 'interface',
      fields: Object.values(type.getFields()).map(toField),
      interfaces: type.getInterfaces().map((i) => i.name),
      possibleTypes: schema.getPossibleTypes(type).map((t) => t.name),
    };
  }
  if (isUnionType(type)) {
    return { ...base, kind: 'union', possibleTypes: schema.getPossibleTypes(type).map((t) => t.name) };
  }
  if (isEnumType(type)) {
    return {
      ...base,
      kind: 'enum',
      enumValues: type.getValues().map((v) => ({
        name: v.name,
        description: v.description ?? undefined,
        deprecation: v.deprecationReason ?? undefined,
      })),
    };
  }
  if (isInputObjectType(type)) {
    return { ...base, kind: 'input', fields: Object.values(type.getFields()).map(toField) };
  }
  if (isScalarType(type)) return { ...base, kind: 'scalar' };
  // Every kind is handled above; this is the safe answer for a future one.
  return null;
}
```

Create `src/lib/graphql-language-mapping.ts`:

```ts
// Pure mapping between graphql-language-service results and Monaco values. The
// Monaco enums are passed in, so this file imports nothing from Monaco and the
// tests need no editor.

export interface MonacoCompletionKinds {
  Field: number;
  Variable: number;
  Class: number;
  Interface: number;
  Property: number;
  Value: number;
  Enum: number;
  EnumMember: number;
  Keyword: number;
  Text: number;
  Constant: number;
  Struct: number;
}

// LSP CompletionItemKind numbers used by graphql-language-service.
export function mapCompletionKind(kind: number | undefined, kinds: MonacoCompletionKinds): number {
  switch (kind) {
    case 5:
      return kinds.Field;
    case 6:
      return kinds.Variable;
    case 7:
      return kinds.Class;
    case 8:
      return kinds.Interface;
    case 10:
      return kinds.Property;
    case 12:
      return kinds.Value;
    case 13:
      return kinds.Enum;
    case 14:
      return kinds.Keyword;
    case 20:
      return kinds.EnumMember;
    case 21:
      return kinds.Constant;
    case 22:
      return kinds.Struct;
    default:
      return kinds.Text;
  }
}

export function completionDocumentation(
  doc: string | { kind?: string; value: string } | null | undefined,
): string | undefined {
  if (doc === null || doc === undefined) return undefined;
  return typeof doc === 'string' ? doc : doc.value;
}

export interface LspDiagnostic {
  message: string;
  /** 1 error, 2 warning, 3 information, 4 hint. Missing means error. */
  severity?: number;
  range: {
    start: { line: number; character: number };
    end: { line: number; character: number };
  };
}

export interface MonacoSeverities {
  Hint: number;
  Info: number;
  Warning: number;
  Error: number;
}

export interface EditorMarker {
  message: string;
  severity: number;
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
}

// LSP positions are zero-based, Monaco's are one-based.
export function diagnosticToMarker(d: LspDiagnostic, severities: MonacoSeverities): EditorMarker {
  const severity =
    d.severity === 2
      ? severities.Warning
      : d.severity === 3
        ? severities.Info
        : d.severity === 4
          ? severities.Hint
          : severities.Error;
  return {
    message: d.message,
    severity,
    startLineNumber: d.range.start.line + 1,
    startColumn: d.range.start.character + 1,
    endLineNumber: d.range.end.line + 1,
    endColumn: d.range.end.character + 1,
  };
}
```

Create `src/lib/graphql-schema-input.ts`:

```ts
import {
  getActiveGlobalEnvName,
  getActiveWorkspaceRequestGuardPolicy,
  resolveRequestFields,
} from '@/lib/execute-request';
import type { ExecuteRequestInput } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

// The HTTP side of an introspection request: the tab's endpoint, headers, auth
// and settings. Scripts, assertions and the body are left out on purpose; the
// backend adds the introspection query and ignores them anyway.
export async function buildSchemaRequestInput(
  tabId: string,
  request: RequestState,
): Promise<ExecuteRequestInput> {
  const resolved = await resolveRequestFields(tabId, request);
  return {
    method: request.method,
    url: resolved.url,
    headers: resolved.headers,
    queryParams: resolved.queryParams,
    auth: resolved.auth,
    options: {
      followRedirects: request.settings?.followRedirects ?? true,
      timeoutMs: request.settings?.timeoutMs ?? 30000,
      verifySsl: request.settings?.verifySsl ?? true,
    },
    collection: resolved.collection,
    environmentName: resolved.environmentName,
    requestPath: resolved.requestPath,
    globalEnvName: getActiveGlobalEnvName(),
    requestName: 'GraphQL introspection',
    requestGuardPolicy: await getActiveWorkspaceRequestGuardPolicy(),
  };
}
```

Create `src/stores/graphql-schema-store.ts`:

```ts
import type { GraphQLSchema } from 'graphql';
import { create } from 'zustand';
import { buildSchemaRequestInput } from '@/lib/graphql-schema-input';
import { buildSchemaFromIntrospection } from '@/lib/graphql-schema';
import {
  fetchGraphQlSchema,
  type GraphQlSchemaResult,
  getCachedGraphQlSchema,
} from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

export interface SchemaEntry {
  status: 'idle' | 'loading' | 'ready' | 'error';
  schema?: GraphQLSchema;
  fetchedAt?: string;
  error?: string;
}

interface GraphQlSchemaState {
  /** One entry per request tab. */
  entries: Record<string, SchemaEntry>;
  /** Reads the backend cache. Never touches the network. */
  loadCached: (tabId: string, request: RequestState) => Promise<void>;
  /** Fetches by introspection, or uses the backend cache unless `refresh` is set. */
  fetchSchema: (tabId: string, request: RequestState, refresh: boolean) => Promise<void>;
  clear: (tabId: string) => void;
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function ready(result: GraphQlSchemaResult): SchemaEntry {
  return {
    status: 'ready',
    schema: buildSchemaFromIntrospection(result.introspection),
    fetchedAt: result.fetchedAt,
  };
}

export const useGraphQlSchemaStore = create<GraphQlSchemaState>((set) => {
  const put = (tabId: string, entry: SchemaEntry) =>
    set((s) => ({ entries: { ...s.entries, [tabId]: entry } }));

  return {
    entries: {},

    async loadCached(tabId, request) {
      try {
        const input = await buildSchemaRequestInput(tabId, request);
        const cached = await getCachedGraphQlSchema(
          input.collection,
          input.environmentName,
          input.url,
        );
        put(tabId, cached ? ready(cached) : { status: 'idle' });
      } catch (err) {
        put(tabId, { status: 'error', error: message(err) });
      }
    },

    async fetchSchema(tabId, request, refresh) {
      put(tabId, { status: 'loading' });
      try {
        const input = await buildSchemaRequestInput(tabId, request);
        const result = await fetchGraphQlSchema({ request: input, refresh });
        put(tabId, ready(result));
      } catch (err) {
        put(tabId, { status: 'error', error: message(err) });
      }
    },

    clear(tabId) {
      set((s) => {
        const { [tabId]: _removed, ...rest } = s.entries;
        return { entries: rest };
      });
    },
  };
});
```

In `src/lib/request-profile.ts`, add `showSchema: boolean;` to `RequestProfile`, `showSchema: true` to the `graphql` return and `showSchema: false` to the HTTP return.

- [ ] **Step 6: Implement the Monaco binding**

Create `src/components/editor/graphql-language.ts`. It imports `monaco-editor`, so it must only be loaded through `import()`:

```ts
import type { GraphQLSchema } from 'graphql';
import { getAutocompleteSuggestions, getDiagnostics, Position } from 'graphql-language-service';
import * as monaco from 'monaco-editor';
import {
  completionDocumentation,
  diagnosticToMarker,
  mapCompletionKind,
} from '@/lib/graphql-language-mapping';

export interface GraphQlSupport {
  /** Re-run the diagnostics, for example after the schema changed. */
  revalidate: () => void;
  dispose: () => void;
}

// Registers schema-aware completion for one editor and keeps its diagnostics
// current. The completion provider is registered for the `graphql` language, so
// it checks the model id and answers only for this editor's model.
export function attachGraphQlSupport(
  editor: monaco.editor.IStandaloneCodeEditor,
  getSchema: () => GraphQLSchema | undefined,
): GraphQlSupport {
  const model = editor.getModel();
  if (!model) return { revalidate: () => {}, dispose: () => {} };

  const completion = monaco.languages.registerCompletionItemProvider('graphql', {
    triggerCharacters: ['{', '(', ' ', ':', '$', '@', '.', '\n'],
    provideCompletionItems(m, position) {
      const schema = getSchema();
      if (m.id !== model.id || !schema) return { suggestions: [] };
      const items = getAutocompleteSuggestions(
        schema,
        m.getValue(),
        new Position(position.lineNumber - 1, position.column - 1),
      );
      const word = m.getWordUntilPosition(position);
      const range = new monaco.Range(
        position.lineNumber,
        word.startColumn,
        position.lineNumber,
        word.endColumn,
      );
      return {
        suggestions: items.map((item) => ({
          label: item.label,
          kind: mapCompletionKind(item.kind, monaco.languages.CompletionItemKind),
          detail: item.detail ?? undefined,
          documentation: completionDocumentation(item.documentation),
          insertText: item.insertText ?? item.label,
          range,
        })),
      };
    },
  });

  const revalidate = () => {
    // Without a schema this still reports syntax errors.
    const diagnostics = getDiagnostics(model.getValue(), getSchema());
    monaco.editor.setModelMarkers(
      model,
      'graphql',
      diagnostics.map((d) => diagnosticToMarker(d, monaco.MarkerSeverity)),
    );
  };

  let timer: ReturnType<typeof setTimeout> | undefined;
  const onChange = editor.onDidChangeModelContent(() => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(revalidate, 300);
  });
  revalidate();

  return {
    revalidate,
    dispose() {
      if (timer) clearTimeout(timer);
      onChange.dispose();
      completion.dispose();
      monaco.editor.setModelMarkers(model, 'graphql', []);
    },
  };
}
```

If `getAutocompleteSuggestions` items expose `kind` under a different type than `number | undefined` in the installed version, widen `mapCompletionKind`'s first parameter accordingly. `yarn tsc --noEmit` reports this.

- [ ] **Step 7: Implement the docs explorer**

Create `src/components/request/GraphQlDocsExplorer.tsx`:

```tsx
import type { GraphQLSchema } from 'graphql';
import { AlertTriangle, ArrowLeft, RefreshCw } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { describeType, listTypeNames, rootTypes } from '@/lib/graphql-docs';
import type { SchemaEntry } from '@/stores/graphql-schema-store';

interface GraphQlDocsExplorerProps {
  schema: GraphQLSchema | undefined;
  status: SchemaEntry['status'];
  error?: string;
  fetchedAt?: string;
  /** `refresh` is true when the user asks to fetch again. */
  onFetch: (refresh: boolean) => void;
}

// A navigable view of the schema: root types, then any type by name.
export function GraphQlDocsExplorer({
  schema,
  status,
  error,
  fetchedAt,
  onFetch,
}: GraphQlDocsExplorerProps) {
  const [stack, setStack] = useState<string[]>([]);
  const [search, setSearch] = useState('');

  const names = useMemo(() => (schema ? listTypeNames(schema) : []), [schema]);
  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? names.filter((n) => n.toLowerCase().includes(q)) : [];
  }, [names, search]);

  const loading = status === 'loading';
  const current = stack.length > 0 ? stack[stack.length - 1] : undefined;
  const view = schema && current ? describeType(schema, current) : null;

  const toolbar = (
    <div className='flex items-center gap-2 border-b border-border px-3 py-1.5 shrink-0'>
      <Button
        size='sm'
        variant='outline'
        className='h-7'
        disabled={loading}
        onClick={() => onFetch(Boolean(schema))}
      >
        <RefreshCw className={`mr-1 h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
        {schema ? 'Refresh' : 'Fetch schema'}
      </Button>
      {fetchedAt && (
        <span className='text-xs text-muted-foreground'>
          Fetched {new Date(fetchedAt).toLocaleString()}
        </span>
      )}
    </div>
  );

  if (!schema) {
    return (
      <div className='flex h-full flex-col'>
        {toolbar}
        <div className='flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center'>
          {error ? (
            <Alert variant='destructive' className='max-w-md text-left'>
              <AlertTriangle className='h-4 w-4' />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : (
            <p className='max-w-sm text-xs text-muted-foreground'>
              Fetch the schema to browse its types and get completion and validation in the query
              editor. The request uses this tab's URL, headers and auth.
            </p>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className='flex h-full flex-col'>
      {toolbar}
      <div className='flex items-center gap-2 px-3 py-2 shrink-0'>
        {stack.length > 0 && (
          <Button
            size='sm'
            variant='ghost'
            className='h-7 px-2'
            onClick={() => setStack((s) => s.slice(0, -1))}
            aria-label='Back'
          >
            <ArrowLeft className='h-3.5 w-3.5' />
          </Button>
        )}
        <Input
          className='h-7 text-xs'
          placeholder='Search types'
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>
      <ScrollArea className='flex-1 min-h-0'>
        <div className='space-y-1 px-3 pb-4'>
          {search.trim() ? (
            filtered.map((n) => (
              <Button
                key={n}
                variant='link'
                className='h-6 px-0 text-xs font-mono'
                onClick={() => {
                  setStack((s) => [...s, n]);
                  setSearch('');
                }}
              >
                {n}
              </Button>
            ))
          ) : view ? (
            <TypeView
              view={view}
              onOpen={(name) => setStack((s) => [...s, name])}
            />
          ) : (
            rootTypes(schema).map((r) => (
              <div key={r.operation} className='flex items-center gap-2'>
                <span className='w-20 text-xs text-muted-foreground'>{r.operation}</span>
                <Button
                  variant='link'
                  className='h-6 px-0 text-xs font-mono'
                  onClick={() => setStack([r.typeName])}
                >
                  {r.typeName}
                </Button>
              </div>
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

function TypeLink({ name, onOpen }: { name: string; onOpen: (name: string) => void }) {
  return (
    <Button variant='link' className='h-5 px-0 text-xs font-mono' onClick={() => onOpen(name)}>
      {name}
    </Button>
  );
}

function TypeView({
  view,
  onOpen,
}: {
  view: NonNullable<ReturnType<typeof describeType>>;
  onOpen: (name: string) => void;
}) {
  return (
    <div className='space-y-3 text-xs'>
      <div>
        <div className='font-mono text-sm font-semibold'>{view.name}</div>
        <div className='text-muted-foreground'>{view.kind}</div>
        {view.description && <p className='mt-1 text-muted-foreground'>{view.description}</p>}
      </div>
      {view.interfaces.length > 0 && (
        <div className='flex flex-wrap items-center gap-1'>
          <span className='text-muted-foreground'>implements</span>
          {view.interfaces.map((i) => (
            <TypeLink key={i} name={i} onOpen={onOpen} />
          ))}
        </div>
      )}
      {view.possibleTypes.length > 0 && (
        <div className='flex flex-wrap items-center gap-1'>
          <span className='text-muted-foreground'>
            {view.kind === 'union' ? 'one of' : 'implemented by'}
          </span>
          {view.possibleTypes.map((t) => (
            <TypeLink key={t} name={t} onOpen={onOpen} />
          ))}
        </div>
      )}
      {view.fields.map((f) => (
        <div key={f.name} className='space-y-0.5'>
          <div className='flex flex-wrap items-center gap-1 font-mono'>
            <span className={f.deprecation ? 'line-through' : ''}>{f.name}</span>
            {f.args.length > 0 && (
              <span className='text-muted-foreground'>
                ({f.args.map((a) => `${a.name}: ${a.type}`).join(', ')})
              </span>
            )}
            <span className='text-muted-foreground'>:</span>
            <TypeLink name={f.namedType} onOpen={onOpen} />
            <span className='text-muted-foreground'>{f.type !== f.namedType ? f.type : ''}</span>
          </div>
          {f.description && <p className='pl-2 text-muted-foreground'>{f.description}</p>}
          {f.deprecation && <p className='pl-2 text-amber-500'>Deprecated: {f.deprecation}</p>}
        </div>
      ))}
      {view.enumValues.map((v) => (
        <div key={v.name} className='font-mono'>
          <span className={v.deprecation ? 'line-through' : ''}>{v.name}</span>
          {v.description && (
            <span className='ml-2 font-sans text-muted-foreground'>{v.description}</span>
          )}
        </div>
      ))}
    </div>
  );
}
```

The test `getByRole('button', { name: 'User' })` finds the type link; `getByRole('button', { name: /query/i })` finds the root `Query` link because the `query` operation label is plain text, not a button.

- [ ] **Step 8: Wire the editor and the Schema tab**

In `src/components/request/GraphQlEditor.tsx`:

1. Imports: add `useCallback, useRef` to the `react` import, `import type { GraphQLSchema } from 'graphql';`, `import type * as monacoNs from 'monaco-editor';` (a type-only import: it adds nothing to the bundle), and `Info` to the `lucide-react` import.
2. Add `schema?: GraphQLSchema;` to `GraphQlEditorProps` and destructure it.
3. Add the support wiring after the existing state:

```tsx
  const schemaRef = useRef<GraphQLSchema | undefined>(schema);
  const supportRef = useRef<{ revalidate: () => void; dispose: () => void } | null>(null);
  const unmountedRef = useRef(false);

  // Keep the provider reading the latest schema and refresh the diagnostics.
  useEffect(() => {
    schemaRef.current = schema;
    supportRef.current?.revalidate();
  }, [schema]);

  useEffect(() => {
    unmountedRef.current = false;
    return () => {
      unmountedRef.current = true;
      supportRef.current?.dispose();
      supportRef.current = null;
    };
  }, []);

  // Monaco is loaded lazily, so the language support is too.
  const handleQueryEditorReady = useCallback((editor: monacoNs.editor.IStandaloneCodeEditor) => {
    void import('@/components/editor/graphql-language').then((m) => {
      if (unmountedRef.current) return;
      supportRef.current?.dispose();
      supportRef.current = m.attachGraphQlSupport(editor, () => schemaRef.current);
    });
  }, []);
```

4. Pass `onEditorReady={handleQueryEditorReady}` to the query `MonacoWrapper` (the one with `language='graphql'`) only.
5. Add the subscription notice under the operation toolbar:

```tsx
  const chosen = operations.find((o) => o.name === state.operationName) ?? operations[0];
  const isSubscription = chosen?.kind === 'subscription';
```

   and render, before the query editor block:

```tsx
      {isSubscription && (
        <div className='flex items-center gap-2 border-b border-border bg-muted/40 px-3 py-1.5 text-xs text-muted-foreground shrink-0'>
          <Info className='h-3.5 w-3.5' aria-hidden='true' />
          Subscriptions need the WebSocket client and cannot be sent over HTTP.
        </div>
      )}
```

   The message must contain the text `Subscriptions need the WebSocket client`, which the test looks for.

In `src/components/request/RequestPanel.tsx`:

1. Imports: `GraphQlDocsExplorer`, `useGraphQlSchemaStore`.
2. Add `'schema'` to the `SectionTab` union.
3. After the `profile` constant add:

```tsx
  const schemaEntry = useGraphQlSchemaStore((s) => s.entries[tab.id]);
  const loadCachedSchema = useGraphQlSchemaStore((s) => s.loadCached);
  const fetchSchema = useGraphQlSchemaStore((s) => s.fetchSchema);
  const activeEnvForSchema = useEnvStore((s) => s.activeEnvId);

  // Pick up a schema already fetched for this endpoint, without any network call.
  useEffect(() => {
    if (!isGraphQl) return;
    void loadCachedSchema(tab.id, request);
    // The cache is keyed by endpoint and environment, so those are the triggers.
    // biome-ignore lint/correctness/useExhaustiveDependencies: `request` changes on every keystroke
  }, [isGraphQl, tab.id, request.url, activeEnvForSchema, loadCachedSchema]);
```

   (`useEnvStore` is already imported in this file as the source of `activeEnvIdForScope`; reuse that variable instead of a second selector if it is in scope: `activeEnvIdForScope`.)
4. In `tabDefs`, add a tab after `docs` for GraphQL only:

```tsx
      ...(profile.showSchema
        ? [
            {
              value: 'schema',
              label: <>Schema</>,
              isActive: activeSection === 'schema',
              onClick: () => setActiveSection('schema'),
            },
          ]
        : []),
```

   Add `profile.showSchema` to the memo's dependencies.
5. Pass the schema to the editor: `schema={schemaEntry?.schema}` on `<GraphQlEditor>`.
6. Render the explorer next to the other full-height sections, and add `activeSection === 'schema' ||` to the `hidden` condition of the generic container:

```tsx
      {activeSection === 'schema' ? (
        <div className='flex-1 min-h-0 overflow-hidden'>
          <GraphQlDocsExplorer
            schema={schemaEntry?.schema}
            status={schemaEntry?.status ?? 'idle'}
            error={schemaEntry?.error}
            fetchedAt={schemaEntry?.fetchedAt}
            onFetch={(refresh) => void fetchSchema(tab.id, request, refresh)}
          />
        </div>
      ) : null}
```

- [ ] **Step 9: Run the checks**

Run:
- `yarn test graphql-schema graphql-docs graphql-language-mapping GraphQlDocsExplorer GraphQlEditor request-profile`
- `yarn tsc --noEmit`
- `yarn check`

Expected: PASS. If `yarn check` reports the import order in the new files, run `yarn lint` for those files and re-run `yarn check`.

- [ ] **Step 10: Verify the bundle split and try it in the app**

Run: `yarn build` and compare the main chunk size with the build on `main`; the main chunk must not grow by the size of Monaco. If it does, a module that imports `monaco-editor` is imported statically: find it with `grep -rn "graphql-language'" src` and confirm only `import('@/components/editor/graphql-language')` appears.

Run `yarn tauri dev`. Open a GraphQL request that points at a public endpoint, open the Schema tab, press Fetch schema, and confirm: root types appear, field types are clickable, search filters, Back works; in the Query tab, typing `{ ` offers field names, an unknown field is underlined with the server-schema message, and closing and reopening the tab shows the schema without a new request (the cache), while Refresh fetches again.

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add package.json yarn.lock \
  src/lib/graphql-schema.ts src/lib/graphql-docs.ts src/lib/graphql-language-mapping.ts \
  src/lib/graphql-schema-input.ts src/lib/request-profile.ts src/lib/__tests__ \
  src/components/editor/graphql-language.ts src/stores/graphql-schema-store.ts \
  src/stores/__tests__/graphql-schema-store.test.ts \
  src/components/request/GraphQlDocsExplorer.tsx src/components/request/GraphQlEditor.tsx \
  src/components/request/RequestPanel.tsx src/components/request/__tests__
```

Suggested subject: `feat(graphql): add a docs explorer and schema-aware editor`.

---

## Task 3: Query builder

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/lib/graphql-query-builder.ts`
- Create: `src/components/request/GraphQlQueryBuilder.tsx`
- Modify: `src/components/request/RequestPanel.tsx`
- Create tests: `src/lib/__tests__/graphql-query-builder.test.ts`, `src/components/request/__tests__/GraphQlQueryBuilder.test.tsx`

**Interfaces:**
- Consumes: `GraphQLSchema` (Task 2), `DEFAULT_GRAPHQL_QUERY` (`src/lib/pane-utils.ts`), `RequestState.graphql` (Plan 05).
- Produces:
  - `buildOperation(schema, input): BuilderOutput` where `BuilderInput = { operation: 'query' | 'mutation'; name?: string; paths: string[] }` and `BuilderOutput = { query: string; variables: string; operationName?: string }`. A path is the dot-joined field names from the root type, for example `user.posts.title`.
  - `shouldConfirmReplace(currentQuery: string): boolean`.
  - `<GraphQlQueryBuilder schema currentQuery onApply />` with `onApply(output: BuilderOutput): void`. `currentQuery` is the query now in the editor.

Scope, so the implementer does not widen it:
- Fields of union types and the implementations of interface types are not expandable. A union field gets `__typename` only, because a union needs inline fragments, which this builder does not write.
- Only required arguments (non-null with no default) become variables. Optional arguments are left out. The user can add them in the editor, which has completion.
- Subscriptions are not offered. They need the WebSocket client.
- The builder writes into the request on demand and never edits the query while it is open.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing builder tests**

Create `src/lib/__tests__/graphql-query-builder.test.ts`:

```ts
import { buildSchema } from 'graphql';
import { describe, expect, it } from 'vitest';
import { buildOperation, shouldConfirmReplace } from '../graphql-query-builder';

const schema = buildSchema(`
  type Query {
    me: User
    user(id: ID!, verbose: Boolean = false): User
    post(id: ID!): Post
    search(term: String!, limit: Int): [Result!]!
    ping: String
  }
  type Mutation { rename(id: ID!, name: String!): User }
  type User { id: ID! name: String posts(first: Int!): [Post!]! role: Role }
  type Post { id: ID! title: String author: User }
  union Result = User | Post
  enum Role { ADMIN MEMBER }
`);

describe('buildOperation', () => {
  it('writes a plain selection for a field without arguments', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'Me', paths: ['me.id', 'me.name'] });
    expect(out.query).toBe('query Me {\n  me {\n    id\n    name\n  }\n}\n');
    expect(out.variables).toBe('');
    expect(out.operationName).toBe('Me');
  });

  it('turns a required argument into a typed variable and a variables skeleton', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'U', paths: ['user.name'] });
    expect(out.query).toBe(
      'query U($id: ID!) {\n  user(id: $id) {\n    name\n  }\n}\n',
    );
    expect(JSON.parse(out.variables)).toEqual({ id: '' });
  });

  it('skips optional arguments and arguments with defaults', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'S', paths: ['search.__typename'] });
    expect(out.query).toContain('search(term: $term)');
    expect(out.query).not.toContain('limit');
    expect(out.query).not.toContain('verbose');
  });

  it('gives colliding variable names a distinct name', () => {
    const out = buildOperation(schema, {
      operation: 'query',
      name: 'Both',
      paths: ['user.name', 'post.title'],
    });
    expect(out.query).toContain('$id: ID!');
    expect(out.query).toContain('$postId: ID!');
    expect(out.query).toContain('user(id: $id)');
    expect(out.query).toContain('post(id: $postId)');
    expect(Object.keys(JSON.parse(out.variables)).sort()).toEqual(['id', 'postId']);
  });

  it('handles a required argument on a nested field', () => {
    const out = buildOperation(schema, {
      operation: 'query',
      name: 'N',
      paths: ['me.posts.title'],
    });
    expect(out.query).toContain('query N($first: Int!)');
    expect(out.query).toContain('posts(first: $first)');
    expect(JSON.parse(out.variables)).toEqual({ first: 0 });
  });

  it('never leaves an object field with an empty selection set', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'E', paths: ['me'] });
    expect(out.query).toBe('query E {\n  me {\n    __typename\n  }\n}\n');
  });

  it('selects only __typename on a union field', () => {
    const out = buildOperation(schema, { operation: 'query', name: 'R', paths: ['search'] });
    expect(out.query).toContain('search(term: $term) {\n    __typename\n  }');
  });

  it('builds a mutation from the mutation root', () => {
    const out = buildOperation(schema, { operation: 'mutation', name: 'Rename', paths: ['rename.id'] });
    expect(out.query).toBe(
      'mutation Rename($id: ID!, $name: String!) {\n  rename(id: $id, name: $name) {\n    id\n  }\n}\n',
    );
    expect(JSON.parse(out.variables)).toEqual({ id: '', name: '' });
  });

  it('fills enum variables with the first value', () => {
    const s = buildSchema('type Query { byRole(role: Role!): String } enum Role { ADMIN MEMBER }');
    const out = buildOperation(s, { operation: 'query', name: 'R', paths: ['byRole'] });
    expect(JSON.parse(out.variables)).toEqual({ role: 'ADMIN' });
  });

  it('ignores paths that are not in the schema and returns an empty query for no paths', () => {
    expect(buildOperation(schema, { operation: 'query', name: 'X', paths: ['nope.x'] }).query).toBe('');
    expect(buildOperation(schema, { operation: 'query', name: 'X', paths: [] }).query).toBe('');
  });

  it('sanitises the operation name and omits it when nothing is left', () => {
    const named = buildOperation(schema, { operation: 'query', name: 'my query!', paths: ['ping'] });
    expect(named.query.startsWith('query myquery {')).toBe(true);
    const anon = buildOperation(schema, { operation: 'query', name: '!!', paths: ['ping'] });
    expect(anon.query.startsWith('query {')).toBe(true);
    expect(anon.operationName).toBeUndefined();
  });

  it('returns an empty query when the schema has no mutation root', () => {
    const s = buildSchema('type Query { a: String }');
    expect(buildOperation(s, { operation: 'mutation', name: 'M', paths: ['a'] }).query).toBe('');
  });
});

describe('shouldConfirmReplace', () => {
  it('does not ask before replacing an empty or default query', () => {
    expect(shouldConfirmReplace('')).toBe(false);
    expect(shouldConfirmReplace('  \n')).toBe(false);
    expect(shouldConfirmReplace('{\n  __typename\n}\n')).toBe(false);
  });

  it('asks before replacing a query the user wrote', () => {
    expect(shouldConfirmReplace('{ users { id } }')).toBe(true);
  });
});
```

Create `src/components/request/__tests__/GraphQlQueryBuilder.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { buildSchema } from 'graphql';
import { describe, expect, it, vi } from 'vitest';
import { GraphQlQueryBuilder } from '../GraphQlQueryBuilder';

const schema = buildSchema(`
  type Query { user(id: ID!): User ping: String }
  type User { id: ID! name: String }
`);

describe('GraphQlQueryBuilder', () => {
  it('previews the query for the ticked fields', async () => {
    render(<GraphQlQueryBuilder schema={schema} currentQuery='' onApply={vi.fn()} />);
    await userEvent.click(screen.getByRole('checkbox', { name: 'ping' }));
    expect(screen.getByTestId('builder-preview').textContent).toContain('ping');
  });

  it('expands an object field and ticks a nested field', async () => {
    render(<GraphQlQueryBuilder schema={schema} currentQuery='' onApply={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Expand user' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'user.name' }));
    const preview = screen.getByTestId('builder-preview').textContent ?? '';
    expect(preview).toContain('user(id: $id)');
    expect(preview).toContain('name');
  });

  it('applies straight away when the current query is the default', async () => {
    const onApply = vi.fn();
    render(
      <GraphQlQueryBuilder schema={schema} currentQuery={'{\n  __typename\n}\n'} onApply={onApply} />,
    );
    await userEvent.click(screen.getByRole('checkbox', { name: 'ping' }));
    await userEvent.click(screen.getByRole('button', { name: 'Use in request' }));
    expect(onApply).toHaveBeenCalledTimes(1);
    expect(onApply.mock.calls[0][0].query).toContain('ping');
  });

  it('asks before replacing a query the user wrote', async () => {
    const onApply = vi.fn();
    render(
      <GraphQlQueryBuilder schema={schema} currentQuery='{ users { id } }' onApply={onApply} />,
    );
    await userEvent.click(screen.getByRole('checkbox', { name: 'ping' }));
    await userEvent.click(screen.getByRole('button', { name: 'Use in request' }));
    expect(onApply).not.toHaveBeenCalled();
    await userEvent.click(await screen.findByRole('button', { name: 'Replace query' }));
    await waitFor(() => expect(onApply).toHaveBeenCalledTimes(1));
  });

  it('disables Use in request until a field is ticked', () => {
    render(<GraphQlQueryBuilder schema={schema} currentQuery='' onApply={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Use in request' }).hasAttribute('disabled')).toBe(true);
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test graphql-query-builder GraphQlQueryBuilder`
Expected: FAIL (modules do not exist).

- [ ] **Step 4: Implement the builder**

Create `src/lib/graphql-query-builder.ts`:

```ts
import {
  type GraphQLArgument,
  type GraphQLField,
  type GraphQLInputType,
  type GraphQLObjectType,
  type GraphQLSchema,
  getNamedType,
  isEnumType,
  isInputObjectType,
  isInterfaceType,
  isListType,
  isNonNullType,
  isObjectType,
  isScalarType,
  isUnionType,
} from 'graphql';
import { DEFAULT_GRAPHQL_QUERY } from '@/lib/pane-utils';

export interface BuilderInput {
  operation: 'query' | 'mutation';
  name?: string;
  /** Dot-joined field names from the root type, such as `user.posts.title`. */
  paths: string[];
}

export interface BuilderOutput {
  query: string;
  /** A JSON skeleton of the variables, or an empty string when there are none. */
  variables: string;
  operationName?: string;
}

interface Tree {
  [field: string]: Tree;
}

interface Variable {
  name: string;
  type: GraphQLInputType;
}

const INDENT = '  ';

function buildTree(paths: string[]): Tree {
  const root: Tree = {};
  for (const path of paths) {
    let node = root;
    for (const part of path.split('.')) {
      node[part] = node[part] ?? {};
      node = node[part];
    }
  }
  return root;
}

// A required argument is non-null and has no default.
function requiredArgs(field: GraphQLField<unknown, unknown>): GraphQLArgument[] {
  return field.args.filter((a) => isNonNullType(a.type) && a.defaultValue === undefined);
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0].toUpperCase() + s.slice(1) : s;
}

// The first free name: the argument name, then `<field><Arg>`, then a numbered name.
function variableName(arg: string, field: string, taken: Set<string>): string {
  const candidates = [arg, `${field}${capitalize(arg)}`];
  for (const c of candidates) if (!taken.has(c)) return c;
  let i = 2;
  while (taken.has(`${arg}${i}`)) i += 1;
  return `${arg}${i}`;
}

function skeleton(type: GraphQLInputType): unknown {
  if (isNonNullType(type)) return skeleton(type.ofType);
  if (isListType(type)) return [];
  const named = getNamedType(type);
  if (isEnumType(named)) return named.getValues()[0]?.name ?? '';
  if (isInputObjectType(named)) return {};
  if (isScalarType(named)) {
    switch (named.name) {
      case 'Int':
      case 'Float':
        return 0;
      case 'Boolean':
        return false;
      default:
        return '';
    }
  }
  return null;
}

function emit(
  type: GraphQLObjectType,
  tree: Tree,
  depth: number,
  variables: Variable[],
  taken: Set<string>,
): string {
  const pad = INDENT.repeat(depth);
  const lines: string[] = [];
  for (const field of Object.values(type.getFields())) {
    const sub = tree[field.name];
    if (!sub) continue;

    let call = field.name;
    const args = requiredArgs(field);
    if (args.length > 0) {
      const parts = args.map((a) => {
        const name = variableName(a.name, field.name, taken);
        taken.add(name);
        variables.push({ name, type: a.type });
        return `${a.name}: $${name}`;
      });
      call += `(${parts.join(', ')})`;
    }

    const named = getNamedType(field.type);
    if (isObjectType(named)) {
      const children = emit(named, sub, depth + 1, variables, taken);
      const body = children || `${pad}${INDENT}__typename\n`;
      lines.push(`${pad}${call} {\n${body}${pad}}\n`);
    } else if (isInterfaceType(named) || isUnionType(named)) {
      // These need inline fragments, which this builder does not write.
      lines.push(`${pad}${call} {\n${pad}${INDENT}__typename\n${pad}}\n`);
    } else {
      lines.push(`${pad}${call}\n`);
    }
  }
  return lines.join('');
}

// Builds an operation from a set of selected field paths. Unknown paths are
// ignored. With nothing to select, the query is an empty string.
export function buildOperation(schema: GraphQLSchema, input: BuilderInput): BuilderOutput {
  const root = input.operation === 'query' ? schema.getQueryType() : schema.getMutationType();
  const tree = buildTree(input.paths);
  if (!root) return { query: '', variables: '' };

  const variables: Variable[] = [];
  const body = emit(root, tree, 1, variables, new Set());
  if (!body) return { query: '', variables: '' };

  const name = (input.name ?? '').replace(/[^_A-Za-z0-9]/g, '');
  const defs = variables.map((v) => `$${v.name}: ${String(v.type)}`).join(', ');
  const header = `${input.operation}${name ? ` ${name}` : ''}${defs ? `(${defs})` : ''}`;
  const skeletonObject = Object.fromEntries(variables.map((v) => [v.name, skeleton(v.type)]));

  return {
    query: `${header} {\n${body}}\n`,
    variables: variables.length > 0 ? JSON.stringify(skeletonObject, null, 2) : '',
    operationName: name || undefined,
  };
}

// The builder replaces the whole query, so a query the user wrote needs a confirmation.
export function shouldConfirmReplace(currentQuery: string): boolean {
  const trimmed = currentQuery.trim();
  return trimmed !== '' && trimmed !== DEFAULT_GRAPHQL_QUERY.trim();
}
```

- [ ] **Step 5: Implement the builder component**

Create `src/components/request/GraphQlQueryBuilder.tsx`:

```tsx
import { type GraphQLObjectType, type GraphQLSchema, getNamedType, isObjectType } from 'graphql';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  type BuilderOutput,
  buildOperation,
  shouldConfirmReplace,
} from '@/lib/graphql-query-builder';

interface GraphQlQueryBuilderProps {
  schema: GraphQLSchema;
  /** The query currently in the editor, to decide whether replacing it needs a confirmation. */
  currentQuery: string;
  onApply: (output: BuilderOutput) => void;
}

// A field tree for the query or mutation root. Ticking fields builds the query
// text; "Use in request" writes it into the request.
export function GraphQlQueryBuilder({ schema, currentQuery, onApply }: GraphQlQueryBuilderProps) {
  const [operation, setOperation] = useState<'query' | 'mutation'>('query');
  const [name, setName] = useState('');
  const [paths, setPaths] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);

  const root = operation === 'query' ? schema.getQueryType() : schema.getMutationType();
  const output = useMemo(
    () => buildOperation(schema, { operation, name, paths: [...paths] }),
    [schema, operation, name, paths],
  );

  const toggle = (path: string) =>
    setPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        // Unticking a field also unticks everything below it.
        for (const p of prev) if (p === path || p.startsWith(`${path}.`)) next.delete(p);
      } else {
        next.add(path);
      }
      return next;
    });

  const toggleExpanded = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const apply = () => {
    if (shouldConfirmReplace(currentQuery)) setConfirming(true);
    else onApply(output);
  };

  const renderFields = (type: GraphQLObjectType, prefix: string, depth: number) =>
    Object.values(type.getFields()).map((field) => {
      const path = prefix ? `${prefix}.${field.name}` : field.name;
      const named = getNamedType(field.type);
      const expandable = isObjectType(named);
      const open = expanded.has(path);
      return (
        <div key={path}>
          <div className='flex items-center gap-1.5 py-0.5' style={{ paddingLeft: depth * 16 }}>
            {expandable ? (
              <Button
                variant='ghost'
                size='icon'
                className='h-5 w-5'
                aria-label={`${open ? 'Collapse' : 'Expand'} ${path}`}
                onClick={() => toggleExpanded(path)}
              >
                {open ? (
                  <ChevronDown className='h-3.5 w-3.5' />
                ) : (
                  <ChevronRight className='h-3.5 w-3.5' />
                )}
              </Button>
            ) : (
              <span className='inline-block w-5' />
            )}
            <Checkbox
              checked={paths.has(path)}
              onCheckedChange={() => toggle(path)}
              aria-label={path}
            />
            <span className='font-mono text-xs'>{field.name}</span>
            <span className='font-mono text-2xs text-muted-foreground'>{String(field.type)}</span>
          </div>
          {expandable && open && renderFields(named, path, depth + 1)}
        </div>
      );
    });

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center gap-2 border-b border-border px-3 py-1.5 shrink-0'>
        <Tabs
          value={operation}
          onValueChange={(v) => {
            setOperation(v as 'query' | 'mutation');
            setPaths(new Set());
            setExpanded(new Set());
          }}
        >
          <TabsList className='h-6'>
            <TabsTrigger value='query' className='text-[10px] px-2.5 py-0.5'>
              Query
            </TabsTrigger>
            <TabsTrigger
              value='mutation'
              className='text-[10px] px-2.5 py-0.5'
              disabled={!schema.getMutationType()}
            >
              Mutation
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <Input
          className='h-7 w-40 text-xs'
          placeholder='Operation name'
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Button size='sm' className='ml-auto h-7' disabled={!output.query} onClick={apply}>
          Use in request
        </Button>
      </div>

      <div className='flex min-h-0 flex-1'>
        <ScrollArea className='flex-1 min-w-0 border-r border-border'>
          <div className='p-2'>
            {root ? (
              renderFields(root, '', 0)
            ) : (
              <p className='p-2 text-xs text-muted-foreground'>This schema has no {operation} root.</p>
            )}
          </div>
        </ScrollArea>
        <ScrollArea className='flex-1 min-w-0'>
          <pre data-testid='builder-preview' className='p-3 font-mono text-xs whitespace-pre-wrap'>
            {output.query || 'Tick fields to build a query.'}
            {output.variables ? `\n# Variables\n${output.variables}` : ''}
          </pre>
        </ScrollArea>
      </div>

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Replace the query?</AlertDialogTitle>
            <AlertDialogDescription>
              The request already has a query. Using the builder replaces it and its variables.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setConfirming(false);
                onApply(output);
              }}
            >
              Replace query
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
```

The test `getByRole('button', { name: 'Expand user' })` relies on the `aria-label`. The preview `<pre>` is not a form control, so the "no raw form elements" rule does not apply to it.

- [ ] **Step 6: Mount the builder in the Schema tab**

In `src/components/request/RequestPanel.tsx`, import `GraphQlQueryBuilder`, `Tabs`/`TabsList`/`TabsTrigger` (already imported for the docs mode switch), and add a local state next to the other `useState`s:

```tsx
  const [schemaView, setSchemaView] = useState<'docs' | 'builder'>('docs');
```

Replace the Schema section from Task 2 with:

```tsx
      {activeSection === 'schema' ? (
        <div className='flex flex-1 min-h-0 flex-col overflow-hidden'>
          <div className='flex items-center border-b border-border px-3 py-1 shrink-0'>
            <Tabs value={schemaView} onValueChange={(v) => setSchemaView(v as 'docs' | 'builder')}>
              <TabsList className='h-6'>
                <TabsTrigger value='docs' className='text-[10px] px-2.5 py-0.5'>
                  Docs
                </TabsTrigger>
                <TabsTrigger
                  value='builder'
                  className='text-[10px] px-2.5 py-0.5'
                  disabled={!schemaEntry?.schema}
                >
                  Builder
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </div>
          <div className='flex-1 min-h-0 overflow-hidden'>
            {schemaView === 'builder' && schemaEntry?.schema ? (
              <GraphQlQueryBuilder
                schema={schemaEntry.schema}
                currentQuery={request.graphql?.query ?? ''}
                onApply={(out) => {
                  handleGraphQlChange({
                    query: out.query,
                    variables: out.variables,
                    operationName: out.operationName,
                  });
                  setActiveSection('body');
                }}
              />
            ) : (
              <GraphQlDocsExplorer
                schema={schemaEntry?.schema}
                status={schemaEntry?.status ?? 'idle'}
                error={schemaEntry?.error}
                fetchedAt={schemaEntry?.fetchedAt}
                onFetch={(refresh) => void fetchSchema(tab.id, request, refresh)}
              />
            )}
          </div>
        </div>
      ) : null}
```

- [ ] **Step 7: Run the checks**

Run:
- `yarn test graphql-query-builder GraphQlQueryBuilder GraphQlDocsExplorer GraphQlEditor`
- `yarn tsc --noEmit`
- `yarn check`
- `cargo check -j4 -p rocket`

Expected: PASS.

- [ ] **Step 8: Manual check in the real app**

Run `yarn tauri dev`. Fetch a schema, open Schema, then Builder. Tick fields under a root field that has a required argument, confirm the preview shows `query Name($id: ID!) { ... }` and a variables skeleton, press Use in request, and confirm the editor shows the query, the Variables pane shows the skeleton, and Send runs it. With a hand-written query in the editor, confirm the Replace confirmation appears.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add src/lib/graphql-query-builder.ts src/lib/__tests__/graphql-query-builder.test.ts \
  src/components/request/GraphQlQueryBuilder.tsx src/components/request/RequestPanel.tsx \
  src/components/request/__tests__/GraphQlQueryBuilder.test.tsx
```

Suggested subject: `feat(graphql): build queries from the schema field tree`.

---

## Next Plan

[Plan 08: WebSocket backend](2026-10-05-protocol-parity-plan-08-websocket-backend.md). It does not depend on this plan. Chain to it automatically when this one finishes.

GraphQL subscriptions are not part of Plans 05 to 07. The editor already lists subscription operations and shows a notice that they cannot be sent over HTTP. Subscriptions are delivered by the WebSocket author in Plan 10 (the `graphql-transport-ws` protocol over the WebSocket client). That plan needs these hooks from this plan set: `GraphQlRequest` and `RequestState.graphql` (Plan 05), `list_graphql_operations` to tell a subscription from a query (Plan 06), `build_wire` for the payload shape (Plan 06), and the schema builder's subscription root, which Plan 07 deliberately leaves disabled.
