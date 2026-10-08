# rok parity B, plan 02: split time budget

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 5 s wall-clock script timeout with a split budget: 5 s of busy time that pauses while the script waits, a 60 s cap per `rok.sleep`, and a 5-minute wall-clock ceiling, all injectable for tests.

**Architecture:** `ScriptLimits` carries the three limits and lives on `DenoScriptEngine` and in `OpState`. A shared `BudgetClock` records busy time: the script thread marks every `execute_script` call and every event-loop poll as busy. The watchdog loop in `run_script_bounded` asks the pure `check` function whether to continue, and on a trip sets the clock's abort flag (which ends a waiting run) and terminates the isolate (which ends running JS).

**Tech Stack:** Rust, Tokio (`Notify`, `select!`), `deno_core` 0.400.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`, section 2. Index with rulings: `00-plan-index.md` (ruling 4 applies here). Requires plan 01.

## Global Constraints

- `cargo` commands always pass `-j4`. No `cargo test --workspace`.
- Default limits, copied from the spec: 5 s of script CPU time, `sleep` capped at 60 s per call, a 5-minute ceiling per script. "CPU time" is busy wall time on the script thread (ruling 4).
- Every timeout error message contains `timed out`, as the existing timeout tests and users expect.
- The "terminate whenever the isolate handle arrives" thread stays (the queued-script regression test depends on it).
- Lock poisoning is handled without panicking (`unwrap_or_else(|p| p.into_inner())`).
- Comments are short full sentences ending in a period.

## Review Focus

- A busy loop that runs while a host call is still in flight: it still dies at the busy-time budget.
- A script that awaits a host call slower than the busy-time budget: it succeeds.
- A single `rok.sleep` longer than the ceiling: the run ends at the ceiling, not after the sleep.
- After a ceiling abort, the next script on the same engine runs normally (no wedged thread).
- The existing timeout tests (`infinite_loop_script_is_terminated_by_timeout`, `queued_script_that_times_out_before_starting_is_still_terminated`, `repeated_timeouts_do_not_crash_or_wedge_the_engine`, `memory_hog_script_does_not_abort_the_process`) pass unchanged.

---

### Task 1: `ScriptLimits`, injectable limits and the sleep cap

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-infra/src/scripting/budget.rs`
- Modify: `crates/rocket-infra/src/scripting/mod.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (`DenoScriptEngine`, `SCRIPT_TIMEOUT`, `run_script_bounded`, `run_script_with_timeout`, `run_script`, `run_script_async`, tests)
- Modify: `crates/rocket-infra/src/scripting/ops/host.rs` (`op_rok_sleep`, `MAX_SLEEP_MS`)

**Interfaces:**
- Consumes: plan 01 (`run_script_bounded(ctx, host, timeout)`, `run_script(ctx, handle_tx, calls)`, `op_rok_sleep(ms)`).
- Produces: `pub struct ScriptLimits { pub cpu: Duration, pub ceiling: Duration, pub sleep_cap: Duration }` with `ScriptLimits::DEFAULT` and `Default`; `pub enum Verdict { Continue(Duration), CpuExceeded, CeilingExceeded }` with `Verdict::message(self, &ScriptLimits) -> String`; `pub fn check(limits: &ScriptLimits, busy: Duration, wall: Duration) -> Verdict`; `DenoScriptEngine::with_limits(limits: ScriptLimits) -> Self`; `run_script_bounded(ctx, host, limits: ScriptLimits)`; `run_script(ctx, handle_tx, calls, limits)`; `op_rok_sleep(state: Rc<RefCell<OpState>>, ms: f64)`. `crate::scripting::ScriptLimits` is re-exported.

- [ ] **Step 1: Write the budget module with its failing tests**

Create `crates/rocket-infra/src/scripting/budget.rs`:

```rust
//! Time limits for one script run.
//!
//! A script gets a budget of busy time: the time the script thread spends
//! running its code. Waiting for a host call or `rok.sleep` is not busy time,
//! so a script can wait on the network without using its budget. A wall-clock
//! ceiling still ends a script that waits forever, such as an endless polling loop.

use std::time::Duration;

/// The limits one script run gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLimits {
    /// Busy time the script may use.
    pub cpu: Duration,
    /// Wall-clock time the whole run may take, waiting included.
    pub ceiling: Duration,
    /// Longest single `rok.sleep`.
    pub sleep_cap: Duration,
}

impl ScriptLimits {
    /// The limits every real script run uses.
    ///
    /// Five seconds of busy time comfortably exceeds any legitimate script,
    /// which does templating, signing and small JSON work. Waiting does not
    /// count, so network calls get their own timeouts instead.
    pub const DEFAULT: ScriptLimits = ScriptLimits {
        cpu: Duration::from_secs(5),
        ceiling: Duration::from_secs(300),
        sleep_cap: Duration::from_secs(60),
    };
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the watchdog decides after reading the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Keep going, and look again after this long at the latest.
    Continue(Duration),
    /// The script used up its busy time.
    CpuExceeded,
    /// The run reached the wall-clock ceiling.
    CeilingExceeded,
}

impl Verdict {
    /// The error text for a verdict that ends the run.
    pub fn message(self, limits: &ScriptLimits) -> String {
        match self {
            Verdict::CpuExceeded => format!(
                "script execution timed out after {:?} of script time",
                limits.cpu
            ),
            Verdict::CeilingExceeded => format!(
                "script execution timed out: a script may run for at most {:?}",
                limits.ceiling
            ),
            Verdict::Continue(_) => String::new(),
        }
    }
}

/// Decides whether a run may continue, given its busy time and its wall-clock time.
pub fn check(limits: &ScriptLimits, busy: Duration, wall: Duration) -> Verdict {
    if busy >= limits.cpu {
        return Verdict::CpuExceeded;
    }
    if wall >= limits.ceiling {
        return Verdict::CeilingExceeded;
    }
    // Busy time grows no faster than wall time, so neither limit can pass before this.
    let next = (limits.cpu - busy).min(limits.ceiling - wall);
    Verdict::Continue(next.max(Duration::from_millis(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    const LIMITS: ScriptLimits = ScriptLimits {
        cpu: Duration::from_millis(100),
        ceiling: Duration::from_millis(1000),
        sleep_cap: Duration::from_millis(50),
    };

    #[test]
    fn default_limits_match_the_spec() {
        assert_eq!(ScriptLimits::DEFAULT.cpu, Duration::from_secs(5));
        assert_eq!(ScriptLimits::DEFAULT.ceiling, Duration::from_secs(300));
        assert_eq!(ScriptLimits::DEFAULT.sleep_cap, Duration::from_secs(60));
        assert_eq!(ScriptLimits::default(), ScriptLimits::DEFAULT);
    }

    #[test]
    fn check_continues_until_the_nearer_limit() {
        assert_eq!(check(&LIMITS, ms(30), ms(500)), Verdict::Continue(ms(70)));
        assert_eq!(check(&LIMITS, ms(0), ms(950)), Verdict::Continue(ms(50)));
    }

    #[test]
    fn check_stops_at_the_cpu_budget_first() {
        assert_eq!(check(&LIMITS, ms(100), ms(2000)), Verdict::CpuExceeded);
    }

    #[test]
    fn check_stops_at_the_ceiling() {
        assert_eq!(check(&LIMITS, ms(10), ms(1000)), Verdict::CeilingExceeded);
    }

    #[test]
    fn check_never_asks_for_a_zero_wait() {
        let almost = check(&LIMITS, ms(99) + Duration::from_micros(999), ms(0));
        assert_eq!(almost, Verdict::Continue(ms(1)));
    }

    #[test]
    fn stop_messages_say_timed_out() {
        assert!(Verdict::CpuExceeded.message(&LIMITS).contains("timed out"));
        assert!(Verdict::CpuExceeded.message(&LIMITS).contains("of script time"));
        assert!(Verdict::CeilingExceeded.message(&LIMITS).contains("timed out"));
        assert!(Verdict::CeilingExceeded.message(&LIMITS).contains("at most"));
    }
}
```

In `crates/rocket-infra/src/scripting/mod.rs`, add `pub mod budget;` as the first module line and add at the end:

```rust
pub use budget::ScriptLimits;
```

Run: `cargo test -j4 -p rocket-infra scripting::budget`
Expected: PASS (6 tests). The pure module needs no engine change to pass.

- [ ] **Step 2: Write the failing engine tests**

Add to the `tests` module in `engine.rs`, after the `sleep_` tests from plan 01:

```rust
    // ── limits ───────────────────────────────────────────────────────────────

    use crate::scripting::budget::ScriptLimits;

    #[tokio::test]
    async fn limits_cap_a_long_sleep() {
        let limits = ScriptLimits {
            sleep_cap: Duration::from_millis(50),
            ..ScriptLimits::DEFAULT
        };
        let ctx = minimal_ctx(
            "const t = Date.now(); await rok.sleep(10000); rok.setVar('short', Date.now() - t < 1000)",
        );
        let result = DenoScriptEngine::with_limits(limits)
            .execute(ctx)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("short").expect("short present"), true);
    }

    #[tokio::test]
    async fn limits_set_the_engine_cpu_budget() {
        let limits = ScriptLimits {
            cpu: Duration::from_millis(200),
            ..ScriptLimits::DEFAULT
        };
        let started = std::time::Instant::now();
        let outcome = DenoScriptEngine::with_limits(limits)
            .execute(minimal_ctx("while (true) {}"))
            .await;
        let err = outcome.expect_err("a busy loop must time out");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra limits_`
Expected: FAIL to compile (`no function or associated item named with_limits`).

- [ ] **Step 4: Thread the limits through the engine**

In `crates/rocket-infra/src/scripting/engine.rs`:

1. Add `use crate::scripting::budget::ScriptLimits;` to the `crate::scripting` imports.

2. Replace `pub struct DenoScriptEngine;` and its two `impl` blocks (`impl DenoScriptEngine` and `impl Default for DenoScriptEngine`) with:

```rust
pub struct DenoScriptEngine {
    limits: ScriptLimits,
}

impl DenoScriptEngine {
    pub fn new() -> Self {
        Self {
            limits: ScriptLimits::DEFAULT,
        }
    }

    /// An engine with other time limits. Tests use short ones.
    pub fn with_limits(limits: ScriptLimits) -> Self {
        Self { limits }
    }
}

impl Default for DenoScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}
```

3. Delete the `SCRIPT_TIMEOUT` constant and its doc comment (`ScriptLimits::DEFAULT` now carries that reasoning).

4. In `impl ScriptEngine for DenoScriptEngine`, replace both `SCRIPT_TIMEOUT` arguments with `self.limits`.

5. In `run_script_bounded`, change the parameter `timeout: Duration` to `limits: ScriptLimits`, change the spawn line to:

```rust
    let mut join =
        tokio::task::spawn_blocking(move || run_script(ctx, handle_tx, call_tx, limits));
```

change `let deadline = tokio::time::sleep(timeout);` to `let deadline = tokio::time::sleep(limits.cpu);`, and change the final error to:

```rust
    Err(DomainError::Internal(format!(
        "script execution timed out after {:?}",
        limits.cpu
    )))
```

6. Replace the test-only `run_script_with_timeout` with:

```rust
/// Runs a script with no host and a plain busy-time limit. The timeout tests use it.
#[cfg(test)]
async fn run_script_with_timeout(
    ctx: ScriptContext,
    timeout: Duration,
) -> DomainResult<ScriptResult> {
    let limits = ScriptLimits {
        cpu: timeout,
        ..ScriptLimits::DEFAULT
    };
    run_script_bounded(ctx, None, limits).await
}
```

7. Add `limits: ScriptLimits` as the last parameter of `run_script` and of `run_script_async`, pass it through in `tokio_rt.block_on(run_script_async(ctx, handle_tx, calls, limits))`, and in the "Seed OpState" block add after `state.put(HostChannel(calls));`:

```rust
        state.put(limits);
```

- [ ] **Step 5: Read the sleep cap from the limits**

In `crates/rocket-infra/src/scripting/ops/host.rs`, add `use crate::scripting::budget::ScriptLimits;` to the imports, delete `MAX_SLEEP_MS`, and replace `op_rok_sleep` with:

```rust
/// rok.sleep(ms) — waits without blocking the event loop. The value is clamped
/// to 0 and the run's sleep cap (60 s by default). The JS wrapper rejects
/// non-numbers first.
#[op2]
pub async fn op_rok_sleep(state: Rc<RefCell<OpState>>, ms: f64) {
    let cap = state
        .borrow()
        .try_borrow::<ScriptLimits>()
        .map(|limits| limits.sleep_cap)
        .unwrap_or(ScriptLimits::DEFAULT.sleep_cap);
    let cap_ms = cap.as_millis() as f64;
    let ms = if ms.is_nan() { 0.0 } else { ms.clamp(0.0, cap_ms) };
    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -j4 -p rocket-infra scripting`
Expected: PASS, including `limits_cap_a_long_sleep`, `limits_set_the_engine_cpu_budget`, the `sleep_` tests and every existing timeout test.

- [ ] **Step 7: Commit**

Run `cargo check -j4` first. Then use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of: `crates/rocket-infra/src/scripting/budget.rs`, `crates/rocket-infra/src/scripting/mod.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/ops/host.rs`.
Suggested subject: `feat(scripting): add injectable script limits and the sleep cap`.

---

### Task 2: Busy-time watchdog and the ceiling

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/scripting/budget.rs` (add `BudgetClock` and its tests)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (`run_script_bounded`, `run_script`, `run_script_async`, `run_user_code`, new `drive`, tests)

**Interfaces:**
- Consumes: Task 1 (`ScriptLimits`, `Verdict`, `check`).
- Produces: `pub struct BudgetClock` with `new() -> Arc<Self>`, `enter_busy(&self)`, `leave_busy(&self)`, `busy(&self) -> Duration`, `abort(&self)`, `is_aborted(&self) -> bool`, `async fn aborted(&self)`; `run_script(ctx, handle_tx, calls, limits, clock: Arc<BudgetClock>)`; `async fn run_user_code(runtime: &mut JsRuntime, code: &str, clock: &BudgetClock) -> DomainResult<Option<String>>`; `async fn drive<F: Future>(clock: &BudgetClock, fut: F) -> Option<F::Output>`. Later plans call `run_script_bounded(ctx, host, limits)` unchanged.

- [ ] **Step 1: Write the clock with its failing tests**

Add to `crates/rocket-infra/src/scripting/budget.rs`, below `check` (and add `use std::sync::atomic::{AtomicBool, Ordering};`, `use std::sync::{Arc, Mutex, MutexGuard};`, `use std::time::Instant;` and `use tokio::sync::Notify;` to the imports):

```rust
/// Busy time of one run, shared between the script thread and the watchdog.
pub struct BudgetClock {
    time: Mutex<BusyTime>,
    aborted: AtomicBool,
    wake: Notify,
}

#[derive(Default)]
struct BusyTime {
    total: Duration,
    since: Option<Instant>,
}

impl BudgetClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            time: Mutex::new(BusyTime::default()),
            aborted: AtomicBool::new(false),
            wake: Notify::new(),
        })
    }

    fn time(&self) -> MutexGuard<'_, BusyTime> {
        // The guarded data is two plain values that a panic cannot leave
        // half-written, so a poisoned lock is still safe to use.
        self.time.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Marks the start of a stretch of busy time.
    pub fn enter_busy(&self) {
        let mut time = self.time();
        if time.since.is_none() {
            time.since = Some(Instant::now());
        }
    }

    /// Marks the end of a stretch of busy time.
    pub fn leave_busy(&self) {
        let mut time = self.time();
        if let Some(since) = time.since.take() {
            time.total += since.elapsed();
        }
    }

    /// Busy time so far, counting a stretch that is still running.
    pub fn busy(&self) -> Duration {
        let time = self.time();
        time.total + time.since.map(|since| since.elapsed()).unwrap_or_default()
    }

    /// Asks the run to stop. Safe to call more than once.
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
        // A stored permit wakes a waiter that starts waiting later.
        self.wake.notify_one();
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    /// Resolves once `abort` was called.
    pub async fn aborted(&self) {
        if self.is_aborted() {
            return;
        }
        self.wake.notified().await;
    }
}
```

Add to the `tests` module of `budget.rs`:

```rust
    #[test]
    fn busy_time_counts_only_marked_stretches() {
        let clock = BudgetClock::new();
        clock.enter_busy();
        std::thread::sleep(ms(20));
        clock.leave_busy();
        std::thread::sleep(ms(40));
        let busy = clock.busy();
        assert!(busy >= ms(20) && busy < ms(40), "{busy:?}");
    }

    #[test]
    fn busy_time_includes_a_stretch_that_is_still_running() {
        let clock = BudgetClock::new();
        clock.enter_busy();
        std::thread::sleep(ms(20));
        assert!(clock.busy() >= ms(20));
    }

    #[test]
    fn a_second_enter_does_not_restart_the_stretch() {
        let clock = BudgetClock::new();
        clock.enter_busy();
        std::thread::sleep(ms(20));
        clock.enter_busy();
        clock.leave_busy();
        assert!(clock.busy() >= ms(20));
    }

    #[tokio::test]
    async fn aborted_resolves_when_abort_came_first() {
        let clock = BudgetClock::new();
        clock.abort();
        assert!(clock.is_aborted());
        tokio::time::timeout(ms(100), clock.aborted())
            .await
            .expect("aborted() resolves");
    }

    #[tokio::test]
    async fn aborted_wakes_a_waiting_task() {
        let clock = BudgetClock::new();
        let waiter = {
            let clock = Arc::clone(&clock);
            tokio::spawn(async move { clock.aborted().await })
        };
        tokio::time::sleep(ms(10)).await;
        clock.abort();
        tokio::time::timeout(ms(200), waiter)
            .await
            .expect("woken in time")
            .expect("task joined");
    }
```

Run: `cargo test -j4 -p rocket-infra scripting::budget`
Expected: PASS (11 tests).

- [ ] **Step 2: Write the failing engine tests**

Add to the `tests` module in `engine.rs`, after the `limits_` tests:

```rust
    fn short_limits(cpu_ms: u64, ceiling_ms: u64) -> ScriptLimits {
        ScriptLimits {
            cpu: Duration::from_millis(cpu_ms),
            ceiling: Duration::from_millis(ceiling_ms),
            sleep_cap: ScriptLimits::DEFAULT.sleep_cap,
        }
    }

    /// Host whose `send_request` takes a while, then answers 204.
    struct SlowHost(Duration);

    #[async_trait]
    impl ScriptHost for SlowHost {
        async fn send_request(&self, _request: HostRequest) -> Result<HostResponse, HostError> {
            tokio::time::sleep(self.0).await;
            Ok(HostResponse {
                status: 204,
                status_text: "No Content".into(),
                headers: vec![],
                body: String::new(),
                response_time_ms: 400,
            })
        }
    }

    #[tokio::test]
    async fn budget_sleeping_does_not_use_busy_time() {
        let ctx = minimal_ctx("await rok.sleep(400); rok.setVar('done', true)");
        let result = run_script_bounded(ctx, None, short_limits(200, 60_000))
            .await
            .expect("waiting is not busy time");
        assert_eq!(result.runtime_vars.get("done").expect("done present"), true);
    }

    #[tokio::test]
    async fn budget_waiting_on_the_host_does_not_use_busy_time() {
        let host = SlowHost(Duration::from_millis(400));
        let ctx = minimal_ctx(
            "const r = await rok.sendRequest({ url: 'https://x.test' }); rok.setVar('s', r.status)",
        );
        let result = run_script_bounded(ctx, Some(&host), short_limits(200, 60_000))
            .await
            .expect("waiting is not busy time");
        assert_eq!(result.runtime_vars.get("s").expect("s present"), 204);
    }

    #[tokio::test]
    async fn budget_a_busy_loop_after_an_await_still_dies() {
        let started = std::time::Instant::now();
        let ctx = minimal_ctx("await rok.sleep(10); while (true) {}");
        let err = run_script_bounded(ctx, None, short_limits(200, 60_000))
            .await
            .expect_err("a busy loop must time out");
        assert!(err.to_string().contains("of script time"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn budget_a_busy_loop_beside_a_slow_host_call_still_dies() {
        let host = SlowHost(Duration::from_secs(10));
        let started = std::time::Instant::now();
        let ctx = minimal_ctx("rok.sendRequest({ url: 'https://x.test' }); while (true) {}");
        let err = run_script_bounded(ctx, Some(&host), short_limits(200, 60_000))
            .await
            .expect_err("a busy loop must time out");
        assert!(err.to_string().contains("of script time"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn budget_the_ceiling_ends_an_endless_polling_loop() {
        let started = std::time::Instant::now();
        let ctx = minimal_ctx("while (true) { await rok.sleep(20); }");
        let err = run_script_bounded(ctx, None, short_limits(5_000, 300))
            .await
            .expect_err("the ceiling must end it");
        assert!(err.to_string().contains("at most"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn budget_the_ceiling_ends_a_single_long_sleep() {
        let started = std::time::Instant::now();
        let ctx = minimal_ctx("await rok.sleep(10000)");
        let err = run_script_bounded(ctx, None, short_limits(5_000, 300))
            .await
            .expect_err("the ceiling must end it");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2));

        // The engine is not wedged afterwards.
        let result = run_script_bounded(
            minimal_ctx("rok.setVar('alive', 'yes')"),
            None,
            short_limits(5_000, 300),
        )
        .await
        .expect("a later script still runs");
        assert_eq!(result.runtime_vars.get("alive").expect("alive present"), "yes");
    }
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra budget_`
Expected: FAIL. `budget_sleeping_does_not_use_busy_time` and `budget_waiting_on_the_host_does_not_use_busy_time` time out after 200 ms (the deadline is still wall clock), and the two ceiling tests run until the 5 s budget instead of 300 ms.

- [ ] **Step 4: Replace the watchdog**

In `crates/rocket-infra/src/scripting/engine.rs`:

1. Extend the imports: `use std::future::Future;`, `use std::sync::Arc;`, change `use std::time::Duration;` to `use std::time::{Duration, Instant};`, and change the budget import to `use crate::scripting::budget::{check, BudgetClock, ScriptLimits, Verdict};`.

2. Replace the whole `run_script_bounded` function (doc comment included) with:

```rust
/// Runs a script on a blocking thread, serves its host calls, and stops it when
/// it runs out of busy time or reaches the wall-clock ceiling.
///
/// Host calls arrive over a channel and are served here, on the caller's task,
/// because the host may borrow data that cannot move to the script thread.
/// Busy time is only the time the script thread spends running code, so a
/// script that waits on the host or on `rok.sleep` keeps its budget.
async fn run_script_bounded(
    ctx: ScriptContext,
    host: Option<&dyn ScriptHost>,
    limits: ScriptLimits,
) -> DomainResult<ScriptResult> {
    let started = Instant::now();
    let clock = BudgetClock::new();
    // JsRuntime is !Send, so all V8 work must stay on one thread.
    let (handle_tx, handle_rx) = oneshot::channel();
    let (call_tx, mut call_rx) = mpsc::unbounded_channel::<HostCall>();
    // Without a host the sender is dropped, so host ops find no channel and reject.
    let call_tx = host.map(|_| call_tx);
    let thread_clock = Arc::clone(&clock);
    let mut join = tokio::task::spawn_blocking(move || {
        run_script(ctx, handle_tx, call_tx, limits, thread_clock)
    });
    let mut serving = FuturesUnordered::new();

    let verdict = loop {
        let wait = match check(&limits, clock.busy(), started.elapsed()) {
            Verdict::Continue(wait) => wait,
            stop => break stop,
        };
        tokio::select! {
            joined = &mut join => {
                return joined
                    .map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?;
            }
            Some(call) = call_rx.recv(), if host.is_some() => {
                if let Some(host) = host {
                    serving.push(serve_host_call(host, call));
                }
            }
            Some(()) = serving.next(), if !serving.is_empty() => {}
            () = tokio::time::sleep(wait) => {}
        }
    };

    // The abort flag ends a run that is waiting on an op. Termination ends
    // JavaScript that is running. Neither is waited for here.
    clock.abort();
    // Terminate whenever the handle arrives, however late. Bounding
    // this wait would abandon a script that had not started yet: it
    // would then run unterminated and pin a blocking thread forever,
    // since dropping a spawn_blocking JoinHandle detaches rather than
    // cancels it.
    //
    // This must be a plain OS thread, not a `tokio::spawn`ed task: a
    // detached async task is tied to this call's Tokio runtime, and
    // on a short-lived runtime (every #[tokio::test] creates and
    // drops one per test) it can be cancelled before it ever gets
    // polled, deadlocking against the `spawn_blocking` thread that
    // Runtime::Drop waits on. A `std::thread` keeps running
    // regardless of what happens to the runtime that spawned it.
    std::thread::spawn(move || {
        if let Ok(isolate_handle) = handle_rx.blocking_recv() {
            isolate_handle.terminate_execution();
        }
    });
    Err(DomainError::Internal(verdict.message(&limits)))
}
```

3. Change `run_script` to take the clock and to skip a run that was aborted while queued:

```rust
fn run_script(
    ctx: ScriptContext,
    handle_tx: oneshot::Sender<v8::IsolateHandle>,
    calls: Option<mpsc::UnboundedSender<HostCall>>,
    limits: ScriptLimits,
    clock: Arc<BudgetClock>,
) -> DomainResult<ScriptResult> {
    // A run that was stopped while it waited for a thread never starts.
    if clock.is_aborted() {
        return Err(DomainError::Internal(
            "script execution timed out before it started".into(),
        ));
    }
    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| DomainError::Internal(format!("script runtime could not start: {e}")))?;
    tokio_rt.block_on(run_script_async(ctx, handle_tx, calls, limits, clock))
}
```

4. Add `clock: Arc<BudgetClock>,` as the last parameter of `run_script_async`. In its body replace:

```rust
    runtime
        .execute_script("<bootstrap>", BOOTSTRAP)
        .map_err(|e| DomainError::Internal(format!("bootstrap error: {e}")))?;

    // Capture script-level exceptions rather than propagating them as errors.
    let script_error = run_user_code(&mut runtime, &code).await;
```

with:

```rust
    clock.enter_busy();
    let booted = runtime.execute_script("<bootstrap>", BOOTSTRAP);
    clock.leave_busy();
    booted.map_err(|e| DomainError::Internal(format!("bootstrap error: {e}")))?;

    // Capture script-level exceptions rather than propagating them as errors.
    let script_error = run_user_code(&mut runtime, &code, &clock).await?;
```

5. Replace `run_user_code` with these two functions:

```rust
/// Runs the wrapped user code and the event loop, and returns the script error.
///
/// The loop runs until the script's promise settles and then until no work is
/// left, so callback-style work the script did not await still finishes. Every
/// stretch of running code counts as busy time. An aborted run returns `Err`.
async fn run_user_code(
    runtime: &mut JsRuntime,
    code: &str,
    clock: &BudgetClock,
) -> DomainResult<Option<String>> {
    let aborted = || DomainError::Internal("script execution timed out".into());

    clock.enter_busy();
    let started = runtime.execute_script("<user>", wrap_user_code(code));
    clock.leave_busy();
    let promise = match started {
        Ok(promise) => promise,
        Err(e) => return Ok(Some(script_error_message(e.to_string()))),
    };

    let resolve = runtime.resolve(promise);
    let settled = drive(
        clock,
        runtime.with_event_loop_promise(resolve, PollEventLoopOptions::default()),
    )
    .await
    .ok_or_else(aborted)?;
    if let Err(e) = settled {
        return Ok(Some(script_error_message(e.to_string())));
    }

    let drained = drive(clock, runtime.run_event_loop(PollEventLoopOptions::default()))
        .await
        .ok_or_else(aborted)?;
    Ok(drained.err().map(|e| script_error_message(e.to_string())))
}

/// Polls `fut` and counts each poll as busy time. Returns `None` once the run
/// is aborted, even while `fut` waits on an op.
async fn drive<F: Future>(clock: &BudgetClock, fut: F) -> Option<F::Output> {
    let mut fut = std::pin::pin!(fut);
    let tracked = std::future::poll_fn(|cx| {
        clock.enter_busy();
        let polled = fut.as_mut().poll(cx);
        clock.leave_busy();
        polled
    });
    tokio::select! {
        out = tracked => Some(out),
        () = clock.aborted() => None,
    }
}
```

- [ ] **Step 5: Run the budget tests**

Run: `cargo test -j4 -p rocket-infra budget_`
Expected: PASS (6 tests).

- [ ] **Step 6: Run the whole scripting suite**

Run: `cargo test -j4 -p rocket-infra scripting`
Expected: PASS. Pay attention to `queued_script_that_times_out_before_starting_is_still_terminated` (a queued script has no busy time, so it now starts, loops, trips the 200 ms budget and is terminated) and `memory_hog_script_does_not_abort_the_process`.

- [ ] **Step 7: Run the app-level script tests**

Run: `cargo test -j4 -p rocket-app execution_service && cargo test -j4 -p rocket-app flow_execution_service && cargo check -j4`
Expected: PASS.

- [ ] **Step 8: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of `crates/rocket-infra/src/scripting/budget.rs` and `crates/rocket-infra/src/scripting/engine.rs`.
Suggested subject: `feat(scripting): count busy time and add a five-minute ceiling`.

---

## Next plan to execute

When Task 2 is complete, its checks pass and the ledger (`.superpowers/sdd/rok-parity-b-02-split-budget/progress.md`) shows "Task 2: complete", **the executing Claude must go straight on to plan 03**: `docs/superpowers/plans/rok-parity-b/03-send-request.md`. No consent is needed between plans. Run one plan at a time, and swap the visible task list to plan 03's tasks when it starts.

Plan 03 depends on plans 01 and 02: it extends `op_rok_send_request`, the `sendRequest` wrapper and the engine tests' `FakeHost`. Do not start it on a tree where this plan's checks fail.
