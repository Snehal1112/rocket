# Collection trust gate

Spec: `docs/superpowers/specs/2026-10-11-collection-trust-gate-design.md`.

A collection file only requests elevated access. `~/.rocket-api/trust.yml` holds what the user allowed on this computer. What runs is the effective value.

## Capabilities

| Capability | Requested by | Gate |
|---|---|---|
| Developer mode | `sandboxMode: developer` | Script ops and `rok.getProcessEnv` |
| Extra script folders | `additionalContextRoots` | `require()` roots, per approved root |
| Agent request runs | `agentAutonomyEnabled: true` | Assistant `run_request`, ACP MCP servers |
| Host environment | none (implicit) | `{{process.env.NAME}}` on every protocol |

## Rules

- Never read `.sandbox_mode`, `.script_context_roots` or `.agent_autonomy_enabled` outside `collection_trust.rs` (a guard test enforces it). Use `effective_capabilities`.
- `save_collection_settings` never grants and keeps the three file fields as they are. Only `CollectionTrustService` writes capabilities.
- The banner approves with the fingerprint the user saw (`grant_requested_capabilities`). Freeze it when the review opens.
- Services built without a trust store deny everything (`DenyAllTrustStore`). Tests that need access use `InMemoryTrustStore::allow_all()`.

## Host environment (`{{process.env.*}}`)

- Backend (WebSocket, GraphQL subscriptions, gRPC): `process.env.*` comes from the `processEnv` scope the frontend sends (`scratch`, `collection`, `closed`, from `processEnvScopeOf(tab)`). A request's `collection` keeps its own gate (`RequestExecutionService::process_env_allowed`) and a `closed` scope closes it, so the scope can only narrow access.
- HTTP: `get_process_env_vars(collection?)` returns an empty map when the collection is not allowed. `useProcessEnvVars(collection)` and `execute-request.ts` pass the collection.
- Backend-driven HTTP (Flow request nodes, the backend collection runner, `rok.runRequest` targets, the assistant's `run_request`): `resolve_request_parts` fills `process.env.*` under the same gate, decided once per send. These callers opt in with `ExecuteRequestInput.backend_process_env`, which is `#[serde(skip)]`. An input that comes over IPC (the single send, load test, introspection, cURL) never gets backend process env, because the frontend owns that pass and a History tab arrives with no collection.
- On backend sends, values wired from an earlier response and fetched or supplied Auth-node tokens (`ExecuteRequestInput.literal`) are sent as written: no scope resolves in them, `process.env`, environment, collection and RocketVault included, and the runtime pass leaves them alone even after a pre-request script edits them (#129). An Input node resolves its own value once, with global, collection, environment, RocketVault and `{{callback.<name>}}` variables; folder and request variables do not apply to it. Client certificate fields never resolve `process.env.*`. Text from the saved request, its folder and collection settings, and a static Auth node (`Bearer {{process.env.TOKEN}}`) still resolves.
- A Flow `url` wire may not change the origin (scheme, host and port) of a Request node's saved URL, resolved with the send's variables, so a response cannot choose where the saved headers and auth go (#130). The node fails and names both origins, never a path or query. A wired URL that holds a placeholder or is not absolute is refused too. The node's `allow_wired_host` switch ("Allow a wired URL to change the host") opts out. The check is `check_wired_origin` in `flow_execution_service.rs`.
- Masking on backend sends: every `.env` value of the send joins its secret values (History, Console, Flow traces, outputs). The host values a request used, OS values included and nested runs merged, are in `ExecuteRequestOutput.run_process_env_values`, and `run_request` masks them in what it returns to the agent.
- A backend send whose placeholder is withheld adds one Console line with the frontend's wording, only when the request was actually sent, in the caller's Console for a nested run (`process_env_withheld` on the output). A manual single send gets no backend line, because the frontend already warns.
- A request with no collection (scratch) keeps full access. A History tab is not scratch: it follows its entry's collection, or gets no host environment when the entry has none (see Known limits).
- `.env` files named by an environment's `dotEnvFilePath` (and its parents') are read only when this gate allows host environment access. The default `<collection>/.env` (and the workspace base `.env`) is read under the same gate, with no environment selected, as the lowest layers. A denied gate means no file is read at all. The real OS environment wins over a `.env` value with the same name, and every `.env` value is masked in the Console (reveal toggle shows it).
- Before a send, `warnIfProcessEnvWithheld` (`src/lib/process-env-gate.ts`) adds a console line and a toast when the text uses a placeholder and access is withheld. The send is never blocked.
- Who has it: collections that existed at the upgrade (grandfathered), collections created in Rocket. Clones and imports have none until allowed.

## Migration

First start after the upgrade grandfathers every existing collection once. `TrustMigrationNotice` lists the ones that kept Developer mode, extra folders or agent runs until the user clicks OK.

## Commands

`get_collection_trust`, `set_collection_capability`, `set_collection_context_roots`, `grant_requested_capabilities`, `revoke_collection_trust`, `get_trust_migration_notice`, `dismiss_trust_migration_notice`, `get_process_env_vars`, `get_dot_env_status` (gated: a denied gate returns `blocked` and reads no file, only the base folder is filled). The event is `collection-trust-changed`.

## Follow-ups

- Per-row revoke of one extra script folder (today only "Forget permissions").
- A revoke that clears the grant without writing the collection file.
- Host environment count in the migration notice.
- Static script scanner and request guard from git (separate specs).
- If `config.proxy` or `config.clientCertificates` in `opencollection.yml` are ever honoured, they must join this gate.

## Known limits

- Flow Input nodes, Flow Auth-node OAuth2 token fetches and `OAuth2Service` token requests do not resolve `{{process.env}}` (issue #112 non-goals).
- Values wired from a Flow response still resolve collection, environment and vault placeholders, as before #112; only host environment is kept out of them.
- A request resent from History keeps the gate of the collection it was sent from: the History tab carries the entry's collection in `historyCollection` (never in `source`), and the send, the Copy as cURL, the load test and the variable preview use it as the process env scope (`processEnvScopeOf` in `execute-request.ts`). A History entry with no collection (old entries, scratch sends) gets no host environment (`CLOSED_PROCESS_ENV`), and the same warning is shown.- History stores the resolved URL, so a host value that a send was allowed to resolve (a path placeholder, or any non-`.env` OS value) is stored in clear text. Redacting it is a follow-up.
- External folders linked into a workspace are not watched, so a pull there shows in the banner only after the next trust refresh (reopen the tab or grant/revoke).
- The migration notice has no host environment count, and Review opens the first listed collection by its directory name.
- The HTTP send asks the backend for the process env map on every send (one IPC), and the process env queries are invalidated on trust, collection and git changes.
