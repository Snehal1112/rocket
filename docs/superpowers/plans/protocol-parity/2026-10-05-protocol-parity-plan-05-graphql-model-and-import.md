# Protocol parity, Plan 05: GraphQL model, persistence, Bruno import and sidebar

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** GraphQL requests become a first-class, typed collection item. They load, save, rename, appear in the sidebar, open in a tab, can be created from the New Request dialog, and are imported from Bruno `.bru` and OpenCollection `.yml` files. Sending and editing the query come in Plans 06 and 07.

**Architecture:** `rocket-collection` gains `GraphQlRequest`, `CollectionItem::GraphQl` and a `RequestKind` discriminator that also tags sidebar summaries. `CollectionRepository` gains three defaulted methods (`get_graphql_request`, `save_graphql_request`, `request_kind`) so the eleven test doubles keep compiling. `rocket-infra` converts `GraphQlRequest` to and from the existing `OcGraphQLRequest` YAML shape (backward-compatible optional fields only). `rocket-import` stops dropping GraphQL. The frontend routes load and save by `requestType`, which becomes the persisted discriminator.

**Tech Stack:** Rust (serde, serde_yaml), Tauri 2 IPC, React + TypeScript, Zustand, Vitest. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md`, sections 2.4 `GraphQLRequest` and `GraphQLBody`. There is no protocol-parity design spec; the locked decisions are in the Architecture lines of Plans 05 to 07.

## Global Constraints

- Never apply `#[serde(rename_all = "camelCase")]` to the `Oc*` persistence structs in `crates/rocket-infra/src/oc/`. The domain `GraphQlRequest` carries camelCase exactly like `Request` does, because domain JSON is the IPC JSON in this codebase (see `get_request` and `save_request` in `src-tauri/src/commands/collections.rs`).
- Every new persisted field is optional and skipped when empty, so an older build still reads the file. The OpenCollection schema is `additionalProperties: false`. The only keys this plan adds on disk are `uid` (top level) and `settings.verifySsl`, which are the same two deviations HTTP requests already have. Both are listed in `KNOWN_DEFERRED` in `schema_shape_tests.rs`.
- Never `unwrap()` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path only; peer sessions share this repo's index.
- Frontend: shadcn/ui primitives and `lucide-react` only. Zustand: narrow selectors only.
- A plan that adds a `CollectionItem` variant breaks every exhaustive `match`. The WebSocket and gRPC plans do the same for their variants. Whichever lands second resolves the textual merge in `folder.rs`, `repository.rs`, `tree.rs` and `schema_shape_tests.rs`; the logic does not collide.

## Review Focus

1. A GraphQL file with no top-level `uid` (every file written before this plan, and every Bruno import) must load with a generated in-memory uid and must not be rewritten by a read (Task 1 test `get_graphql_request_gives_a_uid_less_file_an_in_memory_uid`).
2. A GraphQL file with `body` as an array of variants must survive a load and save without losing the other variants (Task 1 tests `oc_graphql_variants_keep_every_variant_on_round_trip` and `save_graphql_request_keeps_unselected_variants`).
3. Renaming a GraphQL item must keep it a GraphQL file. Before this plan `rename_request` would try to parse it as HTTP and fail (Task 1 test `rename_request_keeps_a_graphql_item_graphql`).
4. A `.bru` file with `body:graphql` and `body:graphql:vars` must import as a GraphQL item with the query intact. Today `convert()` silently turns it into an empty-body HTTP POST (Task 2 test `bru_graphql_file_imports_as_graphql_item`).
5. Auto-save and the Save button must never write a GraphQL tab through `saveRequest`, because that would overwrite the GraphQL file with an HTTP one (Task 3 test `routes a graphql tab to saveGraphQlRequest`).

---

## Task 1: `GraphQlRequest`, `CollectionItem::GraphQl` and infra persistence

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-collection/src/request_kind.rs`
- Create: `crates/rocket-collection/src/graphql_request.rs`
- Modify: `crates/rocket-collection/src/lib.rs`, `folder.rs`, `request_summary.rs`, `repository.rs`
- Modify: `crates/rocket-infra/src/oc/graphql.rs`, `crates/rocket-infra/src/oc/auth.rs`
- Create: `crates/rocket-infra/src/conversions/graphql.rs`
- Modify: `crates/rocket-infra/src/conversions/mod.rs`, `request.rs`, `folder.rs`, `tests.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs`, `requests.rs`, `tree.rs`, `variables.rs`, `tests.rs`, `schema_shape_tests.rs`
- Modify: `crates/rocket-infra/src/shared_path_collection_repo.rs`
- Modify: `crates/rocket-app/src/runner_sequence.rs`, `contract_service.rs`, `collection_service.rs`
- Modify: `crates/rocket-collection/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md` (the "opaque" lines)

**Interfaces:**
- Produces:
  - `rocket_collection::RequestKind { Http (default), GraphQl, Grpc, WebSocket }`, serde lowercase (`"http"`, `"graphql"`, `"grpc"`, `"websocket"`), `is_http(&self) -> bool`.
  - `rocket_collection::GraphQlRequest`, `GraphQlBody { query: String, variables: Option<String> }`, `GraphQlBodyVariant { title, selected, body }`.
  - `CollectionItem::GraphQl(Box<GraphQlRequest>)`, serde tag `"graphql"`. Boxed for the same size reason `Request` is.
  - `RequestSummary.kind: RequestKind`, skipped when `Http`.
  - `CollectionRepository::get_graphql_request(&self, collection: &str, path: &str) -> DomainResult<GraphQlRequest>`, `save_graphql_request(&self, collection: &str, path: &str, request: &GraphQlRequest) -> DomainResult<String>`, `request_kind(&self, collection: &str, path: &str) -> DomainResult<RequestKind>`. All three have defaults.
  - `rocket_infra::conversions::{graphql_to_oc, oc_graphql_to_domain}` (crate-internal).
  - `CollectionService::{get_graphql_request, save_graphql_request}`.
- Consumes: `Request`, `CollectionVariable`, `rocket_shared::types::{Auth, Header, HttpMethod, PathParam, QueryParam, RequestSettings}`, `OcGraphQLRequest` and friends in `crates/rocket-infra/src/oc/graphql.rs`.

- [ ] **Step 1: Write the failing domain tests**

Create `crates/rocket-collection/src/request_kind.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_to_the_frontend_discriminator() {
        let json = |k: RequestKind| serde_json::to_string(&k).expect("serialize");
        assert_eq!(json(RequestKind::Http), "\"http\"");
        assert_eq!(json(RequestKind::GraphQl), "\"graphql\"");
        assert_eq!(json(RequestKind::Grpc), "\"grpc\"");
        assert_eq!(json(RequestKind::WebSocket), "\"websocket\"");
    }

    #[test]
    fn default_kind_is_http() {
        assert_eq!(RequestKind::default(), RequestKind::Http);
        assert!(RequestKind::Http.is_http());
        assert!(!RequestKind::GraphQl.is_http());
    }
}
```

Create `crates/rocket-collection/src/graphql_request.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_graphql_request_posts_by_default() {
        let g = GraphQlRequest::new("List Users", "https://api.example.com/graphql");
        assert_eq!(g.method, HttpMethod::Post);
        assert!(!g.uid.is_empty());
        assert_eq!(g.body, GraphQlBody::default());
        assert!(g.body_variants.is_empty());
    }

    #[test]
    fn json_is_camel_case_and_skips_empty_fields() {
        let g = GraphQlRequest::new("A", "https://x/graphql").with_query("{ a }");
        let v = serde_json::to_value(&g).expect("serialize");
        assert_eq!(v["body"]["query"], "{ a }");
        assert_eq!(v["method"], "POST");
        assert!(v.get("bodyVariants").is_none());
        assert!(v.get("preRequestScript").is_none());
    }

    #[test]
    fn json_without_optional_fields_still_loads() {
        let json = r#"{"uid":"u1","name":"A","method":"POST","url":"https://x","body":{"query":"{ a }"}}"#;
        let g: GraphQlRequest = serde_json::from_str(json).expect("old shape loads");
        assert_eq!(g.body.query, "{ a }");
        assert!(g.body.variables.is_none());
        assert!(g.headers.is_empty());
    }
}
```

Append to the `tests` module of `crates/rocket-collection/src/folder.rs`:

```rust
    #[test]
    fn graphql_items_count_as_requests_and_use_the_graphql_tag() {
        use crate::graphql_request::GraphQlRequest;
        let mut folder = Folder::new("api");
        folder.add_request(Request::new("A", HttpMethod::Get, "/a"));
        folder.items.push(CollectionItem::GraphQl(Box::new(GraphQlRequest::new(
            "Q",
            "https://x/graphql",
        ))));
        assert_eq!(folder.request_count(), 2);
        let v = serde_json::to_value(&folder.items[1]).expect("serialize");
        assert_eq!(v["type"], "graphql");
        assert_eq!(v["name"], "Q");
    }
```

Append to `crates/rocket-collection/src/request_summary.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_summary_json_without_kind_is_http() {
        let json = r#"{"uid":"u","name":"A","method":"GET","url":"/a"}"#;
        let s: RequestSummary = serde_json::from_str(json).expect("old shape");
        assert_eq!(s.kind, RequestKind::Http);
    }

    #[test]
    fn http_summary_omits_kind_and_graphql_summary_keeps_it() {
        let mut s = RequestSummary {
            uid: "u".into(),
            name: "A".into(),
            method: "GET".into(),
            url: "/a".into(),
            file_name: None,
            kind: RequestKind::Http,
        };
        assert!(serde_json::to_value(&s).expect("ser").get("kind").is_none());
        s.kind = RequestKind::GraphQl;
        assert_eq!(serde_json::to_value(&s).expect("ser")["kind"], "graphql");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-collection graphql`
Expected: FAIL to compile (`RequestKind`, `GraphQlRequest` not found).

- [ ] **Step 3: Implement the domain types**

Put this above the test module in `crates/rocket-collection/src/request_kind.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Which protocol a collection item speaks. The serialized form is the
/// frontend's `requestType` discriminator, so the two stay in step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestKind {
    #[default]
    Http,
    GraphQl,
    Grpc,
    WebSocket,
}

impl RequestKind {
    /// Used by serde to keep HTTP summaries byte-identical to older builds.
    pub fn is_http(&self) -> bool {
        matches!(self, RequestKind::Http)
    }
}
```

Put this above the test module in `crates/rocket-collection/src/graphql_request.rs`:

```rust
use rocket_shared::action::ActionSetVariable;
use rocket_shared::assertion::Assertion;
use rocket_shared::description::{Description, Documentation};
use rocket_shared::types::{Auth, Header, HttpMethod, PathParam, QueryParam, RequestSettings};
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// The query and the variables of a GraphQL request. `variables` is a JSON
/// string, not a parsed object, because the spec stores it that way and a user
/// may leave `{{placeholders}}` in it that are not valid JSON yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlBody {
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
}

/// One named body of a request that stores several (spec `GraphQLBodyVariant`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlBodyVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub body: GraphQlBody,
}

/// A saved GraphQL request. It mirrors `Request` where the fields mean the
/// same thing, so the headers, auth, scripts and settings panels work for both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlRequest {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    /// `POST` (JSON body) or `GET` (query string). Defaults to `POST`.
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<QueryParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_params: Vec<PathParam>,
    /// The body that is sent. When `body_variants` is not empty this is the
    /// selected variant's body.
    #[serde(default)]
    pub body: GraphQlBody,
    /// Every stored variant, so a save does not lose the unselected ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body_variants: Vec<GraphQlBodyVariant>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<Assertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionSetVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<Documentation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<CollectionVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<RequestSettings>,
}

impl GraphQlRequest {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            method: HttpMethod::Post,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            body: GraphQlBody::default(),
            body_variants: Vec::new(),
            auth: Auth::None,
            file_name: None,
            seq: None,
            tags: Vec::new(),
            description: None,
            pre_request_script: None,
            post_response_script: None,
            tests: None,
            assertions: Vec::new(),
            actions: Vec::new(),
            docs: None,
            variables: Vec::new(),
            runtime_auth: None,
            settings: None,
        }
    }

    /// Builder method: set the query text.
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.body.query = query.into();
        self
    }
}
```

In `crates/rocket-collection/src/request_summary.rs`, add `use crate::request_kind::RequestKind;` at the top and this field at the end of the struct:

```rust
    /// Which protocol the file holds. Absent in older JSON, which means HTTP.
    #[serde(default, skip_serializing_if = "RequestKind::is_http")]
    pub kind: RequestKind,
```

In `crates/rocket-collection/src/folder.rs`, add `use crate::graphql_request::GraphQlRequest;` at the top, add this variant to `CollectionItem` between `Folder` and `OpaqueItem`, and change the doc of `OpaqueItem` to say "Raw YAML for protocols that have no typed variant yet (gRPC, WebSocket)":

```rust
    /// A GraphQL request. Boxed for the same reason `Request` is.
    #[serde(rename = "graphql")]
    GraphQl(Box<GraphQlRequest>),
```

In `Folder::request_count`, add the arm `CollectionItem::GraphQl(_) => 1,`.

In `crates/rocket-collection/src/lib.rs` add `pub mod graphql_request;` and `pub mod request_kind;` in the module list and these re-exports:

```rust
pub use graphql_request::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
pub use request_kind::RequestKind;
```

In `crates/rocket-collection/src/repository.rs` add `use rocket_shared::error::DomainError;` (extend the existing `rocket_shared::error` import), `use crate::graphql_request::GraphQlRequest;` and `use crate::request_kind::RequestKind;`, then add these three defaulted methods to the trait, after `delete_request`:

```rust
    /// Read one GraphQL request file. Repositories without GraphQL support keep this default.
    fn get_graphql_request(&self, _collection: &str, _path: &str) -> DomainResult<GraphQlRequest> {
        Err(DomainError::InvalidInput(
            "this repository does not support GraphQL requests".into(),
        ))
    }

    /// Save a GraphQL request. Returns the actual filename written, like `save_request`.
    fn save_graphql_request(
        &self,
        _collection: &str,
        _path: &str,
        _request: &GraphQlRequest,
    ) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "this repository does not support GraphQL requests".into(),
        ))
    }

    /// Which protocol the request file at `path` holds. Repositories that only know HTTP keep this default.
    fn request_kind(&self, _collection: &str, _path: &str) -> DomainResult<RequestKind> {
        Ok(RequestKind::Http)
    }
```

Update the "Domain Model" tree in `crates/rocket-collection/CLAUDE.md`: add a `GraphQl — a saved GraphQL request` line and change the opaque line to "OpaqueProtocolItem — raw YAML passthrough for gRPC/WebSocket". Add `"graphql"` to the `CollectionItem serde tag` bullet.

- [ ] **Step 4: Run the domain tests to verify they pass**

Run: `cargo test -j4 -p rocket-collection`
Expected: PASS. The crate has no exhaustive matches outside `request_count`.

- [ ] **Step 5: Write the failing infra conversion tests**

Append to `crates/rocket-infra/src/conversions/tests.rs`:

```rust
#[test]
fn oc_graphql_single_body_round_trips() {
    use rocket_collection::GraphQlRequest;
    let yaml = r#"
uid: gql-1
info:
  name: List Users
  type: graphql
  seq: 3
graphql:
  method: POST
  url: https://api.example.com/graphql
  headers:
    - name: Accept
      value: application/json
  body:
    query: "{ users { id } }"
    variables: '{"first": 10}'
  auth:
    type: bearer
    token: t
runtime:
  scripts:
    - type: before-request
      code: console.log(1)
settings:
  timeout: 1000
  verifySsl: false
docs: GraphQL docs
"#;
    let oc: OcGraphQLRequest = serde_yaml::from_str(yaml).expect("parse");
    let g: GraphQlRequest = oc_graphql_to_domain(oc);
    assert_eq!(g.uid, "gql-1");
    assert_eq!(g.name, "List Users");
    assert_eq!(g.seq, Some(3));
    assert_eq!(g.method, rocket_shared::types::HttpMethod::Post);
    assert_eq!(g.body.query, "{ users { id } }");
    assert_eq!(g.body.variables.as_deref(), Some("{\"first\": 10}"));
    assert_eq!(g.pre_request_script.as_deref(), Some("console.log(1)"));
    assert!(g.body_variants.is_empty());
    assert!(matches!(g.auth, rocket_shared::types::Auth::Bearer { .. }));

    let back = graphql_to_oc(&g);
    assert_eq!(back.uid.as_deref(), Some("gql-1"));
    assert_eq!(back.info.request_type.as_deref(), Some("graphql"));
    assert_eq!(back.graphql.method.as_deref(), Some("POST"));
    match back.graphql.body {
        Some(OcGraphQLBodyOrVariants::Single(b)) => {
            assert_eq!(b.query, "{ users { id } }");
            assert_eq!(b.variables.as_deref(), Some("{\"first\": 10}"));
        }
        other => panic!("expected a single body, got {other:?}"),
    }
    assert_eq!(back.docs.as_deref(), Some("GraphQL docs"));
    let verify = back.settings.expect("settings").verify_ssl;
    assert!(matches!(verify, Some(InheritableBoolean::Value(false))));
}

#[test]
fn oc_graphql_variants_keep_every_variant_on_round_trip() {
    let yaml = r#"
info:
  name: Multi
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    - title: Users
      selected: false
      body:
        query: "{ users { id } }"
    - title: Orders
      selected: true
      body:
        query: "{ orders { id } }"
        variables: '{"n": 1}'
"#;
    let oc: OcGraphQLRequest = serde_yaml::from_str(yaml).expect("parse");
    let mut g = oc_graphql_to_domain(oc);
    assert_eq!(g.method, rocket_shared::types::HttpMethod::Post, "method defaults to POST");
    assert_eq!(g.body_variants.len(), 2);
    assert_eq!(g.body.query, "{ orders { id } }", "the selected variant is the active body");

    g.body.query = "{ orders { id total } }".into();
    let back = graphql_to_oc(&g);
    let Some(OcGraphQLBodyOrVariants::Variants(vs)) = back.graphql.body else {
        panic!("expected variants to be kept");
    };
    assert_eq!(vs.len(), 2);
    assert_eq!(vs[0].body.query, "{ users { id } }");
    assert!(!vs[0].selected);
    assert!(vs[1].selected);
    assert_eq!(vs[1].body.query, "{ orders { id total } }", "edits land in the selected variant");
}

#[test]
fn graphql_to_oc_drops_blank_variables_and_empty_uid() {
    let mut g = rocket_collection::GraphQlRequest::new("A", "https://x/graphql").with_query("{ a }");
    g.uid = String::new();
    g.body.variables = Some("   ".into());
    let oc = graphql_to_oc(&g);
    assert!(oc.uid.is_none(), "an empty uid is not written");
    let Some(OcGraphQLBodyOrVariants::Single(b)) = oc.graphql.body else {
        panic!("single body");
    };
    assert!(b.variables.is_none());
}
```

Also replace the last three assertions of `non_http_items_preserved_in_folder_roundtrip` (it currently expects an opaque GraphQL item) so they read:

```rust
    assert!(matches!(&folder.items[0], CollectionItem::Request(_)));
    assert!(matches!(&folder.items[1], CollectionItem::GraphQl(g) if g.name == "GQL Query"));

    let back = folder_to_oc_folder(folder);
```

(the lines after `let items = back.items.unwrap();` stay as they are).

- [ ] **Step 6: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra oc_graphql`
Expected: FAIL to compile (`oc_graphql_to_domain`, `uid`, `verify_ssl` not found).

- [ ] **Step 7: Implement the OC struct changes and the conversions**

In `crates/rocket-infra/src/oc/graphql.rs`, add `uid` as the first field of `OcGraphQLRequest`:

```rust
    /// Stable identity for tab deduplication across reloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
```

and add `Default` to the derive list of `OcGraphQLRequestRuntime`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OcGraphQLRequestRuntime {
```

In `crates/rocket-infra/src/oc/auth.rs`, add `verify_ssl` to `OcGraphQLRequestSettings` (same shape as the HTTP settings struct):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_ssl: Option<InheritableBoolean>,
```

In `crates/rocket-infra/src/conversions/request.rs`, add slice-based helpers that GraphQL shares, and make the HTTP code call them. Add:

```rust
/// Splits a `runtime.scripts` list into the before-request, after-response and tests scripts.
pub(super) fn scripts_from_oc(
    scripts: &[OcScript],
) -> (Option<String>, Option<String>, Option<String>) {
    let mut pre = None;
    let mut post = None;
    let mut tests = None;
    for script in scripts {
        match script.script_type.as_str() {
            "before-request" => pre = Some(script.code.clone()),
            "after-response" => post = Some(script.code.clone()),
            "tests" => tests = Some(script.code.clone()),
            _ => {}
        }
    }
    (pre, post, tests)
}

/// Builds the `runtime.scripts` list from the three domain scripts.
pub(super) fn scripts_to_oc(
    pre: &Option<String>,
    post: &Option<String>,
    tests: &Option<String>,
) -> Vec<OcScript> {
    let mut scripts = Vec::new();
    for (script_type, code) in [
        ("before-request", pre),
        ("after-response", post),
        ("tests", tests),
    ] {
        if let Some(code) = code {
            scripts.push(OcScript {
                script_type: script_type.into(),
                code: code.trim_end_matches('\n').to_string(),
            });
        }
    }
    scripts
}

/// Converts `runtime.actions` to domain actions.
pub(super) fn actions_from_oc(actions: &[OcAction]) -> Vec<ActionSetVariable> {
    actions
        .iter()
        .map(|a| match a {
            OcAction::SetVariable {
                description,
                phase,
                selector,
                variable,
                disabled,
            } => ActionSetVariable {
                phase: phase.clone(),
                selector: ActionSelector {
                    expression: selector.expression.clone(),
                    method: selector.method.clone(),
                },
                variable: ActionVariable {
                    name: variable.name.clone(),
                    scope: variable.scope.clone(),
                },
                disabled: *disabled,
                description: description.clone(),
            },
        })
        .collect()
}

/// Converts domain actions to `runtime.actions`.
pub(super) fn actions_to_oc(actions: &[ActionSetVariable]) -> Vec<OcAction> {
    actions
        .iter()
        .map(|a| OcAction::SetVariable {
            description: a.description.clone(),
            phase: a.phase.clone(),
            selector: OcActionSelector {
                expression: a.selector.expression.clone(),
                method: a.selector.method.clone(),
            },
            variable: OcActionVariable {
                name: a.variable.name.clone(),
                scope: a.variable.scope.clone(),
            },
            disabled: a.disabled,
        })
        .collect()
}
```

Replace the bodies of the existing private `extract_scripts` and `extract_actions` so they delegate:

```rust
fn extract_scripts(
    runtime: &Option<OcHttpRequestRuntime>,
) -> (Option<String>, Option<String>, Option<String>) {
    match runtime {
        Some(rt) => scripts_from_oc(&rt.scripts),
        None => (None, None, None),
    }
}

fn extract_actions(runtime: &Option<OcHttpRequestRuntime>) -> Vec<ActionSetVariable> {
    match runtime {
        Some(rt) => actions_from_oc(&rt.actions),
        None => Vec::new(),
    }
}
```

In `request_to_oc_http_request`, replace the inline `let mut scripts = Vec::new(); if let Some(ref code) = ... { ... }` block (the three `scripts.push` blocks) with `let scripts = scripts_to_oc(&req.pre_request_script, &req.post_response_script, &req.tests);` and replace the `let actions: Vec<OcAction> = req.actions.iter().map(...).collect();` block with `let actions = actions_to_oc(&req.actions);`. Run `cargo test -j4 -p rocket-infra conversions` after this edit alone; every existing conversion test must still pass before you go on.

Create `crates/rocket-infra/src/conversions/graphql.rs`:

```rust
//! Conversions between the domain `GraphQlRequest` and the OC GraphQL structs.

use crate::oc::*;
use rocket_collection::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
use rocket_shared::description::Documentation;
use rocket_shared::types::{Auth, Header, HttpMethod, RequestSettings};

use super::auth::persisted_oc_auth;
use super::param::{merge_params, split_params};
use super::request::{actions_from_oc, actions_to_oc, scripts_from_oc, scripts_to_oc};
use super::request_settings::{domain_settings_to_oc, oc_settings_to_domain};

fn body_from_oc(b: OcGraphQLBody) -> GraphQlBody {
    GraphQlBody {
        query: b.query,
        variables: b.variables,
    }
}

fn body_to_oc(b: &GraphQlBody) -> OcGraphQLBody {
    OcGraphQLBody {
        query: b.query.clone(),
        // A blank variables pane is the same as no variables.
        variables: b.variables.clone().filter(|v| !v.trim().is_empty()),
    }
}

fn settings_from_oc(s: OcGraphQLRequestSettings) -> RequestSettings {
    oc_settings_to_domain(OcHttpRequestSettings {
        encode_url: s.encode_url,
        timeout: s.timeout,
        follow_redirects: s.follow_redirects,
        max_redirects: s.max_redirects,
        verify_ssl: s.verify_ssl,
    })
}

fn settings_to_oc(s: RequestSettings) -> OcGraphQLRequestSettings {
    let h = domain_settings_to_oc(s);
    OcGraphQLRequestSettings {
        encode_url: h.encode_url,
        timeout: h.timeout,
        follow_redirects: h.follow_redirects,
        max_redirects: h.max_redirects,
        verify_ssl: h.verify_ssl,
    }
}

/// Convert an OC GraphQL request to the domain type.
pub fn oc_graphql_to_domain(oc: OcGraphQLRequest) -> GraphQlRequest {
    let method = oc
        .graphql
        .method
        .as_deref()
        .and_then(|m| m.parse::<HttpMethod>().ok())
        .unwrap_or(HttpMethod::Post);
    let (query_params, path_params) = split_params(oc.graphql.params);

    let (body, body_variants) = match oc.graphql.body {
        None => (GraphQlBody::default(), Vec::new()),
        Some(OcGraphQLBodyOrVariants::Single(b)) => (body_from_oc(b), Vec::new()),
        Some(OcGraphQLBodyOrVariants::Variants(vs)) => {
            let mut variants: Vec<GraphQlBodyVariant> = vs
                .into_iter()
                .map(|v| GraphQlBodyVariant {
                    title: v.title,
                    selected: v.selected,
                    body: body_from_oc(v.body),
                })
                .collect();
            // Exactly one variant is active. Fall back to the first when none is marked.
            if !variants.iter().any(|v| v.selected) {
                if let Some(first) = variants.first_mut() {
                    first.selected = true;
                }
            }
            let active = variants
                .iter()
                .find(|v| v.selected)
                .map(|v| v.body.clone())
                .unwrap_or_default();
            (active, variants)
        }
    };

    let (pre_request_script, post_response_script, tests) = match &oc.runtime {
        Some(rt) => scripts_from_oc(&rt.scripts),
        None => (None, None, None),
    };
    let assertions = oc
        .runtime
        .as_ref()
        .map(|r| r.assertions.clone())
        .unwrap_or_default();
    let actions = oc
        .runtime
        .as_ref()
        .map(|r| actions_from_oc(&r.actions))
        .unwrap_or_default();
    let variables = oc
        .runtime
        .as_ref()
        .map(|r| {
            r.variables
                .iter()
                .cloned()
                .map(rocket_collection::settings::CollectionVariable::from)
                .collect()
        })
        .unwrap_or_default();
    let runtime_auth = oc
        .runtime
        .as_ref()
        .and_then(|r| r.auth.clone())
        .map(Auth::from);

    GraphQlRequest {
        uid: oc.uid.unwrap_or_default(),
        name: oc.info.name,
        method,
        url: oc.graphql.url,
        headers: oc.graphql.headers.into_iter().map(Header::from).collect(),
        query_params,
        path_params,
        body,
        body_variants,
        auth: oc.graphql.auth.map(Auth::from).unwrap_or(Auth::None),
        file_name: None,
        seq: oc.info.seq,
        tags: oc.info.tags,
        description: oc.info.description,
        pre_request_script,
        post_response_script,
        tests,
        assertions,
        actions,
        docs: oc.docs.map(Documentation::text),
        variables,
        runtime_auth,
        settings: oc.settings.map(settings_from_oc),
    }
}

/// Convert a domain GraphQL request back to the OC struct.
pub fn graphql_to_oc(g: &GraphQlRequest) -> OcGraphQLRequest {
    let body = if g.body_variants.is_empty() {
        OcGraphQLBodyOrVariants::Single(body_to_oc(&g.body))
    } else {
        OcGraphQLBodyOrVariants::Variants(
            g.body_variants
                .iter()
                .map(|v| OcGraphQLBodyVariant {
                    title: v.title.clone(),
                    selected: v.selected,
                    // The selected variant carries the live edits.
                    body: if v.selected {
                        body_to_oc(&g.body)
                    } else {
                        body_to_oc(&v.body)
                    },
                })
                .collect(),
        )
    };

    let scripts = scripts_to_oc(&g.pre_request_script, &g.post_response_script, &g.tests);
    let actions = actions_to_oc(&g.actions);
    let runtime_auth = g.runtime_auth.clone().map(OcAuth::from);
    let has_runtime = !scripts.is_empty()
        || !g.assertions.is_empty()
        || !actions.is_empty()
        || !g.variables.is_empty()
        || runtime_auth.is_some();
    let runtime = if has_runtime {
        Some(OcGraphQLRequestRuntime {
            variables: g.variables.iter().cloned().map(OcVariable::from).collect(),
            scripts,
            assertions: g.assertions.clone(),
            actions,
            auth: runtime_auth,
        })
    } else {
        None
    };

    OcGraphQLRequest {
        uid: if g.uid.is_empty() {
            None
        } else {
            Some(g.uid.clone())
        },
        info: OcGraphQLRequestInfo {
            name: g.name.clone(),
            description: g.description.clone(),
            request_type: Some("graphql".into()),
            seq: g.seq,
            tags: g.tags.clone(),
        },
        graphql: OcGraphQLRequestDetails {
            method: Some(g.method.to_string()),
            url: g.url.clone(),
            headers: g
                .headers
                .iter()
                .cloned()
                .map(OcHttpRequestHeader::from)
                .collect(),
            params: merge_params(&g.query_params, &g.path_params),
            body: Some(body),
            auth: persisted_oc_auth(g.auth.clone()),
        },
        runtime,
        settings: g.settings.clone().map(settings_to_oc),
        docs: g
            .docs
            .as_ref()
            .and_then(|d| d.content().map(String::from)),
    }
}
```

In `crates/rocket-infra/src/conversions/mod.rs` add `mod graphql;` (after `mod folder;`) and:

```rust
pub use graphql::{graphql_to_oc, oc_graphql_to_domain};
```

In `crates/rocket-infra/src/conversions/folder.rs`:
- add `use super::graphql::{graphql_to_oc, oc_graphql_to_domain};`
- change the doc comment of `oc_item_to_collection_item` to say "GraphQL becomes a typed `GraphQl` item. gRPC and WebSocket items become `OpaqueItem`s ..."
- replace the `OcItem::GraphQL(gql)` arm with:

```rust
        OcItem::GraphQL(gql) => Some(CollectionItem::GraphQl(Box::new(oc_graphql_to_domain(gql)))),
```

- in both `folder_to_oc_folder` and `collection_to_oc_collection`, add this arm after the `CollectionItem::Summary(_) => None,` arm:

```rust
            CollectionItem::GraphQl(g) => Some(OcItem::GraphQL(graphql_to_oc(&g))),
```

- [ ] **Step 8: Run the conversion tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra conversions`
Expected: PASS for the three new tests and every older conversion test. (`cargo check -j4 -p rocket-infra` will still fail in `fs_collection` and `rocket-app` until Steps 9 to 12; run only the targeted test filter above. If the crate does not compile far enough to run tests, do Steps 9 and 10 first and come back.)

- [ ] **Step 9: Write the failing repository tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
fn gql_fixture(uid_line: &str) -> String {
    format!(
        "{uid_line}info:\n  name: List Users\n  type: graphql\ngraphql:\n  method: POST\n  url: https://api.example.com/graphql\n  body:\n    query: '{{ users {{ id }} }}'\n"
    )
}

#[test]
fn graphql_request_round_trips_through_the_repo() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut g = rocket_collection::GraphQlRequest::new(
        "List Users",
        "https://api.example.com/graphql",
    )
    .with_query("query Users { users { id } }");
    g.body.variables = Some("{\"first\": 5}".into());
    g.headers.push(rocket_shared::types::Header::new("X-Trace", "1"));

    let saved = repo.save_graphql_request("my-api", "list-users.yml", &g).unwrap();
    assert_eq!(saved, "list-users.yml");

    let back = repo.get_graphql_request("my-api", "list-users.yml").unwrap();
    assert_eq!(back.uid, g.uid);
    assert_eq!(back.body, g.body);
    assert_eq!(back.method, HttpMethod::Post);
    assert_eq!(back.headers.len(), 1);
    assert_eq!(back.file_name.as_deref(), Some("list-users.yml"));

    let yaml = fs::read_to_string(dir.path().join("my-api/list-users.yml")).unwrap();
    assert!(yaml.contains("type: graphql"), "{yaml}");
    assert!(yaml.contains("graphql:"), "{yaml}");
    assert!(!yaml.contains("http:"), "{yaml}");
}

#[test]
fn get_graphql_request_gives_a_uid_less_file_an_in_memory_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let path = dir.path().join("my-api/q.yml");
    fs::write(&path, gql_fixture("")).unwrap();

    let g = repo.get_graphql_request("my-api", "q.yml").unwrap();
    assert!(!g.uid.is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), gql_fixture(""), "a read must not rewrite the file");
}

#[test]
fn save_graphql_request_rejects_an_empty_uid() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut g = rocket_collection::GraphQlRequest::new("A", "https://x/graphql");
    g.uid = String::new();
    assert!(repo.save_graphql_request("my-api", "a.yml", &g).is_err());
}

#[test]
fn save_graphql_request_keeps_unselected_variants_and_stored_variables() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/multi.yml"),
        "uid: g1\ninfo:\n  name: Multi\n  type: graphql\ngraphql:\n  url: https://x/graphql\n  body:\n  - title: A\n    selected: true\n    body:\n      query: '{ a }'\n  - title: B\n    body:\n      query: '{ b }'\nruntime:\n  variables:\n  - name: tenant\n    value: acme\n",
    )
    .unwrap();

    let mut g = repo.get_graphql_request("my-api", "multi.yml").unwrap();
    // The IPC payload carries no request variables, so a save must not erase them.
    g.variables.clear();
    g.body.query = "{ a id }".into();
    repo.save_graphql_request("my-api", "multi.yml", &g).unwrap();

    let yaml = fs::read_to_string(dir.path().join("my-api/multi.yml")).unwrap();
    assert!(yaml.contains("title: B"), "{yaml}");
    assert!(yaml.contains("{ b }"), "{yaml}");
    assert!(yaml.contains("{ a id }"), "{yaml}");
    assert!(yaml.contains("name: tenant"), "{yaml}");
}

#[test]
fn request_kind_reads_the_protocol_key() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/q.yml"), gql_fixture("")).unwrap();
    fs::write(dir.path().join("my-api/g.yml"), GRPC_ITEM_YML).unwrap();
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req).unwrap();

    use rocket_collection::RequestKind;
    assert_eq!(repo.request_kind("my-api", "q.yml").unwrap(), RequestKind::GraphQl);
    assert_eq!(repo.request_kind("my-api", "g.yml").unwrap(), RequestKind::Grpc);
    assert_eq!(repo.request_kind("my-api", "good.yml").unwrap(), RequestKind::Http);
    assert!(repo.request_kind("my-api", "missing.yml").is_err());
}

#[test]
fn request_variables_work_for_a_graphql_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/q.yml"), gql_fixture("uid: g1\n")).unwrap();

    let vars = vec![CollectionVariable {
        key: "tenant".into(),
        value: "acme".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    repo.save_request_variables("my-api", "q.yml", vars).unwrap();
    let back = repo.get_request_variables("my-api", "q.yml").unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].key, "tenant");

    // The save must not turn the file into an HTTP request.
    let g = repo.get_graphql_request("my-api", "q.yml").unwrap();
    assert_eq!(g.body.query, "{ users { id } }");
}

#[test]
fn full_tree_loads_graphql_as_a_typed_item_with_its_file_name() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/list-users.yml"), gql_fixture("uid: g1\n")).unwrap();

    let col = repo.get("my-api").unwrap();
    let found = col.root.items.iter().find_map(|i| match i {
        rocket_collection::CollectionItem::GraphQl(g) => Some(g),
        _ => None,
    });
    let g = found.expect("a typed GraphQl item");
    assert_eq!(g.name, "List Users");
    assert_eq!(g.uid, "g1");
    assert_eq!(g.file_name.as_deref(), Some("list-users.yml"));
}

#[test]
fn get_summaries_returns_a_graphql_summary_with_its_kind() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/list-users.yml"), gql_fixture("uid: g1\n")).unwrap();
    fs::write(dir.path().join("my-api/get-user.yml"), GRPC_ITEM_YML).unwrap();

    let col = repo.get_summaries("my-api").unwrap();
    assert_eq!(col.root.items.len(), 1, "gRPC is still skipped: {:?}", col.root.items);
    match &col.root.items[0] {
        rocket_collection::CollectionItem::Summary(s) => {
            assert_eq!(s.kind, rocket_collection::RequestKind::GraphQl);
            assert_eq!(s.uid, "g1");
            assert_eq!(s.method, "POST");
            assert_eq!(s.url, "https://api.example.com/graphql");
            assert_eq!(s.file_name.as_deref(), Some("list-users.yml"));
        }
        other => panic!("expected a summary, got {other:?}"),
    }
}
```

Update three existing tests in the same file, because GraphQL is no longer opaque.

Replace the body of `build_folder_tree_loads_non_http_items_as_opaque` from the line `let col = repo.get("my-api").unwrap();` through the first `assert_eq!(root, ...)` with:

```rust
    let col = repo.get("my-api").unwrap();
    let mut root: Vec<(&str, &str)> = opaque_items(&col.root)
        .iter()
        .map(|o| (o.protocol.as_str(), o.name.as_str()))
        .collect();
    root.sort();
    // GraphQL is typed now (see full_tree_loads_graphql_as_a_typed_item_with_its_file_name).
    assert_eq!(root, vec![("grpc", "Get User")]);
    assert!(col.root.items.iter().any(
        |i| matches!(i, rocket_collection::CollectionItem::GraphQl(g) if g.name == "List Users")
    ));
```

Rename `get_summaries_skips_non_http_items_without_error` to `get_summaries_skips_grpc_items_without_error` and change its `GRAPHQL_ITEM_YML` write to `GRPC_ITEM_YML`, keeping the file name `list-users.yml` and the assertions.

In the `reorder_items_writes_order_file_and_get_respects_it` test, add the arm `CollectionItem::GraphQl(g) => g.name.as_str(),` to `item_name`.

- [ ] **Step 10: Run the repository tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra graphql_request_round_trips_through_the_repo`
Expected: FAIL to compile (`get_graphql_request` is the defaulted trait method, but `FsCollectionRepo` does not override it, so tests fail at runtime once the crate compiles; the match arms in `tree.rs` and `item_name` do not compile yet).

- [ ] **Step 11: Implement the repository changes**

In `crates/rocket-infra/src/fs_collection/requests.rs`, extend the imports:

```rust
use rocket_collection::{
    generate_uid, request_filename_for, Collection, GraphQlRequest, Request, RequestKind,
};
use crate::conversions::{
    graphql_to_oc, oc_graphql_to_domain, oc_http_request_to_request, request_to_oc_http_request,
};
use crate::oc::{OcGraphQLRequest, OcHttpRequest};
```

(replace the existing `rocket_collection`, `conversions` and `oc` import lines). Append:

```rust
pub(super) fn get_graphql_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<GraphQlRequest> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    let content = fs::read_to_string(&file_path)?;
    let oc: OcGraphQLRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse GraphQL request: {e}")))?;
    let mut request = oc_graphql_to_domain(oc);
    request.file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    // A uid-less file gets an in-memory uid only; the next save persists it.
    if request.uid.is_empty() {
        request.uid = generate_uid();
    }
    Ok(request)
}

#[tracing::instrument(name = "collection_save_graphql_request", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_graphql_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    request: &GraphQlRequest,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_graphql_request: empty uid on request for '{path}' in collection '{collection}'; callers must construct via GraphQlRequest::new()"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let normalized = request_filename_for(path);
    let file_path = repo.validate_path(&collection_dir, Path::new(&normalized))?;

    let mut oc = graphql_to_oc(request);

    // Request variables are saved on their own path, so an empty list in the
    // payload must keep what is on disk.
    if request.variables.is_empty() && file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&file_path) {
            if let Ok(existing) = serde_yaml::from_str::<OcGraphQLRequest>(&existing_content) {
                if let Some(existing_runtime) = existing.runtime {
                    if !existing_runtime.variables.is_empty() {
                        let runtime = oc.runtime.get_or_insert_with(Default::default);
                        runtime.variables = existing_runtime.variables;
                    }
                }
            }
        }
    }

    let yaml = serde_yaml::to_string(&oc).map_err(|e| {
        DomainError::Internal(format!("Failed to serialize GraphQL request YAML: {e}"))
    })?;
    atomic_write(&file_path, yaml.as_bytes())?;

    let actual = file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();
    Ok(actual)
}

/// Reads which protocol the request file holds from its protocol key.
pub(super) fn request_kind(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<RequestKind> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    // Legacy JSON requests are always HTTP.
    if file_path.extension().is_some_and(|e| e == "json") {
        return Ok(RequestKind::Http);
    }
    let content = fs::read_to_string(&file_path)?;
    let value: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;
    Ok(if value.get("graphql").is_some() {
        RequestKind::GraphQl
    } else if value.get("grpc").is_some() {
        RequestKind::Grpc
    } else if value.get("websocket").is_some() {
        RequestKind::WebSocket
    } else {
        RequestKind::Http
    })
}
```

In `crates/rocket-infra/src/fs_collection/mod.rs`, extend the `rocket_collection` import with `GraphQlRequest, RequestKind` and add to the `impl CollectionRepository for FsCollectionRepo` block, after `delete_request`:

```rust
    fn get_graphql_request(&self, collection: &str, path: &str) -> DomainResult<GraphQlRequest> {
        requests::get_graphql_request(self, collection, path)
    }

    fn save_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<String> {
        requests::save_graphql_request(self, collection, path, request)
    }

    fn request_kind(&self, collection: &str, path: &str) -> DomainResult<RequestKind> {
        requests::request_kind(self, collection, path)
    }
```

In `crates/rocket-infra/src/shared_path_collection_repo.rs`, extend its `rocket_collection` import the same way and add the same three methods, each delegating to `self.repo()`:

```rust
    fn get_graphql_request(&self, collection: &str, path: &str) -> DomainResult<GraphQlRequest> {
        self.repo().get_graphql_request(collection, path)
    }

    fn save_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<String> {
        self.repo().save_graphql_request(collection, path, request)
    }

    fn request_kind(&self, collection: &str, path: &str) -> DomainResult<RequestKind> {
        self.repo().request_kind(collection, path)
    }
```

In `crates/rocket-infra/src/fs_collection/tree.rs`:
- add `RequestKind` to the `rocket_collection` import and `OcItem` is already imported;
- in `build_folder_tree`, add a match arm before `Ok(other) => Ok(other),`:

```rust
            Ok(Some(CollectionItem::GraphQl(mut gql))) => {
                gql.file_name = Some(entry_name.to_string());
                Ok(Some(CollectionItem::GraphQl(gql)))
            }
```

- in `load_request_summary`, add `kind: RequestKind::Http,` to both `RequestSummary { ... }` literals (the `MinReq` one and the legacy JSON one), and replace the final `match serde_yaml::from_str::<OcItem>(&content)` with:

```rust
        match serde_yaml::from_str::<OcItem>(&content) {
            Ok(OcItem::GraphQL(gql)) => Ok(Some(RequestSummary {
                uid: gql.uid.unwrap_or_default(),
                name: gql.info.name,
                method: gql.graphql.method.unwrap_or_else(|| "POST".to_string()),
                url: gql.graphql.url,
                file_name: Some(entry_name.to_string()),
                kind: RequestKind::GraphQl,
            })),
            Ok(OcItem::Http(_)) | Ok(OcItem::Folder(_)) | Err(_) => Err(DomainError::Internal(
                format!("Failed to parse request summary: {min_err}"),
            )),
            Ok(OcItem::Grpc(_) | OcItem::WebSocket(_) | OcItem::ScriptFile(_)) => Ok(None),
        }
```

- update the doc comment of `load_request_summary`: GraphQL files return a summary with `kind: GraphQl`; gRPC, WebSocket and script files return `Ok(None)`.

In `crates/rocket-infra/src/fs_collection/variables.rs`, extend the `crate::oc` import with `OcGraphQLRequest, OcGraphQLRequestRuntime` and replace `get_request_variables` and `save_request_variables` bodies' parsing with these helpers (add them at the bottom of the file):

```rust
/// Reads `runtime.variables` from an HTTP or GraphQL request file.
fn runtime_variables_of(content: &str) -> DomainResult<Vec<OcVariable>> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => return Ok(req.runtime.map(|r| r.variables).unwrap_or_default()),
        Err(e) => e,
    };
    match serde_yaml::from_str::<OcGraphQLRequest>(content) {
        Ok(g) => Ok(g.runtime.map(|r| r.variables).unwrap_or_default()),
        // Keep the HTTP error: it is the precise one for a broken HTTP file.
        Err(_) => Err(DomainError::Internal(format!(
            "Failed to parse request file: {http_err}"
        ))),
    }
}

/// Returns the file content with `runtime.variables` replaced, for an HTTP or GraphQL request file.
fn with_runtime_variables(content: &str, vars: Vec<OcVariable>) -> DomainResult<String> {
    let to_err = |e: serde_yaml::Error| {
        DomainError::Internal(format!("Failed to serialize request file: {e}"))
    };
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(mut req) => {
            let runtime = req.runtime.take().unwrap_or_default();
            req.runtime = Some(OcHttpRequestRuntime {
                variables: vars,
                ..runtime
            });
            return serde_yaml::to_string(&req).map_err(to_err);
        }
        Err(e) => e,
    };
    match serde_yaml::from_str::<OcGraphQLRequest>(content) {
        Ok(mut g) => {
            let runtime = g.runtime.take().unwrap_or_default();
            g.runtime = Some(OcGraphQLRequestRuntime {
                variables: vars,
                ..runtime
            });
            serde_yaml::to_string(&g).map_err(to_err)
        }
        Err(_) => Err(DomainError::Internal(format!(
            "Failed to parse request file: {http_err}"
        ))),
    }
}
```

`get_request_variables` becomes: read the file, `let vars = runtime_variables_of(&content)?.into_iter().map(CollectionVariable::from).collect(); Ok(vars)`. `save_request_variables` becomes: read the file, `let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect(); let yaml = with_runtime_variables(&content, oc_vars)?; atomic_write(&file_path, yaml.as_bytes())?; Ok(())`.

- [ ] **Step 12: Update the schema-shape guard**

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`:

Add to `KNOWN_DEFERRED` (with the others, same rule as the HTTP entries):

```rust
    "GraphQLRequest.uid",
    "GraphQLRequestSettings.verifySsl",
```

Replace the loop over `users.items` (the `for item in &users.items { if let CollectionItem::OpaqueItem(o) = item { ... } }` block) so it checks the typed GraphQL item through the same conversion the repo uses:

```rust
    for item in &users.items {
        match item {
            CollectionItem::GraphQl(g) => {
                protocols.push("graphql".to_string());
                let raw = serde_yaml::to_value(crate::conversions::graphql_to_oc(g))
                    .expect("serialize GraphQL request");
                check_graphql_request(&mut v, &g.name, &raw);
            }
            CollectionItem::OpaqueItem(o) => {
                protocols.push(o.protocol.clone());
                match o.protocol.as_str() {
                    "websocket" => check_websocket_request(&mut v, &o.name, &o.raw),
                    other => panic!("unexpected protocol {other}"),
                }
            }
            _ => {}
        }
    }
```

Add a dedicated test that writes through the repo and checks the file on disk:

```rust
#[test]
fn saved_graphql_request_only_uses_schema_keys_besides_deferred() {
    let (dir, repo) = setup();
    repo.create("api").unwrap();
    let mut g = rocket_collection::GraphQlRequest::new("Q", "https://x/graphql").with_query("{ a }");
    g.body.variables = Some("{}".into());
    g.settings = Some(RequestSettings {
        encode_url: None,
        timeout: Some(RequestSettingValue::Value(3000.0)),
        follow_redirects: None,
        max_redirects: None,
        verify_ssl: Some(RequestSettingValue::Value(false)),
    });
    g.pre_request_script = Some("console.log(1)".into());
    let rel = repo.save_graphql_request("api", "q.yml", &g).unwrap();

    let mut v = Violations::default();
    check_graphql_request(&mut v, &rel, &read_yaml(&dir.path().join("api").join(&rel)));
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}
```

If `read_yaml` has a different signature in this file, adapt the call to match the existing calls (`read_yaml(&col_dir.join(rel))`).

- [ ] **Step 13: Fix the exhaustive matches in `rocket-app` and add the service methods**

In `crates/rocket-app/src/runner_sequence.rs`, in `collect_items`, change the skip arm and its comment:

```rust
            // GraphQL becomes a run step in Plan 06; until then it, the other
            // protocols and sidebar summaries are not executable.
            CollectionItem::GraphQl(_) | CollectionItem::OpaqueItem(_) | CollectionItem::Summary(_) => {}
```

In `crates/rocket-app/src/contract_service.rs`, in `walk_folder`, add before `CollectionItem::OpaqueItem(_) => {}`:

```rust
            // Contracts describe HTTP request signatures only.
            CollectionItem::GraphQl(_) => {}
```

In `crates/rocket-app/src/collection_service.rs`, extend the `rocket_collection` import with `GraphQlRequest, RequestKind`, and add after `get_request`:

```rust
    /// Get the full GraphQL request at `path`.
    pub fn get_graphql_request(
        &self,
        collection: &str,
        path: &str,
    ) -> DomainResult<GraphQlRequest> {
        self.repo.get_graphql_request(collection, path)
    }

    /// Save a GraphQL request and return it as stored (the file name may differ from `path`).
    pub fn save_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<GraphQlRequest> {
        let actual_path = self.repo.save_graphql_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_graphql_request(collection, &actual_path)
    }
```

At the top of `rename_request` (before `let mut request = self.repo.get_request(...)`), add:

```rust
        if self.repo.request_kind(collection, old_path)? == RequestKind::GraphQl {
            let mut request = self.repo.get_graphql_request(collection, old_path)?;
            request.name = new_name.to_string();
            let actual_path = self.repo.save_graphql_request(collection, old_path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
```

At the top of `update_request_docs` add the same pattern, setting `request.docs = docs.map(Documentation::text);` on the GraphQL request:

```rust
        if self.repo.request_kind(collection, path)? == RequestKind::GraphQl {
            let mut request = self.repo.get_graphql_request(collection, path)?;
            request.docs = docs.map(Documentation::text);
            let actual_path = self.repo.save_graphql_request(collection, path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
```

Add this test to the `tests` module of `collection_service.rs`:

```rust
    #[test]
    fn rename_request_keeps_a_graphql_item_graphql() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let g = GraphQlRequest::new("Old", "https://x/graphql").with_query("{ a }");
        repo.save_graphql_request("api", "q.yml", &g).expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.rename_request("api", "q.yml", "New").expect("rename");

        let back = svc.get_graphql_request("api", "q.yml").expect("get");
        assert_eq!(back.name, "New");
        assert_eq!(back.body.query, "{ a }");
    }
```

Update the last paragraph of `crates/rocket-infra/CLAUDE.md` "Internal modules" so it says the repo round-trips `OcHttpRequest` and `OcGraphQLRequest`, and only gRPC and WebSocket land as `OpaqueProtocolItem`.

- [ ] **Step 14: Run the checks**

Run:
- `cargo check -j4 -p rocket-collection -p rocket-infra -p rocket-app`
- `cargo test -j4 -p rocket-collection`
- `cargo test -j4 -p rocket-infra conversions`
- `cargo test -j4 -p rocket-infra fs_collection`
- `cargo test -j4 -p rocket-app collection_service`
- `cargo test -j4 -p rocket-app runner_sequence`

Expected: PASS everywhere. `runner_sequence::opaque_protocol_items_are_never_steps` still passes because it builds an `OpaqueItem` directly. If `cargo check` reports a non-exhaustive match anywhere else, add a `GraphQl` arm that skips it and mention the file in the commit body.

- [ ] **Step 15: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-collection crates/rocket-infra/src crates/rocket-infra/CLAUDE.md \
  crates/rocket-app/src/runner_sequence.rs crates/rocket-app/src/contract_service.rs \
  crates/rocket-app/src/collection_service.rs
```

Suggested subject: `feat(collection): add a typed GraphQL request item and persistence`.

---

## Task 2: Bruno GraphQL import

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/bru/ast.rs`, `bru/parser.rs`, `bru/yml_adapter.rs`
- Modify: `crates/rocket-import/src/converter/request.rs`, `importer.rs`
- Modify: `crates/rocket-import/tests/integration_test.rs`
- Modify: `crates/rocket-import/CLAUDE.md`

**Interfaces:**
- Consumes: `rocket_collection::{GraphQlRequest, GraphQlBody}`, `CollectionRepository::save_graphql_request` (Task 1), `BruDocument`.
- Produces:
  - `BruDocument.graphql: Option<BruGraphQl>` with `BruGraphQl { query: String, variables: Option<String> }`.
  - `converter::request::Converted { Http(Request), GraphQl(GraphQlRequest) }` and `converter::request::convert_item(&BruDocument) -> (Option<Converted>, Vec<SkipReason>)`.
  - `converter::request::convert_graphql(&BruDocument) -> (Option<GraphQlRequest>, Vec<SkipReason>)`.

Format facts this task relies on:
- A Bruno `.bru` GraphQL file has `meta { type: graphql }`, a `post { url: ..., body: graphql }` block, a `body:graphql { <query> }` block and an optional `body:graphql:vars { <json> }` block. The lexer splits the header on the first `:` only, so the subtypes arrive as `graphql` and `graphql:vars`.
- A `.yml` GraphQL file in the OpenCollection shape (spec 2.4) has `info.type: graphql` and a `graphql:` block with `method`, `url`, `headers` and `body` (a mapping `{query, variables}` or a list of variants). This is the only YAML GraphQL shape the plan supports, because it is the one the spec defines. The importer's existing `meta:` + `http:` YAML shape has no documented GraphQL form.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** (Sections 2.4 and `GraphQLBody`.)

- [ ] **Step 2: Write the failing parser and adapter tests**

Append to the `tests` module of `crates/rocket-import/src/bru/parser.rs`:

```rust
    #[test]
    fn parses_graphql_body_and_vars_blocks() {
        let doc = parse(
            "meta {\n  name: Users\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://api.example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query Users($n: Int) {\n    users(first: $n) { id }\n  }\n}\n\nbody:graphql:vars {\n  {\n    \"n\": 5\n  }\n}\n",
        );
        let gql = doc.graphql.expect("graphql body parsed");
        assert!(gql.query.contains("users(first: $n)"), "{}", gql.query);
        assert!(gql.variables.as_deref().expect("vars").contains("\"n\": 5"));
        assert!(doc.body.is_none(), "a GraphQL body is not an HTTP body");
        assert!(
            doc.unknown_blocks.iter().all(|b| b.name != "body"),
            "{:?}",
            doc.unknown_blocks
        );
        assert_eq!(doc.method, Some(BruMethod::Post));
    }
```

Append to the `tests` module of `crates/rocket-import/src/bru/yml_adapter.rs`, replacing the existing `graphql_request_type_lands_in_unknown_blocks` test:

```rust
    #[test]
    fn opencollection_graphql_request_is_adapted() {
        let yml = r#"
info:
  name: GQL Query
  type: graphql
  seq: 4
graphql:
  method: POST
  url: https://api.example.com/graphql
  headers:
    - name: Accept
      value: application/json
  body:
    query: "{ users { id } }"
    variables: '{"first": 2}'
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert!(doc.unknown_blocks.is_empty(), "{:?}", doc.unknown_blocks);
        let meta = doc.meta.as_ref().expect("meta");
        assert_eq!(meta.name, "GQL Query");
        assert_eq!(meta.request_type, "graphql");
        assert_eq!(meta.seq, Some(4));
        assert_eq!(doc.method, Some(BruMethod::Post));
        assert_eq!(doc.url.as_deref(), Some("https://api.example.com/graphql"));
        assert_eq!(doc.headers.len(), 1);
        let gql = doc.graphql.expect("graphql");
        assert_eq!(gql.query, "{ users { id } }");
        assert_eq!(gql.variables.as_deref(), Some("{\"first\": 2}"));
    }

    #[test]
    fn opencollection_graphql_variants_use_the_selected_body() {
        let yml = r#"
info:
  name: Multi
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    - title: A
      body:
        query: "{ a }"
    - title: B
      selected: true
      body:
        query: "{ b }"
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.graphql.expect("graphql").query, "{ b }");
    }

    #[test]
    fn opencollection_graphql_auth_is_reported_not_silently_dropped() {
        let yml = r#"
info:
  name: Authed
  type: graphql
graphql:
  url: https://api.example.com/graphql
  body:
    query: "{ a }"
  auth:
    type: oauth2
"#;
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "auth");
        assert_eq!(doc.unknown_blocks[0].subtype.as_deref(), Some("oauth2"));
    }

    #[test]
    fn grpc_request_type_still_lands_in_unknown_blocks() {
        let yml = "meta:\n  name: G\n  type: grpc\nhttp:\n  method: POST\n  url: grpc://x\n";
        let doc = bru_document_from_yml_str(yml).unwrap();
        assert_eq!(doc.unknown_blocks.len(), 1);
        assert_eq!(doc.unknown_blocks[0].name, "unsupported_type");
    }
```

Append to the `tests` module of `crates/rocket-import/src/converter/request.rs`:

```rust
    fn graphql_doc() -> BruDocument {
        BruDocument {
            meta: Some(BruMeta {
                name: "Users".into(),
                request_type: "graphql".into(),
                seq: Some(2),
            }),
            method: Some(BruMethod::Post),
            url: Some("{{baseUrl}}/graphql".into()),
            headers: vec![BruKeyValue {
                key: "Accept".into(),
                value: "application/json".into(),
                disabled: false,
            }],
            graphql: Some(BruGraphQl {
                query: "{ users { id } }".into(),
                variables: Some("{\"n\": 1}".into()),
            }),
            pre_request_script: Some("// pre".into()),
            ..BruDocument::default()
        }
    }

    #[test]
    fn graphql_document_converts_to_a_graphql_request() {
        let (g, skipped) = convert_graphql(&graphql_doc());
        assert!(skipped.is_empty());
        let g = g.expect("graphql request");
        assert_eq!(g.name, "Users");
        assert_eq!(g.seq, Some(2));
        assert_eq!(g.method, HttpMethod::Post);
        assert_eq!(g.url, "{{baseUrl}}/graphql");
        assert_eq!(g.body.query, "{ users { id } }");
        assert_eq!(g.body.variables.as_deref(), Some("{\"n\": 1}"));
        assert_eq!(g.headers.len(), 1);
        assert_eq!(g.pre_request_script.as_deref(), Some("// pre"));
    }

    #[test]
    fn convert_item_routes_by_request_type() {
        let (item, _) = convert_item(&graphql_doc());
        assert!(matches!(item, Some(Converted::GraphQl(_))));
        let (item, _) = convert_item(&doc_with_method(BruMethod::Get, "https://example.com"));
        assert!(matches!(item, Some(Converted::Http(_))));
    }

    #[test]
    fn graphql_document_with_unsupported_auth_still_imports_and_reports() {
        let mut doc = graphql_doc();
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some("oauth2".into()),
            content: String::new(),
        });
        let (g, skipped) = convert_graphql(&doc);
        assert!(g.is_some());
        assert!(matches!(skipped[0], SkipReason::UnsupportedAuthType(_)));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-import graphql`
Expected: FAIL to compile (`BruGraphQl`, `doc.graphql`, `convert_graphql` not found).

- [ ] **Step 4: Implement the AST, parser and adapter changes**

In `crates/rocket-import/src/bru/ast.rs`, add to `BruDocument` (after `post_response_script`):

```rust
    /// The query and variables of a GraphQL request (`body:graphql` and `body:graphql:vars`).
    pub graphql: Option<BruGraphQl>,
```

and add the type:

```rust
/// A GraphQL body: the query text and the optional variables JSON.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BruGraphQl {
    pub query: String,
    pub variables: Option<String>,
}
```

In `crates/rocket-import/src/bru/parser.rs`, in `parse_body`, add two arms to the `match subtype` before the `other =>` arm:

```rust
        "graphql" => {
            doc.graphql.get_or_insert_with(Default::default).query = raw;
            return;
        }
        "graphql:vars" => {
            // An empty vars block is the same as none.
            if !raw.trim().is_empty() {
                doc.graphql.get_or_insert_with(Default::default).variables = Some(raw);
            }
            return;
        }
```

In `crates/rocket-import/src/bru/yml_adapter.rs`:

1. Extend `BruYmlRequest`:

```rust
#[derive(Debug, Deserialize)]
pub struct BruYmlRequest {
    pub meta: Option<BruYmlMeta>,
    pub http: Option<BruYmlHttp>,
    /// OpenCollection-shaped `info:` block, used by GraphQL files.
    pub info: Option<BruYmlMeta>,
    /// OpenCollection-shaped `graphql:` block.
    pub graphql: Option<BruYmlGraphql>,
}

#[derive(Debug, Deserialize)]
pub struct BruYmlGraphql {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<BruYmlHeader>>,
    /// A `{query, variables}` mapping, or a list of titled variants.
    pub body: Option<serde_yaml::Value>,
    pub auth: Option<serde_yaml::Value>,
}
```

2. `BruYmlMeta` is reused for `info:` (it has `name`, `type` and `seq`, all optional). In `adapt_request`, at the very top (before the `// Meta` block), add:

```rust
    if let Some(gql) = yml.graphql {
        return adapt_graphql(yml.info.or(yml.meta), gql);
    }
```

(This must come before `yml.meta` is moved; destructure with `let BruYmlRequest { meta, http, info, graphql } = yml;` at the top of `adapt_request` and use those locals below. Rename the existing `yml.meta` / `yml.http` uses to `meta` / `http`.)

3. Change the non-http meta check so a `graphql` type is not reported as unsupported:

```rust
        if !matches!(request_type.as_str(), "http" | "" | "graphql") {
```

4. Add the adapter and helpers:

```rust
fn adapt_graphql(info: Option<BruYmlMeta>, gql: BruYmlGraphql) -> BruDocument {
    let mut doc = BruDocument::default();
    if let Some(m) = info {
        doc.meta = Some(BruMeta {
            name: m.name.unwrap_or_default(),
            request_type: "graphql".into(),
            seq: m.seq,
        });
    }
    doc.method = gql
        .method
        .as_deref()
        .and_then(|m| BruMethod::from_block_name(&m.to_lowercase()));
    doc.url = gql.url;
    if let Some(headers) = gql.headers {
        doc.headers = headers
            .into_iter()
            .map(|h| BruKeyValue {
                key: h.name,
                value: h.value,
                disabled: h.disabled,
            })
            .collect();
    }
    doc.graphql = gql.body.as_ref().and_then(graphql_body_of);
    // OpenCollection auth is `type:`-tagged, unlike the `mode:`-tagged http block,
    // so it is reported instead of converted.
    if let Some(auth) = gql.auth {
        let auth_type = auth
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        doc.unknown_blocks.push(BruRawBlock {
            name: "auth".into(),
            subtype: Some(auth_type),
            content: String::new(),
        });
    }
    doc
}

/// Reads a GraphQL body that is either a `{query, variables}` mapping or a
/// list of titled variants. A variant list yields the selected variant, or the first.
fn graphql_body_of(body: &serde_yaml::Value) -> Option<BruGraphQl> {
    fn plain(v: &serde_yaml::Value) -> Option<BruGraphQl> {
        let query = v.get("query")?.as_str()?.to_string();
        let variables = v
            .get("variables")
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(String::from);
        Some(BruGraphQl { query, variables })
    }
    match body.as_sequence() {
        Some(variants) => {
            let chosen = variants
                .iter()
                .find(|v| v.get("selected").and_then(|s| s.as_bool()).unwrap_or(false))
                .or_else(|| variants.first())?;
            plain(chosen.get("body")?)
        }
        None => plain(body),
    }
}
```

- [ ] **Step 5: Implement the converter**

In `crates/rocket-import/src/converter/request.rs`, extend the imports:

```rust
use rocket_collection::{GraphQlBody, GraphQlRequest, Request};
```

Add above `convert`:

```rust
/// What a Bruno file turns into.
#[derive(Debug, Clone, PartialEq)]
pub enum Converted {
    Http(Request),
    GraphQl(GraphQlRequest),
}

/// True when the document is a GraphQL request: its `meta` says so or it carries a GraphQL body.
fn is_graphql(doc: &BruDocument) -> bool {
    doc.graphql.is_some()
        || doc
            .meta
            .as_ref()
            .is_some_and(|m| m.request_type == "graphql")
}

/// Converts a Bruno document to whichever domain item it describes.
/// Unsupported request types (gRPC, WebSocket) still produce `(None, [skip])`.
pub fn convert_item(doc: &BruDocument) -> (Option<Converted>, Vec<SkipReason>) {
    if is_graphql(doc) {
        let (g, skipped) = convert_graphql(doc);
        return (g.map(Converted::GraphQl), skipped);
    }
    let (req, skipped) = convert(doc);
    (req.map(Converted::Http), skipped)
}

/// Unsupported-auth reasons recorded in the document.
fn auth_skips(doc: &BruDocument) -> Vec<SkipReason> {
    doc.unknown_blocks
        .iter()
        .filter(|b| b.name == "auth")
        .map(|b| SkipReason::UnsupportedAuthType(b.subtype.clone().unwrap_or_default()))
        .collect()
}

/// Converts a GraphQL Bruno document to a domain `GraphQlRequest`.
/// Unsupported auth is reported and the request still imports with `auth: None`.
pub fn convert_graphql(doc: &BruDocument) -> (Option<GraphQlRequest>, Vec<SkipReason>) {
    let skipped = auth_skips(doc);
    let name = doc
        .meta
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Untitled".into());
    let mut g = GraphQlRequest::new(name, doc.url.clone().unwrap_or_default());
    g.method = doc
        .method
        .as_ref()
        .map(bru_method_to_domain)
        .unwrap_or(HttpMethod::Post);
    g.seq = doc.meta.as_ref().and_then(|m| m.seq);

    for h in &doc.headers {
        g.headers.push(if h.disabled {
            Header::disabled(h.key.clone(), h.value.clone())
        } else {
            Header::new(h.key.clone(), h.value.clone())
        });
    }

    let gql = doc.graphql.clone().unwrap_or_default();
    g.body = GraphQlBody {
        query: gql.query,
        variables: gql.variables.filter(|v| !v.trim().is_empty()),
    };

    if skipped.is_empty() {
        if let Some(auth) = &doc.auth {
            g.auth = bru_auth_to_domain(auth);
        }
    }
    g.pre_request_script = doc.pre_request_script.clone();
    g.post_response_script = doc.post_response_script.clone();
    (Some(g), skipped)
}
```

(`Header` and `HttpMethod` are already imported at the top of the file.)

- [ ] **Step 6: Route the importer through `convert_item`**

In `crates/rocket-import/src/importer.rs`, replace the `Ok(doc) => { ... }` arm in `walk_requests` (the one that calls `req_converter::convert(&doc)`) with:

```rust
                Ok(doc) => {
                    let (item_opt, skipped_reasons) = req_converter::convert_item(&doc);

                    for reason in skipped_reasons {
                        report.skipped.push(SkippedItem {
                            path: rel_str.clone(),
                            reason,
                        });
                    }

                    let out_path = rel_path.with_extension("yml").to_string_lossy().to_string();
                    match item_opt {
                        Some(req_converter::Converted::Http(req)) => {
                            let _ = repo.save_request(collection_name, &out_path, &req);
                            report.imported += 1;
                        }
                        Some(req_converter::Converted::GraphQl(gql)) => {
                            match repo.save_graphql_request(collection_name, &out_path, &gql) {
                                Ok(_) => report.imported += 1,
                                Err(e) => report.skipped.push(SkippedItem {
                                    path: rel_str.clone(),
                                    reason: SkipReason::ParseError(e.to_string()),
                                }),
                            }
                        }
                        None => {}
                    }
                }
```

Update `crates/rocket-import/CLAUDE.md`: in "Non-fatal skips" replace "Unsupported request types (GraphQL, gRPC, WebSocket)" with "Unsupported request types (gRPC, WebSocket)"; add a `converter/request.rs` note that `convert_item` routes GraphQL to `convert_graphql`; add `graphql` to the BruDocument fields table (`body:graphql` and `body:graphql:vars` blocks, or the OpenCollection `graphql:` block).

- [ ] **Step 7: Write the failing integration test**

Append to `crates/rocket-import/tests/integration_test.rs`:

```rust
#[test]
fn bru_graphql_file_imports_as_graphql_item() {
    use rocket_collection::{CollectionRepository, GraphQlRequest};

    let src = TempDir::new().unwrap();
    std::fs::write(
        src.path().join("bruno.json"),
        r#"{ "name": "gql-api", "version": "1", "type": "collection" }"#,
    )
    .unwrap();
    std::fs::write(
        src.path().join("users.bru"),
        "meta {\n  name: Users\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://api.example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query Users($n: Int) {\n    users(first: $n) { id }\n  }\n}\n\nbody:graphql:vars {\n  {\n    \"n\": 5\n  }\n}\n",
    )
    .unwrap();
    std::fs::write(
        src.path().join("orders.yml"),
        "info:\n  name: Orders\n  type: graphql\ngraphql:\n  method: POST\n  url: https://api.example.com/graphql\n  body:\n    query: '{ orders { id } }'\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    let service = make_service(workspace_dir.path());
    let report = service.import_collection(src.path(), "default").unwrap();

    assert_eq!(report.total_files, 2);
    assert_eq!(report.imported, 2, "skipped: {:?}", report.skipped);
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);

    let repo = FsCollectionRepo::new_standalone(workspace_dir.path().join("collections"));
    let g: GraphQlRequest = repo
        .get_graphql_request(&report.created_collections[0], "users.yml")
        .unwrap();
    assert!(g.body.query.contains("users(first: $n)"), "{}", g.body.query);
    assert!(g.body.variables.as_deref().unwrap().contains("\"n\": 5"));
    assert_eq!(g.url, "https://api.example.com/graphql");

    let o = repo
        .get_graphql_request(&report.created_collections[0], "orders.yml")
        .unwrap();
    assert_eq!(o.body.query, "{ orders { id } }");
}
```

- [ ] **Step 8: Run all import tests**

Run:
- `cargo test -j4 -p rocket-import graphql`
- `cargo test -j4 -p rocket-import`

Expected: PASS. `import_report_counts_correctly` is unchanged because the GraphQL fixture lives in a temp directory, not in `tests/fixtures/my-api`.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-import
```

Suggested subject: `feat(import): import Bruno GraphQL requests as typed items`.

---

## Task 3: IPC commands, frontend types, sidebar, open and create

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`
- Modify: `src/lib/tauri-api.ts`, `src/lib/colors.ts`, `src/lib/pane-utils.ts`, `src/lib/request-save-mapper.ts`, `src/lib/auto-save.ts`, `src/lib/contracts/collectPaths.ts`
- Create: `src/lib/save-tab-request.ts`
- Modify: `src/types/pane-types.ts`, `src/stores/pane-store.ts`
- Modify: `src/components/collections/RequestNode.tsx`, `CollectionNode.tsx`, `FolderNode.tsx`
- Modify: `src/components/request/CreateRequestDialog.tsx`, `SaveRequestButton.tsx`, `SaveToCollectionDialog.tsx`
- Create tests: `src/lib/__tests__/graphql-state.test.ts`, `src/lib/__tests__/save-tab-request.test.ts`
- Modify tests: `src/components/collections/__tests__/RequestNode.test.tsx`, `src/lib/contracts/collectPaths.test.ts`

**Interfaces:**
- Consumes: `CollectionService::{get_graphql_request, save_graphql_request}` (Task 1), `RequestSummary.kind`.
- Produces:
  - Tauri commands `get_graphql_request(collection, path) -> GraphQlRequest` and `save_graphql_request(collection, path, request) -> GraphQlRequest`.
  - TS: `RequestKind`, `GraphQlBody`, `GraphQlBodyVariant`, `GraphQlRequest`, `getGraphQlRequest`, `saveGraphQlRequest`, `RequestSummary.kind?`, `CollectionItem` member `{ type: 'graphql' }`.
  - `RequestState.graphql?: GraphQlState` where `GraphQlState = { query: string; variables: string; operationName?: string; bodyVariants?: GraphQlBodyVariant[] }`.
  - `mapGraphQlToState(g: GraphQlRequest): RequestState`, `createDefaultRequestFor(kind)`, `DEFAULT_GRAPHQL_QUERY` in `pane-utils.ts`.
  - `buildGraphQlSavePayload(tab, overrides?)` and `toApiGraphQlRequest(uid, name, request)` in `request-save-mapper.ts`.
  - `saveTabRequest(collection, path, tab, overrides?)` in `save-tab-request.ts`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Add the Tauri commands**

In `src-tauri/src/commands/collections.rs`, extend the import `use rocket_collection::{Collection, CollectionSummary, CollectionVariable, Request};` with `GraphQlRequest`, and add after `get_request`:

```rust
#[tauri::command]
pub fn get_graphql_request(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<GraphQlRequest, DomainError> {
    svc.get_graphql_request(&collection, &path)
}

/// Saves a GraphQL request. The contract audit hook is HTTP-only, so it does not run here.
#[tauri::command]
pub fn save_graphql_request(
    collection: String,
    path: String,
    request: GraphQlRequest,
    svc: State<'_, CollectionService>,
) -> Result<GraphQlRequest, DomainError> {
    svc.save_graphql_request(&collection, &path, &request)
}
```

In `src-tauri/src/lib.rs`, in the `invoke_handler` list, add after `commands::collections::get_request,` the line `commands::collections::get_graphql_request,` and after `commands::collections::save_request,` the line `commands::collections::save_graphql_request,`.

Run: `cargo check -j4 -p rocket`.
Expected: PASS.

- [ ] **Step 3: Write the failing frontend state tests**

Create `src/lib/__tests__/graphql-state.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { GraphQlRequest } from '@/lib/tauri-api';
import { createDefaultRequestFor, DEFAULT_GRAPHQL_QUERY, mapGraphQlToState } from '../pane-utils';

const saved: GraphQlRequest = {
  uid: 'g1',
  name: 'Users',
  method: 'POST',
  url: 'https://api.example.com/graphql',
  headers: [{ key: 'X-Trace', value: '1', enabled: true }],
  auth: { authType: 'bearer', token: 't' } as GraphQlRequest['auth'],
  body: { query: '{ users { id } }', variables: '{"n":1}' },
  bodyVariants: [
    { title: 'A', selected: true, body: { query: '{ users { id } }', variables: '{"n":1}' } },
    { title: 'B', selected: false, body: { query: '{ b }' } },
  ],
  settings: { timeout: 5000 },
  preRequestScript: '// pre',
  tags: ['smoke'],
};

describe('mapGraphQlToState', () => {
  it('marks the tab graphql and carries the query, variables and variants', () => {
    const state = mapGraphQlToState(saved);
    expect(state.requestType).toBe('graphql');
    expect(state.method).toBe('POST');
    expect(state.graphql?.query).toBe('{ users { id } }');
    expect(state.graphql?.variables).toBe('{"n":1}');
    expect(state.graphql?.bodyVariants).toHaveLength(2);
  });

  it('reuses the HTTP mapping for headers, auth, scripts, tags and settings', () => {
    const state = mapGraphQlToState(saved);
    expect(state.headers[0]).toMatchObject({ key: 'X-Trace', value: '1', enabled: true });
    expect(state.auth.authType).toBe('bearer');
    expect(state.preRequestScript).toBe('// pre');
    expect(state.tags).toEqual(['smoke']);
    expect(state.settings.timeoutMs).toBe(5000);
  });

  it('defaults a missing variables field to an empty string', () => {
    const state = mapGraphQlToState({ ...saved, body: { query: '{ a }' }, bodyVariants: undefined });
    expect(state.graphql?.variables).toBe('');
    expect(state.graphql?.bodyVariants).toBeUndefined();
  });
});

describe('createDefaultRequestFor', () => {
  it('builds a POST graphql request with the default query', () => {
    const state = createDefaultRequestFor('graphql');
    expect(state.requestType).toBe('graphql');
    expect(state.method).toBe('POST');
    expect(state.graphql).toEqual({ query: DEFAULT_GRAPHQL_QUERY, variables: '' });
  });

  it('keeps the existing behaviour for the other kinds', () => {
    expect(createDefaultRequestFor('http').requestType).toBe('http');
    expect(createDefaultRequestFor('grpc').requestType).toBe('grpc');
    expect(createDefaultRequestFor('http').graphql).toBeUndefined();
  });
});
```

Create `src/lib/__tests__/save-tab-request.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
  saveGraphQlRequest: vi.fn().mockResolvedValue({ fileName: 'x.yml' }),
}));

import { saveGraphQlRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';
import { createDefaultRequestFor } from '../pane-utils';
import { buildGraphQlSavePayload } from '../request-save-mapper';
import { saveTabRequest } from '../save-tab-request';

function gqlTab(): RequestTab {
  const request = createDefaultRequestFor('graphql');
  request.url = 'https://api.example.com/graphql';
  request.graphql = {
    query: '{ users { id } }',
    variables: '',
    bodyVariants: [
      { title: 'A', selected: true, body: { query: '{ users { id } }' } },
      { title: 'B', selected: false, body: { query: '{ b }' } },
    ],
  };
  return { id: 'tab-1', title: 'Users', tabType: 'request', request, response: null, isDirty: true };
}

describe('saveTabRequest', () => {
  beforeEach(() => vi.clearAllMocks());

  it('routes a graphql tab to saveGraphQlRequest', async () => {
    await saveTabRequest('api', 'users.yml', gqlTab());
    expect(saveGraphQlRequest).toHaveBeenCalledTimes(1);
    expect(saveRequest).not.toHaveBeenCalled();
  });

  it('routes an http tab to saveRequest', async () => {
    const tab = gqlTab();
    tab.request = { ...tab.request, requestType: 'http', graphql: undefined };
    await saveTabRequest('api', 'users.yml', tab);
    expect(saveRequest).toHaveBeenCalledTimes(1);
    expect(saveGraphQlRequest).not.toHaveBeenCalled();
  });
});

describe('buildGraphQlSavePayload', () => {
  it('sends the query, keeps every variant and omits blank variables', () => {
    const payload = buildGraphQlSavePayload(gqlTab());
    expect(payload.uid).toBe('tab-1');
    expect(payload.method).toBe('POST');
    expect(payload.body.query).toBe('{ users { id } }');
    expect(payload.body.variables).toBeUndefined();
    expect(payload.bodyVariants).toHaveLength(2);
  });

  it('applies the save-to-collection overrides', () => {
    const payload = buildGraphQlSavePayload(gqlTab(), { name: 'Chosen', fileName: 'chosen.yml' });
    expect(payload.name).toBe('Chosen');
    expect(payload.fileName).toBe('chosen.yml');
  });
});
```

Add to `src/components/collections/__tests__/RequestNode.test.tsx` (new `describe` block at the end; reuse its `renderNode` helper and mocks, and add `getGraphQlRequest: vi.fn()` to the `vi.mock('@/lib/tauri-api', ...)` factory):

```tsx
describe('RequestNode graphql items', () => {
  const gqlSummary: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }> = {
    type: 'summary',
    uid: 'g-1',
    name: 'List Users',
    method: 'POST',
    url: 'https://api.example.com/graphql',
    kind: 'graphql',
  };

  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.getRequest).mockReset();
    vi.mocked(tauriApi.getGraphQlRequest).mockReset();
  });

  it('shows a GQL badge instead of the method', () => {
    renderNode(gqlSummary, 'list-users.yml');
    expect(screen.getByText('GQL')).toBeTruthy();
    expect(screen.queryByText('POST')).toBeNull();
  });

  it('opens through getGraphQlRequest and yields a graphql tab', async () => {
    vi.mocked(tauriApi.getGraphQlRequest).mockResolvedValue({
      uid: 'g-1',
      name: 'List Users',
      method: 'POST',
      url: 'https://api.example.com/graphql',
      headers: [],
      auth: { authType: 'none' },
      body: { query: '{ users { id } }' },
    });
    renderNode(gqlSummary, 'list-users.yml');
    await userEvent.click(screen.getByLabelText('Open GQL List Users'));
    await waitFor(() => {
      const found = findTabInTree(usePaneStore.getState().root, 'g-1');
      expect(found).not.toBeNull();
    });
    const tab = findTabInTree(usePaneStore.getState().root, 'g-1')?.tab;
    expect(tab && 'request' in tab && tab.request.requestType).toBe('graphql');
    expect(tauriApi.getRequest).not.toHaveBeenCalled();
  });

  it('is not draggable into a Flow, because Flow requests are HTTP only', () => {
    renderNode(gqlSummary, 'list-users.yml');
    expect(screen.getByTestId('request-item-GQL-List Users').getAttribute('draggable')).toBe(
      'false',
    );
  });
});
```

Add a case to `src/lib/contracts/collectPaths.test.ts`:

```ts
  it('skips typed graphql items like opaque ones', () => {
    const folders: string[] = [];
    const requests: string[] = [];
    collectPaths(
      [
        {
          type: 'graphql',
          uid: 'g',
          name: 'Q',
          method: 'POST',
          url: 'https://x/graphql',
          headers: [],
          auth: { authType: 'none' },
          body: { query: '{ a }' },
          fileName: 'q.yml',
        },
      ],
      '',
      folders,
      requests,
    );
    expect(requests).toEqual([]);
  });
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `yarn test graphql-state save-tab-request RequestNode collectPaths`
Expected: FAIL (missing exports and types).

- [ ] **Step 5: Implement the types, mappers and IPC wrappers**

In `src/lib/tauri-api.ts`:

1. Add after the `Request` interface:

```ts
export type RequestKind = 'http' | 'graphql' | 'grpc' | 'websocket';

export interface GraphQlBody {
  query: string;
  /** JSON text, not a parsed object. */
  variables?: string | null;
}

export interface GraphQlBodyVariant {
  title: string;
  selected: boolean;
  body: GraphQlBody;
}

export interface GraphQlRequest {
  uid: string;
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  body: GraphQlBody;
  /** Every stored variant. Send it back unchanged so a save keeps the unselected ones. */
  bodyVariants?: GraphQlBodyVariant[];
  auth: Auth;
  fileName?: string;
  tags?: string[];
  docs?: string | null;
  settings?: ApiRequestSettings;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
  tests?: string | null;
  assertions?: AssertionEntry[];
  actions?: ActionEntry[];
}
```

2. Add `kind?: RequestKind;` to `RequestSummary` (after `fileName?`), and extend the doc comment of `OpaqueProtocolItem` to `protocol: 'grpc' | 'websocket'` (GraphQL is typed now): change the union to `'graphql' | 'grpc' | 'websocket'` only if an existing test still builds a graphql opaque item (`collectPaths.test.ts` does, so keep the union as is).

3. Extend `CollectionItem`:

```ts
export type CollectionItem =
  | ({ type: 'request' } & Request)
  | ({ type: 'folder' } & Folder)
  | ({ type: 'summary' } & RequestSummary)
  | ({ type: 'graphql' } & GraphQlRequest)
  | ({ type: 'opaque' } & OpaqueProtocolItem);
```

4. Add after `saveRequest`:

```ts
export const getGraphQlRequest = (collection: string, path: string) =>
  invoke<GraphQlRequest>('get_graphql_request', { collection, path });

export const saveGraphQlRequest = (collection: string, path: string, request: GraphQlRequest) =>
  invoke<GraphQlRequest>('save_graphql_request', { collection, path, request });
```

In `src/types/pane-types.ts`, add to `RequestState` (after `actions`):

```ts
  /** Present when `requestType` is 'graphql'. */
  graphql?: GraphQlState;
```

and add the type:

```ts
export interface GraphQlState {
  query: string;
  /** JSON text. Empty means no variables. */
  variables: string;
  /** The operation to run when the document defines several. Session state, never saved. */
  operationName?: string;
  /** Stored body variants, round-tripped so a save keeps them. */
  bodyVariants?: import('@/lib/tauri-api').GraphQlBodyVariant[];
}
```

In `src/lib/pane-utils.ts`, add the imports `GraphQlRequest` (from `@/lib/tauri-api`) and `RequestKind`, and add:

```ts
// A valid document on every GraphQL server, used to seed a new request.
export const DEFAULT_GRAPHQL_QUERY = '{\n  __typename\n}\n';

// Maps a saved GraphQL request to the tab state. The shared fields reuse the HTTP mapping.
export function mapGraphQlToState(g: GraphQlRequest): RequestState {
  const { body, bodyVariants, ...rest } = g;
  const base = mapApiRequestToState(rest as ApiRequest, true);
  return {
    ...base,
    requestType: 'graphql',
    method: g.method as RequestState['method'],
    graphql: {
      query: body.query,
      variables: body.variables ?? '',
      ...(bodyVariants && bodyVariants.length > 0 ? { bodyVariants } : {}),
    },
  };
}

// Builds a blank request of the given kind. Only GraphQL has its own editor state so far.
export function createDefaultRequestFor(kind: RequestKind): RequestState {
  const base = createDefaultRequest();
  if (kind === 'graphql') {
    return {
      ...base,
      requestType: 'graphql',
      method: 'POST',
      graphql: { query: DEFAULT_GRAPHQL_QUERY, variables: '' },
    };
  }
  return { ...base, requestType: kind };
}
```

(`ApiRequest` is the alias already imported at the top of `pane-utils.ts` as `import type { Request as ApiRequest }`.)

In `src/stores/pane-store.ts`, change `openEphemeralTab` so its request comes from the new factory: replace `request: { ...createDefaultRequest(), requestType },` with `request: createDefaultRequestFor(requestType),` and update the import from `@/lib/pane-utils` (add `createDefaultRequestFor`; drop `createDefaultRequest` if it is no longer used in that file). The existing test `openEphemeralTab with "graphql" sets requestType correctly` must still pass.

In `src/lib/request-save-mapper.ts`, add:

```ts
import type { GraphQlRequest } from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

// Builds the persisted GraphQL payload from tab state. Shared by the Save button,
// save-to-collection and auto-save, so all three write the same fields.
export function toApiGraphQlRequest(
  uid: string,
  name: string,
  request: RequestState,
): GraphQlRequest {
  const s = request.settings;
  const gql = request.graphql ?? { query: '', variables: '' };
  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    auth: toPersistedAuth(request.auth),
    body: {
      query: gql.query,
      variables: gql.variables.trim() === '' ? undefined : gql.variables,
    },
    bodyVariants: gql.bodyVariants,
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
    settings: s
      ? {
          timeout: s.timeoutMs,
          followRedirects: s.followRedirects,
          verifySsl: s.verifySsl,
          maxRedirects: s.maxRedirects,
          encodeUrl: s.encodeUrl,
        }
      : undefined,
    docs: request.docs ?? null,
    preRequestScript: request.preRequestScript ?? null,
    postResponseScript: request.postResponseScript ?? null,
    tests: request.testsScript ?? null,
    assertions: request.assertions ?? [],
    actions: request.actions ?? [],
  };
}

export function buildGraphQlSavePayload(
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): GraphQlRequest {
  const payload = toApiGraphQlRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
  );
  return overrides?.fileName !== undefined ? { ...payload, fileName: overrides.fileName } : payload;
}
```

(Merge the two new `import type` lines with the existing ones at the top of the file.)

Create `src/lib/save-tab-request.ts`:

```ts
import {
  buildGraphQlSavePayload,
  buildRequestSavePayload,
  type RequestSavePayloadOverrides,
} from '@/lib/request-save-mapper';
import { saveGraphQlRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';

// Saves a tab through the command that matches its protocol. Writing a GraphQL
// tab through saveRequest would replace the GraphQL file with an HTTP one.
export async function saveTabRequest(
  collection: string,
  path: string,
  tab: RequestTab,
  overrides?: RequestSavePayloadOverrides,
): Promise<{ fileName?: string }> {
  if (tab.request.requestType === 'graphql') {
    return saveGraphQlRequest(collection, path, buildGraphQlSavePayload(tab, overrides));
  }
  return saveRequest(collection, path, buildRequestSavePayload(tab, overrides));
}
```

In `src/components/request/SaveRequestButton.tsx`, replace the `saveRequest(...)` call with `await saveTabRequest(tab.source.collection, tab.source.path, tab);` and fix the imports (`saveTabRequest` in, `buildRequestSavePayload` and `saveRequest` out).

In `src/components/request/SaveToCollectionDialog.tsx`, replace

```ts
      const payload = buildRequestSavePayload(tab, { name: trimmedName, fileName: fsName });

      const saved = await saveRequest(collectionName, fsName, payload);
```

with

```ts
      const saved = await saveTabRequest(collectionName, fsName, tab, {
        name: trimmedName,
        fileName: fsName,
      });
```

and fix the imports (keep `createCollection` and `listCollections`).

In `src/lib/auto-save.ts`, add the imports `import { toApiGraphQlRequest } from '@/lib/request-save-mapper';` and `saveGraphQlRequest` from tauri-api, and branch inside the timer callback before the existing `await saveRequest(...)`:

```ts
      if (request.requestType === 'graphql') {
        await saveGraphQlRequest(
          collection,
          path,
          toApiGraphQlRequest(tabId || crypto.randomUUID(), title, request),
        );
      } else {
        await saveRequest(
          collection,
          path,
          toApiRequest(tabId || crypto.randomUUID(), title, request),
        );
      }
```

(keep `usePaneStore.getState().markClean(tabId);` after the branch). Add a case to `src/lib/__tests__/auto-save.test.ts`: it mocks `@/lib/tauri-api` with only `saveRequest`; extend that factory with `saveGraphQlRequest: vi.fn().mockResolvedValue(undefined)`, import it, and add:

```ts
  it('saves a graphql tab through saveGraphQlRequest, never saveRequest', () => {
    const request = baseRequest({
      requestType: 'graphql',
      method: 'POST',
      graphql: { query: '{ a }', variables: '' },
    });
    scheduleAutoSave('tab1', 'my-collection', 'q.yml', 'Q', request);
    vi.advanceTimersByTime(500);
    expect(saveGraphQlRequest).toHaveBeenCalledWith(
      'my-collection',
      'q.yml',
      expect.objectContaining({ body: { query: '{ a }', variables: undefined } }),
    );
    expect(saveRequest).not.toHaveBeenCalled();
  });
```

In `src/lib/contracts/collectPaths.ts`, change the guard to `} else if (item.type !== 'opaque' && item.type !== 'graphql') {`.

In `src/lib/colors.ts`, add to `METHOD_BADGE_COLOR` (after `HEAD`):

```ts
  GQL: 'text-fuchsia-500 dark:text-fuchsia-400 border-fuchsia-500/30 bg-fuchsia-500/10 dark:bg-fuchsia-500/20',
```

- [ ] **Step 6: Implement the sidebar and open behaviour**

In `src/components/collections/RequestNode.tsx`:

- import `getGraphQlRequest` with `getRequest, renameRequest` and `mapGraphQlToState` with the pane-utils imports;
- derive the kind and the badge right after the `RequestNodeProps` destructure inside the component:

```tsx
  const kind = itemData.type === 'summary' ? (itemData.kind ?? 'http') : 'http';
  // The badge shows the protocol for GraphQL, the HTTP verb otherwise.
  const badge = kind === 'graphql' ? 'GQL' : method;
```

- in `createTab`, replace the first two lines with:

```tsx
    let request: RequestState;
    if (kind === 'graphql') {
      request = mapGraphQlToState(await getGraphQlRequest(collectionName, path));
    } else {
      const full = itemData.type === 'request' ? itemData : await getRequest(collectionName, path);
      request = mapApiRequestToState(full, true);
    }
```

- change `data-testid`, `aria-label`, the badge colour lookup and the badge text to use `badge` (`data-testid={`request-item-${badge}-${name}`}`, `aria-label={`Open ${badge} ${name}`}`, `METHOD_BADGE_COLOR[badge.toUpperCase()] ?? METHOD_BADGE_COLOR['GET']`, `{badge}`);
- set `draggable={kind === 'http'}` on the `TreeItem` (Flow nodes run saved HTTP requests only; a GraphQL file dropped there would fail to load);
- leave the drag payload and `onDragStart` as they are.

In `src/components/collections/CollectionNode.tsx` and `FolderNode.tsx`, extend both opaque guards, because a full tree can now carry `graphql` items and `RequestNode` only takes request and summary items:

```tsx
  const filterableItems = rawItems.filter((item) => item.type !== 'opaque' && item.type !== 'graphql');
```

(`FolderNode.tsx` uses `items.filter(...)`) and

```tsx
            if (item.type === 'opaque' || item.type === 'graphql') return null;
```

Update the two nearby comments to say "Opaque and typed GraphQL full-tree items never render here: the sidebar loads summaries, where GraphQL arrives as a `summary` with `kind: 'graphql'`."

In `src/components/request/CreateRequestDialog.tsx`:

- import `DEFAULT_GRAPHQL_QUERY, mapGraphQlToState` from `@/lib/pane-utils` and `saveGraphQlRequest` from tauri-api; keep `createDefaultRequest`;
- add `disabled?: boolean` to the `REQUEST_TYPES` entries and mark gRPC and WebSocket `disabled: true` with labels `'gRPC (coming soon)'` and `'WebSocket (coming soon)'`, and render `<SelectItem key={t.value} value={t.value} disabled={t.disabled}>`. They used to save an HTTP request under a protocol label; until their plans land they must not be selectable;
- in `handleCreate`, branch before building the HTTP payload:

```tsx
      if (requestType === 'graphql') {
        const saved = await saveGraphQlRequest(collectionName, filePath, {
          uid,
          name: trimmedName,
          method: 'POST',
          url,
          headers: [],
          auth: { authType: 'none' as const },
          body: { query: DEFAULT_GRAPHQL_QUERY },
          fileName: filePath,
        });
        const gqlTab: RequestTab = {
          id: uid,
          title: trimmedName,
          tabType: 'request',
          request: mapGraphQlToState(saved),
          response: null,
          isDirty: false,
          source: { collection: collectionName, path: saved.fileName ?? filePath },
        };
        usePaneStore.getState().openTab(gqlTab);
        reset();
        onClose();
        return;
      }
```

- hide the HTTP method select for GraphQL: the condition `(requestType === 'http' || requestType === 'curl')` already does that, so leave it;
- change the dialog description (`sr-only`) from "Add a new HTTP request" to "Add a new request".

Add a test `src/components/request/__tests__/CreateRequestDialog.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
import { createDefaultLeaf } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    saveRequest: vi.fn(),
    saveGraphQlRequest: vi.fn(),
  };
});

describe('CreateRequestDialog graphql', () => {
  beforeEach(() => {
    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    vi.mocked(tauriApi.saveRequest).mockReset();
    vi.mocked(tauriApi.saveGraphQlRequest).mockReset();
  });

  it('saves a real GraphQL item, not an HTTP request tagged graphql', async () => {
    vi.mocked(tauriApi.saveGraphQlRequest).mockImplementation(async (_c, _p, request) => ({
      ...request,
      fileName: 'users.yml',
    }));
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    await userEvent.click(await screen.findByRole('option', { name: 'GraphQL' }));
    await userEvent.type(screen.getByLabelText('Request Name'), 'users');
    await userEvent.type(screen.getByLabelText('URL'), 'https://api.example.com/graphql');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(tauriApi.saveGraphQlRequest).toHaveBeenCalledTimes(1));
    expect(tauriApi.saveRequest).not.toHaveBeenCalled();
    const [, , payload] = vi.mocked(tauriApi.saveGraphQlRequest).mock.calls[0];
    expect(payload.method).toBe('POST');
    expect(payload.body.query).toContain('__typename');
  });

  it('does not offer gRPC or WebSocket until their plans land', async () => {
    render(<CreateRequestDialog open collectionName='api' onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole('combobox', { name: /request type/i }));
    const grpc = await screen.findByRole('option', { name: /gRPC/ });
    expect(grpc.getAttribute('aria-disabled')).toBe('true');
  });
});
```

If the shadcn `Select` trigger is not exposed with the accessible name "Request Type" in jsdom, fall back to `screen.getByText('HTTP')` for the trigger, matching how existing tests open `Select` components in this repo (search `__tests__` for `combobox`).

- [ ] **Step 7: Run the checks**

Run:
- `yarn test graphql-state save-tab-request RequestNode collectPaths auto-save CreateRequestDialog pane-store`
- `yarn tsc --noEmit`
- `yarn check`
- `cargo check -j4 -p rocket`

Expected: PASS. `yarn tsc --noEmit` is the net for every site that switches on `CollectionItem['type']`; if it flags another one (for example `src/components/layout/CollectionsSidebar.tsx` duplicate lookup, which already only handles `type === 'request'`), add a `graphql` case that mirrors what that site does for `opaque`.

- [ ] **Step 8: Manual check in the real app**

Run `yarn tauri dev`. Create a GraphQL request from a collection's New Request dialog. Confirm: a `GQL` badge row appears, the file on disk is `graphql:`-shaped YAML (not `http:`), clicking the row opens a tab, editing the URL and auto-save keep it a GraphQL file, and renaming keeps it a GraphQL file. The Body tab does not edit the query yet; Plan 06 adds the editor.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add src-tauri/src/commands/collections.rs src-tauri/src/lib.rs \
  src/lib src/types/pane-types.ts src/stores/pane-store.ts \
  src/components/collections src/components/request
```

Suggested subject: `feat(ui): open, create and save GraphQL requests from the sidebar`.

---

## Next Plan

[Plan 06: GraphQL execution and editor](2026-10-05-protocol-parity-plan-06-graphql-editor-and-execution.md). It depends on this plan (`GraphQlRequest`, `CollectionItem::GraphQl`, `RequestState.graphql`, `saveTabRequest`). Chain to it automatically when this one finishes.
