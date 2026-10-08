# rok parity B, plan 04: `rok.runRequest`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rok.runRequest("Folder/Name")` runs a saved request through the full pipeline (scripts, auth, variables) and resolves with its response, with a recursion guard and with the nested run's variable writes merged back into the calling script.

**Architecture:** `ScriptHost` gains a defaulted `run_request`. The engine op seeds the call with the script's current runtime variables and merges the outcome into its input snapshot, its output and the JS read-your-writes overlay. In `rocket-app`, pure helpers in `execution_service/run_request.rs` find the target and check the call chain, and `ExecutionScriptHost::run_request` calls the new `RequestExecutionService::execute_nested`, which drives the same phases with the same engine. The chain of request paths travels in `PhaseState`.

**Tech Stack:** Rust, `deno_core`, `async-trait`, Tokio, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`, section 4. Index with rulings: `00-plan-index.md` (rulings 3, 9, 10, 11 and 13 apply here). Requires plans 01 to 03.

## Global Constraints

- `cargo` commands always pass `-j4`. No `cargo test --workspace`.
- Path relative to the collection root, no extension, forward slashes.
- Non-HTTP items resolve to `{ status: "skipped" }`. An unknown path rejects with exactly `rok.runRequest: invalid request path - <path>` (the path as the script wrote it).
- Recursion guard: revisiting a request rejects with `rok.runRequest: recursive call to <path>`; more than 5 nested levels reject with `rok.runRequest: nesting deeper than 5 requests`.
- Nested variable writes are merged into the calling script's input snapshot and output, so a later `rok.getVar` or `rok.getEnvVar` sees them.
- No `Arc`/`Weak` service handle and no second engine (ruling 3).
- gRPC files stay untouched. `run_request.rs` only reads `CollectionItem::Grpc(..).file_name`.
- Comments are short full sentences ending in a period.

## Review Focus

- A script that sets `token`, calls `runRequest`, and the nested run sets `token` too: later reads (and later phases) see the nested value.
- A script that sets env `E` and the nested run also sets `E`: the nested value wins on disk too (the caller's earlier write is dropped).
- A collection-level pre-request script that calls `runRequest` on another request: the nested copy of that script is rejected as recursive, the nested request still sends, and the outer request gets its response.
- A path written with a leading slash, a trailing `.yml` or backslashes: it resolves to the same request.
- A WebSocket request path: `{ status: "skipped" }`, nothing is sent.

---

### Task 1: Engine-side `rok.runRequest`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-scripting/src/host.rs`, `crates/rocket-scripting/src/lib.rs`
- Modify: `crates/rocket-infra/src/scripting/host_bridge.rs`, `crates/rocket-infra/src/scripting/ops/host.rs`, `crates/rocket-infra/src/scripting/engine.rs` (registration, tests), `crates/rocket-infra/src/scripting/bootstrap.js`
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Consumes: plan 01 (`ScriptHost`, `HostCall`, `send_host_call`, `host_error`, `_hostResponse`, `_ov`), plan 03 Task 1 (Console line style).
- Produces (rocket-scripting): `pub struct HostRunRequest { pub path: String, pub runtime_vars: HashMap<String, String> }`, `pub struct HostScopes { pub env, pub global_env, pub collection: HashMap<String, String>, pub secret_values: Vec<String> }`, `pub struct HostRunOutcome { pub response: Option<HostResponse>, pub runtime_set: HashMap<String, String>, pub runtime_removed: Vec<String>, pub scopes: Option<HostScopes> }` (all `Debug, Clone, Default, PartialEq, Eq`; `HostRunRequest` has no `Default`), and `ScriptHost::run_request(&self, request: HostRunRequest) -> Result<HostRunOutcome, HostError>` (default `Err(HostError::Unavailable)`).
- Produces (rocket-infra): `HostCall::Run { request, reply }`, `op_rok_run_request(state, path: String) -> Result<String, ScriptHostError>` returning `{"response": HostResponse | null, "changed": {"runtime": [..], "env": [..], "global": [..], "collection": [..]}}`.
- Produces (JS): `rok.runRequest(path)` resolving to `{ status, statusText, headers, data, responseTime }` or `{ status: 'skipped' }`.

- [ ] **Step 1: Add the types and the trait method**

In `crates/rocket-scripting/src/host.rs`, add `use std::collections::HashMap;` to the imports, add these types after `HostError`:

```rust
/// A `rok.runRequest` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRunRequest {
    /// Path relative to the collection root, without extension, as the script wrote it.
    pub path: String,
    /// The calling script's runtime variables, its own writes so far included.
    pub runtime_vars: HashMap<String, String>,
}

/// Variable scopes as stored after a nested run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostScopes {
    pub env: HashMap<String, String>,
    pub global_env: HashMap<String, String>,
    pub collection: HashMap<String, String>,
    /// Secret values of those scopes, added to the caller's redaction list.
    pub secret_values: Vec<String>,
}

/// The outcome of a `rok.runRequest` call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostRunOutcome {
    /// The response, or `None` when the item is not an HTTP request and was skipped.
    pub response: Option<HostResponse>,
    /// Runtime variables the nested run set or changed.
    pub runtime_set: HashMap<String, String>,
    /// Runtime variables the nested run removed.
    pub runtime_removed: Vec<String>,
    /// The scopes after the nested run, or `None` when nothing ran.
    pub scopes: Option<HostScopes>,
}
```

and add to the `ScriptHost` trait, after `send_request`:

```rust
    /// Runs a saved request through the full pipeline for `rok.runRequest`.
    async fn run_request(&self, _request: HostRunRequest) -> Result<HostRunOutcome, HostError> {
        Err(HostError::Unavailable)
    }
```

Change the export line in `crates/rocket-scripting/src/lib.rs` to:

```rust
pub use host::{
    HostError, HostRequest, HostResponse, HostRunOutcome, HostRunRequest, HostScopes, ScriptHost,
};
```

Run: `cargo test -j4 -p rocket-scripting`
Expected: PASS.

- [ ] **Step 2: Write the failing engine tests**

Add to the `tests` module in `engine.rs`, after the `send_request_` tests:

```rust
    // ── runRequest ───────────────────────────────────────────────────────────

    use rocket_scripting::{HostRunOutcome, HostRunRequest, HostScopes};

    /// Host that answers `rok.runRequest` with a fixed outcome and records the calls.
    struct RunHost {
        seen: StdMutex<Vec<HostRunRequest>>,
        answer: Result<HostRunOutcome, HostError>,
    }

    impl RunHost {
        fn answering(answer: Result<HostRunOutcome, HostError>) -> Self {
            Self {
                seen: StdMutex::new(Vec::new()),
                answer,
            }
        }

        fn seen(&self) -> Vec<HostRunRequest> {
            self.seen.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl ScriptHost for RunHost {
        async fn run_request(&self, request: HostRunRequest) -> Result<HostRunOutcome, HostError> {
            self.seen.lock().expect("lock").push(request);
            self.answer.clone()
        }
    }

    fn ran(status: u16, body: &str) -> HostRunOutcome {
        HostRunOutcome {
            response: Some(HostResponse {
                status,
                status_text: "OK".into(),
                headers: vec![],
                body: body.into(),
                response_time_ms: 5,
            }),
            ..Default::default()
        }
    }

    fn map(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[tokio::test]
    async fn run_request_resolves_with_the_response_and_logs_it() {
        let host = RunHost::answering(Ok(ran(200, "{\"ok\":true}")));
        let ctx = minimal_ctx(
            "const r = await rok.runRequest('auth/login'); rok.setVar('s', r.status + '|' + r.data.ok)",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.runtime_vars.get("s").expect("s present"), "200|true");
        assert_eq!(host.seen()[0].path, "auth/login");
        assert!(result
            .console_entries
            .iter()
            .any(|c| c.message == "rok.runRequest auth/login -> 200"));
    }

    #[tokio::test]
    async fn run_request_passes_the_callers_runtime_vars() {
        let host = RunHost::answering(Ok(ran(200, "{}")));
        let mut ctx = minimal_ctx("rok.setVar('b', 2); rok.deleteVar('a'); await rok.runRequest('x')");
        ctx.variables.runtime = map(&[("a", "1"), ("keep", "k")]);
        DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(host.seen()[0].runtime_vars, map(&[("keep", "k"), ("b", "2")]));
    }

    #[tokio::test]
    async fn run_request_merges_the_nested_writes() {
        let nested_secret = "sk-live-nested1";
        let outcome = HostRunOutcome {
            runtime_set: map(&[("token", "t1")]),
            runtime_removed: vec!["gone".into()],
            scopes: Some(HostScopes {
                env: map(&[("E", "new"), ("F", "same")]),
                global_env: map(&[]),
                collection: map(&[("C", "c2")]),
                secret_values: vec![nested_secret.into()],
            }),
            ..ran(200, "{}")
        };
        let host = RunHost::answering(Ok(outcome));
        let mut ctx = minimal_ctx(&format!(
            "rok.setVar('token', 'old'); rok.setEnvVar('E', 'mine'); rok.setEnvVar('F', 'mine-too'); \
             await rok.runRequest('x'); \
             rok.setVar('seen', [rok.getVar('token'), rok.getEnvVar('E'), rok.getEnvVar('F'), \
               rok.getCollectionVar('C'), rok.hasVar('gone')].join('|')); \
             console.log('{nested_secret}');"
        ));
        ctx.variables.runtime = map(&[("gone", "x")]);
        ctx.variables.env = map(&[("E", "old"), ("F", "same")]);
        ctx.variables.collection = map(&[("C", "c1")]);
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("seen").expect("seen present"),
            "t1|new|mine-too|c2|false"
        );
        assert_eq!(result.runtime_vars.get("token").expect("token present"), "t1");
        assert!(result.runtime_var_deletes.contains(&"gone".to_string()));
        let env_keys: Vec<_> = result.env_var_writes.iter().map(|w| w.key.clone()).collect();
        assert_eq!(env_keys, vec!["F".to_string()], "the nested run's E wins");
        assert!(result
            .console_entries
            .iter()
            .all(|c| !c.message.contains(nested_secret)));
    }

    #[tokio::test]
    async fn run_request_skipped_item_resolves_status_skipped() {
        let host = RunHost::answering(Ok(HostRunOutcome::default()));
        let ctx = minimal_ctx("const r = await rok.runRequest('ws'); rok.setVar('s', r.status)");
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("s").expect("s present"), "skipped");
        assert!(result
            .console_entries
            .iter()
            .any(|c| c.message == "rok.runRequest ws -> skipped"));
    }

    #[tokio::test]
    async fn run_request_failure_rejects_with_the_host_message() {
        let host = RunHost::answering(Err(HostError::Failed(
            "rok.runRequest: invalid request path - nope".into(),
        )));
        let ctx = minimal_ctx(
            "try { await rok.runRequest('nope'); } catch (e) { rok.setVar('e', e.message); }",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(
            result.runtime_vars.get("e").expect("e present"),
            "rok.runRequest: invalid request path - nope"
        );
        assert!(result
            .console_entries
            .iter()
            .any(|c| c.level == rocket_scripting::ConsoleLevel::Error));
    }

    #[tokio::test]
    async fn run_request_without_a_host_rejects_as_not_available() {
        let code = "try { await rok.runRequest('x'); } catch (e) { rok.setVar('e', e.message); }";
        let without = DenoScriptEngine::new()
            .execute(minimal_ctx(code))
            .await
            .expect("execute");
        let bare = DenoScriptEngine::new()
            .execute_with_host(minimal_ctx(code), &BareHost)
            .await
            .expect("execute");
        for result in [without, bare] {
            assert_eq!(
                result.runtime_vars.get("e").expect("e present"),
                "rok.runRequest is not available here"
            );
        }
    }

    #[tokio::test]
    async fn run_request_rejects_an_empty_path_before_the_host() {
        let host = RunHost::answering(Ok(ran(200, "{}")));
        let ctx = minimal_ctx(
            "try { await rok.runRequest('  '); } catch (e) { rok.setVar('t', e instanceof TypeError); }",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("t").expect("t present"), true);
        assert!(host.seen().is_empty());
    }
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra run_request_`
Expected: FAIL with `rok.runRequest is not a function` (the lookups panic).

- [ ] **Step 4: Route the call to the host**

In `crates/rocket-infra/src/scripting/host_bridge.rs`, change the `rocket_scripting` import to:

```rust
use rocket_scripting::{
    HostError, HostRequest, HostResponse, HostRunOutcome, HostRunRequest, ScriptHost,
};
```

add this variant to `HostCall`:

```rust
    /// `rok.runRequest`.
    Run {
        request: HostRunRequest,
        reply: oneshot::Sender<Result<HostRunOutcome, HostError>>,
    },
```

and this arm to the `match` in `serve_host_call`:

```rust
        HostCall::Run { request, reply } => {
            let _ = reply.send(host.run_request(request).await);
        }
```

- [ ] **Step 5: Add the op and the merge**

In `crates/rocket-infra/src/scripting/ops/host.rs`, change the imports to include:

```rust
use std::collections::HashMap;

use rocket_scripting::{ConsoleLevel, HostError, HostRequest, HostRunOutcome, HostRunRequest};

use crate::scripting::state::{ScriptInputState, ScriptOutputState};
```

and add at the end of the file:

```rust
/// Runtime variables the calling script sees right now: its snapshot plus its own writes.
fn current_runtime_vars(state: &OpState) -> HashMap<String, String> {
    let mut vars = state.borrow::<ScriptInputState>().variables.runtime.clone();
    let out = state.borrow::<ScriptOutputState>();
    for (key, value) in &out.runtime_vars {
        match value {
            serde_json::Value::Null => {}
            serde_json::Value::String(text) => {
                vars.insert(key.clone(), text.clone());
            }
            other => {
                vars.insert(key.clone(), other.to_string());
            }
        }
    }
    for key in &out.runtime_var_deletes {
        vars.remove(key);
    }
    vars
}

/// Keys whose values a nested run changed, per scope. The script drops them
/// from its read-your-writes overlay, so its next read sees the nested value.
#[derive(Default, serde::Serialize)]
struct ChangedKeys {
    runtime: Vec<String>,
    env: Vec<String>,
    global: Vec<String>,
    collection: Vec<String>,
}

/// Keys present in either map whose values differ, sorted.
fn changed_keys(before: &HashMap<String, String>, after: &HashMap<String, String>) -> Vec<String> {
    let mut keys: Vec<String> = before
        .keys()
        .chain(after.keys())
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Merges a nested run into the calling script's snapshot and output.
///
/// The nested run happened after the caller's earlier writes, so it wins: a
/// pending caller write to a key the nested run changed is dropped.
fn merge_run_outcome(state: &mut OpState, run: &HostRunOutcome) -> ChangedKeys {
    let mut changed = ChangedKeys::default();
    {
        let input = state.borrow_mut::<ScriptInputState>();
        for (key, value) in &run.runtime_set {
            input.variables.runtime.insert(key.clone(), value.clone());
        }
        for key in &run.runtime_removed {
            input.variables.runtime.remove(key);
        }
        if let Some(scopes) = &run.scopes {
            changed.env = changed_keys(&input.variables.env, &scopes.env);
            changed.global = changed_keys(&input.variables.global_env, &scopes.global_env);
            changed.collection = changed_keys(&input.variables.collection, &scopes.collection);
            input.variables.env = scopes.env.clone();
            input.variables.global_env = scopes.global_env.clone();
            input.variables.collection = scopes.collection.clone();
            for value in &scopes.secret_values {
                input.secret_values.insert(value.clone());
                input.variables.secret_values.insert(value.clone());
            }
        }
    }
    let out = state.borrow_mut::<ScriptOutputState>();
    for (key, value) in &run.runtime_set {
        out.runtime_var_deletes.retain(|k| k != key);
        out.runtime_vars
            .insert(key.clone(), serde_json::Value::String(value.clone()));
    }
    for key in &run.runtime_removed {
        out.runtime_vars.remove(key);
        out.runtime_var_deletes.push(key.clone());
    }
    out.env_var_writes.retain(|w| !changed.env.contains(&w.key));
    out.global_env_var_writes
        .retain(|w| !changed.global.contains(&w.key));
    out.collection_var_writes
        .retain(|w| !changed.collection.contains(&w.key));
    changed.runtime = run
        .runtime_set
        .keys()
        .chain(run.runtime_removed.iter())
        .cloned()
        .collect();
    changed.runtime.sort();
    changed
}

/// rok.runRequest(path) — runs a saved request through the host and merges its
/// variable writes into this script. Returns `{"response": .., "changed": ..}` as JSON.
#[op2]
#[string]
pub async fn op_rok_run_request(
    state: Rc<RefCell<OpState>>,
    #[string] path: String,
) -> Result<String, ScriptHostError> {
    const API: &str = "rok.runRequest";
    let runtime_vars = current_runtime_vars(&state.borrow());
    let request = HostRunRequest {
        path: path.clone(),
        runtime_vars,
    };
    let (reply, answer) = oneshot::channel();
    send_host_call(&state, HostCall::Run { request, reply }, API)?;
    let outcome = answer.await.unwrap_or(Err(HostError::Unavailable));
    let mut state = state.borrow_mut();
    match outcome {
        Err(error) => {
            let error = host_error(&state, API, error);
            let line = redact(&state, format!("{API} {path} failed: {}", error.0));
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Error, line);
            Err(error)
        }
        Ok(run) => {
            let changed = merge_run_outcome(&mut state, &run);
            let status = run
                .response
                .as_ref()
                .map(|r| r.status.to_string())
                .unwrap_or_else(|| "skipped".to_string());
            let line = redact(&state, format!("{API} {path} -> {status}"));
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Log, line);
            Ok(serde_json::json!({ "response": run.response, "changed": changed }).to_string())
        }
    }
}
```

In `engine.rs`, add to the extension list after `host::op_rok_sleep,`:

```rust
        host::op_rok_run_request,
```

- [ ] **Step 6: Add the JS wrapper**

In `bootstrap.js`, in the `rok` block, add after the `sleep:` entry:

```js
    runRequest: async (path) => {
      if (typeof path !== 'string' || path.trim() === '') {
        throw new TypeError('rok.runRequest: path must be a non-empty string');
      }
      const out = JSON.parse(await __ops.op_rok_run_request(path));
      // Reads after this call see the nested run's values, not this script's older writes.
      for (const k of out.changed.runtime) _ov.runtime.delete(k);
      for (const k of out.changed.env) _ov.env.delete(k);
      for (const k of out.changed.global) _ov.global.delete(k);
      for (const k of out.changed.collection) _ov.collection.delete(k);
      if (out.response === null) return { status: 'skipped' };
      return _hostResponse(out.response);
    },
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -j4 -p rocket-infra run_request_`
Expected: PASS (7 tests).

- [ ] **Step 8: Add the typing and run all checks**

In `ROK_DEFS` in `src/components/editor/rok-types.ts`, add after the `sleep(ms: number): Promise<void>;` line:

```ts
  /** Run a saved request with its scripts, auth and variables. The path is relative to the collection root, without extension, e.g. "auth/login". Other protocols resolve to { status: "skipped" }. */
  runRequest(path: string): Promise<RokResponse | { status: 'skipped' }>;
```

Run: `cargo test -j4 -p rocket-infra scripting && cargo test -j4 -p rocket-scripting && cargo check -j4 && yarn test rok-types && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 9: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-scripting/src/host.rs`, `crates/rocket-scripting/src/lib.rs`, `crates/rocket-infra/src/scripting/host_bridge.rs`, `crates/rocket-infra/src/scripting/ops/host.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add rok.runRequest with nested write merge`.

---

### Task 2: Path lookup and the recursion guard

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/execution_service/run_request.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (module declaration only)

**Interfaces:**
- Consumes: `crate::runner_sequence::{flatten_run_set, RunItem}`.
- Produces: `pub(crate) const MAX_RUN_DEPTH: usize = 5`; `pub(crate) enum RunTarget { Http(Box<RunItem>), Skipped, NotFound }`; `pub(crate) fn normalize_run_path(path: &str) -> String`; `pub(crate) fn check_run_chain(chain: &[String], target: &str) -> Result<(), String>`; `pub(crate) fn find_run_target(collection: &Collection, path: &str) -> RunTarget` (`path` already normalized); `pub(crate) fn runtime_changes(seed: &HashMap<String, String>, after: &HashMap<String, String>) -> (HashMap<String, String>, Vec<String>)`. Task 3 is the first caller, so until then `cargo check` warns that they are unused; that is expected.

- [ ] **Step 1: Write the module with its failing tests**

Create `crates/rocket-app/src/execution_service/run_request.rs`:

```rust
//! Finds the target of `rok.runRequest` and guards against runaway nesting.
//!
//! Pure functions over the collection tree. A path is the request's file path
//! relative to the collection root, with folder directory names and no extension,
//! the same paths the Collection Runner uses.

use std::collections::HashMap;

use rocket_collection::{Collection, CollectionItem, Folder};

use crate::runner_sequence::{flatten_run_set, RunItem};

/// Deepest chain of nested `rok.runRequest` runs.
pub(crate) const MAX_RUN_DEPTH: usize = 5;

/// What a `rok.runRequest` path points at.
#[derive(Debug)]
pub(crate) enum RunTarget {
    /// An HTTP or GraphQL request that can run.
    Http(Box<RunItem>),
    /// A request of another protocol, which `rok.runRequest` skips.
    Skipped,
    /// Nothing at that path.
    NotFound,
}

/// Normalizes a request path: forward slashes, no outer slashes, no extension.
pub(crate) fn normalize_run_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_matches('/');
    for ext in [".yml", ".yaml", ".json", ".bru"] {
        if let Some(stem) = path.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    path.to_string()
}

/// Rejects a call that would revisit a request of the chain or nest too deep.
///
/// `chain` holds the requests already running, outermost first, so its length
/// is the nesting level the new run would have.
pub(crate) fn check_run_chain(chain: &[String], target: &str) -> Result<(), String> {
    if chain.iter().any(|path| path == target) {
        return Err(format!("rok.runRequest: recursive call to {target}"));
    }
    if chain.len() > MAX_RUN_DEPTH {
        return Err(format!(
            "rok.runRequest: nesting deeper than {MAX_RUN_DEPTH} requests"
        ));
    }
    Ok(())
}

/// Finds the item at `path`, which must already be normalized.
pub(crate) fn find_run_target(collection: &Collection, path: &str) -> RunTarget {
    if let Ok(items) = flatten_run_set(collection, None) {
        if let Some(item) = items
            .into_iter()
            .find(|item| normalize_run_path(&item.request_path) == path)
        {
            return RunTarget::Http(Box::new(item));
        }
    }
    if has_other_protocol_item(&collection.root, "", path) {
        RunTarget::Skipped
    } else {
        RunTarget::NotFound
    }
}

/// True when a WebSocket or gRPC request file sits at `path`.
fn has_other_protocol_item(folder: &Folder, prefix: &str, path: &str) -> bool {
    folder.items.iter().any(|item| {
        let file_name = match item {
            CollectionItem::WebSocket(ws) => ws.file_name.as_deref(),
            CollectionItem::Grpc(grpc) => grpc.file_name.as_deref(),
            CollectionItem::Folder(sub) => {
                let dir = sub.dir_name.as_deref().unwrap_or(&sub.name);
                return has_other_protocol_item(sub, &format!("{prefix}{dir}/"), path);
            }
            _ => None,
        };
        file_name.is_some_and(|name| normalize_run_path(&format!("{prefix}{name}")) == path)
    })
}

/// What a nested run changed in the runtime scope it was seeded with: the keys
/// it set or changed, and the keys it removed (sorted).
pub(crate) fn runtime_changes(
    seed: &HashMap<String, String>,
    after: &HashMap<String, String>,
) -> (HashMap<String, String>, Vec<String>) {
    let set = after
        .iter()
        .filter(|(key, value)| seed.get(*key) != Some(*value))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut removed: Vec<String> = seed
        .keys()
        .filter(|key| !after.contains_key(*key))
        .cloned()
        .collect();
    removed.sort();
    (set, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{Request, WebSocketRequest};
    use rocket_shared::types::HttpMethod;

    fn saved(name: &str, file: &str) -> Request {
        let mut request = Request::new(name, HttpMethod::Get, format!("https://api.test/{file}"));
        request.file_name = Some(file.to_string());
        request
    }

    /// root: [main.yml, socket.yml (WebSocket), auth/ [login.yml]]
    fn collection() -> Collection {
        let mut collection = Collection::new("api");
        collection.root.add_request(saved("Main", "main.yml"));
        let mut socket = WebSocketRequest::new("Socket", "wss://api.test/ws");
        socket.file_name = Some("socket.yml".into());
        collection
            .root
            .items
            .push(CollectionItem::WebSocket(Box::new(socket)));
        let mut auth = Folder::new("auth");
        auth.add_request(saved("Login", "login.yml"));
        collection.root.add_subfolder(auth);
        collection
    }

    #[test]
    fn normalize_run_path_accepts_the_usual_spellings() {
        assert_eq!(normalize_run_path("auth/login"), "auth/login");
        assert_eq!(normalize_run_path(" /auth/login.yml/ "), "auth/login");
        assert_eq!(normalize_run_path("auth\\login"), "auth/login");
        assert_eq!(normalize_run_path("main.bru"), "main");
    }

    #[test]
    fn check_run_chain_rejects_a_revisit() {
        let chain = vec!["main".to_string(), "auth/login".to_string()];
        assert_eq!(
            check_run_chain(&chain, "main"),
            Err("rok.runRequest: recursive call to main".to_string())
        );
        assert_eq!(check_run_chain(&chain, "other"), Ok(()));
    }

    #[test]
    fn check_run_chain_allows_five_nested_levels() {
        let five: Vec<String> = (1..=5).map(|i| format!("r{i}")).collect();
        assert_eq!(check_run_chain(&five, "r6"), Ok(()));
        let six: Vec<String> = (1..=6).map(|i| format!("r{i}")).collect();
        assert_eq!(
            check_run_chain(&six, "r7"),
            Err("rok.runRequest: nesting deeper than 5 requests".to_string())
        );
        assert_eq!(check_run_chain(&[], "r1"), Ok(()));
    }

    #[test]
    fn find_run_target_finds_http_requests_in_folders() {
        match find_run_target(&collection(), "auth/login") {
            RunTarget::Http(item) => {
                assert_eq!(item.name, "Login");
                assert_eq!(item.request_path, "auth/login.yml");
            }
            other => panic!("expected an HTTP target, got {other:?}"),
        }
        assert!(matches!(find_run_target(&collection(), "main"), RunTarget::Http(_)));
    }

    #[test]
    fn find_run_target_skips_other_protocols_and_misses_unknown_paths() {
        assert!(matches!(find_run_target(&collection(), "socket"), RunTarget::Skipped));
        assert!(matches!(find_run_target(&collection(), "auth/nope"), RunTarget::NotFound));
        assert!(matches!(find_run_target(&collection(), "login"), RunTarget::NotFound));
    }

    #[test]
    fn runtime_changes_reports_sets_and_removals() {
        let seed: HashMap<String, String> = [("keep", "1"), ("change", "a"), ("drop", "x")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let after: HashMap<String, String> = [("keep", "1"), ("change", "b"), ("new", "n")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let (set, removed) = runtime_changes(&seed, &after);
        assert_eq!(set.len(), 2);
        assert_eq!(set.get("change").map(String::as_str), Some("b"));
        assert_eq!(set.get("new").map(String::as_str), Some("n"));
        assert_eq!(removed, vec!["drop".to_string()]);
    }
}
```

In `crates/rocket-app/src/execution_service.rs`, add after `pub(crate) mod script_host;`:

```rust
pub(crate) mod run_request;
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -j4 -p rocket-app execution_service::run_request`
Expected: PASS (6 tests). The tests are written against the final code in Step 1; to see them fail first, run them once with `find_run_target`'s body replaced by `RunTarget::NotFound`, then restore it.

- [ ] **Step 3: Commit**

Run `cargo check -j4` (dead-code warnings for the new helpers are expected until Task 3). Then skill `dev-workflow-skills:1-git-commit`, pathspec commit of `crates/rocket-app/src/execution_service/run_request.rs` and `crates/rocket-app/src/execution_service.rs`.
Suggested subject: `feat(app): find rok.runRequest targets and guard the call chain`.

---

### Task 3: The host side and end-to-end tests

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`PhaseState`, `begin_phases`, `script_host`, the three `self.script_host(state)` calls, new `execute_nested`)
- Modify: `crates/rocket-app/src/execution_service/script_host.rs` (new fields, `run_request`)
- Modify: `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Consumes: Task 1 (`HostRunRequest`, `HostRunOutcome`, `HostScopes`), Task 2 (all helpers), plan 03 (`ExecutionScriptHost`, `host_response`, `KeepingExecutor`, `SharedKeeping`).
- Produces: `PhaseState.external_secrets: Arc<HashMap<String, String>>`, `PhaseState.run_chain: Vec<String>`; `ExecutionScriptHost { svc, input: &'a ExecuteRequestInput, external_secrets: Arc<HashMap<String, String>>, options, chain: Vec<String> }`; `RequestExecutionService::script_host(&self, input, state)`; `pub(crate) async fn execute_nested(&self, input: ExecuteRequestInput, external_secrets: &HashMap<String, String>, chain: Vec<String>, runtime: &HashMap<String, String>) -> DomainResult<(ExecuteRequestOutput, HashMap<String, String>)>`.

- [ ] **Step 1: Write the failing end-to-end tests**

Add to the end of `crates/rocket-app/src/execution_service/script_host_tests.rs`:

```rust
// ── rok.runRequest ───────────────────────────────────────────────────────────

use rocket_collection::{CollectionItem, Folder, Request, WebSocketRequest};
use rocket_environment::EnvironmentRepository;

use super::{ExecuteRequestInput, ExecuteRequestOutput};

/// Environment repo that keeps one environment in memory and saves into it.
struct MemoryEnvRepo(Arc<Mutex<Environment>>);

impl EnvironmentRepository for MemoryEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        Ok(vec![self.0.lock().expect("lock").clone()])
    }
    fn get(&self, _name: &str) -> DomainResult<Environment> {
        Ok(self.0.lock().expect("lock").clone())
    }
    fn save(&self, env: &Environment) -> DomainResult<()> {
        *self.0.lock().expect("lock") = env.clone();
        Ok(())
    }
    fn delete(&self, _name: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// A saved GET request whose URL ends in its file name.
fn saved(name: &str, file: &str, pre_request: &str) -> Request {
    let mut request = Request::new(name, HttpMethod::Get, format!("https://api.test/{file}"));
    request.file_name = Some(file.to_string());
    if !pre_request.is_empty() {
        request.pre_request_script = Some(pre_request.to_string());
    }
    request
}

/// A service over `collection` with the real engine and environment `dev` (`E` = `old`).
fn run_service(collection: Collection) -> (RequestExecutionService, Arc<KeepingExecutor>) {
    let executor = Arc::new(KeepingExecutor::default());
    let mut env = Environment::new("dev");
    env.set_variable(Variable::new("E", "old"));
    let svc = RequestExecutionService::new(
        Box::new(MemoryEnvRepo(Arc::new(Mutex::new(env)))),
        Arc::new(SharedKeeping(Arc::clone(&executor))),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(collection))),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()));
    (svc, executor)
}

/// The input a single send of the request at `path` gets, in environment `dev`.
fn send_input(collection: &Collection, path: &str) -> ExecuteRequestInput {
    let item = crate::runner_sequence::flatten_run_set(collection, None)
        .expect("run set")
        .into_iter()
        .find(|item| item.request_path == path)
        .expect("request in the collection");
    crate::runner_sequence::build_step_input(&item, "api", Some("dev"), None, Default::default())
}

fn console(out: &ExecuteRequestOutput) -> Vec<String> {
    out.console_entries.iter().map(|e| e.message.clone()).collect()
}

fn urls(executor: &KeepingExecutor) -> Vec<String> {
    executor.seen().into_iter().map(|r| r.url).collect()
}

/// root: [main.yml with `main_script`], auth/ [login.yml with `login_pre`, `login_post`]
fn main_and_login(main_script: &str, login_pre: &str, login_post: &str) -> Collection {
    let mut collection = Collection::new("api");
    collection.root.add_request(saved("Main", "main.yml", main_script));
    let mut login = saved("Login", "login.yml", login_pre);
    if !login_post.is_empty() {
        login.post_response_script = Some(login_post.to_string());
    }
    let mut auth = Folder::new("auth");
    auth.add_request(login);
    collection.root.add_subfolder(auth);
    collection
}

#[tokio::test]
async fn run_request_e2e_runs_the_saved_request() {
    let collection = main_and_login(
        "const r = await rok.runRequest('auth/login'); console.log('login', r.status, r.data.ok);",
        "",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    assert_eq!(
        urls(&executor),
        vec![
            "https://api.test/login.yml".to_string(),
            "https://api.test/main.yml".to_string()
        ]
    );
    let lines = console(&out);
    assert!(lines.contains(&"login 200 true".to_string()), "{lines:?}");
    assert!(
        lines.contains(&"rok.runRequest auth/login -> 200".to_string()),
        "{lines:?}"
    );
}

#[tokio::test]
async fn run_request_e2e_nested_runtime_writes_reach_the_caller_and_later_phases() {
    let mut collection = main_and_login(
        "rok.setVar('token', 'old'); await rok.runRequest('auth/login'); \
         console.log('now', rok.getVar('token'));",
        "",
        "rok.setVar('token', 'from-login');",
    );
    if let Some(CollectionItem::Request(main)) = collection.root.items.first_mut() {
        main.tests = Some("console.log('later', rok.getVar('token'));".into());
    }
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    let lines = console(&out);
    assert!(lines.contains(&"now from-login".to_string()), "{lines:?}");
    assert!(lines.contains(&"later from-login".to_string()), "{lines:?}");
}

#[tokio::test]
async fn run_request_e2e_the_nested_run_sees_the_callers_runtime_vars() {
    let collection = main_and_login(
        "rok.setVar('who', 'main'); await rok.runRequest('auth/login'); \
         console.log('seen', rok.getVar('seen'));",
        "rok.setVar('seen', rok.getVar('who'));",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(console(&out).contains(&"seen main".to_string()), "{:?}", console(&out));
}

#[tokio::test]
async fn run_request_e2e_nested_env_writes_are_visible_after_the_call() {
    let collection = main_and_login(
        "await rok.runRequest('auth/login'); console.log('E', rok.getEnvVar('E'));",
        "rok.setEnvVar('E', 'new');",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(console(&out).contains(&"E new".to_string()), "{:?}", console(&out));
}

#[tokio::test]
async fn run_request_e2e_other_protocols_are_skipped() {
    let mut collection = main_and_login(
        "const r = await rok.runRequest('socket'); console.log('ws', r.status);",
        "",
        "",
    );
    let mut socket = WebSocketRequest::new("Socket", "wss://api.test/ws");
    socket.file_name = Some("socket.yml".into());
    collection
        .root
        .items
        .push(CollectionItem::WebSocket(Box::new(socket)));
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(console(&out).contains(&"ws skipped".to_string()), "{:?}", console(&out));
    assert_eq!(urls(&executor), vec!["https://api.test/main.yml".to_string()]);
}

#[tokio::test]
async fn run_request_e2e_an_unknown_path_rejects() {
    let collection = main_and_login(
        "try { await rok.runRequest('nope/missing'); } catch (e) { console.log('err', e.message); }",
        "",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, _executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"err rok.runRequest: invalid request path - nope/missing".to_string()),
        "{:?}",
        console(&out)
    );
}

#[tokio::test]
async fn run_request_e2e_a_self_call_rejects() {
    let collection = main_and_login(
        "try { await rok.runRequest('main'); } catch (e) { console.log('err', e.message); }",
        "",
        "",
    );
    let input = send_input(&collection, "main.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"err rok.runRequest: recursive call to main".to_string()),
        "{:?}",
        console(&out)
    );
    assert_eq!(urls(&executor), vec!["https://api.test/main.yml".to_string()]);
}

#[tokio::test]
async fn run_request_e2e_a_cycle_rejects_inside_the_nested_run() {
    let mut collection = Collection::new("api");
    collection.root.add_request(saved(
        "A",
        "a.yml",
        "await rok.runRequest('b'); console.log('cycle', rok.getVar('cycle'));",
    ));
    collection.root.add_request(saved(
        "B",
        "b.yml",
        "try { await rok.runRequest('a'); } catch (e) { rok.setVar('cycle', e.message); }",
    ));
    let input = send_input(&collection, "a.yml");
    let (svc, executor) = run_service(collection);
    let out = svc.execute(input).await.expect("execute");
    assert!(
        console(&out).contains(&"cycle rok.runRequest: recursive call to a".to_string()),
        "{:?}",
        console(&out)
    );
    assert_eq!(
        urls(&executor),
        vec!["https://api.test/b.yml".to_string(), "https://api.test/a.yml".to_string()]
    );
}

#[tokio::test]
async fn run_request_e2e_nesting_stops_after_five_levels() {
    let mut collection = Collection::new("api");
    for i in 1..=7 {
        let script = if i < 7 {
            format!("await rok.runRequest('r{}');", i + 1)
        } else {
            String::new()
        };
        collection
            .root
            .add_request(saved(&format!("R{i}"), &format!("r{i}.yml"), &script));
    }
    let input = send_input(&collection, "r1.yml");
    let (svc, executor) = run_service(collection);
    svc.execute(input).await.expect("execute");
    let sent = urls(&executor);
    assert_eq!(sent.len(), 6, "{sent:?}");
    assert!(sent.iter().all(|url| !url.ends_with("r7.yml")), "{sent:?}");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app run_request_e2e_`
Expected: FAIL. Every call rejects with `rok.runRequest is not available here`, because `ExecutionScriptHost` does not implement `run_request` yet (the trait default answers `Unavailable`).

- [ ] **Step 3: Carry the secrets and the chain in `PhaseState`**

In `crates/rocket-app/src/execution_service.rs`:

1. Add these fields at the end of `pub(crate) struct PhaseState`:

```rust
    /// RocketVault values of this run, shared with the script host for nested runs.
    pub external_secrets: Arc<std::collections::HashMap<String, String>>,
    /// Request paths of this run and the runs that started it, outermost first.
    /// `rok.runRequest` uses it to stop recursion.
    pub run_chain: Vec<String>,
```

2. In `begin_phases`, add to the `Ok(PhaseState { ... })` literal after `response_body_override: None,`:

```rust
            external_secrets: Arc::new(external_secrets.clone()),
            run_chain: input
                .request_path
                .as_deref()
                .map(run_request::normalize_run_path)
                .into_iter()
                .collect(),
```

3. Replace the `script_host` method with:

```rust
    /// The host for one script run of a request. Script requests reuse the
    /// request's TLS, redirect, cookie and client-certificate options, and
    /// nested runs reuse its collection, environments and RocketVault values.
    fn script_host<'a>(
        &'a self,
        input: &'a ExecuteRequestInput,
        state: &PhaseState,
    ) -> script_host::ExecutionScriptHost<'a> {
        script_host::ExecutionScriptHost {
            svc: self,
            input,
            external_secrets: Arc::clone(&state.external_secrets),
            options: state.http_request.options.clone(),
            chain: state.run_chain.clone(),
        }
    }
```

and change each of the three `let host = self.script_host(state);` lines to `let host = self.script_host(input, state);`.

4. Add this method directly after `execute_capturing`:

```rust
    /// Runs a saved request for `rok.runRequest` with every phase, as a single send does.
    ///
    /// `chain` is the call chain the nested run belongs to, its own path last, and
    /// `runtime` seeds its runtime variables. Returns the output and the runtime
    /// variables the run ended with.
    pub(crate) async fn execute_nested(
        &self,
        input: ExecuteRequestInput,
        external_secrets: &std::collections::HashMap<String, String>,
        chain: Vec<String>,
        runtime: &std::collections::HashMap<String, String>,
    ) -> DomainResult<(ExecuteRequestOutput, std::collections::HashMap<String, String>)> {
        let mut state = self.begin_phases(&input, external_secrets)?;
        state.run_chain = chain;
        state.seed_runtime(runtime);
        self.run_before_request_phase(&input, ExecutionMode::Standalone, &mut state)
            .await?;
        let response = self.send_request(&state).await?;
        self.run_after_response_phase(&input, ExecutionMode::Standalone, &response, &mut state)
            .await;
        self.run_tests_phase(&input, ExecutionMode::Standalone, &response, &mut state)
            .await;
        let output = self.finish_phases(&input, response, &mut state).await;
        Ok((output, state.var_ctx.runtime.clone()))
    }
```

- [ ] **Step 4: Implement `run_request` on the host**

In `crates/rocket-app/src/execution_service/script_host.rs`:

1. Replace the imports with:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rocket_http::{HttpRequest, HttpResponse, RequestOptions};
use rocket_scripting::{
    HostError, HostRequest, HostResponse, HostRunOutcome, HostRunRequest, HostScopes, ScriptHost,
};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};

use super::run_request::{
    check_run_chain, find_run_target, normalize_run_path, runtime_changes, RunTarget,
};
use super::{body_mode_from_content_type, ExecuteRequestInput, RequestExecutionService};
use crate::runner_sequence::build_step_input;
```

2. Replace the struct with:

```rust
/// Serves the host calls of one script run.
pub(crate) struct ExecutionScriptHost<'a> {
    pub(crate) svc: &'a RequestExecutionService,
    /// The request the script belongs to. Nested runs take its collection and environments.
    pub(crate) input: &'a ExecuteRequestInput,
    /// RocketVault values of this run, passed on to nested runs.
    pub(crate) external_secrets: Arc<HashMap<String, String>>,
    /// TLS, redirect, cookie and client-certificate options of the request the
    /// script belongs to. Script requests reuse them.
    pub(crate) options: RequestOptions,
    /// Request paths of this run and the runs that started it, outermost first.
    pub(crate) chain: Vec<String>,
}
```

3. Add this method to `impl ScriptHost for ExecutionScriptHost<'_>`, after `send_request`:

```rust
    async fn run_request(&self, request: HostRunRequest) -> Result<HostRunOutcome, HostError> {
        let target = normalize_run_path(&request.path);
        check_run_chain(&self.chain, &target).map_err(HostError::Failed)?;
        let invalid = || {
            HostError::Failed(format!(
                "rok.runRequest: invalid request path - {}",
                request.path
            ))
        };
        let collection = self.input.collection.as_deref().ok_or_else(invalid)?;
        let tree = self
            .svc
            .collection_repo
            .get(collection)
            .map_err(|_| invalid())?;
        let item = match find_run_target(&tree, &target) {
            RunTarget::Http(item) => item,
            RunTarget::Skipped => return Ok(HostRunOutcome::default()),
            RunTarget::NotFound => return Err(invalid()),
        };
        if let Some(message) = &item.prepare_error {
            return Err(HostError::Failed(format!("rok.runRequest: {message}")));
        }
        let nested = build_step_input(
            &item,
            collection,
            self.input.environment_name.as_deref(),
            self.input.global_env_name.as_deref(),
            self.input.request_guard_policy.clone(),
        );
        let mut chain = self.chain.clone();
        chain.push(target);
        // Boxed, because this future holds another run of the same pipeline.
        let (output, runtime) = Box::pin(self.svc.execute_nested(
            nested,
            &self.external_secrets,
            chain,
            &request.runtime_vars,
        ))
        .await
        .map_err(|e| HostError::Failed(format!("rok.runRequest: {e}")))?;
        // The nested run saved its writes, so the scopes are read back from storage.
        let scopes = self.svc.build_variable_scopes(
            self.input.global_env_name.as_deref(),
            Some(collection),
            self.input.environment_name.as_deref(),
            None,
            &self.external_secrets,
        );
        let (runtime_set, runtime_removed) = runtime_changes(&request.runtime_vars, &runtime);
        Ok(HostRunOutcome {
            response: Some(host_response(&output.response)),
            runtime_set,
            runtime_removed,
            scopes: Some(HostScopes {
                env: scopes.env,
                global_env: scopes.global_env,
                collection: scopes.collection,
                secret_values: scopes.secret_values.into_iter().collect(),
            }),
        })
    }
```

- [ ] **Step 5: Run the end-to-end tests**

Run: `cargo test -j4 -p rocket-app run_request_e2e_`
Expected: PASS (9 tests). If the compiler reports that the `run_request` future is not `Send` or that a cycle was detected computing an opaque type, keep the `Box::pin` and also box the nested call site in `execute_nested` with `Box::pin(self.run_before_request_phase(...)).await?`; log a `Ruling:` line.

- [ ] **Step 6: Run all checks**

Run: `cargo check -j4 && cargo test -j4 -p rocket-app execution_service && cargo test -j4 -p rocket-app collection_runner_service && cargo test -j4 -p rocket-app flow_execution_service && cargo test -j4 -p rocket-infra scripting`
Expected: all PASS, and no dead-code warnings left from Task 2.

- [ ] **Step 7: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/execution_service/script_host.rs`, `crates/rocket-app/src/execution_service/script_host_tests.rs`.
Suggested subject: `feat(app): run saved requests for rok.runRequest`.

## Manual check (real app)

Run `yarn tauri dev`. In a collection with a `login` request whose post-response script sets `rok.setVar('token', res.body.token)`, give another request the pre-request script `await rok.runRequest('login'); req.setHeader('Authorization', 'Bearer ' + rok.getVar('token'))` and send it. The Console shows `rok.runRequest login -> 200`, History has both requests, and the header carries the token. Then make `login`'s own pre-request script call `rok.runRequest('login')` and confirm the recursion error appears for the nested run while the outer request still completes.

---

## Next plan to execute

When Task 3 is complete, its checks pass, the manual check is done or handed to the user, and the ledger (`.superpowers/sdd/rok-parity-b-04-run-request/progress.md`) shows "Task 3: complete", **the executing Claude must go straight on to plan 05**: `docs/superpowers/plans/rok-parity-b/05-snippets-docs-and-manual-check.md`. No consent is needed between plans. Run one plan at a time, and swap the visible task list to plan 05's tasks when it starts.

Plan 05 depends on this plan: its snippets, docs and manual check cover `runRequest`. Do not start it on a tree where this plan's checks fail.
