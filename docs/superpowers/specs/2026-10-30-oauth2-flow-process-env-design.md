# OAuth2 token fetches and Flow Input nodes resolve `{{process.env}}`

Ticket: https://github.com/Snehal1112/rocket/issues/132 (follow-up of #112)

## Goal

`{{process.env.NAME}}` resolves in Flow Input nodes, Flow Auth-node token fetches, request-tab OAuth2 token and refresh requests, and the backend's inline OAuth2 fetch, behind the collection trust gate, with `.env` values masked as secrets.

## Overview

Users keep OAuth2 client secrets in a collection `.env` file. Since #112 a backend send resolves `{{process.env.CLIENT_SECRET}}`, but the token request in front of it does not, so a request works and its token fetch sends the literal braces. Four places still build a variable map with no host environment. Bruno resolves every OAuth2 field in its backend with the same variable map as a send (`packages/bruno-electron/src/ipc/network/interpolate-vars.js:265-360`). Rocket does the same and keeps its trust gate and `.env` masking.

## Current state

| Path | Today | Where |
|---|---|---|
| Flow Input node | `build_variable_context` plus callback vars, no process env. Step masks are `secret_values` plus `secret_forms`, no `.env` values | `crates/rocket-app/src/flow_execution_service.rs:1636-1668` |
| Flow Auth-node fetch | `FetchContext.vars` from `build_variable_context`, once per run. `build_get_token_request` resolves every field with it. It also feeds `static_wire_snapshot` | `flow_execution_service.rs:1168-1179`, `flow_auth.rs:207`, `:278`, `:304-319` |
| `OAuth2Service` | Own `build_variable_context` (collection, env, folder, request), no process env, no trust store, no `.env` loader | `crates/rocket-app/src/oauth2_service.rs:265-331`, `:435`, `:581` |
| OAuth2 IPC | `oauth2_get_token`, `oauth2_refresh_token` pass vault secrets only. The frontend fills every field first, process env included | `src-tauri/src/commands/oauth2.rs:265-318`, `src/lib/oauth2-requests.ts`, `src/lib/execute-request.ts:675-705`, `src/components/request/oauth2/OAuth2AuthEditor.tsx:73-120` |
| Inline OAuth2 | `map_auth_fields` returns `Auth::OAuth2` untouched, so the executor's client-credentials and password fetch gets raw `{{...}}` | `crates/rocket-app/src/execution_service.rs:3532-3595`, `crates/rocket-infra/src/reqwest_executor.rs:1055-1120` |
| Tab send with folder OAuth2 and no token | The frontend sends `inherit`, the backend fetches inline. IPC sends never read process env (`backend_process_env` is `#[serde(skip)]`) | `src/lib/execute-request.ts:428-439`, `execution_service.rs:143-148`, `:1458-1466` |
| Executor errors | Returned from `send_request` unmasked. The token error text holds the endpoint body | `execution_service.rs:2771-2773`, `reqwest_executor.rs:1419` |

Building blocks from #112: `process_env_for` returns `ProcessEnv { vars, dot_env_keys, allowed }` and reads no file when denied (`execution_service.rs:519-530`, `:1201-1270`). `process_env_for_scope` narrows it with `ProcessEnvScopeInput` (`:1317-1336`, `execution_service/websocket_resolution.rs:18-35`). `SendProcessEnv` carries `dot_env_values`, `used_values` and `withheld_names` (`execution_service.rs:396-403`). `process_env_withheld_entry` is the warning line (`:378-385`). `redact_secrets` masks text (`crates/rocket-app/src/redaction.rs:16`).

## Non-goals

- Removing the frontend's own resolution pass for OAuth2 fields. Both passes stay.
- Changing the gate, the trust store, or the precedence (the OS value wins over a `.env` value with the same name, and process env is the lowest layer).
- Masking non-`.env` OS values outside the assistant.
- OAuth2 grants the executor does not run inline (authorization code, implicit).

## Design

```
Flow run start ── process_env_for(collection, env, global) once ──┬─> FetchContext.vars (Auth fetch, wire snapshot)
                                                                  └─> Input node vars
Tab: Get Token / auto-fetch / refresh ── processEnv scope ──> oauth2_* command ── process_env_for_scope ──> OAuth2Service
Tab send (inherit OAuth2) ── processEnv scope ──> resolve_request_parts ── OAuth2 fields ──> executor inline fetch
```

### 0. `process_env` module

`ProcessEnvScopeInput` (`execution_service/websocket_resolution.rs:18-35`) and `ProcessEnv` (`execution_service.rs:519-530`) move unchanged into a new `crates/rocket-app/src/process_env.rs`. The pure part of `process_env_for_scope`, picking which collection and environment a scope reads, becomes `pub(crate) fn scope_target<'a>(collection: Option<&'a str>, environment_name: Option<&'a str>, scope: &'a ProcessEnvScopeInput) -> Option<(Option<&'a str>, Option<&'a str>)>` there. It returns `None` for closed. `process_env_for` and `process_env_for_scope` stay on `RequestExecutionService`, because they need its repos and loader, and `process_env_for_scope` calls `scope_target`. `rocket_app::ProcessEnvScopeInput` stays re-exported from `lib.rs`, so `src-tauri` changes no import. The serde tests of the type move with it. This move comes first and changes no behaviour.

### 1. Flow run process env

At run start, after the vault secrets and before `resolve_flow_credentials`, the run calls `exec.process_env_for(Some(&input.collection), input.environment_name.as_deref(), input.global_env_name.as_deref())` once and keeps the `ProcessEnv` for the run. New helper on `RequestExecutionService`:

```rust
// Distinct from build_variable_context_with_process_env (:1340), which reads the gate and .env itself.
pub(crate) fn variable_context_with_process_env(&self, global_env_name: Option<&str>, collection: Option<&str>,
    environment_name: Option<&str>, request_path: Option<&str>,
    external_secrets: &HashMap<String, String>, process_env: &HashMap<String, String>) -> HashMap<String, String>
```

It builds the scopes as `build_variable_context` does, sets `scopes.process_env` and returns `flatten_with_process_env()`. `FetchContext.vars` and the Input node's map use it with the run's `ProcessEnv.vars`. The run keeps its `.env` values (the values of `dot_env_keys`, at least `MIN_REDACTION_LEN` long) as `run_dot_env_values: HashSet<String>`. They join the step mask only: `execute_node` extends its `secret_values` (and so `trace_masks`) with them, the Request arm's own set gets them too, and the run's `masking_secrets` (the cache entry) adds them. They never join `external_secrets`, which is the RocketVault channel: a script write that holds a `.env` value is saved as usual, not held back, and `rok.getSecretVar` cannot read a `.env` value. This matches request nodes since #112 (`SendProcessEnv.dot_env_values`, `execution_service.rs:396`). Request nodes are unchanged: they still read process env per send.

### 2. Flow Input node

Same resolution as today, with the run's process env added. When the gate denied the run (`allowed == false`) and the Input value names a `process.env.*` placeholder, the node gets one `FlowLogEntry` with the text of `process_env_withheld_entry`. The run goes on.

### 3. Flow Auth-node fetch

`FetchContext.vars` now holds process env, so `build_get_token_request` fills `{{process.env.CLIENT_SECRET}}`, and `static_wire_snapshot` resolves `Bearer {{process.env.TOKEN}}` to the value the send uses. When the gate denied the run and an Auth node's OAuth2 fields name a `process.env.*` placeholder, the warning line goes to the logs of the first step that uses that node's credential, or to the run's first step when no step uses it (builder's call on the exact hook). The fetched token stays literal (`literal.auth`, `flow_execution_service.rs:1735`). A failed fetch's error text (`"Auth node \"{label}\": {e}"`, `flow_auth.rs:261`) passes through `redact_secrets` with the run's `.env` values.

### 4. `OAuth2Service` with process env

- `OAuth2GetTokenRequest` and `OAuth2RefreshRequest` (`oauth2_service.rs:24`, `:66`) gain `#[serde(default)] pub process_env: ProcessEnvScopeInput` (default `Closed`).
- `build_variable_context(collection, environment_name, request_path, external_secrets, process_env: &HashMap<String, String>)` puts the map in as the lowest layer.
- `resolve_get_token_request_with_secrets(req, external_secrets, process_env)` and `refresh_token_with_secrets(req, external_secrets, process_env)` take the map. `resolve_get_token_request` and `refresh_token` pass an empty one. `OAuth2ServiceFetcher` passes an empty one, because `build_get_token_request` already resolved every field.
- `OAuth2Service` owns the resolution and the masking. New methods: `pub fn resolve_get_token_request_with_host_env(&self, req, external_secrets, process_env: &ProcessEnv) -> ResolvedOAuth2Config`, `pub async fn get_token_with_host_env(&self, config: &ResolvedOAuth2Config, process_env: &ProcessEnv) -> DomainResult<OAuthToken>` and `pub async fn refresh_token_with_host_env(&self, req, external_secrets, process_env: &ProcessEnv) -> DomainResult<OAuthToken>`. The last two map an error through `redact_secrets` with the `.env` values of `process_env` (values of `dot_env_keys`). The authorization-code exchange in the command uses the resolved config, so it gets the values without more code.
- The commands stay thin. `oauth2_get_token` and `oauth2_refresh_token` add one service call next to the vault one they already make, `exec.process_env_for_scope(request.collection, request.environment_name, None, &request.process_env)`, and pass the `ProcessEnv` to the `_with_host_env` methods. No resolution or masking happens in `src-tauri`. No global environment name is sent, so the global environment's `.env` layer is skipped here (the frontend pass still covers it).
- Frontend: `OAuth2GetTokenRequest` and `OAuth2RefreshRequest` in `src/lib/tauri-api.ts:1569`, `:1593` get a required `processEnv: ProcessEnvScopeInput`. `buildGetTokenRequest` and `buildRefreshRequest` take it from `OAuth2Target` (new required field `processEnv`). `maybeAutoRefreshOrFetchToken` passes `toProcessEnvScopeInput(processEnvScopeOf(tab))`. `flow-auth-preflight.ts` passes `{ kind: 'collection', name, environmentName }`. `OAuth2AuthEditor` gets an optional `processEnvScope?: ProcessEnvScope` prop through `AuthEditor`. `RequestPanel` passes `processEnvScopeOf(tab)`. Without the prop it uses the `collection` prop, or `CLOSED_PROCESS_ENV` when there is none.

### 5. Inline OAuth2 resolution

- `map_auth_fields` gets an `Auth::OAuth2` arm that applies `r` to every string field of every variant: token, refresh, authorization and callback URLs, `client_id`, `client_secret`, `scope`, `state`, resource owner `username` and `password`, and `name` and `value` of every additional parameter. `placement`, settings and token config are untouched. The order is fixed, so the runtime pass (`execution_service/runtime_fill.rs`) maps templates back in the same order. `auth_has_placeholder` (`execution_service.rs:3469`) probes through `map_auth_fields`, so it now sees OAuth2 placeholders too; that is intended and pinned by `auth_has_placeholder_sees_oauth2_fields`. In `resolve_request_parts` (`:1513`) the auth closure picks its map by variant before mapping: OAuth2 fields use the OAuth2 map below, every other variant keeps `vars`.
- `ExecuteRequestInput` gains `#[serde(default)] pub process_env: ProcessEnvScopeInput`. In TypeScript `ExecuteRequestInput` (`src/lib/tauri-api.ts:586`) it is required. Every frontend builder of the input sets it from `toProcessEnvScopeInput(processEnvScopeOf(tab))`, or `{ kind: 'closed' }` when no tab is known.
- In `resolve_request_parts`, when `backend_process_env` is false and the effective auth is `Auth::OAuth2`, the OAuth2 fields resolve with `vars` plus `process_env_for_scope(collection, environment, global, &input.process_env).vars`. Nothing else in the send sees that map, and no file is read when the effective auth is not OAuth2. Its `.env` values join `SendProcessEnv.dot_env_values`. Its used names join `used_values`, or `withheld_names` when denied. A tab send then warns for these names, because the frontend cannot see an inherited auth. When `backend_process_env` is true, the send's own map already covers the OAuth2 fields.
- `send_request` maps an executor error through `redact_secrets` with the send's secret values, `.env` values included.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Gate denied (Flow, tab, IPC) | No `.env` file read, placeholder sent literal, one warning line, nothing blocked |
| History tab with no collection | Scope `closed`, no host environment |
| IPC caller omits `processEnv` | Serde default `closed`, no host environment |
| Scope `collection` while `collection` is also set | `collection` keeps its own gate, the scope only closes it |
| Token response contains `{{process.env.X}}` | Never resolved |
| `.env` value shorter than `MIN_REDACTION_LEN` | Not masked, as in #112 |
| Unknown `{{process.env.NOPE}}` with gate allowed | Stays literal, no warning |
| Static Auth node `Bearer {{process.env.T}}` | Wire snapshot equals the sent value, masked |
| Token endpoint error echoes the secret | Masked in the step error, the IPC error and the send error |

## Testing

`rocket-app` (`cargo test -j4 -p rocket-app <name>`):
- `flow_input_node_resolves_process_env_when_allowed`, `flow_input_node_keeps_process_env_literal_when_denied`, `flow_input_node_masks_dot_env_value_in_step_and_output`, `flow_input_node_warns_once_when_withheld`, `flow_run_reads_no_dot_env_file_when_denied`.
- `flow_auth_fetch_sends_process_env_client_secret` (wiremock token endpoint), `flow_auth_fetch_keeps_literal_when_denied`, `flow_auth_fetched_token_stays_literal`, `flow_static_bearer_wire_snapshot_resolves_process_env`, `flow_auth_fetch_error_masks_dot_env_value`.
- `scope_target_closed_is_none`, `scope_target_collection_input_keeps_its_own_gate`, `scope_target_scratch_without_collection_reads_no_collection`; the moved serde tests pass unchanged.
- `oauth2_service_resolves_process_env_map_as_lowest_layer`, `oauth2_get_token_request_defaults_to_closed_scope`, `oauth2_refresh_resolves_process_env_map`, `oauth2_get_token_error_masks_dot_env_value` (wiremock endpoint echoing the secret).
- `map_auth_fields_resolves_every_oauth2_field`, `inherited_oauth2_resolves_vars_on_backend_send`, `tab_send_inherited_oauth2_resolves_process_env_with_scope`, `tab_send_inherited_oauth2_closed_scope_keeps_literal_and_warns`, `execute_request_input_defaults_to_closed_scope`, `executor_error_is_masked`, `auth_has_placeholder_sees_oauth2_fields`. Update `oauth2_folder_auth_reaches_the_executor_unchanged` (`execution_service.rs:6321`) to expect resolved fields.

`src-tauri` (`cargo test -j4 -p rocket <name>`): `oauth2_dto_reads_the_process_env_scope`, like `grpc_dto_reads_the_process_env_scope` (`src-tauri/src/commands/grpc.rs:150`).

Frontend (`yarn test oauth2-requests`, `yarn test execute-request`): `buildGetTokenRequest` and `buildRefreshRequest` carry `processEnv`. The send input carries the tab's scope, and `closed` for a History tab with no collection.

Commands: `cargo check -j4`, the tests above, `yarn tsc --noEmit`, `yarn check`.

## Files changed

`crates/rocket-app/src/{process_env.rs (new), lib.rs, execution_service/websocket_resolution.rs, flow_execution_service.rs, flow_auth.rs, oauth2_service.rs, execution_service.rs}`, `src-tauri/src/commands/oauth2.rs`, `src/lib/{tauri-api.ts, oauth2-requests.ts, execute-request.ts, flow-auth-preflight.ts}`, `src/components/request/{AuthEditor.tsx, RequestPanel.tsx, oauth2/OAuth2AuthEditor.tsx}`, the other `ExecuteRequestInput` builders that tsc names, `docs/features/collection-trust.md`.

## Constraints

- Read `docs/superpowers/specs/opencollection-spec-reference.md` before each task.
- DDD layering (`.claude/rules/rust-ddd-boundaries.md`, `tauri-ipc-boundaries.md`): logic in `rocket-app`, no new I/O there, `.env` reading stays behind the injected `dot_env_loader`, the gate behind the injected trust store. `rocket-infra` gets no secret or scope knowledge. Tauri commands only call services and pass results on.
- Known layering debts that stay as they are: `OAuth2Service` sends its token requests from `rocket-app`, and `process_env_for` reads `std::env::vars()` directly. Neither is extended here.
- The gate is decided once per Flow run, once per IPC call, once per send.
- Every Flow lookup stays inside `with_send_pins`.
- No panics in production paths. Tests use `.expect("...")`.
- Serde `camelCase` only on the IPC DTOs.

## Acceptance criteria

- A Flow Auth-node client-credentials fetch sends the `.env` value of `{{process.env.CLIENT_SECRET}}` when the gate allows it and the literal when it denies it.
- A fetched token is sent as written and never resolves `{{process.env}}`.
- A static Auth-node `Bearer {{process.env.TOKEN}}` wire snapshot equals the sent value.
- A Flow Input node resolves `{{process.env.X}}` under the run's gate, and its `.env` value is masked in the step trace and outputs.
- A denied gate gives one warning line per Flow node or tab send that used a placeholder, and blocks nothing.
- A denied Flow run reads no `.env` file.
- `oauth2_get_token` and `oauth2_refresh_token` resolve process env under the request's `processEnv` scope, and an omitted scope means closed.
- `src-tauri` OAuth2 commands do no resolution or masking; `OAuth2Service` does both.
- `ProcessEnvScopeInput` and `ProcessEnv` live in `rocket-app/src/process_env.rs`, and `rocket_app::ProcessEnvScopeInput` still resolves.
- `map_auth_fields` resolves every OAuth2 string field, so an inherited OAuth2 auth on a backend send reaches the executor resolved.
- A tab send of an inherited OAuth2 auth resolves `{{process.env}}` under the tab's scope, and a History tab with no collection keeps the literal.
- Executor and token-fetch error text masks `.env` values.
- `docs/features/collection-trust.md` Known limits no longer lists these paths, and the Host environment section names them.

## Out of scope

- The assistant's stricter OS-value masking (it has no `run_flow` tool).
- A global environment `.env` layer on the OAuth2 IPC path.

## Decisions

- When the gate denies access and a Flow Input node or an Auth-node token fetch uses `{{process.env.X}}`, should the run warn? Options: one Console line, no warning. Chosen: one Console line, worded and flagged like #112's R7, never blocking. Decided by: user
- Should the backend executor's inline OAuth2 fetch (inherited client-credentials or password auth, which resolves no `{{vars}}` today) be fixed here? Options: separate ticket, fold into #132, ignore. Chosen: fold into #132. Decided by: user
- Which process env values does a Flow Input node mask? Options: `.env` values only, all used values. Chosen: `.env` values only, like request nodes since #112. Decided by: user
- How do `OAuth2Service` token and refresh requests from the request tab get `{{process.env}}`? Options: frontend only plus a test, backend with the tab's process env scope, backend by collection. Chosen: do what Bruno does, so the backend resolves every OAuth2 field with the send's map, under a process env scope so a History tab never counts as scratch. Decided by: user
- How does a tab send of an inherited OAuth2 auth with no cached token get `{{process.env}}`? Options: HTTP send gets a process env scope, frontend resolves the folder OAuth2, known limit. Chosen: the HTTP send carries a `processEnv` scope (`ProcessEnvScopeInput`, default closed), used only inside the inherited OAuth2 auth of a tab send. Decided by: user
- Should the frontend keep filling OAuth2 fields before `oauth2_get_token` and `oauth2_refresh_token` once the backend resolves them too? Options: keep both passes, backend only now, backend only later. Chosen: keep both passes; the backend resolves what is left, process env included, under the tab's scope. Decided by: user
- How strict is the new `processEnv` scope on `ExecuteRequestInput`? Options: TypeScript required with a Rust serde default of closed, required on both sides, only on `execute_request`. Chosen: required in the TypeScript type, `#[serde(default)]` closed in Rust, so a missed caller fails closed. Decided by: user
- The OAuth2 IPC DTOs use the same scope rule (TypeScript required, Rust default closed). Options: same rule, required on both sides. Chosen: same rule as `ExecuteRequestInput`, for one consistent contract. Decided by: call
- Where do the OAuth2 process env resolution and error masking live? Options: `OAuth2Service` owns them with the `ProcessEnv` passed in, an injected provider trait, the Tauri command. Chosen: `OAuth2Service` owns them; the command only adds the `process_env_for_scope` service call next to the vault one. Decided by: user
- Should `ProcessEnvScopeInput` move out of the WebSocket module? Options: move it with `ProcessEnv` and the pure scope logic into `process_env.rs`, move the type only, leave it. Chosen: move both types and `scope_target` into `crates/rocket-app/src/process_env.rs`, keeping the `rocket_app::ProcessEnvScopeInput` re-export. Decided by: user
- Flow reads process env once per run for Input and Auth nodes. Decided by: call
- How do Flow Input and Auth nodes mask `.env` values? Options: the step mask only (`secret_values`), `external_secrets` (adds the vault hold-back and `rok.getSecretVar` reads), the step mask plus a hold-back. Chosen: the step mask only, as request nodes do since #112. Decided by: user
- Executor errors are masked in `send_request` for every send, not only OAuth2 ones. Decided by: call
- What scope does `OAuth2AuthEditor` use without a `processEnvScope` prop? Options: its `collection` prop or closed with none, always closed, a required prop. Chosen: its `collection` prop (that collection's own gate), or closed with none. Decided by: user
