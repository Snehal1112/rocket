# Realtime process env scope Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** WebSocket, GraphQL subscription and gRPC sends follow the tab's process env scope, so a History tab with no collection or an untrusted one gets no host environment.

**Architecture:** A required tagged IPC field `processEnv` (`scratch`, `collection`, `closed`) is added to `WebSocketScope` and `GrpcExecuteDto`. `RequestExecutionService::process_env_for_scope` applies it on top of the collection trust gate. The frontend fills it from `processEnvScopeOf(tab)`, and the warnings and variable previews use the same scope.

**Tech Stack:** Rust (rocket-app, src-tauri, serde), React + TypeScript, Vitest, TanStack Query, Zustand.

**Spec:** `docs/superpowers/specs/2026-10-30-realtime-process-env-scope-design.md`

## Global Constraints

- `rename_all = "camelCase"` only on IPC DTOs, never on persistence structs.
- The `processEnv` field is required: no `#[serde(default)]` on it, and required in the TS types.
- Rust `Default` for `ProcessEnvScopeInput` is `Closed`.
- A `collection` set on the input always keeps its own gate; the scope can only narrow it (a `Closed` scope closes it).
- `build_variable_context_with_process_env` and the Flow caller (`crates/rocket-app/src/flow_execution_service.rs:2061`) are not changed in behaviour or signature.
- No unwrap calls in production paths; tests use `.expect("...")` and `expect_err`.
- Tests read `PATH` from the OS (`std::env::var("PATH")`) instead of setting env vars.
- No new Tauri command; `src-tauri/app-commands.txt` does not change.
- cargo commands are per crate: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`, never a bare workspace `cargo check -j4` or `cargo test -j4`.
- shadcn/ui primitives and narrow Zustand selectors in any touched component.

## Review Focus

1. A History tab of collection `clone` while the active collection is `api` with environment `dev`: the scope must carry no `environmentName`, so `api`'s `dev` `.env` is never read for `clone`. Pinned by `toProcessEnvScopeInput omits the environment for a non-active collection` (Task 2).
2. A scratch tab whose input has `collection: None` and scope `scratch`: full host access stays, as today. A regression here breaks every scratch WS tab. Pinned by `scratch_scope_without_collection_keeps_host_environment` (Task 1).
3. A saved request (`collection: "api"`, denied) whose scope says `scratch`: the backend must not widen it. Pinned by `scratch_scope_cannot_widen_a_denied_collection` (Task 1).
4. The gRPC method list opened from a History tab: `grpc_list_services` goes through the same `resolve_input`, so its input must carry the same scope as the call. Pinned by `GrpcPanel` test `lists services with the tab's process env scope` (Task 2).
5. A WS message sent after the tab was reopened from History: `ws_send` resolves through `resolve_websocket_message`, which must use the scope too, not only the connect. Pinned by `resolve_websocket_message_with_closed_scope_keeps_the_placeholder` (Task 1) and `sendSelectedMessage sends the History scope` (Task 2).

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-app/src/execution_service/websocket_resolution.rs` | `ProcessEnvScopeInput`, the `process_env` field on `WebSocketScope`, the WS call sites, WS tests |
| `crates/rocket-app/src/execution_service.rs` | `process_env_for_scope`, `build_variable_context_for_scope`, `.env` tests |
| `crates/rocket-app/src/graphql_subscription.rs` | test fixture and one closed-scope test (code unchanged, it uses `resolve_websocket`) |
| `crates/rocket-app/src/lib.rs` | re-export `ProcessEnvScopeInput` |
| `crates/rocket-app/CLAUDE.md` | one note line |
| `src-tauri/src/commands/grpc.rs` | `process_env` on `GrpcExecuteDto`, `resolve_input` uses the scope |
| `src/lib/process-env-gate.ts` | `ProcessEnvScopeInput` TS type and `toProcessEnvScopeInput` |
| `src/lib/tauri-api.ts` | `processEnv` on `WebSocketScopeInput` and `GrpcExecuteInput` |
| `src/lib/websocket-session.ts`, `src/lib/graphql-subscription-session.ts`, `src/components/grpc/GrpcPanel.tsx` | send the scope, warn with the scope |
| `src/hooks/useWebSocketVariableContext.ts`, `src/hooks/useGrpcVariableContext.ts`, `src/components/request/websocket/WebSocketPanel.tsx` | previews follow the scope |
| `docs/features/collection-trust.md` | doc update, remove the known gap |

---

### Task 1: Backend scope type and resolution

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service/websocket_resolution.rs` (struct at lines 17-30, calls at 150 and 216, tests from 250)
- Modify: `crates/rocket-app/src/execution_service.rs` (next to `process_env_for` at 1200 and `build_variable_context_with_process_env` at 1314; tests near `dot_env_svc` at 7863)
- Modify: `crates/rocket-app/src/graphql_subscription.rs` (tests module `resolve_tests`, `input()` at about 703)
- Modify: `crates/rocket-app/src/lib.rs:117-119`
- Modify: `crates/rocket-app/CLAUDE.md` (the backend process env note at line 74)

**Interfaces:**
- Consumes: `RequestExecutionService::process_env_for(collection: Option<&str>, environment_name: Option<&str>, global_env_name: Option<&str>) -> ProcessEnv`, `build_variable_scopes`, `ProcessEnv::default()`.
- Produces:
  - `pub enum ProcessEnvScopeInput { Scratch, Collection { name: String, environment_name: Option<String> }, Closed }`, derives `Debug, Clone, Default, PartialEq, Eq, Deserialize`, `#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]`, `#[default] Closed`, `#[serde(default)]` on `environment_name` only. Re-exported as `rocket_app::ProcessEnvScopeInput`.
  - `WebSocketScope.process_env: ProcessEnvScopeInput` (no serde default).
  - `pub fn process_env_for_scope(&self, collection: Option<&str>, environment_name: Option<&str>, global_env_name: Option<&str>, scope: &ProcessEnvScopeInput) -> ProcessEnv`
  - `pub fn build_variable_context_for_scope(&self, global_env_name: Option<&str>, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>, external_secrets: &std::collections::HashMap<String, String>, scope: &ProcessEnvScopeInput) -> std::collections::HashMap<String, String>`

- [ ] **Step 1: Read the reference docs and the code this task touches**

Read the spec sections Design 1 and 2, `docs/features/collection-trust.md`, `execution_service.rs:1170-1335`, all of `websocket_resolution.rs`, and `graphql_subscription.rs:160-200` and its `resolve_tests` module.

- [ ] **Step 2: Write the failing tests in `websocket_resolution.rs`**

First update the existing fixtures so they keep their meaning: `input(url)` adds `"processEnv": { "kind": "collection", "name": "api" }` to its JSON; `inputs_deserialize_from_the_camel_case_ipc_shape` adds `"processEnv": { "kind": "scratch" }` to both JSON objects; struct literals of `WebSocketScope` in `process_env_stays_unresolved_for_a_collection_without_the_permission` and `process_env_resolves_in_messages_with_the_permission` add `process_env: ProcessEnvScopeInput::Collection { name: "api".into(), environment_name: None }`.

Full example of the new tests:

```rust
fn path_ctx(svc: &RequestExecutionService, collection: Option<&str>, scope: ProcessEnvScopeInput) -> std::collections::HashMap<String, String> {
    svc.build_variable_context_for_scope(None, collection, None, None, &std::collections::HashMap::new(), &scope)
}

#[test]
fn closed_scope_gives_no_host_environment_even_for_a_trusted_collection() {
    let svc = service(dev_env(), CollectionSettings::default())
        .with_trust_store(crate::test_doubles::InMemoryTrustStore::allow_all());
    assert!(!path_ctx(&svc, Some("api"), ProcessEnvScopeInput::Closed).contains_key("process.env.PATH"));
    assert!(!path_ctx(&svc, None, ProcessEnvScopeInput::Closed).contains_key("process.env.PATH"));
}
```

| Test | Service trust | `collection` | Scope | Assert |
|---|---|---|---|---|
| `history_collection_scope_follows_that_collections_gate` | `new()` (deny), then `allow_all()` | `None` | `Collection { name: "api", environment_name: None }` | deny: no `process.env.PATH`; allow: `process.env.PATH` equals `std::env::var("PATH").ok()` |
| `scratch_scope_without_collection_keeps_host_environment` | default (deny) | `None` | `Scratch` | `process.env.PATH` equals `std::env::var("PATH").ok()` |
| `scratch_scope_cannot_widen_a_denied_collection` | default (deny) | `Some("api")` | `Scratch` | no `process.env.PATH` |
| `collection_scope_for_another_collection_cannot_widen_a_denied_collection` | default (deny) | `Some("api")` | `Collection { name: "other", environment_name: None }` | no `process.env.PATH` |

Deserialize tests:

```rust
#[test]
fn scope_deserializes_scratch_collection_and_closed() {
    let s: ProcessEnvScopeInput = serde_json::from_value(serde_json::json!({ "kind": "scratch" })).expect("scratch");
    assert_eq!(s, ProcessEnvScopeInput::Scratch);
    let c: ProcessEnvScopeInput = serde_json::from_value(serde_json::json!({ "kind": "collection", "name": "api", "environmentName": "dev" })).expect("collection");
    assert_eq!(c, ProcessEnvScopeInput::Collection { name: "api".into(), environment_name: Some("dev".into()) });
    let n: ProcessEnvScopeInput = serde_json::from_value(serde_json::json!({ "kind": "collection", "name": "api" })).expect("no env");
    assert_eq!(n, ProcessEnvScopeInput::Collection { name: "api".into(), environment_name: None });
    let x: ProcessEnvScopeInput = serde_json::from_value(serde_json::json!({ "kind": "closed" })).expect("closed");
    assert_eq!(x, ProcessEnvScopeInput::Closed);
    assert_eq!(ProcessEnvScopeInput::default(), ProcessEnvScopeInput::Closed);
    serde_json::from_value::<ProcessEnvScopeInput>(serde_json::json!({ "kind": "open" })).expect_err("unknown kind");
}

#[test]
fn websocket_scope_without_process_env_fails_to_deserialize() {
    let err = serde_json::from_value::<WebSocketConnectInput>(serde_json::json!({ "url": "wss://h/ws", "collection": "api" })).expect_err("missing processEnv");
    assert!(err.to_string().contains("processEnv"), "{err}");
    serde_json::from_value::<WebSocketSendInput>(serde_json::json!({ "kind": "text", "data": "x" })).expect_err("missing processEnv");
}
```

`resolve_websocket_message_with_closed_scope_keeps_the_placeholder` (`#[tokio::test]`): service with `allow_all()`, `WebSocketScope { collection: None, process_env: ProcessEnvScopeInput::Closed, ..WebSocketScope::default() }`, message `"{{process.env.PATH}}"` of kind `Text`; assert the frame equals `WebSocketFrame::Text("{{process.env.PATH}}".into())`.

- [ ] **Step 3: Write the failing tests in `execution_service.rs` and `graphql_subscription.rs`**

In `execution_service.rs` tests, build the service as `build_variable_context_with_process_env_exposes_dot_env_values_as_process_env_keys` does (`FakeDotEnv::new(&[(".env", &[("ROCKET_DOT_A", "1")])])`, `dot_env_svc(Box::new(MockEnvRepo::empty()), Some(env_with_dot_env("dev", ".env")), Some(&fake), true)`):

- `closed_scope_reads_no_dot_env_file`: `svc.process_env_for_scope(Some("api"), Some("dev"), None, &ProcessEnvScopeInput::Closed)`; assert `pe.vars.is_empty()`, `!pe.allowed`, and the fake recorded no call (`fake.calls` and `fake.default_calls` locks are empty).
- `collection_scope_reads_that_collections_environment_dot_env`: `svc.process_env_for_scope(None, None, None, &ProcessEnvScopeInput::Collection { name: "api".into(), environment_name: Some("dev".into()) })`; assert `pe.vars.get("ROCKET_DOT_A").map(String::as_str) == Some("1")` (the environment came from the scope, the argument was `None`).

In `graphql_subscription.rs` `resolve_tests`: the `input()` base JSON adds `"processEnv": { "kind": "collection", "name": "api" }`; `the_ipc_input_deserializes_from_camel_case` adds a `processEnv` key to its JSON. New test:

```rust
#[tokio::test]
async fn subscription_with_closed_scope_keeps_the_process_env_placeholder() {
    let svc = service().with_trust_store(crate::test_doubles::InMemoryTrustStore::allow_all());
    let start = resolve_graphql_subscription(
        &svc,
        input(serde_json::json!({
            "collection": null,
            "processEnv": { "kind": "closed" },
            "headers": [{ "key": "X-Path", "value": "{{process.env.PATH}}", "enabled": true }]
        })),
    )
    .await
    .expect("resolve");
    assert!(start.connect.headers.iter().any(|(k, v)| k == "X-Path" && v == "{{process.env.PATH}}"));
}
```

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -j4 -p rocket-app scope`
Expected: compile errors such as `cannot find type 'ProcessEnvScopeInput' in this scope` and `no method named 'build_variable_context_for_scope'`.

- [ ] **Step 5: Implement**

- Add `ProcessEnvScopeInput` (signature in Interfaces) above `WebSocketScope`, and `pub process_env: ProcessEnvScopeInput` on `WebSocketScope` with a doc comment that it is required and never widens `collection`.
- `process_env_for_scope` in `execution_service.rs`, next to `process_env_for`, implementing the table in spec Design 2: `Closed` returns `ProcessEnv::default()`; `Some(c)` calls `process_env_for(Some(c), environment_name, global_env_name)`; `None` + `Collection { name, environment_name: e }` calls `process_env_for(Some(name), e.as_deref(), global_env_name)`; `None` + `Scratch` calls `process_env_for(None, environment_name, global_env_name)`. Import the type from `websocket_resolution`.
- `build_variable_context_for_scope`: same body as `build_variable_context_with_process_env` with `process_env_for_scope` in place of `process_env_for`. Remove the sentence naming WebSocket, GraphQL subscriptions and gRPC from the old function's doc comment.
- `resolve_websocket` (line 150) and `resolve_websocket_message` (line 216) call `build_variable_context_for_scope(..., &scope.process_env)`.
- Re-export `ProcessEnvScopeInput` in `lib.rs:117-119` with `WebSocketScope`.
- `crates/rocket-app/CLAUDE.md`: add one line after the backend process env note: WS, GraphQL subscriptions and gRPC resolve process env through `process_env_for_scope` with the required `processEnv` IPC field; `collection` keeps its gate and a closed scope closes it.

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -j4 -p rocket-app scope`, then `cargo test -j4 -p rocket-app websocket_resolution`, then `cargo test -j4 -p rocket-app graphql_subscription`, then `cargo test -j4 -p rocket-app dot_env`.
Expected: all pass, the 11 new tests included, no existing test fails. Then `cargo check -j4 -p rocket-app`: no errors. (`src-tauri` will not compile until Task 2 adds the gRPC field; that is expected.)

### Task 2: gRPC DTO and frontend senders

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/grpc.rs` (DTO at 13-29, `resolve_input` at 48-64, tests at 130)
- Modify: `src/lib/process-env-gate.ts`
- Modify: `src/lib/tauri-api.ts` (`WebSocketScopeInput` at 3629, `GrpcExecuteInput`)
- Modify: `src/lib/websocket-session.ts` (`scopeFor` at 25, warnings at 63 and 78)
- Modify: `src/lib/graphql-subscription-session.ts` (warning at 45)
- Modify: `src/components/grpc/GrpcPanel.tsx` (`buildInput` at 75, warnings at 107 and 122)
- Test: `src/lib/__tests__/process-env-gate.test.ts`, `src/lib/__tests__/websocket-session.test.ts`, `src/lib/__tests__/graphql-subscription-session.test.ts`, `src/components/grpc/__tests__/GrpcPanel.test.tsx`, `src/components/grpc/__tests__/GrpcMethodPicker.test.tsx` (fixture only)

**Interfaces:**
- Consumes: `rocket_app::ProcessEnvScopeInput` and `RequestExecutionService::build_variable_context_for_scope` from Task 1 (signature in Task 1 Interfaces); `processEnvScopeOf(tab: Tab | undefined, tabId?: string): ProcessEnvScope` from `src/lib/execute-request.ts:110`; `CLOSED_PROCESS_ENV`, `ProcessEnvScope`, `warnIfProcessEnvWithheld(scope: ProcessEnvScope | null, parts: unknown[], requestName?: string)` from `src/lib/process-env-gate.ts`.
- Produces:
  - `GrpcExecuteDto.process_env: ProcessEnvScopeInput` (no serde default).
  - TS `export type ProcessEnvScopeInput = { kind: 'scratch' } | { kind: 'collection'; name: string; environmentName?: string } | { kind: 'closed' };` in `process-env-gate.ts`.
  - TS `export function toProcessEnvScopeInput(scope: ProcessEnvScope): ProcessEnvScopeInput` in `process-env-gate.ts`.
  - `WebSocketScopeInput.processEnv: ProcessEnvScopeInput` and `GrpcExecuteInput.processEnv: ProcessEnvScopeInput` (required) in `tauri-api.ts`.

- [ ] **Step 1: Read the reference docs and the code this task touches**

Read spec Design 3 and 4, `src-tauri/src/commands/grpc.rs`, `src/lib/process-env-gate.ts`, `src/lib/execute-request.ts:100-140`, `src/lib/websocket-session.ts`, `src/lib/graphql-subscription-session.ts`, `src/components/grpc/GrpcPanel.tsx`, `src/components/grpc/GrpcMethodPicker.tsx` and the five test files above.

- [ ] **Step 2: Write the failing Rust tests in `grpc.rs`**

`dto_json` adds `, "processEnv": {"kind": "scratch"}` before `{extra}`, so the existing tests keep passing. New tests:

```rust
#[test]
fn grpc_dto_without_process_env_fails_to_deserialize() {
    let json = r#"{"collection": "api", "request": {"name": "Say", "url": "localhost:50051", "methodType": "unary"}}"#;
    let err = serde_json::from_str::<GrpcExecuteDto>(json).expect_err("missing processEnv");
    assert!(err.to_string().contains("processEnv"), "{err}");
}

#[test]
fn grpc_dto_reads_the_process_env_scope() {
    let json = r#"{"collection": null, "request": {"name": "Say", "url": "localhost:50051", "methodType": "unary"}, "processEnv": {"kind": "closed"}}"#;
    let dto: GrpcExecuteDto = serde_json::from_str(json).expect("dto");
    assert_eq!(dto.process_env, rocket_app::ProcessEnvScopeInput::Closed);
}
```

- [ ] **Step 3: Write the failing frontend tests**

`process-env-gate.test.ts`, new `describe('toProcessEnvScopeInput')` (set `useEnvStore.setState({ activeCollection, activeEnvId })` per case):

```ts
it('maps a scratch, a closed and a collection scope', () => {
  useEnvStore.setState({ activeCollection: 'api', activeEnvId: 'dev' });
  expect(toProcessEnvScopeInput(undefined)).toEqual({ kind: 'scratch' });
  expect(toProcessEnvScopeInput(CLOSED_PROCESS_ENV)).toEqual({ kind: 'closed' });
  expect(toProcessEnvScopeInput('api')).toEqual({ kind: 'collection', name: 'api', environmentName: 'dev' });
});
```

| Test | Store | Input | Expected |
|---|---|---|---|
| `toProcessEnvScopeInput omits the environment for a non-active collection` | `activeCollection: 'api', activeEnvId: 'dev'` | `'clone'` | `{ kind: 'collection', name: 'clone' }` |
| `toProcessEnvScopeInput omits the environment when none is active` | `activeCollection: 'api', activeEnvId: null` | `'api'` | `{ kind: 'collection', name: 'api' }` |

`websocket-session.test.ts` (reuse `wsTab()`; make History tabs as `{ ...wsTab(), id: 'history-h1', tabType: 'history', source: undefined, historyCollection: null }`):

| Test | Tab | Assert |
|---|---|---|
| `buildConnectInput sends the saved collection scope` | `wsTab()` with `useEnvStore` active collection `other` | `input.processEnv` equals `{ kind: 'collection', name: 'my-api' }` |
| `buildConnectInput sends a closed scope for a History tab without collection` | History, `historyCollection: null` | `input.processEnv` equals `{ kind: 'closed' }` and `input.collection` is `undefined` |
| `buildConnectInput sends the History collection scope` | History, `historyCollection: 'clone'` | `input.processEnv` equals `{ kind: 'collection', name: 'clone' }`, `input.collection` is `undefined` |
| `buildConnectInput sends a scratch scope for a scratch tab` | `wsTab()` with `source: undefined` | `input.processEnv` equals `{ kind: 'scratch' }` |
| `sendSelectedMessage sends the History scope` | History `historyCollection: null`, open session in `useWebSocketStore` as the existing `sendSelectedMessage` test sets it | `wsSend` input `processEnv` equals `{ kind: 'closed' }` |
| `connectTab warns with the History scope` | History `historyCollection: null`, header value `{{process.env.HOME}}`; `vi.mock('@/lib/process-env-gate', ...)` spreading the actual module with `warnIfProcessEnvWithheld: vi.fn()` | `warnIfProcessEnvWithheld` called with `CLOSED_PROCESS_ENV` as first argument |

`graphql-subscription-session.test.ts`: `buildSubscribeInput sends a closed scope for a History tab without collection`: `{ ...gqlTab(), id: 'history-h1', tabType: 'history', source: undefined, historyCollection: null }`; assert `processEnv` equals `{ kind: 'closed' }`.

`GrpcPanel.test.tsx`:
- `sends the process env scope of a History tab`: mount `{ ...grpcTab('unary'), id: 'history-h1', tabType: 'history', source: undefined, historyCollection: null }`, start the call as `sends the editor state and shows the reply and status` does; assert `vi.mocked(tauriApi.grpcUnaryCall).mock.calls[0][0].processEnv` equals `{ kind: 'closed' }`.
- `lists services with the tab's process env scope`: same tab; open the method list with `await userEvent.click(screen.getByLabelText('Method'))` (as `GrpcMethodPicker.test.tsx` `loads the methods the first time the list is opened` does); assert `await waitFor(() => expect(vi.mocked(tauriApi.grpcListServices).mock.calls[0][0].processEnv).toEqual({ kind: 'closed' }))`.

`GrpcMethodPicker.test.tsx`: its `buildInput` fixture adds `processEnv: { kind: 'scratch' }` so it type-checks.

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -j4 -p rocket grpc`
Expected: compile error `no field 'process_env' on type 'GrpcExecuteDto'`.
Run: `yarn test process-env-gate websocket-session graphql-subscription-session GrpcPanel`
Expected: failures such as `toProcessEnvScopeInput is not a function` and `expected undefined to deeply equal { kind: 'closed' }`.

- [ ] **Step 5: Implement**

- `grpc.rs`: add `pub process_env: rocket_app::ProcessEnvScopeInput` to `GrpcExecuteDto` with a doc line; `resolve_input` calls `exec.build_variable_context_for_scope(..., &dto.process_env)`.
- `process-env-gate.ts`: the type and `toProcessEnvScopeInput` from Interfaces. A collection name adds `environmentName` only when `name === useEnvStore.getState().activeCollection` and `activeEnvId` is set (the rule of `getProcessEnvVars`, `execute-request.ts:122-125`).
- `tauri-api.ts`: required `processEnv: ProcessEnvScopeInput` on `WebSocketScopeInput` and `GrpcExecuteInput` (import the type from `@/lib/process-env-gate`).
- `websocket-session.ts`: `scopeFor(tab)` adds `processEnv: toProcessEnvScopeInput(processEnvScopeOf(tab))`; the warnings at 63 and 78 pass `processEnvScopeOf(tab)` instead of `input.collection`.
- `graphql-subscription-session.ts:45`: pass `processEnvScopeOf(tab)`.
- `GrpcPanel.tsx`: `buildInput` adds `processEnv: toProcessEnvScopeInput(processEnvScopeOf(tab))`; the warnings at 107 and 122 pass `processEnvScopeOf(tab)`.
- Fix every other TS builder of these inputs that `yarn tsc --noEmit` reports.

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -j4 -p rocket grpc`
Expected: all pass, 2 new tests included.
Run: `yarn test process-env-gate websocket-session graphql-subscription-session GrpcPanel GrpcMethodPicker`
Expected: all pass, 12 new tests included.
Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

### Task 3: Previews follow the scope, docs

**Files:**
- Modify: `src/hooks/useWebSocketVariableContext.ts:17-25`
- Modify: `src/hooks/useGrpcVariableContext.ts:36-46`
- Modify: `src/components/request/websocket/WebSocketPanel.tsx:66`
- Modify: `src/components/grpc/GrpcPanel.tsx:60`
- Modify: `docs/features/collection-trust.md:25,57`
- Test: `src/hooks/__tests__/useGrpcVariableContext.test.tsx`, create `src/hooks/__tests__/useWebSocketVariableContext.test.tsx`, `src/components/request/websocket/__tests__/WebSocketPanel.test.tsx`

**Interfaces:**
- Consumes: `processEnvScopeOf` and `CLOSED_PROCESS_ENV`, `ProcessEnvScope` (as in Task 2); `useProcessEnvVars(collection: string | null | undefined, enabled?: boolean)` from `src/lib/queries/environment-queries.ts:79`.
- Produces:
  - `useWebSocketVariableContext(collection: string | undefined, processEnvScope: ProcessEnvScope): Map<string, VariableScopeEntry>`
  - `useGrpcVariableContext(source: { collection: string; path: string } | undefined, processEnvScope: ProcessEnvScope): GrpcVariableScope`

- [ ] **Step 1: Read the reference docs and the code this task touches**

Read spec Design 5 and 6, both hooks, `RequestPanel.tsx:565-580`, `WebSocketPanel.tsx:55-70`, `docs/features/collection-trust.md` and the three test files.

- [ ] **Step 2: Write the failing tests**

`useGrpcVariableContext.test.tsx`: change the `useProcessEnvVars` mock to a `vi.fn((collection, enabled = true) => ({ data: enabled ? { HOME: '/home/me' } : {} }))` exported through the mocked module so tests can read its calls. Update the existing `includes_inherited_variables` call to pass `'api'` as the second argument.

```ts
it('closed_scope_queries_no_process_env', () => {
  const { result } = renderHook(() => useGrpcVariableContext(undefined, CLOSED_PROCESS_ENV));
  expect(processEnvMock).toHaveBeenCalledWith(null, false);
  expect(result.current.variableContext.has('process.env.HOME')).toBe(false);
});
```

| Test | Args | Assert |
|---|---|---|
| `history_collection_scope_queries_that_collection` | `(undefined, 'clone')` | `processEnvMock` called with `('clone', true)` |
| `scratch_scope_queries_the_full_map` | `(undefined, undefined)` | `processEnvMock` called with `(null, true)` and `variableContext.has('process.env.HOME')` is `true` |

Create `useWebSocketVariableContext.test.tsx` with the same `environment-queries` mock shape (plus `useGlobalEnvironment: () => ({ data: null })`) and `vi.mock('@/lib/tauri-api', ...)` returning `getCollectionSettings: vi.fn().mockResolvedValue({ variables: [] })`:

| Test | Args | Assert |
|---|---|---|
| `closed scope queries no process env` | `(undefined, CLOSED_PROCESS_ENV)` | mock called with `(null, false)`; result has no `process.env.HOME` key |
| `history collection scope queries that collection` | `(undefined, 'clone')` | mock called with `('clone', true)` |
| `saved request queries its collection` | `('api', 'api')` | mock called with `('api', true)` |

`WebSocketPanel.test.tsx`: turn the `useWebSocketVariableContext` mock into a `vi.fn(() => undefined)`; new test `passes the History scope to the variable preview`: render the panel with a tab `{ id: 'history-h1', tabType: 'history', source: undefined, historyCollection: null, ... }` built like the file's existing tab; assert the mock was called with `(undefined, CLOSED_PROCESS_ENV)`.

- [ ] **Step 3: Run the tests to see them fail**

Run: `yarn test useGrpcVariableContext useWebSocketVariableContext WebSocketPanel`
Expected: failures such as `expected "spy" to be called with arguments: [ null, false ]`.

- [ ] **Step 4: Implement**

- Both hooks take the new `processEnvScope` parameter and call `useProcessEnvVars(typeof processEnvScope === 'string' ? processEnvScope : null, processEnvScope !== CLOSED_PROCESS_ENV)`, using an empty map for a closed scope, as `RequestPanel.tsx:571-578` does. Collection variables still come from `collection` / `source`.
- `WebSocketPanel.tsx:66` passes `processEnvScopeOf(tab)`; `GrpcPanel.tsx:60` passes `processEnvScopeOf(tab)`.
- `collection-trust.md:25`: the backend protocols get `process.env.*` from the `processEnv` scope the frontend sends (scratch, collection, closed); a request's `collection` keeps its own gate and a closed scope closes it. Line 57: delete the sentence starting "Known gap: a History tab switched to a WebSocket".

- [ ] **Step 5: Run the tests to see them pass**

Run: `yarn test useGrpcVariableContext useWebSocketVariableContext WebSocketPanel GrpcPanel`
Expected: all pass, 7 new tests included.
Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.
Run: `cargo check -j4 -p rocket`
Expected: no errors.

Next plan: none
