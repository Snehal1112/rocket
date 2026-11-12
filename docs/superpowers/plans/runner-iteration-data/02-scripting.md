# Runner iteration data: scripting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scripts read the current row through `rok.runner.iterationData`, and `rok.runner.iterationIndex` and `totalIterations` return real values.

**Architecture:** `ScriptContext` gains an optional `IterationInfo`, set at the three phase sites from `ExecuteRequestInput.iteration`. The engine copies it into `ScriptInputState`, three read-only ops return it, and `bootstrap.js` builds a frozen `iterationData` object from them.

**Tech Stack:** Rust, `deno_core` ops, `bootstrap.js`, TypeScript typings in `rok-types.ts`.

**Spec:** `docs/superpowers/specs/2026-11-12-runner-iteration-data-design.md`

## Global Constraints

- `rok.runner.iterationData` exposes `get(key)` (string or `undefined`), `has(key)` (boolean) and `toObject()` (a plain copy). The object is read-only. The Bruno member names are confirmed in task 1 step 1 and this list changes if they differ.
- `iterationIndex` is zero-based. With no `iteration` on the context, `iterationIndex` is 0, `totalIterations` is 1 and `iterationData` is an empty read-only row (`get` returns `undefined`, `has` returns `false`, `toObject()` returns `{}`).
- The values are the same in every script phase of a send (pre-request, post-response, tests), including folder scripts, and in a nested `rok.runRequest`, which inherits its caller's `iteration` (plan 01).
- A script cannot change a row. Writing to `iterationData` throws in strict mode or is ignored, never changes a later read.

## Review Focus

- A script that sets `rok.runner.iterationData.toObject().x = 1` does not change the next `get('x')` (task 1 test).
- A key that is `undefined` in the row and a key set to `""` are told apart by `has` (task 1 test).
- A row value that looks like a number stays a string (task 1 test).
- A script in the tests phase sees the same row as the pre-request script of the same send (task 1 test).
- A second isolate for the next pass does not see the previous row (task 1 test).

---

### Task 1: Context, engine state, ops and `bootstrap.js`

**Files:**
- Modify: `crates/rocket-scripting/src/context.rs` (field and builder), `crates/rocket-infra/src/scripting/state.rs` (`ScriptInputState`), `crates/rocket-infra/src/scripting/engine.rs` (copy into state, register ops, replace the old default test), `crates/rocket-infra/src/scripting/ops/mod.rs` and a new `crates/rocket-infra/src/scripting/ops/iteration.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`
- Modify: `crates/rocket-app/src/execution_service.rs` (the three phase sites near lines 2907, 3329 and 3397)
- Test: `crates/rocket-infra/src/scripting/engine.rs` tests, one test per phase in `crates/rocket-app/src/execution_service.rs` tests

**Interfaces:**
- Consumes (plan 01): `rocket_scripting::IterationInfo`, `ExecuteRequestInput.iteration`.
- Produces:
  ```rust
  // ScriptContext: pub iteration: Option<IterationInfo>,  (None in every constructor)
  impl ScriptContext { pub fn with_iteration(mut self, iteration: Option<IterationInfo>) -> Self; }
  // ScriptInputState: pub iteration: Option<IterationInfo>,
  // ops/iteration.rs:
  pub fn op_rok_iteration_index(state: &OpState) -> u32;      // 0 when absent
  pub fn op_rok_iteration_total(state: &OpState) -> u32;      // 1 when absent
  pub fn op_rok_iteration_data(state: &OpState) -> String;    // JSON object text, "{}" when absent
  ```

- [ ] **Step 1: Check the Bruno names.** Fetch Bruno's JavaScript API reference for `bru.runner.iterationData` (use WebFetch on the Bruno docs) and write the member names you find into the ledger as a `Ruling:` line. If they differ from `get`, `has` and `toObject`, change the Global Constraints list above, the spec's "Script API" section and `rok-types.ts` in step 7 before going on, and tell the user.
- [ ] **Step 2: Write the failing engine tests** in `engine.rs` (replace `rok_runner_iteration_values_default_to_a_single_iteration`): `iteration_values_default_to_a_single_empty_iteration`, `iteration_index_total_and_row_come_from_the_context`, `iteration_data_get_returns_strings_and_undefined_for_missing_keys`, `iteration_data_has_tells_empty_string_from_missing`, `iteration_data_to_object_returns_a_copy_that_cannot_change_the_row`, `iteration_data_cannot_be_assigned`, `a_numeric_looking_value_stays_a_string`.
- [ ] **Step 3: Write the failing phase tests** in `execution_service.rs` tests: `pre_request_post_response_and_tests_scripts_see_the_same_iteration` (a recording engine double, as `every_script_phase_reports_runner_execution_mode` does) and `a_send_without_iteration_gives_the_default_to_every_phase`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-infra scripting::engine` and `cargo test -j4 -p rocket-app see_the_same_iteration`, and confirm the failures.
- [ ] **Step 5: Implement** the interfaces above. Register the three ops in the list at `engine.rs` near line 295 and copy `ctx.iteration` into `ScriptInputState` where it is built near line 500. At each of the three phase sites in `execution_service.rs`, chain `.with_iteration(input.iteration.clone())` after `.with_collection_name(...)`. Folder-chain scripts go through the same sites.
- [ ] **Step 6: Update `bootstrap.js`.** Replace the constants in the `runner` object with getters: `iterationIndex` and `totalIterations` call the ops. Add `iterationData` as a lazily built, frozen object whose `get`, `has` and `toObject` read a parsed copy of the op's JSON (parse once per script run, copy on each `toObject` call).
- [ ] **Step 7: Update `src/components/editor/rok-types.ts`** (the `runner` block near line 710): real doc comments for `iterationIndex` and `totalIterations`, and an `iterationData` member typed with `get(key: string): string | undefined`, `has(key: string): boolean` and `toObject(): Record<string, string>`. Update the matching test in `src/components/editor/` if one asserts the old comments (`bun run test rok-types`).
- [ ] **Step 8: Run** `cargo test -j4 -p rocket-infra scripting`, `cargo test -j4 -p rocket-app execution_service`, `cargo test -j4 -p rocket-scripting`, then `bun run tsc --noEmit` and `bun run check`. All pass.
- [ ] **Step 9: Commit** with the git-commit skill. Type `feat(scripting)`.

### Task 2: End-to-end check with the backend runner

**Files:**
- Modify: `crates/rocket-app/src/collection_runner_service.rs` (tests only)
- Test: the same file

**Interfaces:**
- Consumes: the row loop and `with_iteration_data` (plan 01 task 3), the ops from task 1.

- [ ] **Step 1: Write the failing test** `a_pre_request_script_reads_its_row_in_every_pass`: a three-row fake data file, a `ProgrammableEngine` (or the real engine if the harness can host it) whose script writes `rok.runner.iterationData.get('name')` and `rok.runner.iterationIndex` to a runtime variable, and assertions that pass 0, 1 and 2 saw `name` and the index of their own row.
- [ ] **Step 2: Write the failing test** `a_nested_run_request_sees_the_callers_row`, using the existing `run_request` test setup in `script_host_tests.rs` as the pattern.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app a_pre_request_script_reads_its_row_in_every_pass` and `cargo test -j4 -p rocket-app a_nested_run_request_sees_the_callers_row`. Fix whatever they find in task 1's wiring, then confirm both pass.
- [ ] **Step 4: Commit** with the git-commit skill. Type `test(runner)`.
