# SP3-01 — `rocket-scripting` Domain Crate

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the `rocket-scripting` domain crate — the `ScriptEngine` trait, `ScriptContext`, `ScriptResult`, `ScriptPhase`, and all supporting types that define the scripting contract.

**Architecture:** New crate at `crates/rocket-scripting/`. Depends only on `rocket-shared`, `rocket-environment`, and `rocket-http`. Never imports `rocket-infra`, `rocket-app`, or `src-tauri`. `rocket-app` and `rocket-infra` will both depend on this crate in later plans.

**Tech Stack:** Rust 2021, `async-trait`, `serde`, `serde_json`

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §2

---

## Task 1: Scaffold crate + `ScriptPhase` + `ScriptEngine` trait

**Files:**
- Create: `crates/rocket-scripting/Cargo.toml`
- Create: `crates/rocket-scripting/src/lib.rs`
- Create: `crates/rocket-scripting/src/phase.rs`
- Create: `crates/rocket-scripting/src/engine.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add crate to workspace**

Open `Cargo.toml` at the repo root. Find the `[workspace]` `members` array and add:

```toml
"crates/rocket-scripting",
```

- [ ] **Step 2: Create `crates/rocket-scripting/Cargo.toml`**

```toml
[package]
name = "rocket-scripting"
version = "0.1.0"
edition = "2021"

[dependencies]
rocket-shared = { path = "../rocket-shared" }
rocket-environment = { path = "../rocket-environment" }
rocket-http = { path = "../rocket-http" }
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
```

- [ ] **Step 3: Create `crates/rocket-scripting/src/phase.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Identifies which lifecycle hook a script belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptPhase {
    /// Runs before the HTTP request is sent. `req` is writable, `res` is unavailable.
    BeforeRequest,
    /// Runs after the HTTP response is received. `res` is read-only, `req` mutations are rejected.
    AfterResponse,
    /// Runs after after-response. `res` is read-only. `test()` + `expect()` are available.
    Tests,
}

impl ScriptPhase {
    /// Returns the canonical string name used in `OcScript.script_type`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScriptPhase::BeforeRequest => "before-request",
            ScriptPhase::AfterResponse => "after-response",
            ScriptPhase::Tests => "tests",
        }
    }
}

impl std::fmt::Display for ScriptPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_as_str() {
        assert_eq!(ScriptPhase::BeforeRequest.as_str(), "before-request");
        assert_eq!(ScriptPhase::AfterResponse.as_str(), "after-response");
        assert_eq!(ScriptPhase::Tests.as_str(), "tests");
    }

    #[test]
    fn phase_display() {
        assert_eq!(ScriptPhase::Tests.to_string(), "tests");
    }
}
```

- [ ] **Step 4: Create `crates/rocket-scripting/src/engine.rs`**

```rust
use async_trait::async_trait;
use rocket_shared::DomainResult;
use crate::{ScriptContext, ScriptResult};

/// Contract for a JS script execution engine.
///
/// `rocket-infra` provides `DenoScriptEngine` which implements this using `deno_core`.
/// `rocket-app` depends on this trait via `Box<dyn ScriptEngine>` — it never
/// constructs `DenoScriptEngine` directly.
#[async_trait]
pub trait ScriptEngine: Send + Sync {
    /// Execute `ctx.code` in a sandboxed JS runtime for the given lifecycle phase.
    ///
    /// Returns a `ScriptResult` carrying all side-effects to apply (variable mutations,
    /// request mutations, test outcomes, console entries). The engine itself applies
    /// nothing — callers apply mutations after this call returns.
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult>;
}
```

- [ ] **Step 5: Create stub `crates/rocket-scripting/src/lib.rs`** (will be fleshed out in Task 2-3)

```rust
pub mod context;
pub mod engine;
pub mod phase;
pub mod result;

pub use context::ScriptContext;
pub use engine::ScriptEngine;
pub use phase::ScriptPhase;
pub use result::{
    CollectionVarWrite, ConsoleEntry, ConsoleLevel, EnvVarWrite, NextRequest,
    RequestMutations, ScriptResult, TestResult, TestStatus,
};
```

- [ ] **Step 6: Verify compile (context + result are stubs — will error until Task 2)**

```bash
cargo check -p rocket-scripting 2>&1 | grep "^error" | head -20
```

Expected: errors about missing `context` and `result` modules. That is correct at this stage — proceed.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-scripting/ Cargo.toml
git commit -m "feat(rocket-scripting): scaffold crate, ScriptPhase, ScriptEngine trait"
```

---

## Task 2: `ScriptResult` and all supporting output types

**Files:**
- Create: `crates/rocket-scripting/src/result.rs`

- [ ] **Step 1: Create `crates/rocket-scripting/src/result.rs`**

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// All side-effects produced by a single script execution.
///
/// Nothing in this struct is applied automatically — callers (in `rocket-app`)
/// read these fields and apply mutations to the request, variable context, and
/// persistence layer after the engine returns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptResult {
    /// Mutations to the outgoing request. Only meaningful from `BeforeRequest` phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_mutations: Option<RequestMutations>,

    /// Runtime variable writes via `rok.setVar(key, value)`.
    /// Cleared after the request completes — never persisted to disk.
    #[serde(default)]
    pub runtime_vars: HashMap<String, serde_json::Value>,

    /// Environment variable writes via `rok.setEnvVar(key, value, opts?)`.
    #[serde(default)]
    pub env_var_writes: Vec<EnvVarWrite>,

    /// Collection variable writes via `rok.setCollectionVar(key, value)`.
    #[serde(default)]
    pub collection_var_writes: Vec<CollectionVarWrite>,

    /// Global environment variable writes via `rok.setGlobalEnvVar(key, value)`.
    #[serde(default)]
    pub global_env_var_writes: Vec<EnvVarWrite>,

    /// Next request to run in a collection runner. `None` = no override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_request: Option<NextRequest>,

    /// When `true`, the runner skips this request entirely.
    #[serde(default)]
    pub skip_request: bool,

    /// `rok.test()` assertion outcomes. Populated from the `Tests` phase.
    #[serde(default)]
    pub test_results: Vec<TestResult>,

    /// `console.log/warn/error` entries emitted during execution, in order.
    #[serde(default)]
    pub console_entries: Vec<ConsoleEntry>,

    /// Script-level error message if execution threw an uncaught exception.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Mutations that a `before-request` script can apply to the outgoing `HttpRequest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMutations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,

    /// Headers to add or overwrite (key = name, value = value).
    #[serde(default)]
    pub headers_set: HashMap<String, String>,

    /// Header names to remove.
    #[serde(default)]
    pub headers_deleted: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
}

/// A single variable write to an environment scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarWrite {
    pub key: String,
    /// JSON value. `null` = delete.
    pub value: serde_json::Value,
    /// When `true`, write is persisted to the environment `.yml` file.
    pub persist: bool,
}

/// A single variable write to the collection's variable map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionVarWrite {
    pub key: String,
    /// JSON value. `null` = delete.
    pub value: serde_json::Value,
}

/// Controls the next request to execute in a collection runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NextRequest {
    /// Jump to the request with this display name.
    Name(String),
    /// Stop the runner immediately.
    Stop,
}

/// The outcome of a single `rok.test(name, fn)` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Display name passed to `rok.test(name, ...)`.
    pub name: String,
    pub status: TestStatus,
    /// Error message if the assertion failed or threw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Pass/fail status for a single test assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    Failed,
}

/// A single `console.log`, `console.warn`, or `console.error` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub message: String,
}

/// Severity level of a console entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_result_default_is_empty() {
        let r = ScriptResult::default();
        assert!(r.request_mutations.is_none());
        assert!(r.runtime_vars.is_empty());
        assert!(r.env_var_writes.is_empty());
        assert!(r.test_results.is_empty());
        assert!(r.console_entries.is_empty());
        assert!(r.error.is_none());
    }

    #[test]
    fn env_var_write_serde_roundtrip() {
        let w = EnvVarWrite {
            key: "TOKEN".into(),
            value: serde_json::json!("abc123"),
            persist: true,
        };
        let json = serde_json::to_string(&w).unwrap();
        let back: EnvVarWrite = serde_json::from_str(&json).unwrap();
        assert_eq!(back.key, "TOKEN");
        assert_eq!(back.persist, true);
    }

    #[test]
    fn test_result_serde_roundtrip() {
        let t = TestResult {
            name: "status is 200".into(),
            status: TestStatus::Failed,
            error: Some("expected 200 got 404".into()),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: TestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "status is 200");
        assert_eq!(back.status, TestStatus::Failed);
    }

    #[test]
    fn next_request_serde_roundtrip() {
        let n = NextRequest::Name("Poll Status".into());
        let json = serde_json::to_string(&n).unwrap();
        let back: NextRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, NextRequest::Name(s) if s == "Poll Status"));
    }
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p rocket-scripting 2>&1 | grep "^error" | head -20
```

Expected: errors about missing `context` module only. `result` should compile clean.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-scripting/src/result.rs
git commit -m "feat(rocket-scripting): add ScriptResult and all output types"
```

---

## Task 3: `ScriptContext` + tests + CLAUDE.md

**Files:**
- Create: `crates/rocket-scripting/src/context.rs`
- Create: `crates/rocket-scripting/CLAUDE.md`

- [ ] **Step 1: Create `crates/rocket-scripting/src/context.rs`**

```rust
use rocket_environment::VariableContext;
use rocket_http::{HttpRequest, HttpResponse};
use crate::ScriptPhase;

/// Everything the JS sandbox needs to read at execution time.
///
/// This is a snapshot — immutable once constructed. The engine returns
/// `ScriptResult` carrying any mutations; it never writes back to this struct.
#[derive(Debug, Clone)]
pub struct ScriptContext {
    /// The JavaScript source code to execute.
    pub code: String,

    /// Which lifecycle phase this script runs in.
    pub phase: ScriptPhase,

    /// Resolved variable scopes (read-only snapshot at send time).
    pub variables: VariableContext,

    /// The outgoing HTTP request. In `BeforeRequest` phase, `req.set*` mutations
    /// are collected into `ScriptResult.request_mutations`. In later phases,
    /// `req.set*` calls return a JS error.
    pub request: HttpRequest,

    /// The completed HTTP response. `None` in `BeforeRequest` phase.
    /// Accessing `res` in `BeforeRequest` throws a JS error.
    pub response: Option<HttpResponse>,

    /// Name of the currently active environment, for `rok.getEnvName()`.
    pub env_name: Option<String>,

    /// `"runner"` when executing inside a collection run, `"standalone"` otherwise.
    pub execution_mode: String,

    /// Always `"app"` for the desktop app.
    pub execution_platform: String,
}

impl ScriptContext {
    /// Convenience constructor for a `BeforeRequest` context.
    pub fn before_request(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        env_name: Option<String>,
    ) -> Self {
        Self {
            code,
            phase: ScriptPhase::BeforeRequest,
            variables,
            request,
            response: None,
            env_name,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }

    /// Convenience constructor for an `AfterResponse` context.
    pub fn after_response(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        response: HttpResponse,
        env_name: Option<String>,
    ) -> Self {
        Self {
            code,
            phase: ScriptPhase::AfterResponse,
            variables,
            request,
            response: Some(response),
            env_name,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }

    /// Convenience constructor for a `Tests` context.
    pub fn tests(
        code: String,
        variables: VariableContext,
        request: HttpRequest,
        response: HttpResponse,
        env_name: Option<String>,
    ) -> Self {
        Self {
            code,
            phase: ScriptPhase::Tests,
            variables,
            request,
            response: Some(response),
            env_name,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::VariableContext;
    use rocket_http::HttpRequest;

    fn stub_request() -> HttpRequest {
        HttpRequest {
            name: "Test Request".into(),
            url: "https://example.com".into(),
            method: rocket_http::HttpMethod::Get,
            ..Default::default()
        }
    }

    fn empty_vars() -> VariableContext {
        VariableContext::default()
    }

    #[test]
    fn before_request_has_no_response() {
        let ctx = ScriptContext::before_request(
            "console.log('hi')".into(),
            empty_vars(),
            stub_request(),
            None,
        );
        assert_eq!(ctx.phase, ScriptPhase::BeforeRequest);
        assert!(ctx.response.is_none());
        assert_eq!(ctx.execution_platform, "app");
    }

    #[test]
    fn after_response_has_response() {
        let response = HttpResponse {
            status: 200,
            ..Default::default()
        };
        let ctx = ScriptContext::after_response(
            "".into(),
            empty_vars(),
            stub_request(),
            response,
            Some("dev".into()),
        );
        assert_eq!(ctx.phase, ScriptPhase::AfterResponse);
        assert!(ctx.response.is_some());
        assert_eq!(ctx.env_name, Some("dev".into()));
    }
}
```

> **Note to subagent:** `HttpRequest` and `HttpResponse` must use `Default::default()` — check the actual field names in `crates/rocket-http/src/` before writing test stubs. Adjust the `stub_request()` and `HttpResponse` construction to match the real structs.

- [ ] **Step 2: Run all tests in the crate**

```bash
cargo test -p rocket-scripting 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 3: Create `crates/rocket-scripting/CLAUDE.md`**

```markdown
# rocket-scripting

Domain crate that defines the **JS scripting contract** for RocketAPI.

## Purpose

Owns the `ScriptEngine` trait, `ScriptContext` (input), `ScriptResult` (output),
`ScriptPhase`, and all supporting types. Does NOT contain any JS engine code.

## Dependency rule

Only imports from:
- `rocket-shared` (DomainError, DomainResult)
- `rocket-environment` (VariableContext)
- `rocket-http` (HttpRequest, HttpResponse)

Never imports `rocket-infra`, `rocket-app`, or `src-tauri`.

## Key types

| Type | File | Purpose |
|---|---|---|
| `ScriptEngine` | `engine.rs` | Async trait — `execute(ctx) -> DomainResult<ScriptResult>` |
| `ScriptContext` | `context.rs` | Immutable input snapshot passed to engine |
| `ScriptResult` | `result.rs` | All side-effects to apply after execution |
| `ScriptPhase` | `phase.rs` | `BeforeRequest` / `AfterResponse` / `Tests` |
| `RequestMutations` | `result.rs` | `req.set*` changes from before-request scripts |
| `EnvVarWrite` | `result.rs` | `rok.setEnvVar` writes; `persist` flag controls disk write |
| `TestResult` | `result.rs` | Outcome of a single `rok.test()` block |
| `ConsoleEntry` | `result.rs` | Captured `console.log/warn/error` output |

## Execution model

`DenoScriptEngine` in `rocket-infra` implements `ScriptEngine`.
`HttpService` in `rocket-app` calls `engine.execute(ctx)` three times per send:
1. `BeforeRequest` — may mutate the outgoing request
2. `AfterResponse` — may write env/collection vars
3. `Tests` — collects `TestResult` entries

Mutations in `ScriptResult` are applied by `HttpService` after each call returns.
The engine never applies anything itself.
```

- [ ] **Step 4: Final compile check**

```bash
cargo check -p rocket-scripting 2>&1 | grep "^error" | head -20
```

Expected: zero errors.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-scripting/src/context.rs crates/rocket-scripting/CLAUDE.md
git commit -m "feat(rocket-scripting): add ScriptContext, convenience constructors, CLAUDE.md"
```
