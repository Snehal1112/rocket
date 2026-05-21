# SP3-02 — `rocket-infra`: Deno Runtime Scaffolding

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `deno_core` to `rocket-infra` and wire up a working `DenoScriptEngine` skeleton — one `JsRuntime` per execution, sandboxed (no Deno stdlib, no fs, no network), with op extension scaffolding in place. All op implementations are stubs that will be filled in SP3-03 through SP3-05.

**Architecture:** `crates/rocket-infra/src/scripting/` module. `DenoScriptEngine` implements `rocket_scripting::ScriptEngine`. Op state is thread-local inside the runtime per execution. The JS bootstrap script (`init.js`) sets up the `rok`, `req`, `res`, and `console` globals before user code runs.

**Tech Stack:** Rust, `deno_core` (pin to latest stable), `async-trait`

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §3

**Depends on:** SP3-01 merged.

---

## Task 1: Add `deno_core` dependency + module scaffold

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`
- Create: `crates/rocket-infra/src/scripting/mod.rs`
- Create: `crates/rocket-infra/src/scripting/ops/mod.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Find the latest stable `deno_core` version**

```bash
cargo search deno_core 2>&1 | head -5
```

Note the version number. Use it in the next step.

- [ ] **Step 2: Add dependencies to `crates/rocket-infra/Cargo.toml`**

Add to `[dependencies]`:

```toml
rocket-scripting = { path = "../rocket-scripting" }
deno_core = "X.Y.Z"   # use version from step 1
serde_json = "1"
```

- [ ] **Step 3: Create `crates/rocket-infra/src/scripting/ops/mod.rs`**

```rust
pub mod console;
pub mod req;
pub mod res;
pub mod rok;
```

- [ ] **Step 4: Create stub op files**

Create `crates/rocket-infra/src/scripting/ops/rok.rs`:

```rust
/// Stub — rok.* ops implemented in SP3-03.
use deno_core::op2;

#[op2]
#[string]
pub fn op_rok_get_var(#[string] _key: String) -> String {
    String::new()
}
```

Create `crates/rocket-infra/src/scripting/ops/req.rs`:

```rust
/// Stub — req.* ops implemented in SP3-04.
use deno_core::op2;

#[op2]
#[string]
pub fn op_req_get_url() -> String {
    String::new()
}
```

Create `crates/rocket-infra/src/scripting/ops/res.rs`:

```rust
/// Stub — res.* ops implemented in SP3-04.
use deno_core::op2;

#[op2(fast)]
pub fn op_res_get_status() -> u32 {
    0
}
```

Create `crates/rocket-infra/src/scripting/ops/console.rs`:

```rust
/// Stub — console ops implemented in SP3-05.
use deno_core::op2;

#[op2(fast)]
pub fn op_console_log(#[string] _msg: String) {}
```

- [ ] **Step 5: Expose `scripting` module in `crates/rocket-infra/src/lib.rs`**

Add at the top of the module list:

```rust
pub mod scripting;
```

- [ ] **Step 6: Verify stub compile**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

Expected: zero errors (stubs compile).

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/
git commit -m "feat(rocket-infra): scaffold scripting module + deno_core dependency"
```

---

## Task 2: `OpState` structs + JS bootstrap script

**Files:**
- Create: `crates/rocket-infra/src/scripting/state.rs`
- Create: `crates/rocket-infra/src/scripting/bootstrap.js`

- [ ] **Step 1: Create `crates/rocket-infra/src/scripting/state.rs`**

This is the shared mutable state that ops read/write inside the `JsRuntime`.

```rust
use std::collections::HashMap;
use rocket_http::{HttpRequest, HttpResponse};
use rocket_scripting::{
    CollectionVarWrite, ConsoleEntry, ConsoleLevel, EnvVarWrite,
    NextRequest, RequestMutations, ScriptPhase, TestResult, TestStatus,
};
use rocket_environment::VariableContext;

/// Holds everything ops need to read from the `ScriptContext`.
/// Stored in `deno_core::OpState` as a read-only snapshot.
pub struct ScriptInputState {
    pub phase: ScriptPhase,
    pub variables: VariableContext,
    pub request: HttpRequest,
    pub response: Option<HttpResponse>,
    pub env_name: Option<String>,
    pub execution_mode: String,
    pub execution_platform: String,
}

/// Accumulates all side-effects produced by ops during execution.
/// Stored in `deno_core::OpState` as mutable state.
#[derive(Default)]
pub struct ScriptOutputState {
    pub request_mutations: RequestMutations,
    pub any_request_mutation: bool,
    pub runtime_vars: HashMap<String, serde_json::Value>,
    pub env_var_writes: Vec<EnvVarWrite>,
    pub collection_var_writes: Vec<CollectionVarWrite>,
    pub global_env_var_writes: Vec<EnvVarWrite>,
    pub next_request: Option<NextRequest>,
    pub skip_request: bool,
    pub test_results: Vec<TestResult>,
    pub console_entries: Vec<ConsoleEntry>,
}

impl ScriptOutputState {
    pub fn add_console(&mut self, level: ConsoleLevel, message: String) {
        self.console_entries.push(ConsoleEntry { level, message });
    }

    pub fn add_test_result(&mut self, name: String, passed: bool, error: Option<String>) {
        self.test_results.push(TestResult {
            name,
            status: if passed { TestStatus::Passed } else { TestStatus::Failed },
            error,
        });
    }
}
```

- [ ] **Step 2: Create `crates/rocket-infra/src/scripting/bootstrap.js`**

This script runs before user code. It sets up `rok`, `req`, `res`, and `console` globals using the Rust ops, and provides `test()` and `expect()` (Chai-compatible subset) as globals.

```javascript
"use strict";

// ── console ──────────────────────────────────────────────────────────────────
const console = {
  log:   (...args) => Deno.core.ops.op_console_log(args.map(String).join(" ")),
  warn:  (...args) => Deno.core.ops.op_console_warn(args.map(String).join(" ")),
  error: (...args) => Deno.core.ops.op_console_error(args.map(String).join(" ")),
};
globalThis.console = console;

// ── rok ───────────────────────────────────────────────────────────────────────
globalThis.rok = {
  getVar:            (key)        => Deno.core.ops.op_rok_get_var(key),
  setVar:            (key, value) => Deno.core.ops.op_rok_set_var(key, JSON.stringify(value)),
  getEnvVar:         (key)        => Deno.core.ops.op_rok_get_env_var(key),
  setEnvVar:         (key, value, opts) => Deno.core.ops.op_rok_set_env_var(key, JSON.stringify(value), !!(opts && opts.persist)),
  hasEnvVar:         (key)        => Deno.core.ops.op_rok_has_env_var(key),
  deleteEnvVar:      (key)        => Deno.core.ops.op_rok_delete_env_var(key),
  getEnvName:        ()           => Deno.core.ops.op_rok_get_env_name(),
  getCollectionVar:  (key)        => Deno.core.ops.op_rok_get_collection_var(key),
  setCollectionVar:  (key, value) => Deno.core.ops.op_rok_set_collection_var(key, JSON.stringify(value)),
  getGlobalEnvVar:   (key)        => Deno.core.ops.op_rok_get_global_env_var(key),
  setGlobalEnvVar:   (key, value) => Deno.core.ops.op_rok_set_global_env_var(key, JSON.stringify(value)),
  interpolate:       (template)   => Deno.core.ops.op_rok_interpolate(template),
  runner: {
    setNextRequest: (name)  => Deno.core.ops.op_rok_set_next_request(name),
    skipRequest:    ()      => Deno.core.ops.op_rok_skip_request(),
  },
};

// ── req ───────────────────────────────────────────────────────────────────────
globalThis.req = {
  getUrl:              ()           => Deno.core.ops.op_req_get_url(),
  setUrl:              (url)        => Deno.core.ops.op_req_set_url(url),
  getHost:             ()           => Deno.core.ops.op_req_get_host(),
  getPath:             ()           => Deno.core.ops.op_req_get_path(),
  getQueryString:      ()           => Deno.core.ops.op_req_get_query_string(),
  getPathParams:       ()           => JSON.parse(Deno.core.ops.op_req_get_path_params()),
  getMethod:           ()           => Deno.core.ops.op_req_get_method(),
  setMethod:           (method)     => Deno.core.ops.op_req_set_method(method),
  getName:             ()           => Deno.core.ops.op_req_get_name(),
  getTags:             ()           => JSON.parse(Deno.core.ops.op_req_get_tags()),
  getAuthMode:         ()           => Deno.core.ops.op_req_get_auth_mode(),
  getHeader:           (name)       => Deno.core.ops.op_req_get_header(name),
  getHeaders:          ()           => JSON.parse(Deno.core.ops.op_req_get_headers()),
  setHeader:           (name, val)  => Deno.core.ops.op_req_set_header(name, val),
  setHeaders:          (headers)    => Deno.core.ops.op_req_set_headers(JSON.stringify(headers)),
  deleteHeader:        (name)       => Deno.core.ops.op_req_delete_header(name),
  deleteHeaders:       (names)      => Deno.core.ops.op_req_delete_headers(JSON.stringify(names)),
  getBody:             (opts)       => {
    const raw = Deno.core.ops.op_req_get_body(!!(opts && opts.raw));
    return (opts && opts.raw) ? raw : JSON.parse(raw);
  },
  setBody:             (body)       => Deno.core.ops.op_req_set_body(JSON.stringify(body)),
  getTimeout:          ()           => Deno.core.ops.op_req_get_timeout(),
  setTimeout:          (ms)         => Deno.core.ops.op_req_set_timeout(ms),
  setMaxRedirects:     (n)          => Deno.core.ops.op_req_set_max_redirects(n),
  getExecutionMode:    ()           => Deno.core.ops.op_req_get_execution_mode(),
  getExecutionPlatform:()           => Deno.core.ops.op_req_get_execution_platform(),
  onFail:              (cb)         => { /* no-op in safe mode */ },
};

// ── res ───────────────────────────────────────────────────────────────────────
globalThis.res = {
  getStatus:        ()      => Deno.core.ops.op_res_get_status(),
  getStatusText:    ()      => Deno.core.ops.op_res_get_status_text(),
  getHeader:        (name)  => Deno.core.ops.op_res_get_header(name),
  getHeaders:       ()      => JSON.parse(Deno.core.ops.op_res_get_headers()),
  getBody:          (opts)  => {
    const raw = Deno.core.ops.op_res_get_body(!!(opts && opts.raw));
    return (opts && opts.raw) ? raw : (() => { try { return JSON.parse(raw); } catch { return raw; } })();
  },
  getResponseTime:  ()      => Deno.core.ops.op_res_get_response_time(),
};

// ── test() + expect() (chai subset) ──────────────────────────────────────────
function expect(actual) {
  return {
    to: {
      equal: (expected) => {
        if (actual !== expected) throw new Error(`Expected ${JSON.stringify(actual)} to equal ${JSON.stringify(expected)}`);
      },
      be: {
        true:  () => { if (actual !== true)  throw new Error(`Expected true`); },
        false: () => { if (actual !== false) throw new Error(`Expected false`); },
        null:  () => { if (actual !== null)  throw new Error(`Expected null`); },
      },
      include: (val) => {
        if (typeof actual === 'string' && !actual.includes(val)) throw new Error(`Expected "${actual}" to include "${val}"`);
        if (Array.isArray(actual) && !actual.includes(val)) throw new Error(`Expected array to include ${JSON.stringify(val)}`);
      },
      have: {
        property: (key) => {
          if (typeof actual !== 'object' || actual === null || !(key in actual))
            throw new Error(`Expected object to have property "${key}"`);
        },
        status: (code) => {
          if (actual.status !== code) throw new Error(`Expected status ${code}, got ${actual.status}`);
        },
      },
      not: {
        equal: (expected) => {
          if (actual === expected) throw new Error(`Expected ${JSON.stringify(actual)} to not equal ${JSON.stringify(expected)}`);
        },
      },
    },
  };
}
globalThis.expect = expect;

globalThis.test = function(name, fn) {
  Deno.core.ops.op_test_run(name);
  try {
    fn();
    Deno.core.ops.op_test_pass(name);
  } catch (e) {
    Deno.core.ops.op_test_fail(name, String(e));
  }
};

// ── require() module loader ───────────────────────────────────────────────────
globalThis.require = function(name) {
  const src = Deno.core.ops.op_require_module(name);
  if (!src) throw new Error(`Module not found: ${name}`);
  const mod = { exports: {} };
  const fn = new Function("module", "exports", "require", src);
  fn(mod, mod.exports, globalThis.require);
  return mod.exports;
};
```

- [ ] **Step 3: Verify compile**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "feat(rocket-infra): add OpState structs and JS bootstrap script"
```

---

## Task 3: `DenoScriptEngine` — runtime lifecycle + `ScriptEngine` impl

**Files:**
- Create: `crates/rocket-infra/src/scripting/engine.rs`
- Modify: `crates/rocket-infra/src/scripting/mod.rs`

- [ ] **Step 1: Create `crates/rocket-infra/src/scripting/engine.rs`**

```rust
use async_trait::async_trait;
use deno_core::{Extension, JsRuntime, RuntimeOptions, op2};
use rocket_scripting::{
    ScriptContext, ScriptEngine, ScriptPhase, ScriptResult, RequestMutations,
};
use rocket_shared::{DomainError, DomainResult};
use std::rc::Rc;
use std::cell::RefCell;

use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use crate::scripting::ops::{console, req, res, rok};

/// JS scripting engine backed by `deno_core` (V8).
///
/// Creates one `JsRuntime` per `execute()` call — complete isolation between requests.
/// No Deno standard library, no file system, no network — only the `rok`, `req`,
/// `res`, `console`, `test`, `expect`, and `require` globals defined in `bootstrap.js`.
pub struct DenoScriptEngine;

impl DenoScriptEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DenoScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScriptEngine for DenoScriptEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        // All deno_core work must happen on the same thread (JsRuntime is !Send).
        // We use spawn_blocking to avoid blocking the async executor.
        let result = tokio::task::spawn_blocking(move || run_script(ctx))
            .await
            .map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?;
        result
    }
}

fn build_extension() -> Extension {
    Extension {
        name: "rocket_scripting",
        ops: std::borrow::Cow::Borrowed(&[
            // rok ops (stubs until SP3-03)
            rok::op_rok_get_var(),
            // req ops (stubs until SP3-04)
            req::op_req_get_url(),
            // res ops (stubs until SP3-04)
            res::op_res_get_status(),
            // console ops (stubs until SP3-05)
            console::op_console_log(),
            // test runner ops — implemented here
            op_test_run(),
            op_test_pass(),
            op_test_fail(),
            op_require_module(),
        ]),
        ..Default::default()
    }
}

fn run_script(ctx: ScriptContext) -> DomainResult<ScriptResult> {
    let output = Rc::new(RefCell::new(ScriptOutputState::default()));

    let ext = build_extension();
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![ext],
        ..Default::default()
    });

    // Insert input + output state into OpState
    {
        let op_state = runtime.op_state();
        let mut state = op_state.borrow_mut();
        state.put(ScriptInputState {
            phase: ctx.phase.clone(),
            variables: ctx.variables.clone(),
            request: ctx.request.clone(),
            response: ctx.response.clone(),
            env_name: ctx.env_name.clone(),
            execution_mode: ctx.execution_mode.clone(),
            execution_platform: ctx.execution_platform.clone(),
        });
        state.put(ScriptOutputState::default());
    }

    // Run bootstrap
    const BOOTSTRAP: &str = include_str!("bootstrap.js");
    runtime.execute_script("<bootstrap>", BOOTSTRAP)
        .map_err(|e| DomainError::Internal(format!("bootstrap error: {e}")))?;

    // Run user code — catch script-level errors gracefully
    let script_error = match runtime.execute_script("<user>", &ctx.code) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    };

    // Extract output state
    let out = {
        let op_state = runtime.op_state();
        let mut state = op_state.borrow_mut();
        state.take::<ScriptOutputState>()
    };

    let request_mutations = if out.any_request_mutation {
        Some(out.request_mutations)
    } else {
        None
    };

    Ok(ScriptResult {
        request_mutations,
        runtime_vars: out.runtime_vars,
        env_var_writes: out.env_var_writes,
        collection_var_writes: out.collection_var_writes,
        global_env_var_writes: out.global_env_var_writes,
        next_request: out.next_request,
        skip_request: out.skip_request,
        test_results: out.test_results,
        console_entries: out.console_entries,
        error: script_error,
    })
}

// ── test runner ops ──────────────────────────────────────────────────────────

#[op2(fast)]
fn op_test_run(#[string] _name: String) {
    // No-op: test registration is just a marker for now.
}

#[op2(fast)]
fn op_test_pass(state: &mut deno_core::OpState, #[string] name: String) {
    state.borrow_mut::<ScriptOutputState>().add_test_result(name, true, None);
}

#[op2(fast)]
fn op_test_fail(
    state: &mut deno_core::OpState,
    #[string] name: String,
    #[string] error: String,
) {
    state.borrow_mut::<ScriptOutputState>().add_test_result(name, false, Some(error));
}

#[op2]
#[string]
fn op_require_module(#[string] name: String) -> String {
    // Returns the UMD source for the requested module, or empty string if unknown.
    // Module bundles are added in SP3-05.
    match name.as_str() {
        _ => String::new(),
    }
}
```

- [ ] **Step 2: Update `crates/rocket-infra/src/scripting/mod.rs`**

```rust
pub mod engine;
pub mod ops;
pub mod state;

pub use engine::DenoScriptEngine;
```

- [ ] **Step 3: Compile check**

```bash
cargo check -p rocket-infra 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

> **Note to subagent:** `deno_core` API details (e.g. `Extension` field names, `op2` macro syntax, `OpState` access patterns) may differ slightly between versions. Check the `deno_core` changelog for the pinned version and adjust accordingly. The intent is clear — one runtime per execution, bootstrap runs first, user code runs second, output state is extracted.

- [ ] **Step 4: Write a smoke test**

Add to `crates/rocket-infra/src/scripting/engine.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_scripting::ScriptPhase;
    use rocket_environment::VariableContext;

    fn minimal_ctx(code: &str) -> ScriptContext {
        ScriptContext {
            code: code.into(),
            phase: ScriptPhase::BeforeRequest,
            variables: VariableContext::default(),
            request: rocket_http::HttpRequest {
                name: "test".into(),
                url: "https://example.com".into(),
                method: rocket_http::HttpMethod::Get,
                ..Default::default()
            },
            response: None,
            env_name: None,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }

    #[tokio::test]
    async fn console_log_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("console.log('hello from script')");
        let result = engine.execute(ctx).await.unwrap();
        assert_eq!(result.console_entries.len(), 1);
        assert!(result.console_entries[0].message.contains("hello from script"));
    }

    #[tokio::test]
    async fn script_error_captured() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("throw new Error('deliberate')");
        let result = engine.execute(ctx).await.unwrap();
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("deliberate"));
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p rocket-infra scripting 2>&1 | tail -20
```

Expected: both smoke tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/scripting/
git commit -m "feat(rocket-infra): DenoScriptEngine skeleton — runtime lifecycle, test ops, smoke tests"
```
