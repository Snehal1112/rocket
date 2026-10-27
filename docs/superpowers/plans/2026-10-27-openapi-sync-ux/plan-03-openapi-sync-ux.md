# OpenAPI sync UX: Overview and Your changes Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The Overview and Your changes tabs show real data from the new read-only commands.

**Architecture:** TypeScript wrappers and react-query hooks for `openapi_sync_overview`, `openapi_sync_changes` and `openapi_sync_badges` (all local-only commands from plan 01); the existing `useOpenApiSyncEvents` also refreshes them. Two components replace the empty states of plan 02.

**Tech Stack:** React + TypeScript, react-query, shadcn/ui (Badge, Table, StateBlock, Button), lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md`

## File Structure

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Types `OpenApiOverview`, `OpenApiChangeRow`, `OpenApiChanges`, `OpenApiBadge`; wrappers; `pendingId?: string` on `OpenApiSyncStatus` |
| `src/lib/queries/openapi-sync-queries.ts` | Keys, `useOpenApiSyncOverview`, `useOpenApiSyncChanges`, `useOpenApiSyncBadges`; event invalidation |
| `src/components/collections/openapi-sync/OpenApiSyncOverview.tsx` | Counts, banner, facts table |
| `src/components/collections/openapi-sync/OpenApiSyncChanges.tsx` | Read-only Your changes lists |
| `src/components/collections/openapi-sync/OpenApiSyncTab.tsx` | Mount the two components |
| tests | `src/lib/__tests__/tauri-api.openapi-sync.test.ts`, `src/lib/queries/__tests__/openapi-sync-queries.test.tsx`, `openapi-sync/__tests__/OpenApiSyncOverview.test.tsx`, `OpenApiSyncChanges.test.tsx`, `fixtures.ts` |

## Global Notes

- The Rust DTOs are `OverviewDto { endpoints, inSync: number | null, changedByYou: number | null, updatesWaiting, specVersion: string | null, lastSyncedAt: string | null, lastSyncedUpdated }`, `ChangesDto { baselineTrusted, modified: ChangeRowDto[], added, deleted }`, `ChangeRowDto { key: string | null, uid, name, method, url, requestPath: string | null, fields: string[] }`, `BadgeDto { collection, waiting }` (camelCase, in `src-tauri/src/commands/openapi_sync.rs`). Mirror them exactly.
- shadcn/ui and lucide-react only; strings inline English; no raw backend text on screen (use `syncErrorMessage`).
- Tests: `renderWithQuery`, `vi.mock('@/lib/tauri-api', ...importActual...)`, `makeStatus` and a new `makeOverview` / `makeChanges` in `fixtures.ts`. Commands: `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

---

### Task 1: API wrappers, types and hooks

**Interfaces:**
- Consumes: `invoke` wrappers pattern of `getOpenApiSyncStatus` (`tauri-api.ts:2072`), `openApiSyncKeys` (`openapi-sync-queries.ts:26`), `useOpenApiSyncEvents` (`:289`).
- Produces: `getOpenApiSyncOverview(collection: string): Promise<OpenApiOverview>`, `getOpenApiSyncChanges(collection: string): Promise<OpenApiChanges>`, `getOpenApiSyncBadges(): Promise<OpenApiBadge[]>` (commands `openapi_sync_overview`, `openapi_sync_changes`, `openapi_sync_badges`, args `{ collection }`); keys `openApiSyncKeys.overview(c)`, `.changes(c)`, `.badges`; hooks `useOpenApiSyncOverview(collection)`, `useOpenApiSyncChanges(collection)`, `useOpenApiSyncBadges()` (all `staleTime: 0`, `retry: false`, `networkMode: 'always'`).

- [ ] **Step 1: Read** the status wrapper and hook (`useOpenApiSyncStatus`), `useOpenApiSyncEvents` and their tests.
- [ ] **Step 2: Write the failing tests.**
  - `tauri-api.openapi-sync.test.ts`: `overview_calls_openapi_sync_overview_with_the_collection`, `changes_calls_openapi_sync_changes`, `badges_calls_openapi_sync_badges_without_arguments` (each asserts `invoke` was called with the exact command name and args).
  - `openapi-sync-queries.test.tsx`: `an_applied_event_refreshes_overview_changes_and_badges_for_that_collection`, `a_checked_event_refreshes_overview_and_badges_but_not_other_collections` (events for `A` leave the `B` overview query untouched), `the_overview_hook_returns_the_rows_unchanged`.
- [ ] **Step 3: Run to see them fail.** `yarn test openapi-sync`. Expected: `getOpenApiSyncOverview is not a function`.
- [ ] **Step 4: Implement** types, wrappers, keys, hooks and the extra invalidations in `useOpenApiSyncEvents` (badges are global: invalidate `openApiSyncKeys.badges` on every event).
- [ ] **Step 5: Run again.** Expected: the new tests pass; `yarn tsc --noEmit` clean.

### Task 2: Overview tab

**Interfaces:**
- Consumes: Task 1 hooks, `splitLocation`, `intervalLabel` (`openapi-sync/openApiSyncView.ts`, formerly `specSyncView.ts:113,128`), the "last checked" formatting the header already uses, `OpenApiSyncConnectionDialog`.
- Produces: `OpenApiSyncOverview({ collection, status, onReview }: { collection: string; status: OpenApiSyncStatus; onReview: () => void })`. Four stat cards: **Endpoints**, **In sync with spec**, **Changed by you**, **Spec updates waiting** (`inSync` / `changedByYou` null render as "–" with the note "Sync data is from an older version, so these counts are unknown."). A banner "N updates are waiting." with a **Review updates** button (calls `onReview`) only when `updatesWaiting > 0`. A facts table: Spec link (host and path, query behind "Show full link"), Spec version (or "–"), Last checked (with "no problems" or the fixed `lastMessage`), Automatic checks (interval label and an **Edit connection settings** button that opens the connection dialog), Last synced ("Never" or the time plus "N requests updated").

- [ ] **Step 1: Read** `OpenApiSyncHeader.tsx` (location, last checked) and `OpenApiSyncTab.tsx` (inner tab switching).
- [ ] **Step 2: Write the failing tests** (`OpenApiSyncOverview.test.tsx`; mock `getOpenApiSyncOverview`):
  - `shows_the_four_counts` (overview `{ endpoints: 42, inSync: 36, changedByYou: 3, updatesWaiting: 5 }` shows 42, 36, 3, 5 under their labels).
  - `shows_a_dash_for_unknown_counts` (`inSync: null`, `changedByYou: null` show "–" and the older-version note).
  - `the_banner_appears_only_when_updates_wait` and `review_updates_calls_onReview`.
  - `hides_the_query_until_show_full_link` (status location with `?token=abc`; the text `abc` is absent until the button is clicked).
  - `edit_connection_settings_opens_the_dialog`.
  - `shows_never_when_nothing_was_synced` and `shows_the_sync_time_and_count` (`lastSyncedUpdated: 4` -> "4 requests updated").
  - `shows_a_fixed_sentence_when_the_overview_fails` (rejected query text contains a path; not displayed).
  - `the_tab_mounts_the_overview_and_review_switches_to_spec_updates` (in `OpenApiSyncTab.test.tsx`).
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncOverview`. Expected: module not found.
- [ ] **Step 4: Implement** the component and mount it in the Overview tab; `onReview` switches the inner tab to `updates`.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Your changes tab (read-only)

**Interfaces:**
- Consumes: Task 1 `useOpenApiSyncChanges`, shadcn `Badge`, `StateBlock`.
- Produces: `OpenApiSyncChanges({ collection }: { collection: string })` with three groups **Modified (n)**, **Added by you (n)**, **Deleted by you (n)**. A row shows a kind badge, the method, the URL or path, and for Modified the field names rendered with the existing `fieldLabel` ("Header Accept", "URL"). A short intro line: "Differences between your collection and the last synced spec. Sync never overwrites these without asking." and the footer "Scripts, tests and assertions are not tracked here." When `baselineTrusted` is false: "These differences are approximate because the sync data is from an older version." and Modified rows show no field names. Empty state: "You have not changed anything." No buttons.

- [ ] **Step 1: Read** `OpenApiSyncList.tsx` row rendering and `fieldLabel` in `openApiSyncView.ts`.
- [ ] **Step 2: Write the failing tests** (`OpenApiSyncChanges.test.tsx`):
  - `groups_modified_added_and_deleted_with_counts`.
  - `modified_rows_name_the_changed_fields` (`fields: ['url', 'header:Accept']` shows "URL" and "Header Accept").
  - `shows_the_approximate_note_without_a_trusted_baseline_and_no_field_names`.
  - `shows_the_empty_state`.
  - `has_no_buttons_in_this_version` (no element with role `button` inside the lists).
  - `shows_a_fixed_sentence_when_loading_fails`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncChanges`. Expected: module not found.
- [ ] **Step 4: Implement** the component and mount it in the Your changes tab; show the count `Your changes (n)` on the inner tab label when `n > 0`.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

## Review Focus

1. A failing overview or changes query must show a fixed sentence, never a path or raw text. Pinned by `shows_a_fixed_sentence_when_the_overview_fails` and `shows_a_fixed_sentence_when_loading_fails`.
2. Unknown counts must never render as "0". Pinned by `shows_a_dash_for_unknown_counts`.
3. A link with a query string (it can hold credentials) must not show the query by default. Pinned by `hides_the_query_until_show_full_link`.
4. An event for another collection must not refresh this collection's overview. Pinned by `a_checked_event_refreshes_overview_and_badges_but_not_other_collections`.
5. Your changes must offer no destructive action in this version. Pinned by `has_no_buttons_in_this_version`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux/plan-04-openapi-sync-ux.md
