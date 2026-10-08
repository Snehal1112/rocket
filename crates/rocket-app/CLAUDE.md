# rocket-app

Application-layer orchestration crate. It wires together the domain crates
(`rocket-collection`, `rocket-environment`, `rocket-git`, `rocket-history`,
`rocket-http`, `rocket-workspace`) into concrete use-case services, but never
touches the filesystem or any I/O directly — those concerns live in
`rocket-infra`.

## Public Types

| Type | Purpose |
|---|---|
| `CollectionService` | CRUD for collections, folders, and requests; enforces name validation; publishes a `DomainEvent` for every mutation. |
| `CookieService` | Thin wrapper over `CookieRepository` for per-domain cookie jars. |
| `EnvironmentService` | CRUD for environments; publishes `EnvironmentSaved/Deleted` events. |
| `RequestExecutionService` | Core HTTP dispatch: resolves variables, merges collection settings, runs the request, saves history, publishes `RequestExecuted`. |
| `ExecuteRequestInput` | Serialisable input DTO for `RequestExecutionService::execute`. |
| `CollectionRunnerService` | Runs a folder's/collection's requests in sequence; honours `rok.runner.setNextRequest`/`skipRequest`; publishes `RunnerStarted/StepCompleted/Finished`. |
| `RunCollectionInput` / `RunSummary` | IPC DTOs for `CollectionRunnerService::run`. |
| `GitAppService` | Full git workflow (status, stage, commit, push/pull/fetch, branch, stash, conflicts) with event publishing. |
| `HistoryService` | List, search, and clear request history. |
| `TemplateService` | CRUD for saved request templates (stored via `rocket-history`). |
| `AgentConfigService` | CRUD for ACP agent configs; `resolve_credential` fetches the agent's API key from RocketVault. |
| `AcpSessionService` | Starts/prompts/ends ACP agent sessions via `Box<dyn AcpSessionClient>`; publishes `AcpSessionStarted/Chunk/Finished/Failed` (all chunks before the terminal event); fixed 120s prompt timeout force-kills the session. |
| `WebSocketService` | Session registry for WebSocket connections, keyed by a frontend-chosen session id. `connect`/`send`/`disconnect`/`end_all_sessions` over `Arc<dyn WebSocketClient>`; inbound frames and lifecycle leave as `WebSocketMessage` and `WebSocketStatus` events (exactly one terminal status, then the id is free). A `disconnect` during the handshake cancels it and closes the late socket. `RequestExecutionService::resolve_websocket` and `resolve_websocket_message` (in `execution_service/websocket_resolution.rs`) apply variables, collection defaults and auth; OAuth, Digest, NTLM, WSSE and SigV4 are refused for WebSocket with an explicit error. |
| `GraphQlSubscriptionService` | GraphQL subscriptions over the `WebSocketClient` port, keyed by a frontend-chosen session id. `resolve_graphql_subscription` resolves the request (variables, collection headers, auth, `http`→`ws` scheme) through `resolve_websocket`; `start`/`stop`/`end_all` drive one task per subscription. The message formats of `graphql-transport-ws` and legacy `graphql-ws` live in the pure `graphql_subscription/protocol.rs`; the server's chosen subprotocol decides the dialect. `subscribe` is sent only after `connection_ack` (10 s limit). Results leave as `GraphQlSubscriptionMessage` and `GraphQlSubscriptionStatus` events (exactly one terminal status). |
| `WorkspaceService` | Create, switch, rename, close, delete, pin/unpin workspaces; link external collections; toggle multi-workspace mode; mutates the shared `Arc<Mutex<PathBuf>>` active path on switch. |

## Service Method Details

### CollectionService
- Every mutating method publishes a `DomainEvent` (e.g. `CollectionCreated`, `RequestSaved`, `FolderDeleted`, `ItemMoved`, `CollectionSettingsSaved`) after the underlying repository call succeeds.
- `rename_request` — mutates only the `name` field inside the JSON; the filename stays the same, producing a single `Modify` filesystem event.
- `move_item` — moves a request or folder between collections or paths.
- `reorder_items` — reorders items within a folder by supplying the new name order.
- `get_settings` / `save_settings` — read and write per-collection settings (auth, headers, variables).

### WorkspaceService
Constructor takes **two** repos: `WorkspaceRepository` (registry) and `WorkspaceConfigRepository` (`workspace.yml` files).

| Method | Notes |
|---|---|
| `create` | Creates the directory, `collections/`, `environments/` subdirs, and writes `workspace.yml`. |
| `switch` | Updates `active_workspace_id` in registry and mutates `Arc<Mutex<PathBuf>>`. |
| `delete` | Removes the workspace directory from disk via `fs::remove_dir_all`. Cannot delete `"default"` or the last workspace. |
| `close` | Removes from registry only (no disk deletion). Cannot close the last workspace. |
| `pin` / `unpin` | Toggles `workspace.pinned` in the registry. |
| `update_description` | Sets `workspace.description`; pass `None` to clear. |
| `open_workspace` | Registers an existing on-disk workspace; directory must contain `workspace.yml`. |
| `get_workspace_config` | Loads `WorkspaceConfig` from the workspace's `workspace.yml` via `WorkspaceConfigRepository`. |
| `link_external_collection` | Validates `opencollection.yml` presence, reads its `name` field, appends a `CollectionReference` to `WorkspaceConfig`. |
| `get_multi_workspace_mode` / `set_multi_workspace_mode` | Reads/writes `registry.multi_workspace_mode`. |

### GitAppService
Wraps `Box<dyn GitService>`. Every mutating operation publishes a `DomainEvent`. Notable methods:
- `diff_staged` — diff for already-staged files.
- `checkout_remote_branch` — creates a local tracking branch. Rejects an existing local branch of the same name unless `force` is set, which instead resets that branch (and its upstream) to the remote's tip, discarding local commits absent from that remote. A forced reset of the *checked-out* branch still refuses to run over uncommitted tracked changes.
- `fetch` — no event published (read-only remote op).
- `conflicts` — publishes `GitConflictDetected` only when the list is non-empty.
- `abort_merge` — reverts in-progress merge; publishes `GitStatusChanged`.

## Key Patterns

- **Trait-object injection.** Every service takes `Box<dyn SomeRepository>` and `Box<dyn EventPublisher>` via its constructor. No concrete types appear in this crate, making all services fully testable with in-memory mocks.
- **DomainResult everywhere.** All fallible methods return `DomainResult<T>` from `rocket-shared`.
- **Variable resolution in `RequestExecutionService::execute`.** Collection variables are loaded first; environment variables override them. The merged map is passed to `rocket_environment::resolve()` before the HTTP call.
- **Client certificates and RocketVault.** `client_certificates::environment_client_certificates(repo, environment_name, collection_dir, vars, external_secrets)` turns the environment's persisted `ClientCertificate` entries into `ResolvedClientCertificate`s. `RequestExecutionService::resolve_request` and `OAuth2Service` (`resolve_get_token_request_with_secrets`, `refresh_token_with_secrets`) both call it, so they resolve identically. Per entry: `{{placeholders}}` in the domain, file paths and passphrase, then relative paths joined onto the collection folder, then each `certificateSecret`, `privateKeySecret` and `pkcs12Secret` reference looked up in the RocketVault map (`alias.secretName`). A found reference becomes `CertificateSource::Inline` (PKCS12 secrets are base64 and ignore whitespace). A missing, empty, undecodable or over-1-MiB (`MAX_INLINE_SECRET_BYTES`) reference makes the entry `CertificateMaterial::Unavailable { reason }`, which the executor turns into an error only when that entry is selected for the URL. Load tests call `resolve_request` with an empty secrets map, so vault-backed certificates are `Unavailable` there. Multi-line vault values are masked whole and line by line (`redaction::redaction_forms`). A script or declarative-action write whose value contains a vault value is not persisted (environment, global environment, collection, folder and request scopes). It is kept in `var_ctx.runtime` with a console warning instead. This hold-back is best-effort: base64 or URL-encoded forms, slices of a secret and secrets under `MIN_REDACTION_LEN` are not caught. A `vault` entry (a certificate stored in a RocketVault vault, picked by name) becomes `CertificateMaterial::Deferred`: no bytes are read while resolving. `vault_certificates.rs` fetches only the selected entry just before the send (`with_vault_certificates`, and for OAuth2 token URLs) through `VaultSecretFetcher::fetch_certificate`, and a failed fetch turns that entry into `Unavailable` with no fallback. History, scripts and events keep the names-only form. Load tests treat vault certificates as unavailable (`unavailable_in_load_tests`). `SecretManagerService::save` and `delete` call `VaultSecretFetcher::forget_connection` to drop cached certificate ids. The PKCS12 export asks for `compat: legacy`, and whether that loads on OpenSSL 3 is untested until the ignored live test runs.
- **Phase-callable execution.** `RequestExecutionService::execute` is a thin composition over `begin_phases` → `run_before_request_phase` → `send_request` → `run_after_response_phase` → `run_tests_phase` → `finish_phases`, all sharing one `PhaseState`. `CollectionRunnerService` drives the same methods one phase at a time so it can act on `skip_request` before the send and on `next_request` after every phase. Do not add phase logic to only one caller.
- **Folder script chain.** `begin_phases` reads the folder chain once (`folder_chain`, which calls `get_folder_chain_settings`) and stores `PhaseScripts` on `PhaseState`, ordered by `rocket_collection::chain_scripts` and `CollectionSettings.script_flow` (`execution_service/script_chain.rs`). Each script is its own engine run, built from the current `state.var_ctx` and `state.http_request`, so later scripts see earlier mutations and runtime variables. Sandbox mode, file scope and vault hold-back are the request script's. The first error wins and ends its phase; a folder script's error reads `Folder "<path>" <phase> script: <message>`. In a run, `skipRequest()` ends the pre-request chain; a single send ignores it. Inline Flow requests (`__flow_inline__/`) and GraphQL introspection (`skip_folder_scripts`) run no folder scripts. `references_alias` also scans folder scripts, headers and auth. Test results carry no folder label.
- **Header and auth merging.** `RequestExecutionService::inherited_auth_and_headers` is the one place defaults apply: collection headers, then the folder chain (`folder_chain` → `get_folder_chain_settings`, outermost first, merged by `rocket_collection::inherited_headers`), then the request. The request wins by key and a disabled header never shadows. A request auth of `none` or `inherit` takes the nearest folder auth (`resolve_folder_auth`), then the collection auth. `resolve_request` (single send, runner, Flow, load test, GraphQL introspection) and `resolve_websocket` (WebSocket, GraphQL subscriptions) both call it. A folder chain that fails to load fails the send. Inline Flow requests (`FLOW_INLINE_PATH_PREFIX`) inherit no folder settings. gRPC still applies collection defaults only, because `GrpcExecuteInput` carries no request path.
- **Non-fatal history write.** `let _ = self.history_repo.save(...)` is intentional — a history persistence failure must not abort the response.
- **Event publishing is fire-and-forget.** Services publish `DomainEvent` variants after successful operations; errors from downstream listeners are not propagated back.
- **Tests use inline mocks.** Each service module contains its own mock implementations in `#[cfg(test)]`. `tempfile` is used in `WorkspaceService` tests that require real directories.

## Workspace Position

```
src-tauri (Tauri commands)
    └── rocket-app  ← this crate
            ├── rocket-collection
            ├── rocket-environment
            ├── rocket-git
            ├── rocket-history
            ├── rocket-http
            ├── rocket-workspace
            └── rocket-shared
```

`src-tauri/src/lib.rs` constructs each service by injecting concrete `rocket-infra` implementations, then stores them in Tauri managed state. Tauri commands call service methods directly; services never call back into Tauri.

## Flow Auth nodes (`flow_auth.rs`)

`resolve_flow_credentials` turns every Auth node into a credential before a
run starts: a UI-supplied token wins, a non-interactive OAuth2 grant is fetched
through the `FlowTokenFetcher` port (`OAuth2ServiceFetcher` in production), an
interactive grant without a token fails the run before any event; each fetch is
bounded by `TOKEN_FETCH_TIMEOUT` (30 s per fetch; fetches are sequential).
Static auth types pass through unresolved, so each request resolves
`{{variables}}` at send time with its own scopes. For a static Bearer or API
key the node's wire output is a run-start snapshot; the Request arm of
`execute_node` resolves an Auth-node template with the request's own variables
and adds the sent value to that request's secrets (`flow-auth-sent.<node id>`),
so what is sent is masked. `FlowExecutionService::run_with_auth`
(`flow_execution_service.rs`) injects each credential secret into the run's
external-secrets map as `flow-auth.<node id>` so the existing redaction masks
it. Request nodes whose auth is `inherit` or `none` (the backend treats them alike) use the auto-apply credential; an
`auth` wire overrides it. Types holding secrets (`SuppliedToken`,
`FetchContext`, `FlowCredentials`) have redacting `Debug` impls; keep it that
way.

Send-time masking caveats: a `repeat_until` attempt after a script rotates the
variable may send a value not in that request's mask (unless it is a secret
variable); `{{$dynamic}}` placeholders in a static token are generated
separately for the mask and the send, so they are not masked; a partly
resolved template (`{{token}}-{{unset}}`) is not masked by the Auth node; and
`flow-auth-sent.<id>` widens the script-write hold-back for that request.

## Flow partial runs (`flow_partial.rs`, `flow_run_cache.rs`)

`FlowExecutionService::run_partial` re-runs one node (`FlowPartialMode::Node`)
or a node and its descendants (`FromHere`) on top of a cached earlier run.
`flow_run_cache` keeps the last 8 runs in memory (64 MiB of outputs, 16 MiB
per output, never persisted, never sent over IPC). It is to be cleared on
workspace switch by P20 (`clear_run_cache` must be wired there); until then
only a restart clears it. Input and Auth nodes always run again; their
outputs are never cached. A run is refused before any event when the base
run is gone, used other environments, a node upstream changed (Merkle
fingerprints, saved request text without `uid`), a needed input was skipped,
failed or not kept, a seed is stale after an earlier partial run, or a Wait's
callback sender is outside the run. Not detected: variable value changes
(decision D5), folder and collection settings a saved request inherits,
shared `.js` script files, and auth tokens supplied for the partial run (they
are not compared with the base run's). Values the base run masked are masked
again under `}}prev-run.<n>` external-secret keys. No `{{template}}` can
reference them, but a script can read one by exact name (`rok.getSecretVar`).

## Flow run ids (`flow_run_id.rs`)

`FlowExecutionService::run_with_options(exec, input, tokens, FlowRunOptions { run_id, partial })`
is the one run entry; `run_with_auth` and `run_partial` call it. The frontend
chooses the run id (a UUID from `newFlowRunId()` in `src/lib/flow-run-id.ts`)
and sends it as `runId` in `RunFlowInputDto`; without one the service makes a
ULID (`choose_run_id`). A chosen id must be 1 to 64 ASCII letters, digits, `-`
or `_`, or the run is refused with `InvalidInput` before any event (the message
does not quote the id). `RunRegistration::reserve` registers the id first
thing in the run and refuses with `AlreadyExists` an id that is in flight or
still kept in the run cache. A run is cached before it leaves `in_flight`, so a
used id is never free while it is kept. The id names the run in every
`FlowRun*` and `FlowStep*` event, in `cancel` (`cancel_flow_run`), in the run
cache and in `FlowRunSummary.run_id`. A `cancel` that arrives while secrets,
tokens and callback endpoints are prepared is kept and stops the run before its
first node (`FlowRunStarted` and `FlowRunFinished` still go out); the fetch
itself is not interrupted (roadmap F-05).

Frontend: `FlowToolbar` makes the id after the save and sign-in steps, stores it
on the tab as `pendingRunId` (`setFlowPendingRun`) before `run_flow` is sent,
and ignores every event with another id and every event after `run_flow`
settled. A remounted toolbar follows the tab's `pendingRunId` until
`flow-run-started`, then its `runId`. `setFlowRunState` clears `pendingRunId`.

## Flow step trace (`flow_trace.rs`)

`execute_node` fills a `NodeTrace` out-param; the run loop moves it into
`FlowStepResult.trace` and the `FlowStepCompleted` event. It records each data
wire's value (`record_wire`), an `auth` wire as `credential: true` with no
value, an If/Switch decision (`record_route`) and the failing wire
(`record_failure`, `failed_edge_id`). Values are masked with `secret_values`
plus `credentials.secret_forms()` first and capped second (16 KB per wire,
64 KB per step, 1 KB per route value, 256 KB per step value). Wire errors are
named by `wire_err` and never quote a resolved value. The run loop fills
`duration_ms` for every node that ran and did not set its own.
Wires are recorded with the credential captured at run start, so a credential
that changes at send time is not in the trace mask (same caveat as the
send-time note above).

## Flow callback URLs

`RunCallbacks::infos()` lists `(node id, name, url)` per Wait for callback
node. `FlowRunStarted.callbacks` is the only place a callback URL leaves the
backend, because the URL holds a bearer token. Never put it in a
`FlowRunSummary`, a step, an exchange, a log, a live progress event or
history. The field is omitted when empty and defaults on old payloads.
Each endpoint's full URL and bare token also join the run's
`external_secrets` (`flow-callback.<node id>.url` and `.token`, from
`RunCallbacks::mask_secrets`), so a sender that echoes its URL is masked in
every other sink. A token under `MIN_REDACTION_LEN` is not masked alone (a
real one is 32 characters); the full URL still is.

## Flow lint (`flow_service.rs`)

`FlowService::lint(collection, flow)` lints the graph it is given (the
canvas, saved or not) and never fails or touches the repository: a
`validate` failure first, as `invalid_graph` error lints, then the
`rocket_flow::validate_with_warnings` warnings. It passes `NoLintContext`
until F-21 and F-22 add a context built from the collection. `save` and the
run path do not call it, so lints never block either. The `lint_flow` IPC
command is plan P21. `lint` runs `validate` first, which is super-linear on very
large flows (about 3.5 s at 20k If nodes), so it is fine at drawn sizes.
