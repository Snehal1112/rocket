# OpenAPI sync UX: dedicated tab and header Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** OpenAPI sync opens as its own `openapi-sync` tab with a header (status chip, Check now, ··· menu) and three inner tabs, and the old collection section and notice are gone.

**Architecture:** A new collection-scoped tab type, modelled on `contract`, is opened by `openOpenApiSyncTab(collectionName)`. The existing `spec-sync/` components move to `openapi-sync/` and are renamed. A new header derives a chip state with a pure function. The inner tabs are Overview, Spec updates (the existing review UI) and Your changes; the first and last show an empty state until the next plan fills them.

**Tech Stack:** React + TypeScript, Zustand, shadcn/ui (Tabs, DropdownMenu, Badge), lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md`

## File Structure

| File | Change |
|---|---|
| `src/types/pane-types.ts` | `OpenApiSyncTab`, `isOpenApiSyncTab`, `Tab` union; `CollectionSection` loses `'spec-sync'` |
| `src/stores/pane-store.ts` | `openOpenApiSyncTab`; collection-name chain |
| `src/components/panes/{EditorGroup,TabItem,BreadcrumbBar}.tsx` | Render branch, icon, breadcrumb |
| `src/components/collections/spec-sync/` -> `src/components/collections/openapi-sync/` | Directory move; files renamed `SpecSync*` -> `OpenApiSync*`; tests move with them |
| `src/components/collections/openapi-sync/OpenApiSyncTab.tsx` | Tab body: header, inner tabs |
| `src/components/collections/openapi-sync/OpenApiSyncHeader.tsx`, `chipState.ts` | Header and pure chip logic |
| `src/components/collections/{CollectionNode,CollectionOverviewTab}.tsx` | Sidebar opener; remove section, `TABS` entry, notice |
| `src/lib/queries/openapi-sync-queries.ts` | Rename `useSpecSyncAvailability` to `useOpenApiSyncAvailability` |

## Global Notes

- shadcn/ui and lucide-react only; narrow Zustand selectors; strings inline English; every visible "Spec sync" becomes "OpenAPI sync".
- Tests: React Testing Library with `renderWithQuery` from `@/test/trust-fixtures`; mock `@/lib/tauri-api` with `vi.importActual` as in `SpecSyncNotice.test.tsx:16-24`; fixtures `makeStatus`, `newItem` in `__tests__/fixtures.ts` (moves with the directory). Reset module state with `clearApplyState()` in `afterEach`. No 3000-row fixtures.
- Commands: `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

---

### Task 1: Tab type, store opener and routing

**Interfaces:**
- Consumes: the `contract` tab as template (`pane-types.ts:97-106`, `pane-store.ts:718-730`, `EditorGroup.tsx:275-281`, `TabItem.tsx:113`, `BreadcrumbBar.tsx:437-455`).
- Produces: `interface OpenApiSyncTab extends BaseTab { tabType: 'openapi-sync'; collectionName: string }`; `isOpenApiSyncTab(tab: Tab): tab is OpenApiSyncTab`; `openOpenApiSyncTab(collectionName: string): void` with id `openapi-sync:${collectionName}` and title `OpenAPI sync — ${collectionName}`; `OpenApiSyncTab` component (`{ collection: string }`) that for now renders the existing review UI.

- [ ] **Step 1: Read** `pane-types.ts`, `pane-store.ts` (`openTab`, `openContractTab`, the collection-name chain at `:470-478`), `EditorGroup.tsx`, `TabItem.tsx`, `BreadcrumbBar.tsx` and the tests `pane-store-*.test.ts`, `TabBar.test.tsx`.
- [ ] **Step 2: Write the failing tests.**
  - `src/stores/__tests__/pane-store-openapi-sync.test.ts`: `opens_one_tab_per_collection` (call `openOpenApiSyncTab('Orders')` twice: one tab with id `openapi-sync:Orders`, active), `title_names_the_collection` (`OpenAPI sync — Orders`), `opening_switches_the_active_collection` (the collection-name chain makes `switchCollection` run for the tab's collection).
  - `src/components/panes/__tests__/EditorGroup-openapi-sync.test.tsx`: `renders_the_openapi_sync_tab_instead_of_the_collection_overview` (mock `OpenApiSyncTab` to a stub with `data-testid="openapi-sync-tab"`; assert it renders and `CollectionOverviewTab` does not).
  - `src/components/panes/__tests__/TabItem-openapi-sync.test.tsx`: `shows_a_sync_icon_and_the_title`.
  - `src/components/panes/__tests__/BreadcrumbBar-openapi-sync.test.tsx`: `shows_collection_then_openapi_sync`.
- [ ] **Step 3: Run to see them fail.** `yarn test openapi-sync`. Expected: `openOpenApiSyncTab is not a function` and missing module errors.
- [ ] **Step 4: Implement** the type, guard, union member, store action and chain entry, the lazy `EditorGroup` branch (before the `CollectionOverviewTab` fallthrough), the `RefreshCw` icon, and the breadcrumb branch (collection picker like the contract branch, second segment `OpenAPI sync`). Create `OpenApiSyncTab.tsx` in `src/components/collections/spec-sync/` for now, rendering the existing `SpecSyncTab`.
- [ ] **Step 5: Run again.** Expected: the new tests pass, `yarn tsc --noEmit` is clean.

### Task 2: Move, rename and remove the old section

**Interfaces:**
- Consumes: Task 1.
- Produces: all files of `src/components/collections/spec-sync/` live in `src/components/collections/openapi-sync/` with `SpecSync*` renamed to `OpenApiSync*` (for example `SpecSyncList` -> `OpenApiSyncList`, `SpecSyncDetail` -> `OpenApiSyncDetail`, `SpecSyncConnectionDialog` -> `OpenApiSyncConnectionDialog`); `useSpecSyncAvailability` -> `useOpenApiSyncAvailability`; `CollectionSection` is `'overview' | 'auth' | 'variables' | 'documentation'`; `SpecSyncNotice` and its test are deleted; `CollectionNode` opens the tab with `openOpenApiSyncTab(summary.name)`.

- [ ] **Step 1: Read** `CollectionOverviewTab.tsx` (`TABS` `:127-133`, `validSections` `:202-208`, `resolveCollectionSection` `:111-125`, notice `:408-409`, section block `:612-615`), `CollectionNode.tsx` (`openSpecSync` `:310-320`, menu items `:522-526`, `:589-593`), `BreadcrumbBar.tsx` (`ALL_SECTIONS` `:300-312`, `:99`, `:114`, `:710`) and the tests `CollectionOverviewTab.spec-sync.test.tsx`, `CollectionNode.spec-sync.test.tsx`, `BreadcrumbBar.spec-sync.test.tsx`.
- [ ] **Step 2: Write the failing tests** (rename the existing test files to `*.openapi-sync.test.tsx` and change them):
  - `CollectionOverviewTab.openapi-sync.test.tsx`: `has_no_spec_sync_tab_any_more` (the tab list has Overview, Auth, Variables, Documentation only), `a_stored_spec_sync_section_falls_back_to_overview` (render with `activeSection: 'spec-sync' as never`; assert the overview content), `shows_no_sync_notice` (status `changed` and `hasPending`; no "A new version of the spec was found").
  - `CollectionNode.openapi-sync.test.tsx`: `the_menu_entry_opens_the_openapi_sync_tab` (click "OpenAPI sync"; assert `openOpenApiSyncTab` called with the collection name and no collection-section call), `the_entry_is_hidden_without_a_source` (await a settled render before asserting absence).
  - `BreadcrumbBar.openapi-sync.test.tsx`: `the_section_picker_has_no_spec_sync_entry`.
  - Moved tests keep their names; update import paths only.
- [ ] **Step 3: Run to see them fail.** `yarn test openapi-sync`. Expected: `Cannot find module '../OpenApiSyncList'` style errors and the old section still present.
- [ ] **Step 4: Implement** the move (plain file moves, then fix every import with a project-wide search for `spec-sync` and `SpecSync`), the removals, the `CollectionSection` change, the hook rename, and every visible label. Keep `resolveCollectionSection` (it still maps unknown values to `overview`).
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`, `yarn test CollectionOverviewTab`, `yarn test CollectionNode`. Expected: green, and `grep -rn "SpecSync\|Spec sync\|spec-sync" src` finds nothing outside the plan docs and this task's tests that assert the removal.

### Task 3: Header, status chip and inner tabs

**Interfaces:**
- Consumes: Task 2, `useOpenApiSyncStatus`, `useOpenApiSyncPlan`, `useCheckRunner`/`CheckNowButtonView`, `OpenApiSyncConnectionDialog`, `OpenApiSyncConsentBanner`, shadcn `Tabs`, `DropdownMenu`, `Badge`, `StateBlock`.
- Produces:
  - `chipState(status: OpenApiSyncStatus, waiting: number): ChipState` where `ChipState = { kind: 'needs-approval' | 'failed' | 'updates' | 'off' | 'connected'; label: string; detail?: string }`. First match wins: `consent !== 'granted'` -> needs-approval ("Needs approval"); `lastStatus.state === 'failed'` -> failed ("Check failed", `detail = status.lastMessage`); `waiting > 0` -> updates (`"${waiting} updates"`, singular `"1 update"`); `interval === 'off'` -> off ("Checks off"); else connected ("Connected").
  - `OpenApiSyncHeader({ collection, status, waiting })`: name, chip, location (query behind "Show full link", existing `splitLocation`), last checked, interval label, **Check now**, and a **···** menu with "Edit connection settings" (opens the existing dialog) and "Disconnect" (opens the existing disconnect dialog).
  - `OpenApiSyncTab` body: header, consent banner when consent is not granted, `Tabs` with values `overview`, `updates`, `changes`; labels `Overview`, `Spec updates (N)` (N from the plan, hidden at 0), `Your changes`; default `updates` when updates wait, else `overview`. `updates` renders the existing review UI. `overview` and `changes` render `StateBlock tone="empty"` titled "Nothing to show yet".

- [ ] **Step 1: Read** `OpenApiSyncTab.tsx` (was `SpecSyncTab.tsx`), `OpenApiSyncHeader.tsx`, `CheckNowButton.tsx`, `OpenApiSyncConnectionDialog.tsx` and the header tests.
- [ ] **Step 2: Write the failing tests.**
  - `chipState.test.ts`: table test with these cases: (consent missing, anything) -> `needs-approval`; (granted, lastStatus failed, waiting 3) -> `failed` with the message as detail; (granted, ok, waiting 3) -> `updates` labelled `3 updates`; waiting 1 -> `1 update`; (granted, ok, 0, interval off) -> `off`; (granted, ok, 0, every_5_minutes) -> `connected`; consent `locationChanged` and failed -> `needs-approval` (precedence).
  - `OpenApiSyncHeader.test.tsx`: `shows_the_chip_location_and_interval`, `the_menu_opens_connection_settings` (click ···, then "Edit connection settings"; the dialog title is visible), `the_menu_opens_the_disconnect_dialog`, `shows_the_query_only_after_show_full_link`, `check_now_is_disabled_without_consent`.
  - `OpenApiSyncTab.test.tsx`: `defaults_to_spec_updates_when_updates_wait` (hasPending, plan with 2 items; the review list is visible), `defaults_to_overview_otherwise`, `shows_the_consent_banner_when_not_granted`, `inner_tabs_switch`, `overview_and_changes_show_the_empty_state`, `shows_a_fixed_sentence_for_an_unreadable_state` (status query rejects with a path in its text; the page shows the generic sentence and no path), `after_disconnect_the_tab_shows_that_nothing_is_connected` (status becomes `null`; the tab shows "This collection is not connected to an OpenAPI spec." and no crash).
- [ ] **Step 3: Run to see them fail.** `yarn test chipState` and `yarn test OpenApiSyncHeader`. Expected: `Cannot find module '../chipState'`.
- [ ] **Step 4: Implement** `chipState.ts`, the header, the inner tabs and the dialog wiring. Remove the old connection button and the old overview strip from the header (the overview counts return in the next plan).
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: all green.

## Review Focus

1. A persisted collection tab with `activeSection: 'spec-sync'` must open the overview, not a blank pane. Pinned by `a_stored_spec_sync_section_falls_back_to_overview`.
2. Opening OpenAPI sync twice must activate the same tab, not duplicate it. Pinned by `opens_one_tab_per_collection`.
3. A collection without a source must offer no sync entry. Pinned by `the_entry_is_hidden_without_a_source`.
4. Chip precedence must put consent problems before failures and failures before updates. Pinned by the `chipState` table cases.
5. A status error (unreadable state file) must show a fixed message, never raw error text. Pinned by an `OpenApiSyncTab.test.tsx` case `shows_a_fixed_sentence_for_an_unreadable_state`.
6. Disconnect must leave the tab usable (it closes or shows the connect hint, no crash). Pinned by `after_disconnect_the_tab_shows_that_nothing_is_connected`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux/plan-03-openapi-sync-ux.md
