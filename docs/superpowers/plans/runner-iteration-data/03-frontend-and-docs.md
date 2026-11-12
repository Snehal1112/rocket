# Runner iteration data: frontend and docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Runner tab runs the included requests once per data row, the user can pick a data file or use the collection's saved default, and the feature is documented.

**Architecture:** `startRun` in `pane-store.ts` asks `load_iteration_data` for the rows, then loops rows around the existing entries loop and passes `iteration` to each send. The data-file controls live in the Runner pane, and the saved default is a field in collection settings.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui, lucide-react, Vitest, Bun.

**Spec:** `docs/superpowers/specs/2026-11-12-runner-iteration-data-design.md`

## Global Constraints

- Use shadcn/ui primitives and `lucide-react` icons only. No raw `<button>`, `<input>`, `<select>` or `<dialog>`, no inline SVG.
- Zustand: narrow selectors, never destructure a whole store at the top of a component.
- Use the `frontend-design:frontend-design` skill for the new UI pieces, within the hard rules above.
- With no data file, `startRun` behaves exactly as before: one pass, no `iteration` on any send.
- The Stop button ends the whole run. The tab does not get `setNextRequest`, stop-on-failure or runtime-variable carry in this plan.
- The saved default is a relative path. The settings field shows the backend's error text for an invalid value and never blocks saving.
- Commands: `bun run tsc --noEmit`, `bun run check`, `bun run test <pattern>`. Never Yarn or npm.

## Review Focus

- Pressing Stop during pass 2 marks the rest of pass 2 and every later pass as skipped (task 1 test).
- A data file that fails to load shows the error and runs nothing (task 1 test).
- Excluded entries stay excluded in every pass (task 1 test).
- Re-run starts again from pass 1 with the same rows (task 1 test).
- A saved default that is invalid shows a warning in the Runner pane before Run (task 1 test).

---

### Task 1: Runner tab row loop and data-file controls

**Files:**
- Modify: `src/lib/tauri-api.ts` (types and wrappers), `src/lib/runner-execute.ts` (`iteration` parameter), `src/types/pane-types.ts` (`RunnerTab` fields), `src/stores/pane-store.ts` (`startRun`, `stopRun`, `rerunAll`), `src/components/request/runner/RunnerPane.tsx` (controls), `src/components/request/runner/RunnerResultsList.tsx` (group by pass), `src/components/request/runner/RunnerSummaryHeader.tsx` (pass counter)
- Test: `src/lib/__tests__/runner-execute.test.ts`, `src/stores/__tests__/` (the existing pane-store runner tests), `src/components/request/runner/RunnerPane.test.tsx`, `RunnerResultsList.test.tsx`

**Interfaces:**
- Consumes (plan 01): the `load_iteration_data` command and `IterationDataDto`; `ExecuteRequestInput.iteration`.
- Produces, in `tauri-api.ts`:
  ```ts
  export interface IterationInfo { index: number; total: number; data: Record<string, string> }
  export interface IterationData { columns: string[]; rows: Record<string, string>[]; source: 'perRun' | 'savedDefault'; fileName: string }
  export function loadIterationData(collection: string, perRunPath?: string): Promise<IterationData | null>;
  // ExecuteRequestInput gains: iteration?: IterationInfo;
  ```
  and in `runner-execute.ts`: `executeRunnerEntry(collection, requestPath, request, environmentName, graphql?, iteration?: IterationInfo)`.
- Produces, on `RunnerTab` in `pane-types.ts`:
  ```ts
  dataFilePath?: string;              // the per-run override the user picked
  iterationData?: IterationData;      // loaded when Run is pressed, kept for Re-run
  iterationIndex?: number;            // the pass that is running
  completedPasses?: { iteration: number; requests: RunnerEntry[] }[];
  dataFileError?: string;
  ```

- [ ] **Step 1: Write failing tests** for the store, named: `startRun_without_a_data_file_sends_no_iteration`, `startRun_runs_every_included_entry_once_per_row_in_order`, `startRun_passes_index_total_and_the_row_to_each_send`, `excluded_entries_stay_excluded_in_every_pass`, `stopRun_during_pass_two_skips_the_rest_of_the_run`, `a_load_error_shows_dataFileError_and_sends_nothing`, `rerunAll_starts_again_from_pass_one`. Use the pane-store test setup that already mocks `executeRunnerEntry` (follow `src/stores/__tests__/` runner tests).
- [ ] **Step 2: Write failing component tests**: `RunnerPane` shows a "Data file" row with a picker button and a "Use saved default" state; `RunnerPane` shows the row count and column names after a successful load; `RunnerPane` shows a warning when the saved default is invalid; `RunnerResultsList` groups results under "Pass 1 of 3" headings when `completedPasses` or `iterationData` is set and shows a plain list otherwise.
- [ ] **Step 3: Run** `bun run test runner` and confirm the new tests fail.
- [ ] **Step 4: Add the types and `loadIterationData`** to `tauri-api.ts` (invoke `load_iteration_data` with `{ collection, perRunPath }`), the `iteration` parameter to `executeRunnerEntry` (put it on `ExecuteRequestInput`), and the new `RunnerTab` fields.
- [ ] **Step 5: Implement the loop in `startRun`.** Before the first send call `loadIterationData(collectionName, found.tab.dataFilePath)`. On error set `dataFileError` and return without sending. With `null` run one pass with no `iteration`. Otherwise for each row run the existing included-entries loop, pass `{ index, total: rows.length, data: row }`, move the finished pass's `requests` into `completedPasses` and reset statuses to `pending` for the next pass. The run-id guard and `runState` checks run inside the inner loop and break both loops.
- [ ] **Step 6: Implement the pane controls.** A data-file row in `RunnerPane` with a shadcn `Button` (icon from `lucide-react`) that opens the file dialog (`open` from `@tauri-apps/plugin-dialog`, filters `csv` and `json`, the same plugin `src/lib/save-file.ts` uses), a clear button, and a status line (file name, row count, columns, or the error). Show the saved-default state from a `loadIterationData(collection)` call made when the collection is chosen.
- [ ] **Step 7: Implement the results grouping** in `RunnerResultsList` and the pass counter in `RunnerSummaryHeader` ("Pass 2 of 3").
- [ ] **Step 8: Run** `bun run test runner`, `bun run tsc --noEmit`, `bun run check`. All pass.
- [ ] **Step 9: Commit** with the git-commit skill. Type `feat(runner)`.

### Task 2: Collection settings field

**Files:**
- Modify: `src/lib/tauri-api.ts` (`CollectionSettings.runnerDataFile?: string`), `src/components/collections/CollectionOverviewTab.tsx` (field next to the script-flow control)
- Test: the existing test file for `CollectionOverviewTab` (or `src/components/collections/__tests__/`)

**Interfaces:**
- Consumes (plan 01 task 2): `CollectionSettings.runner_data_file` over IPC as `runnerDataFile`.

- [ ] **Step 1: Write failing tests**: `shows_the_saved_runner_data_file`, `editing_the_field_saves_a_trimmed_relative_path`, `clearing_the_field_saves_no_path`, `the_field_never_blocks_saving_when_the_path_looks_invalid` (shows a hint for a leading `/` or a `..` segment, the save still goes through), `the_hint_explains_the_path_is_relative_to_the_collection`.
- [ ] **Step 2: Implement** a shadcn `Input` labelled "Runner data file" with a short hint, wired to the existing settings save path the script-flow control uses.
- [ ] **Step 3: Run** `bun run test CollectionOverviewTab`, `bun run tsc --noEmit`, `bun run check`. All pass.
- [ ] **Step 4: Commit** with the git-commit skill. Type `feat(runner)`.

### Task 3: Docs, manual test and roadmap

**Files:**
- Create: `docs/features/runner-iteration-data.md`, `docs/manual-tests/runner-iteration-data.md`
- Modify: `crates/rocket-collection/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `crates/rocket-scripting/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md` (public types and the new file reader), `docs/roadmaps/roadmap.md` (R-04 row and the sections for any new follow-up rows), `docs/features/rok-api-parity-notes.md` (Part D row), `.claude/docs-index.md` (add the feature doc)

- [ ] **Step 1: Write the feature doc**: what it does, file formats and limits, the saved default key and its rules, the script API, which runner does what (Runner tab and `run_collection` differences, from the spec's decision table), and the known limits.
- [ ] **Step 2: Write the manual test file** with these checks: a CSV run of three rows; a JSON run; a bad file (wrong extension, empty, 1001 rows); the saved default after a pull on a second machine; Stop during pass two; a script reading `rok.runner.iterationData.get('name')`; a nested `rok.runRequest` in a data run.
- [ ] **Step 3: Update the roadmap and notes.** Set R-04 to `done` with the commit range, point it at the manual test, and add rows (new IDs at the end of their sections, never renumber) for the three follow-ups in the spec: `{{column}}` variables, row filters and delays, data files in load tests and Flow. Replace "Deferred" in the Part D row of `rok-api-parity-notes.md` with the shipped status, and fix the "Always 0 until data-driven runs exist" wording anywhere it remains.
- [ ] **Step 3b: Update the four crate `CLAUDE.md` files**: `rocket-collection` (parser, path rule, `DataFileReader` port, `runner_data_file`), `rocket-app` (`IterationDataService`, the `iteration` field and the runner loop), `rocket-scripting` (`IterationInfo`), `rocket-infra` (`FsDataFileReader`). Keep each entry to the format the file already uses.
- [ ] **Step 4: Check** that links resolve: `rg -n "runner-iteration-data" docs .claude` shows the new files referenced from the index and the roadmap.
- [ ] **Step 5: Commit** with the git-commit skill. Type `docs(runner)`.
