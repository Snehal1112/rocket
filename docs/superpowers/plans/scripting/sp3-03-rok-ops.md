# SP3-03 — `rocket-infra`: `rok.*` Ops

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all `rok.*` Rust ops — variable reads/writes across all scopes (runtime, env, collection, global env), `rok.getEnvName()`, `rok.interpolate()`, and the runner ops (`setNextRequest`, `skipRequest`). Replace the stubs from SP3-02 with full implementations.

**Architecture:** All ops read from `ScriptInputState` and write to `ScriptOutputState` inside `deno_core::OpState`. The ops themselves never write to disk — they only populate `ScriptResult` fields which `HttpService` applies after execution.

**Tech Stack:** Rust, `deno_core::op2`

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §4 (Variable API, Runner API)

**Depends on:** SP3-02 merged.

---

## Task 1: Variable read ops (`getVar`, `getEnvVar`, `hasEnvVar`, `getEnvName`, `getCollectionVar`, `getGlobalEnvVar`)

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs`

- [ ] **Step 1: Replace stub with full read ops in `crates/rocket-infra/src/scripting/ops/rok.rs`**

```rust
use deno_core::{op2, OpState};
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use rocket_scripting::{EnvVarWrite, CollectionVarWrite, NextRequest};

// ── Variable reads ────────────────────────────────────────────────────────────

/// rok.getVar(key) — reads from the runtime scope (highest priority).
#[op2]
#[string]
pub fn op_rok_get_var(state: &OpState, #[string] key: String) -> String {
    let input = state.borrow::<ScriptInputState>();
    input.variables.runtime
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.getEnvVar(key) — reads from the active environment scope.
#[op2]
#[string]
pub fn op_rok_get_env_var(state: &OpState, #[string] key: String) -> String {
    let input = state.borrow::<ScriptInputState>();
    input.variables.env
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.hasEnvVar(key) — returns true if the key exists in the active environment.
#[op2(fast)]
pub fn op_rok_has_env_var(state: &OpState, #[string] key: String) -> bool {
    let input = state.borrow::<ScriptInputState>();
    input.variables.env.contains_key(&key)
}

/// rok.getEnvName() — name of the active environment, or empty string if none.
#[op2]
#[string]
pub fn op_rok_get_env_name(state: &OpState) -> String {
    let input = state.borrow::<ScriptInputState>();
    input.env_name.clone().unwrap_or_default()
}

/// rok.getCollectionVar(key) — reads from the collection variable scope.
#[op2]
#[string]
pub fn op_rok_get_collection_var(state: &OpState, #[string] key: String) -> String {
    let input = state.borrow::<ScriptInputState>();
    input.variables.collection
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.getGlobalEnvVar(key) — reads from the global environment scope.
#[op2]
#[string]
pub fn op_rok_get_global_env_var(state: &OpState, #[string] key: String) -> String {
    let input = state.borrow::<ScriptInputState>();
    input.variables.global_env
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.interpolate(template) — resolves {{var}} tokens using the flattened variable context.
#[op2]
#[string]
pub fn op_rok_interpolate(state: &OpState, #[string] template: String) -> String {
    let input = state.borrow::<ScriptInputState>();
    let flat = input.variables.flatten();
    let mut result = template;
    for (key, value) in &flat {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/scripting/ops/rok.rs
git commit -m "feat(rocket-infra): rok variable read ops"
```

---

## Task 2: Variable write ops + runner ops

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (register new ops)

- [ ] **Step 1: Add write ops to `rok.rs`**

Append to `crates/rocket-infra/src/scripting/ops/rok.rs`:

```rust
// ── Variable writes ───────────────────────────────────────────────────────────

/// rok.setVar(key, jsonValue) — writes to runtime scope (in-memory only).
#[op2(fast)]
pub fn op_rok_set_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
) {
    let value: serde_json::Value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().runtime_vars.insert(key, value);
}

/// rok.setEnvVar(key, jsonValue, persist) — writes to active environment.
/// When persist=true the value is written to the .yml file after execution.
#[op2(fast)]
pub fn op_rok_set_env_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
    persist: bool,
) {
    let value: serde_json::Value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().env_var_writes.push(EnvVarWrite { key, value, persist });
}

/// rok.deleteEnvVar(key) — tombstone write (value=null) to active environment.
#[op2(fast)]
pub fn op_rok_delete_env_var(state: &mut OpState, #[string] key: String) {
    state.borrow_mut::<ScriptOutputState>().env_var_writes.push(EnvVarWrite {
        key,
        value: serde_json::Value::Null,
        persist: false,
    });
}

/// rok.setCollectionVar(key, jsonValue) — writes to collection variable scope.
#[op2(fast)]
pub fn op_rok_set_collection_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
) {
    let value: serde_json::Value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().collection_var_writes.push(CollectionVarWrite { key, value });
}

/// rok.setGlobalEnvVar(key, jsonValue) — writes to global environment scope.
#[op2(fast)]
pub fn op_rok_set_global_env_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
) {
    let value: serde_json::Value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().global_env_var_writes.push(EnvVarWrite {
        key,
        value,
        persist: false,
    });
}

// ── Runner ops ────────────────────────────────────────────────────────────────

/// rok.runner.setNextRequest(name | null) — controls flow in collection runner.
#[op2(fast)]
pub fn op_rok_set_next_request(state: &mut OpState, #[string] name: String) {
    let next = if name.is_empty() || name == "null" {
        NextRequest::Stop
    } else {
        NextRequest::Name(name)
    };
    state.borrow_mut::<ScriptOutputState>().next_request = Some(next);
}

/// rok.runner.skipRequest() — marks this request to be skipped in a runner.
#[op2(fast)]
pub fn op_rok_skip_request(state: &mut OpState) {
    state.borrow_mut::<ScriptOutputState>().skip_request = true;
}
```

- [ ] **Step 2: Register all new ops in `engine.rs`**

Open `crates/rocket-infra/src/scripting/engine.rs`. In `build_extension()`, replace the `rok` stub with the full op list:

```rust
// rok ops
rok::op_rok_get_var(),
rok::op_rok_set_var(),
rok::op_rok_get_env_var(),
rok::op_rok_set_env_var(),
rok::op_rok_has_env_var(),
rok::op_rok_delete_env_var(),
rok::op_rok_get_env_name(),
rok::op_rok_get_collection_var(),
rok::op_rok_set_collection_var(),
rok::op_rok_get_global_env_var(),
rok::op_rok_set_global_env_var(),
rok::op_rok_interpolate(),
rok::op_rok_set_next_request(),
rok::op_rok_skip_request(),
```

- [ ] **Step 3: Compile**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "feat(rocket-infra): rok variable write + runner ops"
```

---

## Task 3: Tests for `rok.*` ops end-to-end

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (append test module)

- [ ] **Step 1: Add rok op integration tests**

Append to the `#[cfg(test)]` block in `crates/rocket-infra/src/scripting/engine.rs`:

```rust
#[tokio::test]
async fn rok_set_and_get_runtime_var() {
    let engine = DenoScriptEngine::new();
    // setVar writes to runtime_vars in ScriptResult
    let ctx = minimal_ctx("rok.setVar('token', 'abc123')");
    let result = engine.execute(ctx).await.unwrap();
    let val = result.runtime_vars.get("token").unwrap();
    assert_eq!(val, "abc123");
}

#[tokio::test]
async fn rok_get_env_var_reads_from_context() {
    let engine = DenoScriptEngine::new();
    let mut vars = rocket_environment::VariableContext::default();
    vars.env.insert("BASE_URL".into(), "https://api.example.com".into());

    let mut ctx = minimal_ctx("rok.setVar('url', rok.getEnvVar('BASE_URL'))");
    ctx.variables = vars;
    let result = engine.execute(ctx).await.unwrap();
    let val = result.runtime_vars.get("url").unwrap();
    assert_eq!(val, "https://api.example.com");
}

#[tokio::test]
async fn rok_set_env_var_no_persist() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("rok.setEnvVar('SESSION', 'xyz')");
    let result = engine.execute(ctx).await.unwrap();
    assert_eq!(result.env_var_writes.len(), 1);
    assert_eq!(result.env_var_writes[0].key, "SESSION");
    assert!(!result.env_var_writes[0].persist);
}

#[tokio::test]
async fn rok_set_env_var_with_persist() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("rok.setEnvVar('TOKEN', 'abc', { persist: true })");
    let result = engine.execute(ctx).await.unwrap();
    assert!(result.env_var_writes[0].persist);
}

#[tokio::test]
async fn rok_has_env_var() {
    let engine = DenoScriptEngine::new();
    let mut vars = rocket_environment::VariableContext::default();
    vars.env.insert("EXISTS".into(), "yes".into());
    let mut ctx = minimal_ctx("rok.setVar('found', rok.hasEnvVar('EXISTS') ? '1' : '0')");
    ctx.variables = vars;
    let result = engine.execute(ctx).await.unwrap();
    assert_eq!(result.runtime_vars.get("found").unwrap(), "1");
}

#[tokio::test]
async fn rok_interpolate() {
    let engine = DenoScriptEngine::new();
    let mut vars = rocket_environment::VariableContext::default();
    vars.env.insert("host".into(), "api.example.com".into());
    let mut ctx = minimal_ctx("rok.setVar('url', rok.interpolate('https://{{host}}/users'))");
    ctx.variables = vars;
    let result = engine.execute(ctx).await.unwrap();
    assert_eq!(result.runtime_vars.get("url").unwrap(), "https://api.example.com/users");
}

#[tokio::test]
async fn rok_runner_skip_request() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("rok.runner.skipRequest()");
    let result = engine.execute(ctx).await.unwrap();
    assert!(result.skip_request);
}

#[tokio::test]
async fn rok_runner_set_next_request() {
    let engine = DenoScriptEngine::new();
    let ctx = minimal_ctx("rok.runner.setNextRequest('Poll Status')");
    let result = engine.execute(ctx).await.unwrap();
    assert!(matches!(result.next_request, Some(rocket_scripting::NextRequest::Name(s)) if s == "Poll Status"));
}
```

- [ ] **Step 2: Run all scripting tests**

```bash
cargo test -p rocket-infra scripting 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "test(rocket-infra): rok ops integration tests"
```
