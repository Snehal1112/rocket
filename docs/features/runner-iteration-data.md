# Runner iteration data

Run a collection or folder once per row of a CSV or JSON data file. Scripts read the current row. Roadmap item R-04. Spec: `docs/superpowers/specs/2026-11-12-runner-iteration-data-design.md`. Manual checks: `docs/manual-tests/runner-iteration-data.md` (open, nobody has run the app yet).

## What the user gets

- In the Runner tab, a "Data file" row. Choose a CSV or JSON file, or use the default saved in the collection settings.
- The whole run set runs once per row. Results are grouped by pass (one section per row).
- Scripts read the row with `rok.runner.iterationData`.
- In the collection Overview tab, a "Runner" card with a "Runner data file" field for the saved default.

## Data files

| Rule | Value |
|---|---|
| Formats | `.csv` and `.json`, picked by the extension (case-insensitive). Any other extension is an error. |
| CSV | The first line is the header row. A UTF-8 BOM is skipped. Duplicate headers are an error. A row with fewer cells than the header gets empty values. A row with more cells is an error. |
| JSON | An array of objects. Any other shape is an error. Columns are the keys, in the order first seen. |
| Values | Always strings. In JSON a number or boolean becomes its JSON text, `null` becomes an empty string, and an object or array becomes its JSON text. |
| Size | At most 10 MiB, and the file must be valid UTF-8. The size is checked before the file is read. |
| Rows | At least 1 and at most 1000. A file with only a header has no rows and is an error. |

Every error is found before the run starts: no event is published and nothing is sent. Parse errors start with the file name.

## Saved default and per-run file

The saved default is stored in `opencollection.yml` at `extensions.rocketapi.runner.dataFile` (a string). `extensions` is the free-form object of the OpenCollection schema, so this is allowed. See `docs/superpowers/specs/opencollection-spec-reference.md`.

Rules of the saved default:

- It is a path relative to the collection folder, with `/` separators.
- It is checked when a run needs it, not when it is saved. The path may not be empty, absolute, contain `\`, `:` or a NUL byte, or have an empty, `.` or `..` part.
- The file is resolved with symlinks followed. If the result is outside the collection folder, it is rejected.
- A settings save keeps it unchanged unless the save sets it. The other cards of the Overview tab send the loaded value back, so saving them does not delete it.
- Setting it needs no trust capability, because it only reads a file inside the collection folder. `docs/features/collection-trust.md` needs no change.

The per-run file is chosen in the Runner tab (or `data_file` in `RunCollectionInput`). It can be any file on disk. It wins over the saved default. Its extension is checked after the file is read.

A saved default that is invalid shows an error in the Runner tab and blocks the run. The X button ("Use saved default") of the Data file row drops a per-run choice. With a saved default set there is no "run without a data file" choice in the Runner tab; clear the setting instead.

## Script API

| Member | Meaning |
|---|---|
| `rok.runner.iterationData.get(key)` | The value of a column, or `undefined`. |
| `rok.runner.iterationData.get()` | A copy of the whole row (Bruno does the same). |
| `rok.runner.iterationData.has(key)` | Whether the column exists. |
| `rok.runner.iterationData.toObject()` | A copy of the whole row (a Rocket extra). |
| `rok.runner.iterationData.stringify()` | The row as JSON text. |
| `rok.runner.iterationIndex` | Zero-based pass number. |
| `rok.runner.totalIterations` | Number of passes. |

Without a data file the values are `0`, `1` and an empty row.

`unset(key)` is a Bruno member and is not provided. The row is read-only, and each script runs in its own isolate, so an unset could not reach the later requests of a pass. Changing a copy never changes the real row. Keys of a row list alphabetically, not in column order, because rows are maps.

The row reaches the pre-request, post-response and tests scripts of a send. A nested `rok.runRequest` inherits the caller's row. Typings are in `src/components/editor/rok-types.ts`; the runtime is in `crates/rocket-infra/src/scripting/bootstrap.js`.

## Runner tab and backend runner

The Runner tab does not call the backend runner. It loops in the frontend (`startRun` in `src/stores/pane-store.ts`) and sends each request with `ExecuteRequestInput.iteration`. The backend runner (`run_collection`, `CollectionRunnerService`) has the same row loop and more rules.

| Behaviour | Runner tab | `run_collection` |
|---|---|---|
| Passes | One per row. Each pass runs every included request. | The same. |
| `rok.runner.setNextRequest` | Not supported. | Jumps stay inside the current pass. |
| Stop by script and stop on failure | Not supported. | End the whole run, not only the pass. |
| Runtime variables | Not carried by the tab. | Carry across rows. |
| Step cap | None. | 1000 steps for the whole run, all passes together. |
| Stop button or cancel | Ends the whole run. The open pass and later passes show as skipped. | Ends the run before the next step. |

Moving the tab onto `run_collection`, or adding the extras to the tab, is roadmap item R-86.

## Events and IPC

- Command `load_iteration_data(collection, per_run_path)` in `src-tauri/src/commands/runner.rs`, listed in `src-tauri/app-commands.txt`. It returns nothing when neither a per-run path nor a saved default is set. Otherwise it returns the columns, the rows, the source (`perRun` or `savedDefault`) and the file name. The Runner tab calls it for the preview and again when a run starts.
- `ExecuteRequestInput.iteration` is `{ index, total, data }`. A nested run inherits it. It can be set over IPC, so a caller can give its own send a row, and only that send's scripts see it.
- `RunCollectionInput.data_file` is the per-run path of the backend runner.
- `runner-started` carries `total_iterations`. `runner-step-completed` carries `iteration` (zero-based). `RunStepResult.iteration` is in the run summary. Event fields are snake_case on purpose (see `.claude/rules/tauri-ipc-boundaries.md`).
- `CollectionSettings.runnerDataFile` is the IPC field of the saved default (`runner_data_file` in Rust).

## Compatibility

Bruno and Postman do not store a data file inside the collection. The saved default is a Rocket extra under `extensions.rocketapi`. Bruno ignores it, and the Bruno export drops the `rocketapi` namespace. Rocket keeps the key, with every other key under `extensions`, on every save.

## Known limits and follow-ups

- No `{{column}}` variables from the row yet. Scripts read the row. See R-84.
- No row filters, no delay between rows, no data files in load tests or Flow. See R-85.
- The Runner tab lacks the backend runner's extras (table above). See R-86.
- `iterationData.unset(key)` is missing (see above).
- A re-run in the Runner tab reloads the data file, so it uses the rows the file holds at that moment.
- The Runner tab's preview does not refresh when the file changes on disk. It reloads when the path changes and when a run starts.
- A failing load in the Runner tab (unreadable file, invalid saved default) blocks the run.
- The Overview field reads the saved value once, when the tab loads.
- The backend runner fetches external secrets before it loads the data file.
