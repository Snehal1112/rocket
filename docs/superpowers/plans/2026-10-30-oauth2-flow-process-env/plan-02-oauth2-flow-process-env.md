# Plan 2: `OAuth2Service` token and refresh requests

> **For agentic workers:** this plan is run by the factory driver. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Request-tab OAuth2 token and refresh requests resolve `{{process.env}}` in the backend under the tab's process env scope, with `.env` values masked in errors, while the Tauri commands stay thin.

**Architecture:** The OAuth2 IPC DTOs carry a `processEnv` scope (serde default closed). `OAuth2Service` gets `_with_host_env` methods that take a `ProcessEnv`, resolve with it as the lowest layer and mask their own errors. The commands add one `process_env_for_scope` service call next to the vault one. The frontend sends the scope and keeps its own resolution pass.

**Tech Stack:** Rust (rocket-app, src-tauri), wiremock 0.6, TypeScript, React, Vitest.

**Spec:** docs/superpowers/specs/2026-10-30-oauth2-flow-process-env-design.md (section 4)

## Global Constraints

- Cargo is per crate: `cargo check -j4 -p rocket-app`, `cargo test -j4 -p rocket-app <filter>`, `cargo check -j4 -p rocket` for `src-tauri`. Never a bare workspace command.
- No panics in production paths. Tests use `.expect("...")` and `expect_err`.
- `src-tauri` commands do no resolution and no masking. They call services and pass results on.
- `processEnv` is required in the TypeScript types and `#[serde(default)]` (closed) in Rust.
- Serde `camelCase` only on IPC DTOs. The OAuth2 DTOs already have it.
- UI uses shadcn primitives only; no new raw elements. Zustand selectors stay narrow.
- Frontend checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`.

## Review Focus

1. A request with no `processEnv` field over IPC must get no host environment, even when its `collection` is trusted. Pinned by `oauth2_get_token_request_defaults_to_closed_scope` (Task 1); the closed scope giving nothing for a trusted collection is already pinned by `closed_scope_gives_no_host_environment_even_for_a_trusted_collection` (`websocket_resolution.rs:712`).
2. A collection or environment variable with the same name as a process env key must win over the process env value. Pinned by `oauth2_service_resolves_process_env_map_as_lowest_layer` (Task 1).
3. A token endpoint that echoes the client secret in a 400 body must not show the `.env` value in the returned error. Pinned by `oauth2_get_token_error_masks_dot_env_value` (Task 1).
4. The token editor in folder settings or the Flow Auth node (no tab) must send a collection scope, and one with no collection must send closed. Pinned by `oauth2_editor_scope_falls_back_to_collection_or_closed` (Task 2).
5. The refresh path must use the same rule as get-token. Pinned by `oauth2_refresh_resolves_process_env_map` (Task 1).

## File Structure

- Modify `crates/rocket-app/src/oauth2_service.rs`: DTO fields, `build_variable_context` parameter, `_with_host_env` methods, tests.
- Modify `src-tauri/src/commands/oauth2.rs:265-318`: one extra service call per command.
- Modify `src/lib/tauri-api.ts:1569-1620`, `src/lib/oauth2-requests.ts`, `src/lib/execute-request.ts:585-660`, `src/lib/flow-auth-preflight.ts:90-104`, `src/components/request/AuthEditor.tsx`, `src/components/request/RequestPanel.tsx:1332`, `src/components/request/oauth2/OAuth2AuthEditor.tsx`, and their tests.

---

### Task 1: `OAuth2Service` resolves and masks with a `ProcessEnv`

**Files:**
- Modify: `crates/rocket-app/src/oauth2_service.rs:24-100`, `:265-331`, `:410-470`, `:575-625`, tests

**Interfaces:**
- Consumes: `crate::process_env::{ProcessEnv, ProcessEnvScopeInput}` (plan 1, Task 1).
- Produces:
  - `OAuth2GetTokenRequest.process_env: ProcessEnvScopeInput` and `OAuth2RefreshRequest.process_env: ProcessEnvScopeInput`, both `#[serde(default)]`.
  - `pub(crate) fn build_variable_context(&self, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>, external_secrets: &HashMap<String, String>, process_env: &HashMap<String, String>) -> HashMap<String, String>`.
  - `pub fn resolve_get_token_request_with_host_env(&self, req: &OAuth2GetTokenRequest, external_secrets: &HashMap<String, String>, process_env: &ProcessEnv) -> ResolvedOAuth2Config`.
  - `pub async fn get_token_with_host_env(&self, config: &ResolvedOAuth2Config, process_env: &ProcessEnv) -> DomainResult<OAuthToken>`.
  - `pub async fn refresh_token_with_host_env(&self, req: &OAuth2RefreshRequest, external_secrets: &HashMap<String, String>, process_env: &ProcessEnv) -> DomainResult<OAuthToken>`.
  - `resolve_get_token_request_with_secrets` and `refresh_token_with_secrets` keep their signatures and delegate with `&ProcessEnv::default()`.

- [ ] **Step 1: Read the reference and the code.** Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `oauth2_service.rs:1-130`, `:265-470`, `:575-640`, the tests `make_service_with_env` (`:702`) and `resolve_replaces_variables_in_all_fields`, `crate::redaction::redact_secrets` (`redaction.rs:16`), and an existing wiremock test in `crates/rocket-app/src/graphql_schema.rs` for the `MockServer` setup.

- [ ] **Step 2: Write the failing tests** in the `oauth2_service.rs` tests module. Build `ProcessEnv { vars, dot_env_keys, allowed: true }` by hand. Full example:

```rust
#[tokio::test]
async fn oauth2_get_token_error_masks_dot_env_value() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(400).set_body_string("bad secret dotenv-secret-654321"))
        .mount(&server)
        .await;
    let svc = make_service_with_env(&[]);
    let mut req = client_credentials_request(&format!("{}/token", server.uri()));
    req.client_secret = Some("{{process.env.ROCKET_CS_KEY}}".into());
    let env = ProcessEnv {
        vars: HashMap::from([("ROCKET_CS_KEY".to_string(), "dotenv-secret-654321".to_string())]),
        dot_env_keys: vec!["ROCKET_CS_KEY".to_string()],
        allowed: true,
    };
    let config = svc.resolve_get_token_request_with_host_env(&req, &HashMap::new(), &env);
    assert_eq!(config.client_secret, "dotenv-secret-654321");
    let err = svc.get_token_with_host_env(&config, &env).await.expect_err("400");
    assert!(!err.to_string().contains("dotenv-secret-654321"), "{err}");
}
```

`client_credentials_request(token_url)` is a new test helper that builds `OAuth2GetTokenRequest` as `resolve_replaces_variables_in_all_fields` does, grant `client_credentials`, `..` the new `process_env: Default::default()`.

| Test | Inputs | Assertion |
|---|---|---|
| `oauth2_service_resolves_process_env_map_as_lowest_layer` | env var `my_secret = env-wins`; process env `{my_secret: host, ROCKET_X: hostx}`; `client_secret = "{{my_secret}}"`, `scope = "{{process.env.ROCKET_X}}"` | `client_secret == "env-wins"`, `scope == "hostx"`; with `ProcessEnv::default()` the scope stays `"{{process.env.ROCKET_X}}"` |
| `oauth2_get_token_request_defaults_to_closed_scope` (stands in for the spec's `oauth2_dto_reads_the_process_env_scope`: the DTO is defined in `rocket-app`, and `src-tauri` tests build the whole app) | `serde_json::from_value` of a JSON get-token and a JSON refresh request without `processEnv` | both `process_env == ProcessEnvScopeInput::Closed`; with `{"kind":"scratch"}` it is `Scratch` |
| `oauth2_refresh_resolves_process_env_map` | wiremock 200 `{"access_token":"t","token_type":"bearer"}` that captures the form; refresh `client_secret = "{{process.env.ROCKET_CS_KEY}}"` | the received form has `client_secret=dotenv-secret-654321` |

- [ ] **Step 3: Run them.** `cargo test -j4 -p rocket-app oauth2_` — expected FAIL: `no method named 'resolve_get_token_request_with_host_env'` and `no field 'process_env'`.

- [ ] **Step 4: Implement.** Add the DTO fields. Give `build_variable_context` the `process_env` parameter and set `ctx.process_env`, flattening with `flatten_with_process_env()`. Update every caller, tests included, with `&HashMap::new()` where no map applies. Add the three `_with_host_env` methods; the two async ones map an error's text through `redact_secrets` with the `.env` values of `process_env` (values of `dot_env_keys`, at least `MIN_REDACTION_LEN` long), keeping the error variant. `OAuth2ServiceFetcher::fetch_token` keeps calling the `_with_secrets` form.

- [ ] **Step 5: Run.** `cargo test -j4 -p rocket-app oauth2` and `cargo test -j4 -p rocket-app flow_auth` — expected PASS, the 4 new tests plus every existing OAuth2 and flow_auth test.

### Task 2: Commands and the frontend send the scope

**Files:**
- Modify: `src-tauri/src/commands/oauth2.rs:265-318`
- Modify: `src/lib/tauri-api.ts:1569-1620`, `src/lib/oauth2-requests.ts`, `src/lib/execute-request.ts:585-660`, `src/lib/flow-auth-preflight.ts:90-104`, `src/components/request/AuthEditor.tsx`, `src/components/request/RequestPanel.tsx:1332-1340`, `src/components/request/oauth2/OAuth2AuthEditor.tsx:50-130`
- Test: `src/lib/__tests__/oauth2-requests.test.ts`, `src/components/request/oauth2/__tests__/OAuth2AuthEditor.test.tsx`

**Interfaces:**
- Consumes: Task 1's `_with_host_env` methods and DTO fields; `RequestExecutionService::process_env_for_scope`; `toProcessEnvScopeInput`, `processEnvScopeOf`, `CLOSED_PROCESS_ENV`, `ProcessEnvScope`, `ProcessEnvScopeInput` from `src/lib/process-env-gate.ts` and `src/lib/execute-request.ts:110`.
- Produces: TS `OAuth2GetTokenRequest.processEnv: ProcessEnvScopeInput` and `OAuth2RefreshRequest.processEnv: ProcessEnvScopeInput` (required); `OAuth2Target.processEnv: ProcessEnvScopeInput` (required); `OAuth2AuthEditor` and `AuthEditor` optional prop `processEnvScope?: ProcessEnvScope`; exported helper `oauth2EditorScope(scope: ProcessEnvScope | undefined, collection: string | undefined, hasScopeProp: boolean): ProcessEnvScopeInput` in `OAuth2AuthEditor.tsx` or `oauth2-requests.ts`.

- [ ] **Step 1: Read the code.** Read `src-tauri/src/commands/oauth2.rs:255-320`, `src/lib/process-env-gate.ts:1-40`, `src/lib/oauth2-requests.ts`, `src/lib/execute-request.ts:100-125` and `:580-660`, `src/lib/flow-auth-preflight.ts:85-175`, `src/components/request/oauth2/OAuth2AuthEditor.tsx:40-200`, `src/components/request/AuthEditor.tsx`, and the two test files above.

- [ ] **Step 2: Write the failing tests.** In `oauth2-requests.test.ts`, give `target` a `processEnv: { kind: 'collection', name: 'api', environmentName: 'dev' }` and add:

```ts
it('carries the process env scope on get-token and refresh requests', () => {
  expect(buildGetTokenRequest(oauthState(), rv, target).processEnv).toEqual({ kind: 'collection', name: 'api', environmentName: 'dev' });
  expect(buildRefreshRequest(oauthState(), rv, target).processEnv).toEqual({ kind: 'collection', name: 'api', environmentName: 'dev' });
});
```

In `OAuth2AuthEditor.test.tsx` add `oauth2_editor_scope_falls_back_to_collection_or_closed` as a `describe` of `oauth2EditorScope`: `(undefined, 'api', false)` gives `{ kind: 'collection', name: 'api' }`; `(undefined, undefined, false)` gives `{ kind: 'closed' }`; `(CLOSED_PROCESS_ENV, 'api', true)` gives `{ kind: 'closed' }`; `(undefined, undefined, true)` gives `{ kind: 'scratch' }`. Stub `useEnvStore` the way `process-env-scope.test.ts` does, so the collection case carries no environment.

- [ ] **Step 3: Run them.** `yarn test oauth2-requests OAuth2AuthEditor` — expected FAIL: `processEnv` is `undefined`, and `oauth2EditorScope` is not exported.

- [ ] **Step 4: Implement.** In each command, after the vault call, add `let host_env = exec.process_env_for_scope(request.collection.as_deref(), request.environment_name.as_deref(), None, &request.process_env);` and call the `_with_host_env` methods (`get_token_direct` becomes `get_token_with_host_env(&config, &host_env)`; the authorization-code and implicit arms keep using `config`). Add the TS fields, the `OAuth2Target.processEnv` field and its use in both builders. `maybeAutoRefreshOrFetchToken` passes `toProcessEnvScopeInput(processEnvScopeOf(tab))` for its tab (thread the scope from the caller at `:725`). `flow-auth-preflight.ts` `resolveEnv` passes `{ kind: 'collection', name: scope.collection, environmentName: scope.environmentName }`. Add `oauth2EditorScope`: with the prop it is `toProcessEnvScopeInput(scope)`, without it `collection ? toProcessEnvScopeInput(collection) : { kind: 'closed' }`. `RequestPanel` passes `processEnvScope={processEnvScope}` (the value at `:570`) through `AuthEditor`. Fix every other caller `yarn tsc --noEmit` names.

- [ ] **Step 5: Run.** `yarn test oauth2-requests OAuth2AuthEditor execute-request flow-auth` — expected PASS. Then `yarn tsc --noEmit` (0 errors), `yarn check` (clean), `cargo check -j4 -p rocket` (no errors).

Next plan: docs/superpowers/plans/2026-10-30-oauth2-flow-process-env/plan-03-oauth2-flow-process-env.md
