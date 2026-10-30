# WebSocket, GraphQL subscriptions and gRPC follow the process env scope

Ticket: https://github.com/Snehal1112/rocket/issues/131 (follow-up of #112 and #116)

## Goal

A WebSocket, GraphQL subscription or gRPC send from a History tab whose entry has no collection, or comes from an untrusted collection, gets no host environment (`{{process.env.NAME}}` and `.env` values), the same as the HTTP resend. Saved requests and scratch tabs behave as today.

## Overview

#116 gave History tabs a process env scope for HTTP (`processEnvScopeOf`), and #112 made backend process env opt-in for HTTP. The three protocols that resolve variables in the backend were left out. Their frontend inputs carry only `collection: tab.source?.collection`. A History tab has no `source`, so the backend sees `collection: None`, treats the send as scratch and fills in the full host environment.

The reachable path today is a GraphQL subscription: a History tab is an HTTP tab, its body can be set to GraphQL with a `subscription` operation, and `RequestPanel` then subscribes through `graphql_subscribe`. A History tab cannot be turned into a WebSocket or gRPC tab in the UI (`requestType` is set only at creation, `src/lib/pane-utils.ts:201,204`, or from a file), so those two paths are fixed as defence in depth, through the same code.

`collection-trust.md` lists this as a known gap (`docs/features/collection-trust.md:57`).

## Current state

| Area | Today | Where |
|---|---|---|
| Gate | `process_env_allowed(None)` is `true` (scratch); `Some(name)` follows the trust store, an unknown name is denied | `crates/rocket-app/src/execution_service.rs:1174` |
| Values | `process_env_for(collection, environment, global)` returns empty and reads no file when denied | `execution_service.rs:1200` |
| Resolver | `build_variable_context_with_process_env` always calls `process_env_for(collection, ...)` | `execution_service.rs:1314` |
| WS connect, GraphQL subscribe | `resolve_websocket` calls the resolver with `input.scope` | `crates/rocket-app/src/execution_service/websocket_resolution.rs:150`, `crates/rocket-app/src/graphql_subscription.rs:191` |
| WS send, subscription texts | `resolve_websocket_message` calls the resolver | `websocket_resolution.rs:216`, `graphql_subscription.rs:99` |
| Scope DTO | `WebSocketScope { collection, environment_name, global_env_name, request_path }`, all `#[serde(default)]`, derives `Default`; flattened into `WebSocketConnectInput`, `WebSocketSendInput`, `GraphQlSubscribeInput` | `websocket_resolution.rs:18-30,58-63`, `graphql_subscription.rs:41` |
| gRPC | `GrpcExecuteDto` has its own scope fields; `resolve_input` calls the resolver for `grpc_unary_call`, `grpc_start_session` and `grpc_list_services` | `src-tauri/src/commands/grpc.rs:13-29,48-64,68,80,119` |
| gRPC method list | `GrpcMethodPicker` calls `grpcListServices(buildInput(), refresh)` with the panel's `buildInput` | `src/components/grpc/GrpcMethodPicker.tsx:41,84` |
| Frontend WS/GQL scope | `scopeFor(tab)` sends `collection: tab.source?.collection` | `src/lib/websocket-session.ts:25-33`, used by `src/lib/graphql-subscription-session.ts:28` |
| Frontend gRPC input | `buildInput` sends `collection: tab.source?.collection` | `src/components/grpc/GrpcPanel.tsx:75-84` |
| HTTP rule | `processEnvScopeOf(tab, tabId?)`: `source` wins, a `history` tab uses `historyCollection ?? CLOSED_PROCESS_ENV`, else `undefined` (scratch) | `src/lib/execute-request.ts:110-115` |
| HTTP environment | the active environment is used only when the scope collection is the active collection | `src/lib/execute-request.ts:122-125` |
| Warnings | `warnIfProcessEnvWithheld(input.collection, ...)`, so a History tab never warns | `websocket-session.ts:63,78`, `graphql-subscription-session.ts:45`, `GrpcPanel.tsx:107,122` |
| Previews | `useProcessEnvVars(collection)` with the tab's `source` collection | `src/hooks/useWebSocketVariableContext.ts:25`, `src/hooks/useGrpcVariableContext.ts:46` |
| HTTP preview pattern | `useProcessEnvVars(name, scope !== CLOSED_PROCESS_ENV)` and an empty map for a closed scope | `src/components/request/RequestPanel.tsx:571-578` |

Gaps: the backend cannot tell a History tab from a scratch tab on these paths. The warning and the previews use the wrong scope too.

## Non-goals

- Changing what `collection` drives (collection variables, headers, auth, folder chain). It stays `tab.source?.collection`.
- The Flow caller of `build_variable_context_with_process_env` (`crates/rocket-app/src/flow_execution_service.rs:2061`). It keeps its current signature and behaviour.
- Letting a History tab become a WebSocket or gRPC tab.
- Redacting host values that History stores in clear text.

## Design

Flow: `tab -> processEnvScopeOf(tab) -> toProcessEnvScopeInput(scope) -> IPC processEnv -> process_env_for_scope -> process_env_for(...)`.

### 1. Wire type `ProcessEnvScopeInput` (rocket-app)

New IPC DTO in `crates/rocket-app/src/execution_service/websocket_resolution.rs`, re-exported from `rocket_app` next to `WebSocketScope` (`crates/rocket-app/src/lib.rs:117`):

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProcessEnvScopeInput {
    Scratch,
    Collection { name: String, #[serde(default)] environment_name: Option<String> },
    #[default]
    Closed,
}
```

JSON: `{"kind":"scratch"}`, `{"kind":"collection","name":"api","environmentName":"dev"}`, `{"kind":"closed"}`. `Default` (`Closed`) exists only so Rust code and tests can build `WebSocketScope::default()`; it is the fail-closed choice.

`WebSocketScope` gains `pub process_env: ProcessEnvScopeInput` with no `#[serde(default)]`, so an input without `processEnv` fails to deserialize. `GrpcExecuteDto` gains the same field, also required.

### 2. Resolution rule (rocket-app)

New method on `RequestExecutionService` in `execution_service.rs`, next to `process_env_for`:

```rust
pub fn process_env_for_scope(
    &self,
    collection: Option<&str>,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
    scope: &ProcessEnvScopeInput,
) -> ProcessEnv
```

| `scope` | `collection` | Result |
|---|---|---|
| `Closed` | any | `ProcessEnv::default()`, no file read |
| `Scratch` or `Collection { .. }` | `Some(c)` | `process_env_for(Some(c), environment_name, global_env_name)` (the collection's gate wins) |
| `Collection { name, environment_name: e }` | `None` | `process_env_for(Some(name), e.as_deref(), global_env_name)` |
| `Scratch` | `None` | `process_env_for(None, environment_name, global_env_name)` (today) |

New sibling of the resolver, same file:

```rust
pub fn build_variable_context_for_scope(
    &self,
    global_env_name: Option<&str>,
    collection: Option<&str>,
    environment_name: Option<&str>,
    request_path: Option<&str>,
    external_secrets: &HashMap<String, String>,
    scope: &ProcessEnvScopeInput,
) -> HashMap<String, String>
```

It is `build_variable_context_with_process_env` with `process_env_for_scope` in place of `process_env_for`. `resolve_websocket`, `resolve_websocket_message` and `grpc.rs::resolve_input` call it with the input's `process_env`. `build_variable_context_with_process_env` stays for Flow; its doc comment drops the sentence naming WebSocket, GraphQL subscriptions and gRPC.

### 3. Frontend mapping

In `src/lib/process-env-gate.ts`, next to `CLOSED_PROCESS_ENV`:

```ts
export type ProcessEnvScopeInput =
  | { kind: 'scratch' }
  | { kind: 'collection'; name: string; environmentName?: string }
  | { kind: 'closed' };

export function toProcessEnvScopeInput(scope: ProcessEnvScope): ProcessEnvScopeInput
```

- `undefined` gives `{ kind: 'scratch' }`, `CLOSED_PROCESS_ENV` gives `{ kind: 'closed' }`.
- A collection name gives `{ kind: 'collection', name }`, plus `environmentName: activeEnvId` only when `name === useEnvStore.getState().activeCollection` and `activeEnvId` is set, the same rule as `getProcessEnvVars`.

`src/lib/tauri-api.ts`: `WebSocketScopeInput` and `GrpcExecuteInput` gain `processEnv: ProcessEnvScopeInput` (required). `scopeFor(tab)` adds `processEnv: toProcessEnvScopeInput(processEnvScopeOf(tab))`. `GrpcPanel.buildInput` does the same.

Example: a History tab with `historyCollection: 'clone'`, active collection `api`, sends `{"collection":null,"processEnv":{"kind":"collection","name":"clone"}}`. The backend reads `clone`'s gate, so an untrusted `clone` gets nothing.

### 4. Warnings

The five `warnIfProcessEnvWithheld` calls pass `processEnvScopeOf(tab)` instead of `input.collection`. A closed scope shows the existing History message, an untrusted collection the existing collection message, and a scratch tab no warning.

### 5. Previews

`useWebSocketVariableContext(collection, processEnvScope)` and `useGrpcVariableContext(source, processEnvScope)` gain a `ProcessEnvScope` parameter and call `useProcessEnvVars(typeof scope === 'string' ? scope : null, scope !== CLOSED_PROCESS_ENV)`, using an empty map for a closed scope, as `RequestPanel.tsx:571-578` does. `WebSocketPanel.tsx:66` and `GrpcPanel.tsx:60` pass `processEnvScopeOf(tab)`. The collection variables still come from `source`.

### 6. Docs

- `docs/features/collection-trust.md:25`: the backend protocols follow the process env scope the frontend sends (`processEnv`), with the table above in words.
- `collection-trust.md:57`: remove the sentence starting "Known gap: a History tab switched to a WebSocket".
- `crates/rocket-app/CLAUDE.md`: one line on `process_env_for_scope` and the required `processEnv` field.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Input without `processEnv` | Deserialize error, the connect, send, subscribe or call fails with the serde message |
| `processEnv.kind` unknown | Deserialize error |
| Closed scope with `{{process.env.NAME}}` | Placeholder sent unresolved, History warning shown |
| History scope of an untrusted or unknown collection | Placeholder unresolved, no `.env` read, collection warning |
| History scope of a trusted collection | Resolves like the HTTP resend |
| `collection` set and scope `scratch` | The collection's gate decides, never full access |
| `collection` set and scope `closed` | No host environment |
| Scratch tab | Full access, no warning, as today |
| `Collection` scope with no `environmentName` | Default `.env` files of that collection and the OS environment only |
| WS message without `{{` | No vault fetch, scope still applied (no change) |
| Flow request nodes | Unchanged |

## Testing

Rust, `crates/rocket-app/src/execution_service/websocket_resolution.rs` tests (fixtures: `service(dev_env(), CollectionSettings::default())`, `InMemoryTrustStore::allow_all()` or `new()` with `grant`). Read `PATH` from the OS instead of setting env vars:

- `closed_scope_gives_no_host_environment_even_for_a_trusted_collection`
- `history_collection_scope_follows_that_collections_gate` (untrusted: no `process.env.PATH`; trusted: `process.env.PATH` equals `std::env::var("PATH")`)
- `scratch_scope_without_collection_keeps_host_environment`
- `scratch_scope_cannot_widen_a_denied_collection`
- `collection_scope_for_another_collection_cannot_widen_a_denied_collection`
- `scope_deserializes_scratch_collection_and_closed` (the three JSON forms above)
- `websocket_scope_without_process_env_fails_to_deserialize`
- `resolve_websocket_message_with_closed_scope_keeps_the_placeholder`

Rust, `execution_service.rs` tests (fixtures `FakeDotEnv`, `dot_env_svc`):

- `closed_scope_reads_no_dot_env_file` (the fake records no call)
- `collection_scope_reads_that_collections_environment_dot_env` (environment name taken from the scope, not the argument)

Rust, `graphql_subscription.rs` tests: `subscription_with_closed_scope_keeps_the_process_env_placeholder`.

Rust, `src-tauri/src/commands/grpc.rs` tests: `grpc_dto_without_process_env_fails_to_deserialize` and `grpc_dto_reads_the_process_env_scope`.

Frontend (Vitest):

- `src/lib/__tests__/process-env-gate.test.ts`: `toProcessEnvScopeInput` maps undefined, closed, a collection, and the active collection with and without `activeEnvId`.
- `src/lib/__tests__/websocket-session.test.ts`: `buildConnectInput` and `sendSelectedMessage` send `processEnv` for a saved, a History (with and without `historyCollection`) and a scratch tab; the warning gets the scope.
- `src/lib/__tests__/graphql-subscription-session.test.ts`: `buildSubscribeInput` sends a closed scope for a History tab without collection.
- `src/components/grpc/__tests__/GrpcPanel.test.tsx`: the call input carries `processEnv` for a History tab. `GrpcMethodPicker.test.tsx` fixtures gain the field, because `grpc_list_services` also goes through `resolve_input`.
- Existing Rust fixtures that break: the JSON `input()` helper and `process_env_resolves_in_messages_with_the_permission` in `websocket_resolution.rs` set `processEnv` explicitly (scratch or collection), since `Default` is now `Closed`.
- `src/hooks/__tests__/useGrpcVariableContext.test.tsx` and `src/components/request/websocket/__tests__/WebSocketPanel.test.tsx`: a closed scope makes no process env query and shows no host value.

Commands: `cargo check -j4`, `cargo test -j4 -p rocket-app websocket_resolution`, `cargo test -j4 -p rocket-app graphql_subscription`, `cargo test -j4 -p rocket-app scope`, `cargo test -j4 -p rocket grpc`, `yarn tsc --noEmit`, `yarn check`, `yarn test process-env websocket graphql-subscription Grpc`.

## Files changed

- `crates/rocket-app/src/execution_service/websocket_resolution.rs`, `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/graphql_subscription.rs` (tests), `crates/rocket-app/src/lib.rs`, `crates/rocket-app/CLAUDE.md`
- `src-tauri/src/commands/grpc.rs`
- `src/lib/process-env-gate.ts`, `src/lib/tauri-api.ts`, `src/lib/websocket-session.ts`, `src/lib/graphql-subscription-session.ts`, `src/components/grpc/GrpcPanel.tsx`, `src/components/request/websocket/WebSocketPanel.tsx`, `src/hooks/useWebSocketVariableContext.ts`, `src/hooks/useGrpcVariableContext.ts`
- the test files named above, and every Rust or TS fixture that builds these inputs
- `docs/features/collection-trust.md`

## Constraints

- Read `docs/superpowers/specs/opencollection-spec-reference.md` first.
- `rename_all = "camelCase"` only on the IPC DTOs, never on persistence structs.
- No unwrap calls in production paths; tests use `.expect("...")` and `expect_err`.
- No new Tauri command, so `src-tauri/app-commands.txt` does not change.
- The frontend and backend changes land in the same merge, because the field is required.
- shadcn/ui and narrow Zustand selectors in any touched component.

## Acceptance criteria

- A GraphQL subscription, WS connect, WS send, gRPC unary call or gRPC session from a History tab with no collection sends `{{process.env.NAME}}` unresolved and shows the History warning.
- The same from a History tab of an untrusted collection sends it unresolved, reads no `.env` file and shows the collection warning.
- The same from a History tab of a trusted collection resolves it as the HTTP resend does.
- A saved request and a scratch tab behave as today.
- A `collection` set on the input is never widened by the scope; a closed scope still closes it.
- An input without `processEnv` fails to deserialize.
- The gRPC method list (`grpc_list_services`) uses the same scope as the call.
- WS and gRPC variable previews show no host value for a closed or untrusted scope.
- Flow request nodes are unchanged.
- The known-gap sentence is removed from `collection-trust.md`.
- All commands in Testing pass.

## Out of scope

- The Flow resolver call and `flow_execution_service.rs`.
- History storing host values in clear text when the gate allows.

## Decisions

- Where does the process env scope of a WebSocket, GraphQL subscription or gRPC send come from? Options: the frontend sends the tab's scope from `processEnvScopeOf`, the backend gives host values only to a scope it can verify (no collection gets nothing). Chosen: the frontend sends the tab's scope, so scratch tabs keep full access as for HTTP. Decided by: user
- What does a missing scope field on the IPC input mean? Options: required field (deserialize error), default to scratch (today's behaviour), default to closed. Chosen: required field, so a caller that forgets it fails instead of getting full access. Decided by: user
- What is the wire shape of the scope? Options: one tagged field `processEnv` (`scratch`, `collection` with a name, `closed`), two fields (`processEnvCollection` and `processEnvClosed`), one string with reserved words. Chosen: one tagged field. Decided by: user
- What happens when the input has a `collection` and the scope disagrees? Options: the collection's gate wins and the scope can only narrow it, reject the input, trust the scope alone. Chosen: the collection's gate wins, a `closed` scope still closes it, a `scratch` or other-collection scope never widens it. Decided by: user
- Which environment `.env` does a History tab's collection scope read? Options: match the HTTP resend (the active environment only when the scope collection is the active collection), always the active environment, default `.env` files only. Chosen: match the HTTP resend. Decided by: user
- Who knows whether the scope collection is the active collection? Options: the frontend puts the environment name into the `collection` scope only when it is the active collection, the backend tracks the active collection. Chosen: the frontend, because the backend has no active-collection state. Decided by: call
- How far beyond the issue does this go? Options: sends, warnings and variable previews; sends and warnings; sends only. Chosen: sends, warnings and the WS and gRPC variable previews. Decided by: user
- How is the work split into factory plans? Options: one plan with three tasks, two plans (sends then warnings, previews and docs), two plans with a temporary default. Chosen: one plan with three tasks, so main is never broken by a required field without its senders. Decided by: user
- Is the Flow resolver changed? Options: a new sibling `build_variable_context_for_scope` and Flow untouched, add a scope parameter to the existing resolver. Chosen: a new sibling, so Flow keeps its signature. Decided by: call
- What does Rust's `Default` give for the scope? Options: `Closed`, `Scratch`, no `Default`. Chosen: `Closed`, used only by Rust code and tests, never by serde. Decided by: call
