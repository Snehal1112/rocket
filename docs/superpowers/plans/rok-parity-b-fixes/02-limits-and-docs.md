# rok parity B fixes 02: host-call limits, roadmap and docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop a Safe-mode script from starting an unbounded number of host calls and nested runs, and bring the roadmap and series notes up to date.

**Architecture:** A per-script in-flight cap in `run_script_bounded` (rocket-infra), plus a per-send active-nested-run counter in rocket-app that is shared down the run chain and rejects instead of blocking.

**Tech Stack:** Rust, `tokio`, `FuturesUnordered`.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`; rulings in `00-plan-index.md`.

## Global Constraints

- Read `.claude/rules/rust-ddd-boundaries.md`, `crates/rocket-infra/CLAUDE.md` and `crates/rocket-app/CLAUDE.md`.
- Cargo commands always carry `-j4`; never `--workspace` or `--all`.
- Never unwrap in production paths.
- Commit through the `dev-workflow-skills:1-git-commit` skill, with a pathspec commit.

## Review Focus

- `Promise.all` over 200 `rok.sendRequest` calls: all 200 resolve, never more than 8 in flight.
- `Promise.all` over 30 `rok.runRequest` calls: the ones past the nested-run limit reject with a clear message, the others succeed, and the script ends well before the ceiling.
- A run that finishes releases its slot, also when it errors or is dropped.
- The wall-clock ceiling and busy-time budget still fire while calls are queued.

---

### Task 1: cap concurrent host calls and active nested runs

**Files:**
- Modify: `crates/rocket-infra/src/scripting/budget.rs` (`ScriptLimits`, line 15)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (`run_script_bounded`, lines 119-124)
- Modify: `crates/rocket-app/src/execution_service.rs` (`PhaseState`, `begin_phases`, `script_host`, `execute_nested`)
- Modify: `crates/rocket-app/src/execution_service/script_host.rs` (`ExecutionScriptHost`, `run_request`)
- Test: `crates/rocket-infra/src/scripting/engine.rs` (its test module), `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Produces:
  - `ScriptLimits.max_host_calls: usize`; `ScriptLimits::DEFAULT.max_host_calls == 8`. Every struct literal of `ScriptLimits` in tests must keep compiling (they use `..ScriptLimits::DEFAULT`).
  - `pub(crate) const MAX_ACTIVE_NESTED_RUNS: usize = 16;` in `script_host.rs`.
  - `PhaseState.active_runs: Arc<std::sync::atomic::AtomicUsize>`, a fresh counter at 0 in `begin_phases`; `ExecutionScriptHost.active_runs` is a clone of it; `execute_nested` takes it as a new parameter `active_runs: &Arc<AtomicUsize>` and stores it on the nested state.
  - A small RAII guard in `script_host.rs`, `struct ActiveRun(Arc<AtomicUsize>)`, whose `Drop` decrements the counter.

- [ ] **Step 1: Write the failing tests.**
  - In `engine.rs` tests, with a test `ScriptHost` that counts concurrent `send_request` calls (an `AtomicUsize` current and a max, each call sleeping 20 ms before returning) and `limits.max_host_calls = 3`: `host_calls_are_served_at_most_max_host_calls_at_a_time` runs `await Promise.all(Array.from({length: 12}, () => rok.sendRequest({url: 'http://x.test'})))`. Assert all 12 resolve and the host's observed maximum is 3.
  - `queued_host_calls_do_not_hide_the_ceiling`: `max_host_calls = 1`, a host call that never returns, ceiling 300 ms. Assert the run errors with the ceiling message, not a hang.
  - In `script_host_tests.rs`: `run_request_rejects_past_the_active_nested_run_limit`: a saved nested request whose own pre-request script does `await rok.sleep(200)`; the outer script runs `Promise.allSettled(Array.from({length: 30}, () => rok.runRequest('slow')))` and logs the number of rejections. Note that the per-script cap of 8 also applies, so set `max_host_calls` high enough in this test's limits (at least 30) to reach the nested-run limit. Assert exactly 14 reject (30 minus the 16 allowed), each with `rok.runRequest: too many nested requests running (limit 16)`.
  - `run_request_releases_its_slot_after_a_failed_nested_run`: call a nested request that errors 20 times in sequence. Assert none of them is rejected for the limit.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra host_calls_are_served_at_most`
Expected: FAIL (compile error for the missing field, then an observed maximum of 12).

- [ ] **Step 3: Implement.**
  - Add `max_host_calls` to `ScriptLimits` and the default 8, with a doc comment saying why (each in-flight `runRequest` holds a blocking thread and an isolate).
  - In `run_script_bounded`, add `&& serving.len() < limits.max_host_calls` to the `call_rx.recv()` branch guard. Unserved calls stay in the unbounded channel. Check that the `Some(()) = serving.next()` branch still wakes the loop so queued calls are picked up when a slot frees.
  - In `run_request`, before `execute_nested`: `fetch_add(1)` on `self.active_runs`; when the previous value is `>= MAX_ACTIVE_NESTED_RUNS`, `fetch_sub(1)` and return `HostError::Failed(format!("rok.runRequest: too many nested requests running (limit {MAX_ACTIVE_NESTED_RUNS})"))`. Otherwise wrap the counter in `ActiveRun` so every exit path releases the slot. Pass `&self.active_runs` to `execute_nested`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra scripting` then `cargo test -j4 -p rocket-app execution_service`
Expected: PASS (the review run showed 230 and 474 green before the change).

- [ ] **Step 5: Run `cargo check -j4` and `cargo clippy -j4` on both crates, then commit** (`fix(scripting): cap concurrent host calls and nested runs`).

---

### Task 2: roadmap and series notes

**Files:**
- Modify: `docs/roadmaps/roadmap.md` (section 1, rows R-01 to R-05, plus two new rows at the end of that table; the newest ID is R-65, so the new ones are R-66 and R-67, placed in section 1 even though the numbers are out of order)
- Modify: `docs/features/rok-api-parity-notes.md` (the "Series status" table, rows A, B, C, E)
- Modify: `crates/rocket-app/CLAUDE.md` and `crates/rocket-infra/CLAUDE.md` (one line each: the host-call cap, the nested-run limit, the guard on `sendRequest`, the unresolved-alias carry)

**Interfaces:** none.

- [ ] **Step 1: Update the roadmap rows.**
  - R-01: status `done (on main)`; "What remains": `getOauth2CredentialVar`, `resetOauth2Credential`, per-folder `__dirname`. Remove "Merge." and the branch reference.
  - R-02: status `done (on main; review fixes in plans/rok-parity-b-fixes/)`; where: `plans/rok-parity-b/`, `plans/rok-parity-b-fixes/`; "What remains": the follow-ups R-66 and R-67 below; `req.onFail` stays a no-op.
  - R-03: "What remains": `Write plan. Part B (R-02) is on main, so the dependency is met.`
  - R-05: keep as is.
  - R-66 (open, high priority): "Re-resolve the outer request after the pre-request script, so `await rok.runRequest('auth/login')` followed by a `{{token}}` header works. Today `begin_phases` resolves before scripts run, and nothing refreshes `PhaseState.var_ctx.env` after a script write. Until then the documented workaround is `req.setHeader`. Add a snippet showing it." Where: spec "Known limits", ruling 11 of `plans/rok-parity-b/00-plan-index.md`.
  - R-67 (open): "Part B review minors": collection load failure reported as "invalid request path" (`script_host.rs:121-125`); doubled `rok.sendRequest` prefix in failure lines (`ops/host.rs:78`); unsaved outer request gets 6 nested levels (`run_request.rs:43-52`); `.YML` extension not stripped (`run_request.rs:28-37`); repeated response headers collapse and binary bodies arrive lossy (`bootstrap.js` `_hostResponse`); object runtime variables reach nested runs as JSON strings; `test()` typed `: void` in `rok-types.ts`; document that nested runs report `getExecutionMode()` as standalone; `runRequest` reloads the collection tree on every call.

- [ ] **Step 2: Update the series notes table.** A: merged to main. B: implemented and on main; list the same follow-ups as R-66 and R-67 instead of the old list, keeping the old items still true (History badge, scanner flags). C: dependency on B is met. E: the gRPC dependency is now on main.

- [ ] **Step 3: Add the crate guidance lines.** In `crates/rocket-app/CLAUDE.md`: `rok.sendRequest` runs `check_request_guard`; nested runs carry `unresolved_aliases` and refuse a send that references one; at most 16 nested runs are active per send. In `crates/rocket-infra/CLAUDE.md`: `ScriptLimits.max_host_calls` bounds in-flight host calls per script.

- [ ] **Step 4: Check the docs.** Run `grep -n "R-02\|R-66\|R-67" docs/roadmaps/roadmap.md` and confirm the three rows exist and no row still says "Write plan, then build" for R-02.

- [ ] **Step 5: Commit** (`docs(scripting): mark rok parity B done and record the review follow-ups`).

---

## Next plan to execute

This is the last plan of the fixes. When Task 2 is complete and the ledger shows "Task 2: complete", run a final review of plans 01 and 02 together (scoped to the fix commits), then move to sub-project C (cookies) as described in `00-plan-index.md`.
