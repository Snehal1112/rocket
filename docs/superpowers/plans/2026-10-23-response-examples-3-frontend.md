# Response Examples, Plan 3: Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user save a response as a named example from the response toolbar, see examples nested under their request in the sidebar, open one in a read-only tab, and press Try to get an editable temporary copy of its request.

**Architecture:** A pure `buildExampleSnapshot` turns the open request tab and its live response into the on-disk example shape (Plan 2 types). A `SaveExampleDialog` calls `addRequestExample`. The sidebar reads `exampleNames` from `RequestSummary` and nests rows under the request. A new `example` tab type renders the saved snapshot read-only. Try opens a normal request tab flagged `transient`, so it carries the parent's scope but can never be saved over the parent.

**Tech Stack:** React, TypeScript, Zustand (`src/stores/pane-store.ts`), shadcn/ui, lucide-react, Monaco for body viewers, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-response-examples-design.md`

## Global Constraints

- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), lucide-react icons only, Monaco for multi-line viewers, narrow Zustand selectors (never destructure a whole store).
- Examples are HTTP only. The Save button is enabled only when the tab is an HTTP request with `tab.source` set and a response exists. Otherwise it is disabled with a title that says why ("Save the request first", "Send the request first").
- The request side of an example keeps `{{vars}}` exactly as in the editor (unresolved). Disabled headers, params and form rows are not saved. (Orchestrator's call.)
- Body rules (orchestrator's call): `json`, `text`, `xml`, `sparql` modes save `{type, data: content}`. `formurlencoded` saves `{type: 'form-urlencoded', data: [{name, value}]}`. `formdata`, `binary` and `none` save no body. A response body over 512 KiB (524288 bytes of text) or `isBinary` saves `body: {type: 'binary', data: ''}`. The dialog tells the user when either rule dropped something.
- Response body `type` comes from the response `content-type` header: contains `json` gives `json`, `xml` gives `xml`, `html` gives `html`, anything else `text`.
- Example identity is the request path plus the example name.
- Saving an example must not mark the request tab dirty and must not call `save_request`.
- Run `yarn tsc --noEmit`, `yarn check` and `yarn test <pattern>` for what you touch.

## Review Focus

- Pressing Save as example on a tab whose request has unsaved edits stores the edited (sent) request and leaves the tab dirty state unchanged.
- A name already used on that request shows an inline error in the dialog, keeps it open, and writes nothing.
- A transient Try tab: Ctrl+S, the Save button and the close prompt never write to the parent request file, and closing it never asks to save.
- Renaming or deleting an example whose tab is open closes or retitles that tab. Deleting the parent request closes its example tabs.
- A request with zero examples shows no chevron. A request whose `exampleNames` has 50 entries renders without layout breakage.
- A response with `isBinary`, or a 1 MB text body, saves without a body and the dialog says so.
- An example written by hand with a missing `request`, `response`, `headers` or `body` opens without crashing.

---

### Task 1: Save as example (snapshot, dialog, button)

**Files:**
- Create: `src/lib/example-snapshot.ts`
- Create: `src/components/response/SaveExampleDialog.tsx`
- Modify: `src/components/response/ResponseBodyViewer.tsx` (add the toolbar button)
- Modify: `src/components/request/RequestPanel.tsx` (own the dialog, pass the callback and the disabled reason)
- Test: `src/lib/__tests__/example-snapshot.test.ts`, `src/components/response/__tests__/SaveExampleDialog.test.tsx`

**Interfaces:**
- Consumes (Plan 2): `addRequestExample(collection, requestPath, example: RequestExample): Promise<void>`, types `RequestExample`, `ExampleRequestSnapshot`, `ExampleResponseSnapshot` from `src/lib/tauri-api.ts`.
- Produces:
  - `export interface ExampleSnapshot { request: ExampleRequestSnapshot; response: ExampleResponseSnapshot; notes: string[] }` in `example-snapshot.ts`. `notes` lists what was dropped, as user-facing sentences.
  - `export function buildExampleSnapshot(request: RequestState, response: ResponseState): ExampleSnapshot`
  - `export function defaultExampleName(response: ResponseState): string` returns `"<status> <statusText>"` trimmed, for example `200 OK`, or just the status when the text is empty.
  - `SaveExampleDialog` props: `{ open: boolean; onOpenChange: (open: boolean) => void; collection: string; requestPath: string; snapshot: ExampleSnapshot; defaultName: string; existingNames: string[] }`.
  - `ResponseBodyViewer` gains optional props `onSaveAsExample?: () => void` and `saveExampleDisabledReason?: string | null`. The button sits in the existing toolbar group, before the copy button, icon `BookmarkPlus` from lucide-react, `aria-label='Save as example'`.

- [ ] **Step 1: Write the failing tests for `buildExampleSnapshot`:**
  - `keeps_templates_and_skips_disabled_rows`: request with url `{{base}}/login`, one enabled and one disabled header, one disabled query param. Snapshot headers has only the enabled one, url keeps `{{base}}`.
  - `json_body_maps_to_type_and_data`: mode `json`, content `{"a":1}` gives `body: {type: 'json', data: '{"a":1}'}`.
  - `form_urlencoded_maps_to_a_list`: two rows give `data: [{name, value}, ...]`, `type: 'form-urlencoded'`.
  - `formdata_and_binary_bodies_are_dropped_with_a_note`: no `body`, one note.
  - `response_body_type_comes_from_content_type`: `application/problem+json` gives `json`, `text/html; charset=utf-8` gives `html`, `application/octet-stream` with `isBinary` gives binary.
  - `big_response_body_is_dropped_with_a_note`: 600000-character body gives `body: {type: 'binary', data: ''}` and a note.
  - `status_and_headers_are_copied`: `status`, `statusText`, all headers as `{name, value}`.
  - `default_name_uses_status_and_text`: `200 OK`; empty text gives `404`.

- [ ] **Step 2: Run to verify failure**

Run: `yarn test example-snapshot`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement `buildExampleSnapshot` and `defaultExampleName`** in `example-snapshot.ts` with the exact rules in Global Constraints. Query params: use the editor's `queryParams` rows as `{name, value, type: 'query'}` and path params as `type: 'path'`; check how `request-to-oc` splits the URL query from `queryParams` in `crates/rocket-infra/src/conversions/request.rs` and `src/lib/pane-utils.ts` (`mapApiRequestToState`) and store the URL the same way the saved request stores it, so the two never disagree.

- [ ] **Step 4: Write the failing dialog tests** (React Testing Library, mock `@/lib/tauri-api`):
  - `shows_default_name_and_saves`: opens with `200 OK`, clicking Save calls `addRequestExample('shop', 'auth/login.yml', {name: '200 OK', request: ..., response: ...})` and then `onOpenChange(false)`.
  - `blocks_empty_and_duplicate_names`: an empty name disables Save. A name in `existingNames` (case-insensitive) shows `An example with this name already exists.` and disables Save.
  - `shows_backend_error_and_stays_open`: `addRequestExample` rejects with a message, the message shows, the dialog stays open.
  - `lists_notes`: a snapshot with two notes shows both.

- [ ] **Step 5: Implement `SaveExampleDialog`** with shadcn `Dialog`, `Input`, `Button`, `Label`. Enter submits. Trim the name. On success also call `toast.success('Example saved')` (`sonner`, as in `RequestNode.tsx`).

- [ ] **Step 6: Wire the toolbar button and `RequestPanel`.** In `RequestPanel.tsx`, build `snapshot` with `useMemo` from `tab.request` and `tab.response`, compute `existingNames` by calling `listRequestExamples` when the dialog opens, and set `saveExampleDisabledReason` to `null` only when `tab.request.requestType === 'http'`, `tab.source` is set and `tab.response` exists. Add a `ResponseBodyViewer` test: the button renders, is disabled with the reason as `title`, and calls the callback when enabled.

- [ ] **Step 7: Run to verify pass**

Run: `yarn test example-snapshot`, `yarn test SaveExampleDialog`, `yarn test ResponseBodyViewer`, `yarn tsc --noEmit`, `yarn check`
Expected: PASS.

### Task 2: Sidebar nesting, context menu and the read-only example tab

**Files:**
- Create: `src/components/collections/ExampleNode.tsx`
- Create: `src/components/examples/ExampleTabView.tsx`
- Modify: `src/components/collections/RequestNode.tsx` (nest examples)
- Modify: `src/types/pane-types.ts` (`ExampleTab`, `isExampleTab`, add to `Tab`)
- Modify: `src/stores/pane-store.ts` (`openExampleTab`, close and retitle handling)
- Modify: the tab renderer that switches on `tabType` (find it with `git grep -n "isRunnerTab" -- src` and follow the same pattern)
- Test: `src/components/collections/__tests__/ExampleNode.test.tsx`, `src/components/examples/__tests__/ExampleTabView.test.tsx`, `src/stores/__tests__/pane-store.example.test.ts`

**Interfaces:**
- Consumes (Plan 2): `listRequestExamples`, `renameRequestExample`, `deleteRequestExample`, `RequestSummary.exampleNames`.
- Produces:
  - `export interface ExampleTab extends BaseTab { tabType: 'example'; source: { collection: string; path: string }; exampleName: string }` (`isDirty` is always false) and `export function isExampleTab(tab: Tab): tab is ExampleTab`.
  - `openExampleTab: (collection: string, path: string, exampleName: string) => void` on the pane store. It reuses an already open tab with the same collection, path and name (activates it), else opens a new one titled with the example name.
  - `closeExampleTabs: (collection: string, path: string, exampleName?: string) => void` closes the matching tabs (all of a request's when the name is omitted).
  - `retitleExampleTab: (collection: string, path: string, oldName: string, newName: string) => void`.

- [ ] **Step 1: Write failing store tests** (`pane-store.example.test.ts`): `openExampleTab` twice with the same key leaves one tab. `closeExampleTabs` with a name closes only that one, without a name closes all of that request's. `retitleExampleTab` changes `exampleName` and `title`.

- [ ] **Step 2: Run to verify failure**

Run: `yarn test pane-store.example`
Expected: FAIL.

- [ ] **Step 3: Implement the type, the guard and the three store actions.** Follow how `openFolderTab` finds an existing tab (`findTabInTree`).

- [ ] **Step 4: Write failing sidebar tests** (`ExampleNode.test.tsx`, plus additions to `RequestNode.test.tsx`):
  - `request_with_examples_shows_a_chevron_and_nested_rows`: summary with `exampleNames: ['200 OK', '401']` renders both names under it when expanded. A summary without them shows no chevron.
  - `clicking_an_example_opens_its_tab`: calls `openExampleTab(collection, path, '401')`.
  - `rename_calls_the_command_and_retitles`: context menu Rename, type a new name, Enter calls `renameRequestExample` then `retitleExampleTab`. A rejected promise shows a toast and keeps the old name.
  - `delete_asks_for_confirmation_then_closes_tabs`: confirm calls `deleteRequestExample` then `closeExampleTabs`.
  - `deleting_a_request_closes_its_example_tabs`: in the existing delete path of `RequestNode`.

- [ ] **Step 5: Implement `ExampleNode` and the `RequestNode` nesting.** Row: lucide `FileText` icon, the name, the response status badge is optional (skip it: `exampleNames` carries names only). Use the existing `TreeItem` and `ContextMenu` primitives as `RequestNode` does. Expanded state is local state. After a successful save from Task 1 the sidebar refreshes through the existing `RequestSaved` event path; if the sidebar does not refresh on that event today, call the same collection reload the rename flow uses. Also call `closeExampleTabs(collection, path)` where `RequestNode` and its rename/move flow handle delete and move.

- [ ] **Step 6: Write failing view tests** (`ExampleTabView.test.tsx`, mock `listRequestExamples`):
  - `renders_request_and_response_panes`: shows method and URL, request tabs Params, Headers, Body, response tabs Response, Headers, and the status.
  - `missing_parts_do_not_crash`: an example with only a name renders empty states.
  - `shows_not_found_when_the_example_was_deleted_elsewhere`: the list no longer has the name.

- [ ] **Step 7: Implement `ExampleTabView`.** Load with `listRequestExamples(collection, path)` and pick by name; show read-only key-value tables for params and headers and a read-only Monaco view for bodies (see how `ResponseBodyViewer` mounts Monaco and reuse its language pick). Top bar: example name, method and status. The `Try` button is added in Task 3, not here. Refetch when a `RequestSaved` event for the same path arrives.

- [ ] **Step 8: Run to verify pass**

Run: `yarn test pane-store.example`, `yarn test ExampleNode`, `yarn test RequestNode`, `yarn test ExampleTabView`, `yarn tsc --noEmit`, `yarn check`
Expected: PASS.

### Task 3: Try (temporary editable copy) and the feature doc

**Files:**
- Create: `src/lib/example-to-request.ts`
- Modify: `src/types/pane-types.ts` (`transient?: boolean` on `BaseTab`)
- Modify: `src/stores/pane-store.ts` (`openTryTab`, guard the save and close paths)
- Modify: `src/components/examples/ExampleTabView.tsx` (the Try button)
- Modify: the request save paths found with `git grep -n "saveRequest\|updateRequest(" -- src/lib/auto-save.ts src/stores/pane-store.ts src/components/request/RequestPanel.tsx`
- Create: `docs/features/response-examples.md`
- Modify: `CLAUDE.md` (one link line), `docs/roadmaps/roadmap.md` (mark the item shipped if one exists)
- Test: `src/lib/__tests__/example-to-request.test.ts`, `src/stores/__tests__/pane-store.try.test.ts`

**Interfaces:**
- Consumes: Task 2's `ExampleTabView` and `isExampleTab`; `getRequest(collection, path)` and `mapApiRequestToState` for the parent's base state.
- Produces:
  - `export function exampleToRequestState(example: RequestExample, base: RequestState): RequestState` returns `base` with method, url, query and path params, headers and body replaced from the example's `request` snapshot. Missing parts keep the base value. Auth, scripts, settings, assertions and actions stay the parent's.
  - `openTryTab: (collection: string, path: string, example: RequestExample) => Promise<void>` on the pane store. It opens a `RequestTab` with `tabType: 'request'`, `source: { collection, path }` (so variables, inherited auth, scripts and the trust gate resolve as for the parent), `transient: true`, `isDirty: false`, `response: null`, and a title `Try: <example name>`. Every call opens a new tab.

- [ ] **Step 1: Write failing tests** for `exampleToRequestState`:
  - `replaces_request_parts_and_keeps_the_rest`: base has bearer auth and a script; the result keeps both and takes url, method, headers, params and body from the example.
  - `form_urlencoded_data_becomes_rows_and_text_bodies_become_content`.
  - `missing_parts_keep_base_values`.
  - `templates_are_kept`: `{{token}}` in a header value stays as written.

- [ ] **Step 2: Write failing store tests** (`pane-store.try.test.ts`):
  - `try_opens_a_transient_tab_with_the_parent_source`: tab has `transient: true`, `source` equals the parent's, `isDirty` false.
  - `try_twice_opens_two_tabs`.
  - `a_transient_tab_never_saves_to_the_parent`: calling the store's save action and the auto-save path for that tab does not call `saveRequest` (mock `@/lib/tauri-api` and assert it was not called). Editing the tab keeps `isDirty` false or the save entry points ignore it (pick one, and assert the user-visible result: no write, no close prompt).
  - `closing_a_transient_tab_does_not_prompt`.

- [ ] **Step 3: Run to verify failure**

Run: `yarn test example-to-request` and `yarn test pane-store.try`
Expected: FAIL.

- [ ] **Step 4: Implement `exampleToRequestState`, the `transient` flag and `openTryTab`.** Add `transient` guards at every save entry point found by the grep in Files (store `closeTab` save-before-close, auto-save, Ctrl+S handler, any "Save" button in `RequestPanel`). The Save button shows disabled with the title `Temporary tab: not saved`. Show a small `Badge` "Temporary" in the tab header or request bar. Whether a transient send is recorded in history is left as the existing send path does it (do not special-case); note the result in `docs/features/response-examples.md`.

- [ ] **Step 5: Add the Try button** (`Button`, lucide `Play`, label `Try`) to the `ExampleTabView` top bar. It loads the parent request with `getRequest`, maps it with `mapApiRequestToState`, calls `exampleToRequestState`, then `openTryTab`.

- [ ] **Step 6: Run to verify pass**

Run: `yarn test example-to-request`, `yarn test pane-store.try`, `yarn test ExampleTabView`, `yarn tsc --noEmit`, `yarn check`
Expected: PASS.

- [ ] **Step 7: Write `docs/features/response-examples.md`** (short): how to save and view an example, the snapshot shape, the limits (512 KiB response body, 1 MiB per example, 120-character names), where secrets are masked (commit only) and that exports leave examples out, how Try works (`transient`), and the manual test list from the spec. Add one line to `CLAUDE.md` under the other `.claude/` pointers and update `docs/roadmaps/roadmap.md` if it lists this item.

- [ ] **Step 8: Manual check in the real app** (`yarn tauri dev`, record the result in the doc): send a login request, save `200 OK`, see it nested in the sidebar, open it, press Try and send, rename and delete it, commit and confirm the token and refresh token read `[masked]` in the commit while the file on disk keeps them, export to Bruno and confirm examples are left out.
