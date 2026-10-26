# Bruno-style sync settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make scheduling OpenAPI sync as easy as in Bruno: a "Check for spec updates" option at import, an "OpenAPI sync" entry in the collection's menu, an "Edit connection settings" dialog (automatic checks on or off, how often, token, disconnect), short intervals (5, 15, 30 minutes), and an overview of how many endpoints are in sync. No user ever edits a JSON file.

**Architecture:** `SyncInterval` gains three short intervals. The import command takes an optional "check for updates" flag that turns checks on at import (consent is already granted by the typed link). A new `openapi_sync_disconnect` command removes the sync state. The UI replaces the collapsible settings with a dialog opened from a menu next to the header, adds the menu entry on the collection in the sidebar, and adds an overview strip. Consent, locking and the gate stay exactly as built in Plan 9.

**Tech Stack:** Rust (`rocket-collection`, `rocket-app`, `src-tauri`), React + TypeScript, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`. Bruno reference (verified 2026-10-27): https://docs.usebruno.com/open-api/openapi-sync and https://blog.usebruno.com/keep-your-api-collections-in-sync-with-openapi-sync. Bruno facts we match: entry from the collection's ··· menu ("OpenAPI"); a "Check for Spec Updates" checkbox at import (off by default); an "Edit connection settings" page with enable/disable automatic checks, disconnect and how often; an Overview with total, in sync, changed in collection and spec updates pending; a read-only spec view; checks every 5 minutes when on.

## Global Constraints

- Tasks touching `rocket-collection`, `rocket-app` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- Cargo: `-j4` and `-p <crate>`, never `--workspace`; use timeouts on test runs. No unwrap calls in production paths. camelCase renames on IPC DTOs only. New persisted values stay backward compatible: an old file with `off|hourly|daily|weekly|monthly` still loads; a build that does not know a new value must not crash on it (decide in Task 1 and test).
- Default stays "off": a new import makes no background request unless the user ticks the option. Consent rules from Plan 9 are unchanged (no check, token read or interval without a granted consent for the exact location).
- shadcn/ui and lucide-react only, no raw controls; narrow Zustand selectors; strings inline English; comments short full sentences ending with a period.
- Gates: `cargo check -j4`, targeted `cargo test -j4 -p <crate> <name>`, `yarn tsc --noEmit`, `yarn check`, targeted `yarn test <name>`. Commits through `dev-workflow-skills:1-git-commit` with explicit paths, run the gates normally (no `HARNESS_GATE=0`).

## Review Focus

- Short intervals and rate limits: a 5-minute interval on a GitHub file uses conditional requests (ETag / Last-Modified), so an unchanged spec costs a 304; the backoff and `Retry-After` rules still apply; the scheduler tick stays 60 s; jitter stays at most 10% (30 s at 5 minutes).
- Unknown or future interval value in the local file does not crash status or the scheduler (reads as off, and the UI says the setting is not understood).
- Import with the checkbox ticked: interval is set only if the import succeeded and the source was saved; a failed save leaves it off; a file import (no link) has no checkbox.
- Disconnect removes only sync state (source file, local state, pending and applied spec copies, baseline, consent, keychain token when asked), never requests, folders or collection settings; it is blocked while an apply is running; a second disconnect is a no-op.
- The menu entry opens the Spec sync section and is hidden for collections without a source; the settings dialog works with consent missing (shows the banner) and never shows the stored token.
- Changing the interval while a check is running does not start a second check.

---

### Task 1: Short intervals, import option and disconnect (backend)

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/openapi_source.rs` (`SyncInterval`), `crates/rocket-collection/src/openapi_schedule.rs` (interval length, jitter), `crates/rocket-app/src/openapi_sync_service.rs` (`disconnect`), `src-tauri/src/commands/import.rs` (`import_openapi_url` gains `check_for_updates: Option<bool>`), `src-tauri/src/commands/openapi_sync.rs` (`openapi_sync_disconnect`, interval strings, `openapi_sync_set_interval` accepts the new values), `src-tauri/src/lib.rs` (register)
- Test: next to each change, plus `crates/rocket-app/src/openapi_sync_consent_tests.rs` style tests for disconnect

**Interfaces:**
- Consumes: `SyncInterval`, `next_check_at`, `OpenApiSyncService::{set_interval, revoke_consent, clear_token, root_of, collection_lock}`, `FsFileTx` busy-root guard, the repos for source, local state, baseline and spec copies.
- Produces: `SyncInterval` variants `Min5`, `Min15`, `Min30` (serialized `every_5_minutes`, `every_15_minutes`, `every_30_minutes`; keep `hourly|daily|weekly|monthly|off`), DTO strings the same; an unknown interval string in a file deserializes to `Off` with the original kept out of the way (add `#[serde(other)]` or a custom deserializer and test it); `OpenApiSyncService::disconnect(&self, root: &Path, remove_token: bool) -> DomainResult<()>` under the collection lock, refusing with Conflict while an apply is running; `import_openapi_url(..., check_for_updates: Option<bool>)` sets interval `Min5` after the source is saved and consent granted, only when true; `openapi_sync_disconnect(collection: String, removeToken: Option<bool>)` through `run_blocking`, publishing nothing else.

- [ ] **Step 1: Write failing tests**: interval lengths (5, 15, 30 minutes) in `next_check_at` with jitter at most 10%; an old file with `hourly` still loads and a file with `every_2_hours` loads as `Off` without error and status reports it; import with `check_for_updates: true` sets `Min5` only after the source saved, false/None leaves `Off`, a failed save leaves `Off`; `disconnect` deletes the source file, local state, pending and applied spec copies, baseline, consent record and (when asked) the keychain token, leaves requests, folders and `opencollection.yml` untouched, is a no-op the second time, and refuses while an apply holds the busy-root guard.
- [ ] **Step 2: Run them to see them fail, implement, run green.**
- [ ] **Step 3: `cargo check -j4`, `cargo test -j4 -p rocket-collection openapi`, `cargo test -j4 -p rocket-app openapi_sync`, `cargo test -j4 -p rocket --lib openapi_sync`; commit.**

---

### Task 2: Import dialog option and TypeScript API

**Files:**
- Modify: `src/components/import/ImportCollectionDialog.tsx` (checkbox "Check for spec updates" in the OpenAPI link flow, off by default, with a one-line hint "Rocket checks the link every 5 minutes while the app is open"), `src/lib/tauri-api.ts` (interval union with the new values, `importOpenApiUrl(..., checkForUpdates?)`, `disconnectOpenApiSync(collection, removeToken?)`), `src/lib/queries/openapi-sync-queries.ts` (`useDisconnectSync`)
- Test: `src/components/import/ImportCollectionDialog.test.tsx` (extend), `src/lib/__tests__/tauri-api.openapi-sync.test.ts`, `src/lib/queries/__tests__/openapi-sync-queries.test.tsx`

**Interfaces:**
- Consumes: Task 1 commands (`import_openapi_url` arg `checkForUpdates`, `openapi_sync_disconnect` args `collection`, `removeToken`).
- Produces: `OpenApiSyncInterval = 'off' | 'every_5_minutes' | 'every_15_minutes' | 'every_30_minutes' | 'hourly' | 'daily' | 'weekly' | 'monthly'`; the checkbox only appears for a link import (not a file); `useDisconnectSync(collection)` invalidates status, plan and the collection tree and clears the last-apply store entry for that collection.

- [ ] **Step 1: Write failing tests** (checkbox default off, sent as `checkForUpdates: true` only when ticked, absent for file import; wrapper arg names; disconnect hook invalidations).
- [ ] **Step 2: Implement, run green: `yarn tsc --noEmit`, `yarn check`, `yarn test ImportCollectionDialog`, `yarn test openapi-sync`; commit.**

---

### Task 3: Connection settings dialog, menu entry and overview

**Files:**
- Modify: `src/components/collections/spec-sync/SpecSyncSettings.tsx` (becomes the dialog body), `SpecSyncHeader.tsx` (a "···" menu next to the title with "Edit connection settings" and "Check now"), `SpecSyncTab.tsx`, the collection's sidebar context menu component (find it with `grep -rn "Rename collection\|onRenameCollection" src/components`), `src/components/collections/CollectionOverviewTab.tsx` (open the section from the menu)
- Create: `SpecSyncConnectionDialog.tsx`, `SpecSyncOverview.tsx` (counts strip), `DisconnectSyncDialog.tsx`
- Test: `__tests__/SpecSyncConnectionDialog.test.tsx`, `SpecSyncOverview.test.tsx`, `DisconnectSyncDialog.test.tsx`, extend `SpecSyncHeader.test.tsx`, `SpecSyncTab.test.tsx`; a test for the sidebar menu entry next to the existing menu tests

**Interfaces:**
- Consumes: Task 2 hooks and the interval union, existing `useSetInterval`, `useSetToken`, `useClearToken`, `useGrantConsent`, `useRevokeConsent`, `useCheckNow`, `usePaneStore.openCollectionTab(collection, 'spec-sync')`, plan and status hooks.
- Produces: the dialog "Edit connection settings" with: a switch "Check for spec updates automatically" (on sets the last chosen interval or 5 minutes, off sets `off`), a frequency select (Every 5 minutes, 15 minutes, 30 minutes, Hourly, Daily, Weekly, Monthly) enabled only while the switch is on and consent is granted, the location (read-only, query behind "Show full link"), the access token field (write-only, GitHub links only), "Stop trusting this link" (revoke), and "Disconnect" (confirm dialog: "Rocket stops checking this spec. Your requests are not changed."; checkbox "Also remove the stored token"); the sidebar collection menu gets "OpenAPI sync" (shown only when the status is non-null) that opens the Spec sync section; `SpecSyncOverview({ plan, status })` shows "Endpoints in sync", "Changed in your collection", "Spec updates pending" (from the plan: unchanged count, items whose merge has local edits, items count) and the last checked time; an interval value the UI does not know shows "This setting is not understood. Pick a frequency." and the select shows no value.

- [ ] **Step 1: Write failing tests**: the switch on with consent missing shows the consent banner instead of changing anything; switch on sets 5 minutes, off sets off; frequency select options and values; token field behaviour from Plan 10 still holds inside the dialog (cleared after save, gcTime 0, hidden for non-GitHub); disconnect confirm wording, single call on double click, token checkbox passes `removeToken`, the tab disappears after a disconnect (status null); menu entry present only when the collection has a source and opens the tab; overview counts for a plan with new, changed, removed and unchanged items; unknown interval message.
- [ ] **Step 2: Implement, run green: `yarn tsc --noEmit`, `yarn check`, `yarn test spec-sync`, `yarn test CollectionOverviewTab`, `yarn test CollectionsSidebar` (or the menu's test file); commit.**
- [ ] **Step 3: Manual check list (record in the ledger):** import a public link with the box ticked and watch the Spec sync tab show "Every 5 minutes" and a last-checked time within about a minute and a half of launch; untick the switch and confirm no more checks; change to Hourly; disconnect and confirm requests stay and the tab disappears; copy the folder elsewhere and confirm the banner appears before anything can be switched on.
