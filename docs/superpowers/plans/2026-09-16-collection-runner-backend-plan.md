# Collection Runner — Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the backend for a Collection Runner that executes a folder's or collection's requests in sequence and actually honours `rok.runner.setNextRequest()` / `rok.runner.skipRequest()`.

**Architecture:** `RequestExecutionService::execute()` is split into phase-callable pieces that share one mutable `PhaseState`; `execute()` becomes a thin wrapper that runs every phase unconditionally (zero behaviour change), and a new `CollectionRunnerService` in `rocket-app` drives the same pieces one phase at a time so it can observe `skip_request` before sending and `next_request` after every phase that ran. Sequencing comes from the existing `_order.yml`-ordered tree returned by `CollectionRepository::get`. Progress is streamed with three new `DomainEvent` variants through the existing `TauriEventBus`.

**Tech Stack:** Rust (Cargo workspace: `rocket-scripting`, `rocket-shared`, `rocket-app`, `src-tauri`), Tauri v2 IPC + event bus, `tokio` async, `ulid` for run ids, plain `#[cfg(test)]` unit tests with in-memory doubles.

**Spec:** `docs/superpowers/specs/2026-09-16-collection-runner-design.md` (read §4, §5, §7 and §8 before starting — the resolved decisions there are binding).

## Global Constraints

- 📖 Before starting any task below, read `docs/superpowers/specs/opencollection-spec-reference.md` — every task in this plan touches collections, environments, request models, variable resolution, or the Tauri commands that deal with them.
- Frontend is **out of scope**. A separate plan covers it. This plan's only obligation to it is the "Frontend contract" section at the bottom, which must stay exactly in sync with the code.
- Rust: never `unwrap()` in production paths — use `?`, `let ... else`, or `unwrap_or_default()`. `expect()` is allowed in `#[cfg(test)]` code only.
- Domain crates hold logic and traits only; `rocket-infra` holds concrete I/O; `rocket-app` is trait-first (`Box<dyn Trait>` injection, no concrete infra types).
- `#[serde(rename_all = "camelCase")]` goes on IPC DTOs only, never on persistence structs.
- `DomainEvent` keeps its existing serde shape: `#[serde(tag = "type", rename_all = "camelCase")]` renames **variants** only, so struct-variant **fields serialise as snake_case** (verified: `DomainEvent::RequestExecuted` emits `{"type":"requestExecuted","method":"GET","url":"u","status":200,"duration_ms":5}`). Do not add per-variant renames — new variants must match the existing convention.
- Runner is strictly sequential in v1. No parallelism, no data-driven runs, no `rok.runRequest()`.
- Continue-on-failure is the default (`stop_on_failure: false`), matching Bruno's `--bail` being opt-in (spec §8.3).
- Commits: conventional commits (`feat:`, `refactor:`, `test:`, `docs:`).
- Verification commands: `cargo check`, `cargo test -p <crate>`, and `cargo test -p rocket-app --lib execution_service::` for the regression net. No frontend build is required by this plan.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-scripting/src/context.rs` (modify) | Add `ExecutionMode` enum + `ScriptContext::with_execution_mode`. The three constructors keep defaulting to `"standalone"`. |
| `crates/rocket-scripting/src/lib.rs` (modify) | Re-export `ExecutionMode`. |
| `crates/rocket-app/src/execution_service.rs` (modify) | Split `execute()` into `PhaseState` + six `pub(crate)` phase methods. `execute()` becomes their unconditional composition. |
| `crates/rocket-app/src/runner_sequence.rs` (create) | Pure sequencing: `RunItem`, `flatten_run_set`, `build_step_input`. No I/O, no service state — trivially unit-testable. |
| `crates/rocket-app/src/collection_runner_service.rs` (create) | `CollectionRunnerService` — the run loop, skip/jump/stop handling, runtime carry-forward, `stop_on_failure`, cancellation, event publishing. |
| `crates/rocket-app/src/test_doubles.rs` (create, `#[cfg(test)]`) | In-memory doubles shared by the new runner tests. `execution_service.rs` keeps its own doubles untouched. |
| `crates/rocket-app/src/lib.rs` (modify) | Module declarations + public re-exports. |
| `crates/rocket-shared/src/events.rs` (modify) | `RunnerStarted`, `RunnerStepCompleted`, `RunnerFinished` variants. |
| `src-tauri/src/tauri_event_bus.rs` (modify) | Map the three new variants to `runner-started` / `runner-step-completed` / `runner-finished`. |
| `src-tauri/src/commands/runner.rs` (create) | `run_collection`, `stop_collection_run`. Thin: validate, call service, map errors. |
| `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (modify) | Register the module, construct `CollectionRunnerService`, `app.manage` it, add both commands to `generate_handler!`. |
| `crates/rocket-app/CLAUDE.md` (modify) | Document the two new services/types rows. |

---

### Task 1: Settable script execution mode

**Files:**
- Modify: `crates/rocket-scripting/src/context.rs`
- Modify: `crates/rocket-scripting/src/lib.rs:6-12`
- Test: `crates/rocket-scripting/src/context.rs` (existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `rocket_scripting::ExecutionMode` (`Standalone` | `Runner`, `Copy`, `Default = Standalone`, `fn as_str(self) -> &'static str`) and `ScriptContext::with_execution_mode(self, mode: ExecutionMode) -> Self`.
- Consumes: nothing.

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-scripting/src/context.rs`:

```rust
    #[test]
    fn execution_mode_as_str_matches_script_api_strings() {
        assert_eq!(ExecutionMode::Standalone.as_str(), "standalone");
        assert_eq!(ExecutionMode::Runner.as_str(), "runner");
        assert_eq!(ExecutionMode::default(), ExecutionMode::Standalone);
    }

    #[test]
    fn with_execution_mode_overrides_the_standalone_default() {
        let ctx = ScriptContext::before_request(
            String::new(),
            VariableContext::default(),
            stub_request(),
            None,
            String::new(),
            vec![],
            vec![],
        )
        .with_execution_mode(ExecutionMode::Runner);
        assert_eq!(ctx.execution_mode, "runner");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-scripting context::`
Expected: FAIL — `cannot find type ExecutionMode in this scope`.

- [ ] **Step 3: Add the enum and the builder method**

In `crates/rocket-scripting/src/context.rs`, directly above `pub struct ScriptContext`:

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

impl ExecutionMode {
    /// The exact string the sandbox exposes. Do not change these values.
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionMode::Standalone => "standalone",
            ExecutionMode::Runner => "runner",
        }
    }
}
```

Then add this method at the end of `impl ScriptContext` (after the `tests` constructor):

```rust
    /// Overrides the execution mode. The three constructors default to
    /// `Standalone`; the Collection Runner sets `Runner` on every context it
    /// builds, so `req.getExecutionMode()` reports the truth.
    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode.as_str().to_string();
        self
    }
```

- [ ] **Step 4: Export the new type**

In `crates/rocket-scripting/src/lib.rs`, change the context re-export line to:

```rust
pub use context::{ExecutionMode, ScriptContext};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rocket-scripting`
Expected: PASS — all existing tests still pass, including `before_request_has_no_response` which asserts the `"standalone"` default.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-scripting/src/context.rs crates/rocket-scripting/src/lib.rs
git commit -m "feat(scripting): add settable ExecutionMode on ScriptContext"
```

---

### Task 2: Split `execute()` into phase-callable pieces (behaviour-preserving)

This is a **pure extraction**. No new fields, no new parameters, no behaviour change. The whole point is that the 33 existing `execution_service` tests pass unmodified afterwards, which is what licenses the runner to reuse these pieces.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:534-841` (the `#[tracing::instrument]` attribute through the end of `execute()`)
- Test: `crates/rocket-app/src/execution_service.rs` — the existing `#[cfg(test)] mod tests` block (lines 988-2527) **must not be edited**

**Interfaces:**
- Produces (all `pub(crate)`, all on `impl RequestExecutionService`):
  - `struct PhaseState { http_request: HttpRequest, var_ctx: VariableContext, script_error: Option<String>, console: Vec<ConsoleEntry>, test_results: Vec<TestResult> }`
  - `fn begin_phases(&self, input: &ExecuteRequestInput) -> DomainResult<PhaseState>`
  - `async fn run_before_request_phase(&self, input: &ExecuteRequestInput, state: &mut PhaseState)`
  - `async fn send_request(&self, state: &PhaseState) -> DomainResult<HttpResponse>`
  - `async fn run_after_response_phase(&self, input: &ExecuteRequestInput, response: &HttpResponse, state: &mut PhaseState)`
  - `async fn run_tests_phase(&self, input: &ExecuteRequestInput, response: &HttpResponse, state: &mut PhaseState)`
  - `async fn finish_phases(&self, input: &ExecuteRequestInput, response: HttpResponse, state: &mut PhaseState) -> ExecuteRequestOutput`
  - `fn publish_console(&self, request_name: &str, entries: &[ConsoleEntry])`
- Consumes: nothing from Task 1 (the `mode` parameter arrives in Task 3).

- [ ] **Step 1: Capture the pre-refactor baseline**

```bash
mkdir -p /tmp/rocket-runner-refactor
cargo test -p rocket-app --lib execution_service:: -- --list > /tmp/rocket-runner-refactor/tests_before.txt
cargo test -p rocket-app --lib execution_service:: 2>&1 | tail -3
awk '/^#\[cfg\(test\)\]/,0' crates/rocket-app/src/execution_service.rs > /tmp/rocket-runner-refactor/testmod_before.rs
wc -l /tmp/rocket-runner-refactor/testmod_before.rs
```

Expected: `test result: ok. 33 passed; 0 failed`. `testmod_before.rs` is 1540 lines. Both files are the equivalence baseline for Step 8.

- [ ] **Step 2: Add the `PhaseState` struct**

In `crates/rocket-app/src/execution_service.rs`, directly after the `ExecuteRequestOutput` struct (line 76) and before `pub struct RequestExecutionService`:

```rust
/// Mutable state threaded through the phases of one request execution.
///
/// `RequestExecutionService::execute()` and `CollectionRunnerService` both drive
/// the same phase methods against this struct, so phase orchestration is never
/// duplicated between the single-send path and the runner.
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
}
```

- [ ] **Step 3: Extract `begin_phases`**

Add to `impl RequestExecutionService`, directly above the `#[tracing::instrument]` attribute on `execute()`. This is lines 543-580 of the old body, verbatim:

```rust
    /// Resolves the request, emits the sensitive-auth audit event, and builds
    /// the scope-separated variable context. Every phase method below assumes
    /// this ran first.
    pub(crate) fn begin_phases(&self, input: &ExecuteRequestInput) -> DomainResult<PhaseState> {
        let http_request = self.resolve_request(input)?;

        // Emit a sensitive-auth audit event BEFORE dispatch when the resolved
        // request carries a real credential (not None / Inherit). This captures
        // the intent even if the network call itself fails.
        if let Some(auth_type) = sensitive_auth_label(&http_request.auth) {
            self.audit.publish(
                "system".into(),
                None,
                AuditEventKind::SensitiveAuthUsed {
                    auth_type: auth_type.to_string(),
                    collection: input.collection.clone().unwrap_or_default(),
                    request_path: input.request_path.clone().unwrap_or_default(),
                },
            );
        }

        // Build scope-separated variable context for script phases. Scripts read
        // individual scopes via rok.getCollectionVar/getEnvVar/getGlobalEnvVar, so
        // each scope must stay distinct rather than being pre-flattened into one.
        let mut var_ctx = self.build_variable_scopes(
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
        );
        if let Some(name) = input.global_env_name.as_deref() {
            if let Ok(global_env) = self.env_repo.get(name) {
                for (k, v) in global_env.enabled_variables() {
                    var_ctx.global_env.insert(k.to_string(), v.to_string());
                }
            }
        }

        Ok(PhaseState {
            http_request,
            var_ctx,
            script_error: None,
            console: Vec::new(),
            test_results: Vec::new(),
        })
    }
```

- [ ] **Step 4: Extract the before-request phase**

Add directly below `begin_phases`. This is lines 582-690 of the old body — the pre-request script, the request-mutation application, the side effects, and the before-request actions, in exactly that order:

```rust
    /// Runs the before-request script (if any), applies its request mutations
    /// and side effects, then runs the `before-request` declarative actions.
    pub(crate) async fn run_before_request_phase(
        &self,
        input: &ExecuteRequestInput,
        state: &mut PhaseState,
    ) {
        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();

        if let Some(code) = &input.pre_request_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::before_request(
                    code.clone(),
                    state.var_ctx.clone(),
                    state.http_request.clone(),
                    env_name.clone(),
                    request_name.clone(),
                    input.tags.clone(),
                    input.path_params.clone(),
                );
                let result = self.run_script_phase(
                    code, ctx, &request_name, "before-request", &mut state.console,
                ).await;

                // Apply request mutations.
                if let Some(ref mutations) = result.request_mutations {
                    if let Some(ref url) = mutations.url {
                        state.http_request.url = url.clone();
                    }
                    if let Some(ref method_str) = mutations.method {
                        if let Ok(m) = method_str.parse() {
                            state.http_request.method = m;
                        } else {
                            tracing::warn!(
                                method = %method_str,
                                "req.setMethod() called with an unrecognized HTTP method, ignored"
                            );
                            state.script_error.get_or_insert_with(|| format!(
                                "req.setMethod('{method_str}') is not a valid HTTP method — ignored."
                            ));
                        }
                    }
                    // Apply header mutations in the order the script issued them —
                    // e.g. deleteHeader() then setHeader() on the same name must
                    // result in the header being present, not dropped.
                    for mutation in &mutations.headers {
                        match mutation {
                            rocket_scripting::HeaderMutation::Set { name, value } => {
                                if let Some(h) = state.http_request.headers.iter_mut()
                                    .find(|h| h.key.eq_ignore_ascii_case(name))
                                {
                                    h.value = value.clone();
                                } else {
                                    state.http_request.headers.push(Header::new(name, value));
                                }
                            }
                            rocket_scripting::HeaderMutation::Delete { name } => {
                                state.http_request.headers.retain(|h| !h.key.eq_ignore_ascii_case(name));
                            }
                        }
                    }
                    if let Some(ms) = mutations.timeout_ms {
                        state.http_request.options.timeout_ms = ms;
                    }
                    if let Some(ref body_val) = mutations.body {
                        // A JS object/array is unambiguously meant as JSON. A string
                        // may be non-JSON text (XML, plain text, etc) — respect an
                        // explicit Content-Type header the script already set instead
                        // of forcing JSON, which would mislabel the body on the wire.
                        let mode = if body_val.is_object() || body_val.is_array() {
                            rocket_shared::types::BodyMode::Json
                        } else {
                            body_mode_from_content_type(&state.http_request.headers)
                        };
                        let content = body_val.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| body_val.to_string());
                        state.http_request.body = Some(rocket_shared::types::Body {
                            mode,
                            content: Some(content),
                            form_data: None,
                            file_path: None,
                        });
                    }
                    if let Some(n) = mutations.max_redirects {
                        state.http_request.options.max_redirects = Some(n);
                    }
                }

                self.apply_script_side_effects(
                    &result,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    input.collection.as_deref(),
                    &mut state.var_ctx,
                );

                if result.error.is_some() {
                    state.script_error = result.error;
                }
            }
        }

        // ── Before-request actions (runtime.actions, set-variable) ─────────────
        let http_request = state.http_request.clone();
        self.apply_actions(
            &input.actions,
            "before-request",
            &request_name,
            &http_request,
            None,
            input.environment_name.as_deref(),
            input.collection.as_deref(),
            input.request_path.as_deref(),
            &mut state.var_ctx,
            &input.tags,
            &input.path_params,
        ).await;
    }
```

Note: the `http_request` clone before `apply_actions` exists only to satisfy the borrow checker (`apply_actions` takes `&HttpRequest` and `&mut VariableContext`, both reachable through `state`). `apply_actions` never mutates the request, so the clone is behaviour-neutral.

- [ ] **Step 5: Extract send, after-response, tests, and finish**

Add these four methods below `run_before_request_phase`. They are old lines 693-700, 703-729, 732-759, and 762-840 respectively:

```rust
    /// Dispatches the (possibly script-mutated) request.
    pub(crate) async fn send_request(&self, state: &PhaseState) -> DomainResult<HttpResponse> {
        let response = self.executor.execute(&state.http_request).await?;

        tracing::info!(
            status = response.status,
            duration_ms = response.duration_ms,
            size_bytes = response.size_bytes,
            "Request completed"
        );

        Ok(response)
    }

    /// Runs the after-response script (if any) and applies its side effects.
    pub(crate) async fn run_after_response_phase(
        &self,
        input: &ExecuteRequestInput,
        response: &HttpResponse,
        state: &mut PhaseState,
    ) {
        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();

        if let Some(code) = &input.post_response_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::after_response(
                    code.clone(),
                    state.var_ctx.clone(),
                    state.http_request.clone(),
                    response.clone(),
                    env_name.clone(),
                    request_name.clone(),
                    input.tags.clone(),
                    input.path_params.clone(),
                );
                let result = self.run_script_phase(
                    code, ctx, &request_name, "after-response", &mut state.console,
                ).await;
                self.apply_script_side_effects(
                    &result,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    input.collection.as_deref(),
                    &mut state.var_ctx,
                );
                if result.error.is_some() && state.script_error.is_none() {
                    state.script_error = result.error;
                }
            }
        }
    }

    /// Runs the tests script (if any) and collects its `rok.test()` results.
    pub(crate) async fn run_tests_phase(
        &self,
        input: &ExecuteRequestInput,
        response: &HttpResponse,
        state: &mut PhaseState,
    ) {
        let request_name = input.request_name.clone().unwrap_or_default();
        let env_name = input.environment_name.clone();

        if let Some(code) = &input.tests_script {
            if !code.trim().is_empty() {
                let ctx = ScriptContext::tests(
                    code.clone(),
                    state.var_ctx.clone(),
                    state.http_request.clone(),
                    response.clone(),
                    env_name.clone(),
                    request_name.clone(),
                    input.tags.clone(),
                    input.path_params.clone(),
                );
                let result = self.run_script_phase(
                    code, ctx, &request_name, "tests", &mut state.console,
                ).await;
                self.apply_script_side_effects(
                    &result,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    input.collection.as_deref(),
                    &mut state.var_ctx,
                );
                state.test_results.extend(result.test_results.clone());
                if result.error.is_some() && state.script_error.is_none() {
                    state.script_error = result.error;
                }
            }
        }
    }

    /// Publishes collected console output, if any. Shared by `finish_phases`
    /// and by the runner, which needs it for a step that was skipped before the
    /// send (and therefore never reaches `finish_phases`).
    pub(crate) fn publish_console(&self, request_name: &str, entries: &[ConsoleEntry]) {
        if entries.is_empty() {
            return;
        }
        let entries = entries.iter().map(|e| {
            let level = match e.level {
                ConsoleLevel::Log => "log",
                ConsoleLevel::Warn => "warn",
                ConsoleLevel::Error => "error",
            };
            serde_json::json!({ "level": level, "message": e.message })
        }).collect();
        self.events.publish(DomainEvent::ConsoleOutput {
            request_name: request_name.to_string(),
            entries,
        });
    }

    /// Runs the after-response actions and declarative assertions, publishes the
    /// console/tests/executed events, saves history, and builds the output.
    pub(crate) async fn finish_phases(
        &self,
        input: &ExecuteRequestInput,
        response: HttpResponse,
        state: &mut PhaseState,
    ) -> ExecuteRequestOutput {
        let request_name = input.request_name.clone().unwrap_or_default();

        // ── After-response actions (runtime.actions, set-variable) ─────────────
        let http_request = state.http_request.clone();
        self.apply_actions(
            &input.actions,
            "after-response",
            &request_name,
            &http_request,
            Some(&response),
            input.environment_name.as_deref(),
            input.collection.as_deref(),
            input.request_path.as_deref(),
            &mut state.var_ctx,
            &input.tags,
            &input.path_params,
        ).await;

        // ── Declarative assertions ────────────────────────────────────────────
        // Run after tests script so JS test results appear first in TestsPanel.
        let assertion_results = crate::assertion_evaluator::evaluate_assertions(
            &input.assertions,
            &response,
        );
        state.test_results.extend(assertion_results);

        // ── Emit events ───────────────────────────────────────────────────────
        self.publish_console(&request_name, &state.console);

        if !state.test_results.is_empty() {
            let results = state.test_results.iter().map(|t| {
                let status = match t.status {
                    TestStatus::Passed => "passed",
                    TestStatus::Failed => "failed",
                };
                serde_json::json!({ "name": t.name, "status": status, "error": t.error })
            }).collect();
            self.events.publish(DomainEvent::TestsCompleted {
                request_name: request_name.clone(),
                results,
            });
        }

        // Persist history (non-fatal — a save failure won't cancel the response).
        let mut entry = HistoryEntry::new(
            input.method.to_string(),
            &state.http_request.url,
            response.status,
            response.duration_ms,
            response.size_bytes,
        );
        if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
            entry = entry.with_collection(col, name);
        }
        let _ = self.history_repo.save(&entry);

        // Publish domain event.
        self.events.publish(DomainEvent::RequestExecuted {
            method: input.method.to_string(),
            url: state.http_request.url.clone(),
            status: response.status,
            duration_ms: response.duration_ms,
        });

        ExecuteRequestOutput {
            response,
            test_results: state.test_results.clone(),
            console_entries: state.console.clone(),
            script_error: state.script_error.clone(),
        }
    }
```

- [ ] **Step 6: Replace the body of `execute()` with the composition**

Replace everything between `pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<ExecuteRequestOutput> {` and its closing brace (old lines 543-840) with:

```rust
        // Every phase runs unconditionally — this is the single-send path. The
        // Collection Runner calls the same methods one at a time so it can act
        // on skip_request / next_request between them.
        let mut state = self.begin_phases(&input)?;
        self.run_before_request_phase(&input, &mut state).await;
        let response = self.send_request(&state).await?;
        self.run_after_response_phase(&input, &response, &mut state).await;
        self.run_tests_phase(&input, &response, &mut state).await;
        Ok(self.finish_phases(&input, response, &mut state).await)
```

Keep the `#[tracing::instrument(name = "http_request", ...)]` attribute on `execute()` exactly as it is.

- [ ] **Step 7: Build**

Run: `cargo check -p rocket-app`
Expected: clean. If `all_console`, `all_test_results` or `script_error` are reported as unused, a leftover line from the old body was not deleted — remove it.

- [ ] **Step 8: Prove behavioural equivalence**

```bash
cargo test -p rocket-app --lib execution_service:: -- --list > /tmp/rocket-runner-refactor/tests_after.txt
diff /tmp/rocket-runner-refactor/tests_before.txt /tmp/rocket-runner-refactor/tests_after.txt && echo "TEST LIST UNCHANGED"
awk '/^#\[cfg\(test\)\]/,0' crates/rocket-app/src/execution_service.rs > /tmp/rocket-runner-refactor/testmod_after.rs
diff /tmp/rocket-runner-refactor/testmod_before.rs /tmp/rocket-runner-refactor/testmod_after.rs && echo "TEST MODULE UNCHANGED"
cargo test -p rocket-app --lib execution_service:: 2>&1 | tail -3
```

Expected: both `diff`s print nothing and echo their banner, and the test run reports `33 passed; 0 failed`. **If any test needed editing to pass, the refactor changed behaviour — revert and redo it.** Also run the whole crate once: `cargo test -p rocket-app` → all green.

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "refactor(app): split execute() into phase-callable pieces"
```

---

### Task 3: Runner hooks on the phase API

Now the phase methods learn about execution mode and start reporting what the runner needs: `skip_request` from the before-request phase and `next_request` from every phase that ran.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`PhaseState`, the four phase methods, `execute()`)
- Test: `crates/rocket-app/src/execution_service.rs` (append to the existing test module — these are *new* tests; the 33 existing ones still must not change)

**Interfaces:**
- Consumes: `rocket_scripting::ExecutionMode` (Task 1).
- Produces:
  - `PhaseState` gains `pub next_request: Option<NextRequest>`, `pub skip_request: bool`, and `pub(crate) fn seed_runtime(&mut self, carried: &std::collections::HashMap<String, String>)`.
  - Phase methods gain a `mode: ExecutionMode` parameter in second position:
    `run_before_request_phase(&self, input: &ExecuteRequestInput, mode: ExecutionMode, state: &mut PhaseState)`,
    `run_after_response_phase(&self, input: &ExecuteRequestInput, mode: ExecutionMode, response: &HttpResponse, state: &mut PhaseState)`,
    `run_tests_phase(&self, input: &ExecuteRequestInput, mode: ExecutionMode, response: &HttpResponse, state: &mut PhaseState)`.
    `begin_phases`, `send_request`, `finish_phases` are unchanged.

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-app/src/execution_service.rs`:

```rust
    /// Script engine that reports the execution mode string it was given and
    /// returns a fixed result for the before-request phase.
    struct ModeProbeEngine {
        seen_modes: Mutex<Vec<String>>,
        before_request_result: ScriptResult,
    }

    #[async_trait]
    impl ScriptEngine for ModeProbeEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            use rocket_scripting::ScriptPhase;
            self.seen_modes.lock().expect("lock").push(ctx.execution_mode.clone());
            if ctx.phase == ScriptPhase::BeforeRequest {
                Ok(self.before_request_result.clone())
            } else {
                Ok(ScriptResult::default())
            }
        }
    }

    struct SharedModeProbe(Arc<ModeProbeEngine>);
    #[async_trait]
    impl ScriptEngine for SharedModeProbe {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            self.0.execute(ctx).await
        }
    }

    #[tokio::test]
    async fn execute_always_reports_standalone_execution_mode() {
        let engine = Arc::new(ModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
            before_request_result: ScriptResult::default(),
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedModeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        input.post_response_script = Some("// post".into());
        input.tests_script = Some("// tests".into());
        svc.execute(input).await.expect("execute");

        let modes = engine.seen_modes.lock().expect("lock").clone();
        assert_eq!(modes, vec!["standalone", "standalone", "standalone"]);
    }

    #[tokio::test]
    async fn execute_ignores_skip_request_and_still_sends() {
        // skipRequest() is a runner-only control. The single-send path must not
        // start honouring it.
        let engine = Arc::new(ModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
            before_request_result: ScriptResult { skip_request: true, ..Default::default() },
        });
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(SharedModeProbe(Arc::clone(&engine))),
        );

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        let out = svc.execute(input).await.expect("execute");
        assert_eq!(out.response.status, 200, "single send must ignore skipRequest()");
    }

    #[tokio::test]
    async fn before_request_phase_records_skip_and_next_request() {
        let engine = ModeProbeEngine {
            seen_modes: Mutex::new(vec![]),
            before_request_result: ScriptResult {
                skip_request: true,
                next_request: Some(rocket_scripting::NextRequest::Name("Poll Status".into())),
                ..Default::default()
            },
        };
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(engine),
        );

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("// pre".into());
        let mut state = svc.begin_phases(&input).expect("begin");
        svc.run_before_request_phase(&input, rocket_scripting::ExecutionMode::Runner, &mut state).await;

        assert!(state.skip_request);
        assert!(matches!(
            state.next_request,
            Some(rocket_scripting::NextRequest::Name(ref n)) if n == "Poll Status"
        ));
    }

    #[tokio::test]
    async fn seed_runtime_puts_carried_vars_in_the_runtime_scope() {
        let svc = build_svc_with_script(
            Box::new(MockEnvRepo::empty()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(ErrorJsonqEngine),
        );
        let input = sample_input("https://example.com", None);
        let mut state = svc.begin_phases(&input).expect("begin");

        let mut carried = std::collections::HashMap::new();
        carried.insert("TOKEN".to_string(), "from-step-1".to_string());
        state.seed_runtime(&carried);

        assert_eq!(state.var_ctx.runtime.get("TOKEN"), Some(&"from-step-1".to_string()));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib execution_service::`
Expected: FAIL — `no field skip_request on type PhaseState`, `this method takes 2 arguments but 3 were supplied`.

- [ ] **Step 3: Extend `PhaseState`**

Add these two fields to `PhaseState` and the `impl` below it:

```rust
    /// Last `next_request` set by any phase that ran — later phase wins, the
    /// same "later overrides earlier" rule `runtime_vars` merging already uses.
    /// Only the Collection Runner reads this.
    pub next_request: Option<NextRequest>,
    /// Set by a before-request script calling `rok.runner.skipRequest()`.
    /// Only the Collection Runner reads this; `execute()` always sends.
    pub skip_request: bool,
```

```rust
impl PhaseState {
    /// Seeds the runtime scope with variables carried over from earlier steps
    /// of the same collection run (spec §8.2). No-op for a single send.
    pub(crate) fn seed_runtime(&mut self, carried: &std::collections::HashMap<String, String>) {
        for (k, v) in carried {
            self.var_ctx.runtime.insert(k.clone(), v.clone());
        }
    }
}
```

Initialise both new fields in `begin_phases`'s `Ok(PhaseState { ... })` with `next_request: None,` and `skip_request: false,`.

Add `NextRequest` and `ExecutionMode` to the `rocket_scripting` import at the top of the file:

```rust
use rocket_scripting::{
    ConsoleEntry, ConsoleLevel, ExecutionMode, NextRequest, ScriptContext, ScriptEngine,
    ScriptResult, TestResult, TestStatus,
};
```

- [ ] **Step 4: Thread the mode and capture the runner fields**

In `run_before_request_phase`, add `mode: ExecutionMode,` as the second parameter, chain `.with_execution_mode(mode)` onto the `ScriptContext::before_request(...)` call, and directly after the `if result.error.is_some()` block (still inside the `if !code.trim().is_empty()` block) add:

```rust
                // Runner controls. `execute()` never reads these; the runner
                // checks them after every phase that ran (spec §4).
                if result.skip_request {
                    state.skip_request = true;
                }
                if result.next_request.is_some() {
                    state.next_request = result.next_request.clone();
                }
```

Do the same in `run_after_response_phase` and `run_tests_phase`: add `mode: ExecutionMode,` as the second parameter, chain `.with_execution_mode(mode)` onto their `ScriptContext::after_response(...)` / `ScriptContext::tests(...)` calls, and add this after each one's `if result.error.is_some() && state.script_error.is_none()` block:

```rust
                if result.next_request.is_some() {
                    state.next_request = result.next_request.clone();
                }
```

`skip_request` is only read from the before-request phase — after the send there is nothing left to skip.

- [ ] **Step 5: Update `execute()` to pass `Standalone`**

```rust
        let mut state = self.begin_phases(&input)?;
        self.run_before_request_phase(&input, ExecutionMode::Standalone, &mut state).await;
        let response = self.send_request(&state).await?;
        self.run_after_response_phase(&input, ExecutionMode::Standalone, &response, &mut state).await;
        self.run_tests_phase(&input, ExecutionMode::Standalone, &response, &mut state).await;
        Ok(self.finish_phases(&input, response, &mut state).await)
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p rocket-app --lib execution_service:: 2>&1 | tail -3`
Expected: `37 passed; 0 failed` — the original 33 plus the 4 new ones, with none of the original 33 edited.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(app): expose execution mode and runner controls on the phase API"
```

---

### Task 4: Run-set sequencing

**Files:**
- Create: `crates/rocket-app/src/runner_sequence.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Test: `crates/rocket-app/src/runner_sequence.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `rocket_app::ExecuteRequestInput`.
- Produces:
  - `pub struct RunItem { pub name: String, pub request_path: String, pub request: rocket_collection::Request }`
  - `pub fn flatten_run_set(collection: &Collection, folder_path: Option<&str>) -> DomainResult<Vec<RunItem>>`
  - `pub fn build_step_input(item: &RunItem, collection: &str, environment_name: Option<&str>, global_env_name: Option<&str>) -> ExecuteRequestInput`

- [ ] **Step 1: Write the failing tests**

Create `crates/rocket-app/src/runner_sequence.rs` containing only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{Collection, Folder, OpaqueProtocolItem, Request};
    use rocket_shared::types::HttpMethod;

    fn req(name: &str, file: &str) -> Request {
        let mut r = Request::new(name, HttpMethod::Get, format!("https://api.test/{name}"));
        r.file_name = Some(file.to_string());
        r
    }

    fn folder(name: &str, dir: &str) -> Folder {
        let mut f = Folder::new(name);
        f.dir_name = Some(dir.to_string());
        f
    }

    /// root: [Login, auth/{Refresh, admin/{Purge}}, Logout]
    fn sample_collection() -> Collection {
        let mut admin = folder("admin", "admin");
        admin.add_request(req("Purge", "purge.yml"));

        let mut auth = folder("auth", "auth");
        auth.add_request(req("Refresh", "refresh.yml"));
        auth.add_subfolder(admin);

        let mut collection = Collection::new("my-api");
        collection.root.dir_name = Some("my-api".into());
        collection.root.add_request(req("Login", "login.yml"));
        collection.root.add_subfolder(auth);
        collection.root.add_request(req("Logout", "logout.yml"));
        collection
    }

    #[test]
    fn flattens_whole_collection_depth_first_in_item_order() {
        let items = flatten_run_set(&sample_collection(), None).expect("flatten");
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Login", "Refresh", "Purge", "Logout"]);
    }

    #[test]
    fn builds_request_paths_relative_to_the_collection_root() {
        let items = flatten_run_set(&sample_collection(), None).expect("flatten");
        let paths: Vec<&str> = items.iter().map(|i| i.request_path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["login.yml", "auth/refresh.yml", "auth/admin/purge.yml", "logout.yml"]
        );
    }

    #[test]
    fn folder_scoped_run_only_contains_that_subtree() {
        let items = flatten_run_set(&sample_collection(), Some("auth")).expect("flatten");
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["Refresh", "Purge"]);
        assert_eq!(items[1].request_path, "auth/admin/purge.yml");
    }

    #[test]
    fn unknown_folder_path_is_not_found() {
        let err = flatten_run_set(&sample_collection(), Some("nope")).expect_err("must fail");
        assert!(matches!(err, rocket_shared::error::DomainError::NotFound(_)));
    }

    #[test]
    fn opaque_protocol_items_are_never_steps() {
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("Login", "login.yml"));
        collection.root.items.push(rocket_collection::CollectionItem::OpaqueItem(
            OpaqueProtocolItem {
                protocol: "graphql".into(),
                name: "Search".into(),
                raw: serde_yaml::Value::Null,
            },
        ));

        let items = flatten_run_set(&collection, None).expect("flatten");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Login");
    }

    #[test]
    fn step_input_carries_scripts_path_and_scope_names() {
        let mut request = req("Login", "login.yml");
        request.pre_request_script = Some("// pre".into());
        request.tests = Some("// tests".into());
        request.tags = vec!["smoke".into()];
        let item = RunItem {
            name: request.name.clone(),
            request_path: "auth/login.yml".into(),
            request,
        };

        let input = build_step_input(&item, "my-api", Some("dev"), Some("shared-global"));

        assert_eq!(input.collection.as_deref(), Some("my-api"));
        assert_eq!(input.request_path.as_deref(), Some("auth/login.yml"));
        assert_eq!(input.request_name.as_deref(), Some("Login"));
        assert_eq!(input.environment_name.as_deref(), Some("dev"));
        assert_eq!(input.global_env_name.as_deref(), Some("shared-global"));
        assert_eq!(input.pre_request_script.as_deref(), Some("// pre"));
        assert_eq!(input.tests_script.as_deref(), Some("// tests"));
        assert_eq!(input.tags, vec!["smoke".to_string()]);
    }

    #[test]
    fn step_input_maps_request_settings_onto_request_options() {
        use rocket_shared::types::{RequestSettingValue, RequestSettings};

        let mut request = req("Login", "login.yml");
        request.settings = Some(RequestSettings {
            encode_url: None,
            timeout: Some(RequestSettingValue::Value(5000.0)),
            follow_redirects: Some(RequestSettingValue::Value(false)),
            max_redirects: Some(RequestSettingValue::Value(3.0)),
            verify_ssl: Some(RequestSettingValue::Inherit("inherit".into())),
        });
        let item = RunItem {
            name: request.name.clone(),
            request_path: "login.yml".into(),
            request,
        };

        let input = build_step_input(&item, "my-api", None, None);
        assert_eq!(input.options.timeout_ms, 5000);
        assert!(!input.options.follow_redirects);
        assert_eq!(input.options.max_redirects, Some(3));
        assert!(input.options.verify_ssl, "\"inherit\" falls back to the default");
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/rocket-app/src/lib.rs`, add `pub mod runner_sequence;` to the module list (alphabetically, after `oauth2_service`) and add to the re-exports:

```rust
pub use runner_sequence::{build_step_input, flatten_run_set, RunItem};
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib runner_sequence::`
Expected: FAIL — `cannot find function flatten_run_set`.

- [ ] **Step 4: Write the implementation**

Prepend to `crates/rocket-app/src/runner_sequence.rs`, above the test module:

```rust
//! Turns a collection tree into the ordered list of executable steps for one
//! Collection Runner run, and turns each step into an `ExecuteRequestInput`.
//!
//! Pure functions — no I/O and no service state. The order is whatever
//! `CollectionRepository::get` returned, which is already `_order.yml` order
//! (see `rocket-infra` `build_folder_tree`), so the runner needs no ordering
//! concept of its own (spec §4).

use rocket_collection::{Collection, CollectionItem, Folder, Request};
use rocket_http::RequestOptions;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{RequestSettingValue, RequestSettings};

use crate::execution_service::ExecuteRequestInput;

/// One executable step in a run set.
#[derive(Debug, Clone)]
pub struct RunItem {
    /// Display name. This is what `rok.runner.setNextRequest(name)` matches on.
    pub name: String,
    /// Path relative to the collection root, e.g. `"auth/login.yml"`.
    pub request_path: String,
    /// The saved request definition.
    pub request: Request,
}

/// Flattens a collection, or one folder inside it, into the ordered list of
/// executable HTTP requests.
///
/// `folder_path` is relative to the collection root and uses on-disk directory
/// names; `None` or `""` runs the whole collection. Sub-folders are traversed
/// depth-first in item order. Folders, opaque protocol items (GraphQL/gRPC/
/// WebSocket) and sidebar summaries are not executable and never become steps.
pub fn flatten_run_set(
    collection: &Collection,
    folder_path: Option<&str>,
) -> DomainResult<Vec<RunItem>> {
    let trimmed = folder_path.unwrap_or("").trim_matches('/');

    let mut folder = &collection.root;
    let mut prefix = String::new();
    if !trimmed.is_empty() {
        for segment in trimmed.split('/') {
            folder = folder
                .items
                .iter()
                .find_map(|item| match item {
                    CollectionItem::Folder(f) if folder_dir_name(f) == segment => Some(f),
                    _ => None,
                })
                .ok_or_else(|| {
                    DomainError::NotFound(format!(
                        "folder '{trimmed}' in collection '{}'",
                        collection.name
                    ))
                })?;
            prefix.push_str(segment);
            prefix.push('/');
        }
    }

    let mut out = Vec::new();
    collect_items(folder, &prefix, &mut out);
    Ok(out)
}

/// Depth-first walk that preserves the on-disk item order.
fn collect_items(folder: &Folder, prefix: &str, out: &mut Vec<RunItem>) {
    for item in &folder.items {
        match item {
            CollectionItem::Request(request) => {
                let Some(file_name) = request.file_name.as_ref() else {
                    tracing::warn!(
                        request = %request.name,
                        "run set: request has no on-disk file name, skipping"
                    );
                    continue;
                };
                out.push(RunItem {
                    name: request.name.clone(),
                    request_path: format!("{prefix}{file_name}"),
                    request: request.clone(),
                });
            }
            CollectionItem::Folder(sub) => {
                let sub_prefix = format!("{prefix}{}/", folder_dir_name(sub));
                collect_items(sub, &sub_prefix, out);
            }
            // Non-HTTP protocols and sidebar summaries are not executable.
            CollectionItem::OpaqueItem(_) | CollectionItem::Summary(_) => {}
        }
    }
}

/// On-disk directory name for a folder, falling back to its display name.
fn folder_dir_name(folder: &Folder) -> &str {
    folder.dir_name.as_deref().unwrap_or(&folder.name)
}

/// Builds the execution input for one run step.
///
/// Mirrors what the Request tab sends for a single send: request-level auth
/// (collection auth is merged later inside `resolve_request`), the saved
/// settings mapped onto `RequestOptions`, and all three script phases.
/// `request.runtime_auth` is deliberately ignored — the single-send path does
/// not consume it either, and the runner must not diverge from it.
pub fn build_step_input(
    item: &RunItem,
    collection: &str,
    environment_name: Option<&str>,
    global_env_name: Option<&str>,
) -> ExecuteRequestInput {
    let request = &item.request;
    ExecuteRequestInput {
        method: request.method.clone(),
        url: request.url.clone(),
        headers: request.headers.clone(),
        query_params: request.query_params.clone(),
        body: request.body.clone(),
        auth: request.auth.clone(),
        options: request_options_from(request.settings.as_ref()),
        environment_name: environment_name.map(str::to_string),
        collection: Some(collection.to_string()),
        request_name: Some(request.name.clone()),
        request_path: Some(item.request_path.clone()),
        tags: request.tags.clone(),
        path_params: request.path_params.clone(),
        pre_request_script: request.pre_request_script.clone(),
        post_response_script: request.post_response_script.clone(),
        tests_script: request.tests.clone(),
        global_env_name: global_env_name.map(str::to_string),
        assertions: request.assertions.clone(),
        actions: request.actions.clone(),
    }
}

/// Maps a saved request's `settings` block onto executor `RequestOptions`.
/// A missing setting and the literal `"inherit"` both fall back to the
/// `RequestOptions` default, which is what a settings-less request sends today.
fn request_options_from(settings: Option<&RequestSettings>) -> RequestOptions {
    let mut options = RequestOptions::default();
    let Some(settings) = settings else {
        return options;
    };
    if let Some(RequestSettingValue::Value(v)) = settings.follow_redirects.as_ref() {
        options.follow_redirects = *v;
    }
    if let Some(RequestSettingValue::Value(v)) = settings.timeout.as_ref() {
        // `settings.timeout` is milliseconds in the OpenCollection format.
        if *v > 0.0 {
            options.timeout_ms = *v as u64;
        }
    }
    if let Some(RequestSettingValue::Value(v)) = settings.verify_ssl.as_ref() {
        options.verify_ssl = *v;
    }
    if let Some(RequestSettingValue::Value(v)) = settings.max_redirects.as_ref() {
        if *v >= 0.0 {
            options.max_redirects = Some(*v as u32);
        }
    }
    options
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib runner_sequence:: 2>&1 | tail -3`
Expected: `7 passed; 0 failed`.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/runner_sequence.rs crates/rocket-app/src/lib.rs
git commit -m "feat(app): add collection runner sequencing helpers"
```

---

### Task 5: Runner domain events

**Files:**
- Modify: `crates/rocket-shared/src/events.rs:30-31` (add below the HTTP execution events)
- Modify: `src-tauri/src/tauri_event_bus.rs:54-57`
- Test: `crates/rocket-shared/src/events.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces three `DomainEvent` variants — exact field names and types below; these are the wire contract the frontend plan is written against.

- [ ] **Step 1: Write the failing tests**

Append to `#[cfg(test)] mod tests` in `crates/rocket-shared/src/events.rs`:

```rust
    #[test]
    fn runner_started_wire_shape() {
        let event = DomainEvent::RunnerStarted {
            run_id: "01J".into(),
            collection: "my-api".into(),
            folder_path: Some("auth".into()),
            total_steps: 3,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"runnerStarted","run_id":"01J","collection":"my-api","folder_path":"auth","total_steps":3}"#
        );
    }

    #[test]
    fn runner_step_completed_wire_shape() {
        let event = DomainEvent::RunnerStepCompleted {
            run_id: "01J".into(),
            index: 0,
            item_name: "Login".into(),
            request_path: "auth/login.yml".into(),
            status: "completed".into(),
            status_code: Some(200),
            duration_ms: 12,
            test_pass_count: 2,
            test_fail_count: 0,
            script_error: None,
            error: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""type":"runnerStepCompleted""#));
        // Struct-variant fields stay snake_case — the enum's rename_all only
        // renames variants. The frontend contract depends on this.
        assert!(json.contains(r#""run_id":"01J""#));
        assert!(json.contains(r#""item_name":"Login""#));
        assert!(json.contains(r#""test_pass_count":2"#));
        assert!(json.contains(r#""status_code":200"#));
    }

    #[test]
    fn runner_finished_wire_shape() {
        let event = DomainEvent::RunnerFinished {
            run_id: "01J".into(),
            stopped_reason: "completed".into(),
            step_count: 3,
            failed_count: 1,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"runnerFinished","run_id":"01J","stopped_reason":"completed","step_count":3,"failed_count":1}"#
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-shared events::`
Expected: FAIL — `no variant named RunnerStarted`.

- [ ] **Step 3: Add the variants**

In `crates/rocket-shared/src/events.rs`, directly below the `RequestExecuted` line:

```rust
    // Collection Runner events
    /// Emitted once when a run starts, before its first step.
    /// `total_steps` is the run set's length; a script jumping with
    /// `setNextRequest` can make the number of executed steps differ from it.
    RunnerStarted {
        run_id: String,
        collection: String,
        folder_path: Option<String>,
        total_steps: usize,
    },
    /// Emitted after every step of a run, in execution order.
    /// `status` is `"completed"`, `"skipped"`, or `"error"`.
    RunnerStepCompleted {
        run_id: String,
        /// Position in the emitted step stream, starting at 0.
        index: usize,
        item_name: String,
        request_path: String,
        status: String,
        /// `None` for a skipped or errored step.
        status_code: Option<u16>,
        duration_ms: u64,
        test_pass_count: usize,
        test_fail_count: usize,
        /// Uncaught script exception message, if any.
        script_error: Option<String>,
        /// Transport or sequencing error, if any.
        error: Option<String>,
    },
    /// Emitted once when a run ends, for any reason.
    /// `stopped_reason` is `"completed"`, `"stoppedByScript"`,
    /// `"stoppedOnFailure"`, `"unknownNextRequest"`, `"cancelled"`, or
    /// `"stepLimitReached"`.
    RunnerFinished {
        run_id: String,
        stopped_reason: String,
        step_count: usize,
        failed_count: usize,
    },
```

- [ ] **Step 4: Wire them to the Tauri event bus**

In `src-tauri/src/tauri_event_bus.rs`, add these arms to the `match &event` before the script events:

```rust
            // Collection Runner events — each gets its own channel so the run
            // view can append steps without re-reading the whole run.
            DomainEvent::RunnerStarted { .. } => "runner-started",
            DomainEvent::RunnerStepCompleted { .. } => "runner-step-completed",
            DomainEvent::RunnerFinished { .. } => "runner-finished",
```

The `match` is exhaustive, so omitting this is a compile error rather than a silent drop.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rocket-shared events:: 2>&1 | tail -3` → all green.
Run: `cargo check -p rocket` (`rocket` is the `src-tauri` crate's package name) → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-shared/src/events.rs src-tauri/src/tauri_event_bus.rs
git commit -m "feat(shared): add collection runner domain events"
```

---

### Task 6: Test doubles for runner tests

`execution_service.rs` already owns a set of doubles, but they are private to its test module and Task 2 depends on that module staying byte-identical. The runner tests get their own module.

**Files:**
- Create: `crates/rocket-app/src/test_doubles.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Test: `crates/rocket-app/src/test_doubles.rs` (`#[cfg(test)] mod tests` — one smoke test)

**Interfaces:**
- Produces (all `#[cfg(test)] pub(crate)`):
  - `InMemoryCollectionRepo::new(collection: Collection) -> Self` implementing `CollectionRepository`
  - `NullEnvRepo`, `InMemoryHistoryRepo::new()` (with `pub entries: Mutex<Vec<HistoryEntry>>`), `NullCookieRepo`
  - `RecordingExecutor::new() -> Arc<Self>`, `.set_status(&self, url_substring: &str, status: u16)`, `.sent_urls() -> Vec<String>`
  - `ProgrammableEngine::new() -> Arc<Self>`, `.on(&self, request_name: &str, phase: &str, result: ScriptResult)`, `.modes() -> Vec<String>`, `.calls() -> Vec<String>`, `.runtime_reads() -> Vec<HashMap<String, String>>`
  - `RecordingPublisher::new() -> Arc<Self>`, `.events() -> Vec<DomainEvent>`
  - `shared_*` newtype wrappers so one `Arc` double can be handed to a service as a `Box<dyn Trait>`: `SharedCollectionRepo`, `SharedHistoryRepo`, `SharedExecutor`, `SharedEngine`, `SharedPublisher`

- [ ] **Step 1: Create the module**

Create `crates/rocket-app/src/test_doubles.rs`:

```rust
//! In-memory test doubles shared by the Collection Runner tests.
//!
//! `execution_service.rs` predates this module and keeps its own doubles — do
//! not migrate those, their byte-for-byte stability is what proves the
//! phase-split refactor changed no behaviour.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    Request as CollectionRequest,
};
use rocket_environment::{Environment, EnvironmentRepository};
use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

// ---------------------------------------------------------------------------
// Collection repo
// ---------------------------------------------------------------------------

/// Collection repo backed by one in-memory `Collection`.
pub struct InMemoryCollectionRepo {
    collection: Collection,
}

impl InMemoryCollectionRepo {
    pub fn new(collection: Collection) -> Arc<Self> {
        Arc::new(Self { collection })
    }
}

impl CollectionRepository for InMemoryCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> { Ok(vec![]) }
    fn get(&self, name: &str) -> DomainResult<Collection> {
        if name == self.collection.name {
            Ok(self.collection.clone())
        } else {
            Err(DomainError::NotFound(name.into()))
        }
    }
    fn get_summaries(&self, name: &str) -> DomainResult<Collection> { self.get(name) }
    fn create(&self, _: &str) -> DomainResult<Collection> {
        Err(DomainError::NotFound("stub".into()))
    }
    fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
    fn rename(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
    fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
        Err(DomainError::NotFound("stub".into()))
    }
    fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> {
        Ok(path.to_string())
    }
    fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
    fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
    fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
    fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
    fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
    fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> { Ok(()) }
    fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
        Ok(self.collection.settings.clone())
    }
    fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> { Ok(()) }
    fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
        Ok(vec![])
    }
    fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
        Ok(vec![])
    }
    fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
        Ok(())
    }
    fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
        Ok(vec![])
    }
    fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
        Ok(())
    }
}

/// Hands one `Arc<InMemoryCollectionRepo>` to a service expecting a `Box<dyn>`.
pub struct SharedCollectionRepo(pub Arc<InMemoryCollectionRepo>);

impl CollectionRepository for SharedCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> { self.0.list() }
    fn get(&self, n: &str) -> DomainResult<Collection> { self.0.get(n) }
    fn get_summaries(&self, n: &str) -> DomainResult<Collection> { self.0.get_summaries(n) }
    fn create(&self, n: &str) -> DomainResult<Collection> { self.0.create(n) }
    fn delete(&self, n: &str) -> DomainResult<()> { self.0.delete(n) }
    fn rename(&self, a: &str, b: &str) -> DomainResult<()> { self.0.rename(a, b) }
    fn get_request(&self, a: &str, b: &str) -> DomainResult<CollectionRequest> { self.0.get_request(a, b) }
    fn save_request(&self, a: &str, b: &str, c: &CollectionRequest) -> DomainResult<String> { self.0.save_request(a, b, c) }
    fn rename_request(&self, a: &str, b: &str, c: &str) -> DomainResult<()> { self.0.rename_request(a, b, c) }
    fn delete_request(&self, a: &str, b: &str) -> DomainResult<()> { self.0.delete_request(a, b) }
    fn create_folder(&self, a: &str, b: &str) -> DomainResult<()> { self.0.create_folder(a, b) }
    fn delete_folder(&self, a: &str, b: &str) -> DomainResult<()> { self.0.delete_folder(a, b) }
    fn move_item(&self, a: &str, b: &str, c: &str, d: &str) -> DomainResult<()> { self.0.move_item(a, b, c, d) }
    fn reorder_items(&self, a: &str, b: &str, c: &[String]) -> DomainResult<()> { self.0.reorder_items(a, b, c) }
    fn get_settings(&self, n: &str) -> DomainResult<CollectionSettings> { self.0.get_settings(n) }
    fn save_settings(&self, n: &str, s: &CollectionSettings) -> DomainResult<()> { self.0.save_settings(n, s) }
    fn get_folder_chain_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> { self.0.get_folder_chain_variables(a, b) }
    fn get_folder_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> { self.0.get_folder_variables(a, b) }
    fn save_folder_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> { self.0.save_folder_variables(a, b, c) }
    fn get_request_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> { self.0.get_request_variables(a, b) }
    fn save_request_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> { self.0.save_request_variables(a, b, c) }
}

// ---------------------------------------------------------------------------
// Environment, history, cookies
// ---------------------------------------------------------------------------

/// Environment repo with no environments.
pub struct NullEnvRepo;

impl EnvironmentRepository for NullEnvRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> { Ok(vec![]) }
    fn get(&self, name: &str) -> DomainResult<Environment> {
        Err(DomainError::NotFound(name.into()))
    }
    fn save(&self, _: &Environment) -> DomainResult<()> { Ok(()) }
    fn delete(&self, _: &str) -> DomainResult<()> { Ok(()) }
}

/// History repo that records everything saved to it.
pub struct InMemoryHistoryRepo {
    pub entries: Mutex<Vec<HistoryEntry>>,
}

impl InMemoryHistoryRepo {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { entries: Mutex::new(Vec::new()) })
    }
    pub fn saved_count(&self) -> usize {
        self.entries.lock().expect("lock").len()
    }
}

impl HistoryRepository for InMemoryHistoryRepo {
    fn list(&self, _: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        Ok(self.entries.lock().expect("lock").clone())
    }
    fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        self.entries.lock().expect("lock").iter().find(|e| e.id == id).cloned()
            .ok_or_else(|| DomainError::NotFound(id.into()))
    }
    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
        self.entries.lock().expect("lock").push(entry.clone());
        Ok(())
    }
    fn clear(&self) -> DomainResult<()> {
        self.entries.lock().expect("lock").clear();
        Ok(())
    }
    fn search(&self, _: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
        Ok(self.entries.lock().expect("lock").clone())
    }
}

/// Hands one `Arc<InMemoryHistoryRepo>` to a service expecting a `Box<dyn>`.
pub struct SharedHistoryRepo(pub Arc<InMemoryHistoryRepo>);

impl HistoryRepository for SharedHistoryRepo {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> { self.0.list(limit) }
    fn get(&self, id: &str) -> DomainResult<HistoryEntry> { self.0.get(id) }
    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> { self.0.save(entry) }
    fn clear(&self) -> DomainResult<()> { self.0.clear() }
    fn search(&self, f: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> { self.0.search(f) }
}

/// Cookie repo that stores nothing.
pub struct NullCookieRepo;

impl CookieRepository for NullCookieRepo {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>> { Ok(vec![]) }
    fn get_by_domain(&self, _: &str) -> DomainResult<Option<CookieJar>> { Ok(None) }
    fn save(&self, _: &CookieJar) -> DomainResult<()> { Ok(()) }
    fn clear(&self) -> DomainResult<()> { Ok(()) }
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

/// Executor that records every URL it was asked to send and answers with a
/// per-URL status code (default 200). A URL whose status is registered as 0
/// fails with a transport error instead.
pub struct RecordingExecutor {
    sent: Mutex<Vec<String>>,
    statuses: Mutex<HashMap<String, u16>>,
}

impl RecordingExecutor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { sent: Mutex::new(Vec::new()), statuses: Mutex::new(HashMap::new()) })
    }
    /// Registers a status for any URL containing `url_substring`. A status of
    /// `0` makes the send fail with a transport error instead.
    /// Takes `&self` so it can be called straight through the `Arc`.
    pub fn set_status(&self, url_substring: &str, status: u16) {
        self.statuses.lock().expect("lock").insert(url_substring.to_string(), status);
    }
    pub fn sent_urls(&self) -> Vec<String> {
        self.sent.lock().expect("lock").clone()
    }
}

#[async_trait]
impl HttpExecutor for RecordingExecutor {
    async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
        self.sent.lock().expect("lock").push(req.url.clone());
        let status = self
            .statuses
            .lock()
            .expect("lock")
            .iter()
            .find(|(fragment, _)| req.url.contains(fragment.as_str()))
            .map(|(_, status)| *status)
            .unwrap_or(200);
        if status == 0 {
            return Err(DomainError::Http("connection refused".into()));
        }
        Ok(HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![],
            body: "{}".into(),
            duration_ms: 1,
            ttfb_ms: 1,
            size_bytes: 2,
        })
    }
}

/// Hands one `Arc<RecordingExecutor>` to a service expecting `Arc<dyn>`.
pub struct SharedExecutor(pub Arc<RecordingExecutor>);

#[async_trait]
impl HttpExecutor for SharedExecutor {
    async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
        self.0.execute(req).await
    }
}

// ---------------------------------------------------------------------------
// Script engine
// ---------------------------------------------------------------------------

/// Script engine that returns a canned `ScriptResult` per (request name, phase)
/// and records the execution mode every call carried.
pub struct ProgrammableEngine {
    results: Mutex<HashMap<String, ScriptResult>>,
    modes: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
    runtime_reads: Mutex<Vec<HashMap<String, String>>>,
}

impl ProgrammableEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            results: Mutex::new(HashMap::new()),
            modes: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            runtime_reads: Mutex::new(Vec::new()),
        })
    }
    /// Registers a canned result. `phase` is `"before-request"`,
    /// `"after-response"`, or `"tests"`. Takes `&self` so it can be called
    /// straight through the `Arc`.
    pub fn on(&self, request_name: &str, phase: &str, result: ScriptResult) {
        self.results.lock().expect("lock").insert(format!("{request_name}|{phase}"), result);
    }
    pub fn modes(&self) -> Vec<String> {
        self.modes.lock().expect("lock").clone()
    }
    /// Keys of the form `"<request name>|<phase>"`, in call order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock").clone()
    }
    /// The runtime variable scope each call saw, in call order.
    pub fn runtime_reads(&self) -> Vec<HashMap<String, String>> {
        self.runtime_reads.lock().expect("lock").clone()
    }
}

#[async_trait]
impl ScriptEngine for ProgrammableEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        let key = format!("{}|{}", ctx.request_name, ctx.phase.as_str());
        self.modes.lock().expect("lock").push(ctx.execution_mode.clone());
        self.calls.lock().expect("lock").push(key.clone());
        self.runtime_reads.lock().expect("lock").push(ctx.variables.runtime.clone());
        Ok(self.results.lock().expect("lock").get(&key).cloned().unwrap_or_default())
    }
}

/// Hands one `Arc<ProgrammableEngine>` to a service expecting a `Box<dyn>`.
pub struct SharedEngine(pub Arc<ProgrammableEngine>);

#[async_trait]
impl ScriptEngine for SharedEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        self.0.execute(ctx).await
    }
}

// ---------------------------------------------------------------------------
// Event publisher
// ---------------------------------------------------------------------------

/// Publisher that records every event it is handed.
pub struct RecordingPublisher {
    events: Mutex<Vec<DomainEvent>>,
}

impl RecordingPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { events: Mutex::new(Vec::new()) })
    }
    pub fn events(&self) -> Vec<DomainEvent> {
        self.events.lock().expect("lock").clone()
    }
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: DomainEvent) {
        self.events.lock().expect("lock").push(event);
    }
}

/// Hands one `Arc<RecordingPublisher>` to a service expecting a `Box<dyn>`.
pub struct SharedPublisher(pub Arc<RecordingPublisher>);

impl EventPublisher for SharedPublisher {
    fn publish(&self, event: DomainEvent) {
        self.0.publish(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::HttpMethod;

    #[tokio::test]
    async fn recording_executor_reports_registered_status_and_records_urls() {
        let executor = RecordingExecutor::new();
        executor.set_status("/boom", 500);

        let ok = executor
            .execute(&HttpRequest::new(HttpMethod::Get, "https://api.test/ok"))
            .await
            .expect("send");
        let boom = executor
            .execute(&HttpRequest::new(HttpMethod::Get, "https://api.test/boom"))
            .await
            .expect("send");

        assert_eq!(ok.status, 200);
        assert_eq!(boom.status, 500);
        assert_eq!(executor.sent_urls().len(), 2);
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/rocket-app/src/lib.rs`, after the other module declarations:

```rust
#[cfg(test)]
pub(crate) mod test_doubles;
```

- [ ] **Step 3: Run the smoke test**

Run: `cargo test -p rocket-app --lib test_doubles:: 2>&1 | tail -3`
Expected: `1 passed; 0 failed`. Fix any unused-import warnings `cargo check -p rocket-app --tests` reports.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/test_doubles.rs crates/rocket-app/src/lib.rs
git commit -m "test(app): add shared in-memory doubles for runner tests"
```

---

### Task 7: `CollectionRunnerService` — sequential run

**Files:**
- Create: `crates/rocket-app/src/collection_runner_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Test: `crates/rocket-app/src/collection_runner_service.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `flatten_run_set`, `build_step_input`, `RunItem` (Task 4); `PhaseState` + phase methods + `ExecutionMode` (Tasks 2-3); the `DomainEvent` variants (Task 5); `test_doubles` (Task 6).
- Produces:
  - `pub struct CollectionRunnerService` with `pub fn new(collection_repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self`
  - `pub async fn run(&self, exec: &RequestExecutionService, input: RunCollectionInput) -> DomainResult<RunSummary>`
  - `pub struct RunCollectionInput { collection: String, folder_path: Option<String>, environment_name: Option<String>, global_env_name: Option<String>, stop_on_failure: bool }` (camelCase IPC DTO)
  - `pub struct RunSummary { run_id: String, collection: String, folder_path: Option<String>, steps: Vec<RunStepResult>, stopped_reason: StoppedReason }`
  - `pub struct RunStepResult { index, item_name, request_path, status, status_code, duration_ms, test_pass_count, test_fail_count, script_error, error }` with `pub fn is_failure(&self) -> bool`
  - `pub enum RunStepStatus { Completed, Skipped, Error }` with `pub fn as_str(self) -> &'static str`
  - `pub enum StoppedReason { Completed, StoppedByScript, StoppedOnFailure { item_name }, UnknownNextRequest { item_name, next_request }, Cancelled, StepLimitReached { limit } }` with `pub fn as_str(&self) -> &'static str`

- [ ] **Step 1: Write the failing tests**

Create `crates/rocket-app/src/collection_runner_service.rs` with only this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{
        InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo,
        ProgrammableEngine, RecordingExecutor, RecordingPublisher, SharedCollectionRepo,
        SharedEngine, SharedExecutor, SharedHistoryRepo, SharedPublisher,
    };
    use rocket_collection::{Collection, Request};
    use rocket_scripting::ScriptResult;
    use rocket_shared::types::HttpMethod;
    use std::sync::Arc;

    fn req(name: &str, file: &str) -> Request {
        let mut r = Request::new(name, HttpMethod::Get, format!("https://api.test/{file}"));
        r.file_name = Some(file.to_string());
        // Every phase has a script so the engine is always consulted.
        r.pre_request_script = Some("// pre".into());
        r.post_response_script = Some("// post".into());
        r.tests = Some("// tests".into());
        r
    }

    /// root: [First, Second, Third]
    fn three_step_collection() -> Collection {
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("First", "first.yml"));
        collection.root.add_request(req("Second", "second.yml"));
        collection.root.add_request(req("Third", "third.yml"));
        collection
    }

    fn sample_run_input() -> RunCollectionInput {
        RunCollectionInput {
            collection: "my-api".into(),
            folder_path: None,
            environment_name: None,
            global_env_name: None,
            stop_on_failure: false,
        }
    }

    struct Harness {
        runner: CollectionRunnerService,
        exec: RequestExecutionService,
        executor: Arc<RecordingExecutor>,
        engine: Arc<ProgrammableEngine>,
        history: Arc<InMemoryHistoryRepo>,
        publisher: Arc<RecordingPublisher>,
    }

    fn harness(collection: Collection, engine: Arc<ProgrammableEngine>, executor: Arc<RecordingExecutor>) -> Harness {
        let repo = InMemoryCollectionRepo::new(collection);
        let history = InMemoryHistoryRepo::new();
        let publisher = RecordingPublisher::new();

        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));

        let runner = CollectionRunnerService::new(
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
        );

        Harness { runner, exec, executor, engine, history, publisher }
    }

    #[tokio::test]
    async fn runs_every_request_in_order() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(
            h.executor.sent_urls(),
            vec![
                "https://api.test/first.yml".to_string(),
                "https://api.test/second.yml".to_string(),
                "https://api.test/third.yml".to_string(),
            ]
        );
        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Second", "Third"]);
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
        assert!(summary.steps.iter().all(|s| s.status == RunStepStatus::Completed));
    }

    #[tokio::test]
    async fn every_script_phase_reports_runner_execution_mode() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let modes = h.engine.modes();
        assert_eq!(modes.len(), 9, "3 requests x 3 phases");
        assert!(modes.iter().all(|m| m == "runner"), "got {modes:?}");
    }

    #[tokio::test]
    async fn each_step_still_lands_in_history() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        h.runner.run(&h.exec, sample_run_input()).await.expect("run");
        assert_eq!(h.history.saved_count(), 3);
    }

    #[tokio::test]
    async fn runtime_variables_carry_forward_to_later_steps() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "after-response",
            ScriptResult {
                runtime_vars: std::collections::HashMap::from([(
                    "TOKEN".to_string(),
                    serde_json::json!("abc123"),
                )]),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        // The before-request phase of step 2 must already see step 1's write.
        let calls = h.engine.calls();
        let reads = h.engine.runtime_reads();
        let idx = calls
            .iter()
            .position(|c| c == "Second|before-request")
            .expect("second step ran");
        assert_eq!(reads[idx].get("TOKEN"), Some(&"abc123".to_string()));
    }

    #[tokio::test]
    async fn publishes_started_step_and_finished_events() {
        use rocket_shared::events::DomainEvent;

        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");
        let events = h.publisher.events();

        assert!(matches!(
            events.first(),
            Some(DomainEvent::RunnerStarted { total_steps: 3, .. })
        ));
        let step_events: Vec<&DomainEvent> = events
            .iter()
            .filter(|e| matches!(e, DomainEvent::RunnerStepCompleted { .. }))
            .collect();
        assert_eq!(step_events.len(), 3);
        assert!(matches!(
            events.last(),
            Some(DomainEvent::RunnerFinished { step_count: 3, failed_count: 0, .. })
        ));
        assert!(events.iter().all(|e| match e {
            DomainEvent::RunnerStarted { run_id, .. }
            | DomainEvent::RunnerStepCompleted { run_id, .. }
            | DomainEvent::RunnerFinished { run_id, .. } => run_id == &summary.run_id,
            _ => true,
        }));
    }

    #[tokio::test]
    async fn unknown_collection_is_not_found() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        let mut input = sample_run_input();
        input.collection = "missing".into();
        let err = h.runner.run(&h.exec, input).await.expect_err("must fail");
        assert!(matches!(err, rocket_shared::error::DomainError::NotFound(_)));
    }

    // These two lock the IPC DTO wire shapes the frontend plan is written
    // against. Unlike `DomainEvent`, these are camelCase all the way down.
    #[test]
    fn stopped_reason_wire_shape_is_camel_case() {
        let reason = StoppedReason::UnknownNextRequest {
            item_name: "First".into(),
            next_request: "Nowhere".into(),
        };
        let json = serde_json::to_string(&reason).expect("serialize");
        assert_eq!(
            json,
            r#"{"kind":"unknownNextRequest","itemName":"First","nextRequest":"Nowhere"}"#
        );
    }

    #[test]
    fn run_step_status_wire_shape_is_camel_case() {
        assert_eq!(
            serde_json::to_string(&RunStepStatus::Completed).expect("serialize"),
            r#""completed""#
        );
        assert_eq!(
            serde_json::to_string(&RunStepStatus::Skipped).expect("serialize"),
            r#""skipped""#
        );
        assert_eq!(
            serde_json::to_string(&RunStepStatus::Error).expect("serialize"),
            r#""error""#
        );
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/rocket-app/src/lib.rs`, add `pub mod collection_runner_service;` to the module list and to the re-exports:

```rust
pub use collection_runner_service::{
    CollectionRunnerService, RunCollectionInput, RunStepResult, RunStepStatus, RunSummary,
    StoppedReason,
};
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib collection_runner_service::`
Expected: FAIL — `cannot find struct CollectionRunnerService`.

- [ ] **Step 4: Write the types**

Prepend to `crates/rocket-app/src/collection_runner_service.rs`:

```rust
//! Collection Runner — runs a folder's or collection's requests in sequence and
//! honours the `rok.runner.*` scripting API.
//!
//! The runner is an orchestration layer, not a second execution engine: it
//! drives the same phase methods on `RequestExecutionService` that a single
//! send does, one phase at a time, so it can act on `skip_request` before the
//! send and on `next_request` after every phase that ran (spec §4).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rocket_collection::CollectionRepository;
use rocket_scripting::{ExecutionMode, NextRequest};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::runner_sequence::{build_step_input, flatten_run_set, RunItem};

/// Hard cap on executed steps in one run. `setNextRequest` can form a cycle
/// (A → B → A); without a cap the run would never end.
const MAX_RUN_STEPS: usize = 1_000;

/// Input for one run. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCollectionInput {
    pub collection: String,
    /// Folder to run, relative to the collection root, using on-disk directory
    /// names. `None` or `""` runs the whole collection.
    #[serde(default)]
    pub folder_path: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub global_env_name: Option<String>,
    /// Stop at the first failed step. Defaults to `false` — Bruno's `--bail` is
    /// opt-in too (spec §8.3).
    #[serde(default)]
    pub stop_on_failure: bool,
}

/// Outcome of a single step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStepStatus {
    /// The request was sent and every phase that applies ran.
    Completed,
    /// A before-request script called `rok.runner.skipRequest()`.
    Skipped,
    /// The request could not be dispatched, or the run could not continue past it.
    Error,
}

impl RunStepStatus {
    /// Wire string used in `DomainEvent::RunnerStepCompleted.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStepStatus::Completed => "completed",
            RunStepStatus::Skipped => "skipped",
            RunStepStatus::Error => "error",
        }
    }
}

/// One row of the run summary. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepResult {
    /// Position in the executed-step stream, starting at 0.
    pub index: usize,
    pub item_name: String,
    pub request_path: String,
    pub status: RunStepStatus,
    /// `None` for a skipped or errored step.
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    pub test_pass_count: usize,
    pub test_fail_count: usize,
    /// Uncaught script exception, if any.
    pub script_error: Option<String>,
    /// Transport or sequencing error, if any.
    pub error: Option<String>,
}

impl RunStepResult {
    /// A step counts as failed when it errored, returned a non-2xx status, or
    /// had at least one failing test (spec §7). A skipped step never fails.
    pub fn is_failure(&self) -> bool {
        match self.status {
            RunStepStatus::Error => true,
            RunStepStatus::Skipped => false,
            RunStepStatus::Completed => {
                self.test_fail_count > 0
                    || self.status_code.map(|s| !(200..300).contains(&s)).unwrap_or(true)
            }
        }
    }
}

/// Why a run ended. IPC DTO.
///
/// The container `rename_all` only renames variants — each struct variant
/// carries its own `rename_all` so its fields are camelCase on the wire too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StoppedReason {
    /// Every item in the run set ran.
    Completed,
    /// A script called `rok.runner.setNextRequest(null)`.
    StoppedByScript,
    /// `stop_on_failure` was set and this step failed.
    #[serde(rename_all = "camelCase")]
    StoppedOnFailure { item_name: String },
    /// `rok.runner.setNextRequest(name)` named an item that is not in the run set.
    #[serde(rename_all = "camelCase")]
    UnknownNextRequest { item_name: String, next_request: String },
    /// `stop_collection_run` was called for this run.
    Cancelled,
    /// `MAX_RUN_STEPS` executed steps were reached — almost certainly a
    /// `setNextRequest` cycle.
    #[serde(rename_all = "camelCase")]
    StepLimitReached { limit: usize },
}

impl StoppedReason {
    /// Wire string used in `DomainEvent::RunnerFinished.stopped_reason`.
    pub fn as_str(&self) -> &'static str {
        match self {
            StoppedReason::Completed => "completed",
            StoppedReason::StoppedByScript => "stoppedByScript",
            StoppedReason::StoppedOnFailure { .. } => "stoppedOnFailure",
            StoppedReason::UnknownNextRequest { .. } => "unknownNextRequest",
            StoppedReason::Cancelled => "cancelled",
            StoppedReason::StepLimitReached { .. } => "stepLimitReached",
        }
    }
}

/// Full result of one run. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub run_id: String,
    pub collection: String,
    pub folder_path: Option<String>,
    pub steps: Vec<RunStepResult>,
    pub stopped_reason: StoppedReason,
}

/// What one step reported back to the run loop.
struct StepOutcome {
    result: RunStepResult,
    /// Last `next_request` any phase of this step set.
    next_request: Option<NextRequest>,
}
```

- [ ] **Step 5: Write the service and the run loop**

Append below the types, still above the test module:

```rust
/// Runs a folder's or collection's requests in sequence.
///
/// Holds no execution machinery of its own — `run()` takes the
/// `RequestExecutionService` to drive, the same way `LoadTestService::run`
/// does, so both are constructed independently in the DI layer.
pub struct CollectionRunnerService {
    collection_repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
    /// Run ids that have been asked to stop. Shared behind an `Arc` so a test
    /// (and, later, any other holder) can flip a run to cancelled mid-flight.
    cancelled: Arc<Mutex<HashSet<String>>>,
}

impl CollectionRunnerService {
    pub fn new(
        collection_repo: Box<dyn CollectionRepository>,
        events: Box<dyn EventPublisher>,
    ) -> Self {
        Self {
            collection_repo,
            events,
            cancelled: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Runs every request in the target folder or collection, in order.
    ///
    /// Returns once the run ends. Progress is also streamed as
    /// `RunnerStarted` / `RunnerStepCompleted` / `RunnerFinished` events.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunCollectionInput,
    ) -> DomainResult<RunSummary> {
        let collection = self.collection_repo.get(&input.collection)?;
        let items = flatten_run_set(&collection, input.folder_path.as_deref())?;
        let run_id = Ulid::new().to_string();

        self.events.publish(DomainEvent::RunnerStarted {
            run_id: run_id.clone(),
            collection: input.collection.clone(),
            folder_path: input.folder_path.clone(),
            total_steps: items.len(),
        });

        let mut steps: Vec<RunStepResult> = Vec::new();
        let mut carried_runtime: HashMap<String, String> = HashMap::new();
        let mut cursor = 0usize;
        let mut stopped_reason = StoppedReason::Completed;

        while cursor < items.len() {
            if self.is_cancelled(&run_id) {
                stopped_reason = StoppedReason::Cancelled;
                break;
            }
            if steps.len() >= MAX_RUN_STEPS {
                stopped_reason = StoppedReason::StepLimitReached { limit: MAX_RUN_STEPS };
                break;
            }

            let item = &items[cursor];
            let outcome = self
                .run_step(exec, &input, item, steps.len(), &mut carried_runtime)
                .await;
            let mut result = outcome.result;

            // Resolve the jump before publishing so an unknown target is part
            // of the step the frontend sees (spec §4).
            let mut jump_to: Option<usize> = None;
            let mut stop_after = false;
            match outcome.next_request {
                Some(NextRequest::Stop) => {
                    stopped_reason = StoppedReason::StoppedByScript;
                    stop_after = true;
                }
                Some(NextRequest::Name(ref name)) => {
                    match items.iter().position(|i| &i.name == name) {
                        Some(idx) => jump_to = Some(idx),
                        None => {
                            result.status = RunStepStatus::Error;
                            result.error = Some(format!(
                                "rok.runner.setNextRequest('{name}') — no request named '{name}' in this run"
                            ));
                            stopped_reason = StoppedReason::UnknownNextRequest {
                                item_name: result.item_name.clone(),
                                next_request: name.clone(),
                            };
                            stop_after = true;
                        }
                    }
                }
                None => {}
            }

            let failed = result.is_failure();
            self.publish_step(&run_id, &result);
            let item_name = result.item_name.clone();
            steps.push(result);

            if stop_after {
                break;
            }
            if failed && input.stop_on_failure {
                stopped_reason = StoppedReason::StoppedOnFailure { item_name };
                break;
            }
            cursor = match jump_to {
                Some(idx) => idx,
                None => cursor + 1,
            };
        }

        self.clear_cancellation(&run_id);

        let failed_count = steps.iter().filter(|s| s.is_failure()).count();
        self.events.publish(DomainEvent::RunnerFinished {
            run_id: run_id.clone(),
            stopped_reason: stopped_reason.as_str().to_string(),
            step_count: steps.len(),
            failed_count,
        });

        Ok(RunSummary {
            run_id,
            collection: input.collection,
            folder_path: input.folder_path,
            steps,
            stopped_reason,
        })
    }

    /// Runs one step: before-request, then — unless the script skipped it —
    /// send, after-response, tests, and the shared finish work (events,
    /// history, assertions).
    async fn run_step(
        &self,
        exec: &RequestExecutionService,
        input: &RunCollectionInput,
        item: &RunItem,
        index: usize,
        carried_runtime: &mut HashMap<String, String>,
    ) -> StepOutcome {
        let step_input: ExecuteRequestInput = build_step_input(
            item,
            &input.collection,
            input.environment_name.as_deref(),
            input.global_env_name.as_deref(),
        );

        let mut state = match exec.begin_phases(&step_input) {
            Ok(state) => state,
            Err(e) => {
                return StepOutcome {
                    result: error_step(index, item, e.to_string()),
                    next_request: None,
                }
            }
        };
        // Runtime variables set by earlier steps stay readable (spec §8.2).
        state.seed_runtime(carried_runtime);

        exec.run_before_request_phase(&step_input, ExecutionMode::Runner, &mut state)
            .await;

        if state.skip_request {
            // Nothing was sent, so after-response and tests have no response to
            // run against — the step ends here (spec §4). `finish_phases` never
            // runs either, so publish this step's console output directly.
            exec.publish_console(&item.name, &state.console);
            *carried_runtime = state.var_ctx.runtime.clone();
            return StepOutcome {
                next_request: state.next_request.clone(),
                result: RunStepResult {
                    index,
                    item_name: item.name.clone(),
                    request_path: item.request_path.clone(),
                    status: RunStepStatus::Skipped,
                    status_code: None,
                    duration_ms: 0,
                    test_pass_count: 0,
                    test_fail_count: 0,
                    script_error: state.script_error.clone(),
                    error: None,
                },
            };
        }

        let response = match exec.send_request(&state).await {
            Ok(response) => response,
            Err(e) => {
                *carried_runtime = state.var_ctx.runtime.clone();
                let mut result = error_step(index, item, e.to_string());
                result.script_error = state.script_error.clone();
                return StepOutcome {
                    next_request: state.next_request.clone(),
                    result,
                };
            }
        };

        exec.run_after_response_phase(&step_input, ExecutionMode::Runner, &response, &mut state)
            .await;
        exec.run_tests_phase(&step_input, ExecutionMode::Runner, &response, &mut state)
            .await;
        let output = exec.finish_phases(&step_input, response, &mut state).await;

        *carried_runtime = state.var_ctx.runtime.clone();

        let passed = output
            .test_results
            .iter()
            .filter(|t| t.status == rocket_scripting::TestStatus::Passed)
            .count();
        let failed = output.test_results.len() - passed;

        StepOutcome {
            next_request: state.next_request.clone(),
            result: RunStepResult {
                index,
                item_name: item.name.clone(),
                request_path: item.request_path.clone(),
                status: RunStepStatus::Completed,
                status_code: Some(output.response.status),
                duration_ms: output.response.duration_ms,
                test_pass_count: passed,
                test_fail_count: failed,
                script_error: output.script_error.clone(),
                error: None,
            },
        }
    }

    fn publish_step(&self, run_id: &str, result: &RunStepResult) {
        self.events.publish(DomainEvent::RunnerStepCompleted {
            run_id: run_id.to_string(),
            index: result.index,
            item_name: result.item_name.clone(),
            request_path: result.request_path.clone(),
            status: result.status.as_str().to_string(),
            status_code: result.status_code,
            duration_ms: result.duration_ms,
            test_pass_count: result.test_pass_count,
            test_fail_count: result.test_fail_count,
            script_error: result.script_error.clone(),
            error: result.error.clone(),
        });
    }

    fn is_cancelled(&self, run_id: &str) -> bool {
        self.cancelled
            .lock()
            .map(|set| set.contains(run_id))
            .unwrap_or(false)
    }

    fn clear_cancellation(&self, run_id: &str) {
        if let Ok(mut set) = self.cancelled.lock() {
            set.remove(run_id);
        }
    }
}

/// An errored step with no response and no tests.
fn error_step(index: usize, item: &RunItem, message: String) -> RunStepResult {
    RunStepResult {
        index,
        item_name: item.name.clone(),
        request_path: item.request_path.clone(),
        status: RunStepStatus::Error,
        status_code: None,
        duration_ms: 0,
        test_pass_count: 0,
        test_fail_count: 0,
        script_error: None,
        error: Some(message),
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib collection_runner_service:: 2>&1 | tail -3`
Expected: `8 passed; 0 failed`.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/collection_runner_service.rs crates/rocket-app/src/lib.rs
git commit -m "feat(app): add CollectionRunnerService sequential run loop"
```

---

### Task 8: `skipRequest` and `setNextRequest`

Spec §4's table and the §7 acceptance criteria that depend on it.

**Files:**
- Modify: `crates/rocket-app/src/collection_runner_service.rs` (tests only — the loop written in Task 7 already implements this; these tests prove it)
- Test: `crates/rocket-app/src/collection_runner_service.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: everything from Task 7. Produces no new API.

- [ ] **Step 1: Write the tests**

Append to the test module in `crates/rocket-app/src/collection_runner_service.rs`:

```rust
    #[tokio::test]
    async fn skip_request_makes_no_http_call_and_runs_no_later_phase() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "Second",
            "before-request",
            ScriptResult { skip_request: true, ..Default::default() },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(
            h.executor.sent_urls(),
            vec![
                "https://api.test/first.yml".to_string(),
                "https://api.test/third.yml".to_string(),
            ],
            "the skipped request must never reach the executor"
        );
        assert_eq!(summary.steps[1].status, RunStepStatus::Skipped);
        assert_eq!(summary.steps[1].status_code, None);
        assert!(
            !h.engine.calls().contains(&"Second|after-response".to_string()),
            "a skipped step has no response, so no later phase may run"
        );
        assert!(!h.engine.calls().contains(&"Second|tests".to_string()));
        // Only the two sent requests are history-worthy.
        assert_eq!(h.history.saved_count(), 2);
    }

    #[tokio::test]
    async fn set_next_request_from_tests_phase_jumps_the_run() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Third".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Third"], "Second must be jumped over");
    }

    #[tokio::test]
    async fn skip_request_combined_with_set_next_request_honours_the_jump() {
        // Bruno cannot do this (usebruno/bruno#5831) because it only reads
        // setNextRequest from post-response scripts. Rocket checks after every
        // phase that ran, so both calls in one before-request script work.
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "before-request",
            ScriptResult {
                skip_request: true,
                next_request: Some(NextRequest::Name("Third".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Third"]);
        assert_eq!(summary.steps[0].status, RunStepStatus::Skipped);
        assert_eq!(h.executor.sent_urls(), vec!["https://api.test/third.yml".to_string()]);
    }

    #[tokio::test]
    async fn later_phase_wins_when_two_phases_set_next_request() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "before-request",
            ScriptResult {
                next_request: Some(NextRequest::Name("Second".into())),
                ..Default::default()
            },
        );
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Third".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        let names: Vec<&str> = summary.steps.iter().map(|s| s.item_name.as_str()).collect();
        assert_eq!(names, vec!["First", "Third"], "the last phase that ran wins");
    }

    #[tokio::test]
    async fn set_next_request_null_stops_the_run() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "after-response",
            ScriptResult { next_request: Some(NextRequest::Stop), ..Default::default() },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.stopped_reason, StoppedReason::StoppedByScript);
        assert_eq!(h.executor.sent_urls().len(), 1);
    }

    #[tokio::test]
    async fn unknown_next_request_records_an_error_and_stops() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Nowhere".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].status, RunStepStatus::Error);
        let error = summary.steps[0].error.as_deref().expect("error recorded on the step");
        assert!(error.contains("Nowhere"), "got {error}");
        assert_eq!(
            summary.stopped_reason,
            StoppedReason::UnknownNextRequest {
                item_name: "First".into(),
                next_request: "Nowhere".into(),
            }
        );
    }

    #[tokio::test]
    async fn a_next_request_cycle_stops_at_the_step_limit() {
        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("Second".into())),
                ..Default::default()
            },
        );
        engine.on(
            "Second",
            "tests",
            ScriptResult {
                next_request: Some(NextRequest::Name("First".into())),
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), MAX_RUN_STEPS);
        assert_eq!(
            summary.stopped_reason,
            StoppedReason::StepLimitReached { limit: MAX_RUN_STEPS }
        );
    }
```

`NextRequest` is already in scope in the test module: the parent module imports it and the test module does `use super::*`.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p rocket-app --lib collection_runner_service:: 2>&1 | tail -3`
Expected: `15 passed; 0 failed`. If `a_next_request_cycle_stops_at_the_step_limit` is slow, that is expected — it executes 1000 in-memory steps; it should still finish in well under a second.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-app/src/collection_runner_service.rs
git commit -m "test(app): cover runner skip, jump and stop behaviour"
```

---

### Task 9: Failure policy and cancellation

**Files:**
- Modify: `crates/rocket-app/src/collection_runner_service.rs`
- Test: `crates/rocket-app/src/collection_runner_service.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `CollectionRunnerService::cancel(&self, run_id: &str)` (public — the Tauri command calls it)
  - `CollectionRunnerService::with_cancellations(self, cancelled: Arc<Mutex<HashSet<String>>>) -> Self` (`pub(crate)`, test seam — lets a test flip a run to cancelled while it is in flight)

- [ ] **Step 1: Write the failing tests**

Append to the test module:

```rust
    /// Publisher that cancels the run as soon as it sees the Nth
    /// `RunnerStepCompleted` event, by writing straight into the shared
    /// cancellation registry the service was built with.
    struct CancelAfterSteps {
        cancel_after: usize,
        seen: Mutex<usize>,
        cancelled: Arc<Mutex<HashSet<String>>>,
    }

    impl EventPublisher for CancelAfterSteps {
        fn publish(&self, event: DomainEvent) {
            if let DomainEvent::RunnerStepCompleted { run_id, .. } = &event {
                let mut seen = self.seen.lock().expect("lock");
                *seen += 1;
                if *seen >= self.cancel_after {
                    self.cancelled.lock().expect("lock").insert(run_id.clone());
                }
            }
        }
    }

    #[tokio::test]
    async fn a_failed_step_does_not_stop_the_run_by_default() {
        let executor = RecordingExecutor::new();
        executor.set_status("second.yml", 500);
        let h = harness(three_step_collection(), ProgrammableEngine::new(), executor);
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 3, "every remaining item still runs");
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
        assert!(summary.steps[1].is_failure());
        assert_eq!(summary.steps[1].status_code, Some(500));
    }

    #[tokio::test]
    async fn stop_on_failure_ends_the_run_at_the_first_failure() {
        let executor = RecordingExecutor::new();
        executor.set_status("second.yml", 500);
        let h = harness(three_step_collection(), ProgrammableEngine::new(), executor);
        let mut input = sample_run_input();
        input.stop_on_failure = true;
        let summary = h.runner.run(&h.exec, input).await.expect("run");

        assert_eq!(summary.steps.len(), 2);
        assert_eq!(
            summary.stopped_reason,
            StoppedReason::StoppedOnFailure { item_name: "Second".into() }
        );
    }

    #[tokio::test]
    async fn a_failing_test_counts_as_a_step_failure() {
        use rocket_scripting::{TestResult, TestStatus};

        let engine = ProgrammableEngine::new();
        engine.on(
            "First",
            "tests",
            ScriptResult {
                test_results: vec![
                    TestResult { name: "ok".into(), status: TestStatus::Passed, error: None },
                    TestResult {
                        name: "nope".into(),
                        status: TestStatus::Failed,
                        error: Some("expected 200".into()),
                    },
                ],
                ..Default::default()
            },
        );
        let h = harness(three_step_collection(), engine, RecordingExecutor::new());
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps[0].test_pass_count, 1);
        assert_eq!(summary.steps[0].test_fail_count, 1);
        assert!(summary.steps[0].is_failure());
        assert_eq!(summary.steps.len(), 3, "continue-on-failure is the default");
    }

    #[tokio::test]
    async fn a_transport_error_is_an_error_step_and_the_run_continues() {
        // Status 0 makes the recording executor fail the send.
        let executor = RecordingExecutor::new();
        executor.set_status("second.yml", 0);
        let h = harness(three_step_collection(), ProgrammableEngine::new(), executor);
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 3);
        assert_eq!(summary.steps[1].status, RunStepStatus::Error);
        assert!(summary.steps[1].error.is_some());
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
    }

    #[tokio::test]
    async fn cancelling_mid_run_stops_before_the_next_step() {
        // Build the runner by hand so the test and the canceller share one
        // cancellation registry; the publisher cancels once step 1 reports.
        let collection = three_step_collection();
        let repo = InMemoryCollectionRepo::new(collection);
        let executor = RecordingExecutor::new();
        let engine = ProgrammableEngine::new();
        let cancelled: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        let exec = RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::new(SharedExecutor(Arc::clone(&executor))),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
        )
        .with_script_engine(Box::new(SharedEngine(Arc::clone(&engine))));

        let runner = CollectionRunnerService::new(
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(CancelAfterSteps {
                cancel_after: 1,
                seen: Mutex::new(0),
                cancelled: Arc::clone(&cancelled),
            }),
        )
        .with_cancellations(Arc::clone(&cancelled));

        let summary = runner.run(&exec, sample_run_input()).await.expect("run");

        assert_eq!(summary.steps.len(), 1, "the run stops before step 2 starts");
        assert_eq!(summary.stopped_reason, StoppedReason::Cancelled);
        assert_eq!(executor.sent_urls(), vec!["https://api.test/first.yml".to_string()]);
        assert!(
            !cancelled.lock().expect("lock").contains(&summary.run_id),
            "a finished run must not leak its id in the registry"
        );
    }

    #[tokio::test]
    async fn cancelling_an_unknown_run_id_is_a_no_op() {
        let h = harness(three_step_collection(), ProgrammableEngine::new(), RecordingExecutor::new());
        h.runner.cancel("not-a-real-run");
        let summary = h.runner.run(&h.exec, sample_run_input()).await.expect("run");
        assert_eq!(summary.stopped_reason, StoppedReason::Completed);
        assert_eq!(summary.steps.len(), 3);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app --lib collection_runner_service::`
Expected: FAIL — `no method named cancel found`.

- [ ] **Step 3: Add `cancel` and the test seam**

In `impl CollectionRunnerService`, add below `new`:

```rust
    /// Replaces the cancellation registry. Test seam — it lets a test hold the
    /// same registry the run loop reads and cancel a run while it is in flight.
    #[cfg(test)]
    pub(crate) fn with_cancellations(mut self, cancelled: Arc<Mutex<HashSet<String>>>) -> Self {
        self.cancelled = cancelled;
        self
    }
```

and below `run`:

```rust
    /// Asks an in-progress run to stop. The run ends before its next step; a
    /// step already in flight finishes first. Cancelling an unknown or finished
    /// run id is a no-op.
    pub fn cancel(&self, run_id: &str) {
        if let Ok(mut set) = self.cancelled.lock() {
            set.insert(run_id.to_string());
        }
    }
```

The test module also needs `use rocket_shared::events::{DomainEvent, EventPublisher};` — both are already imported by the parent module, so `use super::*` covers them; add `use std::collections::HashSet;` only if the compiler reports it missing (the parent imports it too).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app --lib collection_runner_service:: 2>&1 | tail -3`
Expected: `21 passed; 0 failed`.

- [ ] **Step 5: Run the whole crate**

Run: `cargo test -p rocket-app 2>&1 | tail -3`
Expected: all green, including the untouched 33 `execution_service` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/collection_runner_service.rs
git commit -m "feat(app): add stop-on-failure and run cancellation to the runner"
```

---

### Task 10: Tauri IPC, DI wiring, and docs

**Files:**
- Create: `src-tauri/src/commands/runner.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (imports, service construction near line 207, `app.manage` near line 238, `generate_handler!` near line 300)
- Modify: `crates/rocket-app/CLAUDE.md`

**Interfaces:**
- Consumes: `CollectionRunnerService`, `RunCollectionInput`, `RunSummary` (Task 7), `RequestExecutionService` (managed state).
- Produces: the two IPC commands documented in "Frontend contract" below.

- [ ] **Step 1: Create the command module**

Create `src-tauri/src/commands/runner.rs`:

```rust
use rocket_app::{CollectionRunnerService, RequestExecutionService, RunCollectionInput, RunSummary};
use rocket_shared::error::DomainError;
use tauri::State;

/// Runs every request in a collection or folder, in order.
///
/// Streams `runner-started`, `runner-step-completed` and `runner-finished`
/// events while it runs, and returns the same data as one summary when the run
/// ends — mirroring how `run_load_test_v2_command` streams progress and the
/// frontend reads results from the events.
#[tauri::command]
pub async fn run_collection(
    input: RunCollectionInput,
    runner: State<'_, CollectionRunnerService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<RunSummary, DomainError> {
    runner.run(&exec, input).await
}

/// Asks an in-progress run to stop. The run ends before its next step; the step
/// already in flight finishes first. An unknown or finished run id is a no-op.
#[tauri::command]
pub fn stop_collection_run(
    run_id: String,
    runner: State<'_, CollectionRunnerService>,
) -> Result<(), DomainError> {
    runner.cancel(&run_id);
    Ok(())
}
```

- [ ] **Step 2: Register the module**

In `src-tauri/src/commands/mod.rs`, add (keeping the list's existing ordering style):

```rust
pub mod runner;
```

- [ ] **Step 3: Construct and manage the service**

In `src-tauri/src/lib.rs`, add `CollectionRunnerService` to the `rocket_app::{...}` import list at line 10-11. Then, directly after the `oauth2_svc` construction block (around line 210), add:

```rust
            // Collection Runner — its own collection repo instance (same path as
            // the execution service) and the Tauri bus, so run progress reaches
            // the frontend as it happens.
            let runner_svc = CollectionRunnerService::new(
                Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );
```

Then add `app.manage(runner_svc);` immediately after `app.manage(exec_svc);` (line 235).

- [ ] **Step 4: Register the commands**

In the `generate_handler![...]` list, directly after `commands::execution::evaluate_var_expression,`:

```rust
            commands::runner::run_collection,
            commands::runner::stop_collection_run,
```

- [ ] **Step 5: Build**

Run: `cargo check --workspace 2>&1 | tail -20`
Expected: clean. A `CollectionRunnerService is not Send/Sync` error would mean the `Mutex<HashSet<String>>` field was written as something else — it must be `std::sync::Mutex`, and no lock guard may be held across an `.await`.

- [ ] **Step 6: Document the new services**

In `crates/rocket-app/CLAUDE.md`, add these rows to the "Public Types" table (after the `RequestExecutionService` row):

```markdown
| `CollectionRunnerService` | Runs a folder's/collection's requests in sequence; honours `rok.runner.setNextRequest`/`skipRequest`; publishes `RunnerStarted/StepCompleted/Finished`. |
| `RunCollectionInput` / `RunSummary` | IPC DTOs for `CollectionRunnerService::run`. |
```

And add this bullet to "Key Patterns":

```markdown
- **Phase-callable execution.** `RequestExecutionService::execute` is a thin
  composition over `begin_phases` → `run_before_request_phase` → `send_request`
  → `run_after_response_phase` → `run_tests_phase` → `finish_phases`, all
  sharing one `PhaseState`. `CollectionRunnerService` drives the same methods
  one phase at a time so it can act on `skip_request` before the send and on
  `next_request` after every phase. Do not add phase logic to only one caller.
```

- [ ] **Step 7: Full verification**

```bash
cargo check --workspace
cargo test -p rocket-shared 2>&1 | tail -3
cargo test -p rocket-scripting 2>&1 | tail -3
cargo test -p rocket-app 2>&1 | tail -3
cargo test -p rocket-app --lib execution_service:: 2>&1 | tail -3
```

Expected: all green; the last one still reports 37 passed (33 original + 4 added in Task 3), with the original 33 never edited.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/runner.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs crates/rocket-app/CLAUDE.md
git commit -m "feat(tauri): add run_collection and stop_collection_run commands"
```

---

## Frontend contract

Everything below is final and is what the frontend plan should be written against.

### IPC commands

```ts
// invoke('run_collection', { input })  — resolves when the run ends
interface RunCollectionInput {
  collection: string;              // collection name
  folderPath?: string | null;      // on-disk dir path relative to the collection root; omit/null = whole collection
  environmentName?: string | null;
  globalEnvName?: string | null;
  stopOnFailure?: boolean;         // default false
}

interface RunSummary {
  runId: string;
  collection: string;
  folderPath: string | null;
  steps: RunStepResult[];
  stoppedReason: StoppedReason;
}

interface RunStepResult {
  index: number;                   // position in the executed-step stream, from 0
  itemName: string;
  requestPath: string;             // e.g. "auth/login.yml"
  status: 'completed' | 'skipped' | 'error';
  statusCode: number | null;       // null when skipped or errored
  durationMs: number;
  testPassCount: number;
  testFailCount: number;
  scriptError: string | null;
  error: string | null;            // transport or sequencing error
}

type StoppedReason =
  | { kind: 'completed' }
  | { kind: 'stoppedByScript' }
  | { kind: 'stoppedOnFailure'; itemName: string }
  | { kind: 'unknownNextRequest'; itemName: string; nextRequest: string }
  | { kind: 'cancelled' }
  | { kind: 'stepLimitReached'; limit: number };

// invoke('stop_collection_run', { runId })  — returns void
```

`run_collection` rejects with a `DomainError` string (serialised as its `Display` text) when the collection or the folder path does not exist. Per-step failures never reject — they come back as `status: 'error'` rows.

Both commands are `Result<_, DomainError>` on the Rust side, so a rejection is a plain string in JS.

### Events

Emitted on the Tauri event bus. **Payload fields are snake_case** — `DomainEvent`'s `rename_all = "camelCase"` renames variants only, not struct-variant fields (verified against the existing `request-executed` payload). Do not assume camelCase here; the command return values above *are* camelCase.

| Channel | Payload |
|---|---|
| `runner-started` | `{ type: 'runnerStarted', run_id: string, collection: string, folder_path: string \| null, total_steps: number }` |
| `runner-step-completed` | `{ type: 'runnerStepCompleted', run_id: string, index: number, item_name: string, request_path: string, status: 'completed' \| 'skipped' \| 'error', status_code: number \| null, duration_ms: number, test_pass_count: number, test_fail_count: number, script_error: string \| null, error: string \| null }` |
| `runner-finished` | `{ type: 'runnerFinished', run_id: string, stopped_reason: 'completed' \| 'stoppedByScript' \| 'stoppedOnFailure' \| 'unknownNextRequest' \| 'cancelled' \| 'stepLimitReached', step_count: number, failed_count: number }` |

Ordering guarantee: exactly one `runner-started`, then one `runner-step-completed` per executed step in execution order (`index` 0,1,2,…), then exactly one `runner-finished`. `total_steps` is the run set's size; a `setNextRequest` jump can make the executed-step count differ from it, so drive progress off `index` and treat `total_steps` as an estimate.

The `run_id` needed by `stop_collection_run` arrives in `runner-started`, before any step completes.

Notes for the run view:
- A `skipped` step has `status_code: null` and no test counts — render it as "skipped by script".
- A step "failed" when `status === 'error'`, or `status_code` is outside 200-299, or `test_fail_count > 0`. `runner-finished.failed_count` already applies that rule.
- Each step also emits the existing per-request events (`script-console`, `script-tests`, `request-executed`) only if the execution service is wired with a publishing bus; in production it is wired with `NullEventPublisher`, so the runner events above are the frontend's only source of run progress.
