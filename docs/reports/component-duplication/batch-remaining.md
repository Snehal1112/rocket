# Semantic Duplication Audit — Remaining Components

## Scope and method

Reviewed the assigned component scope only for findings, while using `docs/reports/component-duplication/function-catalog.json` as a discovery index and inspecting every complete implementation before judging intent.

Assigned scope:

- `src/components/ErrorBoundary.tsx`
- `src/components/SplashScreen.tsx`
- `src/components/audit/`
- `src/components/import/`
- `src/components/history/`
- `src/components/response/`
- `src/components/status-bar/`
- `src/components/title-bar/`
- `src/components/workspace/`

The catalog contained 39 function/component entries in this scope. All 20 implementation files and both barrel files were reviewed. Comparisons included functions within each assigned folder and across assigned folders. Existing code outside the scope was consulted only to avoid recommending a duplicate helper as the survivor.

Confidence means confidence that the cited implementations encode the same reusable intent, not confidence that extraction should happen immediately. Generic React/UI similarities were excluded unless they carry the same behavior and maintenance risk.

## Findings summary

| ID | Confidence | Priority | Duplicate intent | Locations |
|---|---:|---:|---|---|
| R-1 | 99% (high) | P1 | Append a child workspace name to a host filesystem directory | `import/`, `workspace/` |
| R-2 | 98% (high) | P1 | Pick an import source and normalize it into selected-source state | `import/` |
| R-3 | 95% (high) | P2 | Resolve a workspace from the workspace list and active/explicit ID | `status-bar/`, `title-bar/`, `workspace/` |
| R-4 | 91% (high) | P2 | Copy text to the clipboard and expose temporary success feedback | `response/` |

## R-1 — Workspace child-path construction is duplicated

**Confidence:** 99% (high)  
**Priority:** P1

### Exact references

- `src/components/import/ImportCollectionDialog.tsx:138-148` — inside `handleImport`, derives `wsName`, obtains the app data directory, detects `\` versus `/`, appends the workspace name, and creates a workspace.
- `src/components/workspace/CreateWorkspaceDialog.tsx:71-77` — inside `handleCreate`, detects `\` versus `/`, appends the workspace name to the chosen directory, and creates a workspace.

The duplicated core is:

```ts
const sep = base.includes('\\') ? '\\' : '/';
const fullPath = base.endsWith(sep) ? base + name : base + sep + name;
```

### Shared intent

Construct the filesystem path for a new workspace as a direct child of a selected/base directory, without introducing a duplicate separator and while preserving the host path style.

### Material differences

- Import derives the child name from the selected source and strips a trailing `.zip`; create uses a trimmed user-entered name.
- Import always uses `getAppDataDir()` as the base; create uses the folder chosen by the user.
- Import calls the low-level `apiCreateWorkspace`; create calls the React Query mutation.
- Error and post-create behavior differ and should remain in the components.

Those differences surround the duplicated path operation; they do not change its intent.

### Recommendation / survivor

Extract one host-path helper, for example `appendPathSegment(base, segment)`, in an existing frontend path utility module (or a narrowly named new one if none exists). Treat the implementation in `CreateWorkspaceDialog.tsx:74-75` as the behavioral survivor because its variables describe the generic operation; replace both inline copies with that helper.

Do not move workspace creation or source-name derivation into the helper. The helper should own only separator-safe path joining. Before implementing, consider whether the project’s Tauri path API can be used without making the call sites needlessly asynchronous; the current helper must preserve Windows-style paths received from Tauri even when the frontend runs in a webview.

### Staged remediation

1. Add table-driven unit tests for `/base + child`, `/base/ + child`, `C:\\base + child`, and `C:\\base\\ + child`.
2. Extract the path helper with the existing behavior unchanged.
3. Migrate `CreateWorkspaceDialog` first and verify create-workspace tests or a focused component test.
4. Migrate `ImportCollectionDialog` and verify both normal import and `createWorkspace` import flows.
5. In a separate follow-up, decide whether to reject/sanitize child names containing separators; that is behavior hardening, not part of deduplication.

## R-2 — Import source picker handlers repeat the same normalization flow

**Confidence:** 98% (high)  
**Priority:** P1

### Exact references

- `src/components/import/ImportCollectionDialog.tsx:86-93` — `handleChooseFolder`.
- `src/components/import/ImportCollectionDialog.tsx:95-106` — `handleChooseZip`.
- `src/components/import/ImportCollectionDialog.tsx:108-119` — `handleChoosePostmanJson`.
- Related variant: `src/components/import/ImportCollectionDialog.tsx:121-128` — `handleChooseEnvJson` performs the same picker call/returned-string guard but intentionally stores only a path.

### Shared intent

Open a single-selection Tauri file/folder picker, ignore cancellation/non-string results, derive display metadata for the selected source, store it, and clear the previous error.

### Material differences

- Folder selection uses `directory: true` and no filter.
- ZIP and Postman selections use different filters and different `SelectedSource.kind` values.
- Environment JSON is auxiliary input rather than the primary `SelectedSource`; it should not clear or replace the primary source.
- The three primary handlers derive a name with `path.split('/')`, which does not recognize Windows backslashes. Elsewhere, `CreateWorkspaceDialog.tsx:52` already uses `split(/[\\/]/)` for the same basename intent.

### Recommendation / survivor

Keep small event-specific wrappers if they improve JSX readability, but route the three primary handlers through one parameterized operation such as:

```ts
chooseSource(kind, pickerOptions)
```

The shared operation should own the picker call, cancellation guard, cross-platform basename extraction, `setSource`, and `setError(null)`. Use the cross-platform basename behavior at `src/components/workspace/CreateWorkspaceDialog.tsx:52` as the survivor for name extraction, rather than preserving `split('/')`.

Keep `handleChooseEnvJson` separate or share only a lower-level `pickSinglePath(options)` helper; forcing auxiliary environment selection into `SelectedSource` would erase a meaningful domain distinction.

### Staged remediation

1. Add focused tests/mocks for cancellation, folder selection, ZIP selection, Postman collection selection, and a Windows-style returned path.
2. Extract/test a cross-platform basename helper or reuse a canonical path basename helper if one is introduced with R-1.
3. Consolidate the three primary source handlers behind one parameterized picker operation.
4. Optionally reuse only the picker/cancellation layer for environment JSON.
5. Verify that changing import type still clears `source`, `envFilePath`, and `error` via `switchImportSource` (`79-84`).

## R-3 — Workspace resolution is repeated across four components

**Confidence:** 95% (high)  
**Priority:** P2

### Exact references

- `src/components/status-bar/ContractsStatusItem.tsx:17-26` — queries workspaces, reads `activeWorkspaceId`, finds the active workspace, then derives a collection root.
- `src/components/title-bar/WorkspaceSwitcher.tsx:50-70` — queries workspaces, reads `activeWorkspaceId`, and finds the active workspace for the trigger label.
- `src/components/workspace/WorkspaceGitTab.tsx:9-13` — queries workspaces, reads `activeWorkspaceId`, and finds `workspaceId || activeWorkspaceId`.
- `src/components/workspace/WorkspaceOverviewTab.tsx:35-39` — repeats the same explicit-ID-with-active-fallback lookup as `WorkspaceGitTab`.

An existing canonical query also exists at `src/lib/queries/workspace-queries.ts:35-40`: `useActiveWorkspace()`.

### Shared intent

Resolve the current workspace object from workspace query state, usually from the active workspace ID and sometimes from an explicit workspace ID supplied by a workspace tab.

### Material differences

- `ContractsStatusItem` and `WorkspaceSwitcher` always want the active workspace.
- `WorkspaceGitTab` and `WorkspaceOverviewTab` accept an explicit `workspaceId`; their fallback expression is currently redundant at the type level because the prop is declared as required `string` in both files.
- The consumers use different fields (`path`, `name`, full object) and have different empty-state behavior.
- Existing `useActiveWorkspace()` invokes the backend under a separate query key instead of deriving the object from `useWorkspaces()` plus the Zustand ID. Replacing all call sites blindly could alter fetching/cache behavior.

### Recommendation / survivor

Make workspace selection a query-layer concern rather than preserving any component-local `.find(...)` as the long-term survivor.

1. For active-only consumers, evaluate `useActiveWorkspace()` as the canonical survivor.
2. For tabs that need an explicit ID, add a narrowly scoped selector hook such as `useWorkspace(workspaceId)` or derive from the shared `useWorkspaces()` cache in one place.
3. Decide and encode whether an explicit workspace ID may be absent. If not, remove the misleading active-ID fallback from `WorkspaceGitTab` and `WorkspaceOverviewTab`; if it may be absent, change the prop type to `string | undefined` and test the fallback.

Do not extract a generic array `findById` utility; the duplicated value lies in workspace source-of-truth and fallback semantics, not in `Array.prototype.find` syntax.

### Staged remediation

1. Add query/hook tests establishing active-only and explicit-ID behavior, including missing IDs.
2. Resolve the contract ambiguity for the two required `workspaceId` props.
3. Introduce or adopt one query-layer workspace resolver.
4. Migrate `WorkspaceGitTab` and `WorkspaceOverviewTab` together because their logic is identical.
5. Migrate `ContractsStatusItem` and `WorkspaceSwitcher`, checking that no extra backend request or loading flicker is introduced.
6. Run focused component tests for workspace switching, the Git tab empty state, overview title/description selection, and contract-root derivation.

## R-4 — Clipboard copy with transient success state is duplicated in the response UI

**Confidence:** 91% (high)  
**Priority:** P2

### Exact references

- `src/components/response/ResponseBodyViewer.tsx:215-223` — `handleCopyBody` writes `prettyBody`, sets a boolean copied state, resets after 1500 ms, and silently ignores clipboard failure.
- `src/components/response/ResponseHeadersTable.tsx:29-37` — `handleCopy` writes a header value, stores the copied header key, conditionally resets it after 1000 ms, and silently ignores clipboard failure.

### Shared intent

Copy text through the Clipboard API and expose a short-lived success marker so the triggering control can switch from a copy icon to a check icon.

### Material differences

- Body copy tracks a single boolean; header copy tracks which row was copied.
- Header reset protects against an older timeout clearing a newer copied key; body copy has only one target.
- Feedback durations differ (1500 ms versus 1000 ms).
- Body copy copies the formatted/pretty representation even from the response toolbar; header copy copies one raw value.

### Recommendation / survivor

Extract a small reusable hook that supports an identity token, for example `useClipboardFeedback<T>()`, returning the active copied token and a `copy(text, token)` callback. The token-based semantics in `ResponseHeadersTable.tsx:29-34` are the stronger survivor because they also handle the single-target case and guard against clearing a newer selection.

The hook should centralize Clipboard API failure handling and timeout cleanup on unmount. Make duration configurable or choose one product-standard duration before migration. Preserve the copied payload at each call site.

### Staged remediation

1. Add fake-timer tests for successful copy, clipboard rejection, feedback expiry, a second copy before the first timeout expires, and unmount cleanup.
2. Extract the token-based hook without changing the current UI.
3. Migrate `ResponseHeadersTable` first because it exercises the stronger multi-target semantics.
4. Migrate `ResponseBodyViewer` with a constant token or a boolean adapter.
5. Standardize timeout duration only after confirming the intended UX; otherwise preserve 1000/1500 ms through a hook option.

## Reviewed areas with no credible duplication

### Folders with no credible duplication

- `src/components/audit/` — no consolidation candidate survived review. `isoStartOfDay` and `isoEndOfDay` are complementary range-boundary operations, not duplicates. Audit filtering, event summarization, relative-time presentation, profile editing, and evidence export have distinct domain behavior.
- `src/components/history/` — no credible within-folder duplicate exists. Its time display and URL display helpers are purpose-specific. `formatTime` is not a duplicate of audit `formatRelative`: one emits local clock time while the other emits relative elapsed time with future handling.

### Assigned root files with no credible duplication

- `src/components/ErrorBoundary.tsx` — error fallback rendering, logging, and boundary composition are distinct responsibilities.
- `src/components/SplashScreen.tsx` — script loading, animation lifecycle, and completion timing have no peer in the assigned scope.

### Reviewed folders whose only credible duplication is reported above

- `src/components/import/` — R-1 and R-2 only.
- `src/components/response/` — R-4 only.
- `src/components/status-bar/` — R-3 only.
- `src/components/title-bar/` — R-3 only; window controls and title-bar composition are otherwise distinct.
- `src/components/workspace/` — R-1 and R-3 only.

## Similarities deliberately not reported as duplicates

- Search/filter expressions in `AuditLogTab`, `ResponseHeadersTable`, and `ResponseBodyViewer`: all use lowercase substring matching, but they search different data shapes and have different result semantics. A shared abstraction would mostly wrap `toLowerCase().includes(...)` without centralizing domain policy.
- Empty states across audit, history, response, Git, environment, and workspace views: they share visual vocabulary but not behavior or content contracts. This is a design-system consistency opportunity, not function-level semantic duplication.
- `formatBody` and `countJsonKeys` in `ResponseBodyViewer`: both parse JSON, but one formats content while the other computes summary metadata. Parsing once may be a local performance/structure improvement, but the functions do not have duplicate intent.
- Create/rename/import dialog close handlers: each resets materially different state and lifecycle timing. Their superficial setter sequences do not justify a shared function.
- Inline creation flows for collections and environments: both trim a name and react to Enter/Escape, but they invoke different persistence semantics and have different blur/validation behavior. Consolidation would likely increase coupling.
- Test result counts and contract status counts: both aggregate arrays, but their status domains, output shapes, and downstream behavior are unrelated.

## Validation notes

This was a read-only production-code audit. No production files were changed. No test or build command was run because the only change is this Markdown report. The repository search found no direct tests for the components involved in R-1 through R-4; the staged plans therefore begin with characterization coverage.
