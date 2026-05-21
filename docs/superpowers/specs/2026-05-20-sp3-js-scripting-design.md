# SP3 — JS Runtime Scripting Design Spec

**Date:** 2026-05-20
**Status:** Approved
**Scope:** Bruno-parity JS scripting via `rok` global, powered by `deno_core` (V8), executed Rust-side in a new `rocket-scripting` domain crate.
**Out of scope:** `rok.runRequest()` (deferred to SP4).

---

## 1. Goals

- Full Bruno scripting API parity, with `rok` as the global namespace (replacing `bru`).
- Scripts execute Rust-side via `deno_core` — isolated from the Tauri renderer process, no access to Tauri IPC.
- Follows the existing DDD bounded-context architecture: trait in domain crate, implementation in `rocket-infra`.
- `console.log` output routes to the existing Console tab in the bottom status bar.
- Script editor UI follows Bruno exactly: a "Scripts" tab with `Pre Request` / `Post Response` / `Tests` sub-tabs, plus a "Vars" tab for declarative variable expressions.

---

## 2. New Crate: `rocket-scripting`

A new domain crate added to the Cargo workspace. It owns:

- The `ScriptEngine` trait — the contract `rocket-app` depends on.
- `ScriptContext` — the input type passed into the engine before execution.
- `ScriptResult` — the output type returned after execution, carrying all mutations to apply.
- `ScriptPhase` — enum discriminating `BeforeRequest`, `AfterResponse`, `Tests`.
- `TestResult` — outcome of a single `rok.test()` assertion block.

### Dependency position

```
rocket-scripting
    ↓ imports from
rocket-environment   (VariableContext — 8 scopes)
rocket-http          (HttpRequest, HttpResponse)
rocket-shared        (DomainError, DomainResult)
```

`rocket-scripting` must **never** import `rocket-infra`, `rocket-app`, or `src-tauri`.

### Public API surface (`crates/rocket-scripting/src/lib.rs`)

```rust
pub mod engine;
pub mod context;
pub mod result;
pub mod phase;

pub use engine::ScriptEngine;
pub use context::ScriptContext;
pub use result::{ScriptResult, TestResult, TestStatus, ConsoleEntry};
pub use phase::ScriptPhase;
```

### `ScriptPhase`

```rust
pub enum ScriptPhase {
    BeforeRequest,
    AfterResponse,
    Tests,
}
```

### `ScriptContext`

Carries everything the JS sandbox needs to read at execution time. Immutable snapshot — the engine never writes back to these fields directly; mutations are returned via `ScriptResult`.

```rust
pub struct ScriptContext {
    /// The script source code to execute.
    pub code: String,
    /// Which lifecycle phase this script runs in.
    pub phase: ScriptPhase,
    /// Resolved variable scopes (read-only snapshot).
    pub variables: VariableContext,
    /// The outgoing request (read + mutable via req.set* in before-request).
    pub request: HttpRequest,
    /// The completed response (read-only; None in before-request phase).
    pub response: Option<HttpResponse>,
    /// Name of the currently active environment (for rok.getEnvName()).
    pub env_name: Option<String>,
    /// Execution mode: "runner" | "standalone"
    pub execution_mode: String,
    /// Execution platform: always "app" for desktop
    pub execution_platform: String,
}
```

### `ScriptResult`

All side-effects the script produced, returned as a value — nothing is applied directly inside the engine.

```rust
pub struct ScriptResult {
    /// Mutations to the outgoing request (only populated from before-request).
    pub request_mutations: Option<RequestMutations>,
    /// Runtime variable writes (rok.setVar).
    pub runtime_vars: HashMap<String, serde_json::Value>,
    /// Environment variable writes (rok.setEnvVar).
    pub env_var_writes: Vec<EnvVarWrite>,
    /// Collection variable writes (rok.setCollectionVar).
    pub collection_var_writes: Vec<CollectionVarWrite>,
    /// Global env variable writes (rok.setGlobalEnvVar).
    pub global_env_var_writes: Vec<EnvVarWrite>,
    /// Next request to execute (runner only) — None = no override.
    pub next_request: Option<NextRequest>,
    /// Whether to skip this request in the runner.
    pub skip_request: bool,
    /// Test assertion results (populated from tests phase).
    pub test_results: Vec<TestResult>,
    /// console.log / console.error entries, in order.
    pub console_entries: Vec<ConsoleEntry>,
    /// Script-level error, if execution threw.
    pub error: Option<String>,
}

pub struct RequestMutations {
    pub url: Option<String>,
    pub method: Option<String>,
    pub headers_set: HashMap<String, String>,
    pub headers_deleted: Vec<String>,
    pub body: Option<serde_json::Value>,
    pub timeout_ms: Option<u64>,
    pub max_redirects: Option<u32>,
}

pub struct EnvVarWrite {
    pub key: String,
    pub value: serde_json::Value,
    /// When true, write is persisted to the .yml file.
    pub persist: bool,
}

pub struct CollectionVarWrite {
    pub key: String,
    pub value: serde_json::Value,
}

pub enum NextRequest {
    /// Jump to request with this display name.
    Name(String),
    /// Null — stop the runner.
    Stop,
}

pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub error: Option<String>,
}

pub enum TestStatus {
    Passed,
    Failed,
}

pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub message: String,
}

pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
}
```

### `ScriptEngine` trait

```rust
#[async_trait::async_trait]
pub trait ScriptEngine: Send + Sync {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult>;
}
```

---

## 3. `rocket-infra`: `DenoScriptEngine`

`rocket-infra` adds a `scripting/` module that implements `ScriptEngine` using `deno_core`.

### Cargo dependency

```toml
[dependencies]
deno_core = "0.x"   # pin to latest stable at implementation time
```

### Runtime setup

- One `deno_core::JsRuntime` is created **per script execution** (not shared). This ensures complete isolation between requests and prevents state leakage.
- The runtime is created with a custom set of extensions that expose only the `rok`, `req`, `res`, and `console` ops — no Deno standard library, no file system, no network, no Tauri IPC.
- `deno_core::op2` macros define each host-side function (e.g. `op_rok_set_var`, `op_req_set_header`).
- All ops are synchronous from JS's perspective except future SP4 additions. `async/await` and `Promise` work normally for user code.

### Inbuilt `require()` libraries

Bruno's inbuilt libraries are bundled as pre-compiled JS modules resolved via a custom module loader (no Node.js `node_modules` on disk). Libraries available without developer mode:

| `require()` name | Implementation |
|---|---|
| `chai` | bundled UMD build |
| `crypto-js` | bundled UMD build |
| `jsonwebtoken` | bundled UMD build |
| `uuid` | bundled UMD build |
| `moment` | bundled UMD build |
| `axios` | bundled UMD build (calls Rust HTTP via op) |
| `atob` / `btoa` | native V8 globals (no bundle needed) |
| `nanoid` | bundled UMD build |
| `tv4` | bundled UMD build |

Libraries are embedded as `include_str!()` byte strings compiled into the binary — no runtime file reads.

### `console` capture

`console.log`, `console.warn`, and `console.error` are intercepted via a custom `console` object injected into the JS global scope. Each call appends to a `Vec<ConsoleEntry>` held in `deno_core::OpState`, which is extracted into `ScriptResult` after execution.

---

## 4. `rok` API — Full Surface

All functions are synchronous from JS. Each maps to a Rust op.

### Variable API

| JS call | Rust op | Behaviour |
|---|---|---|
| `rok.getVar(key)` | `op_rok_get_var` | Read from `ScriptContext.variables.runtime` |
| `rok.setVar(key, value)` | `op_rok_set_var` | Append to `ScriptResult.runtime_vars` |
| `rok.getEnvVar(key)` | `op_rok_get_env_var` | Read from `ScriptContext.variables.env` |
| `rok.setEnvVar(key, value, opts?)` | `op_rok_set_env_var` | Append to `ScriptResult.env_var_writes`; `opts.persist` sets the persist flag |
| `rok.hasEnvVar(key)` | `op_rok_has_env_var` | Returns bool |
| `rok.deleteEnvVar(key)` | `op_rok_delete_env_var` | Appends a tombstone write with `value: null` |
| `rok.getEnvName()` | `op_rok_get_env_name` | Returns `ScriptContext.env_name` |
| `rok.getCollectionVar(key)` | `op_rok_get_collection_var` | Read from `ScriptContext.variables.collection` |
| `rok.setCollectionVar(key, value)` | `op_rok_set_collection_var` | Append to `ScriptResult.collection_var_writes` |
| `rok.getGlobalEnvVar(key)` | `op_rok_get_global_env_var` | Read from `ScriptContext.variables.global_env` |
| `rok.setGlobalEnvVar(key, value)` | `op_rok_set_global_env_var` | Append to `ScriptResult.global_env_var_writes` |
| `rok.interpolate(template)` | `op_rok_interpolate` | Resolves `{{var}}` tokens using the flattened variable context |

### Runner API

| JS call | Rust op | Behaviour |
|---|---|---|
| `rok.runner.setNextRequest(name)` | `op_rok_set_next_request` | Sets `ScriptResult.next_request = NextRequest::Name(name)` |
| `rok.runner.setNextRequest(null)` | `op_rok_set_next_request` | Sets `ScriptResult.next_request = NextRequest::Stop` |
| `rok.runner.skipRequest()` | `op_rok_skip_request` | Sets `ScriptResult.skip_request = true` |

### `req` object — `before-request` only

| JS call | Rust op | Notes |
|---|---|---|
| `req.getUrl()` | `op_req_get_url` | |
| `req.setUrl(url)` | `op_req_set_url` | Writes to mutation state |
| `req.getHost()` | `op_req_get_host` | Parsed from URL |
| `req.getPath()` | `op_req_get_path` | Parsed from URL |
| `req.getQueryString()` | `op_req_get_query_string` | Parsed from URL |
| `req.getPathParams()` | `op_req_get_path_params` | Returns array of `{name, value, type}` |
| `req.getMethod()` | `op_req_get_method` | |
| `req.setMethod(method)` | `op_req_set_method` | |
| `req.getName()` | `op_req_get_name` | |
| `req.getTags()` | `op_req_get_tags` | Returns `string[]` |
| `req.getAuthMode()` | `op_req_get_auth_mode` | Returns auth type string |
| `req.getHeader(name)` | `op_req_get_header` | Case-insensitive |
| `req.getHeaders()` | `op_req_get_headers` | Returns `Record<string,string>` |
| `req.setHeader(name, value)` | `op_req_set_header` | |
| `req.setHeaders(headers)` | `op_req_set_headers` | |
| `req.deleteHeader(name)` | `op_req_delete_header` | |
| `req.deleteHeaders([names])` | `op_req_delete_headers` | |
| `req.getBody(opts?)` | `op_req_get_body` | `{raw:true}` returns string |
| `req.setBody(body)` | `op_req_set_body` | |
| `req.getTimeout()` | `op_req_get_timeout` | |
| `req.setTimeout(ms)` | `op_req_set_timeout` | |
| `req.setMaxRedirects(n)` | `op_req_set_max_redirects` | |
| `req.getExecutionMode()` | `op_req_get_execution_mode` | `"runner"` or `"standalone"` |
| `req.getExecutionPlatform()` | `op_req_get_execution_platform` | Always `"app"` |
| `req.onFail(callback)` | `op_req_on_fail` | Registers error handler; no-op in safe mode |

`req.set*` calls in `after-response` or `tests` phases throw a JS `Error` with a clear message: `"req mutations are not allowed in after-response/tests scripts"`.

### `res` object — `after-response` and `tests` only

| JS call | Rust op | Notes |
|---|---|---|
| `res.getStatus()` | `op_res_get_status` | Returns HTTP status code as number |
| `res.getStatusText()` | `op_res_get_status_text` | |
| `res.getHeader(name)` | `op_res_get_header` | |
| `res.getHeaders()` | `op_res_get_headers` | |
| `res.getBody(opts?)` | `op_res_get_body` | Auto-parses JSON; `{raw:true}` for string |
| `res.getResponseTime()` | `op_res_get_response_time` | Milliseconds |

`res` in `before-request` phase is `undefined` — accessing it throws: `"res is not available in before-request scripts"`.

### `test()` + `expect()` — `tests` phase only

`test(name, fn)` is a global function that registers an assertion block. Chai's `expect` is available as a global.

```js
test("status is 200", () => {
  expect(res.getStatus()).to.equal(200);
});

test("body has token", () => {
  const body = res.getBody();
  expect(body).to.have.property("token");
});
```

Each `test()` call is caught independently — one failing assertion does not prevent subsequent tests from running. Results accumulate in `ScriptResult.test_results`.

---

## 5. `rocket-app`: `HttpService` integration

`HttpService` receives a `Box<dyn ScriptEngine>` via constructor injection. The send pipeline becomes:

```
1. Resolve VariableContext (all 8 scopes)
2. Build ScriptContext { phase: BeforeRequest, ... }
3. engine.execute(ctx) → ScriptResult
4. Apply RequestMutations to the outgoing HttpRequest
5. Apply runtime_vars to in-memory VariableContext
6. Execute HTTP request → HttpResponse
7. Build ScriptContext { phase: AfterResponse, response: Some(...), ... }
8. engine.execute(ctx) → ScriptResult
9. Apply env_var_writes, collection_var_writes (persist if flagged)
10. Build ScriptContext { phase: Tests, ... }
11. engine.execute(ctx) → ScriptResult
12. Collect all ConsoleEntry from all three ScriptResults → emit DomainEvent::ConsoleOutput
13. Collect TestResult list → emit DomainEvent::TestsCompleted
14. Return response to caller
```

Steps 3, 8, and 11 are skipped if the request has no script of that type in `runtime.scripts`.

### New `DomainEvent` variants

```rust
DomainEvent::ConsoleOutput {
    request_name: String,
    entries: Vec<ConsoleEntry>,
}

DomainEvent::TestsCompleted {
    request_name: String,
    results: Vec<TestResult>,
}

DomainEvent::ScriptError {
    request_name: String,
    phase: String,
    message: String,
}
```

---

## 6. Tauri IPC changes

### New commands

No new send-time commands are needed — scripting executes synchronously inside the existing `execute_request` command flow.

One new command for the Vars tab declarative expressions:

```rust
// Evaluate a single JS expression against the current variable context
// Used by the Vars tab post-response section
evaluate_var_expression(
    collection_root: String,
    expression: String,
    response_json: String,
) -> Result<serde_json::Value, DomainError>
```

### Existing `execute_request` response

The IPC return type for `execute_request` gains two new fields:

```typescript
export interface ExecuteRequestResponse {
  // ... existing fields ...
  testResults: TestResult[]
  consoleEntries: ConsoleEntry[]
  scriptError: string | null
}

export interface TestResult {
  name: string
  status: 'passed' | 'failed'
  error: string | null
}

export interface ConsoleEntry {
  level: 'log' | 'warn' | 'error'
  message: string
}
```

---

## 7. Frontend

### Scripts tab

The request editor gains a "Scripts" tab alongside Body, Headers, Params, Auth, Vars. Inside Scripts, three sub-tabs:

| Sub-tab | Phase | `req` writable | `res` available |
|---|---|---|---|
| Pre Request | `before-request` | ✓ | ✗ |
| Post Response | `after-response` | ✗ | ✓ |
| Tests | `tests` | ✗ | ✓ |

Each sub-tab contains a Monaco editor (multi-line) with JS syntax highlighting. The editor is connected to the `runtime.scripts` array on the request — saving persists to the `.yml` file via the existing save pipeline.

### Vars tab

Separate top-level "Vars" tab (Bruno parity). Two sections:

- **Pre Request** — key/value table. Values are JS literals evaluated before the request. Stored as `runtime.variables` in the `.yml`.
- **Post Response** — key/value table. Values are JS expressions evaluated against `res`. Stored as `runtime.actions` (type: `set-variable`, phase: `after-response`).

### Tests panel in response area

After a request completes, the response area gains a "Tests" tab showing:

- Pass/fail count badge.
- List of `TestResult` entries: green check / red X, test name, error message if failed.

### Console output

`ConsoleEntry` items are forwarded to the existing Console tab in the bottom status bar. Each entry is prefixed with the request name and phase: `[Create User / tests] Token exists: abc123`.

---

## 8. Variable resolution after script execution

Mutations returned in `ScriptResult` are applied in this order before the response is returned to the frontend:

1. `runtime_vars` → stored in `VariableContext.runtime` (in-memory only, cleared after the request completes).
2. `env_var_writes` with `persist: false` → update active environment in memory only.
3. `env_var_writes` with `persist: true` → update active environment in memory AND write to the environment `.yml` file via `FsEnvironmentRepo`.
4. `collection_var_writes` → update the collection's variable map in memory AND write to `opencollection.yml` via `FsCollectionRepo`.
5. `global_env_var_writes` → update global env in memory AND write to the global env `.yml`.

---

## 9. Security model

- The `deno_core` runtime is created with no Deno permissions — no file system, no network (scripts cannot open their own HTTP connections), no `Deno` global, no `process` global.
- `axios` and `node-fetch` inbuilt libraries **are** available and route through a Rust op (`op_axios_request`) that calls `reqwest` internally — scripts can make HTTP calls but only via the controlled op surface, not via raw sockets.
- No access to `window.__TAURI__` or any Tauri IPC internals.
- Script execution is time-limited: a configurable timeout (default 30s) kills the runtime and returns a `ScriptError`.
- One runtime per execution — no shared global state between requests.

---

## 10. New crate in Cargo workspace

`Cargo.toml` workspace `members` gains:

```toml
"crates/rocket-scripting",
```

`rocket-app/Cargo.toml` gains:

```toml
rocket-scripting = { path = "../rocket-scripting" }
```

`rocket-infra/Cargo.toml` gains:

```toml
rocket-scripting = { path = "../rocket-scripting" }
deno_core = "0.x"
```

---

## 11. Files changed / created

### New files

| File | Purpose |
|---|---|
| `crates/rocket-scripting/Cargo.toml` | New crate manifest |
| `crates/rocket-scripting/src/lib.rs` | Public exports |
| `crates/rocket-scripting/src/engine.rs` | `ScriptEngine` trait |
| `crates/rocket-scripting/src/context.rs` | `ScriptContext` |
| `crates/rocket-scripting/src/result.rs` | `ScriptResult`, `TestResult`, `ConsoleEntry` |
| `crates/rocket-scripting/src/phase.rs` | `ScriptPhase` |
| `crates/rocket-infra/src/scripting/mod.rs` | `DenoScriptEngine` impl |
| `crates/rocket-infra/src/scripting/runtime.rs` | `deno_core` runtime setup, op registration |
| `crates/rocket-infra/src/scripting/ops/rok.rs` | `rok.*` ops |
| `crates/rocket-infra/src/scripting/ops/req.rs` | `req.*` ops |
| `crates/rocket-infra/src/scripting/ops/res.rs` | `res.*` ops |
| `crates/rocket-infra/src/scripting/ops/console.rs` | `console.*` ops |
| `crates/rocket-infra/src/scripting/modules/` | Bundled inbuilt JS libraries |
| `frontend/src/components/request/ScriptsTab.tsx` | Scripts tab with sub-tabs |
| `frontend/src/components/request/VarsTab.tsx` | Vars tab (pre/post variable expressions) |
| `frontend/src/components/response/TestsPanel.tsx` | Tests results view |
| `frontend/src/types/scripting.ts` | `TestResult`, `ConsoleEntry` TS types |

### Modified files

| File | Change |
|---|---|
| `Cargo.toml` | Add `rocket-scripting` to workspace members |
| `crates/rocket-app/Cargo.toml` | Add `rocket-scripting` dep |
| `crates/rocket-infra/Cargo.toml` | Add `rocket-scripting`, `deno_core` deps |
| `crates/rocket-app/src/http_service.rs` | Inject `Box<dyn ScriptEngine>`, add 3-phase execution |
| `crates/rocket-shared/src/events.rs` | Add `ConsoleOutput`, `TestsCompleted`, `ScriptError` variants |
| `src-tauri/src/lib.rs` | Wire `DenoScriptEngine` into `HttpService` DI |
| `src-tauri/src/commands/http.rs` | Extend `ExecuteRequestResponse` with `testResults`, `consoleEntries`, `scriptError` |
| `frontend/src/lib/tauri-api.ts` | Update `ExecuteRequestResponse` type; add `evaluateVarExpression` |
| `frontend/src/components/request/RequestEditor.tsx` | Add Scripts and Vars tabs |
| `frontend/src/components/response/ResponsePanel.tsx` | Add Tests tab |
| `frontend/src/stores/consoleStore.ts` | Handle `ConsoleOutput` events from scripting |
| `crates/rocket-scripting/CLAUDE.md` | New crate documentation |

---

## 12. Out of scope (deferred)

| Feature | Milestone |
|---|---|
| `rok.runRequest(path)` — async request chaining | SP4 |
| External library loading (`npm install` in collection) | Future |
| TypeScript script support | Future |
| Script file references (`OcScriptFile`) | Future |
| Prompt variables `{{prompt.VAR}}` | Phase 2 |
| Collection runner scope isolation | SP3 collection runner (separate plan) |
