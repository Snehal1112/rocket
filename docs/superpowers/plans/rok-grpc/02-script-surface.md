# rok.grpc Plan 02: Script Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rok.grpc.run(path, options)` and `rok.grpc.call(options)` work from a Developer-mode script and return the result object of the spec, for all four method types.

**Architecture:** A new `HostCall::Grpc` carries a `HostGrpcRequest` from a new op to `ExecutionScriptHost::grpc`, which resolves the saved path or the ad hoc fields, checks the request guard, and calls `GrpcService::exchange` (plan 01). The Developer-mode check lives in the op.

**Tech Stack:** Rust, deno_core ops, `bootstrap.js`, tonic transport already present.

**Spec:** `docs/superpowers/specs/2026-11-12-rok-grpc-design.md`. Plan 01 must be merged first.

## Global Constraints

- Both forms return a Promise and also accept a Node-style callback `(err, result)` as the last argument.
- Result members (exact): `status` as `{ code, name, message }`, `message`, `messages`, `headers`, `trailers`, `truncated`. `headers` and `trailers` are plain objects with lowercase keys and string values. `message` is `undefined` when no message arrived.
- `messages` is an array of JSON values; `message` (one value) is a shortcut for a one-item array; giving both rejects with a `TypeError`.
- Rejections use the prefix `rok.grpc.run:` or `rok.grpc.call:`. A non-OK gRPC status resolves.
- Safe mode: refused with a message that contains `Developer mode`.
- Metadata keys are lowercased; a value must be a string.
- `timeoutMs` must be a finite number greater than 0 when given.
- Ad hoc fields: `url`, `service`, `method`, `protoFile` are non-empty strings; `tls` is a boolean (default false). `grpcs://` is used when `tls` is true and the URL has no scheme.
- Plan-index rulings 1 to 6 apply.

## Review Focus

- `rok.grpc.call` with `messages` that is not an array, or an element that is not an object: `TypeError`, nothing sent.
- A script that never awaits the call and ends: the run does not hang (same behaviour as `rok.sendRequest`).
- `rok.grpc.run` on an HTTP request path, a WebSocket path, a missing path, and a path with `..`: each rejects with a clear message and sends nothing.
- A host blocked by the request guard: rejects before any connect.
- A secret variable used in a saved gRPC request: its value never appears in the console line or the rejection text.

---

### Task 1: Op, bridge and JS functions

**Files:**
- Modify: `crates/rocket-infra/src/scripting/host_bridge.rs` (`HostCall::Grpc`, `serve_host_call`), `crates/rocket-infra/src/scripting/ops/host.rs` (new op `op_rok_grpc`), `crates/rocket-infra/src/scripting/ops/mod.rs` (register), `crates/rocket-infra/src/scripting/bootstrap.js`
- Test: `crates/rocket-infra/src/scripting/` existing host-bridge test module (read how `op_rok_send_request` is tested with a fake `ScriptHost`).

**Interfaces:**
- Consumes: `HostGrpcRequest`, `HostGrpcResponse`, `ScriptHost::grpc` (plan 01).
- Produces: `op_rok_grpc(state, #[string] request_json) -> Result<String, ScriptHostError>` taking a `HostGrpcRequest` as JSON and returning a `HostGrpcResponse` as JSON. `rok.grpc.run` and `rok.grpc.call` in `bootstrap.js`.

- [ ] **Step 1: Write failing tests.** Rust: `grpc_op_refuses_in_safe_mode_with_a_developer_mode_message` (state `sandbox_mode` Safe, fake host never called), `grpc_op_forwards_the_request_to_the_host_and_returns_its_response`, `grpc_op_without_a_host_reports_unavailable`, `grpc_op_writes_a_console_line_without_message_content` (line holds target and status name only). Script-level, with the fake host (follow the existing `rok.sendRequest` script tests): `run_sends_a_saved_target_with_messages_and_metadata`, `call_sends_an_ad_hoc_target`, `message_shortcut_becomes_a_one_item_messages_array`, `message_and_messages_together_reject`, `metadata_keys_are_lowercased`, `timeout_ms_zero_or_nan_rejects`, `result_headers_are_plain_objects`, `callback_form_receives_error_and_result`, `non_object_message_element_rejects`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-infra grpc_op` and the script-level names. Expect failures.
- [ ] **Step 3: Implement.** In `bootstrap.js` add `rok.grpc = { run, call }` beside `sendRequest`. A shared `_grpc(api, target, options)` validates arguments (the Global Constraints above), builds the JSON `{ target: { kind: 'saved', path } | { kind: 'adHoc', ... }, messages: [...JSON texts], metadata: [[k, v]...], timeoutMs }` (note: `messages` entries are `JSON.stringify` of each object), calls the op, and maps the answer to the result object. The op checks `sandbox_mode == Safe` first and returns a `ScriptHostError` containing `Developer mode`. `runtimeVars` is filled by the engine the same way `op_rok_run_request` fills them (read that op and reuse its helper).
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-infra scripting`. Expect pass.
- [ ] **Step 5: Stage** explicit paths. Do not commit.

---

### Task 2: ExecutionScriptHost::grpc

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (field `grpc: Option<Arc<GrpcService>>` on `RequestExecutionService` and `with_grpc_service(self, Arc<GrpcService>) -> Self`), `crates/rocket-app/src/execution_service/script_host.rs` (`impl ScriptHost::grpc`), `crates/rocket-app/src/execution_service/run_request.rs` (a `find_grpc_target(collection: &Collection, path: &str) -> GrpcTarget` helper, where `GrpcTarget` is `Found(Box<GrpcRequest>)`, `WrongType` or `NotFound`; the HTTP-only `find_run_target` stays unchanged)
- Test: `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Consumes: `GrpcService::exchange`, `GrpcExchangeInput`, `GrpcExchangeOutput` (plan 01); `HostGrpcRequest`, `HostGrpcResponse` (plan 01); `send_variable_map`, `check_request_guard`, `ActiveRun` (existing).
- Produces: `ExecutionScriptHost::grpc` in the `ScriptHost` impl.

Behaviour:
- No `GrpcService` configured: `HostError::Failed("rok.grpc: gRPC is not available here")`.
- Saved target: normalise the path as `rok.runRequest` does (`normalize_run_path`). No run-chain check is needed, because a gRPC request runs no scripts here. Load the collection tree, `find_grpc_target`; `WrongType` fails with "is not a gRPC request", `NotFound` with "invalid request path - <path>".
- Ad hoc target: build a `GrpcRequest` in memory with `url` (scheme from `tls`), `method` as `service/method`, `proto_file_path`, no auth, no metadata of its own.
- Variables: `svc.send_variable_map(self.input, &self.external_secrets, true)` extended with `request.runtime_vars` (runtime wins), passed as `GrpcExecuteInput.variables`; the request's own variables are applied by the existing `with_request_variables`. Script `metadata` pairs are added as extra metadata entries on the request, overriding saved entries with the same name.
- Guard: `svc.check_request_guard(&self.calling_url, &resolved_url, &self.input.request_guard_policy)` after variable resolution of the URL, before `exchange`. The resolved URL comes from a small helper that applies the same resolver as `prepare_call` (make `GrpcService::resolved_url(&GrpcExecuteInput) -> DomainResult<String>` public for this; do not duplicate the resolver).
- Take an `ActiveRun` slot (same limit and error style as `run_request`) for the duration of the call.
- Always `confine_proto: true`.
- Map `GrpcExchangeOutput` to `HostGrpcResponse` (status name from `code_name`).
- Errors: `HostError::Failed(format!("rok.grpc: {e}"))`. Secret values never appear: the existing `host_error` redaction in the op handles the console, and `exchange` errors name no values.

- [ ] **Step 1: Write failing tests** (fake executor from plan 01): `grpc_runs_a_saved_request_with_its_server_and_auth`, `grpc_script_messages_replace_the_saved_message`, `grpc_script_metadata_overrides_a_saved_entry`, `grpc_ad_hoc_builds_a_request_from_the_fields`, `grpc_fills_variables_from_scopes_and_runtime_vars`, `grpc_refuses_a_blocked_host_before_connecting`, `grpc_rejects_an_http_request_path`, `grpc_rejects_a_missing_path`, `grpc_ad_hoc_refuses_an_absolute_proto_path`, `grpc_counts_against_the_nested_run_limit`, `grpc_without_a_service_reports_not_available`, `grpc_error_text_never_contains_a_secret_value`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app script_host_tests::grpc_` and expect failures.
- [ ] **Step 3: Implement** per the behaviour above.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app execution_service` and `cargo test -j4 -p rocket-app script_host`. Expect pass.
- [ ] **Step 5: Stage** explicit paths. Do not commit.

---

### Task 3: Wiring and end-to-end tests

**Files:**
- Modify: `src-tauri/src/lib.rs` (pass the existing `GrpcService` Arc to the `RequestExecutionService` with `with_grpc_service`; read where both are built, the service may need to be constructed first)
- Test: `crates/rocket-app/src/execution_service/script_host_tests.rs` (end to end with the real engine, like `run_request_e2e_*`), and one transport test in `crates/rocket-infra/src/grpc/` tests using the greeter fixture in `crates/rocket-infra/test-fixtures/grpc`.

**Interfaces:**
- Consumes: everything above. No new command, so `src-tauri/app-commands.txt` is unchanged.

- [ ] **Step 1: Write failing tests** with the real script engine (not `ProgrammableEngine`): `grpc_e2e_run_returns_status_and_message_to_a_real_script`, `grpc_e2e_call_awaits_in_a_pre_request_script_and_sets_a_header_from_the_reply`, `grpc_e2e_safe_mode_script_is_refused`, `grpc_e2e_server_stream_returns_all_messages`, `grpc_e2e_client_stream_sends_the_messages_array`. Transport: `script_level_unary_call_over_the_real_transport` (greeter fixture, reply message equals the expected greeting).
- [ ] **Step 2: Run** them and expect failures (or ignored without wiring).
- [ ] **Step 3: Implement** the wiring and the missing pieces the tests find.
- [ ] **Step 4: Run** `cargo check -j4 -p rocket-app --tests`, `cargo test -j4 -p rocket-app grpc_e2e`, `cargo test -j4 -p rocket-infra script_level_unary`, `cargo check -j4` for `src-tauri` (via the package name in its `Cargo.toml`).
- [ ] **Step 5: Stage** explicit paths. Do not commit.
