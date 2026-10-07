# rok parity A, plan 02: context fields, test results, response and runner extras

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the APIs that need new data in `ScriptContext` or `ScriptResult`: collection name, sandbox mode, `cwd` and `__dirname`, test and assertion results, `res` extras and runner extras.

**Architecture:** `ScriptContext` gains optional fields set by builders, mirroring `with_file_scope`. `rocket-infra` copies them into `ScriptInputState`. `rocket-app` fills them where it builds contexts in `execution_service.rs`. `res.setBody` returns its value in a new `ScriptResult.response_body`, which the tests phase uses. `stopExecution` reuses the runner's existing `NextRequest::Stop`.

**Tech Stack:** Rust, `deno_core` ops, Vitest, Monaco type definitions.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-a-sync-design.md`. Index with rulings: `00-plan-index.md`. Requires plan 01 to be merged first.

## Global Constraints

- `cargo` commands always pass `-j4`. No `cargo test --workspace`.
- `cwd`, `__dirname` and `__filename` are Developer mode only. In Safe mode `rok.cwd()` throws `rok.cwd() requires Developer mode` and `__dirname` is undefined.
- `res.url` and `res.getUrl()` return the request URL (the final redirect URL is not tracked).
- `getOauth2CredentialVar` and `resetOauth2Credential` are out of this plan (see the index).
- `rok-types.ts` and `bootstrap.js` stay in sync (the plan 01 test enforces it for top-level `rok` names).
- Comments are short full sentences ending in a period.

## Review Focus

- `getAssertionResults()` with a disabled assertion in the list: the disabled one is omitted and the order of the rest is kept.
- `rok.cwd()` and `__dirname` in Safe mode: a clear error and an undefined global, not a silent empty string.
- `res.setBody({ a: 1 })` then a later tests script reading `res.body.a`: it sees `1`, and the UI response is unchanged.
- `rok.runner.stopExecution()` in the tests phase stops the run but does not mark the step as skipped.
- `res.getSize()` for a binary response: `body` uses `size_bytes`, not the text length (text body is empty).

---

### Task 1: Collection name, sandbox mode, cwd and `__dirname`

**Files:**
- Modify: `crates/rocket-scripting/src/context.rs`
- Modify: `crates/rocket-infra/src/scripting/state.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (seeding in `run_script`, registration, tests, existing `ScriptContext { .. }` literals)
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs`
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js`
- Modify: `crates/rocket-app/src/execution_service.rs` (three builder chains, test double, test)
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Produces (Rust): `ScriptContext.collection_name: Option<String>`; `ScriptContext::with_collection_name(self, name: Option<String>) -> Self`; `ScriptInputState.{sandbox_mode: SandboxMode, collection_name: String, collection_root: Option<PathBuf>}`; ops `op_rok_get_collection_name`, `op_rok_is_safe_mode`, `op_rok_cwd`.
- Produces (JS): `rok.getCollectionName(): string`, `rok.isSafeMode(): boolean`, `rok.cwd(): string`, global `__dirname` (Developer mode only).
- Produces (test double, `execution_service.rs` tests module): `CapturingScriptEngine` with `contexts: Mutex<Vec<ScriptContext>>` and `after_response_result: Mutex<ScriptResult>`, wrapped by `SharedCapture(Arc<CapturingScriptEngine>)` implementing `ScriptEngine`. Task 3 reuses it.

- [ ] **Step 1: Write the failing engine tests**

Add to the `tests` module in `engine.rs` (add `use std::path::PathBuf;` and `use rocket_scripting::ScriptFileScope;` to the module imports if missing):

```rust
    #[tokio::test]
    async fn rok_get_collection_name_returns_the_name_or_empty() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx("rok.setVar('n', rok.getCollectionName())");
        ctx.collection_name = Some("Payments".into());
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("n").expect("n present"), "Payments");

        let ctx = minimal_ctx("rok.setVar('n', rok.getCollectionName())");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("n").expect("n present"), "");
    }

    #[tokio::test]
    async fn rok_is_safe_mode_follows_the_sandbox_mode() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('safe', rok.isSafeMode())");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("safe").expect("safe present"), true);

        let mut ctx = minimal_ctx("rok.setVar('safe', rok.isSafeMode())");
        ctx.sandbox_mode = SandboxMode::Developer;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("safe").expect("safe present"), false);
    }

    #[tokio::test]
    async fn rok_cwd_throws_in_safe_mode_and_dirname_is_undefined() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx("rok.cwd()");
        ctx.file_scope = Some(ScriptFileScope {
            collection_root: PathBuf::from("/tmp/some-collection"),
            additional_roots: vec![],
        });
        let result = engine.execute(ctx).await.expect("execute");
        let error = result.error.expect("script error");
        assert!(error.contains("requires Developer mode"), "got: {error}");

        let ctx = minimal_ctx("rok.setVar('t', typeof __dirname)");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("t").expect("t present"), "undefined");
    }

    #[tokio::test]
    async fn rok_cwd_and_dirname_return_the_collection_root_in_developer_mode() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx(
            "rok.setVar('cwd', rok.cwd()); rok.setVar('dir', __dirname); \
             rok.setVar('file', String(__filename))",
        );
        ctx.sandbox_mode = SandboxMode::Developer;
        ctx.file_scope = Some(ScriptFileScope {
            collection_root: PathBuf::from("/tmp/some-collection"),
            additional_roots: vec![],
        });
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("cwd").expect("cwd present"),
            "/tmp/some-collection"
        );
        assert_eq!(
            result.runtime_vars.get("dir").expect("dir present"),
            "/tmp/some-collection"
        );
        assert_eq!(result.runtime_vars.get("file").expect("file present"), "undefined");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra rok_get_collection_name rok_is_safe_mode rok_cwd`
Expected: FAIL to compile (`no field collection_name on ScriptContext`).

- [ ] **Step 3: Add the context field and builder**

In `crates/rocket-scripting/src/context.rs`, add to `ScriptContext` after `file_scope`:

```rust
    /// Display name of the collection the request belongs to, for `rok.getCollectionName()`.
    pub collection_name: Option<String>,
```

Add `collection_name: None,` to the struct literal in each of the three constructors (`before_request`, `after_response`, `tests`). Add next to `with_file_scope`:

```rust
    /// Sets the collection name returned by `rok.getCollectionName()`.
    pub fn with_collection_name(mut self, name: Option<String>) -> Self {
        self.collection_name = name;
        self
    }
```

- [ ] **Step 4: Add the state fields and seed them**

In `crates/rocket-infra/src/scripting/state.rs`, add `use rocket_scripting::SandboxMode;` and `use std::path::PathBuf;`, and add to `ScriptInputState` after `secret_values`:

```rust
    /// Capability level of this run. Decides `rok.isSafeMode()` and `rok.cwd()`.
    pub sandbox_mode: SandboxMode,
    /// Collection display name, empty when unknown.
    pub collection_name: String,
    /// Absolute collection directory, for `rok.cwd()` and `__dirname`.
    pub collection_root: Option<PathBuf>,
```

In `run_script` in `engine.rs`, directly after `let sandbox_mode = ctx.sandbox_mode;` add:

```rust
    let collection_root = ctx.file_scope.as_ref().map(|s| s.collection_root.clone());
```

and add to the `ScriptInputState { ... }` literal:

```rust
            sandbox_mode,
            collection_name: ctx.collection_name.unwrap_or_default(),
            collection_root,
```

In the test helper `minimal_ctx` and every other `ScriptContext { ... }` literal in `engine.rs`, add `collection_name: None,`. Run `cargo check -j4 -p rocket-infra --tests` and fix each remaining E0063 the same way.

- [ ] **Step 5: Add the ops**

In `ops/rok.rs`, add `use crate::scripting::ops::ScriptOpError;` and `use rocket_scripting::SandboxMode;` to the imports, then:

```rust
/// rok.getCollectionName() — display name of the collection, or empty string.
#[op2]
#[string]
pub fn op_rok_get_collection_name(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().collection_name.clone()
}

/// rok.isSafeMode() — true in Safe mode, false in Developer mode.
#[op2(fast)]
pub fn op_rok_is_safe_mode(state: &OpState) -> bool {
    state.borrow::<ScriptInputState>().sandbox_mode == SandboxMode::Safe
}

/// rok.cwd() — absolute collection directory. Developer mode only.
#[op2]
#[string]
pub fn op_rok_cwd(state: &OpState) -> Result<String, ScriptOpError> {
    let input = state.borrow::<ScriptInputState>();
    if input.sandbox_mode == SandboxMode::Safe {
        return Err(ScriptOpError("rok.cwd() requires Developer mode".into()));
    }
    input
        .collection_root
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| ScriptOpError("rok.cwd() has no collection directory here".into()))
}
```

Register in the `engine.rs` extension list:

```rust
        rok::op_rok_get_collection_name,
        rok::op_rok_is_safe_mode,
        rok::op_rok_cwd,
```

- [ ] **Step 6: Add the JS wrappers**

In `bootstrap.js`, add to the `rok` block:

```js
    getCollectionName: ()    => __ops.op_rok_get_collection_name(),
    isSafeMode:        ()    => __ops.op_rok_is_safe_mode(),
    cwd:               ()    => __ops.op_rok_cwd(),
```

and directly after the closing `};` of `globalThis.rok = { ... }`:

```js
  // ── Developer-mode globals ──────────────────────────────────────────────────
  // __dirname is the collection root. The executing script's own path is not
  // known here, so __filename stays undefined. Local modules loaded through
  // require() get their own __dirname and __filename from their wrapper.
  if (!__ops.op_rok_is_safe_mode()) {
    try { globalThis.__dirname = __ops.op_rok_cwd(); } catch (_e) { /* No collection directory. */ }
    globalThis.__filename = undefined;
  }
```

- [ ] **Step 7: Run the engine tests**

Run: `cargo test -j4 -p rocket-infra rok_get_collection_name rok_is_safe_mode rok_cwd`
Expected: PASS (4 tests). Then `cargo test -j4 -p rocket-infra scripting` to confirm the local-module tests that use `__dirname` still pass.

- [ ] **Step 8: Write the failing app-layer test**

In the `tests` module of `execution_service.rs`, add the test double next to `MockScriptEngine`:

```rust
    struct CapturingScriptEngine {
        contexts: Mutex<Vec<ScriptContext>>,
        after_response_result: Mutex<ScriptResult>,
    }

    impl CapturingScriptEngine {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                contexts: Mutex::new(Vec::new()),
                after_response_result: Mutex::new(ScriptResult::default()),
            })
        }

        fn with_after_response(result: ScriptResult) -> Arc<Self> {
            Arc::new(Self {
                contexts: Mutex::new(Vec::new()),
                after_response_result: Mutex::new(result),
            })
        }

        fn contexts(&self) -> Vec<ScriptContext> {
            self.contexts.lock().expect("lock poisoned").clone()
        }
    }

    struct SharedCapture(Arc<CapturingScriptEngine>);

    #[async_trait]
    impl ScriptEngine for SharedCapture {
        async fn execute(
            &self,
            ctx: ScriptContext,
        ) -> rocket_shared::error::DomainResult<ScriptResult> {
            use rocket_scripting::ScriptPhase;
            let phase = ctx.phase.clone();
            self.0.contexts.lock().expect("lock poisoned").push(ctx);
            if phase == ScriptPhase::AfterResponse {
                Ok(self
                    .0
                    .after_response_result
                    .lock()
                    .expect("lock poisoned")
                    .clone())
            } else {
                Ok(ScriptResult::default())
            }
        }
    }
```

and the test:

```rust
    #[tokio::test]
    async fn user_scripts_receive_the_collection_name() {
        let capture = CapturingScriptEngine::new();
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.collection = Some("Payments".into());
        input.pre_request_script = Some("// pre".into());
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        svc.execute(input).await.expect("execute failed");

        let contexts = capture.contexts();
        assert_eq!(contexts.len(), 3);
        assert!(contexts
            .iter()
            .all(|c| c.collection_name.as_deref() == Some("Payments")));
    }
```

- [ ] **Step 9: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-app user_scripts_receive_the_collection_name`
Expected: FAIL (`collection_name` is `None`).

- [ ] **Step 10: Wire the builder at the three user-script sites**

In `execution_service.rs`, in `run_before_request_phase`, `run_after_response_phase` and `run_tests_phase`, add to each builder chain after `.with_file_scope(state.file_scope.clone())`:

```rust
                .with_collection_name(input.collection.clone())
```

(Find them with `grep -n "with_file_scope" crates/rocket-app/src/execution_service.rs`. Leave the `run actions` site that builds `__jsonq_result__` scripts alone.)

- [ ] **Step 11: Run the app-layer test**

Run: `cargo test -j4 -p rocket-app user_scripts_receive_the_collection_name`
Expected: PASS.

- [ ] **Step 12: Add typings, run checks**

In `ROK_DEFS` add:

```ts
  /** Display name of the collection this request belongs to. */
  getCollectionName(): string;
  /** True in Safe mode, false in Developer mode. */
  isSafeMode(): boolean;
  /** Absolute path of the collection directory. Developer mode only, throws in Safe mode. */
  cwd(): string;
```

and a global declaration appended to `ROK_DEFS` (after the `rok` declaration):

```ts
/** Collection directory. Developer mode only. */
declare const __dirname: string;
/** Always undefined in top-level scripts. */
declare const __filename: string | undefined;
```

Run: `yarn test rok-types && yarn tsc --noEmit && yarn check && cargo check -j4`
Expected: PASS.

- [ ] **Step 13: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-scripting/src/context.rs`, `crates/rocket-infra/src/scripting/state.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/ops/rok.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `crates/rocket-app/src/execution_service.rs`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add collection name, safe mode and cwd to rok`.

---

### Task 2: Test and assertion results

**Files:**
- Modify: `crates/rocket-scripting/src/result.rs`, `crates/rocket-scripting/src/lib.rs`, `crates/rocket-scripting/src/context.rs`
- Modify: `crates/rocket-app/src/assertion_evaluator.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (tests-phase builder chain, test)
- Modify: `crates/rocket-infra/src/scripting/state.rs`, `engine.rs`, `ops/rok.rs`, `bootstrap.js`
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Consumes: Task 1 (`CapturingScriptEngine`, `with_collection_name` builder style, `ScriptInputState` seeding).
- Produces (Rust): `rocket_scripting::AssertionOutcome { lhs: String, operator: String, rhs: String, status: TestStatus }` (derive `Debug, Clone, Serialize, Deserialize`); `ScriptContext.assertion_results: Vec<AssertionOutcome>`; `ScriptContext::with_assertion_results(self, Vec<AssertionOutcome>) -> Self`; `rocket_app::assertion_evaluator::assertion_outcomes(assertions: &[Assertion], response: &HttpResponse) -> Vec<AssertionOutcome>`; ops `op_rok_get_test_results`, `op_rok_get_assertion_results` (JSON strings).
- Produces (JS): `rok.getTestResults(): { name: string; status: 'pass' | 'fail'; error?: string }[]`; `rok.getAssertionResults(): { lhs: string; operator: string; rhs: string; status: 'pass' | 'fail' }[]`.

The declarative assertions run after the tests script today, so they cannot be read from state. They are a pure function of the assertions and the response, so they are computed before the tests script and passed in. Order of execution does not change.

- [ ] **Step 1: Write the failing evaluator test**

Add to the `tests` module of `crates/rocket-app/src/assertion_evaluator.rs`:

```rust
    #[test]
    fn assertion_outcomes_skip_disabled_and_keep_order() {
        let first = Assertion::new("res.status", "eq", Some("200".into()));
        let mut disabled = Assertion::new("res.status", "eq", Some("500".into()));
        disabled.disabled = Some(true);
        let last = Assertion::new("res.body", "isJson", None);

        let outcomes = assertion_outcomes(&[first, disabled, last], &resp(200, "{}"));

        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].lhs, "res.status");
        assert_eq!(outcomes[0].operator, "eq");
        assert_eq!(outcomes[0].rhs, "200");
        assert_eq!(outcomes[0].status, TestStatus::Passed);
        assert_eq!(outcomes[1].lhs, "res.body");
        assert_eq!(outcomes[1].rhs, "");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p rocket-app assertion_outcomes_skip_disabled`
Expected: FAIL to compile (`cannot find function assertion_outcomes`).

- [ ] **Step 3: Add the type and the function**

In `crates/rocket-scripting/src/result.rs`, after `TestStatus`:

```rust
/// One declarative assertion as seen by `rok.getAssertionResults()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionOutcome {
    /// The expression, for example `res.status`.
    pub lhs: String,
    pub operator: String,
    /// The expected value, empty for unary operators.
    pub rhs: String,
    pub status: TestStatus,
}
```

In `lib.rs`, add `AssertionOutcome` to the `pub use result::{ ... }` list.

In `assertion_evaluator.rs`, add `use rocket_scripting::AssertionOutcome;` and after `evaluate_assertions`:

```rust
/// Evaluates the enabled assertions and returns them in the shape scripts read.
///
/// Pure, so it can run before the tests script and again afterwards without
/// changing the outcome.
pub fn assertion_outcomes(assertions: &[Assertion], response: &HttpResponse) -> Vec<AssertionOutcome> {
    assertions
        .iter()
        .filter(|a| a.disabled != Some(true))
        .map(|a| AssertionOutcome {
            lhs: a.expression.clone(),
            operator: a.operator.clone(),
            rhs: a.value.clone().unwrap_or_default(),
            status: evaluate_one(a, response).status,
        })
        .collect()
}
```

- [ ] **Step 4: Run the evaluator test**

Run: `cargo test -j4 -p rocket-app assertion_outcomes_skip_disabled`
Expected: PASS.

- [ ] **Step 5: Write the failing engine tests**

Add to the `tests` module in `engine.rs` (import `rocket_scripting::{AssertionOutcome, TestStatus}`):

```rust
    #[tokio::test]
    async fn rok_get_test_results_returns_tests_recorded_so_far() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx(
            "test('first', () => { expect(1).to.equal(1); }); \
             test('second', () => { expect(1).to.equal(2); }); \
             rok.setVar('out', JSON.stringify(rok.getTestResults().map(r => r.name + ':' + r.status)))",
        );
        ctx.phase = ScriptPhase::Tests;
        ctx.response = Some(rocket_http::HttpResponse::default());
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "[\"first:pass\",\"second:fail\"]"
        );
    }

    #[tokio::test]
    async fn rok_get_assertion_results_returns_the_precomputed_outcomes() {
        let engine = DenoScriptEngine::new();
        let mut ctx = minimal_ctx(
            "rok.setVar('out', rok.getAssertionResults().map(a => a.lhs + ' ' + a.operator + ' ' + a.rhs + ' ' + a.status).join('|'))",
        );
        ctx.phase = ScriptPhase::Tests;
        ctx.response = Some(rocket_http::HttpResponse::default());
        ctx.assertion_results = vec![AssertionOutcome {
            lhs: "res.status".into(),
            operator: "eq".into(),
            rhs: "200".into(),
            status: TestStatus::Passed,
        }];
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "res.status eq 200 pass"
        );
    }
```

- [ ] **Step 6: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra rok_get_test_results rok_get_assertion_results`
Expected: FAIL to compile (`no field assertion_results on ScriptContext`).

- [ ] **Step 7: Add the context field, state field and ops**

In `context.rs`, add `use crate::AssertionOutcome;`, add to `ScriptContext` after `collection_name`:

```rust
    /// Declarative assertion outcomes for `rok.getAssertionResults()`. Filled for the tests phase only.
    pub assertion_results: Vec<AssertionOutcome>,
```

set `assertion_results: Vec::new(),` in the three constructors, and add:

```rust
    /// Sets the outcomes returned by `rok.getAssertionResults()`.
    pub fn with_assertion_results(mut self, results: Vec<AssertionOutcome>) -> Self {
        self.assertion_results = results;
        self
    }
```

In `state.rs`, add `use rocket_scripting::AssertionOutcome;` and to `ScriptInputState`:

```rust
    pub assertion_results: Vec<AssertionOutcome>,
```

In `run_script`, add `assertion_results: ctx.assertion_results,` to the `ScriptInputState` literal. Add `assertion_results: vec![],` to `minimal_ctx` and every other `ScriptContext { .. }` literal in `engine.rs` (use `cargo check -j4 -p rocket-infra --tests` to find them).

In `ops/rok.rs`:

```rust
fn status_word(status: &rocket_scripting::TestStatus) -> &'static str {
    match status {
        rocket_scripting::TestStatus::Passed => "pass",
        rocket_scripting::TestStatus::Failed => "fail",
    }
}

/// rok.getTestResults() — tests recorded so far by this script, as JSON.
#[op2]
#[string]
pub fn op_rok_get_test_results(state: &OpState) -> String {
    let items: Vec<serde_json::Value> = state
        .borrow::<ScriptOutputState>()
        .test_results
        .iter()
        .map(|t| {
            let mut item = serde_json::json!({ "name": t.name, "status": status_word(&t.status) });
            if let Some(error) = &t.error {
                item["error"] = serde_json::Value::String(error.clone());
            }
            item
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".into())
}

/// rok.getAssertionResults() — declarative assertion outcomes, as JSON.
#[op2]
#[string]
pub fn op_rok_get_assertion_results(state: &OpState) -> String {
    let items: Vec<serde_json::Value> = state
        .borrow::<ScriptInputState>()
        .assertion_results
        .iter()
        .map(|a| {
            serde_json::json!({
                "lhs": a.lhs,
                "operator": a.operator,
                "rhs": a.rhs,
                "status": status_word(&a.status),
            })
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".into())
}
```

Register both in `engine.rs` and add to `bootstrap.js` `rok` block:

```js
    getTestResults:      () => JSON.parse(__ops.op_rok_get_test_results()),
    getAssertionResults: () => JSON.parse(__ops.op_rok_get_assertion_results()),
```

- [ ] **Step 8: Run the engine tests**

Run: `cargo test -j4 -p rocket-infra rok_get_test_results rok_get_assertion_results`
Expected: PASS.

- [ ] **Step 9: Write the failing app-layer test**

In `execution_service.rs` tests, add:

```rust
    #[tokio::test]
    async fn tests_script_receives_precomputed_assertion_outcomes() {
        let capture = CapturingScriptEngine::new();
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.tests_script = Some("// tests".into());
        input.assertions = vec![rocket_shared::Assertion::new(
            "res.status",
            "eq",
            Some("200".into()),
        )];
        svc.execute(input).await.expect("execute failed");

        let contexts = capture.contexts();
        let tests_ctx = contexts
            .iter()
            .find(|c| c.phase == rocket_scripting::ScriptPhase::Tests)
            .expect("tests phase ran");
        assert_eq!(tests_ctx.assertion_results.len(), 1);
        assert_eq!(tests_ctx.assertion_results[0].lhs, "res.status");
    }
```

- [ ] **Step 10: Run to verify it fails, then wire it**

Run: `cargo test -j4 -p rocket-app tests_script_receives_precomputed_assertion_outcomes`
Expected: FAIL (`assertion_results` is empty).

In `run_tests_phase`, add to the builder chain after `.with_collection_name(...)`:

```rust
                .with_assertion_results(crate::assertion_evaluator::assertion_outcomes(
                    &input.assertions,
                    response,
                ))
```

Run: `cargo test -j4 -p rocket-app tests_script_receives_precomputed_assertion_outcomes assertions_run_after_tests_script`
Expected: PASS (the existing ordering test still passes).

- [ ] **Step 11: Add typings and run checks**

In `ROK_DEFS`:

```ts
  /** Tests recorded so far by this script. Tests phase only. */
  getTestResults(): { name: string; status: 'pass' | 'fail'; error?: string }[];
  /** Declarative assertion outcomes for this request. Tests phase only. */
  getAssertionResults(): { lhs: string; operator: string; rhs: string; status: 'pass' | 'fail' }[];
```

Run: `yarn test rok-types && yarn tsc --noEmit && yarn check && cargo check -j4 && cargo test -j4 -p rocket-infra scripting`
Expected: PASS.

- [ ] **Step 12: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-scripting/src/result.rs`, `crates/rocket-scripting/src/lib.rs`, `crates/rocket-scripting/src/context.rs`, `crates/rocket-app/src/assertion_evaluator.rs`, `crates/rocket-app/src/execution_service.rs`, `crates/rocket-infra/src/scripting/state.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/ops/rok.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add rok test and assertion results`.

---

### Task 3: Response extras and runner extras

**Files:**
- Modify: `crates/rocket-scripting/src/result.rs` (`ScriptResult.response_body`)
- Modify: `crates/rocket-infra/src/scripting/state.rs`, `engine.rs`, `ops/res.rs`, `ops/rok.rs`, `bootstrap.js`
- Modify: `crates/rocket-app/src/execution_service.rs` (`PhaseState`, after-response and tests phases, helper, tests)
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Consumes: Task 1 (`CapturingScriptEngine::with_after_response`, `SharedCapture`).
- Produces (Rust): `ScriptResult.response_body: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`); `ScriptOutputState.response_body: Option<String>`; `PhaseState.response_body_override: Option<String>`; `fn with_body_override(response: &HttpResponse, body: Option<&str>) -> HttpResponse` in `execution_service.rs`; ops `op_res_get_url`, `op_res_get_size`, `op_res_set_body`, `op_rok_stop_execution`.
- Produces (JS): `res.url`, `res.getUrl()`, `res.getSize()`, `res.setBody(body)`, `rok.runner.stopExecution()`, `rok.runner.iterationIndex`, `rok.runner.totalIterations`.

- [ ] **Step 1: Write the failing engine tests**

Add to the `tests` module in `engine.rs`:

```rust
    fn response_ctx(code: &str, body: &str) -> ScriptContext {
        let mut ctx = minimal_ctx(code);
        ctx.phase = ScriptPhase::AfterResponse;
        ctx.response = Some(rocket_http::HttpResponse {
            status: 200,
            status_text: "OK".into(),
            body: body.into(),
            duration_ms: 5,
            ttfb_ms: 1,
            size_bytes: body.len(),
            ..Default::default()
        });
        ctx
    }

    #[tokio::test]
    async fn res_url_and_get_url_return_the_request_url() {
        let engine = DenoScriptEngine::new();
        let ctx = response_ctx("rok.setVar('u', res.url + '|' + res.getUrl())", "{}");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("u").expect("u present"),
            "https://example.com|https://example.com"
        );
    }

    #[tokio::test]
    async fn res_get_size_reports_body_headers_and_total() {
        let engine = DenoScriptEngine::new();
        let ctx = response_ctx("rok.setVar('s', JSON.stringify(res.getSize()))", "{\"a\":1}");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("s").expect("s present"),
            "{\"body\":7,\"headers\":0,\"total\":7}"
        );
    }

    #[tokio::test]
    async fn res_get_size_uses_size_bytes_for_a_binary_body() {
        let engine = DenoScriptEngine::new();
        let mut ctx = response_ctx("rok.setVar('b', res.getSize().body)", "");
        if let Some(response) = ctx.response.as_mut() {
            response.is_binary = true;
            response.size_bytes = 2048;
        }
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("b").expect("b present"), 2048);
    }

    #[tokio::test]
    async fn res_set_body_is_visible_in_the_script_and_returned() {
        let engine = DenoScriptEngine::new();
        let ctx = response_ctx(
            "res.setBody({ a: 2 }); rok.setVar('seen', res.body.a)",
            "{\"a\":1}",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("seen").expect("seen present"), 2);
        assert_eq!(result.response_body.as_deref(), Some("{\"a\":2}"));
    }

    #[tokio::test]
    async fn res_set_body_before_the_response_exists_throws() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("res.setBody('x')");
        let result = engine.execute(ctx).await.expect("execute");
        let error = result.error.expect("script error");
        assert!(error.contains("res is not available"), "got: {error}");
    }

    #[tokio::test]
    async fn rok_stop_execution_stops_and_skips_only_before_the_request() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.runner.stopExecution()");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(matches!(
            result.next_request,
            Some(rocket_scripting::NextRequest::Stop)
        ));
        assert!(result.skip_request);

        let ctx = response_ctx("rok.runner.stopExecution()", "{}");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(matches!(
            result.next_request,
            Some(rocket_scripting::NextRequest::Stop)
        ));
        assert!(!result.skip_request);
    }

    #[tokio::test]
    async fn rok_runner_iteration_values_default_to_a_single_iteration() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "rok.setVar('i', rok.runner.iterationIndex + ',' + rok.runner.totalIterations)",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("i").expect("i present"), "0,1");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra res_url_and res_get_size res_set_body rok_stop_execution rok_runner_iteration`
Expected: FAIL to compile (`no field response_body on ScriptResult`).

- [ ] **Step 3: Add the result and state fields**

In `result.rs`, add to `ScriptResult` after `console_entries`:

```rust
    /// Replacement response body set via `res.setBody`. Later scripts see it. The
    /// stored response and the UI are not changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
```

In `state.rs` add to `ScriptOutputState`: `pub response_body: Option<String>,`. In `run_script` add `response_body: out.response_body,` to the `ScriptResult` literal.

- [ ] **Step 4: Add the ops**

In `ops/res.rs`, add `use crate::scripting::state::ScriptOutputState;` and:

```rust
/// The request URL. The final redirect URL is not tracked, so this is the best known value.
#[op2]
#[string]
pub fn op_res_get_url(state: &OpState) -> Result<String, ScriptOpError> {
    get_response(state)?;
    Ok(state.borrow::<ScriptInputState>().request.url.clone())
}

/// Returns `{ body, headers, total }` in bytes as JSON.
#[op2]
#[string]
pub fn op_res_get_size(state: &OpState) -> Result<String, ScriptOpError> {
    let response = get_response(state)?;
    // Each header line is "key: value\r\n", which adds four bytes.
    let headers: usize = response
        .headers
        .iter()
        .map(|h| h.key.len() + h.value.len() + 4)
        .sum();
    let body = response.size_bytes;
    Ok(serde_json::json!({ "body": body, "headers": headers, "total": body + headers }).to_string())
}

/// res.setBody(body) — replaces the body later scripts see. Takes the raw text.
#[op2(fast)]
pub fn op_res_set_body(state: &mut OpState, #[string] body: String) -> Result<(), ScriptOpError> {
    get_response(state)?;
    state.borrow_mut::<ScriptOutputState>().response_body = Some(body);
    Ok(())
}
```

In `ops/rok.rs`:

```rust
/// rok.runner.stopExecution() — stops the run. Before the request it also skips the send.
#[op2(fast)]
pub fn op_rok_stop_execution(state: &mut OpState) {
    let before_request =
        state.borrow::<ScriptInputState>().phase == rocket_scripting::ScriptPhase::BeforeRequest;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.next_request = Some(NextRequest::Stop);
    if before_request {
        out.skip_request = true;
    }
}
```

Register `res::op_res_get_url`, `res::op_res_get_size`, `res::op_res_set_body` and `rok::op_rok_stop_execution` in `engine.rs`.

- [ ] **Step 5: Add the JS wrappers**

In `bootstrap.js`, change the `res` section. Add above `globalThis.res = {`:

```js
  // A body set by res.setBody replaces the stored one for the rest of this script.
  let _resBodyOverride = null;
  const _rawBody = () => (_resBodyOverride !== null ? _resBodyOverride : __ops.op_res_get_body());
```

Replace the `getBody` member of `globalThis.res` with:

```js
    getBody:          (opts)  => {
      const raw = _rawBody();
      return (opts && opts.raw) ? raw : _resBody(raw);
    },
    setBody:          (body)  => {
      const raw = typeof body === 'string' ? body : JSON.stringify(body);
      __ops.op_res_set_body(raw);
      _resBodyOverride = raw;
    },
    getUrl:           ()      => __ops.op_res_get_url(),
    getSize:          ()      => JSON.parse(__ops.op_res_get_size()),
```

and in the `Object.defineProperties(globalThis.res, {` block replace the `body` getter and add `url`:

```js
    body:         { get: () => _resBody(_rawBody()), enumerable: false },
    url:          { get: () => __ops.op_res_get_url(), enumerable: false },
```

In the `rok.runner` block add:

```js
      stopExecution:   ()      => __ops.op_rok_stop_execution(),
      iterationIndex:  0,
      totalIterations: 1,
```

- [ ] **Step 6: Run the engine tests**

Run: `cargo test -j4 -p rocket-infra res_url_and res_get_size res_set_body rok_stop_execution rok_runner_iteration`
Expected: PASS (7 tests). Then `cargo test -j4 -p rocket-infra scripting`.

- [ ] **Step 7: Write the failing app-layer tests**

In `execution_service.rs` tests:

```rust
    #[test]
    fn with_body_override_replaces_text_body_and_size() {
        let original = HttpResponse {
            status: 200,
            body: "{\"a\":1}".into(),
            size_bytes: 7,
            is_binary: true,
            body_base64: Some("e30=".into()),
            ..Default::default()
        };
        let patched = with_body_override(&original, Some("{\"a\":22}"));
        assert_eq!(patched.body, "{\"a\":22}");
        assert_eq!(patched.size_bytes, 8);
        assert!(!patched.is_binary);
        assert!(patched.body_base64.is_none());
        assert_eq!(patched.status, 200);

        let unchanged = with_body_override(&original, None);
        assert_eq!(unchanged.body, "{\"a\":1}");
    }

    #[tokio::test]
    async fn tests_script_sees_the_body_set_by_the_after_response_script() {
        let capture = CapturingScriptEngine::with_after_response(ScriptResult {
            response_body: Some("patched".into()),
            ..Default::default()
        });
        let svc = build_svc_with_script(
            Box::new(SharedEnvRepo(RecordingEnvRepo::with_env(Environment::new("dev")))),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedCapture(Arc::clone(&capture))),
        );

        let mut input = sample_input("https://example.com", Some("dev"));
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        let output = svc.execute(input).await.expect("execute failed");

        let contexts = capture.contexts();
        let tests_ctx = contexts
            .iter()
            .find(|c| c.phase == rocket_scripting::ScriptPhase::Tests)
            .expect("tests phase ran");
        assert_eq!(
            tests_ctx.response.as_ref().map(|r| r.body.as_str()),
            Some("patched")
        );
        // The response shown to the user is the real one.
        assert_ne!(output.response.body, "patched");
    }
```

- [ ] **Step 8: Run to verify they fail**

Run: `cargo test -j4 -p rocket-app with_body_override tests_script_sees_the_body`
Expected: FAIL to compile (`cannot find function with_body_override`).

- [ ] **Step 9: Implement the app-layer changes**

In `execution_service.rs`:

1. Add near `merge_runtime_vars`:

```rust
/// Returns a copy of `response` whose text body is replaced, for later script phases.
fn with_body_override(response: &HttpResponse, body: Option<&str>) -> HttpResponse {
    let mut patched = response.clone();
    if let Some(body) = body {
        patched.body = body.to_string();
        patched.size_bytes = body.len();
        patched.is_binary = false;
        patched.body_base64 = None;
    }
    patched
}
```

2. Add to `PhaseState` after `file_scope`:

```rust
    /// Body set by an after-response `res.setBody`, shown to the tests script only.
    pub response_body_override: Option<String>,
```

and `response_body_override: None,` to the `PhaseState { ... }` literal in `begin_phases` (and to any other literal; `cargo check -j4 --tests -p rocket-app` lists them).

3. In `run_after_response_phase`, after the `if result.next_request.is_some() { ... }` block inside the script branch, add:

```rust
                if result.response_body.is_some() {
                    state.response_body_override = result.response_body.clone();
                }
```

4. In `run_tests_phase`, build the context from the patched response. Directly before `let ctx = ScriptContext::tests(`, add:

```rust
                let script_response =
                    with_body_override(response, state.response_body_override.as_deref());
```

and pass `script_response` instead of `response.clone()` as the response argument of `ScriptContext::tests(`. Leave the `assertion_outcomes(&input.assertions, response)` argument using the real `response`.

- [ ] **Step 10: Run the app-layer tests**

Run: `cargo test -j4 -p rocket-app with_body_override tests_script_sees_the_body tests_script_receives_precomputed`
Expected: PASS.

- [ ] **Step 11: Add typings and run the full checks**

In `RES_DEFS` add (inside `declare const res: { ... }`):

```ts
  /** The request URL. The final redirect URL is not tracked. */
  getUrl(): string;
  /** Response size in bytes. */
  getSize(): { body: number; headers: number; total: number };
  /** Replaces the body that later scripts see. The stored response is not changed. */
  setBody(body: unknown): void;
  /** The request URL. Same as getUrl(). */
  readonly url: string;
```

In the `runner` block of `ROK_DEFS` add:

```ts
    /** Stop the whole run. Before the request it also skips sending it. Only meaningful during a Collection Runner run. */
    stopExecution(): void;
    /** Zero-based index of the current iteration. Always 0 until data-driven runs exist. */
    readonly iterationIndex: number;
    /** Total number of iterations. Always 1 until data-driven runs exist. */
    readonly totalIterations: number;
```

Run:

```bash
cargo check -j4
cargo test -j4 -p rocket-scripting
cargo test -j4 -p rocket-infra scripting
cargo test -j4 -p rocket-app execution_service
cargo test -j4 -p rocket-app collection_runner_service
yarn tsc --noEmit
yarn check
yarn test rok-types
```

Expected: all PASS.

- [ ] **Step 12: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-scripting/src/result.rs`, `crates/rocket-infra/src/scripting/state.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/ops/res.rs`, `crates/rocket-infra/src/scripting/ops/rok.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `crates/rocket-app/src/execution_service.rs`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add rok response and runner extras`.

## Manual check (real app, after both plans)

Run `yarn tauri dev` and in a request's post-response script try: `rok.setVar('n', 0); console.log(rok.getVar('n'), rok.hasVar('n'))`, then `rok.deleteVar('n')`, `res.getSize()`, `res.setBody({ a: 2 })` followed by a tests script reading `res.body.a`. In a collection with Developer mode, check `rok.cwd()` and `__dirname`.
