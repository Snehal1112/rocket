# Backend sends resolve `{{process.env}}` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Backend-driven HTTP sends (Flow request nodes, `rok.runRequest`, the assistant's `run_request`, the runner, the load test and introspection) resolve `{{process.env.NAME}}` like a normal send, under the collection trust gate, with `.env` values masked as secrets.

**Architecture:** The one shared resolver, `RequestExecutionService::resolve_request_parts`, takes the gated `process_env_for` map once per send and returns what it used. `begin_phases` turns `.env` values into secrets and adds the withheld warning; the run output carries the used host values so `run_request` can mask them for the agent.

**Tech Stack:** Rust (`rocket-app`), Tokio tests with the in-file doubles of `execution_service.rs`.

**Spec:** `docs/superpowers/specs/2026-10-30-backend-process-env-design.md` (R1 to R8).

## Global Constraints

- Logic stays in `rocket-app`; no new I/O. `.env` files are read only through `dot_env_loader`.
- No panicking unwraps in production paths. Tests may use `expect` with a message.
- Every collection and environment lookup goes through `send_collection_repo()`, `global_env_repo()`, `regular_env_repo()` (send pins).
- Phase logic stays shared: no process env handling in only one caller of the phase methods.
- Process env is the lowest layer (`flatten_with_process_env`); a real OS variable beats a `.env` value of the same name.
- The warning text is exactly: `Host environment variables are not allowed for this collection, so {{process.env.NAME}} was sent unresolved.` (the frontend's text in `src/lib/process-env-gate.ts`), level `Warn`.
- Rust commands: `cargo check -j4`, `cargo test -j4 -p rocket-app <filter>`. Never `--workspace`.
- Builders do not commit. Each task's last step stages its paths by explicit path; the controller commits.

## Review Focus

- A `.env` value shorter than `MIN_REDACTION_LEN` is not masked anywhere; it must not crash or mask unrelated text. (Task 1 test `short_dot_env_value_is_sent_and_not_masked`.)
- A collection with the gate denied must read no `.env` file at all, even on a nested run. (Task 1 test `denied_gate_reads_no_dot_env_file_and_keeps_the_placeholder`.)
- A Safe-mode script that writes `{{process.env.X}}` into the URL must not get a host value. (Task 1 test `script_written_process_env_placeholder_stays_unresolved`.)
- A single send from the request tab must not get a second warning or new values. (Task 2 test `manual_send_with_denied_gate_adds_no_backend_warning`.)
- An echoing endpoint returning an OS value the request used must come back masked to the agent, while a value the request did not use is left alone. (Task 3 tests.)

---

### Task 1: The shared resolver uses gated process env

Covers: R1, R2, R4, R5, R8.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`ResolvedParts` at :340, `resolve_request_parts` at :1307, `begin_phases` at :2097, `PhaseState`, `ExecuteRequestOutput` at :176, the output built at :2839)
- Modify: `crates/rocket-app/src/execution_service/runtime_fill.rs` (only if script-written text can reach process env)
- Test: the `#[cfg(test)]` module of `execution_service.rs`, next to the existing `dot_env_svc` / `FakeDotEnv` tests (:7430 to :7940)

**Interfaces:**
- Produces: `struct SendProcessEnv { dot_env_values: HashSet<String>, used_values: HashSet<String>, withheld_names: Vec<String> }` (crate-private, in `execution_service.rs`).
  - `dot_env_values`: the values of every `dot_env_keys` entry of the send's `process_env_for` result.
  - `used_values`: the values of the `process.env.*` names the first pass resolved in this request's field templates (inherited folder and collection headers and auth included).
  - `withheld_names`: the `process.env.*` names the templates reference while `process_env_allowed` is false. Always empty when the gate allows.
- Produces: `ResolvedParts` gains `process_env: SendProcessEnv`.
- Produces: `PhaseState` gains `process_env: SendProcessEnv`.
- Produces: `ExecuteRequestOutput` gains `pub run_process_env_values: std::collections::HashSet<String>`, which is `used_values` of this send merged with the same field of every nested `rok.runRequest` output.

- [ ] **Step 0: Read the reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (variable resolution, line 543 on `.env`), then the spec's "Settled after the builder check" section.

- [ ] **Step 1: Write the failing tests** (all `#[tokio::test]` unless noted, built on `dot_env_svc`, `FakeDotEnv`, `env_with_dot_env` and `MockExecutor`'s `last_url`):
  - `backend_send_resolves_a_dot_env_value_in_the_url`: env `dev` with `.env` `ROCKET_T1_KEY=abc123secret`, input URL `https://x.test/{{process.env.ROCKET_T1_KEY}}`, `run_source = RunSource::Flow`, `execute` → `last_url == "https://x.test/abc123secret"`.
  - `os_value_beats_a_dot_env_value_in_a_backend_send`: `.env` `PATH=from-file`, URL `https://x.test/?p={{process.env.PATH}}` → the sent URL holds the OS `PATH` (URL-encoded as the executor gets it), not `from-file`.
  - `denied_gate_reads_no_dot_env_file_and_keeps_the_placeholder`: `allow = false` → sent URL still contains `{{process.env.ROCKET_T1_KEY}}` (encoded as the sender encodes it), and `fake.calls` and `fake.default_calls` are both empty.
  - `dot_env_value_is_a_run_secret`: allowed send as above → `output.run_secret_values.contains("abc123secret")`, and the saved History entry's URL does not contain `abc123secret`.
  - `short_dot_env_value_is_sent_and_not_masked`: `.env` `ROCKET_T1_S=ab` → sent, no panic, `run_secret_values` behaves as for any short secret (no assertion on masking beyond "the send succeeds").
  - `used_os_value_is_reported_in_run_process_env_values`: URL uses `{{process.env.PATH}}` → `output.run_process_env_values` contains the OS `PATH`; a `.env` key the request does not reference is not in it.
  - `nested_run_request_target_resolves_process_env`: a pre-request script calling `rok.runRequest('target')` (use the existing script-host test setup in `execution_service/script_host_tests.rs` as the model) where the target's URL uses `{{process.env.ROCKET_T1_KEY}}` → the target is sent with `abc123secret`, and the outer `run_process_env_values` contains it.
  - `script_written_process_env_placeholder_stays_unresolved`: Safe mode, a pre-request script `req.setUrl('https://x.test/{{process.env.ROCKET_T1_KEY}}')` → the sent URL keeps the placeholder.
  - `frontend_resolved_manual_send_is_unchanged`: `run_source = Manual`, URL already literal `https://x.test/abc` → sent as is, `run_process_env_values` empty.

- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -j4 -p rocket-app process_env` (and the names above). Expected: the resolve, secret and `run_process_env_values` tests FAIL (field missing or placeholder sent). `denied_gate…`, `script_written…` and `frontend_resolved…` may already pass; keep them as guards.

- [ ] **Step 3: Implement.** In `resolve_request_parts`, call `process_env_for(collection, environment_name, global_env_name)` once and build the map with `build_variable_scopes(...)` plus `scopes.process_env = pe.vars` and `flatten_with_process_env()`, then `vars.extend(input.flow_vars)` as today. Fill `SendProcessEnv` from `pe.dot_env_keys` and from the names the field templates recorded. In `begin_phases`, insert `dot_env_values` into `var_ctx.secret_values` before anything is redacted, and keep the struct on `PhaseState`. Set `run_process_env_values` where `run_secret_values` is set (:2839), and merge a nested run's value where the script host merges its other nested results. Make sure the text-a-script-wrote fill in `runtime_fill.rs` does not draw from process env.

- [ ] **Step 4: Run them and see them pass.** Run: `cargo test -j4 -p rocket-app process_env` and `cargo test -j4 -p rocket-app execution_service`. Expected: PASS, no other failures.

- [ ] **Step 5: Check.** Run: `cargo check -j4`. Expected: no errors, no new warnings.

- [ ] **Step 6: Stage.** Stage these paths by explicit path (only those you changed): `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/execution_service/runtime_fill.rs`, `crates/rocket-app/src/execution_service/script_host.rs`. The controller commits.

### Task 2: Withheld warning, and Flow parity

Covers: R1 (through a real Flow run), R7, plus the spec's Auth-node static token call.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`begin_phases`, `execute_nested`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`send_time_credential_value` at :1990)
- Test: `execution_service.rs` test module, and `flow_execution_service.rs` test module

**Interfaces:**
- Consumes: `SendProcessEnv.withheld_names` and `PhaseState.process_env` from Task 1.
- Produces: `fn process_env_withheld_entry() -> rocket_scripting::ConsoleEntry` (crate-private), level `Warn`, message exactly the Global Constraints text.

- [ ] **Step 0: Read the reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`, and the "Flow Auth nodes" section of `crates/rocket-app/CLAUDE.md`.

- [ ] **Step 1: Write the failing tests:**
  - `backend_send_with_denied_gate_warns_once`: `allow = false`, `run_source = Flow`, URL and a header both use `{{process.env.X}}` → `output.console_entries` has exactly one entry equal to `process_env_withheld_entry()`; the send still happened.
  - `manual_send_with_denied_gate_adds_no_backend_warning`: same with `run_source = Manual` → no such entry.
  - `nested_run_with_denied_gate_warns_in_the_caller_console`: a Manual send whose pre-request script runs a target using `{{process.env.X}}` with the gate denied → the caller's `console_entries` contains the entry once.
  - `scratch_send_never_warns`: `collection = None`, `run_source = Flow` → no entry.
  - In `flow_execution_service.rs`: `flow_request_node_sends_a_dot_env_value` (a one-node Flow whose request URL uses `{{process.env.ROCKET_T2_KEY}}` from the collection's `.env`; the sent URL holds the value and the step trace does not), and `static_bearer_from_process_env_is_masked_in_the_trace` (an Auth node `Bearer {{process.env.ROCKET_T2_TOKEN}}` whose value is an OS variable set for the test with a unique name; the step trace does not contain it).

- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -j4 -p rocket-app withheld` and `cargo test -j4 -p rocket-app flow_request_node_sends_a_dot_env_value static_bearer_from_process_env`. Expected: FAIL.

- [ ] **Step 3: Implement.** In `begin_phases`, push `process_env_withheld_entry()` to the phase console when `withheld_names` is non-empty and `input.run_source != RunSource::Manual`. In `execute_nested`, push it when `withheld_names` is non-empty and the begin step did not already (a nested run of a Manual send). The line reaches the caller's Console the way other nested console lines do. In `send_time_credential_value`, build the map with `build_variable_context_with_process_env` so the mask matches what is sent.

- [ ] **Step 4: Run them and see them pass.** Run the commands from Step 2, then `cargo test -j4 -p rocket-app flow_execution_service` and `cargo test -j4 -p rocket-app execution_service`. Expected: PASS.

- [ ] **Step 5: Check.** Run: `cargo check -j4`. Expected: clean.

- [ ] **Step 6: Stage.** Stage by explicit path: `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/flow_execution_service.rs`, and any test file you changed. The controller commits.

### Task 3: The assistant masks every used host value

Covers: R3, R6.

**Files:**
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (`run_request` at :765, the secrets set at :839)
- Test: the `mcp_tool_service.rs` test module (model: `literal_request_credentials_are_masked_in_an_echoing_response` at :2424)

**Interfaces:**
- Consumes: `ExecuteRequestOutput.run_process_env_values` from Task 1.

- [ ] **Step 0: Read the reference.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`, and the `McpToolService` row of `crates/rocket-app/CLAUDE.md`.

- [ ] **Step 1: Write the failing tests:**
  - `run_request_sends_and_masks_a_dot_env_value_in_an_echoing_response`: a saved request with header `X-Key: {{process.env.ROCKET_T3_KEY}}` from the collection's `.env`, an executor that echoes request headers in the body → the executor received the real value, and the returned body and test results show it masked, never in clear.
  - `run_request_masks_a_used_os_value`: same with an OS variable set for the test under a unique name (value at least `MIN_REDACTION_LEN` long) → masked in the body.
  - `run_request_leaves_an_unused_host_value_alone`: the echo body also contains the value of an OS variable the request did not reference → that text is unchanged.

- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -j4 -p rocket-app run_request_`. Expected: the first two FAIL (value in clear).

- [ ] **Step 3: Implement.** In `run_request`, extend `secrets` with `output.run_process_env_values` before `mask_response_body` and the test-result masking.

- [ ] **Step 4: Run them and see them pass.** Run: `cargo test -j4 -p rocket-app run_request_` and `cargo test -j4 -p rocket-app mcp_tool_service`. Expected: PASS.

- [ ] **Step 5: Check.** Run: `cargo check -j4`. Expected: clean.

- [ ] **Step 6: Stage.** Stage by explicit path: `crates/rocket-app/src/mcp_tool_service.rs`. The controller commits.
