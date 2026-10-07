# rok parity A, plan 01: variable reads, deletes and read-your-writes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the missing variable read and delete APIs to `rok`, apply deletes in the app layer, and make reads see a script's own earlier writes.

**Architecture:** Each API is a wrapper in `bootstrap.js` calling an op in `ops/rok.rs`, registered in the `rocket_scripting_ext` extension in `engine.rs`. Reads come from the `ScriptInputState` snapshot. Deletes are expressed with the existing null-value write convention (`EnvVarWrite.value == null`, `CollectionVarWrite.value == null`), plus one new `ScriptResult.runtime_var_deletes` list for the runtime scope. `rocket-app` applies them after the engine returns. A JS-side overlay makes reads consistent with earlier writes in the same script.

**Tech Stack:** Rust, `deno_core` ops (`#[op2]`), Vitest, Monaco type definitions.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-a-sync-design.md`. Index with rulings: `00-plan-index.md`.

## Global Constraints

- `cargo` commands always pass `-j4`. No `cargo test --workspace`.
- Persistence: new deletes persist exactly like existing writes (env, global and collection writes always persist). `setVar` and `deleteVar` are runtime-only.
- New getters return `""` for a missing key. `getProcessEnv` returns `undefined`.
- `rok-types.ts` and `bootstrap.js` stay in sync (a test enforces it).
- Comments are short full sentences ending in a period.

## Review Focus

- `deleteAllEnvVars()` on an empty environment: no writes, no throw.
- `deleteVar('k')` then `setVar('k', 1)` in one script: the key ends up set, not deleted.
- `setVar('a', 1)` then `getVar('a')` in one script returns `1`, and `getAllVars()` includes it.
- `setVar('n', 0)` (a number) survives into the next script phase as `"0"`, not silently dropped.
- `deleteAllEnvVars()` after `setEnvVar('only_in_script', 'x')` also removes the key the script itself created.

---

### Task 1: Read ops and sync test

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (registration list near `rok::op_rok_skip_request`, tests module near `rok_get_env_var_reads_from_context`)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js` (the `globalThis.rok = {` block)
- Modify: `src/components/editor/rok-types.ts` (`ROK_DEFS`)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (tests module), `src/components/editor/__tests__/rok-types.test.ts`

**Interfaces:**
- Produces (JS): `rok.getAllEnvVars()`, `rok.getAllVars()`, `rok.getAllGlobalEnvVars()` returning `Record<string, string>`; `rok.hasVar(key)`, `rok.hasGlobalEnvVar(key)`, `rok.hasCollectionVar(key)` returning `boolean`; `rok.getRequestVar(key)` returning `string`; `rok.getProcessEnv(key)` returning `string | undefined`; `rok.setNextRequest(name | null)`.
- Produces (Rust ops): `op_rok_get_all_env_vars`, `op_rok_get_all_vars`, `op_rok_get_all_global_env_vars` (all `#[string]` JSON objects); `op_rok_has_var`, `op_rok_has_global_env_var`, `op_rok_has_collection_var`, `op_rok_has_process_env` (bool); `op_rok_get_request_var`, `op_rok_get_process_env` (string).

- [ ] **Step 1: Write the failing engine tests**

Add to the `tests` module in `crates/rocket-infra/src/scripting/engine.rs`, after `rok_get_env_var_reads_from_context`:

```rust
    #[tokio::test]
    async fn rok_get_all_env_vars_returns_the_env_scope() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("A".into(), "1".into());
        vars.env.insert("B".into(), "2".into());
        let mut ctx = minimal_ctx(
            "rok.setVar('keys', Object.keys(rok.getAllEnvVars()).sort().join(','))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("keys").expect("keys present"), "A,B");
    }

    #[tokio::test]
    async fn rok_get_all_vars_and_global_vars_read_their_scopes() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("r".into(), "runtime".into());
        vars.global_env.insert("g".into(), "global".into());
        let mut ctx = minimal_ctx(
            "rok.setVar('out', rok.getAllVars().r + '|' + rok.getAllGlobalEnvVars().g)",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "runtime|global"
        );
    }

    #[tokio::test]
    async fn rok_has_checks_report_presence_per_scope() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("r".into(), "1".into());
        vars.global_env.insert("g".into(), "1".into());
        vars.collection.insert("c".into(), "1".into());
        let mut ctx = minimal_ctx(
            "rok.setVar('out', [rok.hasVar('r'), rok.hasVar('x'), rok.hasGlobalEnvVar('g'), \
             rok.hasGlobalEnvVar('x'), rok.hasCollectionVar('c'), rok.hasCollectionVar('x')].join(','))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "true,false,true,false,true,false"
        );
    }

    #[tokio::test]
    async fn rok_get_request_var_reads_the_request_scope() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.request.insert("source".into(), "warehouse-a".into());
        let mut ctx = minimal_ctx("rok.setVar('v', rok.getRequestVar('source'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("v").expect("v present"), "warehouse-a");
    }

    #[tokio::test]
    async fn rok_get_process_env_returns_value_or_undefined() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.process_env.insert("HOME_DIR".into(), "/home/me".into());
        let mut ctx = minimal_ctx(
            "rok.setVar('out', rok.getProcessEnv('HOME_DIR') + '|' + String(rok.getProcessEnv('NOPE')))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "/home/me|undefined"
        );
    }

    #[tokio::test]
    async fn rok_set_next_request_alias_matches_runner_form() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setNextRequest('Poll Status')");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(matches!(
            result.next_request,
            Some(rocket_scripting::NextRequest::Name(ref n)) if n == "Poll Status"
        ));

        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setNextRequest(null)");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(matches!(
            result.next_request,
            Some(rocket_scripting::NextRequest::Stop)
        ));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra rok_get_all rok_has_checks rok_get_request_var rok_get_process_env rok_set_next_request_alias`
Expected: FAIL with `rok.getAllEnvVars is not a function` style script errors (the `runtime_vars.get(..).expect(..)` panics because the script threw).

- [ ] **Step 3: Add the ops**

In `crates/rocket-infra/src/scripting/ops/rok.rs`, add `use std::collections::HashMap;` below the existing `use` lines, then add after `op_rok_get_global_env_var`:

```rust
fn scope_json(map: &HashMap<String, String>) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".into())
}

/// rok.getAllEnvVars() — every variable of the active environment as a JSON object.
#[op2]
#[string]
pub fn op_rok_get_all_env_vars(state: &OpState) -> String {
    scope_json(&state.borrow::<ScriptInputState>().variables.env)
}

/// rok.getAllVars() — every runtime variable as a JSON object.
#[op2]
#[string]
pub fn op_rok_get_all_vars(state: &OpState) -> String {
    scope_json(&state.borrow::<ScriptInputState>().variables.runtime)
}

/// rok.getAllGlobalEnvVars() — every global environment variable as a JSON object.
#[op2]
#[string]
pub fn op_rok_get_all_global_env_vars(state: &OpState) -> String {
    scope_json(&state.borrow::<ScriptInputState>().variables.global_env)
}

/// rok.hasVar(key) — true if the runtime scope holds key.
#[op2(fast)]
pub fn op_rok_has_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .runtime
        .contains_key(&key)
}

/// rok.hasGlobalEnvVar(key) — true if the global environment holds key.
#[op2(fast)]
pub fn op_rok_has_global_env_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .global_env
        .contains_key(&key)
}

/// rok.hasCollectionVar(key) — true if the collection scope holds key.
#[op2(fast)]
pub fn op_rok_has_collection_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .collection
        .contains_key(&key)
}

/// rok.getRequestVar(key) — reads from the request variable scope.
#[op2]
#[string]
pub fn op_rok_get_request_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .request
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// True if the host environment snapshot holds key.
#[op2(fast)]
pub fn op_rok_has_process_env(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .process_env
        .contains_key(&key)
}

/// rok.getProcessEnv(key) — reads the host environment snapshot.
#[op2]
#[string]
pub fn op_rok_get_process_env(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .process_env
        .get(&key)
        .cloned()
        .unwrap_or_default()
}
```

- [ ] **Step 4: Register the ops**

In `crates/rocket-infra/src/scripting/engine.rs`, in the `extension!(rocket_scripting_ext, ops = [ ... ])` list, add after `rok::op_rok_skip_request,`:

```rust
        rok::op_rok_get_all_env_vars,
        rok::op_rok_get_all_vars,
        rok::op_rok_get_all_global_env_vars,
        rok::op_rok_has_var,
        rok::op_rok_has_global_env_var,
        rok::op_rok_has_collection_var,
        rok::op_rok_get_request_var,
        rok::op_rok_has_process_env,
        rok::op_rok_get_process_env,
```

- [ ] **Step 5: Add the JS wrappers**

In `crates/rocket-infra/src/scripting/bootstrap.js`, in the `globalThis.rok = {` block, add before the `runner:` key, and replace the `runner` block so both forms share one function:

```js
    getAllEnvVars:       ()    => JSON.parse(__ops.op_rok_get_all_env_vars()),
    getAllVars:          ()    => JSON.parse(__ops.op_rok_get_all_vars()),
    getAllGlobalEnvVars: ()    => JSON.parse(__ops.op_rok_get_all_global_env_vars()),
    hasVar:              (key) => __ops.op_rok_has_var(key),
    hasGlobalEnvVar:     (key) => __ops.op_rok_has_global_env_var(key),
    hasCollectionVar:    (key) => __ops.op_rok_has_collection_var(key),
    getRequestVar:       (key) => __ops.op_rok_get_request_var(key),
    getProcessEnv:       (key) => (__ops.op_rok_has_process_env(key) ? __ops.op_rok_get_process_env(key) : undefined),
    setNextRequest:      (name) => __ops.op_rok_set_next_request(name == null ? "" : String(name)),
    runner: {
      setNextRequest: (name)  => __ops.op_rok_set_next_request(name == null ? "" : String(name)),
      skipRequest:    ()      => __ops.op_rok_skip_request(),
    },
```

- [ ] **Step 6: Run the engine tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra rok_get_all rok_has_checks rok_get_request_var rok_get_process_env rok_set_next_request_alias`
Expected: PASS (6 tests).

- [ ] **Step 7: Write the failing sync test**

Add to `src/components/editor/__tests__/rok-types.test.ts` (keep the file's existing imports and add these):

```ts
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

describe('rok typings stay in sync with the runtime', () => {
  it('declares every top-level rok method defined in bootstrap.js', () => {
    const bootstrap = readFileSync(
      join(process.cwd(), 'crates/rocket-infra/src/scripting/bootstrap.js'),
      'utf8',
    );
    const start = bootstrap.indexOf('globalThis.rok = {');
    expect(start).toBeGreaterThan(-1);
    const end = bootstrap.indexOf('\n  };', start);
    const block = bootstrap.slice(start, end);
    const names = [...block.matchAll(/^ {4}(\w+):/gm)]
      .map((m) => m[1])
      .filter((n) => n !== 'runner');
    const defs = ROK_TYPE_DEFS_FOR_PHASE('tests');
    const missing = names.filter((n) => !defs.includes(`${n}(`));
    expect(missing).toEqual([]);
  });
});
```

If `ROK_TYPE_DEFS_FOR_PHASE` is not already imported in that file, add `import { ROK_TYPE_DEFS_FOR_PHASE } from '../rok-types';`.

- [ ] **Step 8: Run it to verify it fails**

Run: `yarn test rok-types`
Expected: FAIL, `missing` lists `getAllEnvVars`, `getAllVars`, `getAllGlobalEnvVars`, `hasVar`, `hasGlobalEnvVar`, `hasCollectionVar`, `getRequestVar`, `getProcessEnv`, `setNextRequest`.

- [ ] **Step 9: Add the typings**

In `ROK_DEFS` in `src/components/editor/rok-types.ts`, add before the `runner:` doc comment block:

```ts
  /** Every variable of the active environment. */
  getAllEnvVars(): Record<string, string>;
  /** Every runtime variable. */
  getAllVars(): Record<string, string>;
  /** Every global environment variable. */
  getAllGlobalEnvVars(): Record<string, string>;
  /** Returns true if the runtime variable exists. */
  hasVar(key: string): boolean;
  /** Returns true if the global environment variable exists. */
  hasGlobalEnvVar(key: string): boolean;
  /** Returns true if the collection variable exists. */
  hasCollectionVar(key: string): boolean;
  /** Read a request variable. */
  getRequestVar(key: string): string;
  /** Read a host environment variable, or undefined when it is not set. */
  getProcessEnv(key: string): string | undefined;
  /** Same as rok.runner.setNextRequest. Pass null to stop the run. */
  setNextRequest(name: string | null): void;
```

- [ ] **Step 10: Run all checks**

Run: `yarn test rok-types && yarn tsc --noEmit && yarn check && cargo check -j4`
Expected: all PASS.

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of exactly:
`crates/rocket-infra/src/scripting/ops/rok.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `src/components/editor/rok-types.ts`, `src/components/editor/__tests__/rok-types.test.ts`.
Suggested subject: `feat(scripting): add rok variable read APIs`.

---

### Task 2: Delete ops and their application

**Files:**
- Modify: `crates/rocket-scripting/src/result.rs` (`ScriptResult`)
- Modify: `crates/rocket-infra/src/scripting/state.rs` (`ScriptOutputState`)
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (registration, `ScriptResult` construction in `run_script`, tests)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js`
- Modify: `crates/rocket-app/src/execution_service.rs` (`apply_script_side_effects`, helpers, tests)
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Consumes: Task 1 (`rok` block layout).
- Produces (Rust): `ScriptResult.runtime_var_deletes: Vec<String>` (`#[serde(default)]`); `ScriptOutputState.runtime_var_deletes: Vec<String>`; ops `op_rok_delete_var`, `op_rok_delete_all_vars`, `op_rok_delete_all_env_vars`, `op_rok_delete_collection_var`, `op_rok_delete_all_collection_vars`, `op_rok_delete_global_env_var`, `op_rok_delete_all_global_env_vars`; in `rocket-app`: `fn merge_runtime_vars(var_ctx: &mut VariableContext, result: &ScriptResult)`, `fn remove_variable(vars: &mut Vec<rocket_collection::CollectionVariable>, key: &str) -> bool`, and `RequestExecutionService::apply_collection_var_delete(&self, collection: &str, key: &str) -> DomainResult<()>`.
- Produces (JS): `rok.deleteVar`, `deleteAllVars`, `deleteAllEnvVars`, `deleteCollectionVar`, `deleteAllCollectionVars`, `deleteGlobalEnvVar`, `deleteAllGlobalEnvVars`.

- [ ] **Step 1: Write the failing engine tests**

Add to the `tests` module in `engine.rs`:

```rust
    #[tokio::test]
    async fn rok_delete_var_records_a_runtime_delete() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("gone".into(), "1".into());
        let mut ctx = minimal_ctx("rok.deleteVar('gone')");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_var_deletes, vec!["gone".to_string()]);
    }

    #[tokio::test]
    async fn rok_set_after_delete_keeps_the_key_set() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("k".into(), "old".into());
        let mut ctx = minimal_ctx("rok.deleteVar('k'); rok.setVar('k', 'new')");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("k").expect("k present"), "new");
        assert!(!result.runtime_var_deletes.contains(&"k".to_string()));
    }

    #[tokio::test]
    async fn rok_delete_all_vars_deletes_every_snapshot_key() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("a".into(), "1".into());
        vars.runtime.insert("b".into(), "2".into());
        let mut ctx = minimal_ctx("rok.deleteAllVars()");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let mut deleted = result.runtime_var_deletes.clone();
        deleted.sort();
        assert_eq!(deleted, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn rok_delete_all_env_vars_writes_null_for_every_key() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("A".into(), "1".into());
        vars.env.insert("B".into(), "2".into());
        let mut ctx = minimal_ctx("rok.deleteAllEnvVars()");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let mut keys: Vec<_> = result
            .env_var_writes
            .iter()
            .filter(|w| w.value.is_null())
            .map(|w| w.key.clone())
            .collect();
        keys.sort();
        assert_eq!(keys, vec!["A".to_string(), "B".to_string()]);
    }

    #[tokio::test]
    async fn rok_delete_all_env_vars_on_empty_env_writes_nothing() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.deleteAllEnvVars()");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.env_var_writes.is_empty());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn rok_delete_all_env_vars_also_removes_keys_the_script_created() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setEnvVar('only_in_script', 'x'); rok.deleteAllEnvVars()");
        let result = engine.execute(ctx).await.expect("execute");
        let last = result.env_var_writes.last().expect("a write");
        assert_eq!(last.key, "only_in_script");
        assert!(last.value.is_null());
    }

    #[tokio::test]
    async fn rok_collection_and_global_deletes_write_null() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.collection.insert("c1".into(), "1".into());
        vars.collection.insert("c2".into(), "2".into());
        vars.global_env.insert("g1".into(), "1".into());
        let mut ctx = minimal_ctx(
            "rok.deleteCollectionVar('c1'); rok.deleteAllCollectionVars(); \
             rok.deleteGlobalEnvVar('g1'); rok.deleteAllGlobalEnvVars()",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.collection_var_writes.iter().all(|w| w.value.is_null()));
        assert!(result.collection_var_writes.iter().any(|w| w.key == "c2"));
        assert!(result.global_env_var_writes.iter().all(|w| w.value.is_null()));
        assert!(result.global_env_var_writes.iter().any(|w| w.key == "g1"));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra rok_delete rok_set_after_delete rok_collection_and_global`
Expected: FAIL to compile (`no field runtime_var_deletes`).

- [ ] **Step 3: Add the new result and state fields**

In `crates/rocket-scripting/src/result.rs`, add to `ScriptResult` after `runtime_vars`:

```rust
    /// Runtime variable keys removed via `rok.deleteVar` or `rok.deleteAllVars`.
    /// Applied after `runtime_vars` is merged.
    #[serde(default)]
    pub runtime_var_deletes: Vec<String>,
```

In `crates/rocket-infra/src/scripting/state.rs`, add to `ScriptOutputState` after `runtime_vars`:

```rust
    pub runtime_var_deletes: Vec<String>,
```

In `run_script` in `engine.rs`, add `runtime_var_deletes: out.runtime_var_deletes,` to the `ScriptResult { ... }` literal after `runtime_vars: out.runtime_vars,`.

- [ ] **Step 4: Add the delete ops**

In `ops/rok.rs`, add `use std::collections::BTreeSet;` and the imports `use rocket_scripting::{CollectionVarWrite, EnvVarWrite, NextRequest};` already exist. Change `op_rok_set_var` so a set clears a pending delete:

```rust
/// rok.setVar(key, jsonValue) — writes to runtime scope (in-memory only).
#[op2(fast)]
pub fn op_rok_set_var(state: &mut OpState, #[string] key: String, #[string] json_value: String) {
    let value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    let out = state.borrow_mut::<ScriptOutputState>();
    out.runtime_var_deletes.retain(|k| k != &key);
    out.runtime_vars.insert(key, value);
}
```

Then add the delete ops after `op_rok_set_global_env_var`:

```rust
/// Keys of a snapshot scope plus keys a script already wrote, in stable order.
fn scope_keys<'a>(
    snapshot: &HashMap<String, String>,
    written: impl Iterator<Item = &'a String>,
) -> BTreeSet<String> {
    snapshot.keys().cloned().chain(written.cloned()).collect()
}

/// rok.deleteVar(key) — removes a runtime variable.
#[op2(fast)]
pub fn op_rok_delete_var(state: &mut OpState, #[string] key: String) {
    let out = state.borrow_mut::<ScriptOutputState>();
    out.runtime_vars.remove(&key);
    out.runtime_var_deletes.push(key);
}

/// rok.deleteAllVars() — removes every runtime variable.
#[op2(fast)]
pub fn op_rok_delete_all_vars(state: &mut OpState) {
    let snapshot: Vec<String> = state
        .borrow::<ScriptInputState>()
        .variables
        .runtime
        .keys()
        .cloned()
        .collect();
    let out = state.borrow_mut::<ScriptOutputState>();
    let written: Vec<String> = out.runtime_vars.keys().cloned().collect();
    out.runtime_vars.clear();
    out.runtime_var_deletes.extend(snapshot);
    out.runtime_var_deletes.extend(written);
}

/// rok.deleteAllEnvVars() — null-writes every key of the active environment.
#[op2(fast)]
pub fn op_rok_delete_all_env_vars(state: &mut OpState) {
    let snapshot = state.borrow::<ScriptInputState>().variables.env.clone();
    let out = state.borrow_mut::<ScriptOutputState>();
    let keys = scope_keys(&snapshot, out.env_var_writes.iter().map(|w| &w.key));
    for key in keys {
        out.env_var_writes.push(EnvVarWrite {
            key,
            value: serde_json::Value::Null,
            persist: true,
        });
    }
}

/// rok.deleteCollectionVar(key) — null-writes one collection variable.
#[op2(fast)]
pub fn op_rok_delete_collection_var(state: &mut OpState, #[string] key: String) {
    state
        .borrow_mut::<ScriptOutputState>()
        .collection_var_writes
        .push(CollectionVarWrite {
            key,
            value: serde_json::Value::Null,
        });
}

/// rok.deleteAllCollectionVars() — null-writes every collection variable.
#[op2(fast)]
pub fn op_rok_delete_all_collection_vars(state: &mut OpState) {
    let snapshot = state.borrow::<ScriptInputState>().variables.collection.clone();
    let out = state.borrow_mut::<ScriptOutputState>();
    let keys = scope_keys(&snapshot, out.collection_var_writes.iter().map(|w| &w.key));
    for key in keys {
        out.collection_var_writes.push(CollectionVarWrite {
            key,
            value: serde_json::Value::Null,
        });
    }
}

/// rok.deleteGlobalEnvVar(key) — null-writes one global environment variable.
#[op2(fast)]
pub fn op_rok_delete_global_env_var(state: &mut OpState, #[string] key: String) {
    state
        .borrow_mut::<ScriptOutputState>()
        .global_env_var_writes
        .push(EnvVarWrite {
            key,
            value: serde_json::Value::Null,
            persist: true,
        });
}

/// rok.deleteAllGlobalEnvVars() — null-writes every global environment variable.
#[op2(fast)]
pub fn op_rok_delete_all_global_env_vars(state: &mut OpState) {
    let snapshot = state.borrow::<ScriptInputState>().variables.global_env.clone();
    let out = state.borrow_mut::<ScriptOutputState>();
    let keys = scope_keys(&snapshot, out.global_env_var_writes.iter().map(|w| &w.key));
    for key in keys {
        out.global_env_var_writes.push(EnvVarWrite {
            key,
            value: serde_json::Value::Null,
            persist: true,
        });
    }
}
```

- [ ] **Step 5: Register the ops and add the JS wrappers**

In `engine.rs` registration list, add:

```rust
        rok::op_rok_delete_var,
        rok::op_rok_delete_all_vars,
        rok::op_rok_delete_all_env_vars,
        rok::op_rok_delete_collection_var,
        rok::op_rok_delete_all_collection_vars,
        rok::op_rok_delete_global_env_var,
        rok::op_rok_delete_all_global_env_vars,
```

In `bootstrap.js`, in the `rok` block add:

```js
    deleteVar:                (key) => __ops.op_rok_delete_var(key),
    deleteAllVars:            ()    => __ops.op_rok_delete_all_vars(),
    deleteAllEnvVars:         ()    => __ops.op_rok_delete_all_env_vars(),
    deleteCollectionVar:      (key) => __ops.op_rok_delete_collection_var(key),
    deleteAllCollectionVars:  ()    => __ops.op_rok_delete_all_collection_vars(),
    deleteGlobalEnvVar:       (key) => __ops.op_rok_delete_global_env_var(key),
    deleteAllGlobalEnvVars:   ()    => __ops.op_rok_delete_all_global_env_vars(),
```

- [ ] **Step 6: Run the engine tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra rok_delete rok_set_after_delete rok_collection_and_global`
Expected: PASS (7 tests).

- [ ] **Step 7: Write the failing app-layer tests**

Add to the `tests` module of `crates/rocket-app/src/execution_service.rs`, near the other `apply_*` tests:

```rust
    #[test]
    fn merge_runtime_vars_keeps_non_string_values_as_json_text() {
        let mut ctx = rocket_environment::VariableContext::default();
        let mut result = ScriptResult::default();
        result.runtime_vars.insert("n".into(), serde_json::json!(0));
        result.runtime_vars.insert("s".into(), serde_json::json!("text"));
        result
            .runtime_vars
            .insert("o".into(), serde_json::json!({ "a": 1 }));
        result.runtime_vars.insert("nil".into(), serde_json::Value::Null);
        merge_runtime_vars(&mut ctx, &result);
        assert_eq!(ctx.runtime.get("n").map(String::as_str), Some("0"));
        assert_eq!(ctx.runtime.get("s").map(String::as_str), Some("text"));
        assert_eq!(ctx.runtime.get("o").map(String::as_str), Some("{\"a\":1}"));
        assert!(!ctx.runtime.contains_key("nil"));
    }

    #[test]
    fn merge_runtime_vars_applies_deletes_after_sets() {
        let mut ctx = rocket_environment::VariableContext::default();
        ctx.runtime.insert("old".into(), "1".into());
        let result = ScriptResult {
            runtime_var_deletes: vec!["old".into()],
            ..Default::default()
        };
        merge_runtime_vars(&mut ctx, &result);
        assert!(!ctx.runtime.contains_key("old"));
    }

    #[test]
    fn remove_variable_drops_the_named_variable_only() {
        let mut vars = vec![
            rocket_collection::CollectionVariable {
                key: "keep".into(),
                value: "1".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
            rocket_collection::CollectionVariable {
                key: "drop".into(),
                value: "2".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
        ];
        assert!(remove_variable(&mut vars, "drop"));
        assert!(!remove_variable(&mut vars, "drop"));
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].key, "keep");
    }

    #[tokio::test]
    async fn post_response_script_env_var_null_write_removes_the_variable() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("TOKEN", "old"));
        env.set_variable(Variable::new("KEEP", "1"));
        let env_repo = RecordingEnvRepo::with_env(env);

        let result = ScriptResult {
            env_var_writes: vec![EnvVarWrite {
                key: "TOKEN".into(),
                value: serde_json::Value::Null,
                persist: true,
            }],
            ..Default::default()
        };

        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(Arc::clone(&env_repo))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(MockScriptEngine::returning_post_response(result)),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        svc.execute(input).await.expect("execute failed");

        let saved = env_repo.last_saved().expect("env_repo.save() was called");
        assert_eq!(saved.get_value("TOKEN"), None);
        assert_eq!(saved.get_value("KEEP"), Some("1"));
    }
```

- [ ] **Step 8: Run to verify they fail**

Run: `cargo test -j4 -p rocket-app merge_runtime_vars remove_variable post_response_script_env_var_null`
Expected: FAIL to compile (`cannot find function merge_runtime_vars`, `remove_variable`).

- [ ] **Step 9: Implement the app-layer changes**

In `crates/rocket-app/src/execution_service.rs`:

1. Next to `upsert_variable` (near line 2074), add:

```rust
/// Removes a collection variable by key. Returns true when one was removed.
fn remove_variable(vars: &mut Vec<rocket_collection::CollectionVariable>, key: &str) -> bool {
    let before = vars.len();
    vars.retain(|v| v.key != key);
    vars.len() != before
}

/// Merges a script's runtime writes and deletes into the variable context.
///
/// Non-string values are kept as JSON text so a number or object set with
/// `rok.setVar` survives into the next script phase. A null value is skipped.
fn merge_runtime_vars(var_ctx: &mut rocket_environment::VariableContext, result: &ScriptResult) {
    for (key, value) in &result.runtime_vars {
        let text = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => continue,
            other => other.to_string(),
        };
        var_ctx.runtime.insert(key.clone(), text);
    }
    for key in &result.runtime_var_deletes {
        var_ctx.runtime.remove(key);
    }
}
```

2. In `apply_script_side_effects`, replace the `// Merge runtime vars ...` loop at the end with:

```rust
        // Merge runtime vars into context for subsequent phases.
        merge_runtime_vars(var_ctx, result);
```

3. Add the delete helper after `apply_collection_var_write`:

```rust
    /// Removes a collection variable and publishes the same events as a write.
    fn apply_collection_var_delete(&self, collection: &str, key: &str) -> DomainResult<()> {
        let mut settings = self.collection_repo.get_settings(collection)?;
        if !remove_variable(&mut settings.variables, key) {
            return Ok(());
        }
        self.collection_repo.save_settings(collection, &settings)?;
        self.events.publish(DomainEvent::CollectionVariableWritten {
            collection: collection.to_string(),
            key: key.to_string(),
        });
        self.events.publish(DomainEvent::ScriptVariableWritten {
            scope: "collection".to_string(),
            environment: None,
            collection: Some(collection.to_string()),
            key: key.to_string(),
        });
        Ok(())
    }
```

4. In the collection loop of `apply_script_side_effects`, add as the first statement inside `for write in &result.collection_var_writes {`:

```rust
                    if write.value.is_null() {
                        if let Err(e) = self.apply_collection_var_delete(col, &write.key) {
                            tracing::warn!(error = %e, key = %write.key, "failed to persist collection var delete");
                        }
                        continue;
                    }
```

- [ ] **Step 10: Run the app-layer tests**

Run: `cargo test -j4 -p rocket-app merge_runtime_vars remove_variable post_response_script_env_var`
Expected: PASS (existing env tests still pass).

- [ ] **Step 11: Add the typings and run the sync test**

In `ROK_DEFS`, add:

```ts
  /** Delete a runtime variable. */
  deleteVar(key: string): void;
  /** Delete every runtime variable. */
  deleteAllVars(): void;
  /** Delete every variable of the active environment (persisted). */
  deleteAllEnvVars(): void;
  /** Delete a collection variable (persisted to opencollection.yml). */
  deleteCollectionVar(key: string): void;
  /** Delete every collection variable (persisted). */
  deleteAllCollectionVars(): void;
  /** Delete a global environment variable (persisted). */
  deleteGlobalEnvVar(key: string): void;
  /** Delete every global environment variable (persisted). */
  deleteAllGlobalEnvVars(): void;
```

Run: `yarn test rok-types && yarn tsc --noEmit && yarn check && cargo check -j4`
Expected: PASS.

- [ ] **Step 12: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-scripting/src/result.rs`, `crates/rocket-infra/src/scripting/state.rs`, `crates/rocket-infra/src/scripting/ops/rok.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `crates/rocket-app/src/execution_service.rs`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add rok delete APIs`.

---

### Task 3: Read-your-writes overlay

**Files:**
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js`
- Test: `crates/rocket-infra/src/scripting/engine.rs` (tests module)

**Interfaces:**
- Consumes: Tasks 1 and 2 (all `rok` read, set and delete wrappers exist).
- Produces: no new names. Reads (`getVar`, `hasVar`, `getAllVars`, and the env, collection and global equivalents) reflect the same script's earlier sets and deletes. Reads return the original JS value for a key set in the same script.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `engine.rs`:

```rust
    #[tokio::test]
    async fn rok_get_var_sees_an_earlier_set_in_the_same_script() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('a', 1); rok.setVar('b', rok.getVar('a') + 1)");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("b").expect("b present"), 2);
    }

    #[tokio::test]
    async fn rok_has_and_get_all_vars_follow_set_and_delete() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("old".into(), "1".into());
        let mut ctx = minimal_ctx(
            "rok.setVar('fresh', 'x'); rok.deleteVar('old'); \
             rok.setVar('out', [rok.hasVar('fresh'), rok.hasVar('old'), \
             Object.keys(rok.getAllVars()).sort().join(',')].join('|'))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "true|false|fresh"
        );
    }

    #[tokio::test]
    async fn rok_env_reads_follow_set_delete_and_delete_all() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("A".into(), "1".into());
        let mut ctx = minimal_ctx(
            "rok.setEnvVar('B', '2'); \
             const before = Object.keys(rok.getAllEnvVars()).sort().join(','); \
             rok.deleteAllEnvVars(); \
             rok.setVar('out', before + '|' + rok.hasEnvVar('A') + '|' + rok.hasEnvVar('B') \
               + '|' + Object.keys(rok.getAllEnvVars()).length)",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "A,B|false|false|0"
        );
    }

    #[tokio::test]
    async fn rok_collection_and_global_reads_follow_writes() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.collection.insert("c".into(), "1".into());
        let mut ctx = minimal_ctx(
            "rok.setCollectionVar('c', '2'); rok.setGlobalEnvVar('g', 'x'); \
             const c1 = rok.getCollectionVar('c'); \
             rok.deleteCollectionVar('c'); \
             rok.setVar('out', c1 + '|' + rok.hasCollectionVar('c') + '|' + rok.getGlobalEnvVar('g') \
               + '|' + rok.hasGlobalEnvVar('g'))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "2|false|x|true"
        );
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra rok_get_var_sees rok_has_and_get_all rok_env_reads_follow rok_collection_and_global_reads`
Expected: FAIL (reads return snapshot values).

- [ ] **Step 3: Implement the overlay**

In `bootstrap.js`, immediately above `globalThis.rok = {`, add:

```js
  // ── read-your-writes overlay ────────────────────────────────────────────────
  // Ops read a snapshot taken before the script ran. These maps remember this
  // script's own sets and deletes so later reads in the same script agree.
  const _GONE = Symbol('gone');
  const _ov = { runtime: new Map(), env: new Map(), collection: new Map(), global: new Map() };

  function _ovRead(scope, key, base) {
    const m = _ov[scope];
    if (m.has(key)) { const v = m.get(key); return v === _GONE ? "" : v; }
    return base(key);
  }
  function _ovHas(scope, key, base) {
    const m = _ov[scope];
    if (m.has(key)) return m.get(key) !== _GONE;
    return base(key);
  }
  function _ovAll(scope, base) {
    const out = base();
    for (const [k, v] of _ov[scope]) {
      if (v === _GONE) delete out[k]; else out[k] = v;
    }
    return out;
  }
  function _ovDeleteAll(scope, base) {
    for (const k of Object.keys(_ovAll(scope, base))) _ov[scope].set(k, _GONE);
  }
```

Then wrap the existing `rok` members. Replace the existing definitions of the members below inside `globalThis.rok = { ... }` with these (leave every other member unchanged):

```js
    getVar:     (key) => _ovRead('runtime', key, (k) => __ops.op_rok_get_var(k)),
    setVar:     (key, value) => { __ops.op_rok_set_var(key, JSON.stringify(value)); _ov.runtime.set(key, value); },
    hasVar:     (key) => _ovHas('runtime', key, (k) => __ops.op_rok_has_var(k)),
    getAllVars: () => _ovAll('runtime', () => JSON.parse(__ops.op_rok_get_all_vars())),
    deleteVar:  (key) => { __ops.op_rok_delete_var(key); _ov.runtime.set(key, _GONE); },
    deleteAllVars: () => {
      _ovDeleteAll('runtime', () => JSON.parse(__ops.op_rok_get_all_vars()));
      __ops.op_rok_delete_all_vars();
    },

    getEnvVar:  (key) => _ovRead('env', key, (k) => __ops.op_rok_get_env_var(k)),
    setEnvVar:  (key, value, opts) => {
      __ops.op_rok_set_env_var(key, JSON.stringify(value), !!(opts && opts.persist));
      _ov.env.set(key, value);
    },
    hasEnvVar:  (key) => _ovHas('env', key, (k) => __ops.op_rok_has_env_var(k)),
    getAllEnvVars: () => _ovAll('env', () => JSON.parse(__ops.op_rok_get_all_env_vars())),
    deleteEnvVar: (key) => { __ops.op_rok_delete_env_var(key); _ov.env.set(key, _GONE); },
    deleteAllEnvVars: () => {
      _ovDeleteAll('env', () => JSON.parse(__ops.op_rok_get_all_env_vars()));
      __ops.op_rok_delete_all_env_vars();
    },

    getCollectionVar: (key) => _ovRead('collection', key, (k) => __ops.op_rok_get_collection_var(k)),
    setCollectionVar: (key, value) => {
      __ops.op_rok_set_collection_var(key, JSON.stringify(value));
      _ov.collection.set(key, value);
    },
    hasCollectionVar: (key) => _ovHas('collection', key, (k) => __ops.op_rok_has_collection_var(k)),
    deleteCollectionVar: (key) => { __ops.op_rok_delete_collection_var(key); _ov.collection.set(key, _GONE); },
    deleteAllCollectionVars: () => {
      // The collection scope has no read-all op, so remember which keys the
      // script touched and let the op expand the snapshot keys.
      for (const k of Array.from(_ov.collection.keys())) _ov.collection.set(k, _GONE);
      __ops.op_rok_delete_all_collection_vars();
    },

    getGlobalEnvVar: (key) => _ovRead('global', key, (k) => __ops.op_rok_get_global_env_var(k)),
    setGlobalEnvVar: (key, value) => {
      __ops.op_rok_set_global_env_var(key, JSON.stringify(value));
      _ov.global.set(key, value);
    },
    hasGlobalEnvVar: (key) => _ovHas('global', key, (k) => __ops.op_rok_has_global_env_var(k)),
    getAllGlobalEnvVars: () => _ovAll('global', () => JSON.parse(__ops.op_rok_get_all_global_env_vars())),
    deleteGlobalEnvVar: (key) => { __ops.op_rok_delete_global_env_var(key); _ov.global.set(key, _GONE); },
    deleteAllGlobalEnvVars: () => {
      _ovDeleteAll('global', () => JSON.parse(__ops.op_rok_get_all_global_env_vars()));
      __ops.op_rok_delete_all_global_env_vars();
    },
```

Known limit, documented in a comment above `deleteAllCollectionVars`: after `deleteAllCollectionVars()`, snapshot collection keys the script never touched still read as their snapshot value for the rest of that script, because there is no read-all op for the collection scope. The persisted result is correct. Add a collection read-all op only if a user asks.

- [ ] **Step 4: Run the new and the earlier tests**

Run: `cargo test -j4 -p rocket-infra rok_`
Expected: PASS for all `rok_*` tests, including Task 1 and Task 2 tests.

- [ ] **Step 5: Run the broader scripting checks**

Run: `cargo test -j4 -p rocket-infra scripting && cargo check -j4 && yarn test rok-types`
Expected: PASS.

- [ ] **Step 6: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of `crates/rocket-infra/src/scripting/bootstrap.js` and `crates/rocket-infra/src/scripting/engine.rs`.
Suggested subject: `feat(scripting): make rok reads see earlier writes`.
