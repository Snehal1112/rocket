# Collection Runner (Frontend) — Design

## Summary

A collection runner that sequentially executes every request in a chosen
collection or folder (respecting the existing folder tree order), running
each request's existing pre-request/test scripts via the already-existing
`execute_request` IPC command, and reporting a pass/fail result per
request. No backend changes are required — this is purely frontend
orchestration and UI, which is why the runner is scoped as a
frontend-only feature.

## Non-goals

- Data-driven / iteration runs (e.g. a CSV of variable sets run N times).
  Out of scope for v1; the core feature is "run this set of requests once,
  in order."
- A "stop on first failure" mode. Runs always continue through the full
  list; a failure is recorded against its request and execution proceeds.
- Per-run environment overrides. The runner uses whatever environment is
  currently active in the app, same as sending a single request normally.
- Backend/Rust changes. `execute_request` (see
  `src-tauri/src/commands/execution.rs`) already returns everything a run
  needs per request: response, `test_results`, `console_entries`, and
  `script_error`.

## Existing building blocks this relies on

- `execute_request` Tauri command — runs one request (with its
  pre-request/test scripts) and returns `ExecuteRequestResponse`
  (`status`, `headers`, `body`, timing, `test_results`, `console_entries`,
  `script_error`). Already used by the single-request Send flow.
- `getCollection(name)` (`src/lib/tauri-api.ts`) — returns the full
  `Collection` object with the complete nested folder/request tree,
  including full request bodies. No per-request file reads are needed at
  run time; the runner flattens this tree once.
- The existing Tests tab's result rendering (test name/status/error,
  console output) — reused for each run row's expandable detail rather
  than reimplemented.
- The tab/pane system (`src/stores/pane-store.ts`,
  `src/types/pane-types.ts`) — the runner is added as a new first-class
  tab type, following the same pattern as `RequestTab`/`CollectionTab`.

## Architecture

### Tab type

A new `RunnerTab` joins the existing `Tab` union
(`RequestTab | CollectionTab | ContractTab | DiffTab | ConflictTab | WorkspaceTab`)
in `src/types/pane-types.ts`:

```ts
interface RunnerTab extends BaseTab {
  type: 'runner';
  collectionRoot: string | null;   // null until picked (blank-picker entry point)
  folderPath?: string;             // undefined = whole collection
  runState: 'idle' | 'running' | 'stopped' | 'done';
  requests: RunnerRequestEntry[];  // flattened, ordered, with included/excluded + result
}

interface RunnerRequestEntry {
  requestPath: string;             // path relative to collection root
  name: string;
  method: HttpMethod;
  included: boolean;               // checkbox state, editable pre-run
  status: 'pending' | 'running' | 'passed' | 'failed' | 'skipped';
  result?: ExecuteRequestResponse; // populated once executed
  error?: string;                  // populated if execute_request itself rejected
}
```

### Entry points

Two entry points, both producing a `RunnerTab`:

1. **Sidebar context menu** — "Run collection" / "Run folder" on any
   collection or folder tree node. Calls a new pane-store action
   `openRunnerTab(collectionRoot, folderPath?)`, pre-scoped.
2. **Dedicated Runner tab** — a new option alongside the existing
   "new tab" affordances, opening a `RunnerTab` with `collectionRoot:
   null`. `RunnerPane` renders an in-tab picker (collection + optional
   folder) until one is chosen, then behaves identically to the
   pre-scoped case.

### Rendering

A new `RunnerPane.tsx` is registered in `PaneRenderer.tsx` for
`tab.type === 'runner'`.

## Components

- **`RunnerPane.tsx`** — top-level: renders the picker when
  `collectionRoot` is null, otherwise the request-list/results view.
- **`RunnerRequestList.tsx`** — pre-run state: the flattened,
  tree-ordered list of requests with checkboxes to exclude specific
  requests from the run, and a "Start" button (disabled if nothing is
  included, or the list is empty).
- **`RunnerResultsList.tsx`** — during/after run: one row per request
  (status icon, method + name, HTTP status, pass/fail test count,
  duration). Each row expands to show the same test-name/error/console
  detail the existing Tests tab renders, reused as-is.
- **`RunnerSummaryHeader.tsx`** — aggregate bar: `X/Y passed`, total
  elapsed time, a progress indicator while running, and the
  Start/Stop/Re-run controls.

State mutations (start/stop/re-run/toggle-exclude/append-result) are
pane-store actions that patch the `RunnerTab`'s `runState`/`requests`
fields in place, following the same pattern `updateRequest`/`setResponse`
already use for `RequestTab` — no separate parallel store.

## Data flow

1. On "Start", `RunnerPane` calls `getCollection(collectionRoot)` (if not
   already available from existing app state) and recursively flattens
   the folder tree — scoped to `folderPath` if set, otherwise the whole
   collection — into an ordered `RunnerRequestEntry[]`, in the same order
   the sidebar tree displays them.
2. A sequential async loop walks the included entries one at a time:
   build `ExecuteRequestInput` from that entry's already-loaded request
   data (the same construction the single-request Send flow uses), with
   `environmentName` set to whatever's currently active. Call
   `executeRequest`. On response, patch the entry's `status` to `passed`
   (2xx and all `test_results` passed and no `script_error`) or `failed`
   otherwise, and store the full `ExecuteRequestResponse` for the
   expandable detail view.
3. A cancellation flag, set by "Stop", is checked before each loop
   iteration. Stopping leaves already-completed rows as-is and marks the
   remaining `pending` rows `skipped`.
4. `RunnerSummaryHeader`'s aggregate counts are derived reactively from
   `requests` — no separate summary state to keep in sync.
5. "Re-run" resets every entry's `status` back to `pending` (keeping the
   current include/exclude selection) and restarts step 2.

## Error handling

- Per-request execution problems (network failure, script error, non-2xx
  status) are not run-aborting — they're recorded as a `failed` row, and
  the run continues to the next request.
- If the `executeRequest` call itself rejects (malformed request, IPC
  error), the row is marked `failed` with the raw error message stored in
  `RunnerRequestEntry.error` and shown in its expanded detail. The run
  continues.
- An empty collection/folder (no requests found while flattening) shows
  an empty state in `RunnerRequestList`; "Start" stays disabled.
- A collection or folder deleted/moved mid-run is out of scope for v1 —
  the run already holds its own snapshot of request data from step 1 of
  the data flow, so it simply completes against that stale snapshot.

## Testing

- Vitest unit tests for the recursive tree-flattening/ordering logic,
  fixture-based, mirroring the ordering already covered on the Rust side
  by `rocket-collection`'s `folder_count_requests_recursive` test.
- Vitest tests for the new pane-store runner actions: start, stop,
  re-run, toggle-exclude transitions, and aggregate-count derivation.
- Component tests for `RunnerPane` covering: picker state, request-list
  state (with exclusions), in-progress state, and completed state with
  expandable result rows — following the existing patterns in
  `src/stores/__tests__` and `src/components/**/__tests__`.
- No backend/Rust changes are anticipated, so no new Rust tests are
  needed. `execute_request` is already stateless per call
  (`State<RequestExecutionService>`), so no changes are needed to support
  being called in a tight sequential loop.
