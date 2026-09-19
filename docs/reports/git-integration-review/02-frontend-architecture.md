# Git Frontend Architecture Review

**Scope reviewed**

- All `src/components/git/*.tsx`
- `src/stores/git-store.ts` and `src/stores/__tests__/git-store.test.ts`
- Git-related pane types and integration in `src/types/pane-types.ts`, `src/stores/pane-store.ts`, `src/components/panes/EditorGroup.tsx`, `src/components/panes/TabItem.tsx`, and `src/components/panes/BreadcrumbBar.tsx`
- `src/components/layout/GitToolbarButton.tsx`
- `src/components/workspace/WorkspaceGitTab.tsx`
- Relevant Tauri API and workspace-query contracts

**Method**

Static call tracing was combined with focused validation. `yarn tsc --noEmit` passed. `yarn test src/stores/__tests__/git-store.test.ts` passed all 53 tests. There are no component-level Git tests in the reviewed tree; the existing store tests primarily assert happy-path API calls and post-action refreshes.

## Executive summary

The frontend has useful component decomposition and generally keeps direct Git IPC behind `git-store.ts`, but its ownership model is unsafe for the pane architecture. A single global, mutable `collectionPath` owns all repository state and actions, while the pane system can mount Git views for path-bearing tabs. Async completions are not scoped to the path or request that started them. This can mix repository A and repository B state, credentials, refreshes, and mutations.

The most urgent problems are:

1. **Global path/state races can display or mutate the wrong repository.**
2. **Clone can execute twice concurrently when credentials already exist.**
3. **Store actions swallow failures and still resolve, so callers report success, clear input, continue destructive workflows, or set success timestamps after failure.**
4. **The toolbar can create a Git tab for the active collection using a stale path from another collection.**
5. **Credential-dialog async state can leak stale secrets across workspace changes.**

The implementation also violates the repository's hard rules for raw controls and unscoped Zustand subscriptions, has incomplete invalidation after Git mutations, and exposes several accessibility gaps.

## Severity and confidence

- **Critical:** plausible wrong-repository mutation, duplicate destructive operation, or serious data-integrity impact.
- **High:** common workflow can produce incorrect state, false success, stale sensitive data, or blocked recovery.
- **Medium:** degraded reliability, accessibility, performance, or maintainability with bounded impact.
- **Low:** cleanup, consistency, or currently unused functionality.
- **Confidence:** High means directly demonstrated by control/data flow; Medium means behavior depends on runtime mounting or backend details.

---

## Findings

### F-01 — Singleton repository state is incompatible with path-bearing panes and is race-prone

- **Severity:** Critical
- **Confidence:** High
- **Refs:**
  - `src/stores/git-store.ts:46-107` — one global repository state and path
  - `src/stores/git-store.ts:133-163` — `setCollection` writes globally with no request/path guard
  - `src/stores/git-store.ts:166-262` — refresh methods re-read the current global path
  - `src/components/git/GitPanel.tsx:42-94` — each panel initializes the same store from its own prop
  - `src/components/panes/EditorGroup.tsx:186-208` — collection Git tabs and workspace Git tabs both mount `GitPanel`
  - `src/types/pane-types.ts:77-81` — `GitTab` itself is path-bearing
  - `src/components/panes/PaneRenderer.tsx:7-30` — split panes can mount multiple active leaf contents

**Impact**

`setCollection('/repo-a')` immediately sets `collectionPath` to A, then awaits `gitIsRepo(A)`. If another panel calls `setCollection('/repo-b')` before A completes, either completion can overwrite global `isRepo`, `status`, credentials, branches, remotes, stashes, `loading`, and `error`. The final `collectionPath` can be B while the final `status` came from A.

The race is worse inside A's `Promise.all`: `gitStatus(path)` is explicitly called for A, but `refreshStashes`, `refreshBranches`, and `refreshRemotes` read `get().collectionPath` at execution time. A single initialization can therefore assemble status from A and branch/remote/stash data from B.

All mutation actions likewise target whichever global path happens to be current when the action starts, not the `collectionPath` represented by the component or pane that invoked the action. A stale panel can therefore mutate another repository.

**Root cause**

Repository identity is implicit mutable store state instead of an argument or key. There is no request generation, abort signal, or path equality check before an async result commits to state.

**Recommended fix**

Prefer one of these models:

1. **Repository-keyed state:** `repos[path]` contains status, branches, remotes, stashes, log, errors, and pending operations. Actions receive `path` and update only that key.
2. **Panel-scoped controller/store:** create one store instance per `GitPanel`, provided through context, with path fixed at construction.

At minimum, make every action accept an explicit path and add a monotonically increasing load token to `setCollection`; only the latest token for the same path may commit results. Never let nested refreshes re-read a mutable global path during an operation—pass the captured path through.

**Focused tests**

- Start `setCollection(A)`, then `setCollection(B)`; resolve B first and A last. Assert all visible state remains B-scoped.
- During A initialization, switch the global path to B before branches/remotes/stashes resolve. Assert no mixed state is committed.
- Render two active split panes with different Git paths; click stage in A and assert IPC receives A, regardless of B's last load.
- Resolve an old refresh after a newer path load and assert it is ignored.

### F-02 — Clone starts twice when credentials already exist

- **Severity:** Critical
- **Confidence:** High
- **Refs:** `src/components/git/GitCloneDialog.tsx:97-111`, `src/components/git/GitCloneDialog.tsx:120-135`

**Impact**

`handleClone` first calls `setStep('progress')`, then directly calls `gitClone` when credentials exist. The state transition causes the effect at lines 97-111 to run with `step === 'progress'` and the same existing credentials, starting a second `gitClone` to the same destination. Concurrent clones can fail unpredictably or leave a partially written repository.

The effect also does not depend on `open`, has no one-shot operation ID, and cannot cancel or ignore stale post-clone work after the dialog closes or reopens.

**Root cause**

Clone execution has two owners: the click handler and a credential-arrival effect. `progress` represents both UI state and an implicit retry trigger.

**Recommended fix**

Use one execution path. Model an explicit pending clone request (`{url, destination, requestId}`), and have exactly one function execute it after credentials are available. Alternatively, let the handler open credentials and return; resume via an explicit callback/result from the credentials flow rather than an effect keyed on broad component state. Guard completion with a request ID and dialog lifecycle.

**Focused tests**

- With credentials preloaded, click Clone once and assert `gitClone` is called exactly once.
- Without credentials, save credentials and assert exactly one clone occurs.
- Close/reopen while a clone is pending; assert an old completion cannot change the new dialog state or open a workspace.

### F-03 — `Promise<void>` actions swallow failures, so callers cannot know whether work succeeded

- **Severity:** High
- **Confidence:** High
- **Refs:**
  - `src/stores/git-store.ts:67-105` — all action contracts return `Promise<void>`
  - `src/stores/git-store.ts:264-550`, `src/stores/git-store.ts:611-700` — actions catch and store errors instead of rejecting/returning a result
  - `src/components/git/GitCommitForm.tsx:17-24` — clears the commit message after a failed commit
  - `src/components/git/GitRemotesDialog.tsx:37-53` — clears add/edit/delete UI after failed operations
  - `src/components/git/BranchSelector.tsx:41-95` — infers success by comparing global error strings
  - `src/components/git/GitLandingPanel.tsx:53-173` — sets fetch timestamps and continues multi-step workflows after failed actions
  - `src/components/git/GitStashSection.tsx:62-119` — infers success from a shared error after completion

**Impact**

Examples:

- A failed commit still clears the user's commit message.
- A failed remote add clears both fields; a failed edit/delete exits the editing/confirmation state.
- Fetch and pull update `lastFetched` even when IPC failed.
- `handleFetchAndPush` can fetch unsuccessfully and then push anyway because `fetch()` resolves normally.
- `handleStashAndPull` can continue to pull after stash save fails, and can pop stash 0 after pull fails. Its `catch` is effectively unreachable for store-managed failures.
- Branch handlers compare `nextError !== prevError`; repeating the same failure string is interpreted as success and closes/clears UI. An unrelated concurrent error can be misattributed to the branch operation.

**Root cause**

The store combines operation execution, global notification state, and result semantics. A shared `error` string is being used as an unreliable out-of-band return value.

**Recommended fix**

Give actions explicit result semantics. Either reject on operational failure or return a discriminated result such as:

```ts
type GitActionResult<T = void> =
  | { ok: true; value: T }
  | { ok: false; error: string; kind?: 'auth-required' | 'conflict' | 'validation' };
```

Use store error state only for presentation/history, not control flow. Multi-step workflows must stop immediately on a failed step and should identify compensating behavior explicitly.

**Focused tests**

- Failed commit preserves the message.
- Failed fetch does not update `lastFetched` and does not proceed to push.
- Failed stash save does not pull; failed pull does not pop.
- Repeating an identical branch error remains a failure and keeps the popover open.
- Failed remote mutations preserve the user's fields/edit state.

### F-04 — Toolbar can open a collection Git tab with another collection's path

- **Severity:** High
- **Confidence:** High
- **Refs:** `src/components/layout/GitToolbarButton.tsx:24-60`

**Impact**

`openGitPanel` first uses `useGitStore.getState().collectionPath` without checking that it belongs to `activeCollection`. The store path is global and may belong to the previously viewed collection or workspace. In that case no `listCollections()` lookup occurs, and the newly created `git:${activeCollection}` tab permanently receives the wrong non-empty path.

The repair logic at lines 41-49 only replaces an existing tab when its path is empty; it does not repair a wrong non-empty path. `openTab` also only focuses an existing tab with the same ID and does not update its payload (`src/stores/pane-store.ts:166-175`).

**Root cause**

A path-only cache is reused as though it were keyed by collection name.

**Recommended fix**

Always resolve the active collection's path from the collection summary or maintain an explicit `collectionName -> path` mapping. Include the absolute path in the tab ID (`git:${path}`) or provide a pane-store action that updates/focuses an existing Git tab atomically. Re-check that the active collection is still the intended target after awaited lookup.

**Focused tests**

- Seed the Git store with A's path, set active collection to B, invoke `openGitPanel`, and assert the tab uses B's path.
- Existing B tab with a wrong non-empty path is repaired or replaced.
- Switch active collection while `listCollections()` is pending; assert no stale tab is opened for the wrong context.

### F-05 — Credential dialog can retain and apply secrets from a previous workspace

- **Severity:** High
- **Confidence:** High
- **Refs:**
  - `src/components/git/GitCredentialsDialog.tsx:27-38` — secret fields live in component-local state
  - `src/components/git/GitCredentialsDialog.tsx:40-85` — async loads have no cancellation/version guard and do not clear all fields on open/workspace change
  - `src/components/git/GitCredentialsDialog.tsx:105-130` — current fields are persisted under the current workspace ID
  - `src/stores/git-store.ts:140-150` — credential load uses whichever workspace is globally active when the await is reached
  - `src/stores/git-store.ts:552-609` — identity setup and retries are also global and unscoped

**Impact**

When the workspace changes or the dialog reopens, fields such as password, token, passphrase, username, and key path are not cleared before loading. If the new workspace has no saved credentials, stale values from the previous workspace remain available and can be saved under the new workspace. Older `loadGitCredentials`, key-list, or default-key promises can also resolve after a newer workspace change and overwrite the newer form.

The store's SSH identity lookup captures one repository path but commits pending credentials and retries against whatever global path/operation exists later. Identity setup cancellation calls `activatePendingCredentials` (`src/components/git/GitPanel.tsx:107-118`), so “Cancel” still activates credentials and may retry a network operation; the UI wording does not communicate that semantic.

**Root cause**

Credential state is globally shared while persistence is workspace-scoped, and async form hydration has no workspace/request identity.

**Recommended fix**

Key in-memory credentials and pending operations by workspace/repository. Reset every secret field synchronously on open/workspace change. Capture `workspaceId` and a load generation, and ignore stale completions. Disable Connect while saving and return an explicit result. Rename the identity cancel action or make it truly cancel credential activation/retry.

**Focused tests**

- Open workspace A credentials, switch to B with no saved credentials, and assert all secret fields are empty.
- Resolve A's credential load after B's; assert B's form remains unchanged.
- Begin SSH identity setup for A, switch to B, confirm/cancel, and assert no operation runs against B.
- Verify Cancel does not retry unless that behavior is explicitly presented and accepted.

### F-06 — Repository loading leaves stale state and hides initialization errors

- **Severity:** High
- **Confidence:** High
- **Refs:**
  - `src/stores/git-store.ts:133-163`
  - `src/components/git/GitPanel.tsx:43-94`, `src/components/git/GitPanel.tsx:173-209`
  - `src/stores/git-store.ts:115-131`

**Impact**

Starting a load changes only `collectionPath`, `loading`, and `error`; status, branches, remotes, stashes, conflicts, log, credentials, and `isRepo` remain from the previous repository until individual results arrive. A non-repository result clears only `status`. A thrown `gitIsRepo` leaves old `isRepo` and old repository data in the store.

`GitPanel.checkAndLoad` awaits an action that catches its own errors, so its `catch` does not run. It then reads stale `store.isRepo` and can render either the normal Git UI or the “not a repository” initialization UI without showing the load error. Store `loading` is not consumed by production UI; `GitPanel` owns a duplicate local tri-state instead.

**Root cause**

Initialization is not atomic and state/error ownership is split between the panel and store.

**Recommended fix**

Use one load state such as `{status: 'idle' | 'loading' | 'ready' | 'not-repo' | 'error', path, error}`. Clear or replace all path-scoped data atomically when path changes. Return/rethrow initialization failures and render a retryable error state distinct from “not a repo.”

**Focused tests**

- Load repo A, then a non-repo B; assert every A-scoped collection is cleared.
- Make `gitIsRepo(B)` reject; assert an error/retry UI is rendered, not Initialize Git and not A's data.
- Assert the skeleton is driven by the store load state and cannot disagree with `store.isRepo`.

### F-07 — Refresh/invalidation is incomplete, and open detail views remain stale

- **Severity:** High
- **Confidence:** High
- **Refs:**
  - `src/stores/git-store.ts:190-214` — conflict resolution/abort refresh lists but do not close the old resolver
  - `src/stores/git-store.ts:300-310` — commit refreshes status/log
  - `src/stores/git-store.ts:453-513` — branch actions inconsistently refresh derived data
  - `src/stores/git-store.ts:611-689` — network actions do not refresh all affected data
  - `src/components/git/GitPanel.tsx:120-137`, `src/components/git/GitPanel.tsx:159-171`
  - `src/components/git/DiffViewForFile.tsx:19-50`
  - `src/components/git/DiffViewer.tsx:41-42`, `src/components/git/DiffViewer.tsx:70-87`

**Impact**

- Pull changes history but does not refresh `commitLog`; merge can create conflicts but does not refresh `conflicts`; branch changes rely on view navigation to trigger some later refreshes.
- Resolved/aborted conflict views stay open with old content and active action buttons.
- A selected file is stored as a `FileStatus` snapshot in `rightPanel`. Status refreshes do not replace that snapshot, so stage/unstage/discard or file-watcher updates leave the displayed diff stale.
- `DiffViewer` copies `initialDiffState` into local state once and never synchronizes subsequent prop updates. Even when `DiffViewForFile` fetches new contents for the same `path:staged` key, the viewer can continue displaying its old local copy.
- Commit diff failures are silently ignored (`src/components/git/GitPanel.tsx:120-126`), leaving no loading or error feedback.

**Root cause**

There is no declared invalidation graph for each mutation, and right-panel navigation stores server-derived snapshots rather than stable IDs/paths.

**Recommended fix**

Centralize invalidation by operation. Store only stable identifiers in navigation state (`filePath`, staged mode, commit OID), derive current data from repository state, and explicitly close/redirect resolved conflict views. Make `DiffViewer` controlled, or synchronize local state when the identifying prop/revision changes. Add loading/error states for commit diff and toggle requests.

**Focused tests**

- Pull while commit history is open and assert history refreshes.
- Resolve a conflict and assert the resolver closes or becomes a completed state.
- Stage the currently selected file and assert the header/content switches to the correct current diff.
- Update `DiffViewer` props for the same path/mode and assert rendered content changes.
- Failed commit-diff load renders an actionable error.

### F-08 — Collection-change subscription can leak and is not repository-filtered

- **Severity:** Medium
- **Confidence:** High
- **Refs:** `src/components/git/GitPanel.tsx:139-157`

**Impact**

If the effect cleans up before `onCollectionChanged(...).then(...)` resolves, `unlisten` is still undefined during cleanup; the later listener is never removed. The callback also does not filter events by the panel's collection/path. Every mounted repo panel can schedule a refresh, but `refreshStatus` then targets the singleton store's current path rather than the panel path.

**Root cause**

Async listener setup lacks a cancelled flag, and callback scope is global.

**Recommended fix**

Use the cancellation pattern already present in `WorkspaceOverviewTab`: if registration resolves after cleanup, immediately invoke the returned unlisten function. Filter events to the repository represented by the panel and call a path-explicit refresh.

**Focused tests**

- Unmount before listener registration resolves and assert the eventual unlisten function is called.
- Emit a change for repo A while repo B is shown and assert B is not refreshed.

### F-09 — Loading and error state is fragmented and permits overlapping operations

- **Severity:** Medium
- **Confidence:** High
- **Refs:**
  - `src/stores/git-store.ts:55-56`, `src/stores/git-store.ts:123-124` — one load boolean and one global error
  - `src/components/git/GitLandingPanel.tsx:46-51`
  - `src/components/git/GitCommitForm.tsx:10-12`
  - `src/components/git/GitStashSection.tsx:40-45`
  - `src/components/git/GitRemotesDialog.tsx:37-53`
  - `src/components/git/BranchSelector.tsx:41-95`
  - `src/components/git/ConflictResolver.tsx:21-31`

**Impact**

Only panel initialization uses store `loading`, and production components do not subscribe to it. Components independently track some operations but not others. Branch, file, conflict, and remote controls remain enabled while requests are running, enabling duplicate or conflicting mutations. A single global `error` can be displayed in unrelated views, overwritten by a refresh error, or consumed as another action's result.

**Root cause**

Operation state is neither centralized nor scoped by repository/action/resource.

**Recommended fix**

Track operation state by path and operation key, for example `operations.stage`, `operations.pull`, and `operations.remoteUpdate`. Provide action-local results and reserve visible error state for scoped banners/toasts. Disable or serialize conflicting operations.

**Focused tests**

- Double-click stage/commit/branch/remote controls and assert only one IPC call runs.
- A refresh failure must not replace a more relevant mutation error.
- An error in stash view must not appear as a network error on the landing view for another path.

### F-10 — Full-store Zustand subscriptions violate project rules and amplify rerenders

- **Severity:** Medium
- **Confidence:** High
- **Rule:** `CLAUDE.md:91` — “Zustand: never fully destructure store state at component top level.”
- **Refs:**
  - `src/components/git/BranchSelector.tsx:16-24`
  - `src/components/git/ConflictResolver.tsx:18`
  - `src/components/git/GitCommitForm.tsx:13`
  - `src/components/git/GitCommitLog.tsx:28`
  - `src/components/git/GitCredentialsDialog.tsx:28`
  - `src/components/git/GitFileList.tsx:26-35`
  - `src/components/git/GitLandingPanel.tsx:33-44`
  - `src/components/git/GitPanel.tsx:52-66`
  - `src/components/git/GitRemotesDialog.tsx:16`
  - `src/components/git/GitStashSection.tsx:47-58`

**Impact**

Every listed component subscribes to the entire Git store. Any status, error, credential, dialog, stash, branch, log, or loading update rerenders most of the Git tree, including heavy diff/Monaco-adjacent boundaries and large lists. This also obscures actual state dependencies.

**Root cause**

Calling `useGitStore()` without selectors.

**Recommended fix**

Use narrow selectors per value/action, combining with `useShallow` only when a grouped selection is genuinely useful. Keep imperative reads (`getState`) only for event-time freshness, not as a substitute for explicit action results.

**Focused tests**

- Use render counters around `GitCommitLog` and `GitStashSection`; changing unrelated credential-dialog state should not rerender them.
- Add lint/static enforcement if available for zero-argument Zustand hook calls.

### F-11 — Raw interactive controls violate the shadcn-only hard rule

- **Severity:** Medium
- **Confidence:** High
- **Rule:** `CLAUDE.md:87` — no raw `<button>`, `<input>`, `<dialog>`, `<select>`, or `<form>`.
- **Refs:**
  - `src/components/git/BranchSelector.tsx:200`
  - `src/components/git/CommitDiffView.tsx:56`
  - `src/components/git/ConflictResolver.tsx:71`, `src/components/git/ConflictResolver.tsx:115`
  - `src/components/git/GitCloneDialog.tsx:191`
  - `src/components/git/GitCommitLog.tsx:63`
  - `src/components/git/GitLandingPanel.tsx:296`
  - `src/components/git/GitStashSection.tsx:194`
  - Git-relevant tab close control: `src/components/panes/TabItem.tsx:107`

**Impact**

The controls bypass shared styling, focus behavior, sizing, accessibility conventions, and project policy. The stash checkbox is specifically a raw `<input>`.

**Root cause**

Nested-interactive row designs led to ad hoc raw controls and role-based rows.

**Recommended fix**

Use `Button` and the shadcn `Checkbox`. For rows containing secondary actions, separate the primary row action from an adjacent action group rather than nesting controls or emulating a button with a `<div role="button">`.

**Focused tests**

- Static check over the scoped files rejects forbidden raw control tags.
- Keyboard tests cover primary row actions and secondary actions independently.

### F-12 — Keyboard and screen-reader behavior is incomplete

- **Severity:** Medium
- **Confidence:** High
- **Refs:**
  - Role-button rows: `src/components/git/BranchSelector.tsx:133-192`, `src/components/git/GitFileList.tsx:102-139`, `src/components/git/GitFileList.tsx:182-254`, `src/components/git/GitCommitLog.tsx:48-91`
  - Hover-only stash selection: `src/components/git/GitStashSection.tsx:181-205`
  - Unlabelled icon controls: `src/components/git/BranchSelector.tsx:159-185`, `src/components/git/GitFileList.tsx:87-94`, `src/components/git/GitFileList.tsx:221-247`, `src/components/git/GitRemotesDialog.tsx:116-131`, `src/components/git/GitRemotesDialog.tsx:155-173`, `src/components/git/GitStashSection.tsx:296-299`, `src/components/git/GitStashSection.tsx:362-370`
  - Placeholder-only/unassociated fields: `src/components/git/GitCloneDialog.tsx:230-251`, `src/components/git/GitRemotesDialog.tsx:100-115`, `src/components/git/GitRemotesDialog.tsx:183-201`, `src/components/git/GitStashSection.tsx:136-145`
  - Status letters without an accessible label: `src/components/git/GitStatusBadge.tsx:9-18`

**Impact**

- Space handlers on role-button rows do not consistently call `preventDefault`, so activation can also scroll the page.
- Stash multi-selection cannot be initiated reliably from the keyboard because the checkbox only appears on pointer hover or after selection has already begun; it also lacks an accessible label.
- Many icon-only buttons rely on visual tooltips and have no `aria-label`.
- Several inputs have only placeholders or labels not connected with `htmlFor`/`id`.
- Error banners generally lack `role="alert"`/live-region behavior; only the credential save error does this correctly.
- Loading indicators often lack a programmatic status message.

**Root cause**

Rows were optimized for compact pointer interaction without a complete keyboard/focus model.

**Recommended fix**

Use semantic primitives with explicit accessible names, make selection controls always keyboard-reachable, add `aria-live`/`role="alert"` for operation feedback, and test tab order plus Enter/Space behavior. Add `aria-label` such as “Modified” to status markers while retaining the visible letter.

**Focused tests**

- `userEvent.tab()` can reach and toggle the first stash checkbox without hover.
- Space activates a row once and does not scroll.
- Axe/accessibility-name assertions for all icon-only buttons and form controls.
- Errors and loading completion are announced.

### F-13 — Component boundaries duplicate ownership and encode backend workflows in presentation components

- **Severity:** Medium
- **Confidence:** High
- **Refs:**
  - Duplicate repo state: `src/components/git/GitPanel.tsx:43-44`, `src/components/git/GitPanel.tsx:52-66`
  - Direct IPC beside store actions: `src/components/git/GitPanel.tsx:26`, `src/components/git/GitCommitForm.tsx:5`, `src/components/git/DiffViewer.tsx:6`, `src/components/git/GitCloneDialog.tsx:15-21`
  - Credential retry orchestration: `src/components/git/GitCloneDialog.tsx:97-135`
  - Multi-step Git orchestration: `src/components/git/GitLandingPanel.tsx:92-173`

**Impact**

`GitPanel` owns a local `isRepo` while also subscribing to store `isRepo` and `loading`. Identity, clone, diff retrieval, stash-before-pull, fetch-before-push, and credential retry workflows are split across store and components. This duplication caused the double clone and swallowed-failure sequencing issues and makes behavior hard to test without rendering large components.

**Root cause**

The store is simultaneously a repository cache, action service, dialog coordinator, and global notification channel, while components still call IPC for adjacent workflows.

**Recommended fix**

Define a repository-scoped controller/service boundary with explicit operation results. Keep ephemeral presentation state (popover open, selected row, editor mode) local, but put multi-step Git workflows and invalidation in testable actions/hooks. Keep dialogs controlled by their owning workflow rather than global booleans where possible.

**Focused tests**

- Unit-test stash/pull/pop and fetch/push workflows without rendering.
- Assert one authoritative repository load state drives panel rendering.

### F-14 — Workspace Git tab conflates query loading/error with “no path”

- **Severity:** Medium
- **Confidence:** High
- **Refs:** `src/components/workspace/WorkspaceGitTab.tsx:9-25`, `src/lib/queries/workspace-queries.ts:28-33`

**Impact**

`useWorkspaces()` defaults data to `[]`, so initial loading and query failure both render “No workspace path configured.” The component ignores query `isLoading` and `error`. Its fallback `(workspaceId || activeWorkspaceId)` is effectively dead when a required non-empty `workspaceId` prop is supplied, and fallback display name “Collection” is misleading for a workspace repository.

**Root cause**

Query lifecycle is collapsed into data absence.

**Recommended fix**

Render explicit loading, error/retry, missing-workspace, and missing-path states. Treat `workspaceId` as authoritative or make it optional in the type if fallback behavior is intended.

**Focused tests**

- Loading renders a skeleton/status, rejection renders retryable error, missing ID renders not-found, and an actual empty path renders the configuration message.

### F-15 — Diff mode and toggle requests have weak validation/error semantics

- **Severity:** Low
- **Confidence:** High
- **Refs:** `src/components/git/DiffViewer.tsx:60-68`, `src/components/git/DiffViewer.tsx:70-87`, `src/components/git/DiffHeader.tsx:29-55`

**Impact**

The local-storage value is cast without validating it against `text | visual`. Stage/working diff toggles have no loading state and silently ignore errors, so rapid toggles can resolve out of order and the user receives no feedback. The comment says visual mode is for “JSON request files,” while the actual gate is `.yml` (`src/components/git/DiffViewer.tsx:89-90`).

**Root cause**

The toggle is treated as cheap local UI state even though it performs async IPC.

**Recommended fix**

Validate persisted mode, track a toggle request ID or abort stale requests, disable the control while loading, and expose errors. Correct the comment or file-type gate.

**Focused tests**

- Invalid persisted mode falls back to `text`.
- Toggle working → staged → working with reversed response order; final content remains working.
- Failed toggle keeps the previous selected mode and announces an error.

---

## Store action-to-caller trace

The table lists production callers in the reviewed frontend. Internal store-to-store refresh calls are noted separately.

| Store action | Production caller(s) | Review note |
|---|---|---|
| `setCollection` | `GitPanel.tsx:78` | Global path race; action catches errors, making caller `catch` ineffective. |
| `refreshStatus` | `GitPanel.tsx:149`; `GitFileList.tsx:71`; many store mutations | Re-reads global path; no loading; errors are global. |
| `refreshConflicts` | `GitFileList.tsx:65`; internally after resolve/abort/pull | Failure is silently converted to `[]`; can make conflicts appear resolved/unavailable. |
| `refreshStashes` | `GitPanel.tsx:136`; internally after stash actions and initialization | Path-global; opening stash view triggers another fetch after initialization. |
| `refreshBranches` | No direct component caller; internal only | Public surface could be private/internal after refactor. |
| `refreshRemotes` | `GitRemotesDialog.tsx:27`; internally after remote actions and initialization | Dialog has no loading/error distinction; duplicates initial load. |
| `refreshLog` | `GitPanel.tsx:131`; `GitCommitLog.tsx:34`; internally after commit | Pull does not invalidate log. |
| `resolveConflict` | `ConflictResolver.tsx:30` | No explicit result/loading; resolved view remains open. |
| `abortMerge` | `ConflictResolver.tsx:22` | No explicit result/loading; stale resolver remains. |
| `stageFiles` | `GitFileList.tsx:243` | Fire-and-forget at call site; selected diff remains stale. |
| `unstageFiles` | `GitFileList.tsx:129` | Same. |
| `discardFiles` | `GitFileList.tsx:50`, `GitFileList.tsx:227` | Single-file discard has no confirmation; caller does not await. |
| `commitChanges` | `GitCommitForm.tsx:20` | Failure resolves and message is cleared. |
| `stageAll` | `GitFileList.tsx:56` | Caller does not await; action pre-refresh can fail yet stale status may still be used. |
| `unstageAll` | `GitFileList.tsx:61` | Caller does not await; reads cached status without pre-refresh. |
| `saveStash` | `GitStashSection.tsx:67`; `GitLandingPanel.tsx:96` | Shared-error inference; pull workflow continues after failure. |
| `popStash` | `GitStashSection.tsx:302`; `GitLandingPanel.tsx:105` | Landing can pop after failed pull due swallowed errors. |
| `applyStash` | `GitStashSection.tsx:309` | No per-row loading/result. |
| `dropStash` | `GitStashSection.tsx:317` | Destructive action has no confirmation/result feedback. |
| `applyStashMany` | `GitStashSection.tsx:92` | Partial success encoded only in global error string. |
| `popStashMany` | `GitStashSection.tsx:103` | Same. |
| `dropStashMany` | `GitStashSection.tsx:114` | Same; destructive batch has no confirmation. |
| `switchBranch` | `BranchSelector.tsx:57` | Success inferred by comparing global error strings. |
| `checkoutRemoteBranch` | `BranchSelector.tsx:69` | Same. |
| `createBranch` | `BranchSelector.tsx:45` | Same; stale identical error can clear input as if successful. |
| `deleteBranch` | `BranchSelector.tsx:181` | Fire-and-forget, no confirmation, no inline result handling. |
| `mergeBranch` | `BranchSelector.tsx:85` | Conflict detection parses error text rather than typed result/state. |
| `addRemote` | `GitRemotesDialog.tsx:38` | Failed add still clears fields. |
| `removeRemote` | `GitRemotesDialog.tsx:51` | Failed remove exits confirmation. |
| `setRemoteUrl` | `GitRemotesDialog.tsx:45` | Failed save exits edit mode. |
| `setCredentials` | `GitCredentialsDialog.tsx:129` | Synchronous API launches async identity/retry work with no completion result. |
| `setShowCredentialsDialog` | `GitCredentialsDialog.tsx:133`; `GitLandingPanel.tsx:221`; imperative use in `GitCloneDialog.tsx:125` | Global dialog ownership couples unrelated clone/network workflows. |
| `clearPendingNetworkOp` | **No production caller** | Unused public action; remove or use in an explicit cancel path. |
| `activatePendingCredentials` | `GitPanel.tsx:113`, `GitPanel.tsx:117` | Both confirm and cancel activate credentials; semantic mismatch. |
| `push` | `GitLandingPanel.tsx:128`, `:141`, `:159`, `:169` | No caller supplies `remote`; optional remote feature is unused. Failure is swallowed. |
| `pull` | `GitLandingPanel.tsx:72`, `:85`, `:97`, `:118` | No caller supplies `remote`; failure is swallowed. |
| `fetch` | `GitLandingPanel.tsx:57`, `:62`, `:151` | No caller supplies `remote`; result payload is discarded and failure is swallowed. |
| `clearError` | `ConflictResolver.tsx:74`, `:118`; `GitLandingPanel.tsx:299`; `GitStashSection.tsx:64`, `:90`, `:101`, `:112` | Clears one global channel shared by all views/paths. |
| `reset` | **No production caller** | Used only by tests; repository/workspace switches depend on `setCollection` instead, which does not fully reset stale state. |
| `initRepo` | `GitPanel.tsx:186` | Failure resolves; panel then reads potentially stale `isRepo`. |
| `hasConflicts` | `GitLandingPanel.tsx:101`, selector at `:178` | Method-style derived state is awkward; use a selector over path-scoped status. |

### Other unused or mismatched pane functionality

- `PaneState.openDiffTab` and `PaneState.openConflictTab` are declared and implemented at `src/stores/pane-store.ts:99-105` and `src/stores/pane-store.ts:363-385`, but have no production callers. Git now renders diffs/conflicts inside `GitPanel`, so these actions and the corresponding `DiffTab`/`ConflictTab` route should either be deliberately supported and tested or removed after confirming no external use.
- `GitState.loading` is set/reset in the store but has no production consumer; `GitPanel` duplicates loading with local `isRepo: null`.
- `push(remote?)`, `pull(remote?)`, and `fetch(remote?)` expose remote selection, but all callers omit it. The fallback `get().remotes[0]?.name` can be `undefined` even though the Tauri API requires `remote: string` (`src/lib/tauri-api.ts:750-757`). The UI should block with “No remote configured” or require a selected remote.
- `GitTab.collectionPath` is required by type (`src/types/pane-types.ts:77-81`) but `openGitPanel` intentionally creates `''` when lookup fails (`src/components/layout/GitToolbarButton.tsx:28-35`, `:52-58`). This converts a load failure into a path value and later into misleading repo state. Model unresolved/error explicitly instead.

---

## Duplicate logic and boundary observations

1. **Relative time formatting is duplicated** in `GitCommitLog.tsx:8-21` and `GitStashSection.tsx:26-37`, with different behavior after 30 days. Extract one tested formatter if the product intends consistency.
2. **Global error banners and dismiss controls are duplicated** in `ConflictResolver.tsx:67-80`, `ConflictResolver.tsx:111-124`, `GitLandingPanel.tsx:291-305`, and `GitStashSection.tsx:158-164`. A shared alert component would also eliminate raw dismiss buttons and normalize live-region behavior.
3. **Repository initialization state is duplicated** between `GitPanel` local `isRepo` and store `isRepo/loading`.
4. **Credential orchestration is split** between `GitCredentialsDialog`, `GitCloneDialog`, `GitPanel`, and store pending-operation state.
5. **Diff loading occurs in two layers:** `DiffViewForFile` loads the initial diff, while `DiffViewer` independently reloads staged/working variants. A single controlled data hook would simplify race and error handling.
6. **Conflict detection is duplicated:** `GitPanel` directly scans `status.files` (`GitPanel.tsx:67-69`) while `GitLandingPanel` calls the store method (`GitLandingPanel.tsx:178`). Prefer one selector.

---

## Recommended implementation order

1. **Stop wrong-path operations:** make repository identity explicit/keyed and add stale-result guards (F-01, F-04, F-06).
2. **Fix clone execution ownership immediately** (F-02).
3. **Introduce explicit action results and repair compound workflows** (F-03).
4. **Scope credentials and pending operations by workspace/repository** (F-05).
5. **Define mutation invalidation and controlled detail views** (F-07, F-08).
6. **Add operation-scoped loading/errors and selector-based subscriptions** (F-09, F-10, F-13).
7. **Resolve raw controls and accessibility gaps** (F-11, F-12).
8. **Clean up unused/mismatched APIs and low-priority diff behavior** (F-14, F-15).

## Minimum regression suite to add

A focused suite should include:

- Store/controller race tests with deferred promises for A → B path switches.
- `GitCloneDialog` tests proving exactly-once clone behavior.
- `GitLandingPanel` workflow tests for failed fetch, stash, pull, and pop.
- `GitCommitForm`, `BranchSelector`, and `GitRemotesDialog` failure-preservation tests.
- Credential hydration race tests across workspace IDs.
- Split-pane integration test proving actions target the panel's own path.
- Diff prop/update and out-of-order toggle tests.
- Event-listener cleanup test.
- Keyboard/accessible-name tests for rows, stash selection, icon buttons, errors, and loading.
- Static tests/lint rules for forbidden raw controls and zero-argument Zustand subscriptions.

## Validation performed

- `yarn tsc --noEmit` — passed.
- `yarn test src/stores/__tests__/git-store.test.ts` — passed, 53/53 tests.

These passing checks do not cover the critical races and caller-result mismatches above; most require deferred-promise and component/integration tests rather than additional happy-path store assertions.
