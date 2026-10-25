# Flow Request Ref Rewrite, Plan 4 (UI and docs) Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The user sees the feature: open flow tabs follow a move, a toast names flows that were not updated, the delete dialog lists the flows that use the item, and the docs and roadmap are current.

**Architecture:** An app-level hook applies the `flow-request-paths-rewritten` event to the pane store and the flow query cache. The sidebar's delete dialog calls the lookup command. A docs task closes F-01 in the roadmap.

**Tech Stack:** React, TypeScript, TanStack Query, sonner, Vitest, Markdown.

**Spec:** `docs/superpowers/specs/2026-10-18-flow-request-ref-rewrite-design.md`

## Global Constraints

- Plans 1 to 3 are merged: the event, `onFlowRequestPathsRewritten`, `flowsUsingRequest` and `rewriteFlowRequestPaths` exist.
- The line in the dialog reads `Used by 1 flow: A.` for one flow and `Used by 2 flows: A, B.` for more. The toast reads `Flows not updated: A (message); B (message)`.
- The Delete button stays enabled whatever the lookup returns.
- shadcn primitives and lucide icons only. No full destructure of Zustand state.
- Docs go in `.claude/`, and the project `CLAUDE.md` only gets one reference line.
- Frontend checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run`.

## Review Focus

1. An event with only `failed` entries shows the toast and touches no tab and no query (test `an event with only failures shows the toast and patches nothing`).
2. A lookup that rejects leaves the dialog usable and shows no list (test `delete dialog stays usable when the lookup fails`).
3. A result that arrives after the user closed or changed the dialog is ignored (test `delete dialog ignores a late lookup result`).
4. A folder delete lists flows too, with the folder path passed to the lookup (test `delete dialog for a folder lists the flows that use it`).
5. The hook unsubscribes on unmount, so a remount does not double-handle events (test `the hook unsubscribes on unmount`).

## File Structure

- Create `src/hooks/useFlowRequestPathEvents.ts`, modify `src/App.tsx`.
- Modify `src/components/layout/CollectionsSidebar.tsx`.
- Tests: `src/hooks/__tests__/useFlowRequestPathEvents.test.tsx`, `src/components/layout/__tests__/CollectionsSidebar.test.tsx`.
- Docs: `docs/roadmaps/flow-roadmap.md`, `docs/features/flow-request-refs.md` (new), `crates/rocket-flow/CLAUDE.md`, `CLAUDE.md`.

### Task 1: Event hook and toast

**Files:**
- Create: `src/hooks/useFlowRequestPathEvents.ts`
- Modify: `src/App.tsx` (hooks block, lines ~49-54)
- Test: `src/hooks/__tests__/useFlowRequestPathEvents.test.tsx`

**Interfaces:**
- Consumes: `onFlowRequestPathsRewritten`, `usePaneStore.getState().rewriteFlowRequestPaths`, `flowKeys.collection` (`src/lib/queries/flow-queries.ts`), `toast` from `sonner`.
- Produces: `export function useFlowRequestPathEvents(): void`.

- [ ] **Step 1: Read** `src/lib/queries/collection-trust-queries.ts:40-60` (hook and unlisten pattern), `src/App.tsx:40-60`, `src/hooks/useBackendLogs.ts` and `src/lib/process-env-gate.ts:44` (toast usage).
- [ ] **Step 2: Write the failing tests.** Mock `@/lib/tauri-api` so `onFlowRequestPathsRewritten` captures its handler and resolves an unlisten `vi.fn()`, mock `sonner` with `toast: { warning: vi.fn() }`, and render with `renderHook` inside a `QueryClientProvider` whose client has `invalidateQueries` spied. Reset the pane store in `beforeEach`. Tests:
  - `an event with flows patches open tabs and refreshes the flow list`: with a flow tab in `c` at `auth/login.yml`, the handler gets `{ collection: 'c', from: 'auth', to: 'identity', flows: ['A'], failed: [] }`. The tab reads `identity/login.yml`, `invalidateQueries` was called with `{ queryKey: flowKeys.collection('c') }`, `toast.warning` was not called.
  - `an event with only failures shows the toast and patches nothing`: `flows: []`, `failed: [{ flow: 'A', message: 'boom' }, { flow: 'B', message: 'moved out of the collection' }]`. `toast.warning` is called once with `Flows not updated: A (boom); B (moved out of the collection)`, `invalidateQueries` is not called and the tab is unchanged.
  - `an event with flows and failures does both`: both effects happen once.
  - `the hook unsubscribes on unmount`: after `unmount()` the unlisten function was called once.
- [ ] **Step 3: Run** `yarn test useFlowRequestPathEvents --run`. Expected: failure, module `@/hooks/useFlowRequestPathEvents` not found.
- [ ] **Step 4: Implement** the hook: one `useEffect` keyed on `queryClient` (from `useQueryClient`), the unlisten pattern of `useCollectionTrustEvents`. Per event: when `flows.length > 0` call `usePaneStore.getState().rewriteFlowRequestPaths(collection, from, to)` and `queryClient.invalidateQueries({ queryKey: flowKeys.collection(collection) })`. When `failed.length > 0` call `toast.warning` once with the text from Global Constraints. Call the hook in `App.tsx` right after `useCollectionTrustEvents()`.
- [ ] **Step 5: Run** `yarn test useFlowRequestPathEvents --run` (expected: 4 passed), `yarn tsc --noEmit` and `yarn check` (clean).

### Task 2: Flow list in the delete dialog

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx` (`deleteTarget` state ~85, dialog ~643-672)
- Test: `src/components/layout/__tests__/CollectionsSidebar.test.tsx`

**Interfaces:**
- Consumes: `flowsUsingRequest(collection, path)` from `@/lib/tauri-api`, `DeleteTarget` (`tree-utils.ts:12`).
- Produces: a line inside the delete dialog description, rendered only for a non-empty lookup result.

- [ ] **Step 1: Read** `CollectionsSidebar.tsx:80-150,640-675`, and the top of `CollectionsSidebar.test.tsx` (mock factory, wrapper) plus the `script delete` test (~line 90) for how the dialog is opened.
- [ ] **Step 2: Write the failing tests.** Add `flowsUsingRequest: vi.fn()` to the `@/lib/tauri-api` mock factory and `warning: vi.fn()` to the `sonner` mock. Open the dialog for a request row, and for a folder row, the way the `script delete` test does. Tests:
  - `delete dialog lists the flows that use the request`: `flowsUsingRequest` resolves `['Checkout','Smoke test']` and the text `Used by 2 flows: Checkout, Smoke test.` appears. A second case resolves `['Checkout']` and the text is `Used by 1 flow: Checkout.`.
  - `delete dialog for a folder lists the flows that use it`: `flowsUsingRequest` was called with `(collection, folderPath)` and the list appears.
  - `delete dialog stays usable when the lookup fails`: it rejects, no `Used by` text, the Delete button is enabled and clicking it calls `deleteRequest`.
  - `delete dialog shows nothing when no flow uses the request`: it resolves `[]`, no `Used by` text.
  - `delete dialog ignores a late lookup result`: the lookup resolves only after the dialog was cancelled and a different target was opened. The first result does not appear.
  - `delete dialog does not call the lookup for a flow or a script target`: opening those targets leaves `flowsUsingRequest` uncalled.
- [ ] **Step 3: Run** `yarn test CollectionsSidebar --run`. Expected: the new tests fail, no `Used by` text appears.
- [ ] **Step 4: Implement.** Add state `flowsUsingTarget: string[]` and an effect on `deleteTarget`: for type `request` or `folder` with a `path`, call `flowsUsingRequest(collection, path)`, store the result, ignore it when the effect was cleaned up, log a rejection with `console.error` and keep the list empty. Clear the list when the target changes or closes. Render the line inside `AlertDialogDescription` after the existing sentence, with the singular and plural forms from Global Constraints.
- [ ] **Step 5: Run** `yarn test CollectionsSidebar --run` (expected: all pass, the existing script and flow delete tests included), `yarn tsc --noEmit` and `yarn check` (clean).

### Task 3: Docs and roadmap

**Files:**
- Modify: `docs/roadmaps/flow-roadmap.md` (F-01 row ~line 27, new section at the end), `crates/rocket-flow/CLAUDE.md` (module map), `CLAUDE.md` (reference lines)
- Create: `docs/features/flow-request-refs.md`

**Interfaces:**
- Produces: F-01 marked `done`, three new follow-up rows, and a feature doc that the project `CLAUDE.md` links.

- [ ] **Step 1: Read** `docs/roadmaps/flow-roadmap.md` (the F-01 row, the maintain rules, the last section and the highest ID), `crates/rocket-flow/CLAUDE.md` (module map table) and the reference lines in `CLAUDE.md`.
- [ ] **Step 2: Write the docs.**
  - `docs/features/flow-request-refs.md`, under 60 lines: what is rewritten and when (same-collection move, folder prefix, whole segments), what is only reported (cross-collection move, delete), the pieces (`rocket-flow` `request_refs.rs`, `FlowReferenceUpdater`, the `CollectionService` hook, the event `flow-request-paths-rewritten`, `flows_using_request`, `rewriteFlowRequestPaths`, the hook, the dialog line), the limits (outside edits, unparseable flows, run cache, the instant before the tab patch) and where the tests live.
  - `docs/roadmaps/flow-roadmap.md`: set the F-01 status to `done` and put the series name `flow-request-ref-rewrite` in its Notes. Add a section `## Follow-ups from F-01 (2026-10-18)` with rows `F-133` (real-app check of move, folder rename, delete dialog and the toast with a flow open), `F-134` (show the dangling-path lint after a delete once `worktree-flow-tier2` merges, F-21) and `F-135` (rewrite or warn on changes made outside the app, such as a git pull), in the file's follow-up table layout. Verify first that `F-133` is the next free ID.
  - `crates/rocket-flow/CLAUDE.md`: one module-map row for `request_refs.rs`.
  - `CLAUDE.md`: one line `See docs/features/flow-request-refs.md for how flows follow request moves (rewrite, event, delete dialog).`
- [ ] **Step 3: Verify.** Run `grep -n "request_refs" crates/rocket-flow/CLAUDE.md`, `grep -n "flow-request-refs" CLAUDE.md` and `grep -nE "^\| F-0?1 \|.*\| done \|" docs/roadmaps/flow-roadmap.md`. Expected: one or more matches each. Run `grep -c "^| F-13[345] " docs/roadmaps/flow-roadmap.md`. Expected: `3`. Run `wc -l docs/features/flow-request-refs.md`. Expected: under 60.

Next plan: none
