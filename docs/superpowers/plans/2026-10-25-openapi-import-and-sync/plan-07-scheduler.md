# OpenAPI Sync Scheduler Implementation Plan (Plan 7 of the OpenAPI series)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collections whose user turned on a sync interval are checked in the background while Rocket is open, with jitter, backoff and one catch-up check after launch. A check that finds changes tells the UI through an event. Nothing runs for a collection the user did not turn on, on this machine.

**Architecture:** Timing and the interval move from the committed `openapi-source.yml` to the git-ignored local state (`.rocket/openapi-local.json`), so a check no longer dirties git and a cloned collection never syncs by itself. A pure function decides when the next check is due from that local state, so a restart repeats no work and hammers no server. A supervisor in `rocket-app` ticks, runs due checks one at a time through `OpenApiSyncService::check`, and publishes an event. `src-tauri` starts it at launch, restarts it on a workspace switch and stops it on exit.

**Tech Stack:** Rust (`rocket-collection`, `rocket-app`, `rocket-shared` event, `src-tauri`), `tokio`, `chrono`.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md` ("Schedules", the open item on the scheduler design, the launch catch-up). Series index: `00-plan-index.md`, including the carry-forwards from the Plan 4 and Plan 6 final reviews.

## Global Constraints

- "Checks run only while the app is open, plus one catch-up check on launch when a check is overdue."
- "Checks run only for collections in the active workspace. The overdue test is per collection. The 'changes found' notice survives a restart until applied or dismissed." The notice is derived from `pending_sha256`, so the event is only a refresh signal.
- "One supervisor started at app start and stopped on exit and on workspace switch, jitter, backoff on 403 or 429 (respect `Retry-After`), and the catch-up check at launch."
- Intervals are the fixed list `Off`, `Hourly`, `Daily`, `Weekly`, `Monthly`; the default is off.
- A failed check keeps the last good copy and applies nothing (Plan 4 rule). The scheduler never applies.
- Persisted structs use snake_case keys with no serde renaming; new fields have `#[serde(default)]`. Never unwrap errors in production paths. Cargo uses `-j4` and `-p <crate>`, never `--workspace`.
- The `DomainEvent` wire format is pinned by contract tests in `crates/rocket-shared/src/events.rs`: the tag is camelCase, variant fields are snake_case. A new variant gets a contract test there.
- Log lines carry counts and codes only, never a URL, a path or a token.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. This plan is Rust only, so the commit gate (`cargo check`) works.
- Tasks that touch `rocket-collection`, `rocket-infra` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Rulings made while planning (open to your objection at plan review)

1. **The interval is local to this machine.** `sync_interval`, `last_checked_at` and `last_status` move into the local state. A committed `openapi-source.yml` can no longer switch on background requests on someone else's machine, which matters because the scheduler is the first caller of `check` that no person triggers (the Plan 4 token carry-forward). A source file written before this plan keeps its old timing as a read-only fallback for `last_checked_at` and `last_status`; its old interval is ignored (the feature is unreleased).
2. **Failure state is stored, not kept in memory.** The local state also holds `consecutive_failures` and `not_before`. The next due time is computed from stored values only, so a restart cannot start a retry storm.
3. **After a failure the schedule is the backoff, not the interval.** The delay is 15 minutes, doubled for each further failure, capped at 6 hours and at the interval. `Retry-After` on a rate-limit answer raises the delay to at least that value (at least 60 seconds, at most 24 hours). Permanent failures (`not_found`, `unauthorized`, `not_a_spec`, `too_large`, `not_https`) use the same ladder and the UI shows the reason; they are not retried faster.
4. **Jitter is deterministic.** The due time gains a stable offset from the source id: up to 10% of the interval, at most 5 minutes. Many collections on one interval therefore do not fire together, and a restart does not move the offset.
5. **Launch catch-up.** A due check runs about 30 seconds after the supervisor starts (so startup stays quiet), at most one check at a time, and at most 5 per tick, so a long absence does not start a burst.
6. **No new workspace-wide state.** The supervisor lists the collections of the pinned active workspace each tick. A workspace switch stops the old supervisor and starts a new one; a check already in flight finishes against its own collection path and is not cancelled.

## Review Focus

- A collection with the interval off, or one with no local state, is never checked, including one whose committed source file says hourly (Tasks 1 and 2).
- After a failed check and an app restart, the next attempt follows the stored backoff, not an immediate retry and not a full interval (Tasks 1 and 2).
- A tick never blocks the UI or runs two checks at once, and a stop request ends the loop even during a long wait (Tasks 2 and 3).
- A check never overwrites the apply's work: timing writes go through the same local-state save as skips, under the collection lock, and an apply keeps the timing fields (Tasks 1 and 2).
- A system sleep or a clock change does not make every collection fire at once and does not stop checks for good (Task 2).

---

### Task 1: Move interval and timing into the local state

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/openapi_local.rs` (new fields), `crates/rocket-collection/src/openapi_source.rs` (legacy fields become read-only: `skip_serializing`).
- Modify: `crates/rocket-app/src/openapi_sync_service.rs` (`check`, `fail`, `status`, `set_interval`), `crates/rocket-app/src/openapi_sync_apply.rs` (the local repository dependency for the service without apply deps), `crates/rocket-import/src/openapi/sync/apply.rs` only if `build_apply` rebuilds the local state without copying the new fields.
- Create: `crates/rocket-collection/src/openapi_schedule.rs` (pure schedule rules, declared and re-exported in `lib.rs`).
- Modify: `crates/rocket-import/CLAUDE.md` or `crates/rocket-app/CLAUDE.md` (a short "Sync timing" section).

**Interfaces:**
- Produces:
  - `OpenApiLocalState` gains `sync_interval: SyncInterval`, `last_checked_at: Option<String>`, `last_status: Option<SyncStatus>`, `consecutive_failures: u32`, `not_before: Option<String>`, all `#[serde(default)]`; the file version stays 1. Fields at their default are not written.
  - `pub fn next_check_at(plan: &SchedulePlan) -> Option<DateTime<Utc>>` and `pub struct SchedulePlan<'a> { pub interval: SyncInterval, pub source_id: &'a str, pub last_checked_at: Option<&'a str>, pub last_failed: bool, pub consecutive_failures: u32, pub not_before: Option<&'a str> }`. `None` when the interval is off. Rules: not failed gives last check plus interval plus jitter; failed gives last check plus the backoff delay (ruling 3), never earlier than `not_before`; no last check, or an unreadable one, is due at once (jitter does not apply); a last check in the future counts as unreadable. `pub fn backoff_secs(failures: u32, interval: SyncInterval, retry_after_secs: Option<u64>) -> u64` and `pub fn jitter_secs(source_id: &str, interval: SyncInterval) -> u64`.
  - `OpenApiSyncService::check` records its timing in the local state (through a `OpenApiLocalRepository` the service now always has; the apply deps share it) and no longer writes `last_checked_at`/`last_status` to the source file. On `Failed` it increments `consecutive_failures` and sets `not_before` from `retry_after_secs`; on success it resets both.
  - `set_interval` writes the local state. `status()` reads interval and timing from the local state with the source file as a fallback for `last_checked_at` and `last_status` only.
  - `OpenApiSyncService::new` keeps its signature; a builder method `with_local(Box<dyn OpenApiLocalRepository>)` is added and `make_sync_service` in `src-tauri` uses it.

- [ ] **Step 1: Write the failing tests.**
  - `next_check_at`: `off_is_never_due`; `no_last_check_is_due_now`; `unreadable_or_future_last_check_is_due_now`; `healthy_check_waits_interval_plus_jitter`; `jitter_is_stable_and_below_the_cap` (10% of the interval, at most 300 s, different ids differ); `failure_follows_the_backoff_ladder` (15 min, 30 min, 1 h, 2 h, 4 h, then 6 h, and never above the interval); `retry_after_raises_the_delay_within_its_bounds` (below 60 s becomes 60 s, above 24 h becomes 24 h); `not_before_is_a_floor`.
  - Service (`#[tokio::test]` with the existing fakes): `check_writes_timing_to_the_local_state_not_the_source` (source file bytes unchanged after an unchanged check); `failure_counts_and_success_resets`; `rate_limit_sets_not_before`; `set_interval_writes_the_local_state`; `status_falls_back_to_legacy_source_timing`; `a_committed_hourly_source_does_not_turn_the_interval_on`; `apply_keeps_the_timing_fields` (apply a change, then read the local state: interval and timing intact).
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-collection openapi_schedule` and `cargo test -j4 -p rocket-app openapi_sync`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. The local state is saved under the collection lock like every other sync write; `check` writes it in the same locked section as its other fields.
- [ ] **Step 4: Run to verify they pass.** Run both commands, `cargo test -j4 -p rocket-import openapi`, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): keep sync timing in the local state`.

---

### Task 2: Schedule rules and the supervisor

**Files:**
- Create: `crates/rocket-app/src/openapi_sync_scheduler.rs` (declared and exported in `lib.rs`).
- Modify: `crates/rocket-shared/src/events.rs` (new variant and contract test), `crates/rocket-app/src/openapi_sync_service.rs` (a `due_targets` method).
- Modify: `crates/rocket-app/CLAUDE.md` (a row and a short section).

**Interfaces:**
- Consumes: Task 1 `next_check_at`, the local state, `OpenApiSyncService::check`; `ApplyDeps.collections` (`pinned()`, `list`, `collection_root_path`) for the collection list; `EventPublisher`.
- Produces:
  - `DomainEvent::OpenApiSyncChecked { collection: String, status: String }` where `status` is `unchanged`, `changed` or `failed`. It tells the UI to reload the status; it carries no URL and no message.
  - `OpenApiSyncService::due_targets(&self, now: DateTime<Utc>) -> Vec<SyncTarget>` with `pub struct SyncTarget { pub collection: String, pub root: PathBuf }`: the collections of the pinned active workspace that have a source file, an interval other than off, and `next_check_at <= now`, oldest due first. A collection whose source or local file cannot be read is skipped and counted in a debug log. Cost per tick is one local-state read per collection.
  - `pub struct SchedulerConfig { pub tick: Duration, pub launch_delay: Duration, pub max_checks_per_tick: usize }` with defaults 60 s, 30 s and 5.
  - `pub struct OpenApiSyncScheduler` built with `new(service_factory: Arc<dyn Fn() -> OpenApiSyncService + Send + Sync>, events: Arc<dyn EventPublisher>, config: SchedulerConfig)` and `with_clock`; `pub fn start(self) -> SchedulerHandle`; `SchedulerHandle::stop(&self)` ends the loop (also on drop) even while it sleeps, and does not cancel a check already running. One loop, one check at a time, a failure of one collection never stops the others. After each check it publishes `OpenApiSyncChecked`.

- [ ] **Step 1: Write the failing tests** (`#[tokio::test(start_paused = true)]` for timing, a fake service factory over temp collections, a recording `EventPublisher`):
  - `due_targets`: `lists_only_collections_with_an_interval_that_are_due`; `a_collection_without_local_state_is_not_listed`; `oldest_due_first`; `an_unreadable_local_file_is_skipped_not_fatal`.
  - Scheduler: `first_check_waits_for_the_launch_delay`; `checks_run_one_at_a_time_and_at_most_the_per_tick_limit`; `a_failed_check_is_not_retried_before_its_backoff_even_after_a_restart` (build a second scheduler over the same temp collection); `a_changed_result_publishes_the_event_with_status_changed`; `stop_ends_the_loop_during_a_sleep`; `stop_does_not_cancel_a_running_check`; `a_check_error_for_one_collection_does_not_stop_the_next`; `after_a_long_pause_checks_run_in_batches_not_all_at_once` (set the clock one month ahead with 12 due collections, assert 5, 5, 2 over three ticks).
  - Event contract test in `events.rs`: tag `openApiSyncChecked`, fields `collection` and `status`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-app openapi_sync_scheduler` and `cargo test -j4 -p rocket-shared events`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. Use `tokio::time::sleep` raced with a stop signal (`tokio::sync::Notify` or a watch channel); take the clock from the injected closure so tests do not sleep. Run `due_targets` and each `check` so that no std lock guard crosses an `.await` (the Plan 6 rule), moving the blocking parts to `spawn_blocking` where they read many files.
- [ ] **Step 4: Run to verify they pass.** Run both commands, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): add the OpenAPI sync supervisor`.

---

### Task 3: Start, restart and stop the supervisor

**Files:**
- Modify: `src-tauri/src/lib.rs` (start after the sync service wiring, stop on `RunEvent::Exit`), `src-tauri/src/commands/workspaces.rs` (`switch_workspace` restarts it), `src-tauri/src/commands/import.rs` (a shared holder and `start_sync_scheduler`).
- Modify: `docs/roadmaps/roadmap.md` (R-70), `crates/rocket-app/CLAUDE.md`.

**Interfaces:**
- Consumes: Task 2 scheduler, `make_sync_service`, the app's `TauriEventBus`.
- Produces:
  - `pub(crate) struct SyncSchedulerSlot(Mutex<Option<SchedulerHandle>>)` managed in Tauri state, with `replace(handle)` that stops the previous handle first and `stop()`.
  - `pub(crate) fn start_sync_scheduler(app: &AppHandle, active_workspace_path, fetcher)` builds the factory from `make_sync_service`, uses `TauriEventBus` as the publisher, starts the scheduler and stores the handle in the slot.
  - `switch_workspace` calls `start_sync_scheduler` right after the workspace is switched and after the recovery call. `RunEvent::Exit` calls `slot.stop()`.

- [ ] **Step 1: Write the failing tests.** Unit tests for the slot (`replace_stops_the_previous_handle`, `stop_is_idempotent`) with a counting handle. The Tauri wiring itself has no unit test; the manual check below covers it.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket sync_scheduler_slot`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. Recovery (`recover_openapi_syncs`) must finish before the first check: start the scheduler from the same background task, after `recover_all`, or rely on `settle` inside `check` (it already recovers first); say which in the report.
- [ ] **Step 4: Run to verify they pass.** Run the command, then `cargo check -j4`. Expected: all pass, no warnings. Then update R-70 and the plan index (P7 row done, with a "Carry-forward from the Plan 7 final review" section if the review leaves items).
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): run the OpenAPI sync supervisor in the app`.
- **Manual check (for you, not a test):** run a built app with a collection whose interval is set to hourly through a temporary command or by writing `.rocket/openapi-local.json` by hand with `last_checked_at` two hours old; within about a minute the file's `last_checked_at` updates, and with the network off it shows `consecutive_failures` and a `not_before` in the future. Switch workspaces and confirm checks stop for the old one.

---

## Self-review notes

- Spec coverage: supervisor started at launch and stopped on exit and switch (Task 3); jitter (Task 1 rule, Task 2 use); backoff and `Retry-After` (Task 1); launch catch-up (Task 2 launch delay plus the due-at-once rule); per-collection overdue test, active workspace only (Task 2 `due_targets`); notice survives restart (derived from `pending_sha256`, event only refreshes).
- Carry-forward covered: move check timing out of the committed source (ruling 1, Task 1); the Plan 4 token decision is narrowed, not closed. The scheduler checks only collections the user opted in on this machine, but a manual `check` from Plan 9 still needs the trust gate or the location-bound token. Plan 9 keeps that item.
- Not here: "Check now" and the interval picker (Plan 9), the Spec sync tab and notice UI (Plans 8 and 9), a partial-apply marker for the revert case (listed in the Plan 6 carry-forward; Plan 9 or a later fix).
- Known limits: a check in flight when the app exits is dropped; a monthly interval is 30 days; there is no per-host rate limit shared across collections beyond the one-check-at-a-time rule.
