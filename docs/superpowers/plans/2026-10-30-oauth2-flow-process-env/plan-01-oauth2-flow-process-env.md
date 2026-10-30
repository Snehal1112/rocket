# Plan 1: process env module and Flow nodes

> **For agentic workers:** this plan is run by the factory driver. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Move the process env types into their own module, then let Flow Input nodes and Auth-node OAuth2 fetches resolve `{{process.env}}` under one gate decision per run, with `.env` values masked.

**Architecture:** A pure `process_env` module in `rocket-app` holds `ProcessEnvScopeInput`, `ProcessEnv` and `scope_target`. The Flow run reads `process_env_for` once at start, feeds its map to the Input node and to `FetchContext.vars`, and adds its `.env` values to `external_secrets` under keys no template can reference, so every existing mask path covers them.

**Tech Stack:** Rust, tokio, serde. Tests use `tokio::test` and the doubles already in `flow_execution_service.rs`.

**Spec:** docs/superpowers/specs/2026-10-30-oauth2-flow-process-env-design.md (sections 0 to 3)

## Global Constraints

- Cargo is per crate: `cargo check -j4 -p rocket-app`, `cargo test -j4 -p rocket-app <filter>`. Never a bare workspace command.
- No panics in production paths. Tests use `.expect("...")` and `expect_err`.
- Logic stays in `rocket-app`, no new I/O. `.env` files are read only through `dot_env_loader`, and only when the gate allows it.
- The gate is decided once per Flow run. Every Flow lookup stays inside `with_send_pins`.
- Fetched tokens and token responses never resolve any scope.
- `.env` values shorter than `MIN_REDACTION_LEN` are not masked, as in #112.
- The withheld line is exactly the message of `process_env_withheld_entry()` (`execution_service.rs:378`), level warn.

## Review Focus

1. A run whose gate is denied must read no `.env` file at all, even though Input and Auth nodes now ask for process env. Pinned by `flow_run_reads_no_dot_env_file_when_denied` (Task 2).
2. An Input value that names an unknown `{{process.env.NOPE}}` while the gate allows it stays literal and gives no warning. Pinned by `flow_input_node_unknown_process_env_stays_literal_without_warning` (Task 2).
3. A supplied UI token for an OAuth2 Auth node must not trigger the withheld warning, because no fetch happened. Pinned by `flow_auth_supplied_token_never_warns` (Task 3).
4. A `.env` value used by an Input node must be masked in the Output node's step value too, not only in the Input step. Pinned by `flow_input_node_masks_dot_env_value_in_step_and_output` (Task 2).
5. Moving the types must not change their wire form: the serde tests move unchanged and still pass. Pinned by the moved `scope_deserializes_scratch_collection_and_closed` (Task 1).

## File Structure

- Create `crates/rocket-app/src/process_env.rs`: `ProcessEnvScopeInput`, `ProcessEnv`, `scope_target`, their unit tests.
- Modify `crates/rocket-app/src/lib.rs`: `mod process_env;`, re-export `ProcessEnvScopeInput` and `ProcessEnv` from it.
- Modify `crates/rocket-app/src/execution_service/websocket_resolution.rs`: drop the type, import it.
- Modify `crates/rocket-app/src/execution_service.rs`: drop `ProcessEnv`, import it; `process_env_for_scope` uses `scope_target`; new `variable_context_with_process_env`.
- Modify `crates/rocket-app/src/flow_execution_service.rs`: run-level process env, Input arm, Auth arm warning, fetch error mask, tests.
- Modify `crates/rocket-app/src/flow_auth.rs`: `uses_process_env`, tests.

---

### Task 1: `process_env` module

**Files:**
- Create: `crates/rocket-app/src/process_env.rs`
- Modify: `crates/rocket-app/src/lib.rs:118-121`, `crates/rocket-app/src/execution_service/websocket_resolution.rs:18-35`, `:757-785`, `crates/rocket-app/src/execution_service.rs:519-530`, `:1314-1336`

**Interfaces:**
- Produces: `crate::process_env::{ProcessEnvScopeInput, ProcessEnv}` (same derives, fields and serde attributes as today), `pub(crate) fn scope_target<'a>(collection: Option<&'a str>, environment_name: Option<&'a str>, scope: &'a ProcessEnvScopeInput) -> Option<(Option<&'a str>, Option<&'a str>)>`. `rocket_app::ProcessEnvScopeInput` and `rocket_app::ProcessEnv` still resolve.

- [ ] **Step 1: Read the reference and the code.** Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `execution_service.rs:515-530` and `:1314-1336`, `websocket_resolution.rs:1-60` and `:700-790`, `lib.rs:110-125`.

- [ ] **Step 2: Write the failing tests in `process_env.rs`** (add `mod process_env;` to `lib.rs` first; the types stay where they are until Step 4):

```rust
#[test]
fn scope_target_closed_is_none() {
    assert_eq!(scope_target(Some("api"), Some("dev"), &ProcessEnvScopeInput::Closed), None);
    assert_eq!(scope_target(None, None, &ProcessEnvScopeInput::Closed), None);
}

#[test]
fn scope_target_collection_input_keeps_its_own_gate() {
    let scope = ProcessEnvScopeInput::Collection { name: "other".into(), environment_name: Some("prod".into()) };
    assert_eq!(scope_target(Some("api"), Some("dev"), &scope), Some((Some("api"), Some("dev"))));
    assert_eq!(scope_target(Some("api"), Some("dev"), &ProcessEnvScopeInput::Scratch), Some((Some("api"), Some("dev"))));
}

#[test]
fn scope_target_scratch_without_collection_reads_no_collection() {
    assert_eq!(scope_target(None, Some("dev"), &ProcessEnvScopeInput::Scratch), Some((None, Some("dev"))));
    let scope = ProcessEnvScopeInput::Collection { name: "api".into(), environment_name: None };
    assert_eq!(scope_target(None, Some("dev"), &scope), Some((Some("api"), None)));
}
```

Also move `scope_deserializes_scratch_collection_and_closed` (`websocket_resolution.rs:759-785`) into this module unchanged.

- [ ] **Step 3: Run them.** `cargo test -j4 -p rocket-app process_env::` — expected FAIL: `cannot find function 'scope_target' in this scope`.

- [ ] **Step 4: Implement.** Move `ProcessEnvScopeInput` and `ProcessEnv` (with their doc comments) into `process_env.rs`. Write `scope_target` with the match of `process_env_for_scope` (`execution_service.rs:1321-1335`): closed gives `None`, a given collection gives `(collection, environment_name)`, no collection with `Collection { name, environment_name }` gives `(Some(name), environment_name)`, no collection with `Scratch` gives `(None, environment_name)`. Rewrite `process_env_for_scope` as `scope_target(...)` then `process_env_for(c, e, global_env_name)` or `ProcessEnv::default()`. Update the imports in `websocket_resolution.rs`, `execution_service.rs` and the `lib.rs` re-exports. `src-tauri` must need no change.

- [ ] **Step 5: Run.** `cargo test -j4 -p rocket-app process_env` — expected PASS (4 new or moved tests plus every existing `process_env` test). Then `cargo check -j4 -p rocket-app`.

### Task 2: Run-level process env and Input nodes

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (near `build_variable_context`, `:1150-1170`), `crates/rocket-app/src/flow_execution_service.rs:1160-1180`, `:1636-1668`, tests module

**Interfaces:**
- Consumes: `crate::process_env::ProcessEnv` (Task 1), `RequestExecutionService::process_env_for`.
- Produces: `pub(crate) fn variable_context_with_process_env(&self, global_env_name: Option<&str>, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>, external_secrets: &HashMap<String, String>, process_env: &HashMap<String, String>) -> HashMap<String, String>` on `RequestExecutionService` (named apart from `build_variable_context_with_process_env`, which reads the gate itself). `pub(crate) fn dot_env_secret_key(index: usize) -> String` in `flow_execution_service.rs`, built like `flow_run_cache::previous_run_secret_key` (`flow_run_cache.rs:505`) with its own prefix, so no template can name it. The run keeps `run_process_env: ProcessEnv` for Task 3 and passes it down to `execute_node` (add it to `NodeRunContext` or as a parameter).

- [ ] **Step 1: Read the reference and the code.** Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `flow_execution_service.rs:1120-1260`, `:1618-1670`, the fixtures `host_env_exec` (`:10349`), `FixedDotEnv` and `DirEnvFactory` (`:10194-10232`), `flow_request_node_sends_a_dot_env_value` (`:10235`), `a_flow_step_with_a_withheld_value_warns_in_its_logs` (`:10850`), `input_node_with` (`:4437`), `output_node_named` and `output_of_input`.

- [ ] **Step 2: Write the failing tests** in the `flow_execution_service.rs` tests module. Build the exec like `flow_request_node_sends_a_dot_env_value` does (a `dotenv_exec(allow: bool)` helper with `FixedDotEnv` pairs `[("ROCKET_IN_KEY", "dotenv-input-123456")]` and an `InMemoryTrustStore::allow_all()` or `::new()`). For the read count, wrap `FixedDotEnv` in a counting variant (`CountingDotEnv` with an `Arc<AtomicUsize>` bumped in `load` and `load_default`). Full example:

```rust
#[tokio::test]
async fn flow_input_node_masks_dot_env_value_in_step_and_output() {
    let flow = Flow {
        name: "in".to_string(),
        nodes: vec![input_node_with("i", "k={{process.env.ROCKET_IN_KEY}}"), output_node_named("out")],
        edges: vec![wire("e1", "i", "out")],
        callback_host: None,
    };
    let (service, exec) = input_flow_service(flow, dotenv_exec(true));
    let summary = service.run(&exec, run_input("in")).await.expect("run");
    let input_step = step_of(&summary, "i");
    assert!(!format!("{input_step:?}").contains("dotenv-input-123456"), "{input_step:?}");
    let out_step = step_of(&summary, "out");
    assert!(!format!("{out_step:?}").contains("dotenv-input-123456"), "{out_step:?}");
}
```

| Test | Input value | Gate | Assertion |
|---|---|---|---|
| `flow_input_node_resolves_process_env_when_allowed` | `{{process.env.PATH}}` wired to an Output | allow | the Output's captured value equals `std::env::var("PATH").expect("PATH is set")` |
| `flow_input_node_keeps_process_env_literal_when_denied` | `{{process.env.PATH}}` | deny | captured value is `"{{process.env.PATH}}"` |
| `flow_input_node_warns_once_when_withheld` | `{{process.env.PATH}}-{{process.env.HOME}}` | deny | `step_of(&summary, "i").logs` has exactly 1 entry whose message equals `process_env_withheld_entry().message`, level `FlowLogLevel::Warn` |
| `flow_input_node_unknown_process_env_stays_literal_without_warning` | `{{process.env.ROCKET_NOPE_9}}` | allow | value literal, 0 withheld lines |
| `flow_run_reads_no_dot_env_file_when_denied` | `{{process.env.ROCKET_IN_KEY}}` | deny | the counting loader's count is 0 after the run |

Read captured values the way `output_of_input` does. `input_flow_service(flow, exec) -> (FlowExecutionService, RequestExecutionService)` is a new helper that builds the service as `flow_request_node_sends_a_dot_env_value` does, with `FakeFlowRepository::new().with_flow("my-api", flow)`. `dotenv_exec(allow)` uses `CountingDotEnv` so `flow_run_reads_no_dot_env_file_when_denied` can read the count.

- [ ] **Step 3: Run them.** `cargo test -j4 -p rocket-app flow_input_node_ flow_run_reads_no_dot_env` — expected FAIL: `flow_input_node_resolves_process_env_when_allowed` gets `"{{process.env.PATH}}"` instead of the PATH value, and `flow_input_node_warns_once_when_withheld` finds 0 withheld lines. The literal and no-read tests may already pass; they guard the change.

- [ ] **Step 4: Implement.** Add `variable_context_with_process_env` (build scopes as `build_variable_context` does, set `scopes.process_env = process_env.clone()`, return `flatten_with_process_env()`). In `run_with_options_unpinned`, right after `resolve_external_secrets`, call `exec.process_env_for(Some(&input.collection), input.environment_name.as_deref(), input.global_env_name.as_deref())` once. Insert each `.env` value (values of `dot_env_keys`, length at least `MIN_REDACTION_LEN`) into `external_secrets` under `dot_env_secret_key(i)`. In the Input arm, resolve with `variable_context_with_process_env(..., &run_process_env.vars)` plus `callbacks.vars()`. When `!run_process_env.allowed` and the value names a `process.env.` placeholder, push one `FlowLogEntry { level: FlowLogLevel::Warn, message: process_env_withheld_entry().message }` into `logs`.

- [ ] **Step 5: Run.** `cargo test -j4 -p rocket-app flow_` — expected PASS, the 5 new tests plus all existing Flow tests.

### Task 3: Auth-node fetches

**Files:**
- Modify: `crates/rocket-app/src/flow_auth.rs:38-45`, `:221-288`, tests; `crates/rocket-app/src/flow_execution_service.rs:1168-1185`, `:2001-2015`, tests

**Interfaces:**
- Consumes: `run_process_env` and `variable_context_with_process_env` (Task 2).
- Produces: `pub(crate) fn uses_process_env(flow: &OAuth2Flow) -> bool` in `flow_auth.rs`: true when any field `build_get_token_request` resolves holds `{{process.env.`. Test double `CapturingFetcher` in `flow_auth::test_support`: records `build_get_token_request(flow, ctx)` and returns a fixed token.

- [ ] **Step 1: Read the reference and the code.** Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `flow_auth.rs:200-345`, `test_support` (`:445-560`, `client_credentials()`, `FakeFetcher`, `SharedFetcher`), `vars_ctx` (`:1125`), `flow_execution_service.rs:960-970` (`with_token_fetcher`), `:1160-1195`, `:1730-1740`, `:2001-2015`, and an existing Flow test that runs an OAuth2 Auth node with `with_token_fetcher`.

- [ ] **Step 2: Write the failing tests.** Add `CapturingFetcher` (fields `seen: Mutex<Vec<OAuth2GetTokenRequest>>`, `token: String`, error variant `fail_with: Option<String>`). Use an OAuth2 node built from `client_credentials()` with `client_secret` set to `"{{process.env.ROCKET_CS_KEY}}"`, `FixedDotEnv` pair `("ROCKET_CS_KEY", "dotenv-secret-654321")`, applied to inherit by a request node. `run_auth_flow(exec, fetcher: &Arc<CapturingFetcher>) -> DomainResult<FlowRunSummary>` is a new helper: a flow of that Auth node (`apply_to_inherit: true`) and `request_flow_node("r", "https://api.example.com/r")` with `auth: Auth::Inherit`, the service built with `.with_token_fetcher(Box::new(SharedCapturingFetcher(Arc::clone(fetcher))))` (a wrapper like `SharedFetcher`). Full example:

```rust
#[tokio::test]
async fn flow_auth_fetch_sends_process_env_client_secret() {
    let fetcher = CapturingFetcher::ok("tok-abcdef123456");
    let summary = run_auth_flow(dotenv_exec(true), &fetcher).await.expect("run");
    let seen = fetcher.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].client_secret.as_deref(), Some("dotenv-secret-654321"));
    assert!(!format!("{summary:?}").contains("dotenv-secret-654321"));
}
```

| Test | Setup | Assertion |
|---|---|---|
| `flow_auth_fetch_keeps_literal_when_denied` | gate denied | `client_secret == Some("{{process.env.ROCKET_CS_KEY}}")`; the Auth node's step logs hold exactly one withheld line |
| `flow_auth_supplied_token_never_warns` | gate denied, a `SuppliedToken` for the node | fetcher called 0 times, no withheld line anywhere |
| `flow_auth_fetched_token_stays_literal` | fetcher returns `"{{process.env.PATH}}"` | the request sent `Authorization: Bearer {{process.env.PATH}}` |
| `flow_static_bearer_wire_snapshot_resolves_process_env` | static `Auth::Bearer { token: "{{process.env.ROCKET_CS_KEY}}" }` node wired to an Output | the request sent `Bearer dotenv-secret-654321`; the Output step does not contain `dotenv-secret-654321` |
| `flow_auth_fetch_error_masks_dot_env_value` | `CapturingFetcher::err("endpoint said dotenv-secret-654321")` | the run error text has no `dotenv-secret-654321` |
| `uses_process_env_finds_secret_and_params` (unit, `flow_auth.rs`) | `client_credentials()` with the secret, then with an additional token parameter value `{{process.env.X}}` | true both times; false for plain `client_credentials()` |

- [ ] **Step 3: Run them.** `cargo test -j4 -p rocket-app flow_auth_ flow_static_bearer uses_process_env` — expected FAIL: `cannot find function 'uses_process_env'`, and the client secret arrives as the literal.

- [ ] **Step 4: Implement.** Build `FetchContext.vars` with `variable_context_with_process_env(..., &run_process_env.vars)`. Add `uses_process_env`. In the Auth arm of `execute_node`, when `!run_process_env.allowed`, `credentials.is_fetched_token(&node.id)` and the node's auth is OAuth2 with `uses_process_env`, push one withheld `FlowLogEntry`. Map the fetch error of `resolve_flow_credentials` through `redact_secrets` with the run's `.env` values before it leaves the run.

- [ ] **Step 5: Run.** `cargo test -j4 -p rocket-app flow_` and `cargo test -j4 -p rocket-app flow_auth` — expected PASS, the 7 new tests plus every existing Flow and flow_auth test.

Next plan: docs/superpowers/plans/2026-10-30-oauth2-flow-process-env/plan-02-oauth2-flow-process-env.md
