# Runner iteration data (R-04, "Part D")

Date: 2026-11-12. Status: design approved in chat, spec awaiting review. Revised the same day after finding that the Runner tab runs a frontend loop and does not call the backend runner (see the "Which runner" decision).

> Before starting any task from this spec that touches `.yml` files or collection settings, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Goal

Run a collection or folder once per row of a CSV or JSON data file. Scripts read the current row through `rok.runner.iterationData`, and `rok.runner.iterationIndex` and `rok.runner.totalIterations` return real values. This is the Bruno parity item "Part D" and the prerequisite named in R-04.

## Decisions from the brainstorm

| Topic | Decision |
|---|---|
| Reach of row values | Scripts only. No `{{column}}` variables, so variable resolution and precedence do not change. |
| Where the file comes from | A saved default in the collection, plus a per-run override picked in the Runner tab. |
| What the saved default may point to | A relative path inside the collection directory only. No trust capability. |
| Per-run override | Any file on disk, chosen by the user in the file picker. The user's own pick needs no trust gate. |
| Which runner | Both. The Runner tab runs a frontend loop (`startRun` in `src/stores/pane-store.ts`, one `executeRunnerEntry` per request) and does not call the backend runner. The tab is what users run, so it gets the row loop. `CollectionRunnerService` (`run_collection`) gets the same row loop so it is correct for any caller and for a later move of the tab onto it. |
| Row delivery to a send | A new optional `iteration` field on `ExecuteRequestInput` (index, total, row). Both runners set it. A nested `rok.runRequest` inherits its caller's value. |
| Where the file is parsed | In Rust. A new command loads and parses the file and returns the rows to the Runner tab, so the frontend has no CSV or JSON parser. |
| `stopExecution()`, `stopOnFailure` (backend runner) | End the whole run, not just the current row. |
| Runner tab stop | The Stop button ends the whole run. The tab has no `setNextRequest`, `stopOnFailure` or `stopExecution` today and this spec does not add them. |
| Runtime variables | Backend runner: carry across rows, as they carry across steps today. Runner tab: unchanged (it carries none between steps today, see R-51). |
| Step cap (backend runner) | The existing 1000-step cap counts steps across all rows. |

## Non-goals

`{{column}}` variables, load tests, Flow, the headless CLI (R-79), a new trust capability, writing data files from Rocket, and data files for single sends.

## Compatibility

Neither Bruno nor Postman stores a data file in the collection, as far as we know (Bruno takes it per run, Postman picks it in the runner). The saved default is a Rocket convenience. It is ignored by Bruno because `extensions` is free-form, and it is dropped by a Postman export. The data file travels with a Rocket-format folder copy as a plain file. The script API is the part that carries over: `rok.runner.iterationData` maps to `bru.runner.iterationData` and `pm.iterationData`. The exact Bruno member names are checked against the Bruno docs as the first step of the script task (Plan 2), and this spec's member list changes if they differ.

## Behaviour

### Data file

- **Formats:** CSV with a header row (RFC 4180, UTF-8, a leading BOM stripped), or JSON with a top-level array of objects. The format is chosen by the extension (`.csv`, `.json`), case-insensitively. Any other extension is an error.
- **Values:** every value becomes a string. `null` becomes `""`. A nested object or array becomes compact JSON text. A JSON number or boolean becomes its JSON text (`3`, `true`).
- **JSON keys:** the key set of a row is the union as written. A row that lacks a key simply has no entry for it.
- **CSV rows:** a row with fewer cells than the header gets `""` for the rest. A row with more cells than the header is an error naming the row. A blank line is skipped. Duplicate header names are an error.
- **Limits:** 10 MB and 1000 rows. Past either, the run does not start.
- **Empty:** a file with a header and no rows, or an empty JSON array, is an error before the run starts. Nothing is sent.

### Iteration

- One full pass over the run set per row. A pass starts at the first item. Excluded items (the Runner tab's include toggle) stay excluded in every pass.
- With no data file the run behaves exactly as today: one pass, `iterationIndex` 0, `totalIterations` 1, and `iteration` absent on every send.
- Backend runner: `setNextRequest` jumps stay inside the current pass. `carried_runtime` lives for the whole run and is not reset between passes. `RunSummary.stopped_reason` keeps its current values, and the `StepLimitReached` message states the limit counts steps across all rows.
- Runner tab: each request row in the list shows one result per pass. The results list groups by pass, newest pass last. Pressing Stop ends the whole run, and the remaining items of every pass are marked skipped.

### Script API

- `rok.runner.iterationData.get(key)` returns the string or `undefined`.
- `rok.runner.iterationData.has(key)` returns a boolean.
- `rok.runner.iterationData.get()` with no key returns a plain copy of the row (the same as Bruno's `get()`).
- `rok.runner.iterationData.toObject()` returns a plain copy of the row.
- `rok.runner.iterationData.stringify()` returns the row as a JSON string (Bruno name).
- Bruno's `unset(key)` is not provided: the row is read-only.
- The object is read-only and rebuilt for each pass. A script cannot change a later row.
- `iterationIndex` is zero-based. `totalIterations` is the row count.
- These values are the same for every script phase of every request in the pass (pre-request, post-response, tests, and nested `rok.runRequest` calls). A nested `rok.runRequest` sees its caller's row.

### Where the data file is resolved

1. The per-run path from the Runner tab, when set.
2. Otherwise the saved default, when set and valid.
3. Otherwise no data file.

## Persistence

- Saved default key: `extensions.rocketapi.runner.dataFile` in `opencollection.yml`, a path relative to the collection root with `/` separators. It follows the existing `extensions.rocketapi.scripts.additionalContextRoots` pattern in `CollectionSettings` (`crates/rocket-collection/src/settings.rs`).
- A settings save keeps every other key under `extensions`, the same as for `scripts.flow` today.
- On read, the value is validated like `validate_path`: no absolute path, no `..` component, no empty value. The resolved file must be inside the collection root. A symlink that leaves the collection root is rejected.
- Forward compatibility: an invalid or unreadable saved value never stops a collection from loading. It is ignored on load, kept unchanged on save, and the Runner tab shows a warning. Nothing turns it into a default.
- No data file path is stored in a folder file. The default is per collection, and the run can still target a folder.

## Architecture

| Layer | Change |
|---|---|
| `rocket-collection` | `CollectionSettings` gains an optional `runner_data_file`. |
| `rocket-collection` (domain) | New pure `iteration_data.rs`: parse CSV or JSON text into `IterationData` (column names and a map per row), the limits, `format_for_path`, the pure saved-path rule `validate_data_file_path`, and the port `DataFileReader` (read a data file's text). `CollectionSettings` gains `runner_data_file`. No I/O and no dependency on `rocket-app` or `rocket-infra`. |
| `rocket-app` (orchestration) | `IterationDataService`: picks the per-run path or the saved default through the port and returns `IterationData`. It does no file reads. `iteration_info` maps a row to `rocket_scripting::IterationInfo`. `ExecuteRequestInput` gains `iteration: Option<IterationInfo>` (`index`, `total`, `data`; camelCase IPC DTO, `#[serde(default)]`). `ExecutionScriptHost::run_request` copies the caller's `iteration` onto the nested input. `RunCollectionInput` gains `data_file: Option<String>`. The outer loop in `CollectionRunnerService::run_unpinned`. `RunStepResult` gains `iteration`. |
| `rocket-infra` (adapter) | `FsDataFileReader` implements the port: it resolves the saved default inside the collection root (canonicalize and symlink checks) and the per-run path as given, enforces the size limit, and reads through the workspace-following repository handle (R-52). It also converts the settings field. `rocket-infra` defines no trait. |
| `src-tauri` | A command `load_iteration_data` (collection, optional per-run path) returning the rows, the column names and which source was used. Added to `src-tauri/app-commands.txt`. The existing `run_collection` command gets the new input field through its DTO. |
| Scripting | `ScriptContext` gains `iteration: Option<IterationInfo>` through a `with_iteration` builder, set at the three phase sites in `execution_service.rs` from `input.iteration`. `ScriptInputState` carries it to the ops. `bootstrap.js` exposes the values. `iterationIndex` and `totalIterations` are no longer constants. |
| Frontend | `startRun` in `pane-store.ts` loops rows around the entries loop and passes `iteration` to `executeRunnerEntry`. A data-file row in the Runner tab (picker, "use saved default", row count after load, warning for an invalid saved value). A field in collection settings for the saved default. Results grouped by pass. `rok-types.ts` typings and doc comments updated. |

The backend runner takes the file path in its input and reads the file itself at run start. The Runner tab asks the new command for the rows once, when the user presses Run, and holds them in tab state for that run. A row travels with each send (at most 1000 rows of at most 10 MB in total, one row per send), so no extra IPC call is needed per request.

### Events

Contract tests (`wire_contract_*`) in `execution_events.rs` are updated:

- `RunnerStarted` gains `total_iterations`.
- `RunnerStepCompleted` gains `iteration`.
- The tag stays camelCase and the field names stay snake_case.
- Listeners already ignore unknown keys, so an older frontend keeps working.

## Errors

| Case | Result |
|---|---|
| Missing file, unreadable file, wrong extension, over the size or row limit, empty data, bad CSV row, duplicate header, invalid JSON | The run fails before `RunnerStarted`, with an error that names the file and the row when there is one. No request is sent. |
| Saved default is invalid (absolute, `..`, outside the root, missing) | Ignored for loading. When a run needs it, the run fails before it starts with the reason. The Runner tab shows the warning ahead of time. |
| Per-run path set and the saved default is invalid | The per-run path is used, and the saved default is not checked. |
| A row is bad in the middle of a run | Not possible. The whole file is parsed and validated before the first step. |

## Testing

- **`iteration_data.rs` (pure):** CSV (quotes, embedded commas and newlines, BOM, short rows, long rows, blank lines, duplicate headers), JSON (all value types, missing keys, non-array top level), limits, empty input, extension detection.
- **Backend runner service:** one pass equals old behaviour; N rows give N passes with the right `iteration`; `setNextRequest` stays within a pass; `stopExecution` and `stopOnFailure` end the whole run; runtime variables carry across passes; the shared step cap reports `StepLimitReached`; a parse error sends nothing and publishes no `RunnerStarted`. Use `NullEventPublisher` and `wiremock`.
- **Send input:** `ExecuteRequestInput.iteration` reaches all three script phases; a nested `rok.runRequest` inherits the caller's value; an absent value gives index 0 and total 1.
- **Runner tab store:** `startRun` with rows runs every included entry once per row in order, passes `iteration` to each send, ends all passes on Stop, and with no data file behaves as before.
- **Scripting engine:** replace `rok_runner_iteration_values_default_to_a_single_iteration` with default and data-driven cases; `get`, `has`, `toObject`; read-only behaviour; a nested `rok.runRequest` sees the caller's row.
- **`rocket-infra`:** saved-path validation (absolute, `..`, symlink out of the root), the size limit, settings round trip keeping other `extensions` keys, an unknown or invalid value kept on save.
- **Wire contract:** updated `RunnerStarted` and `RunnerStepCompleted` payloads.
- **Frontend (Vitest):** Runner tab picker and saved-default states, iteration grouping, settings field. Run `bun run tsc --noEmit` and `bun run check`.
- **Manual check:** a `docs/manual-tests/runner-iteration-data.md` file with a CSV run, a JSON run, a bad file, and the saved default after a pull.

## Delivery

A plan index with at most 3 tasks per plan:

1. Backend core: parser, port, infra reader and settings field; the `iteration` field on `ExecuteRequestInput` with the nested-run copy; the `load_iteration_data` command with its ACL entry; the row loop and events in `CollectionRunnerService`.
2. Scripting: `ScriptContext` and engine state, `bootstrap.js` values, the Bruno member-name check, typings, tests.
3. Frontend and docs: Runner tab row loop and data-file controls, the collection settings field, feature doc `docs/features/runner-iteration-data.md`, manual test file, and R-04 status in `docs/roadmaps/roadmap.md` and the Part D row in `docs/features/rok-api-parity-notes.md`.

Plan 1 ends with the `iteration` field present but unread by scripts, and plan 2 makes scripts read it. Run them in order.

## Open follow-ups, not in this spec

- `{{column}}` variables from the row.
- Row filters (run rows 3 to 5) and a delay between rows.
- Data files in load tests and Flow, and the CLI flags in R-79.
