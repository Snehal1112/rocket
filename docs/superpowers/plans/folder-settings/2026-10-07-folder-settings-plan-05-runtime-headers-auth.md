# Folder settings, Plan 05: Runtime header and auth inheritance

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every request below a folder sends that folder's headers and auth. Header order is collection, then folders from outermost to innermost, then the request. A request set to `inherit` (or `none`, which the backend already treats the same way) takes the nearest folder auth, then the collection auth. The rule applies on every send path: the Send button, Copy as cURL, the collection runner, Flow, load tests, GraphQL introspection, WebSocket and GraphQL subscriptions.

**Architecture:** `rocket-app` gets one shared rule. `RequestExecutionService::folder_chain` loads the chain once per execution through `CollectionRepository::get_folder_chain_settings`. `RequestExecutionService::inherited_auth_and_headers` applies it through the free function `apply_inherited_defaults`, which calls the locked helpers `rocket_collection::inherited_headers` and `rocket_collection::resolve_folder_auth` and then the existing `merge_headers` and `merge_auth`. `resolve_request` becomes a thin wrapper over the new `resolve_request_with_chain`, so plan 06 can load the chain once in `begin_phases` and reuse it for scripts. `resolve_websocket` calls the same method. The frontend resolves inherited auth and collection headers itself before it calls the backend (`src/lib/execute-request.ts`). It puts collection headers into the request's own header list, and those would beat a folder header in the backend merge. So the frontend path gets a mirror of the same rule in a new `src/lib/folder-inheritance.ts`.

**Tech Stack:** Rust (rocket-app, rocket-collection types from plans 01 and 02), React + TypeScript, Vitest. Run Rust tests with `cargo test -j4 -p rocket-app <filter>`.

**Spec:** [2026-10-07-folder-settings-design.md](../../specs/2026-10-07-folder-settings-design.md), section "Runtime rules" (Headers and Auth rows) and "Error handling". Locked names: [00-plan-index.md](00-plan-index.md), sections "Rust domain", "Repository" and "Runtime".

**Depends on:** plan 01 (`FolderSettings`, `inherited_headers`, `resolve_folder_auth`), plan 02 (`get_folder_chain_settings` in `FsCollectionRepo`). Task 3 also needs plan 04 (`getFolderSettings` and the TS `FolderSettings` type in `src/lib/tauri-api.ts`). Plan 04 comes before plan 05 in the recommended order.

## Merge site audit

Every place in the repo that merges collection headers or auth into a request before dispatch, and what this plan does with it.

| Site | What it does today | Decision |
|---|---|---|
| `crates/rocket-app/src/execution_service.rs` `resolve_request` (lines 705-713) | `merge_auth(input.auth, settings.auth)` and `merge_headers(&settings.headers, &input.headers)` | **Change (Task 1).** Shared path for `execute`, `execute_capturing` (Flow), `begin_phases` (collection runner, `collection_runner_service.rs:392`), `run_load_test`, `LoadTestService` (`load_test_service.rs:33`) and GraphQL introspection (`graphql_schema.rs:344` calls `execute`). One change covers all of them. |
| `crates/rocket-app/src/execution_service/websocket_resolution.rs` `resolve_websocket` (lines 154-164) | Same merge, duplicated | **Change (Task 2)** to call the shared method. Also covers GraphQL subscriptions (`graphql_subscription.rs` `resolve_graphql_subscription` calls `resolve_websocket`). |
| `crates/rocket-app/src/collection_runner_service.rs`, `runner_sequence.rs` `build_step_input` | Passes the saved request auth (`runtime_auth` first) and headers unchanged | **No code change.** Runs through `begin_phases` → `resolve_request`. Task 2 adds a runner test to prove it. |
| `crates/rocket-app/src/flow_execution_service.rs`, `flow_auth.rs` | Saved and inline nodes go through `build_step_input` and `execute_capturing`. `FlowCredentials::apply_to_inherit` swaps an `inherit`/`none` request auth for the Auth node's auto-apply credential before resolution. | **Small change (Task 2).** Inline nodes use the synthetic path `__flow_inline__/<node id>`. Task 2 names it `FLOW_INLINE_PATH_PREFIX` and `folder_chain` returns no chain for it, because an inline node is not in the tree. The auto-apply credential keeps winning over folder auth, exactly as it wins over collection auth today. |
| `crates/rocket-app/src/load_test_service.rs` | Calls `resolve_request` | **No change.** Covered by Task 1. |
| `crates/rocket-app/src/oauth2_service.rs` | Builds variables only (`build_variable_context`), no header or auth merge | **No change.** |
| `crates/rocket-app/src/contract_service.rs` | Reads saved request headers to write an OpenAPI document (line 782), no dispatch | **No change.** An export describes the saved request, not the sent one. |
| `crates/rocket-app/src/grpc_service.rs` `build_call` / `auth_metadata` (lines 356-414) | Collection headers and auth as gRPC metadata | **Deferred, gap.** `GrpcExecuteInput` has no request path, so the folder chain cannot be found. Adding one changes the gRPC IPC DTO and the gRPC panel. Listed as a follow-up in Next Plan. |
| `src/lib/execute-request.ts` `resolveRequestFieldsForPath` (lines 244-330) | Merges enabled collection headers under the request headers, and swaps `inherit` for the collection auth from `useCollectionAuthStore` | **Change (Task 3).** It does not call a backend resolve path. It sends the merged list as the request's own headers, so the backend would let a collection header beat a folder header. Callers that are fixed with it: `sendRequest`, `RequestPanel.handleCopyAsCurl` → `curl-generator.ts`, `runner-execute.ts`, `graphql-schema-input.ts`. |
| `src/lib/curl-generator.ts` | Formats `ResolvedRequestFields` | **No change.** Fixed by Task 3 through `resolveRequestFields`. |
| `src/lib/saved-request-preview.ts` | Preview card of the saved request's own headers | **No change.** It shows what is saved in the file, not what is sent. |

## Global Constraints

- Never `unwrap()` in production paths. Tests may use `.expect("reason")`.
- No serde attribute changes. This plan adds no persisted or IPC struct.
- Always pass `-j4` to cargo, one crate at a time. Never `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (`git add <paths>`), commit with a pathspec (`git commit --only -m "..." -- <paths>`). Never `git add -A` or `git add .`. Peer sessions share this repo's index.
- Frontend: no new UI. `lucide-react` and shadcn/ui rules are untouched. Zustand stores are read through `getState()` outside React, as today.
- `merge_auth` and `merge_headers` keep their current behavior and tests (`execution_service.rs` lines 4417-4512). The new code wraps them, it does not edit them.
- `execution_service.rs` is a merge point shared with plan 06 (index, "Merge points between plans"). Keep the new methods next to `resolve_request` so plan 06 rebases cleanly.

## Review Focus

1. A request header beats a folder header, and a folder header beats a collection header, by key (Task 1 test `folder_header_beats_collection_header_and_request_header_beats_folder`).
2. A disabled header at any level never hides an enabled one from an outer level (Task 1 test `disabled_headers_never_shadow_an_inherited_header`).
3. Nested folders: the inner folder wins for headers and auth (Task 1 tests `inner_folder_header_beats_outer_folder_header` and `inherit_takes_the_nearest_folder_auth`).
4. A request on `inherit` skips folders whose auth is `None` or `Inherit` and falls back to the collection auth; an explicit request auth beats every folder (Task 1 tests `inherit_skips_folders_without_auth_and_falls_back_to_the_collection` and `an_explicit_request_auth_beats_folder_auth`).
5. Folder auth `{{placeholders}}` resolve through `resolve_auth`, and an OAuth2 folder auth reaches the executor unchanged, so the executor fetches the client-credentials token as it does for collection OAuth2 (Task 1 tests `folder_auth_placeholders_resolve_through_resolve_auth` and `oauth2_folder_auth_reaches_the_executor_unchanged`).
6. A folder chain that fails to load fails the send. It is never dropped (Task 1 test `a_failing_folder_chain_fails_the_request`).
7. WebSocket and GraphQL subscription handshakes get the same folder headers and auth (Task 2 test `folder_headers_and_auth_apply_between_collection_and_request`). A runner step gets the folder auth (Task 2 test `a_runner_step_inherits_folder_auth`). An inline Flow node inherits nothing (Task 2 test `an_inline_flow_request_inherits_no_folder_settings`).
8. The Send button does not let a collection header or the collection auth store beat a folder value, and an OAuth2 folder auth is left as `inherit` for the backend (Task 3 tests in `execute-request.folder.test.ts`).

---

## Task 1: Folder chain in the shared `resolve_request` path

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (import at line 7, `resolve_request` at lines 687-713, new free function after `merge_headers` at line 2202, `StubCollectionRepo` at lines 2434-2537, new nested test module after `resolve_request_folds_in_external_secrets`, which ends at line 4298)
- Test: `crates/rocket-app/src/execution_service.rs` (nested module `tests::folder_inheritance`)

**Interfaces:**
- Consumes (plans 01 and 02, locked):
  - `rocket_collection::FolderSettings` (`Debug, Clone, PartialEq, Default`; fields `headers: Vec<Header>`, `auth: Option<Auth>`, and others)
  - `rocket_collection::inherited_headers(collection: &[Header], folders: &[FolderSettings]) -> Vec<Header>`
  - `rocket_collection::resolve_folder_auth(folders: &[FolderSettings]) -> Option<Auth>`
  - `CollectionRepository::get_folder_chain_settings(&self, collection: &str, request_path: &str) -> DomainResult<Vec<FolderSettings>>` (default `Ok(vec![])`)
  - Existing: `merge_auth(request_auth: Auth, collection_auth: Option<Auth>) -> Auth`, `merge_headers(collection_headers: &[Header], request_headers: &[Header]) -> Vec<Header>`, `resolve_auth(auth: Auth, vars: &HashMap<String, String>) -> Auth` (all private in `execution_service.rs`)
- Produces:
  - `RequestExecutionService::folder_chain(&self, collection: Option<&str>, request_path: Option<&str>) -> DomainResult<Vec<FolderSettings>>` (`pub(crate)`)
  - `RequestExecutionService::inherited_auth_and_headers(&self, collection: Option<&str>, folders: &[FolderSettings], request_auth: Auth, request_headers: &[Header]) -> (Auth, Vec<Header>)` (`pub(crate)`)
  - `RequestExecutionService::resolve_request_with_chain(&self, input: &ExecuteRequestInput, external_secrets: &HashMap<String, String>, folders: &[FolderSettings]) -> DomainResult<HttpRequest>` (`pub(crate)`, for plan 06)
  - `fn apply_inherited_defaults(request_auth: Auth, request_headers: &[Header], settings: CollectionSettings, folders: &[FolderSettings]) -> (Auth, Vec<Header>)` (private)
  - `resolve_request` keeps its signature.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`, the sections on `Folder`, `RequestDefaults`, `Auth` and `inherit`. Confirm plans 01 and 02 are merged: `grep -n "fn get_folder_chain_settings" crates/rocket-collection/src/repository.rs` and `grep -n "pub fn inherited_headers\|pub fn resolve_folder_auth" crates/rocket-collection/src/folder_settings.rs` must both print a line.

- [ ] **Step 2: Give `StubCollectionRepo` a folder chain**

In `crates/rocket-app/src/execution_service.rs`, test module, add `FolderSettings` to the `rocket_collection` import (line 2232):

```rust
    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionSummary,
        CollectionVariable, FolderSettings, Request as CollectionRequest,
    };
```

Replace the struct and the two constructors (lines 2433-2459):

```rust
    // Collection repo with configurable per-collection settings, folder, and request variables.
    struct StubCollectionRepo {
        settings: CollectionSettings,
        folder_vars: Vec<CollectionVariable>,
        request_vars: Vec<CollectionVariable>,
        root: Option<std::path::PathBuf>,
        folder_chain: Vec<FolderSettings>,
        folder_chain_error: Option<String>,
    }

    impl StubCollectionRepo {
        fn empty() -> Self {
            Self {
                settings: CollectionSettings::default(),
                folder_vars: vec![],
                request_vars: vec![],
                root: None,
                folder_chain: vec![],
                folder_chain_error: None,
            }
        }

        fn with_settings(settings: CollectionSettings) -> Self {
            Self {
                settings,
                folder_vars: vec![],
                request_vars: vec![],
                root: None,
                folder_chain: vec![],
                folder_chain_error: None,
            }
        }
```

After `with_request_vars` (ends line 2474), add:

```rust
        /// Every request path gets `chain` as its folder chain, outermost first.
        fn with_folder_chain(mut self, chain: Vec<FolderSettings>) -> Self {
            self.folder_chain = chain;
            self
        }

        /// Loading the folder chain fails with `message`, like a broken folder.yml.
        fn with_folder_chain_error(mut self, message: &str) -> Self {
            self.folder_chain_error = Some(message.into());
            self
        }
```

In `impl CollectionRepository for StubCollectionRepo`, after `get_folder_chain_variables` (lines 2531-2537), add:

```rust
        fn get_folder_chain_settings(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<FolderSettings>> {
            match &self.folder_chain_error {
                Some(message) => Err(DomainError::InvalidInput(message.clone())),
                None => Ok(self.folder_chain.clone()),
            }
        }
```

- [ ] **Step 3: Write the failing tests**

After the test `resolve_request_folds_in_external_secrets` (its closing brace is line 4298), add a nested module:

```rust
    /// Folder header and auth inheritance (folder settings plan 05).
    mod folder_inheritance {
        use super::*;

        fn folder_service(
            settings: CollectionSettings,
            repo: StubCollectionRepo,
        ) -> RequestExecutionService {
            let mut env = Environment::new("local");
            env.set_variable(Variable::new("token", "jwt-from-env"));
            let repo = StubCollectionRepo { settings, ..repo };
            RequestExecutionService::new(
                Box::new(MockEnvRepo::with_env(env)),
                Arc::new(MockExecutor::new(200)),
                Box::new(MockHistoryRepo::new()),
                Box::new(repo),
                Box::new(NullCookieRepo),
                Box::new(NullEventPublisher),
                Box::new(EmptySecretManagerRepo),
                Arc::new(rocket_environment::NullSecretStore),
                Arc::new(rocket_environment::NullVaultSecretFetcher),
            )
        }

        fn chain(folders: Vec<FolderSettings>) -> StubCollectionRepo {
            StubCollectionRepo::empty().with_folder_chain(folders)
        }

        fn folder(headers: Vec<Header>, auth: Option<Auth>) -> FolderSettings {
            FolderSettings {
                headers,
                auth,
                ..FolderSettings::default()
            }
        }

        fn input() -> ExecuteRequestInput {
            let mut input = sample_input("https://api.example.com/users", Some("local"));
            input.collection = Some("my-api".into());
            input.request_path = Some("users/admin/get.yml".into());
            input
        }

        fn resolve(svc: &RequestExecutionService, input: &ExecuteRequestInput) -> HttpRequest {
            svc.resolve_request(input, &std::collections::HashMap::new())
                .expect("resolve_request")
        }

        /// Values of the enabled headers named `key`, in send order.
        fn enabled_values(request: &HttpRequest, key: &str) -> Vec<String> {
            request
                .headers
                .iter()
                .filter(|h| h.enabled && h.key == key)
                .map(|h| h.value.clone())
                .collect()
        }

        fn bearer(token: &str) -> Auth {
            Auth::Bearer {
                token: token.into(),
            }
        }

        #[tokio::test]
        async fn folder_header_beats_collection_header_and_request_header_beats_folder() {
            let settings = CollectionSettings {
                headers: vec![
                    Header::new("X-Team", "core"),
                    Header::new("X-Env", "collection"),
                    Header::new("X-Trace", "collection"),
                ],
                ..Default::default()
            };
            let svc = folder_service(
                settings,
                chain(vec![folder(
                    vec![Header::new("X-Env", "folder"), Header::new("X-Trace", "folder")],
                    None,
                )]),
            );
            let mut input = input();
            input.headers = vec![Header::new("X-Trace", "request")];

            let resolved = resolve(&svc, &input);

            assert_eq!(enabled_values(&resolved, "X-Team"), vec!["core"]);
            assert_eq!(enabled_values(&resolved, "X-Env"), vec!["folder"]);
            assert_eq!(enabled_values(&resolved, "X-Trace"), vec!["request"]);
        }

        #[tokio::test]
        async fn disabled_headers_never_shadow_an_inherited_header() {
            let settings = CollectionSettings {
                headers: vec![Header::new("X-Env", "collection")],
                ..Default::default()
            };
            let svc = folder_service(
                settings,
                chain(vec![folder(
                    vec![
                        Header::disabled("X-Env", "folder-off"),
                        Header::new("X-Folder", "folder"),
                    ],
                    None,
                )]),
            );
            let mut input = input();
            input.headers = vec![Header::disabled("X-Folder", "request-off")];

            let resolved = resolve(&svc, &input);

            assert_eq!(enabled_values(&resolved, "X-Env"), vec!["collection"]);
            assert_eq!(enabled_values(&resolved, "X-Folder"), vec!["folder"]);
        }

        #[tokio::test]
        async fn inner_folder_header_beats_outer_folder_header() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![
                    folder(
                        vec![Header::new("X-Env", "outer"), Header::new("X-Outer", "only")],
                        None,
                    ),
                    folder(vec![Header::new("X-Env", "inner")], None),
                ]),
            );

            let resolved = resolve(&svc, &input());

            assert_eq!(enabled_values(&resolved, "X-Env"), vec!["inner"]);
            assert_eq!(enabled_values(&resolved, "X-Outer"), vec!["only"]);
        }

        #[tokio::test]
        async fn inherit_takes_the_nearest_folder_auth() {
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let outer = Auth::Basic {
                username: "outer".into(),
                password: "secret".into(),
            };
            let svc = folder_service(
                settings,
                chain(vec![
                    folder(vec![], Some(outer)),
                    folder(vec![], Some(bearer("from-inner"))),
                ]),
            );
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, bearer("from-inner"));
        }

        #[tokio::test]
        async fn inherit_skips_folders_without_auth_and_falls_back_to_the_collection() {
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let svc = folder_service(
                settings,
                chain(vec![
                    folder(vec![], None),
                    folder(vec![], Some(Auth::Inherit)),
                    folder(vec![], Some(Auth::None)),
                ]),
            );
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, bearer("from-collection"));
        }

        #[tokio::test]
        async fn an_explicit_request_auth_beats_folder_auth() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![folder(vec![], Some(bearer("from-folder")))]),
            );
            let mut input = input();
            input.auth = bearer("from-request");

            assert_eq!(resolve(&svc, &input).auth, bearer("from-request"));
        }

        #[tokio::test]
        async fn folder_auth_placeholders_resolve_through_resolve_auth() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![folder(vec![], Some(bearer("{{token}}")))]),
            );
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, bearer("jwt-from-env"));
        }

        #[tokio::test]
        async fn oauth2_folder_auth_reaches_the_executor_unchanged() {
            use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
            let oauth = Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: OAuth2ClientCredentials {
                    client_id: "folder-client".into(),
                    client_secret: "folder-secret".into(),
                    placement: None,
                },
                scope: Some("read".into()),
                additional_parameters: None,
                token_config: None,
                settings: None,
            }));
            let settings = CollectionSettings {
                auth: Some(bearer("from-collection")),
                ..Default::default()
            };
            let svc = folder_service(settings, chain(vec![folder(vec![], Some(oauth.clone()))]));
            let mut input = input();
            input.auth = Auth::Inherit;

            assert_eq!(resolve(&svc, &input).auth, oauth);
        }

        #[tokio::test]
        async fn a_failing_folder_chain_fails_the_request() {
            let svc = folder_service(
                CollectionSettings::default(),
                StubCollectionRepo::empty()
                    .with_folder_chain_error("folder.yml in 'users' is not valid YAML"),
            );

            let err = svc
                .resolve_request(&input(), &std::collections::HashMap::new())
                .expect_err("a broken folder.yml must fail the send");

            assert!(err.to_string().contains("users"), "{err}");
        }
    }
```

- [ ] **Step 4: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-app folder_inheritance`

Expected: the module compiles and fails. `folder_header_beats_collection_header_and_request_header_beats_folder` fails with `left: ["collection"]` against `right: ["folder"]` for `X-Env`. `inherit_takes_the_nearest_folder_auth` fails with the collection bearer. `a_failing_folder_chain_fails_the_request` panics with `a broken folder.yml must fail the send`. `an_explicit_request_auth_beats_folder_auth` passes already.

- [ ] **Step 5: Add the shared helper and split `resolve_request`**

Change the import on line 7:

```rust
use rocket_collection::{
    inherited_headers, resolve_folder_auth, settings::SandboxMode as CollectionSandboxMode,
    CollectionRepository, CollectionSettings, FolderSettings,
};
```

After `merge_headers` (closing brace on line 2202), add:

```rust
/// Applies the collection and folder-chain defaults to a request's own auth and headers.
/// Headers: collection, then folders from outermost to innermost, then the request. A more
/// specific level replaces a header with the same key, and a disabled header never shadows.
/// Auth: a request auth of `none` or `inherit` takes the nearest folder auth, then the
/// collection auth.
fn apply_inherited_defaults(
    request_auth: Auth,
    request_headers: &[Header],
    settings: CollectionSettings,
    folders: &[FolderSettings],
) -> (Auth, Vec<Header>) {
    let headers = merge_headers(&inherited_headers(&settings.headers, folders), request_headers);
    let auth = merge_auth(request_auth, resolve_folder_auth(folders).or(settings.auth));
    (auth, headers)
}
```

Replace lines 687-694 (the doc comment and signature of `resolve_request`) with:

```rust
    /// Loads the folder chain above a request, outermost folder first.
    /// A request outside a collection has no chain. A `folder.yml` that cannot be read fails
    /// the send, so its settings are never dropped silently.
    pub(crate) fn folder_chain(
        &self,
        collection: Option<&str>,
        request_path: Option<&str>,
    ) -> DomainResult<Vec<FolderSettings>> {
        match (collection, request_path) {
            (Some(col), Some(path)) => self.collection_repo.get_folder_chain_settings(col, path),
            _ => Ok(Vec::new()),
        }
    }

    /// The auth and headers a request sends once collection and folder defaults apply.
    /// Every send path calls this, so a folder setting cannot apply on one path only.
    pub(crate) fn inherited_auth_and_headers(
        &self,
        collection: Option<&str>,
        folders: &[FolderSettings],
        request_auth: Auth,
        request_headers: &[Header],
    ) -> (Auth, Vec<Header>) {
        match collection {
            Some(col) => {
                let settings = self.collection_repo.get_settings(col).unwrap_or_default();
                apply_inherited_defaults(request_auth, request_headers, settings, folders)
            }
            None => (request_auth, request_headers.to_vec()),
        }
    }

    /// Resolves all {{placeholders}} in `input` using the full variable precedence
    /// chain and returns a ready-to-send `HttpRequest`. Called by both `execute` and
    /// `run_load_test` so resolution logic is never duplicated.
    pub(crate) fn resolve_request(
        &self,
        input: &ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> DomainResult<HttpRequest> {
        let folders =
            self.folder_chain(input.collection.as_deref(), input.request_path.as_deref())?;
        self.resolve_request_with_chain(input, external_secrets, &folders)
    }

    /// `resolve_request` with the folder chain already loaded. A caller that also needs the
    /// chain, such as the script phases, reads it once and passes it here.
    pub(crate) fn resolve_request_with_chain(
        &self,
        input: &ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
        folders: &[FolderSettings],
    ) -> DomainResult<HttpRequest> {
```

Inside the body, replace the old merge block (the comment `// Merge collection auth and headers with request-level values.` and the `if let Some(col) = &input.collection { ... } else { ... };` expression, old lines 705-713) with:

```rust
        // Merge collection and folder-chain auth and headers with request-level values.
        let (effective_auth, effective_headers) = self.inherited_auth_and_headers(
            input.collection.as_deref(),
            folders,
            input.auth.clone(),
            &input.headers,
        );
```

The rest of the body (`resolve_auth(effective_auth, &vars)` onwards) stays as it is.

- [ ] **Step 6: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-app folder_inheritance`
Expected: 9 passed.

Run: `cargo test -j4 -p rocket-app execution_service`
Expected: all pass, including the existing `merge_headers_*`, `merge_auth_*` and `resolve_request_resolves_placeholders_in_inherited_collection_auth`.

Run: `cargo check -j4 -p rocket-app --tests`
Expected: no warnings from `execution_service.rs`.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage the one path explicitly and commit with a pathspec:

```bash
git add crates/rocket-app/src/execution_service.rs
git commit --only -m "feat(app): inherit folder headers and auth when resolving a request" -- crates/rocket-app/src/execution_service.rs
```

---

## Task 2: WebSocket, runner and Flow share the same rule

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service/websocket_resolution.rs` (import at line 18, merge at lines 154-164, test module from line 232)
- Modify: `crates/rocket-app/src/test_doubles.rs` (`InMemoryCollectionRepo` at lines 34-43 and its impl, `SharedCollectionRepo` impl at lines 132-210)
- Modify: `crates/rocket-app/src/execution_service.rs` (new const after `pub mod websocket_resolution;` at line 27, guard in `folder_chain` from Task 1, one test in `tests::folder_inheritance`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (import at lines 11-13, sentinel at line 489)
- Modify: `crates/rocket-app/src/collection_runner_service.rs` (one test at the end of `mod tests`)
- Modify: `crates/rocket-app/CLAUDE.md` ("Header and auth merging" bullet)

**Interfaces:**
- Consumes: `RequestExecutionService::folder_chain` and `RequestExecutionService::inherited_auth_and_headers` from Task 1; `rocket_collection::FolderSettings`.
- Produces:
  - `pub(crate) const FLOW_INLINE_PATH_PREFIX: &str = "__flow_inline__/";` in `execution_service.rs`
  - `InMemoryCollectionRepo::with_folder_chain(collection: Collection, folder_chain: Vec<FolderSettings>) -> Arc<InMemoryCollectionRepo>` in `test_doubles.rs`
  - `SharedCollectionRepo::get_folder_chain_settings` forwards to the inner repo.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`, the `Auth` and `inherit` sections.

- [ ] **Step 2: Teach the shared test doubles a folder chain**

In `crates/rocket-app/src/test_doubles.rs`, add `FolderSettings` to the `rocket_collection` import (lines 13-16):

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    FolderSettings, Request as CollectionRequest,
};
```

Replace `InMemoryCollectionRepo` and its constructor (lines 33-43):

```rust
/// Collection repo backed by one in-memory `Collection`.
pub struct InMemoryCollectionRepo {
    collection: Collection,
    folder_chain: Vec<FolderSettings>,
}

impl InMemoryCollectionRepo {
    pub fn new(collection: Collection) -> Arc<Self> {
        Self::with_folder_chain(collection, Vec::new())
    }

    /// Like `new`, and every request path gets `folder_chain` as its folder chain.
    pub fn with_folder_chain(collection: Collection, folder_chain: Vec<FolderSettings>) -> Arc<Self> {
        Arc::new(Self {
            collection,
            folder_chain,
        })
    }
}
```

In `impl CollectionRepository for InMemoryCollectionRepo`, after `get_folder_chain_variables`, add:

```rust
    fn get_folder_chain_settings(&self, _: &str, _: &str) -> DomainResult<Vec<FolderSettings>> {
        Ok(self.folder_chain.clone())
    }
```

In `impl CollectionRepository for SharedCollectionRepo`, after its `get_folder_chain_variables`, add:

```rust
    fn get_folder_chain_settings(&self, a: &str, b: &str) -> DomainResult<Vec<FolderSettings>> {
        self.0.get_folder_chain_settings(a, b)
    }
```

Without this forward, `SharedCollectionRepo` would fall back to the trait default `Ok(vec![])` and the tests below would pass for the wrong reason.

- [ ] **Step 3: Write the failing tests**

WebSocket. In `crates/rocket-app/src/execution_service/websocket_resolution.rs`, test module, change `use rocket_collection::{Collection, CollectionSettings};` to:

```rust
    use rocket_collection::{Collection, CollectionSettings, FolderSettings};
```

Below `fn service(...)` add:

```rust
    fn service_with_folders(
        env: Environment,
        settings: CollectionSettings,
        folders: Vec<FolderSettings>,
    ) -> RequestExecutionService {
        let mut collection = Collection::new("api");
        collection.settings = settings;
        RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            RecordingExecutor::new(),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(InMemoryCollectionRepo::with_folder_chain(
                collection, folders,
            ))),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
    }
```

Below `collection_headers_and_auth_apply_and_request_values_win` add:

```rust
    #[tokio::test]
    async fn folder_headers_and_auth_apply_between_collection_and_request() {
        let settings = CollectionSettings {
            headers: vec![Header::new("X-Team", "core"), Header::new("X-Env", "collection")],
            auth: Some(Auth::Bearer { token: "from-collection".into() }),
            ..CollectionSettings::default()
        };
        let folder = FolderSettings {
            headers: vec![Header::new("X-Env", "{{host}}")],
            auth: Some(Auth::Bearer { token: "{{token}}".into() }),
            ..FolderSettings::default()
        };
        let svc = service_with_folders(dev_env(), settings, vec![folder]);
        let mut i = input("wss://h/ws");
        i.scope.request_path = Some("chat/live.yml".into());
        i.auth = Some(Auth::Inherit);

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(header(&resolved, "X-Team"), Some("core"));
        assert_eq!(header(&resolved, "X-Env"), Some("chat.example.com"));
        assert_eq!(header(&resolved, "Authorization"), Some("Bearer abc123"));
    }
```

Runner. In `crates/rocket-app/src/collection_runner_service.rs`, add at the end of `mod tests` (before its closing brace):

```rust
    #[tokio::test]
    async fn a_runner_step_inherits_folder_auth() {
        use rocket_shared::types::Auth;
        let mut list = Request::new("List", HttpMethod::Get, "https://api.test/users");
        list.file_name = Some("list.yml".to_string());
        list.auth = Auth::Inherit;
        let mut users = rocket_collection::Folder::new("users");
        users.add_request(list);
        let mut collection = Collection::new("my-api");
        collection.settings.auth = Some(Auth::Bearer {
            token: "from-collection".into(),
        });
        collection.root.add_subfolder(users);
        let folder = rocket_collection::FolderSettings {
            auth: Some(Auth::Bearer {
                token: "from-folder".into(),
            }),
            ..rocket_collection::FolderSettings::default()
        };
        let repo = InMemoryCollectionRepo::with_folder_chain(collection, vec![folder]);
        let executor = RecordingExecutor::new();
        let engine = ProgrammableEngine::new();
        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));
        let runner = CollectionRunnerService::new(
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(rocket_shared::events::NullEventPublisher),
        );

        let summary = runner.run(&exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(
            executor.sent_auths(),
            vec![Auth::Bearer {
                token: "from-folder".into()
            }]
        );
    }
```

Flow inline sentinel. In `crates/rocket-app/src/execution_service.rs`, inside `mod folder_inheritance` from Task 1, add:

```rust
        #[tokio::test]
        async fn an_inline_flow_request_inherits_no_folder_settings() {
            let svc = folder_service(
                CollectionSettings::default(),
                chain(vec![folder(
                    vec![Header::new("X-Env", "folder")],
                    Some(bearer("from-folder")),
                )]),
            );
            let mut input = input();
            input.request_path = Some(format!("{FLOW_INLINE_PATH_PREFIX}node-1"));
            input.auth = Auth::Inherit;

            let resolved = resolve(&svc, &input);

            assert!(enabled_values(&resolved, "X-Env").is_empty());
            // `merge_auth` turns `inherit` with no collection auth into `none`.
            assert_eq!(resolved.auth, Auth::None);
        }
```

- [ ] **Step 4: Run the tests and watch them fail**

Run: `cargo test -j4 -p rocket-app folder_`
Expected: the test binary does not compile, with `cannot find value FLOW_INLINE_PATH_PREFIX in this scope`. All three tests share one test binary, so none runs yet.

To see the WebSocket failure on its own, comment out the inline Flow test for a moment and run `cargo test -j4 -p rocket-app folder_headers_and_auth_apply_between_collection_and_request`. Expected: FAIL, `X-Env` is `Some("collection")`, not `Some("chat.example.com")`. In the same state, `cargo test -j4 -p rocket-app a_runner_step_inherits_folder_auth` is expected to PASS already, because the runner goes through `begin_phases` → `resolve_request`, fixed in Task 1. This test locks that in. If it fails, the runner bypasses `resolve_request`; stop and report. Restore the inline Flow test before Step 5.

- [ ] **Step 5: Route WebSocket through the shared method and add the Flow guard**

In `websocket_resolution.rs`, change line 18 to:

```rust
use super::{resolve_auth, RequestExecutionService};
```

Replace lines 154-164 (from `let request_auth = ...` through `let auth = resolve_auth(auth, &vars);`) with:

```rust
        let request_auth = input.auth.clone().unwrap_or(Auth::None);
        let folders =
            self.folder_chain(scope.collection.as_deref(), scope.request_path.as_deref())?;
        let (auth, headers) = self.inherited_auth_and_headers(
            scope.collection.as_deref(),
            &folders,
            request_auth,
            &input.headers,
        );
        let auth = resolve_auth(auth, &vars);
```

In `execution_service.rs`, after `pub mod websocket_resolution;` (line 27), add:

```rust
/// Request path prefix of an inline Flow request. It names no file, so it has no folder chain.
pub(crate) const FLOW_INLINE_PATH_PREFIX: &str = "__flow_inline__/";
```

In `folder_chain` (Task 1), add a guard arm before the `(Some(col), Some(path))` arm:

```rust
        match (collection, request_path) {
            (Some(_), Some(path)) if path.starts_with(FLOW_INLINE_PATH_PREFIX) => Ok(Vec::new()),
            (Some(col), Some(path)) => self.collection_repo.get_folder_chain_settings(col, path),
            _ => Ok(Vec::new()),
        }
```

In `flow_execution_service.rs`, change the import at lines 11-13 to:

```rust
use crate::execution_service::{
    ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService, FLOW_INLINE_PATH_PREFIX,
};
```

and line 489 to:

```rust
            format!("{FLOW_INLINE_PATH_PREFIX}{}", node.id),
```

- [ ] **Step 6: Update the crate guide**

In `crates/rocket-app/CLAUDE.md`, replace the bullet that starts with `- **Header and auth merging.**` with:

```markdown
- **Header and auth merging.** `RequestExecutionService::inherited_auth_and_headers` is the one place defaults apply: collection headers, then the folder chain (`folder_chain` → `get_folder_chain_settings`, outermost first, merged by `rocket_collection::inherited_headers`), then the request. The request wins by key and a disabled header never shadows. A request auth of `none` or `inherit` takes the nearest folder auth (`resolve_folder_auth`), then the collection auth. `resolve_request` (single send, runner, Flow, load test, GraphQL introspection) and `resolve_websocket` (WebSocket, GraphQL subscriptions) both call it. A folder chain that fails to load fails the send. Inline Flow requests (`FLOW_INLINE_PATH_PREFIX`) inherit no folder settings. gRPC still applies collection defaults only, because `GrpcExecuteInput` carries no request path.
```

- [ ] **Step 7: Run the tests and watch them pass**

Run: `cargo test -j4 -p rocket-app folder_`
Expected: all pass (the WebSocket test, the inline Flow test and the Task 1 module).

Run: `cargo test -j4 -p rocket-app a_runner_step_inherits_folder_auth`
Expected: PASS.

Run: `cargo test -j4 -p rocket-app websocket_resolution`, then `cargo test -j4 -p rocket-app flow_execution_service`, then `cargo test -j4 -p rocket-app collection_runner_service`
Expected: all pass.

Run: `cargo check -j4 -p rocket-app --tests`
Expected: no warnings. An `unused import: merge_auth` warning means Step 5's import change was missed.

- [ ] **Step 8: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage these paths explicitly and commit with a pathspec:

```bash
git add crates/rocket-app/src/execution_service.rs crates/rocket-app/src/execution_service/websocket_resolution.rs crates/rocket-app/src/test_doubles.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/collection_runner_service.rs crates/rocket-app/CLAUDE.md
git commit --only -m "feat(app): apply folder headers and auth to WebSocket, runner and Flow sends" -- crates/rocket-app/src/execution_service.rs crates/rocket-app/src/execution_service/websocket_resolution.rs crates/rocket-app/src/test_doubles.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/collection_runner_service.rs crates/rocket-app/CLAUDE.md
```

---

## Task 3: The frontend send path honors folder headers and auth

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Why this is needed:** `resolveRequestFieldsForPath` does not call a backend resolve path. It merges the collection headers into the request's own header list and swaps `inherit` for the collection auth from `useCollectionAuthStore` (`execute-request.ts` lines 315-330). The backend then sees the collection header as a request header, and a request header beats a folder header. Without this task the Send button would send `X-Env: collection` where the backend rule says `X-Env: folder`, and the collection auth where the folder auth belongs.

**Files:**
- Create: `src/lib/folder-inheritance.ts`
- Create: `src/lib/__tests__/folder-inheritance.test.ts`
- Modify: `src/lib/execute-request.ts` (imports at lines 1-29, collection settings block at lines 244-254, auth and header merge at lines 315-330)
- Create: `src/lib/__tests__/execute-request.folder.test.ts`

**Interfaces:**
- Consumes (plan 04, locked): `getFolderSettings(collection: string, folderPath: string): Promise<FolderSettings>` and the TS type `FolderSettings` from `src/lib/tauri-api.ts`. This task reads only `headers` and `auth` from it. Existing: `fromPersistedAuth(auth: Auth | null | undefined, fallbackAuthType?: 'none' | 'inherit'): AuthState` from `src/lib/persisted-auth.ts`; `toApiAuth(auth: AuthState, resolve?): Auth`; types `Auth`, `Header` from `src/lib/tauri-api.ts`.
- Produces (`src/lib/folder-inheritance.ts`):
  - `ancestorFolderPaths(requestPath: string): string[]`
  - `loadFolderChain(collection: string, requestPath: string): Promise<FolderSettings[]>`
  - `inheritedHeaders(collection: Header[], folders: FolderSettings[]): Header[]`
  - `resolveFolderAuth(folders: FolderSettings[]): Auth | undefined`

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`, the `Auth` and `inherit` sections. Confirm plan 04 is merged: `grep -n "export const getFolderSettings\|export interface FolderSettings" src/lib/tauri-api.ts` must print two lines.

- [ ] **Step 2: Write the failing helper tests**

Create `src/lib/__tests__/folder-inheritance.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';

const folders = vi.hoisted(() => ({ byPath: new Map<string, unknown>() }));

vi.mock('@/lib/tauri-api', () => ({
  getFolderSettings: vi.fn(async (_collection: string, folderPath: string) => {
    if (!folders.byPath.has(folderPath)) throw new Error(`cannot read ${folderPath}/folder.yml`);
    return folders.byPath.get(folderPath);
  }),
}));

import {
  ancestorFolderPaths,
  inheritedHeaders,
  loadFolderChain,
  resolveFolderAuth,
} from '@/lib/folder-inheritance';
import { getFolderSettings } from '@/lib/tauri-api';

function folder(partial: Partial<FolderSettings>): FolderSettings {
  return { headers: [], variables: [], ...partial } as FolderSettings;
}

const h = (key: string, value: string, enabled = true) => ({ key, value, enabled });

describe('ancestorFolderPaths', () => {
  it('lists every folder above the request, outermost first', () => {
    expect(ancestorFolderPaths('users/admin/get.yml')).toEqual(['users', 'users/admin']);
  });

  it('gives no folders for a request at the collection root', () => {
    expect(ancestorFolderPaths('get.yml')).toEqual([]);
  });

  it('accepts Windows separators', () => {
    expect(ancestorFolderPaths('users\\admin\\get.yml')).toEqual(['users', 'users/admin']);
  });
});

describe('inheritedHeaders', () => {
  it('lets a folder header replace a collection header by key, ignoring case', () => {
    const merged = inheritedHeaders(
      [h('X-Env', 'collection'), h('X-Team', 'core')],
      [folder({ headers: [h('x-env', 'folder')] })],
    );
    expect(merged).toEqual([h('X-Team', 'core'), h('x-env', 'folder')]);
  });

  it('lets the inner folder win over the outer folder', () => {
    const merged = inheritedHeaders(
      [],
      [
        folder({ headers: [h('X-Env', 'outer'), h('X-Outer', 'only')] }),
        folder({ headers: [h('X-Env', 'inner')] }),
      ],
    );
    expect(merged).toEqual([h('X-Outer', 'only'), h('X-Env', 'inner')]);
  });

  it('never lets a disabled header shadow or be sent', () => {
    const merged = inheritedHeaders(
      [h('X-Env', 'collection'), h('X-Off', 'collection', false)],
      [folder({ headers: [h('X-Env', 'folder-off', false)] })],
    );
    expect(merged).toEqual([h('X-Env', 'collection')]);
  });
});

describe('resolveFolderAuth', () => {
  it('returns the innermost folder auth that is not none or inherit', () => {
    const auth = resolveFolderAuth([
      folder({ auth: { authType: 'bearer', token: 'outer' } }),
      folder({ auth: { authType: 'bearer', token: 'inner' } }),
      folder({ auth: { authType: 'inherit' } }),
      folder({ auth: { authType: 'none' } }),
    ]);
    expect(auth).toEqual({ authType: 'bearer', token: 'inner' });
  });

  it('returns undefined when no folder sets auth', () => {
    expect(resolveFolderAuth([folder({}), folder({ auth: { authType: 'inherit' } })])).toBe(
      undefined,
    );
  });
});

describe('loadFolderChain', () => {
  beforeEach(() => {
    folders.byPath.clear();
    vi.clearAllMocks();
  });

  it('reads each ancestor folder outermost first and skips one that cannot be read', async () => {
    const outer = folder({ headers: [h('X-A', '1')] });
    folders.byPath.set('users', outer);

    const chain = await loadFolderChain('api', 'users/admin/get.yml');

    expect(chain).toEqual([outer]);
    expect(getFolderSettings).toHaveBeenCalledTimes(2);
    expect(getFolderSettings).toHaveBeenNthCalledWith(1, 'api', 'users');
    expect(getFolderSettings).toHaveBeenNthCalledWith(2, 'api', 'users/admin');
  });

  it('reads nothing for a request at the collection root', async () => {
    expect(await loadFolderChain('api', 'get.yml')).toEqual([]);
    expect(getFolderSettings).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run it and watch it fail**

Run: `yarn vitest run src/lib/__tests__/folder-inheritance.test.ts`
Expected: FAIL, `Failed to resolve import "@/lib/folder-inheritance"`.

- [ ] **Step 4: Write the helper module**

Create `src/lib/folder-inheritance.ts`:

```ts
import { type Auth, type FolderSettings, getFolderSettings, type Header } from '@/lib/tauri-api';

// Folder paths above a request, outermost first. `users/admin/get.yml` gives
// `['users', 'users/admin']`. A request at the collection root has none.
export function ancestorFolderPaths(requestPath: string): string[] {
  const segments = requestPath.split(/[\\/]/).filter((s) => s.length > 0);
  const paths: string[] = [];
  for (let i = 1; i < segments.length; i++) {
    paths.push(segments.slice(0, i).join('/'));
  }
  return paths;
}

// Loads the settings of every folder above a request, outermost first.
// A folder that cannot be read is skipped here. The backend loads the same
// chain at send time and reports a broken folder.yml as a request error.
export async function loadFolderChain(
  collection: string,
  requestPath: string,
): Promise<FolderSettings[]> {
  const chain: FolderSettings[] = [];
  for (const folderPath of ancestorFolderPaths(requestPath)) {
    try {
      chain.push(await getFolderSettings(collection, folderPath));
    } catch {
      // Skipped on purpose. The backend send reports this folder's error.
    }
  }
  return chain;
}

// Mirrors rocket_collection::inherited_headers. Collection headers come first,
// then each folder from outermost to innermost. A later level replaces an
// earlier header with the same name. Disabled headers never shadow and are
// dropped. Names compare without case, like the request merge below it; the
// Rust helper compares them exactly.
export function inheritedHeaders(collection: Header[], folders: FolderSettings[]): Header[] {
  let merged = collection.filter((h) => h.enabled);
  for (const folder of folders) {
    const own = (folder.headers ?? []).filter((h) => h.enabled);
    const keys = new Set(own.map((h) => h.key.toLowerCase()));
    merged = [...merged.filter((h) => !keys.has(h.key.toLowerCase())), ...own];
  }
  return merged;
}

// Mirrors rocket_collection::resolve_folder_auth: the innermost folder auth
// that is not `none` or `inherit`.
export function resolveFolderAuth(folders: FolderSettings[]): Auth | undefined {
  for (const folder of [...folders].reverse()) {
    const auth = folder.auth;
    if (auth && auth.authType !== 'none' && auth.authType !== 'inherit') return auth;
  }
  return undefined;
}
```

- [ ] **Step 5: Run it and watch it pass**

Run: `yarn vitest run src/lib/__tests__/folder-inheritance.test.ts`
Expected: 10 passed.

- [ ] **Step 6: Write the failing send-path tests**

Create `src/lib/__tests__/execute-request.folder.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import type { AuthState, RequestState } from '@/types/pane-types';

const state = vi.hoisted(() => ({
  folders: new Map<string, unknown>(),
  collectionAuth: undefined as unknown,
}));

vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn(async () => ({
    variables: [{ key: 'token', value: 'tok-123', initialValue: '', enabled: true, secret: false }],
    headers: [
      { key: 'X-Team', value: 'core', enabled: true },
      { key: 'X-Env', value: 'collection', enabled: true },
    ],
  })),
  getFolderChainVariables: vi.fn(async () => []),
  getRequestVariables: vi.fn(async () => []),
  getFolderSettings: vi.fn(async (_collection: string, folderPath: string) => {
    if (!state.folders.has(folderPath)) throw new Error(`cannot read ${folderPath}/folder.yml`);
    return state.folders.get(folderPath);
  }),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));

vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: {
    getState: () => ({
      getCollectionAuth: () => state.collectionAuth as AuthState | undefined,
    }),
  },
}));

vi.mock('@/lib/query-client', () => ({
  getQueryClient: () => ({ getQueryData: () => undefined }),
}));

import { resolveRequestFieldsForPath } from '@/lib/execute-request';

const PATH = 'users/admin/get.yml';

function folder(partial: Partial<FolderSettings>): FolderSettings {
  return { headers: [], variables: [], ...partial } as FolderSettings;
}

function request(overrides: Partial<RequestState> = {}): RequestState {
  return {
    requestType: 'http',
    method: 'GET',
    url: 'https://api.example.com/users',
    pathParams: [],
    queryParams: [],
    headers: [],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'inherit' },
    settings: {
      verifySsl: true,
      followRedirects: true,
      maxRedirects: 5,
      timeoutMs: 0,
      encodeUrl: true,
    },
    docs: null,
    tags: [],
    assertions: [],
    actions: [],
    ...overrides,
  };
}

const values = (headers: { key: string; value: string }[], key: string) =>
  headers.filter((h) => h.key.toLowerCase() === key.toLowerCase()).map((h) => h.value);

describe('resolveRequestFieldsForPath with folder settings', () => {
  beforeEach(() => {
    state.folders.clear();
    state.collectionAuth = undefined;
    vi.clearAllMocks();
  });

  it('lets a folder header beat a collection header and a request header beat the folder', async () => {
    state.folders.set(
      'users',
      folder({
        headers: [
          { key: 'X-Env', value: 'folder', enabled: true },
          { key: 'X-Trace', value: 'folder', enabled: true },
        ],
      }),
    );
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({ headers: [{ id: '1', key: 'X-Trace', value: 'request', enabled: true }] }),
    );
    expect(values(resolved.headers, 'X-Team')).toEqual(['core']);
    expect(values(resolved.headers, 'X-Env')).toEqual(['folder']);
    expect(values(resolved.headers, 'X-Trace')).toEqual(['request']);
  });

  it('lets the inner folder header win over the outer one', async () => {
    state.folders.set('users', folder({ headers: [{ key: 'X-Env', value: 'outer', enabled: true }] }));
    state.folders.set(
      'users/admin',
      folder({ headers: [{ key: 'X-Env', value: 'inner', enabled: true }] }),
    );
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(values(resolved.headers, 'X-Env')).toEqual(['inner']);
  });

  it('never lets a disabled folder header hide the collection header', async () => {
    state.folders.set(
      'users/admin',
      folder({ headers: [{ key: 'X-Env', value: 'off', enabled: false }] }),
    );
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(values(resolved.headers, 'X-Env')).toEqual(['collection']);
  });

  it('gives an inheriting request the nearest folder auth, with placeholders resolved', async () => {
    state.collectionAuth = { authType: 'bearer', bearer: { token: 'from-collection' } };
    state.folders.set(
      'users',
      folder({ auth: { authType: 'basic', username: 'outer', password: 'x' } }),
    );
    state.folders.set('users/admin', folder({ auth: { authType: 'bearer', token: '{{token}}' } }));
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(resolved.auth).toEqual({ authType: 'bearer', token: 'tok-123' });
  });

  it('falls back to the collection auth when no folder sets auth', async () => {
    state.collectionAuth = { authType: 'bearer', bearer: { token: 'from-collection' } };
    state.folders.set('users/admin', folder({ auth: { authType: 'inherit' } }));
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(resolved.auth).toEqual({ authType: 'bearer', token: 'from-collection' });
  });

  it('leaves an OAuth2 folder auth as inherit so the backend resolves it', async () => {
    state.collectionAuth = { authType: 'bearer', bearer: { token: 'from-collection' } };
    state.folders.set(
      'users',
      folder({
        auth: {
          authType: 'o-auth2',
          flow: 'client_credentials',
          accessTokenUrl: 'https://auth.example.com/token',
          credentials: { clientId: 'c', clientSecret: 's' },
        },
      }),
    );
    const resolved = await resolveRequestFieldsForPath('api', PATH, request());
    expect(resolved.auth).toEqual({ authType: 'inherit' });
  });

  it('keeps an explicit request auth over every folder', async () => {
    state.folders.set('users', folder({ auth: { authType: 'bearer', token: 'from-folder' } }));
    const resolved = await resolveRequestFieldsForPath(
      'api',
      PATH,
      request({ auth: { authType: 'bearer', bearer: { token: 'mine' } } }),
    );
    expect(resolved.auth).toEqual({ authType: 'bearer', token: 'mine' });
  });
});
```

- [ ] **Step 7: Run it and watch it fail**

Run: `yarn vitest run src/lib/__tests__/execute-request.folder.test.ts`
Expected: FAIL. The first test gets `['collection']` for `X-Env`. The nearest-folder-auth test gets `{ authType: 'bearer', token: 'from-collection' }`. The OAuth2 test gets the collection bearer. `falls back to the collection auth when no folder sets auth` and `keeps an explicit request auth over every folder` pass already; they guard the existing behavior.

- [ ] **Step 8: Wire the helpers into `resolveRequestFieldsForPath`**

In `src/lib/execute-request.ts`, add two imports in Biome's sorted position: after `import { dispatchSend, type ResolvedGraphQl } from '@/lib/dispatch-send';` add

```ts
import { inheritedHeaders, loadFolderChain, resolveFolderAuth } from '@/lib/folder-inheritance';
```

and after `import { findTabInTree } from '@/lib/pane-utils';` add

```ts
import { fromPersistedAuth } from '@/lib/persisted-auth';
```

Replace lines 244-254 (the `collectionVars` and `collectionHeaders` block) with:

```ts
  let collectionVars: CollectionVariable[] = [];
  let collectionHeaders: Header[] = [];
  if (collection) {
    try {
      const settings = await getCollectionSettings(collection);
      collectionVars = settings.variables;
      collectionHeaders = settings.headers;
    } catch {
      // Collection settings unavailable — proceed without collection vars/headers.
    }
  }

  // Folder headers and auth sit between the collection and the request.
  const folderChain =
    collection && requestPath ? await loadFolderChain(collection, requestPath) : [];
```

Replace lines 315-330 (from `let authToResolve: AuthState = request.auth;` through the closing `];` of `effectiveHeaders`) with:

```ts
  let authToResolve: AuthState = request.auth;
  if (request.auth.authType === 'inherit' && collection) {
    const folderAuth = resolveFolderAuth(folderChain);
    if (folderAuth) {
      // An OAuth2 folder auth stays `inherit`. The backend resolves it, and fetches a
      // client-credentials token at send time, exactly as on every other send path.
      const folderState = fromPersistedAuth(folderAuth);
      if (folderState.authType !== 'oauth2') authToResolve = folderState;
    } else {
      const storedAuth = useCollectionAuthStore.getState().getCollectionAuth(collection);
      if (storedAuth && storedAuth.authType !== 'none' && storedAuth.authType !== 'inherit') {
        authToResolve = storedAuth;
      }
    }
  }
  const resolvedAuth = toApiAuth(authToResolve, resolve);

  const requestHeaderKeys = new Set(resolvedHeaders.map((h) => h.key.toLowerCase()));
  const effectiveHeaders: Header[] = [
    ...inheritedHeaders(collectionHeaders, folderChain)
      .filter((h) => !requestHeaderKeys.has(h.key.toLowerCase()))
      .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: true })),
    ...resolvedHeaders,
  ];
```

- [ ] **Step 9: Run the tests and the checks**

Run: `yarn vitest run src/lib/__tests__/execute-request.folder.test.ts src/lib/__tests__/folder-inheritance.test.ts`
Expected: 17 passed.

Run: `yarn vitest run src/lib/__tests__/execute-request.test.ts src/lib/__tests__/execute-request.oauth.test.ts src/lib/__tests__/execute-request.collection.test.ts src/lib/__tests__/curl-generator.test.ts src/lib/__tests__/runner-execute.test.ts`
Expected: all pass. Their `tauri-api` mocks have no `getFolderSettings`. That is fine: their request paths have no folder, or the missing export throws inside `loadFolderChain`'s `try` and the folder is skipped.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no errors. If Biome reports import order, run `yarn format` and re-run `yarn check`.

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage these paths explicitly and commit with a pathspec:

```bash
git add src/lib/folder-inheritance.ts src/lib/__tests__/folder-inheritance.test.ts src/lib/execute-request.ts src/lib/__tests__/execute-request.folder.test.ts
git commit --only -m "feat(request): honor folder headers and auth on the frontend send path" -- src/lib/folder-inheritance.ts src/lib/__tests__/folder-inheritance.test.ts src/lib/execute-request.ts src/lib/__tests__/execute-request.folder.test.ts
```

---

## Known gaps left for later

- **gRPC.** `GrpcExecuteInput` has no request path, so gRPC calls still apply collection headers and auth only. A follow-up adds `request_path` to the gRPC input and routes `build_call` and `auth_metadata` through `inherited_auth_and_headers`.
- **Interactive OAuth2 on a folder.** A folder OAuth2 auth reaches the backend as is. The executor only fetches client-credentials tokens, so an authorization-code or implicit folder auth sends no token until a folder token store exists (plan 10 territory, like `useCollectionAuthStore` for collections).
- **Header name case.** The frontend compares header names without case, as today. `merge_headers` in Rust compares them with case, as today, and plan 01's `inherited_headers` does the same on purpose (plan 01 test `inherited_headers_key_match_is_exact_like_merge_headers`). So a collection `X-Env` and a folder `x-env` both reach the wire from the backend-only paths (runner, Flow, WebSocket), while the Send button sends only the folder one. This plan keeps both existing rules. Making them agree is a separate decision.

## Next Plan

[2026-10-07-folder-settings-plan-06-runtime-script-chain.md](2026-10-07-folder-settings-plan-06-runtime-script-chain.md): runtime script and test chain. It should load the chain once in `begin_phases` with `folder_chain`, keep it in `PhaseState`, and pass it to `resolve_request_with_chain`, so headers, auth and scripts share one read of the folder chain per execution.
