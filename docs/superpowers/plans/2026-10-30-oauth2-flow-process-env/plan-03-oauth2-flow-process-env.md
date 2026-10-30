# Plan 3: Inline OAuth2 resolution and the send scope

> **For agentic workers:** this plan is run by the factory driver. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** An inherited OAuth2 auth reaches the executor's inline token fetch with its `{{vars}}` resolved, a tab send resolves `{{process.env}}` in it under the tab's scope, executor errors are masked, and the trust doc names every path.

**Architecture:** `map_auth_fields` gains an `Auth::OAuth2` arm, so the send's existing template recording, runtime pass and placeholder probe cover OAuth2 fields. `ExecuteRequestInput` gets a `process_env` scope (serde default closed). For a tab send (`backend_process_env == false`) whose effective auth is OAuth2, only the OAuth2 fields resolve with that scope's process env. `send_request` masks executor errors with the send's secret values.

**Tech Stack:** Rust (rocket-app), TypeScript, Vitest.

**Spec:** docs/superpowers/specs/2026-10-30-oauth2-flow-process-env-design.md (section 5, Docs)

## Global Constraints

- Cargo is per crate: `cargo check -j4 -p rocket-app`, `cargo test -j4 -p rocket-app <filter>`. Never a bare workspace command.
- No panics in production paths. Tests use `.expect("...")` and `expect_err`.
- `rocket-infra` does not change. Masking happens in `rocket-app`.
- `processEnv` is required in the TypeScript `ExecuteRequestInput` and `#[serde(default)]` (closed) in Rust.
- The gate is decided once per send. No `.env` file is read when the effective auth is not OAuth2 on a tab send.
- The withheld line is the message of `process_env_withheld_entry()` (`execution_service.rs:378`).
- Frontend checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`.

## Review Focus

1. A tab send whose effective auth is not OAuth2 must read no `.env` file, even with a `processEnv` scope. Pinned by `tab_send_without_oauth2_reads_no_dot_env_file` (Task 2).
2. A History tab (closed scope) sending an inherited OAuth2 auth keeps the literal and warns once. Pinned by `tab_send_inherited_oauth2_closed_scope_keeps_literal_and_warns` (Task 2).
3. A tab send must still not resolve `{{process.env}}` in its URL, headers or body. The scope covers only the OAuth2 fields. Pinned by `tab_send_scope_never_reaches_non_auth_fields` (Task 2).
4. The OAuth2 arm must keep `placement`, settings and token config as they are. Pinned by `map_auth_fields_resolves_every_oauth2_field` (Task 1).
5. A backend send (`backend_process_env == true`) with inherited OAuth2 uses the send's own map and ignores `process_env` on the input. Pinned by `backend_send_inherited_oauth2_uses_the_send_map` (Task 2).

## File Structure

- Modify `crates/rocket-app/src/execution_service.rs`: `map_auth_fields` OAuth2 arm (`:3538-3595`), `send_request` (`:2771-2773`), `ExecuteRequestInput` (`:95-160`), `resolve_request_parts` (`:1449-1650`), tests.
- Modify `src/lib/tauri-api.ts:586`, `src/lib/execute-request.ts:766`, `src/lib/runner-execute.ts:40`, `src/lib/graphql-schema-input.ts:15`, and every other builder `yarn tsc --noEmit` names; `src/lib/__tests__/process-env-scope.test.ts`.
- Modify `docs/features/collection-trust.md`.

---

### Task 1: `map_auth_fields` covers OAuth2, and executor errors are masked

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:3532-3595`, `:2771-2773`, tests (including `oauth2_folder_auth_reaches_the_executor_unchanged`, `:6321`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `map_auth_fields(Auth::OAuth2(..), r)` applies `r` to, in this fixed order per variant: `access_token_url` or `authorization_url` then `access_token_url`, `refresh_token_url`, `callback_url`, `credentials.client_id`, `credentials.client_secret`, `resource_owner.username`, `resource_owner.password`, `scope`, `state`, then `name` and `value` of each parameter in `additional_parameters.authorization_request`, `access_token_request`, `refresh_token_request`. `send_request` returns executor errors with their text passed through `redact_secrets(text, &state.var_ctx.secret_values)`, same `DomainError` variant.

- [ ] **Step 1: Read the reference and the code.** Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `crates/rocket-shared/src/oauth2.rs:1-230`, `execution_service.rs:3460-3600`, `:2765-2790`, the folder auth tests module (`folder_service`, `chain`, `folder`, `input`, `resolve`, `:6116-6160`), `oauth2_folder_auth_reaches_the_executor_unchanged` (`:6321`), `FailingExecutor` and `capturing_svc` (`:9884-9910`), and `execution_service/runtime_fill.rs` to see how auth templates are mapped back.

- [ ] **Step 2: Write the failing tests.** Full example:

```rust
#[test]
fn map_auth_fields_resolves_every_oauth2_field() {
    use rocket_shared::oauth2::*;
    // Builds the same flow with every templated field run through `f`.
    let flow = |f: &dyn Fn(&str) -> String| OAuth2Flow::ResourceOwnerPassword {
        access_token_url: f("{{u}}/token"),
        refresh_token_url: Some(f("{{u}}/refresh")),
        credentials: OAuth2ClientCredentials { client_id: f("{{cid}}"), client_secret: f("{{cs}}"), placement: Some("basic_auth_header".into()) },
        resource_owner: Some(OAuth2ResourceOwner { username: f("{{user}}"), password: f("{{pw}}") }),
        scope: Some(f("{{scope}}")),
        additional_parameters: Some(OAuth2AdditionalParameters {
            authorization_request: None,
            access_token_request: Some(vec![OAuth2AdditionalParameter { name: f("{{k}}"), value: f("{{v}}"), placement: Some("body".into()), enabled: true }]),
            refresh_token_request: None,
        }),
        token_config: None,
        settings: None,
    };
    let swap = |s: &str| s.replace("{{", "<").replace("}}", ">");
    let out = map_auth_fields(Auth::OAuth2(Box::new(flow(&|s| s.to_string()))), |s| swap(&s));
    assert_eq!(out, Auth::OAuth2(Box::new(flow(&swap))));
}
```

Fill any struct fields the real types have that this example leaves out, with the same value on both sides.

| Test | Inputs | Assertion |
|---|---|---|
| `auth_has_placeholder_sees_oauth2_fields` | `client_credentials` flow with `client_secret = "{{cs}}"`, then with all literal fields | `true`, then `false` |
| `inherited_oauth2_resolves_vars_on_backend_send` | rename and rewrite `oauth2_folder_auth_reaches_the_executor_unchanged`: folder client-credentials auth with `client_id = "{{token}}"` | the resolved auth's `client_id == "jwt-from-env"` (the env value in `folder_service`) |
| `executor_error_is_masked` | a `FailingExecutor` variant returning `Internal("Token endpoint returned 400: s3cret-value-99")`, a secret environment variable `cs = "s3cret-value-99"` used in the URL | `execute` error text has no `s3cret-value-99` |

- [ ] **Step 3: Run them.** `cargo test -j4 -p rocket-app map_auth_fields_resolves auth_has_placeholder inherited_oauth2 executor_error_is_masked` — expected FAIL: the OAuth2 fields keep `{{...}}`, and the error shows the secret.

- [ ] **Step 4: Implement.** Add the `Auth::OAuth2` arm in the order above, leaving every non-string and placement field as it is. Map the error in `send_request` as described. Check that the runtime pass in `runtime_fill.rs` still maps auth templates back through `map_auth_fields` in the same order; no other change should be needed there.

- [ ] **Step 5: Run.** `cargo test -j4 -p rocket-app execution_service` — expected PASS, the 4 tests plus every existing execution test.

### Task 2: The send's process env scope reaches inherited OAuth2

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:95-160`, `:1449-1650`, tests

**Interfaces:**
- Consumes: Task 1's OAuth2 arm; `crate::process_env::ProcessEnvScopeInput` (plan 1); `process_env_for_scope`; `SendProcessEnv` (`:396`).
- Produces: `ExecuteRequestInput.process_env: ProcessEnvScopeInput` with `#[serde(default)]`, serialized as `processEnv`. Every Rust constructor of `ExecuteRequestInput` sets it to `ProcessEnvScopeInput::Closed` (or `..Default::default()` where used).

- [ ] **Step 1: Read the reference and the code.** Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `execution_service.rs:95-160`, `:396-500`, `:1449-1660`, the fixtures `process_env_svc` and `process_env_svc_with` (`:12844-12880`), `backend_input`, `sent_url`, `withheld_count` (`:12970`) and `frontend_send_without_a_collection_keeps_a_process_env_placeholder` (`:13039`).

- [ ] **Step 2: Write the failing tests.** Add `process_env_svc_with_folder_auth(auth: Auth, pairs, allow)`, built like `process_env_svc_with`, with `StubCollectionRepo::empty().with_folder_chain(vec![FolderSettings { auth: Some(auth), ..FolderSettings::default() }])`. Use a client-credentials auth with `client_secret = "{{process.env.ROCKET_T1_KEY}}"` and pair `("ROCKET_T1_KEY", "abc123secret")`. Read the resolved auth from `svc.resolve_request(&input, &HashMap::new())`. A tab input is `sample_input(url, Some("dev"))` with `collection = Some("api")`, `request_path = Some("users/list")`, `auth = Auth::Inherit`, `backend_process_env = false`. Full example:

```rust
#[tokio::test]
async fn tab_send_inherited_oauth2_resolves_process_env_with_scope() {
    let (svc, _, _, fake) = process_env_svc_with_folder_auth(oauth_cc("{{process.env.ROCKET_T1_KEY}}"), &[("ROCKET_T1_KEY", "abc123secret")], true);
    let mut input = tab_input();
    input.process_env = ProcessEnvScopeInput::Collection { name: "api".into(), environment_name: Some("dev".into()) };
    let resolved = svc.resolve_request(&input, &HashMap::new()).expect("resolve");
    assert_eq!(client_secret_of(&resolved.auth), "abc123secret");
    assert!(fake.call_count() + fake.default_call_count() > 0);
}
```

| Test | Setup | Assertion |
|---|---|---|
| `tab_send_inherited_oauth2_closed_scope_keeps_literal_and_warns` | scope `Closed`, run with `svc.execute(input)` against `MockExecutor` | `client_secret` stays `"{{process.env.ROCKET_T1_KEY}}"` (check through `resolve_request`); `withheld_count(&output.console_entries) == 1`; 0 file reads |
| `tab_send_without_oauth2_reads_no_dot_env_file` | folder auth `Auth::Bearer { token: "x" }`, scope `Collection` | `fake.call_count() + fake.default_call_count() == 0` |
| `tab_send_scope_never_reaches_non_auth_fields` | scope `Collection`, URL `https://x.test/?p={{process.env.ROCKET_T1_KEY}}` | sent URL keeps the placeholder; `client_secret == "abc123secret"` |
| `backend_send_inherited_oauth2_uses_the_send_map` | `backend_process_env = true`, scope `Closed` | `client_secret == "abc123secret"` |
| `execute_request_input_defaults_to_closed_scope` | `serde_json::from_value::<ExecuteRequestInput>` of a minimal JSON input without `processEnv`, then with `{"kind":"scratch"}` | `Closed`, then `Scratch` |
| `tab_send_masks_dot_env_value_of_oauth2` | scope `Collection`, run with `svc.execute(input)` | the saved History entry contains no `abc123secret` |

`oauth_cc(secret)` and `client_secret_of(&Auth)` are new small test helpers in the same module.

- [ ] **Step 3: Run them.** `cargo test -j4 -p rocket-app tab_send_ backend_send_inherited execute_request_input_defaults` — expected FAIL: `no field 'process_env' on type 'ExecuteRequestInput'`.

- [ ] **Step 4: Implement.** Add the field. In `resolve_request_parts`, after `inherited_auth_and_headers`, when `!input.backend_process_env` and the effective auth is `Auth::OAuth2`, compute `auth_env = self.process_env_for_scope(collection, environment, global, &input.process_env)` once and build `auth_vars` = `vars` plus `auth_env.vars` as the lowest layer. The auth closure records OAuth2 templates against `auth_vars`; every other field keeps `vars`. Add `auth_env`'s `.env` values to `SendProcessEnv.dot_env_values`, its used names' values to `used_values`, and its referenced names to `withheld_names` when `!auth_env.allowed`. Make the withheld warning fire for a tab send only from these OAuth2 names (the frontend still owns every other field). Set the new field in every Rust constructor the compiler names.

- [ ] **Step 5: Run.** `cargo test -j4 -p rocket-app execution_service` and `cargo test -j4 -p rocket-app process_env` — expected PASS, the 7 new tests plus every existing one.

### Task 3: The frontend sends the scope, and the trust doc is updated

**Files:**
- Modify: `src/lib/tauri-api.ts:586`, `src/lib/execute-request.ts:766`, `src/lib/runner-execute.ts:40`, `src/lib/graphql-schema-input.ts:15`, other builders tsc names
- Test: `src/lib/__tests__/process-env-scope.test.ts`
- Modify: `docs/features/collection-trust.md:28-60`

**Interfaces:**
- Consumes: Task 2's `processEnv` field; `toProcessEnvScopeInput`, `processEnvScopeOf`, `CLOSED_PROCESS_ENV`.
- Produces: TS `ExecuteRequestInput.processEnv: ProcessEnvScopeInput` (required).

- [ ] **Step 1: Read the code.** Read `src/lib/process-env-gate.ts:1-40`, `src/lib/execute-request.ts:100-125`, `:700-800`, `src/lib/runner-execute.ts:20-60`, `src/lib/graphql-schema-input.ts`, `src/lib/__tests__/process-env-scope.test.ts`, and `docs/features/collection-trust.md`.

- [ ] **Step 2: Write the failing tests** in `process-env-scope.test.ts`, following its `openTab` and `sendRequest` pattern:

```ts
it('sends the tab collection scope with the request', async () => {
  openTab({ id: 't1', source: { collection: 'api', path: 'users/list' } });
  await sendRequest('t1');
  const input = vi.mocked(tauriApi.executeRequest).mock.calls.at(-1)?.[0];
  expect(input?.processEnv).toEqual({ kind: 'collection', name: 'api' });
});
```

Add `it('sends a closed scope for a History tab with no collection')`: a tab with `historyCollection` unset and no `source`, marked as a History tab the way the existing History tests in this file do, gives `{ kind: 'closed' }`.

- [ ] **Step 3: Run them.** `yarn test process-env-scope` — expected FAIL: `processEnv` is `undefined`.

- [ ] **Step 4: Implement.** Add the required TS field. `execute-request.ts` sets `processEnv: toProcessEnvScopeInput(processEnvScope)` from the scope it already computes (`:725`). The runner sets `{ kind: 'collection', name: collection }`. GraphQL introspection and the load test take the tab's scope when they have the tab, else `{ kind: 'closed' }`. In `docs/features/collection-trust.md`, remove the Known limits line about Flow Input nodes, Auth-node fetches and `OAuth2Service` (`:55`). Under "Host environment", add one bullet each: Flow Input nodes and Auth-node fetches use one gate decision per run with `.env` values masked; OAuth2 token and refresh requests resolve in the backend under the request's `processEnv` scope (closed by default); an inherited OAuth2 auth on a tab send resolves under the send's `processEnv` scope, and executor errors are masked.

- [ ] **Step 5: Run.** `yarn test process-env-scope execute-request` — expected PASS. Then `yarn tsc --noEmit` (0 errors), `yarn check` (clean), `cargo check -j4 -p rocket-app`.

Next plan: none
