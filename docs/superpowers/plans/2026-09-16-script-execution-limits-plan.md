# Script Execution Timeout and Memory Limit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound the wall-clock time (and, optionally, the V8 heap) of a single JS script execution so a malicious or buggy script surfaces as an ordinary script error instead of hanging the request forever.

**Architecture:** `DenoScriptEngine::execute` currently fires `run_script` into a bare `tokio::task::spawn_blocking` with no deadline. We wrap that join handle in a `tokio::time::timeout`, and — because cancelling an async future does *not* stop the OS thread actually running V8 — we also plumb a `v8::IsolateHandle` back out of the blocking thread over a `tokio::sync::oneshot` channel so the timeout path can call `IsolateHandle::terminate_execution()`, which genuinely aborts the running script. The real `SCRIPT_TIMEOUT` lives only in `execute()`; all the mechanics live in an internal `run_script_with_timeout(ctx, timeout)` helper that tests call directly with a 200 ms budget, so the test suite never waits 5 real seconds.

**Tech Stack:** Rust, `deno_core = "0.400.0"` (re-exports `v8 = 147.4.0` as `deno_core::v8`), `tokio` (`spawn_blocking`, `time::timeout`, `sync::oneshot`), `async_trait`.

**Spec:** [`docs/superpowers/specs/2026-09-16-script-execution-limits-spec.md`](../specs/2026-09-16-script-execution-limits-spec.md)

---

## Global Constraints

- **No new direct dependencies.** `v8` types are reached through `deno_core::v8` only. Do **not** add a `v8 = "…"` line to `crates/rocket-infra/Cargo.toml` — a separately-resolved `v8` crate would be a different type universe from the one `deno_core` links, and `IsolateHandle` would not be interchangeable.
- **`SCRIPT_TIMEOUT` is a hardcoded constant**, 5 seconds. No user-facing setting in this pass (spec §4).
- **Rust: never `unwrap` in production paths** (project CLAUDE.md hard rule). `unwrap`/`expect` inside `#[cfg(test)]` is fine and matches the existing engine tests.
- **No `#[serde(rename_all = "camelCase")]`** is added anywhere by this plan — no persistence or IPC struct changes.
- **Comments: short, plain full sentences ending in a punctuation mark.** No emoji.
- **Commits: conventional commits** (`feat:`, `fix:`, `test:`, `chore:`).
- **The public `ScriptEngine::execute` trait signature does not change.** This is an internal behavior change only (spec §5).
- **Verification commands:** `cargo check -p rocket-infra`, `cargo test -p rocket-infra`, `cargo test -p rocket-app`.

---

## API Verification Findings (done up front — read this before Task 1)

The spec self-flagged the `deno_core` / `v8` API surface as "verify at implementation time". **That verification has been done** and is recorded here so a reviewer can double-check it. Method: inspection of the vendored registry sources at
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{deno_core-0.400.0,v8-147.4.0}/`, plus an actual `cargo check` and `cargo run` of a throwaway `examples/` probe against this workspace (the probe file was deleted afterwards and is not part of this plan).

| Question | Answer | Evidence |
|---|---|---|
| Locked versions | `deno_core 0.400.0`, transitively `v8 147.4.0` | `Cargo.lock` |
| Does `deno_core` re-export `v8`? | **Yes.** `pub use v8;` — so `deno_core::v8::IsolateHandle` is the correct path, and no new dependency is needed. | `deno_core-0.400.0/lib.rs:77` |
| `JsRuntime::v8_isolate()` | `pub fn v8_isolate(&mut self) -> &mut v8::OwnedIsolate` | `deno_core-0.400.0/runtime/jsruntime.rs:1237` |
| `OwnedIsolate` → `Isolate` | `impl Deref/DerefMut for OwnedIsolate`, so `Isolate` methods are callable directly on it. | `v8-147.4.0/src/isolate.rs:2200,2209` |
| `thread_safe_handle()` | `pub fn thread_safe_handle(&self) -> IsolateHandle` | `v8-147.4.0/src/isolate.rs:963` |
| Is `IsolateHandle` `Send + Sync + Clone`? | **Yes.** `#[derive(Clone)] pub struct IsolateHandle(Arc<IsolateHandleInner>)` with `unsafe impl Send/Sync for IsolateHandleInner`. Its doc comment states: "IsolateHandle is Cloneable, Send, and Sync." | `v8-147.4.0/src/isolate.rs:1973-1983` |
| `IsolateHandle::terminate_execution()` | `pub fn terminate_execution(&self) -> bool` — "can be used by any thread even if that thread has not acquired the V8 lock". Returns `false` if the isolate was already destroyed, so calling it on a script that already finished is safe and is a no-op. | `v8-147.4.0/src/isolate.rs:2053` |
| `RuntimeOptions.create_params` | `pub create_params: Option<v8::CreateParams>` — the field exists on `RuntimeOptions`. | `deno_core-0.400.0/runtime/jsruntime.rs:512` |
| `CreateParams::heap_limits` | `pub fn heap_limits(mut self, initial: usize, max: usize) -> Self` | `v8-147.4.0/src/isolate_create_params.rs:137` |
| Near-heap-limit callback | `deno_core` exposes it directly on `JsRuntime`: `pub fn add_near_heap_limit_callback<C>(&mut self, cb: C) where C: FnMut(usize, usize) -> usize + 'static`. **Use this, not the raw `v8::Isolate` method** — `deno_core` owns the callback allocation. Its doc: "Use this to prevent V8 from crashing the process when reaching the limit." | `deno_core-0.400.0/runtime/jsruntime.rs:1958` |
| What does `execute_script` return when terminated? | **Empirically measured**, not assumed. Running `while(true){}` and calling `terminate_execution()` from another thread after 300 ms gave: `terminate_execution() -> true`, elapsed `300.5ms`, `Err -> Uncaught Error: execution terminated`, and then `drop(runtime)` completed cleanly with no panic and no abort. | Throwaway probe, since deleted. |

**Existing precedent in this repo:** none. `grep -rn "v8::" --include="*.rs" crates src-tauri` returns zero matches today — this plan introduces the first direct `v8` usage in the codebase.

**One resolved spec open question.** Spec §3.1 asks whether `RequestExecutionService::run_script_phase` already handles the `Err` branch. **It does** — `crates/rocket-app/src/execution_service.rs:373-381` already has an `Err(e) => { self.events.publish(DomainEvent::ScriptError { … message: e.to_string() }); ScriptResult::default() }` arm. So no *event* change is needed. However there is a real gap the spec did not anticipate: `ExecuteOutput.script_error` is populated **only** from `result.error` (`execution_service.rs:672,725,755`), and that `Err` arm returns `ScriptResult::default()` whose `error` is `None`. So today a timeout would fire the event but leave `output.script_error` empty, and the inline script-error surface in the UI would stay blank. **Task 3 closes that.**

**Deliberate deviation from the spec's sample code.** The spec sketches `handle_rx.try_recv()` on the timeout path. `try_recv()` fails if the blocking thread has started but has not yet finished `JsRuntime::new(...)` — a real window of a few milliseconds — and a missed handle means a genuinely leaked spinning thread. This plan uses a short bounded `tokio::time::timeout(HANDLE_WAIT, handle_rx).await` instead, which closes that race. The rest of §3.1 is followed as written.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/rocket-infra/src/scripting/engine.rs` | Owns `DenoScriptEngine`, `run_script`, the extension registration, and the engine unit tests. This is where the timeout lives, because it is the only place that owns the `JsRuntime` and the `spawn_blocking` call. | Modify (Tasks 1, 2, 4) |
| `crates/rocket-app/src/execution_service.rs` | Orchestrates the script phases and builds `ExecuteOutput`. Only `run_script_phase`'s `Err` arm changes. | Modify (Task 3) |
| `crates/rocket-infra/Cargo.toml` | — | **Unchanged.** Deliberately: no new dependency is required, per the verification table above. |

No new files are created. The timeout logic is roughly 35 lines and belongs next to the `spawn_blocking` it guards; splitting it into its own module would separate it from its only caller and from the existing test module that must exercise it.

---

### Task 1: Wall-clock timeout with real V8 termination

This is the primary fix and the one that closes the audit finding. Everything else in this plan is optional relative to this task.

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs:1-2` (imports)
- Modify: `crates/rocket-infra/src/scripting/engine.rs:28-37` (`impl ScriptEngine for DenoScriptEngine`)
- Modify: `crates/rocket-infra/src/scripting/engine.rs:137-143` (`run_script` signature and runtime construction)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (`#[cfg(test)] mod tests`, append at the end, before the module's closing brace at line 551)

**Interfaces:**
- Consumes: the existing `fn run_script(ctx: ScriptContext) -> DomainResult<ScriptResult>` and the existing `#[cfg(test)] fn minimal_ctx(code: &str) -> ScriptContext` test helper.
- Produces:
  - `const SCRIPT_TIMEOUT: std::time::Duration` (module-private, 5 s).
  - `const HANDLE_WAIT: std::time::Duration` (module-private, 250 ms).
  - `async fn run_script_with_timeout(ctx: ScriptContext, timeout: std::time::Duration) -> DomainResult<ScriptResult>` (module-private) — Tasks 2 and 4 call this directly from tests.
  - `fn run_script(ctx: ScriptContext, handle_tx: tokio::sync::oneshot::Sender<deno_core::v8::IsolateHandle>) -> DomainResult<ScriptResult>` — the signature gains a second parameter.
  - On timeout the returned error is `DomainError::Internal(format!("script execution timed out after {timeout:?}"))`, which `Display`s as `Internal error: script execution timed out after 200ms`.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `crates/rocket-infra/src/scripting/engine.rs`, just before its closing brace:

```rust
    /// Short budget so the timeout tests finish fast instead of waiting the
    /// real five-second SCRIPT_TIMEOUT.
    const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

    #[tokio::test]
    async fn infinite_loop_script_is_terminated_by_timeout() {
        let ctx = minimal_ctx("while (true) {}");

        let started = std::time::Instant::now();
        let outcome = run_script_with_timeout(ctx, TEST_TIMEOUT).await;
        let elapsed = started.elapsed();

        let err = outcome.expect_err("an infinite loop must not return Ok");
        assert!(
            err.to_string().contains("timed out"),
            "expected a timeout error, got: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "execute() must return promptly after the deadline, took {elapsed:?}"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-infra infinite_loop_script_is_terminated_by_timeout`
Expected: **compile FAIL** — `cannot find function 'run_script_with_timeout' in this scope`. That is the correct failure; the function does not exist yet. Do not expect a hang here — the test cannot even build.

- [ ] **Step 3: Write minimal implementation**

3a. Change the import line at `engine.rs:2` from:

```rust
use deno_core::{extension, JsRuntime, OpState, RuntimeOptions, op2};
```

to:

```rust
use deno_core::{extension, v8, JsRuntime, OpState, RuntimeOptions, op2};
use std::time::Duration;
use tokio::sync::oneshot;
```

3b. Replace the whole `#[async_trait] impl ScriptEngine for DenoScriptEngine { … }` block at `engine.rs:28-37` with:

```rust
/// Wall-clock budget for a single script execution.
///
/// Five seconds comfortably exceeds any legitimate pre-request, post-response,
/// or test script. Those scripts do in-memory templating, signing, and small
/// JSON manipulation. They have no network or filesystem access at all, so
/// there is nothing legitimate for them to wait on.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);

/// Grace period for the blocking thread to publish its isolate handle.
///
/// The handle is the first thing `run_script` sends, but the blocking thread
/// may not have been scheduled yet when the deadline fires. Waiting briefly
/// here is what makes termination reliable instead of best-effort.
const HANDLE_WAIT: Duration = Duration::from_millis(250);

#[async_trait]
impl ScriptEngine for DenoScriptEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        run_script_with_timeout(ctx, SCRIPT_TIMEOUT).await
    }
}

/// Runs a script on a blocking thread and aborts it if `timeout` elapses.
///
/// Cancelling the async future alone would not stop the OS thread running V8,
/// so on timeout we ask V8 itself to abort the script through the isolate
/// handle the thread published on start. Tests call this directly with a short
/// timeout so the suite never waits the full `SCRIPT_TIMEOUT`.
async fn run_script_with_timeout(
    ctx: ScriptContext,
    timeout: Duration,
) -> DomainResult<ScriptResult> {
    // JsRuntime is !Send, so all V8 work must stay on one thread.
    let (handle_tx, handle_rx) = oneshot::channel();
    let join = tokio::task::spawn_blocking(move || run_script(ctx, handle_tx));

    match tokio::time::timeout(timeout, join).await {
        Ok(join_result) => join_result
            .map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?,
        Err(_elapsed) => {
            // Terminating makes the blocking thread's execute_script return an
            // "execution terminated" error. It then tears the runtime down on
            // its own and its result is discarded, so we do not wait for it.
            if let Ok(Ok(isolate_handle)) = tokio::time::timeout(HANDLE_WAIT, handle_rx).await {
                isolate_handle.terminate_execution();
            }
            Err(DomainError::Internal(format!(
                "script execution timed out after {timeout:?}"
            )))
        }
    }
}
```

3c. Change the `run_script` signature and add the handle send. At `engine.rs:137-143`, replace:

```rust
fn run_script(ctx: ScriptContext) -> DomainResult<ScriptResult> {
    let code = ctx.code;

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        ..Default::default()
    });
```

with:

```rust
fn run_script(
    ctx: ScriptContext,
    handle_tx: oneshot::Sender<v8::IsolateHandle>,
) -> DomainResult<ScriptResult> {
    let code = ctx.code;

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        ..Default::default()
    });

    // Publish the isolate handle before running any script code, so a timeout
    // can always reach it. A send failure only means the caller already gave
    // up, and there is nothing useful to do about that here.
    let _ = handle_tx.send(runtime.v8_isolate().thread_safe_handle());
```

Everything else in `run_script` — the `OpState` seeding, the bootstrap `execute_script`, the user `execute_script`, and the `ScriptResult` construction — is left exactly as it is.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rocket-infra infinite_loop_script_is_terminated_by_timeout -- --nocapture`
Expected: **PASS**, in roughly 0.2-0.3 s of script time plus compile time. If it instead hangs, the handle is not reaching the timeout path — re-check step 3c's `handle_tx.send` placement.

- [ ] **Step 5: Run the full crate suite to verify no regression**

Run: `cargo test -p rocket-infra`
Expected: PASS. All 30-odd pre-existing `scripting::engine::tests` still pass unchanged — they go through `engine.execute(ctx)`, which now routes via `run_script_with_timeout(ctx, SCRIPT_TIMEOUT)` and is unaffected because they all finish in milliseconds.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/scripting/engine.rs
git commit -m "fix: bound script execution with a 5s timeout and V8 isolate termination"
```

---

### Task 2: Regression guards — fast scripts unaffected, repeated terminations safe

Covers spec acceptance criteria 3 and 4. These are characterization tests: they are expected to pass immediately on top of Task 1. Write them anyway — they are the guard rails that catch a future refactor silently breaking either property. Step 3 checks they are really exercising the new path rather than passing for free.

**Files:**
- Test: `crates/rocket-infra/src/scripting/engine.rs` (`#[cfg(test)] mod tests`, append after `infinite_loop_script_is_terminated_by_timeout`)

**Interfaces:**
- Consumes: `run_script_with_timeout`, `TEST_TIMEOUT`, and `minimal_ctx` from Task 1.
- Produces: nothing other tasks depend on.

- [ ] **Step 1: Write the tests**

Append inside `mod tests`, after the Task 1 test:

```rust
    #[tokio::test]
    async fn fast_script_is_unaffected_by_the_timeout() {
        let ctx = minimal_ctx("console.log('quick'); rok.setVar('x', 'ok')");

        // Deliberately uses the same 200ms budget as the timeout test. A normal
        // script must complete well inside it with a fully populated result.
        let result = run_script_with_timeout(ctx, TEST_TIMEOUT)
            .await
            .expect("a fast script must not be affected by the timeout");

        assert!(result.error.is_none(), "unexpected error: {:?}", result.error);
        assert_eq!(result.runtime_vars.get("x").expect("x present"), "ok");
        assert_eq!(result.console_entries.len(), 1);
        assert!(result.console_entries[0].message.contains("quick"));
    }

    #[tokio::test]
    async fn repeated_timeouts_do_not_crash_or_wedge_the_engine() {
        // Five back-to-back terminations. Each one leaves a blocking thread to
        // unwind, so this also checks those threads are actually released
        // rather than leaked until the pool is exhausted.
        for attempt in 0..5 {
            let ctx = minimal_ctx("while (true) {}");
            let outcome = run_script_with_timeout(ctx, TEST_TIMEOUT).await;
            assert!(outcome.is_err(), "attempt {attempt} should have timed out");
        }

        // Reaching this line at all proves the process did not abort. A working
        // script afterwards proves the engine is not wedged.
        let ctx = minimal_ctx("rok.setVar('alive', 'yes')");
        let result = run_script_with_timeout(ctx, TEST_TIMEOUT)
            .await
            .expect("engine must still work after repeated terminations");
        assert_eq!(result.runtime_vars.get("alive").expect("alive present"), "yes");
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p rocket-infra fast_script_is_unaffected_by_the_timeout repeated_timeouts_do_not_crash_or_wedge_the_engine`

Expected: both PASS. `repeated_timeouts_…` takes roughly 1 s wall clock — five iterations of 200 ms plus teardown. If it takes materially longer than 3 s, terminated threads are not unwinding promptly and that is worth investigating before moving on.

- [ ] **Step 3: Sanity-check that the guards actually guard**

Temporarily comment out the `if let Ok(Ok(isolate_handle)) = … { isolate_handle.terminate_execution(); }` block inside `run_script_with_timeout`, then run:

`cargo test -p rocket-infra repeated_timeouts_do_not_crash_or_wedge_the_engine -- --nocapture`

Without termination the five infinite loops now spin forever in the background for the life of the test binary. On most machines the run visibly slows or the process lingers at exit, and CPU sits pegged. That difference is the proof the termination call is load-bearing.

**Restore the block immediately afterwards.** Then run `git diff crates/rocket-infra/src/scripting/engine.rs` and confirm the `terminate_execution()` call is back before continuing. This step exists only to validate the test, never to leave a change behind.

- [ ] **Step 4: Run the full crate suite**

Run: `cargo test -p rocket-infra`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/scripting/engine.rs
git commit -m "test: guard fast-script latency and repeated script termination safety"
```

---

### Task 3: Surface a timed-out script in `ExecuteOutput.script_error`

`run_script_phase` already publishes `DomainEvent::ScriptError` on an `Err` from the engine, so the event half of spec acceptance criterion 2 is already satisfied. But it returns `ScriptResult::default()`, whose `error` is `None`, and `ExecuteOutput.script_error` is populated *only* from `result.error`. Without this task a timed-out script produces an event but an empty `script_error` field, so the timeout would not surface "exactly like an uncaught JS exception" as the spec's goal requires.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs:373-381` (the `Err(e)` arm of `run_script_phase`)
- Test: `crates/rocket-app/src/execution_service.rs` (`#[cfg(test)] mod tests`, append near the other `with_script_engine` tests)

**Interfaces:**
- Consumes: `ScriptEngine`, `ScriptContext`, `ScriptResult` (already imported at `execution_service.rs:12-14`); `DomainError` and `DomainResult` (already imported inside `mod tests`); the existing test helpers `MockEnvRepo::empty()`, `MockExecutor::new(200)`, `MockHistoryRepo::new()`, `StubCollectionRepo::empty()`, `NullCookieRepo`, `NullEventPublisher`, `sample_input(url, env)`, and the builder `RequestExecutionService::new(...).with_script_engine(Box<dyn ScriptEngine>)`.
- Produces: no signature change. `run_script_phase` keeps returning `ScriptResult`; only the `Err` arm's value changes.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `crates/rocket-app/src/execution_service.rs`:

```rust
    #[tokio::test]
    async fn script_engine_error_surfaces_in_output_script_error() {
        // Stands in for a timed-out script: the engine returns Err, not an
        // Ok(ScriptResult) that carries an error.
        struct FailingEngine;

        #[async_trait]
        impl ScriptEngine for FailingEngine {
            async fn execute(&self, _ctx: ScriptContext) -> DomainResult<ScriptResult> {
                Err(DomainError::Internal(
                    "script execution timed out after 5s".into(),
                ))
            }
        }

        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(MockExecutor::new(200)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        )
        .with_script_engine(Box::new(FailingEngine));

        let mut input = sample_input("https://example.com", None);
        input.pre_request_script = Some("while (true) {}".into());
        let output = svc.execute(input).await.expect("execute failed");

        // The request itself must still complete normally.
        assert_eq!(output.response.status, 200);

        let err = output
            .script_error
            .expect("a timed-out script must populate script_error, not just fire an event");
        assert!(err.contains("timed out"), "unexpected message: {err}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-app script_engine_error_surfaces_in_output_script_error`
Expected: **FAIL** with a panic on `.expect("a timed-out script must populate script_error, not just fire an event")` — the `Option` is `None` because the `Err` arm currently returns `ScriptResult::default()`.

- [ ] **Step 3: Write minimal implementation**

In `crates/rocket-app/src/execution_service.rs`, replace the `Err` arm of the `match engine.execute(ctx).await` inside `run_script_phase`:

```rust
            Err(e) => {
                self.events.publish(DomainEvent::ScriptError {
                    request_name: request_name.to_string(),
                    phase: phase.to_string(),
                    message: e.to_string(),
                });
                ScriptResult::default()
            }
```

with:

```rust
            Err(e) => {
                let message = e.to_string();
                self.events.publish(DomainEvent::ScriptError {
                    request_name: request_name.to_string(),
                    phase: phase.to_string(),
                    message: message.clone(),
                });
                // Carry the failure in `error` as well. Callers build
                // ExecuteOutput.script_error from this field only, so without
                // it a timed-out script would fire an event but show nothing
                // in the request's own error surface.
                ScriptResult {
                    error: Some(message),
                    ..Default::default()
                }
            }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rocket-app script_engine_error_surfaces_in_output_script_error`
Expected: PASS.

- [ ] **Step 5: Run both crate suites**

Run: `cargo test -p rocket-infra -p rocket-app`
Expected: PASS. Watch for any existing test that asserts `output.script_error.is_none()` while using a mock engine that returns `Err` — if one shows up, it was encoding the old gap, so update its expectation to the new message rather than working around it.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "fix: surface script engine errors in ExecuteOutput.script_error"
```

---

### Task 4 (OPTIONAL, LOWER CONFIDENCE): V8 heap limit

> **This task is explicitly deferrable.** Spec §3.2 scopes it as a separate, lower-confidence addition, and spec §3.2/§4 state that Tasks 1-3 alone ship as the primary fix — they are what close the "hangs forever" audit finding. **Do not let this task block, reshape, or complicate Tasks 1-3.** If it fights you, or if the regression test below is flaky on any target platform, stop, revert this task only, and ship Tasks 1-3. The timeout already caps how much an allocate-in-a-loop script can do, because total allocation is bounded by rate times `SCRIPT_TIMEOUT`.
>
> **Why the confidence is lower:** setting `heap_limits` **without** a near-heap-limit callback makes V8 take its default out-of-memory action, which is to **abort the whole process** — strictly worse than the status quo. The callback is not optional garnish here; it is the entire safety mechanism. `deno_core`'s own doc on `add_near_heap_limit_callback` says: "Use this to prevent V8 from crashing the process when reaching the limit."

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (`run_script`, the `JsRuntime::new` call and the lines immediately after it)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `run_script_with_timeout`, `TEST_TIMEOUT`, `minimal_ctx` from Tasks 1-2; the `handle_tx` parameter added to `run_script` in Task 1.
- Produces: `const SCRIPT_HEAP_LIMIT_BYTES: usize` (module-private, 256 MiB). No signature changes.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests`:

```rust
    #[tokio::test]
    async fn memory_hog_script_does_not_abort_the_process() {
        // Roughly 1 MB per iteration, so the 256 MB cap is reached in well
        // under a second. Either outcome is acceptable — the heap limit firing
        // or the wall-clock timeout firing first — what must never happen is
        // the process aborting.
        let ctx = minimal_ctx("let s = ''; while (true) { s += 'x'.repeat(1000000); }");

        let outcome = run_script_with_timeout(ctx, std::time::Duration::from_secs(5)).await;

        match outcome {
            Ok(result) => assert!(
                result.error.is_some(),
                "a heap-exhausting script must report an error, got a clean result"
            ),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("timed out") || msg.contains("terminated"),
                    "unexpected error: {msg}"
                );
            }
        }

        // Reaching this line proves the process survived. A working script
        // afterwards proves the engine is not wedged.
        let ctx = minimal_ctx("rok.setVar('alive', 'yes')");
        let result = run_script_with_timeout(ctx, TEST_TIMEOUT)
            .await
            .expect("engine must still work after a heap-limit termination");
        assert_eq!(result.runtime_vars.get("alive").expect("alive present"), "yes");
    }
```

- [ ] **Step 2: Run test to record its pre-implementation behaviour**

Run: `cargo test -p rocket-infra memory_hog_script_does_not_abort_the_process -- --nocapture`

Expected **before** the implementation: PASS, but slowly — the 5-second wall-clock deadline from Task 1 is what stops it, after the script has allocated freely for five seconds. **Write down the wall-clock time the harness reports.**

This is an unusual TDD shape and it is worth being honest about: the test passes either way, because Task 1 already prevents the catastrophic outcome. What Task 4 buys is bounding *memory* rather than only *time*, and the observable signal is the test getting much faster — sub-second instead of about 5 s — because the heap limit fires long before the deadline. That timing difference is the assertion that matters, which is why Step 2's number must be recorded before Step 3.

- [ ] **Step 3: Write minimal implementation**

In `crates/rocket-infra/src/scripting/engine.rs`, add the constant next to `SCRIPT_TIMEOUT`:

```rust
/// Best-effort V8 heap cap for a single script execution.
///
/// Generous on purpose. Legitimate scripts manipulate small JSON payloads, so
/// this only catches runaway allocation and never ordinary work.
const SCRIPT_HEAP_LIMIT_BYTES: usize = 256 * 1024 * 1024;
```

Then, in `run_script`, replace the runtime construction and handle send added in Task 1:

```rust
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        ..Default::default()
    });

    // Publish the isolate handle before running any script code, so a timeout
    // can always reach it. A send failure only means the caller already gave
    // up, and there is nothing useful to do about that here.
    let _ = handle_tx.send(runtime.v8_isolate().thread_safe_handle());
```

with:

```rust
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        create_params: Some(
            v8::CreateParams::default().heap_limits(0, SCRIPT_HEAP_LIMIT_BYTES),
        ),
        ..Default::default()
    });

    let isolate_handle = runtime.v8_isolate().thread_safe_handle();

    // Publish the isolate handle before running any script code, so a timeout
    // can always reach it. A send failure only means the caller already gave
    // up, and there is nothing useful to do about that here.
    let _ = handle_tx.send(isolate_handle.clone());

    // Without this callback V8's default near-OOM behaviour is to abort the
    // whole process, which would be worse than the problem we are fixing.
    // Terminating the isolate turns an out-of-memory into an ordinary script
    // error instead. The raised limit returned here is only headroom for V8 to
    // unwind in. Termination is what actually stops the script.
    runtime.add_near_heap_limit_callback(move |current, _initial| {
        isolate_handle.terminate_execution();
        current + (current / 4)
    });
```

Notes for the implementer:
- `heap_limits(0, max)` — `0` for the initial limit means "let V8 choose"; only the maximum is being constrained.
- The closure must be `FnMut(usize, usize) -> usize + 'static`, which is why `isolate_handle` is moved into it. It is `Clone` and `Send`, per the verification table.
- Use `JsRuntime::add_near_heap_limit_callback`, **not** `v8::Isolate::add_near_heap_limit_callback`. `deno_core` owns the boxed-callback allocation and keeps it alive for the runtime's lifetime; going around it leaks or dangles.

- [ ] **Step 4: Run test to verify it passes, and compare the timing**

Run: `cargo test -p rocket-infra memory_hog_script_does_not_abort_the_process -- --nocapture`

Expected: PASS, and **substantially faster than the number recorded in Step 2** — under a second of script time rather than the full 5-second deadline, because the heap limit now fires first.

If the process **aborts** — output like `FATAL ERROR: … JavaScript heap out of memory`, or the test harness reporting a signal rather than a test failure — the callback is not wired correctly. Do not patch around it. Revert this task only (`git checkout crates/rocket-infra/src/scripting/engine.rs` against the Task 3 commit), ship Tasks 1-3, and record the failure in the spec for a follow-up.

- [ ] **Step 5: Run the full suites**

Run: `cargo test -p rocket-infra -p rocket-app`
Expected: PASS. Every pre-existing engine test must still pass — the heap limit is far above anything they allocate, so the callback never fires for them.

- [ ] **Step 6: Run a fast compile check and commit**

```bash
cargo check -p rocket-infra
git add crates/rocket-infra/src/scripting/engine.rs
git commit -m "feat: cap script V8 heap at 256MB with a terminating near-OOM callback"
```

---

## Final Verification

- [ ] `cargo check` (whole workspace)
- [ ] `cargo test -p rocket-infra -p rocket-app` — spec acceptance criterion 6
- [ ] `git status` shows no stray files. In particular, confirm no `crates/rocket-infra/examples/` directory was left behind, and that `crates/rocket-infra/Cargo.toml` is **unmodified** — a diff there means a `v8` dependency was added by mistake.
- [ ] Confirm the whole change set is at most three files: `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-app/src/execution_service.rs`, and `CHANGELOG.md` if the project convention calls for an entry.

## Spec Coverage

| Spec item | Task |
|---|---|
| §3.1 timeout plus `terminate_execution` | Task 1 |
| §3.1 note — check `run_script_phase`'s `Err` branch | Resolved in the verification section above; the gap it revealed is closed by Task 3 |
| §3.2 memory limit, separate and lower-confidence | Task 4 |
| §4 non-goals (no user setting, no load-test special case) | Honoured — nothing in this plan touches settings or `rocket-http/src/load_test.rs` |
| §5 `deno_core::v8` path, no new dependency | Verified above; enforced by Global Constraints and the Final Verification `Cargo.toml` check |
| §6.1 `while(true){}` returns promptly, short injected timeout | Task 1, `infinite_loop_script_is_terminated_by_timeout` |
| §6.2 timeout surfaces as a script error | Task 3 |
| §6.3 fast script unaffected | Task 2, `fast_script_is_unaffected_by_the_timeout` |
| §6.4 no abort under repeated stress | Task 2, `repeated_timeouts_do_not_crash_or_wedge_the_engine` |
| §6.5 memory-hog script terminated without aborting | Task 4 |
| §6.6 `cargo test -p rocket-infra -p rocket-app` passes | Final Verification |
