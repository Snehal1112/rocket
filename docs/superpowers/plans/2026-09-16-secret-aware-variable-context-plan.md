# Secret-Aware Variable Context and Console/Test-Output Redaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `VariableContext` a content-addressed set of secret values (`secret_values: HashSet<String>`), populate it wherever a secret-flagged `Variable`/`CollectionVariable` is copied into a scope, and redact every occurrence of those values out of script-emitted `console.log/warn/error` text and `rok.test()` failure messages — without touching the real values used to build the actual outgoing HTTP request.

**Architecture:** `VariableContext` (rocket-environment) gains a new `secret_values` field alongside its existing scope maps. `RequestExecutionService::build_variable_scopes` and the `global_env` loading block in `execute()` (rocket-app) populate it whenever they copy a `secret: true` variable's value into a scope, gated by a `MIN_REDACTION_LEN` floor. `ScriptInputState` (rocket-infra) carries a copy of that set into the Deno `OpState`; a new `redact()` helper in `scripting/ops/mod.rs` does simple substring replacement against it, called from the three `console.*` ops and from `op_test_fail`. `req`/`res` ops are untouched — redaction only ever touches the console/test-failure observability surface, never the real request/response data path.

**Tech Stack:** Rust (Cargo workspace), `deno_core`/`op2` macros for the JS sandbox, `HashSet<String>` for redaction matching, `tokio::test` / `#[test]` for TDD.

**Spec:** `docs/superpowers/specs/2026-09-16-secret-aware-variable-context-spec.md`

## Global Constraints

- Redaction matches on variable **value**, not key/scope — content-based, so a script copying a secret into a different variable name is still caught (spec §3.1).
- `MIN_REDACTION_LEN = 6`: secrets shorter than this are never added to `secret_values` and are never redacted. This is a documented, deliberate trade-off (spec §3.4) — do not "fix" it without revisiting the trade-off, and do not silently change the constant.
- Redaction happens only in `console.log/warn/error` and `op_test_fail`'s error string. `rok.interpolate` and all `req`/`res` ops are never redacted — the real value must reach the actual outgoing request (spec §3.3).
- `flatten()` / `flatten_with_process_env()` on `VariableContext` must remain unchanged in behavior — `secret_values` is a side list, not a filter on the scope maps.
- `CollectionVariable.secret` already exists in `rocket-collection` (`crates/rocket-collection/src/settings.rs:18`) but `crates/rocket-infra/src/conversions/variables.rs:18` hardcodes `secret: false` on every YAML→domain conversion. This plan writes the collection-scope population logic anyway (per spec §5) — it is correct and safe, it simply won't fire for YAML-imported collection variables until that separate, out-of-scope gap is fixed. Do not expand any task in this plan to fix `conversions/variables.rs`.
- Final acceptance: `cargo test -p rocket-environment -p rocket-infra -p rocket-app` passes (spec criterion 8).
- Every task below touches environment/collection variable-resolution scope, so every task's first step is the OpenCollection spec-reference read required by `CLAUDE.md`'s injection rule.
- No `unwrap()` in any new code — test assertions use `.expect("<reason>")` instead, per this repo's "never unwrap() in production paths" convention and its pre-commit content check.

---

### Task 1: `VariableContext` gains `secret_values`

**Files:**
- Modify: `crates/rocket-environment/src/context.rs:1-12` (struct definition), `:139-152` (`full_hierarchy_runtime_wins` test, which constructs `VariableContext` as a full struct literal with no `..Default::default()` and will fail to compile once the field is added)
- Test: `crates/rocket-environment/src/context.rs` (`#[cfg(test)] mod tests`, same file)

**Interfaces:**
- Produces: `VariableContext.secret_values: std::collections::HashSet<String>` — new public field, `#[derive(Default)]`-covered (empty `HashSet` by default). `flatten()` and `flatten_with_process_env()` signatures and behavior are unchanged.

- [ ] **Step 1: Read the required background doc**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Add these to the existing `#[cfg(test)] mod tests` block in `crates/rocket-environment/src/context.rs` (after the existing `full_hierarchy_runtime_wins` test):

```rust
    #[test]
    fn secret_values_defaults_to_empty() {
        assert!(VariableContext::default().secret_values.is_empty());
    }

    #[test]
    fn secret_values_does_not_affect_flatten() {
        let mut ctx = VariableContext {
            env: m(&[("API_KEY", "sk-live-abcdef123")]),
            ..Default::default()
        };
        ctx.secret_values.insert("sk-live-abcdef123".to_string());
        // flatten() still returns the real value — secret_values is a
        // separate redaction list, not a filter on the scope maps.
        let flat = ctx.flatten();
        assert_eq!(flat.get("API_KEY").expect("API_KEY present"), "sk-live-abcdef123");
    }

    #[test]
    fn secret_values_does_not_affect_flatten_with_process_env() {
        let mut ctx = VariableContext {
            env: m(&[("API_KEY", "sk-live-abcdef123")]),
            ..Default::default()
        };
        ctx.secret_values.insert("sk-live-abcdef123".to_string());
        let flat = ctx.flatten_with_process_env();
        assert_eq!(flat.get("API_KEY").expect("API_KEY present"), "sk-live-abcdef123");
    }

    #[test]
    fn secret_values_is_content_addressed_not_tied_to_a_scope_key() {
        // A value can be marked sensitive without needing to also appear
        // in any scope map — redaction matches on the value alone.
        let mut ctx = VariableContext::default();
        ctx.secret_values.insert("standalone-secret".to_string());
        assert!(ctx.secret_values.contains("standalone-secret"));
        assert!(ctx.flatten().is_empty());
    }
```

Also update the existing `full_hierarchy_runtime_wins` test (it constructs every field explicitly, so it will fail to compile — not just fail an assertion — once `secret_values` is added):

```rust
    #[test]
    fn full_hierarchy_runtime_wins() {
        // All 8 scopes present — runtime must win.
        let ctx = VariableContext {
            runtime:       m(&[("k", "runtime")]),
            request:       m(&[("k", "request")]),
            folder:        m(&[("k", "folder")]),
            env:           m(&[("k", "env")]),
            collection:    m(&[("k", "collection")]),
            global_env:    m(&[("k", "global")]),
            process_env:   m(&[("k", "process")]),
            secret_values: std::collections::HashSet::new(),
        };
        assert_eq!(ctx.flatten().get("k").expect("k present"), "runtime");
    }
```

- [ ] **Step 3: Run tests, confirm they fail to compile**

Run: `cargo test -p rocket-environment`
Expected: compile error — `no field secret_values on type VariableContext` (and/or `missing field secret_values in initializer` for `full_hierarchy_runtime_wins`).

- [ ] **Step 4: Add the field**

In `crates/rocket-environment/src/context.rs`, change the top of the file:

```rust
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime:     HashMap<String, String>,
    pub request:     HashMap<String, String>,
    pub folder:      HashMap<String, String>,
    pub env:         HashMap<String, String>,
    pub collection:  HashMap<String, String>,
    pub global_env:  HashMap<String, String>,
    pub process_env: HashMap<String, String>,
    /// Keys (from any scope) whose *value* must be redacted if it appears in
    /// script-emitted console/test-error text. Not a per-scope map — a value is
    /// either sensitive or not, regardless of which scope surfaced it.
    pub secret_values: HashSet<String>,
}
```

Leave `flatten()` and `flatten_with_process_env()` exactly as they are — they must not reference `secret_values`.

- [ ] **Step 5: Run tests, confirm they pass**

Run: `cargo test -p rocket-environment`
Expected: PASS, all tests including the 3 new ones and the fixed `full_hierarchy_runtime_wins`.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/context.rs
git commit -m "feat(rocket-environment): add secret_values to VariableContext"
```

---

### Task 2: Populate `secret_values` for env and collection scopes in `build_variable_scopes`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:145-194` (`build_variable_scopes`), plus a new module-level `const` near the top of the `impl RequestExecutionService` block
- Test: `crates/rocket-app/src/execution_service.rs` (`#[cfg(test)] mod tests`, same file — reuse the existing `CapturingEngine` helper defined around line 2437 and the existing `cv()` helper at line 1434)

**Interfaces:**
- Consumes: `VariableContext.secret_values: HashSet<String>` (Task 1). `rocket_environment::Variable { key, value, enabled, secret, .. }` (already exists, `crates/rocket-environment/src/variable.rs`). `rocket_collection::CollectionVariable { key, value, initial_value, enabled, secret }` (already exists, `crates/rocket-collection/src/settings.rs`).
- Produces: `RequestExecutionService::build_variable_scopes` now inserts into `ctx.secret_values` for every enabled, secret-flagged env or collection variable whose value is at least `MIN_REDACTION_LEN` (6) characters long. `MIN_REDACTION_LEN: usize` constant, module-private to `execution_service.rs`.

- [ ] **Step 1: Read the required background doc**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Add these to `#[cfg(test)] mod tests` in `crates/rocket-app/src/execution_service.rs`, placed after the existing `before_request_script_sees_scope_separated_variables` test (~line 2511) so they can reuse the `CapturingEngine` struct defined just above it:

```rust
    #[tokio::test]
    async fn secret_env_and_collection_vars_populate_secret_values() {
        let settings = CollectionSettings {
            variables: vec![
                CollectionVariable {
                    key: "COL_SECRET".into(),
                    value: "col-secret-val".into(),
                    initial_value: String::new(),
                    enabled: true,
                    secret: true,
                },
                cv("COL_PLAIN", "col-plain-val"),
            ],
            ..Default::default()
        };
        let collection_repo = StubCollectionRepo::with_settings(settings);

        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::secret("API_KEY", "sk-live-abcdef123"));
        active_env.set_variable(Variable::new("PLAIN", "plain-not-secret"));
        let env_repo = MockEnvRepo::with_env(active_env);

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
        struct SharedCapturingEngineSecrets(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineSecrets {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(collection_repo),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineSecrets(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.collection = Some("my-api".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(captured.secret_values.contains("sk-live-abcdef123"), "secret env var value must be in secret_values");
        assert!(captured.secret_values.contains("col-secret-val"), "secret collection var value must be in secret_values");
        assert!(!captured.secret_values.contains("plain-not-secret"), "non-secret env var value must not be in secret_values");
        assert!(!captured.secret_values.contains("col-plain-val"), "non-secret collection var value must not be in secret_values");
    }

    #[tokio::test]
    async fn short_secret_value_is_not_added_to_secret_values() {
        // Documented limitation (MIN_REDACTION_LEN = 6): secrets shorter
        // than this are not added to secret_values, so they are never
        // redacted. Asserted explicitly so this doesn't get "fixed"
        // accidentally later without revisiting the trade-off.
        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::secret("SHORT", "abc")); // 3 chars < MIN_REDACTION_LEN
        let env_repo = MockEnvRepo::with_env(active_env);

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
        struct SharedCapturingEngineShort(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineShort {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineShort(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(!captured.secret_values.contains("abc"), "secrets shorter than MIN_REDACTION_LEN must not be added to secret_values");
    }
```

- [ ] **Step 3: Run tests, confirm they fail**

Run: `cargo test -p rocket-app secret_env_and_collection_vars_populate_secret_values`
Run: `cargo test -p rocket-app short_secret_value_is_not_added_to_secret_values`
Expected: FAIL — `assertion failed: captured.secret_values.contains(...)` (the set is empty; `secret_values` isn't populated yet).

- [ ] **Step 4: Add `MIN_REDACTION_LEN` and update `build_variable_scopes`**

In `crates/rocket-app/src/execution_service.rs`, add a module-level constant just above `impl RequestExecutionService {` (after the `RequestExecutionService` struct definition, ~line 89):

```rust
/// Secrets shorter than this are not added to `VariableContext.secret_values`
/// and are therefore never redacted in console/test-failure output. A
/// documented, deliberate trade-off (see
/// docs/superpowers/specs/2026-09-16-secret-aware-variable-context-spec.md
/// §3.4) — redacting every occurrence of a very short string risks
/// over-redacting unrelated output.
const MIN_REDACTION_LEN: usize = 6;
```

Replace the collection and env blocks inside `build_variable_scopes` (currently lines ~162-175):

```rust
        if let Some(col) = collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                let val = effective_val(cv);
                ctx.collection.insert(cv.key.clone(), val.clone());
                if cv.secret && val.len() >= MIN_REDACTION_LEN {
                    ctx.secret_values.insert(val);
                }
            }
        }

        if let Some(name) = environment_name {
            if let Ok(env) = self.env_repo.get(name) {
                for var in env.variables.iter().filter(|v| v.enabled) {
                    ctx.env.insert(var.key.clone(), var.value.clone());
                    if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                        ctx.secret_values.insert(var.value.clone());
                    }
                }
            }
        }
```

Note the env block no longer calls `env.enabled_variables()` (which returns `Vec<(&str, &str)>` and loses the `secret` flag) — it now iterates `env.variables` directly, filtering on `.enabled`, so `var.secret` is available.

- [ ] **Step 5: Run tests, confirm they pass**

Run: `cargo test -p rocket-app secret_env_and_collection_vars_populate_secret_values`
Run: `cargo test -p rocket-app short_secret_value_is_not_added_to_secret_values`
Expected: PASS

- [ ] **Step 6: Run the full `rocket-app` test suite to confirm no regressions**

Run: `cargo test -p rocket-app`
Expected: PASS (in particular, `before_request_script_sees_scope_separated_variables`, `folder_vars_override_collection_vars`, and `full_precedence_collection_lt_env_lt_folder_lt_request` must still pass unchanged — this task only adds insertions into `ctx.secret_values`, it does not change what's inserted into `ctx.collection`/`ctx.env`).

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(rocket-app): populate VariableContext.secret_values for secret env/collection vars"
```

---

### Task 3: Populate `secret_values` for the global-environment scope in `execute()`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:574-580` (the `global_env_name` loading block inside `execute()`)
- Test: `crates/rocket-app/src/execution_service.rs` (`#[cfg(test)] mod tests` — reuse `MultiEnvRepo` and `CapturingEngine`, both defined ~line 2415-2447)

**Interfaces:**
- Consumes: `MIN_REDACTION_LEN` (Task 2). `Environment.variables: Vec<Variable>` (existing, `crates/rocket-environment/src/environment.rs:16`).
- Produces: `execute()`'s global-environment loading block now inserts secret-flagged values into `var_ctx.secret_values`, same as Task 2 does for `env`/`collection`.

- [ ] **Step 1: Read the required background doc**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing test**

Add to `#[cfg(test)] mod tests` in `crates/rocket-app/src/execution_service.rs`, after the Task 2 tests:

```rust
    #[tokio::test]
    async fn global_env_secret_populates_secret_values() {
        let mut active_env = Environment::new("dev");
        active_env.set_variable(Variable::new("BASE_URL", "https://dev.local"));
        let mut global_env = Environment::new("shared-global");
        global_env.set_variable(Variable::secret("GLOBAL_TOKEN", "glbl-secret-999"));
        global_env.set_variable(Variable::new("GLOBAL_PLAIN", "glbl-plain-val"));
        let env_repo = MultiEnvRepo::new(vec![active_env, global_env]);

        let engine = Arc::new(CapturingEngine { captured: Mutex::new(None) });
        struct SharedCapturingEngineGlobal(Arc<CapturingEngine>);
        #[async_trait]
        impl ScriptEngine for SharedCapturingEngineGlobal {
            async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
                self.0.execute(ctx).await
            }
        }
        let engine_arc = Arc::clone(&engine);

        let svc = RequestExecutionService::new(
            Box::new(env_repo),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedCapturingEngineGlobal(engine)));

        let mut input = sample_input("https://example.com", Some("dev"));
        input.global_env_name = Some("shared-global".into());
        input.pre_request_script = Some("console.log('probe')".into());

        svc.execute(input).await.expect("execute should succeed");

        let captured = engine_arc.captured.lock().expect("lock").clone().expect("engine was called");
        assert!(captured.secret_values.contains("glbl-secret-999"), "secret global env var value must be in secret_values");
        assert!(!captured.secret_values.contains("glbl-plain-val"), "non-secret global env var value must not be in secret_values");
    }
```

- [ ] **Step 3: Run test, confirm it fails**

Run: `cargo test -p rocket-app global_env_secret_populates_secret_values`
Expected: FAIL — `captured.secret_values.contains("glbl-secret-999")` is false.

- [ ] **Step 4: Update the global-env loading block**

In `crates/rocket-app/src/execution_service.rs`, inside `execute()`, replace:

```rust
        if let Some(name) = input.global_env_name.as_deref() {
            if let Ok(global_env) = self.env_repo.get(name) {
                for (k, v) in global_env.enabled_variables() {
                    var_ctx.global_env.insert(k.to_string(), v.to_string());
                }
            }
        }
```

with:

```rust
        if let Some(name) = input.global_env_name.as_deref() {
            if let Ok(global_env) = self.env_repo.get(name) {
                for var in global_env.variables.iter().filter(|v| v.enabled) {
                    var_ctx.global_env.insert(var.key.clone(), var.value.clone());
                    if var.secret && var.value.len() >= MIN_REDACTION_LEN {
                        var_ctx.secret_values.insert(var.value.clone());
                    }
                }
            }
        }
```

- [ ] **Step 5: Run test, confirm it passes**

Run: `cargo test -p rocket-app global_env_secret_populates_secret_values`
Expected: PASS

- [ ] **Step 6: Run the full `rocket-app` test suite**

Run: `cargo test -p rocket-app`
Expected: PASS, including `before_request_script_sees_scope_separated_variables` (which asserts `captured.global_env.get("ORG_ID")` — must be unaffected by this change).

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(rocket-app): populate VariableContext.secret_values for the global environment scope"
```

---

### Task 4: Thread `secret_values` into `ScriptInputState` and redact `console.log/warn/error`

**Files:**
- Modify: `crates/rocket-infra/src/scripting/state.rs:1,12-23` (`ScriptInputState`), `crates/rocket-infra/src/scripting/ops/mod.rs` (add `redact()`), `crates/rocket-infra/src/scripting/ops/console.rs` (all three ops), `crates/rocket-infra/src/scripting/engine.rs:145-162` (`run_script`'s `OpState` seeding)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (`#[cfg(test)] mod tests`, reuse the `minimal_ctx` helper already defined there)

**Interfaces:**
- Consumes: `VariableContext.secret_values` (Task 1). `ScriptContext.variables: VariableContext` (existing, `crates/rocket-scripting/src/context.rs:19`).
- Produces: `ScriptInputState.secret_values: HashSet<String>` — new field. `crates/rocket-infra/src/scripting/ops/mod.rs::redact(state: &deno_core::OpState, msg: String) -> String` — new public helper, used by `op_console_log`/`op_console_warn`/`op_console_error` in this task and by `op_test_fail` in Task 5.

- [ ] **Step 1: Read the required background doc**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Add these to `#[cfg(test)] mod tests` in `crates/rocket-infra/src/scripting/engine.rs`, after the existing `console_warn_and_error_captured` test:

```rust
    #[tokio::test]
    async fn console_log_redacts_secret_env_var() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('API_KEY'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 1);
        assert_eq!(result.console_entries[0].message, "••••••");
    }

    #[tokio::test]
    async fn console_log_redacts_secret_substring_in_larger_string() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log('token=' + rok.getEnvVar('API_KEY'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "token=••••••");
    }

    #[tokio::test]
    async fn console_log_redacts_value_copied_to_different_scope_key() {
        // Redaction is content-based, not name/scope-based: a secret value
        // placed in the runtime scope under a *different* key from where it
        // was originally read is still caught.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.runtime.insert("copy".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx("console.log(rok.getVar('copy'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "••••••");
    }

    #[tokio::test]
    async fn two_phase_set_var_then_get_var_redacts_copied_secret() {
        // Reproduces the real production flow: a before-request script
        // copies a secret into a runtime var with rok.setVar, the host
        // (RequestExecutionService::apply_script_side_effects, rocket-app)
        // merges that write into VariableContext.runtime for the next
        // phase, and a later phase's console.log of the copy is still
        // redacted — even though op_rok_get_var only ever reads the
        // *input* snapshot, never the current phase's own writes.
        let engine = DenoScriptEngine::new();

        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());

        let mut ctx1 = minimal_ctx("rok.setVar('copy', rok.getEnvVar('API_KEY'))");
        ctx1.variables = vars.clone();
        let result1 = engine.execute(ctx1).await.expect("execute phase 1");
        let copied = result1
            .runtime_vars
            .get("copy")
            .and_then(|v| v.as_str())
            .expect("copy runtime var present")
            .to_string();
        assert_eq!(copied, "sk-live-abcdef123");

        // Simulate apply_script_side_effects merging runtime_vars into the
        // context carried forward to the next phase.
        vars.runtime.insert("copy".into(), copied);

        let mut ctx2 = minimal_ctx("console.log(rok.getVar('copy'))");
        ctx2.variables = vars;
        let result2 = engine.execute(ctx2).await.expect("execute phase 2");
        assert_eq!(result2.console_entries[0].message, "••••••");
    }

    #[tokio::test]
    async fn console_warn_and_error_redact_secret_values() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "console.warn(rok.getEnvVar('API_KEY')); console.error('key: ' + rok.getEnvVar('API_KEY'))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 2);
        assert_eq!(result.console_entries[0].message, "••••••");
        assert_eq!(result.console_entries[1].message, "key: ••••••");
    }

    #[tokio::test]
    async fn non_secret_variable_value_is_not_redacted() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("PLAIN".into(), "plain-value-123".into());
        // secret_values intentionally left empty.
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('PLAIN'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "plain-value-123");
    }

    #[tokio::test]
    async fn overlapping_secret_substrings_do_not_panic() {
        // One secret's value is a substring of another's. Whichever order
        // redact()'s HashSet iteration replaces them in, this must not
        // panic, and the longer secret's full raw value must not survive.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("SHORT".into(), "abcdef1".into());
        vars.env.insert("LONG".into(), "abcdef123456".into());
        vars.secret_values.insert("abcdef1".into());
        vars.secret_values.insert("abcdef123456".into());
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('LONG'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert!(!result.console_entries[0].message.contains("abcdef123456"));
    }

    #[tokio::test]
    async fn short_secret_not_in_secret_values_is_not_redacted() {
        // Documents the MIN_REDACTION_LEN trade-off (enforced upstream in
        // RequestExecutionService::build_variable_scopes, rocket-app, Task
        // 2 of this plan): a secret this short is never added to
        // secret_values, so redact() has nothing to match and the raw
        // value passes through unchanged. This is the deliberate,
        // documented limitation from spec §3.4.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("SHORT".into(), "abc".into());
        // secret_values intentionally does NOT contain "abc" — mirrors
        // what rocket-app does for a value under MIN_REDACTION_LEN.
        let mut ctx = minimal_ctx("console.log(rok.getEnvVar('SHORT'))");
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries[0].message, "abc");
    }
```

- [ ] **Step 3: Run tests, confirm they fail**

Run: `cargo test -p rocket-infra console_log_redacts_secret_env_var`
Expected: `VariableContext.secret_values` already exists from Task 1, so this compiles. The tests instead fail on the assertion: `assertion 'left == right' failed`, `left: "sk-live-abcdef123"` (unredacted), `right: "••••••"` — because `ScriptInputState` doesn't have `secret_values` yet and `console.rs`'s ops don't call any redaction helper yet, so nothing strips the secret.

- [ ] **Step 4: Add `secret_values` to `ScriptInputState`**

In `crates/rocket-infra/src/scripting/state.rs`, change the top of the file and the struct:

```rust
use std::collections::{HashMap, HashSet};
use rocket_http::{HttpRequest, HttpResponse};
use rocket_scripting::{
    CollectionVarWrite, ConsoleEntry, ConsoleLevel, EnvVarWrite,
    NextRequest, RequestMutations, ScriptPhase, TestResult, TestStatus,
};
use rocket_environment::VariableContext;
use rocket_shared::types::PathParam;

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
    pub request_name: String,
    pub request_tags: Vec<String>,
    pub path_params: Vec<PathParam>,
    /// Values that must be redacted if they appear in script-emitted
    /// console/test-error text. Copied from `variables.secret_values` when
    /// this state is seeded in `run_script` (engine.rs) — kept as its own
    /// field so ops that only need the redaction list (console/test-fail
    /// ops) don't have to reach through `variables`.
    pub secret_values: HashSet<String>,
}
```

(`ScriptOutputState` below it is unchanged.)

- [ ] **Step 5: Add the `redact()` helper to `ops/mod.rs`**

Replace the full contents of `crates/rocket-infra/src/scripting/ops/mod.rs` with:

```rust
use deno_core::OpState;
use crate::scripting::state::ScriptInputState;

pub mod console;
pub mod req;
pub mod res;
pub mod rok;

/// JS-visible error for op failures (phase guard, unavailable state, etc.).
#[derive(Debug, thiserror::Error, deno_error::JsError)]
#[class(type)]
#[error("{0}")]
pub struct ScriptOpError(pub String);

/// Redacts every known secret value out of `msg`, replacing each occurrence
/// with `"••••••"`. Matching is content-based (against the actual variable
/// *value*, not its key or scope), so a script that copies a secret into a
/// differently-named variable is still caught. Used by the console ops
/// (`ops/console.rs`) and by `op_test_fail` (`engine.rs`) — the two places
/// script-emitted text reaches the UI/event bus. Never used by `req`/`res`
/// ops or `rok.interpolate` — those must carry the real value.
pub fn redact(state: &OpState, msg: String) -> String {
    let secrets = &state.borrow::<ScriptInputState>().secret_values;
    if secrets.is_empty() {
        return msg;
    }
    let mut out = msg;
    for s in secrets {
        out = out.replace(s.as_str(), "••••••");
    }
    out
}
```

- [ ] **Step 6: Update `console.rs`'s three ops to redact**

Replace the full contents of `crates/rocket-infra/src/scripting/ops/console.rs` with:

```rust
/// Console ops — capture log/warn/error into ScriptOutputState, redacting
/// any known secret values first (see ops/mod.rs::redact).
use deno_core::{op2, OpState};
use rocket_scripting::ConsoleLevel;
use crate::scripting::ops::redact;
use crate::scripting::state::ScriptOutputState;

#[op2(fast)]
pub fn op_console_log(state: &mut OpState, #[string] msg: String) {
    let redacted = redact(state, msg);
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Log, redacted);
}

#[op2(fast)]
pub fn op_console_warn(state: &mut OpState, #[string] msg: String) {
    let redacted = redact(state, msg);
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Warn, redacted);
}

#[op2(fast)]
pub fn op_console_error(state: &mut OpState, #[string] msg: String) {
    let redacted = redact(state, msg);
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Error, redacted);
}
```

- [ ] **Step 7: Seed `secret_values` in `run_script`**

In `crates/rocket-infra/src/scripting/engine.rs`, inside `run_script`, replace the `OpState` seeding block:

```rust
    // Seed OpState with input and output state.
    {
        let op_state = runtime.op_state();
        let mut state = op_state.borrow_mut();
        let secret_values = ctx.variables.secret_values.clone();
        state.put(ScriptInputState {
            phase: ctx.phase,
            variables: ctx.variables,
            request: ctx.request,
            response: ctx.response,
            env_name: ctx.env_name,
            execution_mode: ctx.execution_mode,
            execution_platform: ctx.execution_platform,
            request_name: ctx.request_name,
            request_tags: ctx.request_tags,
            path_params: ctx.path_params,
            secret_values,
        });
        state.put(ScriptOutputState::default());
    }
```

- [ ] **Step 8: Run tests, confirm they pass**

Run: `cargo test -p rocket-infra`
Expected: PASS, including all 7 new tests and every pre-existing test in `engine.rs` (especially `console_log_captured` and `console_warn_and_error_captured`, which have no secrets configured and must still see their raw messages unredacted).

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-infra/src/scripting/state.rs crates/rocket-infra/src/scripting/ops/mod.rs crates/rocket-infra/src/scripting/ops/console.rs crates/rocket-infra/src/scripting/engine.rs
git commit -m "feat(rocket-infra): redact secret values from console.log/warn/error output"
```

---

### Task 5: Redact `rok.test()` failure messages via `op_test_fail`

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs:6-7` (imports), `:51-54` (`op_test_fail`)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::scripting::ops::redact` (Task 4).
- Produces: `op_test_fail`'s `error` string is redacted before being stored in `TestResult.error` via `ScriptOutputState::add_test_result`. No signature change to `op_test_fail` itself.

- [ ] **Step 1: Read the required background doc**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Add these to `#[cfg(test)] mod tests` in `crates/rocket-infra/src/scripting/engine.rs`, after the tests added in Task 4:

```rust
    #[tokio::test]
    async fn rok_test_failure_message_redacts_secret_value() {
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "rok.test('leaks secret', () => { throw new Error(rok.getEnvVar('API_KEY')) })",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.test_results.len(), 1);
        assert_eq!(result.test_results[0].status, rocket_scripting::TestStatus::Failed);
        let err = result.test_results[0].error.as_ref().expect("error message present");
        // JS `String(new Error(msg))` formats as "Error: <msg>".
        assert_eq!(err, "Error: ••••••");
    }

    #[tokio::test]
    async fn req_set_header_with_secret_value_is_not_redacted() {
        // Redaction is an observability-surface-only concern (console/test
        // output). req.setHeader must still carry the real secret so the
        // actual outgoing HTTP request functions correctly.
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.env.insert("API_KEY".into(), "sk-live-abcdef123".into());
        vars.secret_values.insert("sk-live-abcdef123".into());
        let mut ctx = minimal_ctx(
            "req.setHeader('Authorization', 'Bearer ' + rok.getEnvVar('API_KEY'))",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        let mutations = result.request_mutations.expect("mutations present");
        assert!(matches!(
            mutations.headers.as_slice(),
            [rocket_scripting::HeaderMutation::Set { name, value }]
                if name == "Authorization" && value == "Bearer sk-live-abcdef123"
        ));
    }
```

- [ ] **Step 3: Run tests, confirm the first fails**

Run: `cargo test -p rocket-infra rok_test_failure_message_redacts_secret_value`
Expected: FAIL — `left: "Error: sk-live-abcdef123"`, `right: "Error: ••••••"`.

Run: `cargo test -p rocket-infra req_set_header_with_secret_value_is_not_redacted`
Expected: PASS already (this is a regression-guard test — `req` ops are untouched by this plan, so it should pass before and after Task 5's code change; it exists to prove that stays true).

- [ ] **Step 4: Update `op_test_fail`**

In `crates/rocket-infra/src/scripting/engine.rs`, update the import line:

```rust
use crate::scripting::ops::{console, redact, req, res, rok};
```

And update `op_test_fail`:

```rust
#[op2(fast)]
fn op_test_fail(state: &mut OpState, #[string] name: String, #[string] error: String) {
    let redacted = redact(state, error);
    state.borrow_mut::<ScriptOutputState>().add_test_result(name, false, Some(redacted));
}
```

- [ ] **Step 5: Run tests, confirm they pass**

Run: `cargo test -p rocket-infra rok_test_failure_message_redacts_secret_value`
Run: `cargo test -p rocket-infra req_set_header_with_secret_value_is_not_redacted`
Expected: PASS

- [ ] **Step 6: Run the full `rocket-infra` test suite**

Run: `cargo test -p rocket-infra`
Expected: PASS, including pre-existing `op_test_fail` consumers such as `require_chai_and_use_expect` (no thrown error, unaffected) and any other `rok.test`/`expect` tests already in this file.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/scripting/engine.rs
git commit -m "feat(rocket-infra): redact secret values from rok.test() failure messages"
```

---

### Task 6: Final verification

**Files:** none (verification only)

**Interfaces:** none

- [ ] **Step 1: Run the full three-crate test suite named in the spec's acceptance criterion 8**

Run: `cargo test -p rocket-environment -p rocket-infra -p rocket-app`
Expected: PASS, zero failures.

- [ ] **Step 2: `cargo check` the whole workspace**

Run: `cargo check`
Expected: no errors, no new warnings introduced by this plan's changes (in particular, check for an unused-import warning on `redact` or `HashSet` if a step was skipped).

- [ ] **Step 3: Walk the spec's acceptance criteria one by one and confirm the covering test**

| # | Criterion | Covering test |
|---|---|---|
| 1 | `console.log(rok.getEnvVar('API_KEY'))` → `"••••••"` | `engine.rs::console_log_redacts_secret_env_var` |
| 2 | Substring redaction inside a larger string | `engine.rs::console_log_redacts_secret_substring_in_larger_string` |
| 3 | Content-based redaction catches a copy via `rok.setVar` | `engine.rs::console_log_redacts_value_copied_to_different_scope_key` and `engine.rs::two_phase_set_var_then_get_var_redacts_copied_secret` |
| 4 | Failing `rok.test()` redacts in `TestResult.error` | `engine.rs::rok_test_failure_message_redacts_secret_value` |
| 5 | Non-secret value never redacted; no panic on overlapping substrings | `engine.rs::non_secret_variable_value_is_not_redacted` and `engine.rs::overlapping_secret_substrings_do_not_panic` |
| 6 | Secret shorter than 6 chars is not redacted (documented) | `execution_service.rs::short_secret_value_is_not_added_to_secret_values` and `engine.rs::short_secret_not_in_secret_values_is_not_redacted` |
| 7 | `req.setHeader(...)` unaffected — real value reaches the request | `engine.rs::req_set_header_with_secret_value_is_not_redacted` |
| 8 | `cargo test -p rocket-environment -p rocket-infra -p rocket-app` passes | Step 1 above |

- [ ] **Step 4: No commit for this task** — it is verification-only. If Step 1, 2, or 3 surfaces a gap, go back to the relevant task, fix it there, and re-run this task.
