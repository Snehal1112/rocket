# rocket-app

Application-layer orchestration crate. It wires together the domain crates
(`rocket-collection`, `rocket-environment`, `rocket-git`, `rocket-history`,
`rocket-http`, `rocket-workspace`) into concrete use-case services. It should
not touch the filesystem or any I/O directly — those concerns live in
`rocket-infra`. Phase 4 of `docs/roadmaps/ddd-roadmap.md` moved files, the token
POST and the host environment (`HostEnv`) behind traits. One read is left: the
`which::which` command lookup in `agent_config_service.rs` (reads `PATH` and the file
system, no roadmap item yet). The home directory comes from `HostEnv::home_dir()` (D-53). Do not add new reads. The crate is also
large (about 33k non-test lines); D-80 to D-82 plan its split by context.

## Public Types

| Type | Purpose |
|---|---|
| `CollectionService` | CRUD for collections, folders, and requests; enforces name validation; publishes an event for every mutation (`move_item` also publishes `FlowEvent::FlowRequestPathsRewritten`). |
| `CookieService` | Thin wrapper over `CookieRepository` for per-domain cookie jars. |
| `EnvironmentService` | CRUD for environments; publishes `EnvironmentEvent::EnvironmentSaved/EnvironmentDeleted`. |
| `WorkspaceWatcher` | Restarts the file watcher on `<workspace>/collections` through the `FileWatcher` port (stop, then start). A failed start is logged and published once as `FileWatcherFailed` with `<folder>: <reason>`. `restart` returns whether the watcher runs. Used at startup and by switch, close and delete of a workspace. |
| `EnvironmentServices` | Managed state that hands out an `EnvironmentService` per call: `for_collection(name)` (rejects an empty name, a NUL byte, an absolute name or a `..` segment with `InvalidInput("invalid collection name")`) and `global()`. Its repository factories follow the active workspace, so each call points at the workspace active at that moment. Reads and writes share one event publisher and audit publisher (reads publish nothing). |
| `RequestExecutionService` | Core HTTP dispatch: resolves variables, merges collection settings, runs the request, saves history, publishes `RequestExecuted`. |
| `ExecuteRequestInput` | Serialisable input DTO for `RequestExecutionService::execute`. |
| `CollectionRunnerService` | Runs a folder's/collection's requests in sequence; honours `rok.runner.setNextRequest`/`skipRequest`; publishes `RunnerStarted/StepCompleted/Finished`. |
| `RunCollectionInput` / `RunSummary` | IPC DTOs for `CollectionRunnerService::run`. |
| `GitAppService` | Full git workflow (status, stage, commit, push/pull/fetch, branch, stash, conflicts) with event publishing. |
| `HistoryService` | List, search, and clear request history. |
| `TemplateService` | CRUD for saved request templates (stored via `rocket-history`). |
| `AgentConfigService` | CRUD for ACP agent configs. The credential source is `vault`, `env` or `keychain`; `resolve_credential` reads the agent's API key from that source (`env` through an injected `EnvReader`, trimmed). `set/clear/has_local_credential` manage the keychain entry (scope `agent-credential`, keyed by config id); delete and a source change away from `keychain` remove the entry. Reads use `get_strict`, so a locked keychain is an error and not a miss. |
| `AcpSessionService` | Starts/prompts/cancels/ends ACP agent sessions and changes session options via `Box<dyn AcpSessionClient>`; publishes `AcpSessionStarted/Chunk/Finished/Failed`, `AcpToolActivity`, `AcpConfigOptionsChanged` and `AcpUsage` (all update events before the terminal event); a 120 s idle limit, restarted by every update, force-kills a silent session (an open permission request pauses that clock, but a request left unanswered for 30 min, `PERMISSION_WAIT_LIMIT`, ends the session the same way); a `cancelled` stop reason is a normal finish. Text and reasoning chunks are joined (`acp_stream.rs`, one event per `CHUNK_FLUSH_WINDOW` 50 ms or 16 KiB, pending text flushed before any other update and before the end) and masked on a rolling buffer with the session's credential, MCP token and, through `with_stream_secrets` (`ConversationService`), the pinned workspace's secret variables; an unreadable workspace refuses the prompt (InvalidInput, nothing sent); `AcpSessionFailed.error` and the returned error are masked too, and so are tool call titles, permission titles, summaries and option names, and config option names and descriptions (ids and values are kept). The update channel stays unbounded on purpose (the agent client must never block or drop). Tracks live session ids and calls the injected SessionCleanup exactly once per session on every end path (end_session, idle timeout, failed prompt except InvalidInput, end_all_sessions). start_workspace_session always attaches the tool server and takes a `SessionPlan` (`session_plan.rs`, built per `AgentKind`: Claude gets `_meta` options and `CLAUDE_CONFIG_DIR` from `agent_isolation.rs`, Gemini gets a private `GEMINI_CLI_HOME` with a lockdown `settings.json` and a `GEMINI.md`, Generic gets only a first-prompt prefix). `ROCKET_MCP_TOOL_NAMES` must match the registered tools (a `src-tauri` test checks it). |
| `ProposalService` | In-memory, per-session queue of the assistant's proposed changes (cap 50 pending). `accept` re-reads the target, compares a SHA-256 fingerprint (`Stale` on mismatch) and applies through `CollectionService`/`EnvironmentService`; a failed write is `Failed`. `UpdateFolderSettings` replaces the sections a patch sets through `save_folder_settings`; its fingerprint covers headers, variables, scripts, docs and auth, and is checked on the saved copy; a header replaced by a patch loses its description. Publishes `AcpProposalCreated`/`AcpProposalResolved`. Proposals are keyed by the real ACP session id; `clear_session` runs from `TauriSessionCleanup`. |
| `ConversationService` | Stored assistant conversations. `pin_session(session_id)` (called by `start_workspace_assistant`) keeps `pinned()` repositories of the workspace active at start (16 pins). `save(session_id, input)` validates (lowercase UUID ids, an agent reply), masks every secret variable value of the pinned workspace with `mask_secret_text` (`conversation_secrets.rs`, fails closed on a read error, no vault), then applies the caps and publishes `AssistantConversationSaved`. `list`/`get`/`delete`/`clear` use the live workspace. See `docs/features/assistant-conversations.md`. |
| `McpToolService` | The workspace assistant's MCP tools. Checks every `collection` against the active workspace, returns masked views (`mcp_read_views`: secrets never returned, literal credentials masked, `{{var}}` kept), gates tools by `AssistantMode` (Ask < Edit < Agent, unknown session = Ask; every tool also refuses a session with no workspace pin, so an unknown or forgotten id fails closed) and `run_request` also by the run switch, `get_folder_settings` returns the masked folder settings plus the direct children (the root is refused with a pointer to `get_collection_settings`; a missing folder is `NotFound`), and keeps per-session state (mode, test results, the first-prompt outline) that `forget_session` clears. |
| `WebSocketService` | Session registry for WebSocket connections, keyed by a frontend-chosen session id. `connect`/`send`/`disconnect`/`end_all_sessions` over `Arc<dyn WebSocketClient>`; inbound frames and lifecycle leave as `WebSocketMessage` and `WebSocketStatus` events (exactly one terminal status, then the id is free). A `disconnect` during the handshake cancels it and closes the late socket. `RequestExecutionService::resolve_websocket` and `resolve_websocket_message` (in `execution_service/websocket_resolution.rs`) apply variables, collection defaults and auth; OAuth, Digest, NTLM, WSSE and SigV4 are refused for WebSocket with an explicit error. |
| `GraphQlSubscriptionService` | GraphQL subscriptions over the `WebSocketClient` port, keyed by a frontend-chosen session id. `resolve_graphql_subscription` resolves the request (variables, collection headers, auth, `http`→`ws` scheme) through `resolve_websocket`; `start`/`stop`/`end_all` drive one task per subscription. The message formats of `graphql-transport-ws` and legacy `graphql-ws` live in the pure `graphql_subscription/protocol.rs`; the server's chosen subprotocol decides the dialect. `subscribe` is sent only after `connection_ack` (10 s limit). Results leave as `GraphQlSubscriptionMessage` and `GraphQlSubscriptionStatus` events (exactly one terminal status). |
| `event_caps` | Live event size caps (`cap_with_marker`, `...[truncated N bytes]`): 256 KiB per gRPC stream message, GraphQL subscription result and WebSocket frame (binary is cut before base64 and flagged `truncated`), 1000 console entries and test results with 16 KiB per string. Only the event is capped, never history or the returned output. |
| `OpenApiSyncScheduler` | Background supervisor for OpenAPI sync checks (`openapi_sync_scheduler.rs`). See "OpenAPI sync scheduler" below. |
| `NetworkService` | Team Network node (`src/network/`): discovery, pairing, presence. Implements `DiscoverySink` and `PeerHandler`, holds the ports as `Arc<dyn Trait>`, and keeps one state behind one `std::sync::Mutex`. `start` never fails; a failure shows in `status()`. |
| `NetworkHandle` | Returned by `NetworkService::start`. `stop()` is idempotent and `Drop` calls it. It ends the background tasks and the listener. |
| `TransferService` | Send a collection to a paired node and receive one (`src/network/transfer*.rs`). `send(device, collection, include_environments, keep_in_sync)`: with `keep_in_sync` the offer carries a new `link_id` (refused with environments, or without `set_sync`), and both ends create the link from the same ZIP through `SyncService::link_first_transfer` (sender on `Completed`, receiver after a successful import, under the imported name; a failure is logged and never changes the outcome). A `CollectionOffer` whose `keep_in_sync` and `link_id` disagree, or whose id does not parse, is refused `failed`. `has_transfer` lets `FrameRouter` keep sync payloads off its ids. It holds the `rocket-network` `OutgoingTransfer` and `IncomingTransfer` machines and one `TransferLimits`, and delegates every rule to them (offer shape, expiry, chunk order, caps). Sender half (`transfer_send.rs`): `send` checks the node is paired and online, exports in `spawn_blocking`, hashes the ZIP, offers it, streams 32 KiB chunks at most 32 ahead of `ChunkAck`, then waits up to `SENDER_RESULT_TIMEOUT` (60 s) for `TransferResult`; a gone peer or no result after the final ack is `Unconfirmed`, before it `Failed`. A send that ended after at least one chunk left the node is audited with its outcome. Receiver half (`transfer_receive.rs`): a pending offer waits for `respond`; Accept stages chunks through `TransferStaging`, acks every 16 chunks and the last through one ordered pump per transfer, checks the size and SHA-256, imports through `CollectionImportPort` (the exact `UNSAFE_ZIP_MESSAGE` is `Rejected`, any other error `Failed` with the fixed text "The collection could not be imported."; the importer error is only logged), frees the file with `TransferStaging::release` (`discard` for an unfinished one; the service never purges), audits `completed`, `damaged` and `rejected`, and sends `TransferResult`. A received collection gets no trust record. Frames and `on_peer_gone` notices arrive through `TransferSink` into one queue and one worker that only enqueues work; the finalize step and outbound frames run as their own tasks. Frames for a closed transfer are ignored and `on_peer_gone` is idempotent. A 120 s idle limit (`Timing`) is the fallback. Whoever removes a transfer emits its single `Finished`. `new_with_timing` is a `#[doc(hidden)] pub` seam; `set_chunk_delay` and `set_tamper` are `#[cfg(test)]` hooks. |
| `SyncService` | Keeps a linked collection in step with one paired node (`src/network/sync*.rs`). `SyncDeps` holds the ports (`PeerSender`, `LinkRepository`, `SyncCollectionPort`, events, audit; no staging, no trust port). Links: `create_link` (serialised), `link_first_transfer` (at the first linked send, both ends, from the same ZIP bytes: strict validation, then `base_from_reader`; an unpaired peer gets no link), `links` (blocks; derives `broken` and `changes_waiting`, never saves), `unlink` (audits `NetworkLinkRemoved`; a sync of that link not yet answered ends `cancelled` first, one being applied refuses the unlink), and removal when the pairing goes (a peer-gone notice deletes that peer's links only when `peer_label` is `None` twice, 50 ms apart). A sync: the starter offers its manifest (`start`), the planner checks the link and `base_hash`, refuses names that differ only by letter case, plans with `plan_sync`, asks for the files it needs, publishes `ReviewReady` (`review`, `conflict_files`), sends the final plan with the new base hash and its files; the starter checks everything, publishes `ConfirmNeeded` (`confirm`), applies first and reports; the planner applies after the starter's `applied` (a result that overtakes the final ack of the last payload counts as that ack). Payloads ride the transfer machines, announced by `SyncOffer`, in memory, with their own `TransferLimits` (one session per peer, the pending cap, the per-peer start rate; follow-up offers do not count). Every received path, hash and size is checked before a dialog (spec section 7; `Unsafe` before `Damaged` before `BadPlan`); a sync never deletes or moves the root `opencollection.yml`, and `keep_both` is refused for settings files. Every apply is `apply_if_unchanged` against the manifest the plan was built from, an empty one included; its `Conflict` is one fixed text. The base is computed locally and only its hash crosses the wire. An unconfirmed sync keeps the new base and marks the link `out_of_date` (index item 17). `cancel`: before this node's dialog it ends `cancelled`, in the dialog it declines, while a review answer is checked or after the answer it is refused. Same worker design as `TransferService`: the handlers only lock, decide and spawn; a step that owns a phase with no deadline runs through `spawn_owned`, which ends the session if the step panics. Every `Finished.message` and error is a fixed sentence from `sync_checks.rs`; the in-memory ZIP is in `sync_payload.rs`. `claims` tells the router which frames are its own. `new` does not register as the sink; `new_with_timing` is the test seam. |
| `FrameRouter` | The one `TransferSink` of the node (`src/network/frame_router.rs`): frames `SyncService::claims` go to the sync service, the rest to `TransferService`; a `SyncOffer` that reuses an id `TransferService` knows is dropped; both get every peer-gone notice. |
| `PeerSender` | The calls `TransferService` needs from the node (`send_to`, `peer_label`, `is_online`, `set_transfer_sink`). `NetworkService` implements it. |
| `ExportBackedSyncCollections` | The real `SyncCollectionPort` (`src/network/sync_collections.rs`). `manifest` and `export_files` run `export_bruno(collection, false)` under the collection lock and strip the root folder (`zip_entries`), so a manifest hash is the hash of the scrubbed export bytes. `apply` plans every change against the local files first (only `is_synced_path` paths; Put merges YAML through `rocket_export::merge_file`; PutAs and Rename need a free target; a target that equals a different path ignoring case is refused; order is renames, deletes, puts), then stages them in one `CollectionFileTx` under `collection_lock` and recovers a failed commit. `apply_if_unchanged` re-reads the manifest inside the lock (without locking again) and returns `Conflict` when it differs. The manifest, the export and the base of a link leave out a nested `opencollection.yml` (`is_sync_entry`), because apply refuses one. Methods block: callers use `spawn_blocking`. |
| `sync_ports.rs` | `SyncCollectionPort`, `FileChange`, `SyncFile`, `is_sync_entry` and the base helpers `base_from_reader`, `base_from_zip`, `manifest_from_zip`: the base of a link from the sent ZIP, environments and nested collection files left out. |
| `NetworkDeps` | The ports `NetworkService` needs: identity, four repositories, discovery, transport factory, event publisher, audit publisher and the OS user name. |
| `WorkspaceService` | Create, switch, rename, close, delete, pin/unpin workspaces; link external collections; toggle multi-workspace mode; mutates the shared `Arc<Mutex<PathBuf>>` active path on switch. |

`ExecuteRequestOutput::sent_request` is the request as sent, with RocketVault values masked, for the frontend Console row (`execution_service/sent_request.rs`).

## Service Method Details

### CollectionService
- Every mutating method publishes an event (e.g. `CollectionCreated`, `RequestSaved`, `FolderDeleted`, `ItemMoved`, `CollectionSettingsSaved`) after the underlying repository call succeeds.
- `rename_request` — mutates only the `name` field inside the JSON; the filename stays the same, producing a single `Modify` filesystem event.
- `move_item` — moves a request or folder between collections or paths. After the move it publishes `FlowEvent::FlowRequestPathsRewritten`.
- `reorder_items` — reorders items within a folder by supplying the new name order.
- `list_request_examples` / `add_request_example` / `rename_request_example` / `delete_request_example` — saved examples of one HTTP request, stored in the request file's `examples` list. Add, rename and delete publish `RequestSaved` only after the repo call succeeds; list publishes nothing. Names are trimmed, 1 to 120 characters, no control characters, unique case-insensitively (`InvalidInput`, `AlreadyExists`). One example is at most 1 MiB as YAML. Non-HTTP files are `InvalidInput`.
- `get_settings` / `save_settings` — read and write per-collection settings (auth, headers, variables).
- Every method that writes collection files holds `collection_lock(repo, root)` (the key is `CollectionRepository::lock_key`, so two spellings of one folder share a lock) (`collection_lock.rs`, shared with the OpenAPI sync apply) for its whole load-modify-write, taken once per public method (the fair FIFO ticket lock is not reentrant; inner calls use `*_unlocked` helpers). Lock order: collection lock first, then the sync transaction guard. The sync apply never calls `CollectionService`. Read paths and `create` (no root yet) take no lock.

### WorkspaceService
Constructor takes **two** repos: `WorkspaceRepository` (registry) and `WorkspaceConfigRepository` (`workspace.yml` files).

| Method | Notes |
|---|---|
| `create` | Creates the directory, `collections/`, `environments/` subdirs, and writes `workspace.yml`. |
| `switch` | Updates `active_workspace_id` in registry and mutates `Arc<Mutex<PathBuf>>`. |
| `delete` | Removes the workspace directory through `WorkspaceRepository::remove_workspace_dir` (a missing directory is fine). Cannot delete `"default"` or the last workspace. |
| `close` | Removes from registry only (no disk deletion). Cannot close the last workspace. |
| `pin` / `unpin` | Toggles `workspace.pinned` in the registry. |
| `update_description` | Sets `workspace.description`; pass `None` to clear. |
| `open_workspace` | Registers an existing on-disk workspace; directory must contain `workspace.yml`. |
| `get_workspace_config` | Loads `WorkspaceConfig` from the workspace's `workspace.yml` via `WorkspaceConfigRepository`. |
| `link_external_collection` | Validates `opencollection.yml` presence, reads its `name` field, appends a `CollectionReference` to `WorkspaceConfig`. |
| `get_multi_workspace_mode` / `set_multi_workspace_mode` | Reads/writes `registry.multi_workspace_mode`. |

### GitAppService
Wraps `Box<dyn GitService>`. Every mutating operation publishes a `GitEvent`. Notable methods:
- `diff_staged` — diff for already-staged files.
- `checkout_remote_branch` — creates a local tracking branch. Rejects an existing local branch of the same name unless `force` is set, which instead resets that branch (and its upstream) to the remote's tip, discarding local commits absent from that remote. A forced reset of the *checked-out* branch still refuses to run over uncommitted tracked changes.
- `fetch` — no event published (read-only remote op).
- `conflicts` — read-only query, publishes nothing. `GitConflictDetected` has no publisher now.
- `abort_merge` — reverts in-progress merge; publishes `GitStatusChanged`.

## Key Patterns

- **Trait-object injection.** Every service takes `Box<dyn SomeRepository>` and `Box<dyn EventPublisher>` via its constructor. Apart from the I/O exceptions listed at the top, no concrete infra types appear in this crate, so services are testable with in-memory mocks.
- **DomainResult everywhere.** All fallible methods return `DomainResult<T>` from `rocket-shared`.
- **Variable resolution in `RequestExecutionService::execute`.** Precedence, lowest first: global environment, collection, RocketVault values, environment, folder chain, request, runtime (Flow vars are runtime). `begin_phases` resolves before any script without the runtime scope; `resolve_runtime_placeholders` (see below) fills what is left after the pre-request scripts.
- **Backend process env (`{{process.env.*}}`).** `resolve_request_parts` adds process env as the lowest layer only when `ExecuteRequestInput.backend_process_env` is set. Both it and `literal` are `#[serde(skip)]`, so an IPC input never sets them, because the frontend owns that pass for its own sends. `runner_sequence::build_step_input` sets the flag for every backend send: the runner, Flow request nodes, nested `rok.runRequest` targets and the assistant's `run_request`. The OS values come from the injected `HostEnv` (`with_host_env`, `ProcessHostEnv` in `src-tauri`); a service built without one resolves no host values. The gate is decided once (`ProcessEnv.allowed`). Fields in `literal` (wired Flow values, fetched or wired Auth-node tokens) are sent as written and resolve no scope at all, even after a script edits them (`FieldTemplate::literal`, `RequestTemplates.literal_headers`, #129), and client certificate fields resolve without process env, so response data never pulls a host or vault value in. Every `.env` value joins `secret_values`, the used host values go to `run_process_env_values` (masked in `run_request` output), and a withheld placeholder sets `process_env_withheld` and adds one Console line in the top-level caller's `finish_phases`. A send's own used host values, plus those of earlier Flow nodes, are masked in its History URL, `RequestExecuted` URL, executor error and script console. A failed send hands its mask set and used host values back through `SendCapture` (`execute_capturing_masked`), so its `exchange`, `debug_request` and later nodes are masked too, and a repeat-until condition uses the attempt's masks. Known limit: a nested `rok.runRequest` child in a Flow Request gets no upstream Flow host values. Flow planning (`run_with_options_unpinned`, entered through `run_with_options`) reads the variable scopes lazily, only when some text has `{{`. See `docs/features/collection-trust.md`.
- **Process env scope for realtime sends.** WebSocket, GraphQL subscriptions and gRPC resolve process env through `process_env_for_scope` with the required `processEnv` IPC field (`ProcessEnvScopeInput`: `scratch`, `collection`, `closed`). A `collection` on the input keeps its own gate and a closed scope closes it; the scope never widens it.
- **Client certificates and RocketVault.** `client_certificates::environment_client_certificates(repo, environment_name, collection_dir, vars, external_secrets)` turns the environment's persisted `ClientCertificate` entries into `ResolvedClientCertificate`s. `RequestExecutionService::resolve_request` and `OAuth2Service` (`resolve_get_token_request_with_secrets`, `refresh_token_with_secrets`) both call it, so they resolve identically. Per entry: `{{placeholders}}` in the domain, file paths and passphrase, then relative paths joined onto the collection folder, then each `certificateSecret`, `privateKeySecret` and `pkcs12Secret` reference looked up in the RocketVault map (`alias.secretName`). A found reference becomes `CertificateSource::Inline` (PKCS12 secrets are base64 and ignore whitespace). A missing, empty, undecodable or over-1-MiB (`MAX_INLINE_SECRET_BYTES`) reference makes the entry `CertificateMaterial::Unavailable { reason }`, which the executor turns into an error only when that entry is selected for the URL. Load tests call `resolve_request` with an empty secrets map, so vault-backed certificates are `Unavailable` there. Multi-line vault values are masked whole and line by line (`redaction::redaction_forms`). A script or declarative-action write whose value contains a vault value is not persisted (environment, global environment, collection, folder and request scopes). It is kept in `var_ctx.runtime` with a console warning instead. This hold-back is best-effort: base64 or URL-encoded forms, slices of a secret and secrets under `MIN_REDACTION_LEN` are not caught. A `vault` entry (a certificate stored in a RocketVault vault, picked by name) becomes `CertificateMaterial::Deferred`: no bytes are read while resolving. `vault_certificates.rs` fetches only the selected entry just before the send (`with_vault_certificates`, and for OAuth2 token URLs) through `VaultSecretFetcher::fetch_certificate`, and a failed fetch turns that entry into `Unavailable` with no fallback. History, scripts and events keep the names-only form. Load tests treat vault certificates as unavailable (`unavailable_in_load_tests`). `SecretManagerService::save` and `delete` call `VaultSecretFetcher::forget_connection` to drop cached certificate ids. The PKCS12 export asks for `compat: legacy`, and whether that loads on OpenSSL 3 is untested until the ignored live test runs.
- **Phase-callable execution.** `RequestExecutionService::execute` is a thin composition over `begin_phases` → `run_before_request_phase` → `send_request` → `run_after_response_phase` → `run_tests_phase` → `finish_phases`, all sharing one `PhaseState`. `CollectionRunnerService` drives the same methods one phase at a time so it can act on `skip_request` before the send and on `next_request` after every phase. Do not add phase logic to only one caller. `run_before_request_phase` ends with `resolve_runtime_placeholders` (`execution_service/runtime_fill.rs`). The first resolution (`resolve_request_parts`) records every field (URL, path parameters, query, headers, body, form values, auth) as a `FieldTemplate`: the original text, each placeholder's name and the value it got. After the scripts and before-request actions, a field the scripts did not rewrite is rendered again from its template. `begin_phases` keeps each name's resolved value in `RequestTemplates.first_values`. After the scripts, `Overrides::between` lists every runtime name, each name whose resolved value changed in any scope (scope order still decides, so a request variable still hides an env write) and each name that no longer resolves (its placeholder is sent as written). Both passes fill nested names with `rocket_environment::expand_nested`: only stored-scope values are expanded (never runtime, Flow or script values, RocketVault names or `process.env.*`), at most 3 levels deep, and the second pass compares expanded values (a name whose raw value a script changed is put in raw; the `$dynamic` cache is `RequestTemplates.nested_dynamic`). Those names get their new value, every other placeholder keeps its first value, so `{{$dynamic}}` values are made once, and no value is read for placeholders again (an env value like `https://x/?k={{vault.key}}` stays literal). Path parameters are substituted again from their rendered values, so values never cut into each other; in a URL a script rewrote, `swap_path_values` swaps a parameter's encoded first value only in whole path segments (or a segment prefix before a suffix such as `.json`), once per segment, never in the host or query. The single send and the frontend runner resolve in the frontend first, so the backend pass sees placeholders the frontend left as written. Nested filling (`expand_nested`, built by `expanded_send` and `RequestExecutionService::send_variable_map`) covers stored scopes only, never vault names or `process.env.*`: at most 3 levels deep, 64 KiB per value and a 1 MiB work budget per top value with 64 bytes charged for every fill. Known limit: a crafted collection with thousands of such values slows its own sends (about 0.8 ms per name). `expanded_send` also returns the secret forms to mask: the filled value of a secret name and, through `expand_nested_recording`, the text put in for a secret inside another value when a fill was cut (depth or budget) and part of it stayed as written. Plain literal text of a secret is never masked on its own. Flow's `send_vars`, `send_time_credential_value` and `check_wired_origin` use the same map, and `secret_values` adds the same forms to Flow's run-level masks. Text a script wrote (`req.setUrl`, `setHeader`, `setBody`) is filled only for names the first pass could not resolve and names the script itself added, from every scope but RocketVault. Residual: script text built from response data with such a name (`req.setUrl(res.next)`) is filled from those scopes, as in Bruno and Postman. The runtime scope includes the runner's carried variables, `execute_nested`'s seeded runtime and Flow vars. A URL this pass changes goes through the request guard. A runner step with `skipRequest()` skips the pass. A runtime value whose key is in `VariableContext.secret_keys` (secret variables of any scope, whatever their value length, plus vault keys) is added to `secret_values` first, so history, console and outputs mask it. The Flow sent record (`flow_debug::sent_masks`) also masks the send's `run_secret_values` and `run_sent_credentials`. Client certificate fields are not resolved again.
- **Folder script chain.** `begin_phases` reads the folder chain once (`folder_chain`, which calls `get_folder_chain_settings`) and stores `PhaseScripts` on `PhaseState`, ordered by `rocket_collection::chain_scripts` and `CollectionSettings.script_flow` (`execution_service/script_chain.rs`). Each script is its own engine run, built from the current `state.var_ctx` and `state.http_request`, so later scripts see earlier mutations and runtime variables. Sandbox mode, file scope and vault hold-back are the request script's. The first error wins and ends its phase; a folder script's error reads `Folder "<path>" <phase> script: <message>`. In a run, `skipRequest()` ends the pre-request chain; a single send ignores it. Inline Flow requests (`__flow_inline__/`) and GraphQL introspection (`skip_folder_scripts`) run no folder scripts. `references_alias` also scans folder scripts, headers and auth. Test results carry no folder label.
- **Script host.** Every request-phase script run gets an `ExecutionScriptHost` (`execution_service/script_host.rs`) through `ScriptEngine::execute_with_host`. It borrows the service, so no `Arc` or second engine exists. `rok.sendRequest` builds an `HttpRequest` with the calling request's resolved options (TLS, redirects, cookie jar, client certificates, vault certificates fetched first) and enforces the script's timeout itself. `rok.runRequest` finds the target with `execution_service/run_request.rs` (file path relative to the collection root, no extension; WebSocket and gRPC are skipped), checks `PhaseState.run_chain` (no revisits, at most 5 nested levels) and runs `execute_nested`, which drives the same phases. Nested runtime changes and re-read env, global and collection scopes flow back to the calling script. After a script started a nested run (`PhaseState.nested_ran`), the pre-request loop reloads env, global and collection from storage (`reload_stored_scopes`), so the next script and the second pass see what the nested run saved. A failed storage read during that reload keeps the current scopes and adds a Console warning. `apply_actions` and Flow transform and condition nodes run without a host.
- **Cookie jar lock.** Every writer of the cookie jars takes the shared `CookieJarLock` (from `rocket-http`): `CookieService` save and clear, the reqwest store and `ScriptCookies` (`script_cookies.rs`, used by `ExecutionScriptHost::cookies` for `rok.cookies` and `jar()`). `src-tauri` makes one lock and gives it to all three. A new writer must take it too.
- **Header and auth merging.** `RequestExecutionService::inherited_auth_and_headers` is the one place defaults apply: collection headers, then the folder chain (`folder_chain` → `get_folder_chain_settings`, outermost first, merged by `rocket_collection::inherited_headers`), then the request. The request wins by key and a disabled header never shadows. A request auth of `none` or `inherit` takes the nearest folder auth (`resolve_folder_auth`), then the collection auth. `resolve_request` (single send, runner, Flow, load test, GraphQL introspection) and `resolve_websocket` (WebSocket, GraphQL subscriptions) both call it. A folder chain that fails to load fails the send. Inline Flow requests (`FLOW_INLINE_PATH_PREFIX`) inherit no folder settings. gRPC still applies collection defaults only, because `GrpcExecuteInput` carries no request path.
- **One send, one workspace.** `execute`, `execute_capturing`, `CollectionRunnerService::run` and `FlowExecutionService::run_with_options` run inside `with_send_pins` (`execution_service/send_pins.rs`, a tokio task-local). At the start it pins the collection repo, the global env repo and the collection env factory (`pinned()` on `CollectionRepository`, `EnvironmentRepository` and `EnvironmentRepositoryFactory`), so a workspace switch mid-send never writes one workspace's script results (environment, collection, folder or request variables) into another. Inside `RequestExecutionService`, collection access goes through `send_collection_repo()` and environment lookups through `global_env_repo()` / `regular_env_repo()`, never `self.collection_repo` / `self.env_repo` directly. Nested calls keep the outer pins. Work spawned onto another tokio task does not inherit them. The runner's and Flow's own repos (run set, flow file) are read at run start and are not pinned.
- **Non-fatal history write.** `let _ = self.history_repo.save(...)` is intentional — a history persistence failure must not abort the response.
- **Event publishing is fire-and-forget.** Services publish events (the per-context enums such as `CollectionEvent` and `AcpEvent`) after successful operations; errors from downstream listeners are not propagated back. `ExecutionEvent` (`execution_events.rs`: request, runner, WebSocket, GraphQL and script events, 12 variants, plus the payload enums `WebSocketDirection`, `WebSocketPayloadKind`, `GraphQlSubscriptionEventKind` and `WebSocketSessionState`) and `OpenApiSyncEvent` (`openapi_sync_events.rs`: `OpenApiSyncChecked` and `OpenApiSyncApplied`) are defined in this crate and implement `PublishedEvent` with the same channels and JSON as the old `DomainEvent` variants (the enum is gone). A test recorder that must see them downcasts the new type, or reads `RecordingPublisher::execution_events()`. Collection events come from `RecordingPublisher::collection_events()`.
- **The Team Network service lives in `src/network/` on purpose** (ruling D-02 in `docs/engineering/ddd-decisions.md`: one `rocket-app`, split by context only later with its own spec). It holds `Arc<dyn Trait>` ports from `rocket-network` and spawns tokio tasks like `openapi_sync_scheduler.rs`; it never imports `rocket-infra`.
- **Tests use inline mocks.** Each service module contains its own mock implementations in `#[cfg(test)]`. `tempfile` is used in `WorkspaceService` tests that require real directories. A `CollectionRepository` stub that needs only a few real methods uses `NullCollectionRepo` from `test_doubles.rs` (`new()` is lenient, `strict()` panics on unhooked methods, `lenient_for` opts methods out, `on_*` hooks override one method). `execution_service.rs` keeps its own doubles on purpose, as `test_doubles.rs` states at the top.

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

## OpenAPI sync scheduler (`openapi_sync_scheduler.rs`)

`OpenApiSyncScheduler::new(service_factory, events, SchedulerConfig)` and `start()` run one loop on the tokio runtime: a 30 s launch delay, then a 60 s tick, at most 5 checks per tick, one at a time. Each tick runs `OpenApiSyncService::due_targets(now)` through `spawn_blocking` (one local-state read per collection, plus one source read for each enabled one) and runs each `check` on a spawn_blocking thread that drives the check with block_on, so a panic fails that check and not the loop. `due_targets` reads timing only from `OpenApiLocalState` through `next_check_at`, never from the legacy fields of the source file. It needs apply deps (the collection list) and is empty without them. A collection with no local state or the interval off is never listed. The opt-in is machine-local consent (`rocket_collection::OpenApiConsentStore`, file `~/.rocket-api/openapi-sync.yml`, keyed by canonical root and `source_id`, holding the structural `location_key` of the approved location): `due_targets` skips a collection without consent for its current location, `status()` reports `consent` (`granted`, `missing`, `locationChanged`) and shows interval off and no token read unless it is granted, and `check` returns `Failed { code: "consent_required" | "location_changed" }` before any recovery, keychain read or request. The keychain read (`read_token`, `stored_token`) is gated too. The record holds a structural key (`location_key`: the `SpecLocation` variant and every field as JSON), never the display text, because different locations can display alike; an old record without the key reads as missing. `check` gates twice, before and after `settle` (journal recovery can rewrite the source file), and fetches with the source object that passed the second gate. Only `grant_consent(root, expected_location)` grants (under the collection lock, it reloads the source, requires the shown text to match and refuses a location that does not survive `parse_spec_url(display_url)`); `grant_consent_for_link` is the import path. `set_interval` (not Off) and `set_token` need `Granted` before any write, including the keychain write, and `set_interval(Off)` always works. `opt_in_root` stays readable but is ignored and no longer written. The keychain token is keyed by `source_id` only, so approving a cloned folder that reuses a `source_id` uses that token against the clone's own GitHub location. A service without `with_consent` treats every source as unapproved. `src-tauri` starts the supervisor in setup (`start_sync_scheduler`), restarts it in `switch_workspace` and stops it on `RunEvent::Exit` (`SyncSchedulerSlot::stop`). A collection whose local or source file cannot be read is listed first, so its `check` fails before any request. An error from `check` (such a file, a failed recovery, a panic) cannot store a failure count, so the scheduler holds an in-memory delay per collection root (15 min, doubled, capped at 6 h, started when the check failed, cleared by a success) and skips it until then. After each check it publishes `OpenApiSyncChecked { collection, status }` (`unchanged`, `changed` or `failed`; an error is `failed`), a refresh signal with no URL or message. `SchedulerHandle::stop` (also on drop) ends the loop, even during a sleep, and never cancels a check in flight. Logs carry codes and counts only.

`OpenApiSyncService::check` also follows the files a spec refers to by URL (remote `$ref`). A main-spec 304 is confirmed file by file (`refs_unchanged_labeled`) when refs are stored, and skipped for a full fetch when `remote_refs_incomplete` is set; a failure on that path keeps its usual code with a fixed message naming the file. A fresh body is bundled by `bundle_spec`, and the bundled text is what is hashed and kept as the pending copy. A bundle that lost a file stored last time (`incomplete` and a stored URL missing from `Bundle.refs`) fails the check with code `ref_unavailable` and leaves the source, hash, pending copy and stored refs as they were; a file that never loaded does not fail the check. See `crates/rocket-import/CLAUDE.md` (Remote references).

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
callback sender is outside the run. A Transform script, an Input value and an
edge expression that contain `{{` also hash the plain values they name
(global, collection and environment variables, nested names followed, `$`
names skipped; see `plain_placeholder_hash`), so a changed plain variable
counts as an edit of that node. The resolved text is never stored, only
hashed. A changed RocketVault or `process.env` value does not invalidate a
cached run. Request nodes ignore variable changes in their own url, headers
and body (decision D5), but the edge expressions into a Request node are
hashed too, because the runtime resolves them (a deliberate exception for edge
scripts only). A `{{$name}}` inside a variable value is blanked in the
hash, so swapping one dynamic name for another there does not invalidate a
cached run. Not detected: Request node variable value changes, folder and
collection settings a saved request inherits, shared `.js` script files, and
auth tokens supplied for the partial run (they are not compared with the base
run's). Values the base run masked are masked again under `}}prev-run.<n>`
external-secret keys. No `{{template}}` can reference them, but a script can
read one by exact name (`rok.getSecretVar`).

## Flow run ids (`flow_run_id.rs`)

`FlowExecutionService::run_with_options(exec, input, tokens, FlowRunOptions { run_id, partial, owner })`
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
wire's value (`record_wire`) with a hash of the wire's script
(`expression_hash`, FNV-1a 32-bit hex, no value) so the UI can mark a wire
stale, an `auth` wire as `credential: true` with no
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

`RunCallbacks::urls()` lists `(node id, name, url)` per Wait for callback
node; `infos()` gives the same list without the URL. The URL holds a bearer
token and events are broadcast to every webview, so no event carries it:
`FlowRunStarted.callbacks` names the nodes only (omitted when empty, an old
`url` key is ignored). The UI fetches the URLs with the
`get_flow_run_callbacks` command, which reads
`FlowExecutionService::callback_urls(run_id, caller)`. The registry entry is
written right after the endpoints open, before `FlowRunStarted`, and removed by
`RunRegistration`'s drop on every end path (finish, error, cancel). The entry
holds the run's owner (`FlowRunOptions.owner`, an opaque tag; the `run_flow`
command sets the calling window's label). A run with an owner hands the URLs
only to a caller with the same tag, so another webview (such as the
`oauth2-auth` window) and a caller of `None` get an empty list. A run without
an owner (MCP, runner, tests) hands them to any caller. An unknown or
finished run id, or a foreign run, returns an empty list, never an error, so
existence does not leak. `FlowRunCallback` hides
the URL in `Debug`. Never put the URL in a `FlowRunSummary`, a step, an
exchange, a log, an event or history.
Each endpoint's full URL and bare token also join the run's
`external_secrets` (`flow-callback.<node id>.url` and `.token`, from
`RunCallbacks::mask_secrets`), so a sender that echoes its URL is masked in
every other sink. A token under `MIN_REDACTION_LEN` is not masked alone (a
real one is 32 characters); the full URL still is.

## Flow lint (`flow_service.rs`)

`FlowService::lint_with_environment(collection, flow, env)` lints the graph it
is given (the canvas, saved or not) and never fails or touches the
repository. First comes the full `validate_report` error list, capped at 50,
as `invalid_graph` error lints that carry their node or edge ids. Then come
the `rocket_flow::validate_with_warnings` warnings, asked of a
`CollectionLintContext` built from the `LintSources` and the environment names
(`with_lint_sources`; without sources the context is `NoLintContext` and those
rules stay silent). `lint` is the same with no environment. The async
`lint_flow` IPC command takes `environmentName` and `globalEnvName`. `save` and
the run path never call lint, so lints never block either. `validate` is linear
in the flow size.

`list_entries` returns an unparseable flow file as a corrupt entry with an
`error` instead of hiding it. A run is refused over `MAX_RUN_STEPS` (10,000
nodes), checked in `load_ordered_nodes` before any event is published.

## Assistant chips (`McpToolService::build_chip_resource`, `mask_response_chip`)

The composer's context chips are built here, never in the frontend. The text comes from the same masked views as the MCP read tools (`assistant_chip_text.rs` lays it out), gets a last pass with the known secret values, and is capped at 8 KB with any open code fence closed before the marker. Plain variables with a credential-like name are masked in the views themselves.

A response chip masks the tab's status line, URL, headers (`Link` entry by entry), test errors and body with: secret variables of every scope and environment of the collection, the RocketVault secrets of every environment of the collection (fails closed: if a binding cannot be resolved or takes over 8 s, the chip is refused with "could not resolve vault secrets to mask this response; try again"), and the literal credentials of the saved request and of the tab's request.

Chip secret lookups fail closed: a real read or parse error of the workspace config, any variable scope, the collection or folder settings, the environment list or the saved request refuses the chip (a missing file or a request that is not saved yet is fine). The folder chain variables use `get_folder_chain_variables_strict`, so a `folder.yml` that exists but cannot be read or parsed refuses the chip, while the run path still skips it. Known residual: `FsEnvironmentRepo::list` skips an environment file that fails to parse (warn only), so the secrets of a corrupt environment file cannot be masked.

Values that exist only at run time in the frontend are not known to the backend, so they are not masked: a script's `setVar` or `setEnvVar` results, and OAuth tokens that the tab does not send as its auth. A secret that is not held in a variable, a literal credential or the vault, and is echoed under a neutral field, cannot be recognised either.

**Script host limits.** `rok.sendRequest` runs `check_request_guard` on its first URL only, so redirect hops are not checked yet (see R-68). Nested runs carry `unresolved_aliases` and refuse a send that references one. At most 16 nested runs (`MAX_ACTIVE_NESTED_RUNS`) are active per send.

## OpenAPI sync series 2

`openapi_sync_spec_view.rs` (`spec_view`), `openapi_sync_reset.rs` (`reset`, `restore`, `discard`, one file transaction each under `collection_lock`) and `openapi_connect.rs` (`preview`, `confirm`, `request_digest`) extend `OpenApiSyncService`. Reset and View spec read local files only. `confirm` fetches the spec once more, checks the request digest and the spec hash against the preview, commits the files in one transaction, then approves the typed link, sets the interval and stores the token. Later failures are warnings. Errors cross IPC as fixed sentences. See `docs/features/openapi-sync-ux.md`.
