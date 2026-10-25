# Duplicate Request Backend, Plan 2 (frontend and docs) Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The sidebar Duplicate action calls the backend command, shows an error toast on failure, and holds no copy logic of its own.

**Architecture:** `handleDuplicate` becomes a single `duplicateRequest(collection, path)` call. The `onDuplicate` callback loses its unused `name` argument in `RequestNode`, `FolderNode` and the sidebar. The roadmap records the work as done.

**Tech Stack:** React, TypeScript, sonner, Vitest, Markdown.

**Spec:** `docs/superpowers/specs/2026-10-19-duplicate-request-backend-design.md`

## Global Constraints

- Plan 1 is merged: the `duplicate_request` command exists.
- `handleDuplicate(collection, path)` calls `duplicateRequest` and nothing else. A failure calls `toast.error('Could not duplicate the request: <message>')` and `console.error`.
- The callback type is `(collection: string, path: string) => Promise<void>` everywhere. The menu entry stays HTTP only.
- shadcn primitives and lucide icons only. No full destructure of Zustand state.
- Frontend checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run`.

## Review Focus

1. A rejected command shows the toast and logs, and does not throw out of the handler (test `duplicate failure shows an error toast and logs`).
2. The handler no longer reads the collection or saves a request itself: neither `getCollection` nor `saveRequest` is called (test `duplicate calls duplicateRequest with the collection and the path`).
3. A request inside a folder passes its full path, not just the file name (test `duplicate passes the full path of a request in a folder`).
4. The Duplicate entry still appears only for HTTP requests (test `Duplicate is hidden for non-HTTP requests`).
5. The roadmap row exists once and uses the next free ID (checked by the docs verification step).

## File Structure

- Modify `src/lib/tauri-api.ts`: `duplicateRequest`.
- Modify `src/components/layout/CollectionsSidebar.tsx`: `handleDuplicate`.
- Modify `src/components/collections/RequestNode.tsx`, `src/components/collections/FolderNode.tsx`: callback type and call.
- Tests: `src/components/layout/__tests__/CollectionsSidebar.test.tsx`, `src/components/collections/__tests__/RequestNode.test.tsx`, `src/components/collections/__tests__/FolderNode.test.tsx`.
- Modify `docs/roadmaps/roadmap.md`.

### Task 1: API wrapper and sidebar handler

**Files:**
- Modify: `src/lib/tauri-api.ts` (next to `saveRequest`, ~923)
- Modify: `src/components/layout/CollectionsSidebar.tsx` (`handleDuplicate`, ~231-285)
- Modify: `src/components/collections/RequestNode.tsx` (callback type ~90, call ~357 and ~433), `src/components/collections/FolderNode.tsx` (type ~64)
- Test: `src/components/layout/__tests__/CollectionsSidebar.test.tsx`

**Interfaces:**
- Produces:
  - `export const duplicateRequest = (collection: string, path: string) => invoke<Request>('duplicate_request', { collection, path })`.
  - `handleDuplicate(collection: string, path: string): Promise<void>`, and the `onDuplicate` prop type `(collection: string, path: string) => Promise<void>` in `RequestNode`, `FolderNode` and the sidebar.

- [ ] **Step 1: Read** `CollectionsSidebar.tsx:226-290`, the `onDuplicate` use in `RequestNode.tsx:90,104,357,433` and `FolderNode.tsx:64,79,397,434`, and the top of `CollectionsSidebar.test.tsx` (mock factory, wrapper, the way the `delete dialog lists the flows that use the request` test opens a request row menu).
- [ ] **Step 2: Write the failing tests** in `CollectionsSidebar.test.tsx`. Add `duplicateRequest: vi.fn()` to the `@/lib/tauri-api` mock factory and make sure the `sonner` mock has `toast.error`. Open the request row menu as the delete-dialog test does and click **Duplicate**. Tests:
  - `duplicate calls duplicateRequest with the collection and the path`: `duplicateRequest` resolves, it was called once with `('c1','login.yml')`, and `getCollection` and `saveRequest` were not called by the handler.
  - `duplicate passes the full path of a request in a folder`: for a request in folder `auth`, it was called with `('c1','auth/login.yml')`.
  - `duplicate failure shows an error toast and logs`: `duplicateRequest` rejects with `new Error('boom')`, `toast.error` was called with `Could not duplicate the request: boom`, `console.error` was called, and nothing is thrown (spy on `console.error`).
- [ ] **Step 3: Run** `yarn test CollectionsSidebar --run`. Expected: the three new tests fail, `duplicateRequest` is never called.
- [ ] **Step 4: Implement.** Add the wrapper. Replace the body of `handleDuplicate` with a `try` that awaits `duplicateRequest(collection, path)` and a `catch` that calls `toast.error` with the message text (`err instanceof Error ? err.message : String(err)`) and `console.error('[CollectionsSidebar] duplicate failed:', err)`. Delete `findRequest`, the name picking and the `saveRequest` call, and remove imports that became unused. Change the callback type and the call sites in `RequestNode.tsx` and `FolderNode.tsx` to two arguments.
- [ ] **Step 5: Run** `yarn test CollectionsSidebar --run` (expected: all pass), `yarn tsc --noEmit` and `yarn check` (clean).

### Task 2: Node component tests

**Files:**
- Test: `src/components/collections/__tests__/RequestNode.test.tsx`, `src/components/collections/__tests__/FolderNode.test.tsx`
- Modify: the same two component files only when a test exposes a defect.

**Interfaces:**
- Consumes: the two-argument `onDuplicate` from Task 1.

- [ ] **Step 1: Read** `RequestNode.test.tsx` (how `onDuplicate={vi.fn()}` is passed, ~lines 58, 251, 264 and how the row menu is opened) and `FolderNode.test.tsx` (how it renders child requests and passes `onDuplicate`).
- [ ] **Step 2: Write the failing tests.**
  - `RequestNode Duplicate passes the collection and path`: render an HTTP request at path `auth/login.yml` in collection `c1`, open the menu, click **Duplicate**. `onDuplicate` was called once with `('c1','auth/login.yml')` and no third argument (`toHaveBeenCalledWith('c1','auth/login.yml')`).
  - `Duplicate is hidden for non-HTTP requests`: render a GraphQL item (use the `kind` the existing tests use for non-HTTP rows) and assert the menu has no **Duplicate** entry.
  - `FolderNode forwards onDuplicate to its child requests`: render a folder containing one HTTP request, trigger Duplicate on the child, and assert the folder's `onDuplicate` mock got `(collection, 'folder/child.yml')`.
- [ ] **Step 3: Run** `yarn test RequestNode FolderNode --run`. Expected: the first and third tests fail while the old three-argument call is still asserted elsewhere, otherwise they pass. If all pass from the start, change one expected path to a wrong value to see a red run, then restore it.
- [ ] **Step 4: Fix** any defect the tests expose, and update older assertions in these files that still expect the `name` argument.
- [ ] **Step 5: Run** `yarn test RequestNode FolderNode CollectionsSidebar --run` (expected: all pass), `yarn tsc --noEmit` and `yarn check` (clean).

### Task 3: Roadmap

**Files:**
- Modify: `docs/roadmaps/roadmap.md` (section `## 8b` and following, after the last row `R-63`)

**Interfaces:**
- Produces: row `R-64`, status `done`.

- [ ] **Step 1: Read** the end of `docs/roadmaps/roadmap.md` sections 8c and 8d to find the highest ID (the last row should be `R-63`) and the row layout.
- [ ] **Step 2: Add** one row `R-64` in the layout of its neighbours: "Duplicate request through the backend `duplicate_request` command (fresh uid, source by full path, unique name in the folder, no overwrite)", status `done`, Where `crates/rocket-app/src/collection_service.rs`, spec `docs/superpowers/specs/2026-10-19-duplicate-request-backend-design.md`, and What remains: the three follow-ups from the spec (non-HTTP kinds, ordering next to the source, duplicate a folder). If `R-64` is already taken, use the next free ID in the same section.
- [ ] **Step 3: Verify.** Run `grep -c "duplicate_request" docs/roadmaps/roadmap.md`. Expected: `1` or more, and the new row ID is unique: `grep -c "^| R-64 " docs/roadmaps/roadmap.md` prints `1`.

Next plan: none
