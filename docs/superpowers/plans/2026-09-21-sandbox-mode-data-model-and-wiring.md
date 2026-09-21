# Sandbox Mode Data Model and Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-collection `sandbox_mode` setting (Safe/Developer) that persists to `collection.json` and flows through to every JS script phase's `ScriptContext`, with no behavior change yet — the engine doesn't act on it until a later plan in this sequence.

**Architecture:** Two small mirrored enums, one per domain crate (`rocket_collection::SandboxMode` on `CollectionSettings`, `rocket_scripting::SandboxMode` on `ScriptContext`), kept deliberately separate to preserve crate decoupling. `rocket-app`'s `RequestExecutionService` reads the setting once per `execute()` call (in `begin_phases`, alongside the other per-execution context it already builds) and threads it onto every phase's `ScriptContext`, mirroring the existing `ExecutionMode` pattern exactly.

**Tech Stack:** Rust (rocket-collection, rocket-scripting, rocket-app crates), `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-21-sandbox-developer-mode-design.md`

## Global Constraints

- `sandbox_mode` defaults to `Safe` via `#[serde(default)]` — an existing `collection.json` with no such field must still deserialize correctly (spec §1, acceptance criteria).
- `rocket_collection::SandboxMode` and `rocket_scripting::SandboxMode` are separate types by design — do not have one crate depend on the other's enum (spec §4).
- No engine/op behavior changes in this plan — `sandbox_mode` is plumbed through but not yet acted on. That's Plans 2-3 in this sequence.
- Rust: avoid `.unwrap` panics in production code paths; test code uses `.expect("message")` for fallible setup calls.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-21-sandbox-fs-ops.md` next (plan 2 of 4 in the sandbox-developer-mode sequence).

---

### Task 1: `SandboxMode` on `CollectionSettings`

**Files:**
- Modify: `crates/rocket-collection/src/settings.rs:20-40` (new enum, new field)
- Test: `crates/rocket-collection/src/settings.rs` (same file, `#[cfg(test)] mod tests` at end)

**Interfaces:**
- Produces: `rocket_collection::settings::SandboxMode` (`Safe` default / `Developer`, `Serialize`/`Deserialize`, `#[serde(rename_all = "lowercase")]`), and `CollectionSettings.sandbox_mode: SandboxMode` — consumed by Task 3 (this plan) and by the frontend plan later in this sequence.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the end of `crates/rocket-collection/src/settings.rs` (after the existing `merge_three_levels_inner_wins` test):

```rust
    #[test]
    fn sandbox_mode_defaults_to_safe_when_absent_from_json() {
        let json = r#"{"headers":[],"variables":[]}"#;
        let settings: CollectionSettings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(settings.sandbox_mode, SandboxMode::Safe);
    }

    #[test]
    fn sandbox_mode_developer_roundtrips_as_camel_case() {
        let settings = CollectionSettings {
            sandbox_mode: SandboxMode::Developer,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(
            json.contains(r#""sandboxMode":"developer""#),
            "expected camelCase sandboxMode field, got {json}"
        );
        let round: CollectionSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.sandbox_mode, SandboxMode::Developer);
    }
```

Run: `cargo test -p rocket-collection sandbox_mode_defaults_to_safe_when_absent_from_json sandbox_mode_developer_roundtrips_as_camel_case`
Expected: FAIL to compile — `no field 'sandbox_mode' on type 'CollectionSettings'` and `cannot find type 'SandboxMode'`.

- [ ] **Step 2: Add the enum and field**

In `crates/rocket-collection/src/settings.rs`, find (lines 20-40):

```rust
/// A collection-scoped variable (like Postman/Bruno collection variables).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionVariable {
    pub key: String,
    pub value: String,
    /// Initial/default value committed to Git; fallback when value is empty.
    #[serde(default)]
    pub initial_value: String,
    pub enabled: bool,
    /// Mark as secret to hide in the UI (like Bruno).
    #[serde(default)]
    pub secret: bool,
}

/// Per-collection default auth, headers, and variables, stored in opencollection.yml.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSettings {
    /// Markdown documentation for this collection (maps to `docs:` in opencollection.yml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,

    /// Optional auth applied to all requests in this collection.
    #[serde(default)]
    pub auth: Option<Auth>,

    /// Default headers prepended to every request in this collection.
    #[serde(default)]
    pub headers: Vec<Header>,

    /// Collection-scoped variables, resolved alongside environment variables.
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
}
```

Replace with:

```rust
/// A collection-scoped variable (like Postman/Bruno collection variables).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionVariable {
    pub key: String,
    pub value: String,
    /// Initial/default value committed to Git; fallback when value is empty.
    #[serde(default)]
    pub initial_value: String,
    pub enabled: bool,
    /// Mark as secret to hide in the UI (like Bruno).
    #[serde(default)]
    pub secret: bool,
}

/// JS sandbox capability level for scripts in a collection. Defaults to `Safe`
/// (no filesystem/process access) so an imported collection never silently
/// inherits an elevated capability from wherever it was authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    #[default]
    Safe,
    Developer,
}

/// Per-collection default auth, headers, and variables, stored in opencollection.yml.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSettings {
    /// Markdown documentation for this collection (maps to `docs:` in opencollection.yml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,

    /// Optional auth applied to all requests in this collection.
    #[serde(default)]
    pub auth: Option<Auth>,

    /// Default headers prepended to every request in this collection.
    #[serde(default)]
    pub headers: Vec<Header>,

    /// Collection-scoped variables, resolved alongside environment variables.
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,

    /// JS sandbox capability level for scripts in this collection.
    #[serde(default)]
    pub sandbox_mode: SandboxMode,
}
```

Run: `cargo test -p rocket-collection sandbox_mode_defaults_to_safe_when_absent_from_json sandbox_mode_developer_roundtrips_as_camel_case`
Expected: PASS (2 tests), and run `cargo test -p rocket-collection` to confirm the full crate suite (including `PartialEq`/`Default` derives on `CollectionSettings` still compiling) still passes.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-collection/src/settings.rs
```

Commit message along the lines of: `feat(collection): add per-collection sandbox_mode setting`.

---

### Task 2: `SandboxMode` on `ScriptContext`

**Files:**
- Modify: `crates/rocket-scripting/src/context.rs:1-20` (new enum, new field, new builder method)
- Modify: `crates/rocket-infra/src/scripting/engine.rs:306-318` (`minimal_ctx` test helper — the only other direct `ScriptContext { ... }` struct literal in the workspace outside `rocket-scripting` itself; Rust struct-literal syntax requires every field, so this site breaks the moment the new field exists unless it's updated in the same task)
- Test: `crates/rocket-scripting/src/context.rs` (same file, `#[cfg(test)] mod tests` at end)

**Interfaces:**
- Consumes: nothing new.
- Produces: `rocket_scripting::SandboxMode` (`Safe` default / `Developer`, no `Serialize`/`Deserialize` — stays an internal Rust type, never crosses the JS boundary directly), `ScriptContext.sandbox_mode: SandboxMode`, `ScriptContext::with_sandbox_mode(mode) -> Self` — consumed by Task 3 (this plan) and by the engine-gating plan later in this sequence.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the end of `crates/rocket-scripting/src/context.rs` (after the existing `with_execution_mode_overrides_the_standalone_default` test):

```rust
    #[test]
    fn sandbox_mode_defaults_to_safe() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        );
        assert_eq!(ctx.sandbox_mode, SandboxMode::Safe);
    }

    #[test]
    fn with_sandbox_mode_overrides_the_safe_default() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        )
        .with_sandbox_mode(SandboxMode::Developer);
        assert_eq!(ctx.sandbox_mode, SandboxMode::Developer);
    }
```

Run: `cargo test -p rocket-scripting sandbox_mode_defaults_to_safe with_sandbox_mode_overrides_the_safe_default`
Expected: FAIL to compile — `no field 'sandbox_mode' on type 'ScriptContext'`, `cannot find type 'SandboxMode'`, `no method named 'with_sandbox_mode'`.

- [ ] **Step 2: Add the enum, field, and builder method**

In `crates/rocket-scripting/src/context.rs`, find the `ExecutionMode` enum definition (lines 8-25):

```rust
/// How the request carrying a script was dispatched.
///
/// Maps 1:1 to the string `req.getExecutionMode()` returns inside the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionMode {
    /// A single send, e.g. from the Request tab.
    #[default]
    Standalone,
    /// A step dispatched by the Collection Runner.
    Runner,
}
```

Add immediately after it (before the `impl ExecutionMode { ... }` block, i.e. between the enum and its `impl`):

```rust
/// JS sandbox capability level for the collection this script belongs to.
///
/// Mirrors `rocket_collection::settings::SandboxMode` — kept as a separate
/// type deliberately, to avoid `rocket-scripting` depending on
/// `rocket-collection`. `rocket-app` maps one to the other when building a
/// `ScriptContext`. Never serialized — this never crosses the JS boundary
/// directly; it only decides which `deno_core` extensions `rocket-infra`
/// registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxMode {
    #[default]
    Safe,
    Developer,
}
```

Then find the `ScriptContext` struct's `execution_mode` field:

```rust
    /// `"runner"` when executing inside a collection run, `"standalone"` otherwise.
    pub execution_mode: String,
```

Replace with:

```rust
    /// `"runner"` when executing inside a collection run, `"standalone"` otherwise.
    pub execution_mode: String,

    /// JS sandbox capability level — defaults to `Safe` on every constructor.
    pub sandbox_mode: SandboxMode,
```

Then, in each of the three constructors (`before_request`, `after_response`, `tests`), find the struct-literal field `execution_mode: "standalone".into(),` (it appears three times, once per constructor) and add `sandbox_mode: SandboxMode::Safe,` immediately after it, each time:

```rust
            execution_mode: "standalone".into(),
            sandbox_mode: SandboxMode::Safe,
```

Finally, find `with_execution_mode`:

```rust
    /// Overrides the execution mode. The three constructors default to
    /// `Standalone`; the Collection Runner sets `Runner` on every context it
    /// builds, so `req.getExecutionMode()` reports the truth.
    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode.as_str().to_string();
        self
    }
```

Add immediately after it:

```rust

    /// Overrides the sandbox mode. Defaults to `Safe`; `rocket-app` sets this
    /// from the collection's `sandbox_mode` setting for every phase.
    pub fn with_sandbox_mode(mut self, mode: SandboxMode) -> Self {
        self.sandbox_mode = mode;
        self
    }
```

In `crates/rocket-infra/src/scripting/engine.rs`, find the `minimal_ctx` test helper:

```rust
    fn minimal_ctx(code: &str) -> ScriptContext {
        ScriptContext {
            code: code.into(),
            phase: ScriptPhase::BeforeRequest,
            variables: VariableContext::default(),
            request: HttpRequest::new(HttpMethod::Get, "https://example.com"),
            response: None,
            env_name: None,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
            request_name: String::new(),
            request_tags: vec![],
            path_params: vec![],
        }
    }
```

Replace with:

```rust
    fn minimal_ctx(code: &str) -> ScriptContext {
        ScriptContext {
            code: code.into(),
            phase: ScriptPhase::BeforeRequest,
            variables: VariableContext::default(),
            request: HttpRequest::new(HttpMethod::Get, "https://example.com"),
            response: None,
            env_name: None,
            execution_mode: "standalone".into(),
            execution_platform: "app".into(),
            request_name: String::new(),
            request_tags: vec![],
            path_params: vec![],
            sandbox_mode: rocket_scripting::SandboxMode::Safe,
        }
    }
```

This keeps every existing test in `engine.rs` (which all call `minimal_ctx`, and therefore all exercise Safe Mode) unchanged in behavior — this plan does not touch what those tests assert.

Run: `cargo test -p rocket-scripting`
Expected: PASS (full crate suite, including the 2 new tests and every existing `context.rs` test whose struct literals now include the new field via the constructors, not by hand). Then run `cargo check -p rocket-infra` to confirm `minimal_ctx`'s update compiles, and `cargo test -p rocket-infra --lib scripting` to confirm every existing `engine.rs` test still passes unchanged.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-scripting/src/context.rs
```

Commit message along the lines of: `feat(scripting): add SandboxMode to ScriptContext`.

---

### Task 3: Thread `sandbox_mode` through `RequestExecutionService`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:1-23` (imports)
- Modify: `crates/rocket-app/src/execution_service.rs:116-134` (`PhaseState` struct)
- Modify: `crates/rocket-app/src/execution_service.rs:799-845` (`begin_phases`)
- Modify: `crates/rocket-app/src/execution_service.rs:853-873`, `1019-1029`, `1063-1073` (the three phase-context builders)
- Test: `crates/rocket-app/src/execution_service.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `rocket_collection::SandboxMode` (Task 1), `rocket_scripting::SandboxMode` + `ScriptContext::with_sandbox_mode` (Task 2).
- Produces: `PhaseState.sandbox_mode: rocket_scripting::SandboxMode`, resolved once in `begin_phases` and applied to every phase's `ScriptContext`. Nothing outside this crate depends on this field directly.

**Note on scope:** `evaluate_var_expression` (a separate jsonq-preview `ScriptContext` construction elsewhere in this file) is deliberately NOT touched by this task — it stays permanently Safe Mode. This wasn't part of the approved spec's scope and isn't one of the three script phases or the Collection Runner the spec's acceptance criteria names.

- [ ] **Step 1: Write the failing test**

This file already has everything this test needs, at these exact names: `StubCollectionRepo::with_settings(settings)` (a `CollectionRepository` mock that returns the given `CollectionSettings` regardless of which collection name is asked for — line ~1504), `MockEnvRepo::empty()`, `build_svc_with_script(env_repo, collection_repo, engine)` (line ~2262, constructs a `RequestExecutionService` wired with a custom collection repo and script engine), `sample_input(url, env_name)` (line ~1557, a minimal `ExecuteRequestInput`), and the `ModeProbeEngine`/`SharedModeProbe` pair (line ~3820) as the exact style precedent for a two-struct engine-plus-shared-wrapper test double (a plain struct owning the recorded state, plus a thin `Arc`-wrapping struct so `Box<dyn ScriptEngine>` can be handed to the service while the test keeps its own handle to read the recording afterward).

Add this test to the `#[cfg(test)] mod tests` block, near `ModeProbeEngine`'s own tests:

```rust
    struct SandboxModeProbeEngine {
        seen_modes: Mutex<Vec<rocket_scripting::SandboxMode>>,
    }

    #[async_trait]
    impl ScriptEngine for SandboxModeProbeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.seen_modes.lock().expect("lock").push(ctx.sandbox_mode);
            Ok(ScriptResult::default())
        }
    }

    struct SharedSandboxModeProbe(Arc<SandboxModeProbeEngine>);
    #[async_trait]
    impl ScriptEngine for SharedSandboxModeProbe {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    #[tokio::test]
    async fn before_request_script_receives_collection_sandbox_mode() {
        let engine = Arc::new(SandboxModeProbeEngine { seen_modes: Mutex::new(vec![]) });
        let collection_repo = StubCollectionRepo::with_settings(CollectionSettings {
            sandbox_mode: rocket_collection::SandboxMode::Developer,
            ..Default::default()
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(collection_repo),
            Box::new(SharedSandboxModeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("// pre".into());
        svc.execute(input).await.expect("execute");

        let modes = engine.seen_modes.lock().expect("lock").clone();
        assert_eq!(modes, vec![rocket_scripting::SandboxMode::Developer]);
    }
```

Fully-qualified paths (`rocket_collection::SandboxMode`, `rocket_scripting::SandboxMode`) are used deliberately so this test compiles before Step 2 adds the aliased `use` imports — it only needs the types to exist (both already do, from Tasks 1 and 2), not the service wiring this task adds.

Run: `cargo test -p rocket-app --lib before_request_script_receives_collection_sandbox_mode`
Expected: FAIL, but not a compile error — the test compiles fine (it only calls the public `svc.execute(...)`, never touches `PhaseState` directly), and fails at the assertion: `modes` is `[Safe]`, not `[Developer]`, because nothing in `execution_service.rs` reads the collection's `sandbox_mode` setting yet — every `ScriptContext` still defaults to `Safe` from Task 2's constructors.

- [ ] **Step 2: Thread the field through**

In `crates/rocket-app/src/execution_service.rs`, replace the imports (lines 6 and 15-18):

```rust
use rocket_collection::CollectionRepository;
```

with:

```rust
use rocket_collection::{CollectionRepository, SandboxMode as CollectionSandboxMode};
```

and:

```rust
use rocket_scripting::{
    ConsoleEntry, ConsoleLevel, ExecutionMode, NextRequest, ScriptContext, ScriptEngine,
    ScriptResult, TestResult, TestStatus,
};
```

with:

```rust
use rocket_scripting::{
    ConsoleEntry, ConsoleLevel, ExecutionMode, NextRequest, SandboxMode, ScriptContext,
    ScriptEngine, ScriptResult, TestResult, TestStatus,
};
```

Replace the `PhaseState` struct (lines 116-134):

```rust
pub(crate) struct PhaseState {
    /// The resolved request. A before-request script can still mutate it.
    pub http_request: HttpRequest,
    /// Scope-separated variables. `runtime` accumulates across phases.
    pub var_ctx: VariableContext,
    /// First script error seen, in phase order.
    pub script_error: Option<String>,
    /// Console output collected from every phase that ran.
    pub console: Vec<ConsoleEntry>,
    /// Test results from the tests phase plus declarative assertions.
    pub test_results: Vec<TestResult>,
    /// Last `next_request` set by any phase that ran — later phase wins, the
    /// same "later overrides earlier" rule `runtime_vars` merging already uses.
    /// Only the Collection Runner reads this.
    pub next_request: Option<NextRequest>,
    /// Set by a before-request script calling `rok.runner.skipRequest()`.
    /// Only the Collection Runner reads this; `execute()` always sends.
    pub skip_request: bool,
}
```

with:

```rust
pub(crate) struct PhaseState {
    /// The resolved request. A before-request script can still mutate it.
    pub http_request: HttpRequest,
    /// Scope-separated variables. `runtime` accumulates across phases.
    pub var_ctx: VariableContext,
    /// First script error seen, in phase order.
    pub script_error: Option<String>,
    /// Console output collected from every phase that ran.
    pub console: Vec<ConsoleEntry>,
    /// Test results from the tests phase plus declarative assertions.
    pub test_results: Vec<TestResult>,
    /// Last `next_request` set by any phase that ran — later phase wins, the
    /// same "later overrides earlier" rule `runtime_vars` merging already uses.
    /// Only the Collection Runner reads this.
    pub next_request: Option<NextRequest>,
    /// Set by a before-request script calling `rok.runner.skipRequest()`.
    /// Only the Collection Runner reads this; `execute()` always sends.
    pub skip_request: bool,
    /// Resolved once in `begin_phases` from the collection's `sandbox_mode`
    /// setting, applied to every phase's `ScriptContext`.
    pub sandbox_mode: SandboxMode,
}
```

In `begin_phases`, find:

```rust
        Ok(PhaseState {
            http_request,
            var_ctx,
            script_error: None,
            console: Vec::new(),
            test_results: Vec::new(),
            next_request: None,
            skip_request: false,
        })
    }
```

Replace with:

```rust
        let sandbox_mode = match input.collection.as_deref() {
            Some(col) => match self.collection_repo.get_settings(col).unwrap_or_default().sandbox_mode {
                CollectionSandboxMode::Safe => SandboxMode::Safe,
                CollectionSandboxMode::Developer => SandboxMode::Developer,
            },
            None => SandboxMode::Safe,
        };

        Ok(PhaseState {
            http_request,
            var_ctx,
            script_error: None,
            console: Vec::new(),
            test_results: Vec::new(),
            next_request: None,
            skip_request: false,
            sandbox_mode,
        })
    }
```

In each of the three phase methods, find `.with_execution_mode(mode);` (it appears three times — in `run_before_request_phase`, `run_after_response_phase`, `run_tests_phase`) and replace each occurrence with:

```rust
                .with_execution_mode(mode)
                .with_sandbox_mode(state.sandbox_mode);
```

Run: `cargo test -p rocket-app --lib before_request_script_receives_collection_sandbox_mode`
Expected: PASS. Then run `cargo test -p rocket-app` (full crate) to confirm nothing else broke, and `cargo check -p rocket` to confirm the whole workspace still compiles.

- [ ] **Step 3: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add crates/rocket-app/src/execution_service.rs
```

Commit message along the lines of: `feat(execution): thread collection sandbox_mode into script phases`.

---

## Final verification (after all 3 tasks)

- [ ] Run `cargo test -p rocket-collection`, `cargo test -p rocket-scripting`, `cargo test -p rocket-app`, `cargo test -p rocket-infra --lib scripting` — expect PASS.
- [ ] Run `cargo check -p rocket` — expect PASS.
- [ ] No frontend files touched by this plan — nothing to verify there yet. `rocket-infra` is touched only at the single `minimal_ctx` test-helper site in Task 2; no new op files or engine behavior yet — that starts in the next two plans.
