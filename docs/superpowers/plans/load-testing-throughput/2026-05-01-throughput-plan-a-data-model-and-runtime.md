# Throughput-Target Phases — Plan A: Data Model + Runtime

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `LoadTestPhase` to support per-phase target type (`Concurrency(u32)` or `Rps(u32)`) and add a token-bucket-based rate driver inside `run_load_test_v2` so phases can target a sustained req/sec instead of concurrent users.

**Architecture:** Replace flat `target_concurrency: u32` with `target: PhaseTarget` enum that carries the unit. Validate at runtime entry that all phases share the same unit. For RPS configs, swap the per-phase semaphore-permit reshaping for a single tokio-`interval`-driven rate driver task that interpolates the target rate continuously between scheduler checkpoints, releases tokens at that rate, and unblocks the spawn loop. Concurrency configs keep today's behavior unchanged. `active_concurrent` becomes a uniform `AtomicU32` counter so both modes report it correctly.

**Tech Stack:** Rust, tokio (`time::interval`, `Notify`), serde (with backward-compat shim), wiremock for integration tests.

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md` (existing) — extends the phase model documented there.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-http/src/load_test.rs` | Add `PhaseTarget` enum, refactor `LoadTestPhase`, add `RateDriver`, branch `run_load_test_v2` on target unit, new tests |
| `crates/rocket-http/src/lib.rs` | Re-export `PhaseTarget` |
| `crates/rocket-http/CLAUDE.md` | Update module map + design rules |

---

## Chunk 1: PhaseTarget enum + backward-compat serde

### Task 1: Introduce `PhaseTarget` and refactor `LoadTestPhase`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Read the existing `LoadTestPhase` block** to confirm exact line locations.

```bash
grep -n "pub struct LoadTestPhase\|target_concurrency\|pub enum PhaseKind" crates/rocket-http/src/load_test.rs
```

Expected: `PhaseKind` enum near line 95, `LoadTestPhase` struct near line 103, `target_concurrency: u32` field inside it.

- [ ] **Step 2: Add the `PhaseTarget` enum**

In `crates/rocket-http/src/load_test.rs`, **above** the existing `LoadTestPhase` struct, add:

```rust
/// What a phase is steering toward: a number of concurrent in-flight requests,
/// or a target requests-per-second rate. All phases in a single config must
/// share the same variant — mixing units in one run is rejected at runtime.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum PhaseTarget {
    Concurrency(u32),
    Rps(u32),
}

impl PhaseTarget {
    pub fn value(self) -> u32 {
        match self {
            PhaseTarget::Concurrency(v) => v,
            PhaseTarget::Rps(v) => v,
        }
    }

    pub fn is_concurrency(self) -> bool {
        matches!(self, PhaseTarget::Concurrency(_))
    }

    pub fn is_rps(self) -> bool {
        matches!(self, PhaseTarget::Rps(_))
    }
}
```

- [ ] **Step 3: Refactor `LoadTestPhase` to use `target: PhaseTarget`**

Replace the existing struct with this version. `Serialize` is derived (always emits the new `target` shape); `Deserialize` is a manual impl that accepts either the new `target: { kind, value }` field or the legacy `targetConcurrency: number` field, so configs saved before this refactor still load:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestPhase {
    pub kind: PhaseKind,
    pub duration_secs: u32,
    pub target: PhaseTarget,
}

impl<'de> Deserialize<'de> for LoadTestPhase {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            kind: PhaseKind,
            duration_secs: u32,
            #[serde(default)]
            target: Option<PhaseTarget>,
            #[serde(default)]
            target_concurrency: Option<u32>,
        }
        let raw = Raw::deserialize(d)?;
        let target = match (raw.target, raw.target_concurrency) {
            (Some(t), _) => t,
            (None, Some(v)) => PhaseTarget::Concurrency(v),
            (None, None) => return Err(serde::de::Error::missing_field("target")),
        };
        Ok(LoadTestPhase {
            kind: raw.kind,
            duration_secs: raw.duration_secs,
            target,
        })
    }
}
```

This keeps `Serialize` derived (always emits the new `target` shape) while `Deserialize` accepts either old or new shape.

- [ ] **Step 4: Update every internal reference to `target_concurrency`**

Three call sites in this file currently read `phase.target_concurrency`. Update them:

**4a.** In `PhaseScheduler::checkpoints` (~line 59), change the line:
```rust
let end_conc = phase.target_concurrency;
```
to:
```rust
let end_conc = phase.target.value();
```

**4b.** In `LoadTestConfigV2::max_concurrency` (~line 139), change:
```rust
self.phases.iter().map(|p| p.target_concurrency).max().unwrap_or(1)
```
to:
```rust
self.phases.iter().map(|p| p.target.value()).max().unwrap_or(1)
```

**4c.** In `run_load_test_v2`'s initial-concurrency calculation (~line 344), change:
```rust
.map(|p| p.target_concurrency)
```
to:
```rust
.map(|p| p.target.value())
```

- [ ] **Step 5: Update every test in this file that constructs `LoadTestPhase`**

Run:
```bash
grep -n "target_concurrency:" crates/rocket-http/src/load_test.rs
```

Every match should be inside `#[cfg(test)] mod tests`. For each, replace `target_concurrency: N` with `target: PhaseTarget::Concurrency(N)`. The struct literal then becomes:

```rust
LoadTestPhase {
    kind: PhaseKind::Hold,
    duration_secs: 1,
    target: PhaseTarget::Concurrency(3),
}
```

- [ ] **Step 6: Add backward-compat serde test**

Inside the existing `#[cfg(test)] mod tests` block, add:

```rust
#[test]
fn legacy_target_concurrency_field_still_deserializes() {
    let json = r#"{
        "kind": "Hold",
        "durationSecs": 30,
        "targetConcurrency": 25
    }"#;
    let phase: LoadTestPhase = serde_json::from_str(json).unwrap();
    assert_eq!(phase.duration_secs, 30);
    assert_eq!(phase.target, PhaseTarget::Concurrency(25));
}

#[test]
fn new_target_field_deserializes() {
    let json = r#"{
        "kind": "Hold",
        "durationSecs": 30,
        "target": { "kind": "rps", "value": 200 }
    }"#;
    let phase: LoadTestPhase = serde_json::from_str(json).unwrap();
    assert_eq!(phase.target, PhaseTarget::Rps(200));
}

#[test]
fn missing_both_target_fields_fails() {
    let json = r#"{ "kind": "Hold", "durationSecs": 30 }"#;
    let res: Result<LoadTestPhase, _> = serde_json::from_str(json);
    assert!(res.is_err());
}

#[test]
fn phase_target_serializes_with_new_shape() {
    let phase = LoadTestPhase {
        kind: PhaseKind::Hold,
        duration_secs: 30,
        target: PhaseTarget::Concurrency(10),
    };
    let json = serde_json::to_string(&phase).unwrap();
    assert!(json.contains("\"target\":{\"kind\":\"concurrency\",\"value\":10}"));
    assert!(!json.contains("targetConcurrency"));
}
```

- [ ] **Step 7: Re-export `PhaseTarget` from `lib.rs`**

In `crates/rocket-http/src/lib.rs`, find the existing `pub use load_test::{...}` block and add `PhaseTarget` to the list (alphabetically between `LoadTestResult` and `PhaseKind`):

```rust
pub use load_test::{
    // ...existing...
    PhaseTarget,
    // ...existing...
};
```

- [ ] **Step 8: Run all rocket-http tests**

```bash
cargo test -p rocket-http 2>&1 | tail -25
```

Expected: every existing test still passes, plus the four new serde tests pass. 0 failures.

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-http/src/load_test.rs crates/rocket-http/src/lib.rs
git commit -m "refactor(rocket-http): replace LoadTestPhase.target_concurrency with PhaseTarget enum (concurrency | rps)"
```

---

## Chunk 2: Validation + uniform `active_concurrent` tracking

### Task 2: Reject mixed-unit configs and refactor active-concurrent tracking

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Add a config validation method**

Inside the `impl LoadTestConfigV2` block (~line 137), add:

```rust
    /// Returns `true` iff every phase uses the same `PhaseTarget` variant.
    /// Mixed-unit configs (some Concurrency, some Rps) are not supported.
    pub fn has_uniform_target_unit(&self) -> bool {
        let mut iter = self.phases.iter();
        let Some(first) = iter.next() else { return true };
        let first_is_rps = first.target.is_rps();
        iter.all(|p| p.target.is_rps() == first_is_rps)
    }

    /// Returns the target unit of the run, or `None` if there are no phases.
    pub fn target_unit(&self) -> Option<TargetUnit> {
        self.phases.first().map(|p| {
            if p.target.is_rps() { TargetUnit::Rps } else { TargetUnit::Concurrency }
        })
    }
```

Add the supporting enum just before `impl LoadTestConfigV2`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetUnit {
    Concurrency,
    Rps,
}
```

- [ ] **Step 2: Add validation tests**

Append to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn uniform_target_unit_all_concurrency() {
    let cfg = LoadTestConfigV2 {
        phases: vec![
            LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target: PhaseTarget::Concurrency(0) },
            LoadTestPhase { kind: PhaseKind::Hold,   duration_secs: 10, target: PhaseTarget::Concurrency(10) },
        ],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 100,
    };
    assert!(cfg.has_uniform_target_unit());
    assert_eq!(cfg.target_unit(), Some(TargetUnit::Concurrency));
}

#[test]
fn uniform_target_unit_all_rps() {
    let cfg = LoadTestConfigV2 {
        phases: vec![
            LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Rps(50) },
            LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Rps(100) },
        ],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 100,
    };
    assert!(cfg.has_uniform_target_unit());
    assert_eq!(cfg.target_unit(), Some(TargetUnit::Rps));
}

#[test]
fn uniform_target_unit_mixed_rejected() {
    let cfg = LoadTestConfigV2 {
        phases: vec![
            LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Concurrency(10) },
            LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Rps(50) },
        ],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 100,
    };
    assert!(!cfg.has_uniform_target_unit());
}
```

- [ ] **Step 3: Re-export `TargetUnit`**

In `crates/rocket-http/src/lib.rs`, add `TargetUnit` to the existing `load_test` re-export block.

- [ ] **Step 4: Run validation tests**

```bash
cargo test -p rocket-http uniform_target 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-http/src/load_test.rs crates/rocket-http/src/lib.rs
git commit -m "feat(rocket-http): add LoadTestConfigV2::has_uniform_target_unit / target_unit and TargetUnit enum"
```

---

## Chunk 3: RateDriver — token-bucket rate limiter

### Task 3: Implement `RateDriver`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Add the `RateDriver` struct**

After the `PhaseScheduler` block (~line 93), add:

```rust
use std::sync::atomic::AtomicU64;
use tokio::sync::Notify;

/// Token-bucket rate limiter that releases requests at a target req/sec rate.
///
/// The current rate is updated by a separate task that interpolates between
/// per-second checkpoints emitted by `PhaseScheduler`. The bucket capacity is
/// hard-coded to 250 ms worth of tokens at the *peak* configured rate, so a
/// quiet period cannot store up unlimited bursts.
///
/// `acquire()` blocks until the next token is available. Internally this
/// is driven by a `Notify` woken by a periodic refill task that ticks every
/// `min(50 ms, 1/rate)` and tops up the token count.
pub struct RateDriver {
    /// Current available tokens (millitokens — i.e. tokens × 1000 — for finer rates).
    millitokens: Arc<AtomicU64>,
    /// Bucket capacity in millitokens. Set once at construction from the peak rate.
    capacity_millitokens: u64,
    /// Wakes any `acquire()` waiters when tokens are added.
    notify: Arc<Notify>,
    /// Current rate in millitokens per millisecond (= rps × 1, since
    /// 1 token/sec = 1 millitoken/ms). Updated by the rate-update task.
    rate_per_ms: Arc<AtomicU64>,
}

impl RateDriver {
    /// Construct with an initial rate (req/sec). `peak_rate` sets the bucket
    /// capacity to 250 ms worth of tokens at that rate so the burst tolerance
    /// scales with the loudest phase.
    pub fn new(initial_rate_rps: u32, peak_rate_rps: u32) -> Self {
        let capacity_millitokens = (peak_rate_rps as u64) * 250; // 250 ms × peak_rps
        Self {
            millitokens: Arc::new(AtomicU64::new(0)),
            capacity_millitokens: capacity_millitokens.max(1000), // floor 1 token of slack
            notify: Arc::new(Notify::new()),
            rate_per_ms: Arc::new(AtomicU64::new(initial_rate_rps as u64)),
        }
    }

    /// Update the current rate. Called by the rate-interpolation task.
    pub fn set_rate(&self, rps: u32) {
        self.rate_per_ms.store(rps as u64, Ordering::Relaxed);
    }

    /// Block until one full token is available, then consume it.
    pub async fn acquire(&self) {
        loop {
            let current = self.millitokens.load(Ordering::Acquire);
            if current >= 1000 {
                // Try to claim one token.
                let want = current.saturating_sub(1000);
                if self
                    .millitokens
                    .compare_exchange(current, want, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return;
                }
                // CAS lost — retry.
                continue;
            }
            // Not enough — wait to be woken by the refill task.
            self.notify.notified().await;
        }
    }

    /// Refill loop. Adds `rate × elapsed_ms` millitokens every tick, capped
    /// at `capacity_millitokens`, then wakes one waiter. Runs until the
    /// returned `JoinHandle` is aborted.
    pub fn spawn_refill(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            const TICK_MS: u64 = 25;
            let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            ticker.tick().await; // skip immediate first tick
            loop {
                ticker.tick().await;
                let rate = me.rate_per_ms.load(Ordering::Relaxed);
                if rate == 0 {
                    continue;
                }
                let add = rate.saturating_mul(TICK_MS); // millitokens to add
                let mut current = me.millitokens.load(Ordering::Acquire);
                loop {
                    let next = (current + add).min(me.capacity_millitokens);
                    match me.millitokens.compare_exchange(
                        current,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => break,
                        Err(observed) => current = observed,
                    }
                }
                me.notify.notify_waiters();
            }
        })
    }
}
```

Note: the existing file already imports `Ordering` via `std::sync::atomic::{AtomicU32, Ordering}` at line 4 and `Duration` at line 6 — reuse them. `AtomicU64` and `Notify` are new and need to be added to the `use` block. Add them next to the existing atomic imports rather than inline `use` statements scattered through the file.

- [ ] **Step 2: Move the new `use` lines to the top of the file**

Open `crates/rocket-http/src/load_test.rs` and ensure the top imports include:

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::sync::{Notify, Semaphore};
```

Remove the inline `use std::sync::atomic::AtomicU64;` and `use tokio::sync::Notify;` lines that were inserted in Step 1.

- [ ] **Step 3: Add `RateDriver` unit tests**

Append to the `#[cfg(test)] mod tests` block:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rate_driver_holds_target_rate() {
    let driver = Arc::new(RateDriver::new(100, 100));
    let _refill = driver.spawn_refill();

    let start = std::time::Instant::now();
    let mut count = 0u32;
    while start.elapsed() < Duration::from_millis(500) {
        driver.acquire().await;
        count += 1;
    }
    // 100 rps × 0.5 s = 50, allow ±25% tolerance for scheduling jitter.
    assert!(count >= 35, "got only {count} requests in 500ms at 100rps");
    assert!(count <= 80, "got {count} requests in 500ms at 100rps (over-firing)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rate_driver_respects_zero_rate() {
    let driver = Arc::new(RateDriver::new(0, 100));
    let _refill = driver.spawn_refill();

    // With rate=0, a single acquire should never resolve. Race it against a
    // 200 ms timeout — the timeout must win.
    let acquired = tokio::time::timeout(Duration::from_millis(200), driver.acquire()).await;
    assert!(acquired.is_err(), "acquire returned at rate=0");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rate_driver_set_rate_updates_throughput() {
    let driver = Arc::new(RateDriver::new(10, 200));
    let _refill = driver.spawn_refill();

    // Drain ~5 tokens at 10 rps (~500 ms-worth of cap)
    for _ in 0..5 {
        driver.acquire().await;
    }
    driver.set_rate(200);

    let start = std::time::Instant::now();
    let mut count = 0u32;
    while start.elapsed() < Duration::from_millis(300) {
        driver.acquire().await;
        count += 1;
    }
    // 200 rps × 0.3 s = 60. Allow generous tolerance.
    assert!(count >= 30, "set_rate(200) yielded only {count} acquires in 300ms");
}
```

- [ ] **Step 4: Run RateDriver tests**

```bash
cargo test -p rocket-http rate_driver 2>&1 | tail -15
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-http/src/load_test.rs
git commit -m "feat(rocket-http): add RateDriver token-bucket rate limiter for RPS-target phases"
```

---

## Chunk 4: Branch `run_load_test_v2` on target unit

### Task 4: Wire `RateDriver` into `run_load_test_v2`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Add a `DomainResult` validation guard at function entry**

`run_load_test_v2` currently returns `LoadTestResult` infallibly. We need to reject mixed-unit configs without changing the public signature (callers in rocket-app would also need updates). Instead, the function clamps to "use first phase's unit; ignore mismatches" — and the rocket-app service layer (Plan B) does the actual validation before calling. To keep the runtime safe even if validation is bypassed, treat any non-uniform config as concurrency-mode using `target.value()`.

Document this at the function doc-comment. Replace the existing doc:

```rust
/// Phase-aware load test runner with optional Tauri event emission.
///
/// Drives either a `PhaseScheduler`-backed semaphore (for Concurrency phases)
/// or a `RateDriver` token bucket (for RPS phases). The unit is determined by
/// the first phase; callers MUST validate uniformity before calling — if they
/// don't, downstream phases with a different unit are reinterpreted as the
/// run's chosen unit (their numeric value is preserved). Validation lives in
/// `LoadTestConfigV2::has_uniform_target_unit`.
pub async fn run_load_test_v2(
```

- [ ] **Step 2: Branch on `target_unit`**

At the top of `run_load_test_v2`, after the `let scheduler = ...` line (~line 343), determine the run's unit:

```rust
    let target_unit = config.target_unit().unwrap_or(TargetUnit::Concurrency);
```

Then split the body. The cleanest way is to extract two helper functions and dispatch from the public function. But to keep the diff small, branch inline.

Replace the existing semaphore setup (lines 343–386 inclusive — the `Semaphore::new(initial_conc)` line through the end of the `phase_handle` task) with:

```rust
    // Shared accumulator + active counter — used by both target-unit paths.
    let active_concurrent = Arc::new(AtomicU32::new(0));
    let accumulator = Arc::new(tokio::sync::Mutex::new(RunAccumulator::new(
        config.ring_buffer_size,
    )));
    let success_rule = config.success_rule.clone();
    let run_start = std::time::Instant::now();

    // Concurrency-mode setup: semaphore + per-second permit reshaping.
    // Rps-mode setup: RateDriver + per-second rate updates.
    let semaphore: Option<Arc<Semaphore>> = match target_unit {
        TargetUnit::Concurrency => {
            let initial_conc = config
                .phases
                .first()
                .map(|p| p.target.value())
                .unwrap_or(1)
                .max(1) as usize;
            Some(Arc::new(Semaphore::new(initial_conc)))
        }
        TargetUnit::Rps => None,
    };

    let rate_driver: Option<Arc<RateDriver>> = match target_unit {
        TargetUnit::Rps => {
            let peak = config
                .phases
                .iter()
                .map(|p| p.target.value())
                .max()
                .unwrap_or(1);
            let initial = config.phases.first().map(|p| p.target.value()).unwrap_or(0);
            Some(Arc::new(RateDriver::new(initial, peak)))
        }
        TargetUnit::Concurrency => None,
    };

    // --- Phase-target updater task: depending on mode, either reshapes the
    //     semaphore permits or updates the rate driver's target rate. ---
    let checkpoints = scheduler.checkpoints();
    let phase_handle = match target_unit {
        TargetUnit::Concurrency => {
            let phase_sem = semaphore.as_ref().expect("semaphore present in concurrency mode").clone();
            let initial_conc = phase_sem.available_permits() as u32;
            let phase_perm = Arc::new(AtomicU32::new(initial_conc));
            tokio::spawn(async move {
                let phase_start = std::time::Instant::now();
                for (target_secs, target_conc) in checkpoints {
                    let now_secs = phase_start.elapsed().as_secs();
                    if target_secs > now_secs {
                        tokio::time::sleep(Duration::from_secs(target_secs - now_secs)).await;
                    }
                    let current = phase_perm.load(Ordering::SeqCst);
                    if target_conc > current {
                        phase_sem.add_permits((target_conc - current) as usize);
                        phase_perm.store(target_conc, Ordering::SeqCst);
                    } else if target_conc < current {
                        let to_remove = current - target_conc;
                        let sem = Arc::clone(&phase_sem);
                        let perm = Arc::clone(&phase_perm);
                        tokio::spawn(async move {
                            if let Ok(p) = sem.acquire_many(to_remove).await {
                                p.forget();
                                perm.fetch_sub(to_remove, Ordering::SeqCst);
                            }
                        });
                    }
                }
            })
        }
        TargetUnit::Rps => {
            let driver = Arc::clone(rate_driver.as_ref().expect("rate driver present in rps mode"));
            // Spawn refill task. Its handle is dropped here — the task lives
            // until we abort `phase_handle` (which doesn't kill it). To keep
            // cleanup simple, we spawn refill inside this same task and abort
            // both together.
            tokio::spawn(async move {
                let _refill = driver.spawn_refill();
                let phase_start = std::time::Instant::now();
                let mut last_rate = driver.rate_per_ms.load(Ordering::Relaxed) as u32;
                // checkpoints already linearly interpolate concurrency-style
                // values; for Rps we treat them the same — the value at
                // each checkpoint is the target rps for that second.
                for (target_secs, target_rps) in checkpoints {
                    let now_secs = phase_start.elapsed().as_secs();
                    if target_secs > now_secs {
                        tokio::time::sleep(Duration::from_secs(target_secs - now_secs)).await;
                    }
                    if target_rps != last_rate {
                        driver.set_rate(target_rps);
                        last_rate = target_rps;
                    }
                }
                // Hold final rate until the spawn loop finishes — keep the
                // refill task alive by not returning.
                std::future::pending::<()>().await;
            })
        }
    };
```

- [ ] **Step 3: Update the snapshot task and spawn loop to use the right gate**

The existing snapshot task (the 250-ms `interval` task) does not need to change — `active_concurrent` is still tracked the same way.

The spawn loop changes shape. Replace the existing block starting at `// --- Per-request spawning loop` through the `join_handles.push(handle);` `}` closing brace with:

```rust
    // --- Per-request spawning loop (duration-based) ---
    let total_duration = Duration::from_secs(config.total_duration_secs() as u64);
    let loop_start = std::time::Instant::now();
    let mut seq: u32 = 0;
    let mut join_handles = Vec::new();

    while loop_start.elapsed() < total_duration {
        // Gate spawn rate by the active mode.
        let owned_permit = match (&semaphore, &rate_driver) {
            (Some(sem), _) => Some(sem.clone().acquire_owned().await.unwrap()),
            (None, Some(driver)) => {
                driver.acquire().await;
                None
            }
            (None, None) => unreachable!("either semaphore or rate_driver must be set"),
        };

        // Recheck duration after gate (acquire may have blocked).
        if loop_start.elapsed() >= total_duration {
            drop(owned_permit);
            break;
        }

        let req = request.clone();
        let exec = Arc::clone(&executor);
        let acc = Arc::clone(&accumulator);
        let active = Arc::clone(&active_concurrent);
        let rule = success_rule.clone();
        let phase_idx = scheduler.phase_index_at(loop_start.elapsed().as_secs());
        let current_seq = seq;
        seq += 1;

        let handle = tokio::spawn(async move {
            active.fetch_add(1, Ordering::SeqCst);
            let t0 = std::time::Instant::now();
            let outcome = exec.execute(&req).await;
            let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let now = std::time::Instant::now();

            let mut acc = acc.lock().await;
            acc.completed += 1;
            acc.record_completion(now);
            match outcome {
                Ok(resp) => {
                    acc.latencies.push(latency_ms);
                    if resp.status < rule.status_below {
                        acc.succeeded += 1;
                    } else {
                        acc.failed_status += 1;
                    }
                    acc.log.push(RequestLogEntry {
                        seq: current_seq,
                        status: Some(resp.status),
                        latency_ms,
                        response_bytes: resp.size_bytes as u64,
                        error: None,
                        phase_index: phase_idx,
                    });
                }
                Err(e) => {
                    acc.failed_transport += 1;
                    acc.log.push(RequestLogEntry {
                        seq: current_seq,
                        status: None,
                        latency_ms: 0.0,
                        response_bytes: 0,
                        error: Some(e.to_string()),
                        phase_index: phase_idx,
                    });
                }
            }
            drop(acc);
            active.fetch_sub(1, Ordering::SeqCst);
            drop(owned_permit); // no-op when None
        });
        join_handles.push(handle);
    }
```

- [ ] **Step 4: Compile-check**

```bash
cargo check -p rocket-http 2>&1 | tail -20
```

Expected: clean. If you see `acquire_many returned () instead of SemaphorePermit`, you've put `acquire_many(to_remove).await` without `.unwrap()` — keep the existing pattern with `if let Ok(p) = ...`.

- [ ] **Step 5: Run all existing rocket-http tests to confirm no regression**

```bash
cargo test -p rocket-http 2>&1 | tail -25
```

Expected: every existing concurrency-mode test still passes.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-http/src/load_test.rs
git commit -m "feat(rocket-http): branch run_load_test_v2 on target unit (concurrency vs rps)"
```

---

## Chunk 5: RPS integration test

### Task 5: End-to-end test with RPS-mode phase

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Add an integration test using `MockExecutor`**

Append to the `#[cfg(test)] mod tests` block:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_load_test_v2_rps_mode_hits_target_rate() {
    // Hold 50 rps for 1 second; expect roughly 50 ± 20 completed requests.
    let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor); // 10ms latency
    let config = LoadTestConfigV2 {
        phases: vec![LoadTestPhase {
            kind: PhaseKind::Hold,
            duration_secs: 1,
            target: PhaseTarget::Rps(50),
        }],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 1000,
    };
    assert!(config.has_uniform_target_unit());
    assert_eq!(config.target_unit(), Some(TargetUnit::Rps));

    let result = run_load_test_v2(executor, &test_request(), &config, None).await;
    assert!(result.total_requests >= 30,
        "expected ≥30 requests at 50rps for 1s, got {}", result.total_requests);
    assert!(result.total_requests <= 75,
        "expected ≤75 requests at 50rps for 1s (over-firing), got {}", result.total_requests);
    assert_eq!(result.failed, 0);
    assert_eq!(result.succeeded, result.total_requests);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_load_test_v2_rps_zero_phase_emits_nothing() {
    let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
    let config = LoadTestConfigV2 {
        phases: vec![LoadTestPhase {
            kind: PhaseKind::Hold,
            duration_secs: 1,
            target: PhaseTarget::Rps(0),
        }],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 100,
    };
    let result = run_load_test_v2(executor, &test_request(), &config, None).await;
    assert_eq!(result.total_requests, 0);
}
```

- [ ] **Step 2: Run new tests**

```bash
cargo test -p rocket-http run_load_test_v2_rps 2>&1 | tail -15
```

Expected: 2 tests pass.

- [ ] **Step 3: Run full rocket-http suite once more**

```bash
cargo test -p rocket-http 2>&1 | tail -25
```

Expected: every test passes — old concurrency tests, RateDriver tests, validation tests, RPS integration tests.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-http/src/load_test.rs
git commit -m "test(rocket-http): integration tests for run_load_test_v2 RPS-mode rate targeting"
```

---

## Chunk 6: CLAUDE.md update

### Task 6: Document the new types and modes

**Files:**
- Modify: `crates/rocket-http/CLAUDE.md`

- [ ] **Step 1: Update the `load_test` row in the Module Map**

Find the `load_test` row in the Module Map table and replace it with:

```markdown
| `load_test` | Phase-based load testing harness. Each `LoadTestPhase` carries a `PhaseTarget` (either `Concurrency(N)` users or `Rps(N)` requests/sec); a single config must use one unit for all phases. `run_load_test_v2()` branches on the unit: concurrency mode uses a `Semaphore` whose permits are reshaped at phase boundaries, rps mode uses a `RateDriver` token bucket whose rate is updated continuously between checkpoints. Both modes share the same `RingBuffer<RequestLogEntry>`, snapshot task, and `LoadTestProgressEvent` shape. The legacy `run_load_test()` is kept for backwards compatibility with existing tests. |
```

- [ ] **Step 2: Add a Key Design Rules entry**

Append to the Key Design Rules section:

```markdown
- `LoadTestPhase` deserialization is backward-compatible: configs saved before the `PhaseTarget` refactor used `targetConcurrency: number`; the manual `Deserialize` impl accepts that legacy field and rewrites it to `PhaseTarget::Concurrency(value)`. New code should always emit the `target: { kind, value }` shape (Serialize is derived and always uses the new shape).
- Mixed-unit configs (some `Concurrency`, some `Rps` phases) are not supported. `LoadTestConfigV2::has_uniform_target_unit()` returns `false` for them; the service layer (`rocket-app`) is responsible for rejecting these before calling `run_load_test_v2`. If validation is bypassed, the runtime falls back to the first phase's unit and reinterprets later phases' values under that unit.
```

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-http/CLAUDE.md
git commit -m "docs(rocket-http): document PhaseTarget, RateDriver, and uniform-unit rule"
```

---

## Verification Gate

Before marking Plan A complete, run from the repo root:

```bash
cargo check 2>&1 | tail -10
cargo test -p rocket-http 2>&1 | tail -25
```

Expected: workspace compiles clean (rocket-app, src-tauri may fail because `LoadTestPhase.target_concurrency` field references in those crates haven't been updated yet — that's Plan B's job. If the only failures are `error[E0609]: no field 'target_concurrency'` in those crates, that is expected.).

Within `rocket-http`: every test passes.
