# Spec sync tab (read-only) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A collection with an OpenAPI source gets a "Spec sync" tab that shows the sync status and, when a check found changes, the New / Changed / Removed endpoints with a field-by-field detail. Nothing can be applied, skipped, checked or configured yet (Plan 9).

**Architecture:** One new read-only service method builds the same plan that `apply` would accept (`plan_with_links` with no links), and two thin Tauri commands return camelCase DTOs. The frontend adds typed API wrappers, a react-query hook that refreshes on the `openapi-sync-checked` event, and a new `spec-sync` section in the collection tab.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), React + TypeScript, react-query, shadcn/ui, lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md` (line 44: the tab; line 52: the notice). Series index: `00-plan-index.md` (carry-forward sections for Plans 6 and 7 list what the tab must handle).

## Global Constraints

- Tasks touching `rocket-app`, `rocket-collection`, `rocket-infra` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- Cargo: `-j4` and `-p <crate>`, never `--workspace`. No unwrap calls in production paths. camelCase renames on IPC DTOs only; `OpenApiLocalState`, `OpenApiSource` and the plan types stay un-serialized.
- shadcn/ui primitives and lucide-react icons only (no raw `<button>`, `<input>`); narrow Zustand selectors; no i18n layer exists, strings are inline English.
- No new write path: this plan adds no command that changes files, the keychain, the network or the scheduler state.
- The tab must never run `check` and must not make a network request.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. Gates: `cargo check -j4`, targeted `cargo test -j4 -p <crate> <name>`, `yarn tsc --noEmit`, `yarn check`, targeted `yarn test`.

## Review Focus

- A collection with no OpenAPI source: the tab is absent, the status command returns `None`, nothing errors.
- Source present, no pending copy (never changed since import): the plan command returns `pending: false`, the tab says "No changes found" with the last check time.
- Unreadable `.rocket/openapi-source.yml` or `openapi-local.json`: `status` returns an error, the tab shows "The sync state file cannot be read" with the message, not a spinner or an empty list.
- Pending copy whose hash no longer matches `pending_sha256` (replaced after the last check): the plan command fails with a Conflict; the tab shows "The pending spec changed, check again" (Plan 9 adds the button).
- The `openapi-sync-checked` event names a collection from the previous workspace right after a switch: the hook refreshes only the collection it is showing and ignores other names.
- A very large plan (thousands of Changed items): the list renders without freezing (a virtual list is not required; the detail pane renders only the selected item).

---

### Task 1: Read-only status and plan commands

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/openapi_sync_apply.rs` (new `plan`)
- Create: `crates/rocket-app/src/openapi_sync_view.rs` (view structs, no serde), `src-tauri/src/commands/openapi_sync.rs` (commands and DTOs)
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (register commands), `crates/rocket-app/src/lib.rs` (module and re-exports)
- Test: `crates/rocket-app/src/openapi_sync_plan_tests.rs`, DTO mapping tests in `src-tauri/src/commands/openapi_sync.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `OpenApiSyncService::status(&self, &Path) -> DomainResult<SyncStatusView>`, `OpenApiSyncService::local_state(&self, collection: &str)`, `load_for_apply` (private, takes the collection lock), `plan_with_links(&SyncInputs, &[(String, String)]) -> ImportResult<SyncPlan>`, `item_digest`, `variable_digest`, `make_sync_service` in `src-tauri/src/commands/import.rs`, `SyncPlan`, `SyncItem`, `FieldChange`, `OpenApiSource.operations`.
- Produces:
  - `OpenApiSyncService::plan(&self, collection: &str) -> DomainResult<PlanView>`; `PlanView { pending: bool, plan: SyncPlan, item_digests: Vec<String>, variable_digests: Vec<String>, unchanged: Vec<UnchangedView> }`; `UnchangedView { key: String, name: String, request_path: Option<String> }`. When there is no pending copy, `pending == false` and the rest is empty.
  - Tauri `openapi_sync_status(collection: String) -> Result<Option<SyncStatusDto>, String>` and `openapi_sync_plan(collection: String) -> Result<SyncPlanDto, String>`, registered in `lib.rs`.
  - `SyncStatusDto { location, interval: "off"|"hourly"|"daily"|"weekly"|"monthly", lastCheckedAt: Option<String>, lastStatus: Option<{state: "unchanged"|"changed"|"failed", code?: String}>, hasPending: bool, hasToken: bool }` (camelCase).
  - `SyncPlanDto { pending: bool, items: Vec<SyncItemDto>, variableChanges: Vec<VariableChangeDto>, unchanged: Vec<UnchangedDto>, skippedHidden: usize, deletedByUser: Vec<String>, cautious: bool, warnings: Vec<String> }`. `SyncItemDto` is a tagged enum (`kind: "new"|"changed"|"removed"`) with `key, name, digest, fingerprint?, folder?, requestPath?, possiblyRenamedFrom?, possiblyRenamedTo?, edited?: bool|null, cautious?: bool, changes?: Vec<FieldChangeDto>`. `FieldChangeDto { field, outcome: "takeSpec"|"keepYours"|"conflict", baseline?, yours?, spec? }`. `VariableChangeDto { name, kind: "baseUrl"|"addSecurity", digest, baseline?, yours?, spec? }`. Digests and fingerprints are carried now so Plan 9 changes no DTO.

- [ ] **Step 1: Write failing service tests** (`openapi_sync_plan_tests.rs`, using the helpers of `openapi_sync_apply_tests.rs`): `plan_is_empty_when_no_pending_copy`; `plan_lists_new_changed_and_removed_items_after_a_changed_check`; `plan_unchanged_lists_operations_not_in_the_plan` (an untouched operation appears in `unchanged`, a skipped one does not); `plan_fails_with_conflict_when_the_pending_copy_hash_differs`; `plan_does_not_write_any_file` (list the collection folder and `.rocket` before and after, same names and mtimes).
- [ ] **Step 2: Run them to see them fail** (`cargo test -j4 -p rocket-app plan_`).
- [ ] **Step 3: Implement `OpenApiSyncService::plan`** in `openapi_sync_apply.rs` next to `apply`: it takes the same `collection_lock`, reuses `load_for_apply` and the exact input assembly of `apply`, calls `plan_with_links(&inputs, &[])`, computes digests with `item_digest` and `variable_digest`, and derives `unchanged` as `source.operations` minus plan item keys minus `deleted_by_user` minus keys in `local.skips` that match their fingerprint. It must not call any method that writes. Extract the shared input assembly into one private function rather than copying it.
- [ ] **Step 4: Run the service tests until green.**
- [ ] **Step 5: Write failing DTO tests** in `commands/openapi_sync.rs`: JSON of a `Changed` item has `kind: "changed"`, camelCase keys and the field changes; `SyncStatusDto` of `Failed{code}` serializes `lastStatus: {state: "failed", code: "..."}`; a `Removed` with `edited: None` serializes `edited: null`.
- [ ] **Step 6: Implement the two commands** (thin: build the service with `make_sync_service`, resolve the collection root the way `import.rs` does, run the blocking service call through `tokio::task::spawn_blocking`, map to DTOs, map errors to `String`). `openapi_sync_status` returns `Ok(None)` when the service says the collection has no OpenAPI source (NotFound), `Err` for an unreadable state file. Register both in `lib.rs`.
- [ ] **Step 7: Run `cargo check -j4`, `cargo test -j4 -p rocket-app plan_`, `cargo test -j4 -p rocket openapi_sync` and commit.**

---

### Task 2: TypeScript API, types and query hook

**Files:**
- Modify: `src/lib/tauri-api.ts` (types, `getOpenApiSyncStatus`, `getOpenApiSyncPlan`, `onOpenApiSyncChecked`; add `sourceId: string | null` to `OpenApiImportResult`)
- Create: `src/lib/queries/openapi-sync-queries.ts`
- Test: `src/lib/__tests__/tauri-api.openapi-sync.test.ts`, `src/lib/queries/__tests__/openapi-sync-queries.test.tsx`

**Interfaces:**
- Consumes: the DTO shapes of Task 1 (mirror them exactly as TS types: `OpenApiSyncStatus`, `OpenApiSyncPlan`, `OpenApiSyncItem` (discriminated union on `kind`), `OpenApiFieldChange`, `OpenApiVariableChange`).
- Produces: `getOpenApiSyncStatus(collection: string): Promise<OpenApiSyncStatus | null>`, `getOpenApiSyncPlan(collection: string): Promise<OpenApiSyncPlan>`, `onOpenApiSyncChecked(handler: (e: { collection: string; status: string }) => void): Promise<UnlistenFn>` listening to `openapi-sync-checked`; `openApiSyncKeys = { all, status(collection), plan(collection) }`; `useOpenApiSyncStatus(collection)`, `useOpenApiSyncPlan(collection, enabled: boolean)`, `useOpenApiSyncEvents()`.

- [ ] **Step 1: Write failing wrapper tests** (mock `invoke`, assert the command names `openapi_sync_status` / `openapi_sync_plan` and the `{ collection }` argument; `onOpenApiSyncChecked` listens to `openapi-sync-checked`).
- [ ] **Step 2: Write failing hook tests** (pattern: `collection-trust-queries.test.tsx`): the status hook returns `null` for a collection without a source; an `openapi-sync-checked` event for collection A invalidates A's status and plan queries and not B's; the plan hook does not call the API while `enabled` is false; unlisten runs on unmount.
- [ ] **Step 3: Implement** the wrappers, types and hooks (`staleTime: 0`, listener cleanup via `unsub.then(fn => fn())`, the events hook mounted from the tab and the notice, never globally).
- [ ] **Step 4: Run `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync` and commit.**

---

### Task 3: Spec sync tab

**Files:**
- Modify: `src/types/pane-types.ts` (`CollectionSection` gains `'spec-sync'`), `src/components/collections/CollectionOverviewTab.tsx` (`TABS`, `validSections`, section block; the tab shows only when the status is not `null`), `src/components/layout/BreadcrumbBar.tsx` (`ALL_SECTIONS`, `collectionSectionLabel`, `collectionSectionIcon`), `src/lib/tauri-api.ts` (`UiStateCollectionTab` accepts the new section)
- Create: `src/components/collections/spec-sync/SpecSyncTab.tsx`, `SpecSyncHeader.tsx`, `SpecSyncList.tsx`, `SpecSyncDetail.tsx`, `SpecSyncNotice.tsx` (the notice on the overview, patterned on `CollectionTrustBanner`), `specSyncView.ts` (pure helpers: group items, labels, field display names)
- Test: `src/components/collections/spec-sync/__tests__/` for each component, `specSyncView.test.ts`

**Interfaces:**
- Consumes: Task 2 hooks and types, `usePaneStore.openCollectionTab(collection, 'spec-sync')`, `Badge`, `CollapsibleSection`, `ScrollArea`, `StateBlock`, `EmptyState`, `Tooltip`.
- Produces: `SpecSyncTab({ collection }: { collection: string })`, `SpecSyncNotice({ collection }: { collection: string })`.

- [ ] **Step 1: Write failing tests**: `specSyncView.test.ts` (groups New / Changed / Removed in that order with counts; labels "Spec wins" / "Yours kept" / "Conflict" for the three outcomes; a rename hint produces "Possibly renamed from X" / "Possibly renamed to X"; field names map `header:Accept` to "Header Accept", `body:$.a.b` to "Body $.a.b"); `SpecSyncHeader` (shows location, last check time, interval label, no "Check now" button yet); `SpecSyncList` (selecting an item calls the callback, groups collapse); `SpecSyncDetail` (Changed shows each field with baseline / yours / spec and the outcome badge; New shows the request summary and its folder; Removed with `edited: null` says "Treated as edited, will be kept"; a cautious plan shows the warning text); `SpecSyncTab` (states: loading, no pending, ready list, status error shows "The sync state file cannot be read" with the message, plan Conflict shows "The pending spec changed, check again"); `SpecSyncNotice` (renders only when `lastStatus.state === "changed"` and `hasPending`, click opens the tab).
- [ ] **Step 2: Run to see failures.**
- [ ] **Step 3: Implement** the helpers and components: left list in a `ScrollArea`, right detail, selected item in component state with the first item selected by default; "Stays as you have it" lists `unchanged` in a collapsed section; `skippedHidden` and `deletedByUser` appear as short notes; variable changes are a fourth group "Variables". Render no checkboxes or action buttons in this plan; Plan 9 adds them. Show values in a wrapping `<pre>` block (the backend already clips them to 500 characters), not an editor.
- [ ] **Step 4: Register the section** (union, `TABS`, `validSections`, `ALL_SECTIONS`, label and icon, UI-state persistence type), mount `useOpenApiSyncEvents()` inside `SpecSyncTab` and `SpecSyncNotice`, and mount `SpecSyncNotice` next to `CollectionTrustBanner` in the overview.
- [ ] **Step 5: Run `yarn tsc --noEmit`, `yarn check`, `yarn test spec-sync`, `yarn test CollectionOverviewTab`, `yarn test BreadcrumbBar` and commit.**
- [ ] **Step 6: Manual check (record in the ledger, not blocking):** open a collection without a source and confirm there is no "Spec sync" tab; for an imported collection with a pending copy, confirm the list and detail match the files on disk.
