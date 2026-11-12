# Runner iteration data (R-04): plan index

**Spec:** `docs/superpowers/specs/2026-11-12-runner-iteration-data-design.md`
**Roadmap row:** R-04 in `docs/roadmaps/roadmap.md`. Series notes: `docs/features/rok-api-parity-notes.md` (Part D).
**Ledger:** one per plan, `.superpowers/sdd/runner-iteration-data-<plan file name without .md>/progress.md`. Create it on the plan's first task and log `Ruling:` lines there.

Run the plans in order. Each plan has at most 3 tasks and produces working, tested software on its own. When a plan is complete, move on to the next without asking.

| Plan | File | Tasks |
|---|---|---|
| 01 | `01-backend-core.md` | Data-file parser and the `iteration` field on `ExecuteRequestInput`; saved default, file reader and the `load_iteration_data` command; the row loop in `CollectionRunnerService` with its events |
| 02 | `02-scripting.md` | `ScriptContext` and engine state, `bootstrap.js` values, the Bruno member-name check, typings |
| 03 | `03-frontend-and-docs.md` | Runner tab row loop and data-file controls; collection settings field; feature doc, manual test, roadmap and parity notes |

## Findings that shaped the plans (read before starting)

- **The Runner tab does not use the backend runner.** `startRun` in `src/stores/pane-store.ts` loops the entries and calls `executeRunnerEntry` (`src/lib/runner-execute.ts`) once per request. Nothing in `src/` calls `run_collection`. `CollectionRunnerService` serves other callers and is kept correct, but the user-visible work is the tab loop in plan 03.
- **One seam carries the row.** `ExecuteRequestInput.iteration` (plan 01) is read when `ScriptContext` is built at the three phase sites in `crates/rocket-app/src/execution_service.rs` (plan 02). Both runners set it, and `ExecutionScriptHost::run_request` copies it to a nested input.
- **DDD placement (ruling, 2026-11-12).** Pure logic and ports go in domain crates: the parser, the saved-path rule, `IterationData` and the `DataFileReader` port live in `rocket-collection` (as D-86 moved `graphql_document` there). `rocket-infra` implements the port and defines no trait. `rocket-app` holds only `IterationDataService` and the runner loop, and does no file reads. The reader uses the workspace-following repository handle (R-52). Each crate's `CLAUDE.md` is updated in plan 03.
- **`IterationInfo` lives in `rocket-scripting`.** `rocket-app` already depends on that crate (it takes `ExecutionMode` from there), and `ScriptContext` is defined there, so the type cannot live in `rocket-app`.
- **Settings keys.** The existing Rocket extension keys are under `extensions.rocketapi` (not `extensions.rocket`). The new key is `extensions.rocketapi.runner.dataFile`.
- **Nothing reads `rok.runner.iterationData` before plan 02.** Plan 01 leaves the field present but unread, and the scripting constants stay 0 and 1.

## Rulings (read before starting)

1. A data file is parsed in Rust only. The frontend has no CSV or JSON parser.
2. A saved default is a relative path inside the collection directory. The pure part of that rule is `validate_data_file_path` in `rocket-collection`; the symlink and canonicalize checks are in `rocket-infra`. A per-run path is any file the user picked.
3. The saved value is validated when it is used, not when the collection loads. A bad value is kept unchanged on every settings save.
4. The row travels with each send. Limits are 10 MB and 1000 rows, so no per-request IPC call is needed.
5. Frontend commands use Bun (the user's standing rule overrides `yarn` in older docs): `bun run tsc --noEmit`, `bun run check`, `bun run test <pattern>`. Cargo commands pass `-j4`, per crate, never the whole workspace.
6. Commits use the `dev-workflow-skills:1-git-commit` skill with explicit paths and a pathspec commit. `git add -A` is never used.

## Next plan to execute

Plan 01 first. After plan 03, R-04 is done; the next items on the Part D list are the follow-ups at the end of the spec.
