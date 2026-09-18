# Spec: Script Execution Timeout and Memory Limit

**Status:** Draft
**Severity:** Low (but cheap, high-value fix)
**Roadmap:** [2026-09-16-scripting-security-roadmap.md](../plans/2026-09-16-scripting-security-roadmap.md), item 5
**Related:** independent of items 1/3/4/6 — can be implemented in any order relative to them.

## 1. Problem

No execution timeout, memory cap, or instruction limit exists anywhere in the script execution
path. Confirmed by direct inspection:

- `DenoScriptEngine::execute` (`crates/rocket-infra/src/scripting/engine.rs:28-37`) wraps
  `run_script` in a bare `tokio::task::spawn_blocking` with no `tokio::time::timeout` around the
  `.await`.
- `run_script` (`engine.rs:137-199`) constructs `JsRuntime::new(RuntimeOptions { extensions: ...,
  ..Default::default() })` — `..Default::default()` means no `create_params` (V8 heap limits), no
  interrupt/step budget.
- Every caller — `RequestExecutionService::run_script_phase` (`crates/rocket-app/src/execution_service.rs:351-384`,
  called for all three phases) and the load-test path — awaits `engine.execute(ctx)` with no
  surrounding deadline.

A malicious script (e.g. from an imported collection) or simply a buggy one (`while(true){}`, an
unbounded `for` loop building a huge string) can hang that request's execution thread indefinitely
or exhaust memory. Because `run_script` runs on a `spawn_blocking` OS thread, a hang there doesn't
block the async runtime itself, but it does hang that specific request forever from the user's
perspective, and leaks a thread for the life of the process (Tokio's blocking thread pool has a
cap — enough leaked hangs eventually exhaust it too).

## 2. Goal

Bound both wall-clock time and (best-effort) memory for a single script execution, surfacing a
timeout/OOM as an ordinary script error (`ScriptResult.error`, exactly like an uncaught JS
exception today) rather than hanging the request or crashing the app.

## 3. Design

### 3.1 Wall-clock timeout via `tokio::time::timeout` + V8 isolate termination

A `tokio::time::timeout` around the `spawn_blocking` `JoinHandle` alone is **not sufficient** —
cancelling the async future doesn't stop the OS thread actually running V8. The thread must be
told to stop via V8's own interrupt mechanism: `v8::IsolateHandle::terminate_execution()`, which
is `Send`/`Sync` and safe to call from a different thread than the one running the isolate (this
is the same mechanism Deno itself uses for `--v8-flags`-style external termination).

```rust
// crates/rocket-infra/src/scripting/engine.rs
const SCRIPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[async_trait]
impl ScriptEngine for DenoScriptEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        let (handle_tx, handle_rx) = tokio::sync::oneshot::channel();
        let join = tokio::task::spawn_blocking(move || run_script(ctx, handle_tx));

        match tokio::time::timeout(SCRIPT_TIMEOUT, join).await {
            Ok(join_result) => join_result
                .map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?,
            Err(_elapsed) => {
                // Best-effort: if run_script already sent its isolate handle, ask V8 to
                // abort the currently-running script. The blocking thread finishes shortly
                // after (run_script returns an Err from execute_script and still tears
                // down cleanly) but we don't wait for it — its result is discarded.
                if let Ok(isolate_handle) = handle_rx.try_recv() {
                    let _: v8::IsolateHandle = isolate_handle;
                    isolate_handle.terminate_execution();
                }
                Err(DomainError::Internal(format!(
                    "script execution timed out after {SCRIPT_TIMEOUT:?}"
                )))
            }
        }
    }
}

fn run_script(ctx: ScriptContext, handle_tx: tokio::sync::oneshot::Sender<v8::IsolateHandle>) -> DomainResult<ScriptResult> {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![rocket_scripting_ext::init()],
        ..Default::default()
    });

    // Send the isolate handle to the async side immediately, before running any
    // script code, so a timeout can always reach it. Ignore send failure (means
    // the async side already timed out and dropped the receiver — nothing to do).
    let _ = handle_tx.send(runtime.v8_isolate().thread_safe_handle());

    // ... rest of run_script unchanged (seed OpState, execute bootstrap + user code) ...
}
```

Note the timeout error path in `execute()` returns `Err(DomainError::Internal(...))` rather than
`Ok(ScriptResult { error: Some(...), .. })` — this matches `run_script`'s existing propagation
style (it returns `DomainResult<ScriptResult>`, and today's only `Err` path is the
`spawn_blocking` join-panic case at `engine.rs:34`). `RequestExecutionService::run_script_phase`
(`execution_service.rs:363` `match engine.execute(ctx).await { Ok(result) => ..., ??? }`) needs to
be checked at implementation time for how it currently handles the `Err` branch (verify: does it
already publish `DomainEvent::ScriptError` on `Err`, or only inspect `result.error` on `Ok`? If the
former, no further change needed there; if the latter, add an `Err(e) => { publish ScriptError with
e.to_string() }` arm so a timeout surfaces exactly like today's script errors do in the UI).

### 3.2 Memory limit — explicitly scoped as a smaller, separate, lower-confidence addition

V8 supports a hard heap limit via `RuntimeOptions.create_params` (`v8::CreateParams::default()
.heap_limits(initial, max)`), but setting a hard limit **without** also registering a
near-heap-limit callback causes V8's *default* out-of-memory behavior, which is to abort the
process — the opposite of the goal here. A correct implementation must register
`isolate.add_near_heap_limit_callback(...)` and have the callback call
`terminate_execution()` (the same handle as §3.1) instead of returning a larger limit.

Given the correctness risk of getting this wrong (an incorrectly-wired heap limit can crash the
whole app instead of protecting it) and that §3.1's timeout already bounds how much damage an
allocate-in-a-loop script can do (it can only run for `SCRIPT_TIMEOUT` before being terminated,
which caps allocation rate * time), this spec treats the memory limit as a **separate, optional
task** in the implementation plan, clearly marked lower-confidence, to be implemented and tested
in isolation with its own dedicated regression test (a script that allocates in a tight loop,
verifying the app does not abort and the request completes with a script error) before being
considered done. If time/risk tolerance doesn't allow it in this pass, §3.1 alone ships as the
primary fix — it is the one that directly closes the "hangs forever" finding from the audit.

## 4. Non-goals

- Not adding a user-configurable timeout setting in this pass — `SCRIPT_TIMEOUT` is a hardcoded
  constant (5 seconds, chosen to comfortably exceed any legitimate pre-request/post-response/test
  script's real-world running time — these scripts do in-memory templating, signing, and small
  JSON manipulation, not long-running I/O, since there is no network/fs access available to them at
  all per the sandbox design). A configurable setting is a reasonable follow-up, not required here.
- Not bounding load-test concurrent script execution specifically — per the prior audit, the
  current load-test firing path (`crates/rocket-http/src/load_test.rs`) does not invoke the script
  engine at all (confirmed: zero matches for "script" in that file), so this fix automatically
  covers load-test-triggered script execution the moment script support is added there, with no
  extra work — but does not need special-casing today.

## 5. Interfaces (for the implementation plan)

- `DenoScriptEngine::execute` — behavior change only (internal), public trait signature (`ScriptEngine::execute`) unchanged.
- New dependency check: confirm `v8` (the `rusty_v8`/`v8` crate re-exported or used transitively by `deno_core = "0.400.0"`) is already available for `IsolateHandle`/`CreateParams` types without adding a new direct dependency — `deno_core` typically re-exports `v8` as `deno_core::v8`; verify the exact path at implementation time (`deno_core::v8::IsolateHandle`, not a separate `v8` crate import) to avoid a version-mismatch dependency.
- `SCRIPT_TIMEOUT: std::time::Duration` — new constant in `crates/rocket-infra/src/scripting/engine.rs`.

## 6. Acceptance criteria

1. A script `while(true){}` (BeforeRequest phase) causes that request's `execute()` call to return
   within `SCRIPT_TIMEOUT + <small margin, e.g. 500ms>`, not hang indefinitely. Test with a
   shortened timeout constant injected for the test (or a `#[cfg(test)]`-only override) so the test
   suite doesn't take 5+ real seconds per run.
2. The timeout surfaces as a script error through the same path an uncaught exception would (via
   `DomainEvent::ScriptError` and/or `ScriptResult.error`, per whichever `run_script_phase` branch
   applies — confirm exact shape during implementation per §3.1's note).
3. A normal, fast script (e.g. `rok.setVar('x', 1)`) is completely unaffected — no latency
   regression, no change in `ScriptResult` for the non-timeout path.
4. The app process itself never aborts/crashes as a result of a timed-out script, under repeated
   stress (run several `while(true){}` scripts back to back in a test — thread pool must not be
   exhausted or leak unboundedly within a reasonable test window).
5. If the memory-limit sub-task (§3.2) is implemented: a script that allocates a large string in a
   loop (e.g. `let s = ''; while(true) { s += 'x'.repeat(1e6); }`) is terminated (either by hitting
   the heap limit and gracefully erroring, or by the wall-clock timeout firing first) without
   aborting the process.
6. `cargo test -p rocket-infra -p rocket-app` passes.
