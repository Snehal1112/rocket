# Spec sync tab actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Spec sync tab can act: approve the spec link, Check now, set the interval and token, choose what to take per endpoint and field, skip, link a rename, and apply, with a clear result. Everything talks to the Plan 9 commands; no backend change except where Task 1 says.

**Architecture:** TypeScript wrappers and react-query mutations for the Plan 9 commands; a pure `buildSelection` helper turns UI choices into the `SelectionDto`; the tab gets selection state, a header with actions, a consent banner and an apply flow with a confirm step and a result view. The backend remains the authority: digests and fingerprints from the plan go back unchanged, stale items come back as `stale`.

**Tech Stack:** React + TypeScript, react-query, shadcn/ui, lucide-react, Vitest. The tab lives in `src/components/collections/spec-sync/`.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md` (line 44: the tab and its buttons). Series index and the Plan 8 and Plan 9 carry-forward sections in `00-plan-index.md` (command list, error strings, consent explanations).

## Global Constraints

- shadcn/ui primitives and lucide-react icons only (no raw `<button>`, `<input>`, `<select>`); narrow Zustand selectors; strings are inline English (no i18n layer); comments are short full sentences ending with a period.
- A secret never lives in component state longer than the field is mounted, is never logged, and is cleared after a successful save; the token field is write-only (the UI never shows the stored token, only "A token is stored").
- The UI never builds a digest or fingerprint; it passes back exactly what the plan DTO gave. Only one `apply` call can be in flight per collection (button disabled, mutation guarded).
- Gates: `yarn tsc --noEmit`, `yarn check`, targeted `yarn test <name>`. Commits through `dev-workflow-skills:1-git-commit` with explicit paths.

## Review Focus

- Consent: status `missing` or `locationChanged` shows the approve banner with the exact location text; nothing else (Check now, interval, token) works until approved; the banner explains why (cloned folder, renamed or moved collection, link changed) and a slash-in-branch `InvalidInput` error is shown as a clear sentence.
- A pending spec is never described as "fetched from <location>" unless the status is granted and the last check succeeded on this machine.
- Apply of 3000 items: selection state stays responsive (Set-based, no per-row re-render of the whole list), rows past the 500 cap are reachable by search or a "show more" control, and "select all" counts hidden rows.
- Stale approvals: the result view lists `stale`, `notRewritable`, `deleted`, `detached` in plain words and offers "Check again"; nothing is claimed as applied that is not in `applied`.
- Double click on Apply or Check now: one request; a second click while pending is ignored.
- Token: saving while consent is missing shows the consent error as a sentence, not a raw `consent_required:` string; clear token works.
- Errors from commands (strings) are mapped to sentences in one place; unknown errors show a generic sentence, never raw backend text containing paths.

---

### Task 1: API wrappers, mutations and the selection builder

**Files:**
- Modify: `src/lib/tauri-api.ts` (types: `hasToken?: boolean`, `consent`, `lastMessage?`; wrappers `checkOpenApiSyncNow`, `setOpenApiSyncInterval`, `setOpenApiSyncToken`, `clearOpenApiSyncToken`, `grantOpenApiSyncConsent`, `revokeOpenApiSyncConsent`, `applyOpenApiSync`; `getOpenApiSyncPlan(collection, links?)`; `getOpenApiSyncStatus(collection, includeToken?)`; `onOpenApiSyncApplied`), `src/lib/queries/openapi-sync-queries.ts` (mutations, `links` and `includeToken` in query keys, applied event refreshes status, plan and the collection tree)
- Create: `src/components/collections/spec-sync/syncErrors.ts` (`syncErrorMessage(error: unknown): string`), `src/components/collections/spec-sync/buildSelection.ts`
- Test: `src/lib/__tests__/tauri-api.openapi-sync.test.ts` (extend), `src/lib/queries/__tests__/openapi-sync-queries.test.tsx` (extend), `__tests__/syncErrors.test.ts`, `__tests__/buildSelection.test.ts`

**Interfaces:**
- Consumes: Plan 9 commands and DTOs as listed in `00-plan-index.md` (Plan 9 carry-forward): `openapi_sync_status(collection, includeToken?)`, `openapi_sync_plan(collection, links?)`, `openapi_sync_check_now`, `openapi_sync_set_interval(collection, interval)`, `openapi_sync_set_token(collection, token)`, `openapi_sync_clear_token`, `openapi_sync_grant_consent(collection, location)`, `openapi_sync_revoke_consent`, `openapi_sync_apply(collection, selection)`; events `openapi-sync-checked`, `openapi-sync-applied`. Rust DTO files: `src-tauri/src/commands/openapi_sync.rs`.
- Produces: `buildSelection(plan: OpenApiSyncPlan, state: SelectionState): OpenApiSelection` where `SelectionState = { approved: Set<string>; choices: Map<string, Record<string, 'spec' | 'yours'>>; skips: Set<string>; variables: Set<string>; links: Array<[string, string]> }` and approved items carry `key`, `digest`, `choices` exactly from the plan item; skips carry `key` and `fingerprint`; variables carry `name` and `digest`; `syncErrorMessage` maps `consent_required:` / `location_changed:` prefixes, the grant sentences, `Conflict` pending-changed, busy/another-apply wording and anything else to fixed sentences (never the raw string); hooks `useCheckNow(collection)`, `useSetInterval`, `useSetToken`, `useClearToken`, `useGrantConsent`, `useRevokeConsent`, `useApplySync`, each invalidating status and plan keys, `useApplySync` also invalidating the collection tree queries and returning `ApplyReport`.

- [ ] **Step 1: Write failing tests**: wrapper command names and args (including `links` and `includeToken` defaults, camelCase `renameLinks`); `buildSelection` passes digests and fingerprints through untouched, omits unapproved items, includes only chosen field choices, keeps `links` order, and never invents a key not in the plan; `syncErrorMessage` for each known prefix and for an unknown string containing a path (no path in the output); mutation hooks invalidate the right keys; the applied event refreshes status, plan and the collection tree for the named collection only.
- [ ] **Step 2: Run to see failures, implement, run green.** Keep `hasToken` optional in every consumer of the status type (fix the existing TS where it assumed boolean).
- [ ] **Step 3: `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`, `yarn test syncErrors`, `yarn test buildSelection`; commit.**

---

### Task 2: Selection, skip, rename links and apply

**Files:**
- Modify: `src/components/collections/spec-sync/SpecSyncTab.tsx`, `SpecSyncList.tsx`, `SpecSyncDetail.tsx`, `specSyncView.ts`
- Create: `SpecSyncActionsBar.tsx` (select all / none, counts, Skip, Apply), `ApplyConfirmDialog.tsx`, `ApplyResultDialog.tsx`, `useSyncSelection.ts` (state hook)
- Test: `__tests__/SpecSyncActionsBar.test.tsx`, `ApplyConfirmDialog.test.tsx`, `ApplyResultDialog.test.tsx`, `useSyncSelection.test.ts`, extend `SpecSyncList.test.tsx`, `SpecSyncDetail.test.tsx`, `SpecSyncTab.test.tsx`

**Interfaces:**
- Consumes: Task 1 (`buildSelection`, `useApplySync`, `useOpenApiSyncPlan(collection, enabled, links)`, `syncErrorMessage`), Plan 8 components and `MAX_ROWS_PER_GROUP`.
- Produces: `useSyncSelection(plan)` returning `{ approved, toggle(key), setMany(keys, on), choices, setChoice(key, field, choice), skips, links, linkRename(oldKey, newKey), unlink(oldKey), reset(), selection: OpenApiSelection }`; list rows get a `Checkbox`; the detail pane lets the user pick "Take spec" or "Keep yours" per Conflict field (default per the field's `outcome`: conflicts default to "yours"); rename hints get a "Link as rename" button that adds the link and re-plans with `links`; a search box filters all groups and lifts the 500-row cap for the matches (cap still applies to the unfiltered view with a "Show more" button); "Select all" applies to the whole filtered set including hidden rows and shows its count.

- [ ] **Step 1: Write failing tests**: selection hook (toggle, select all with hidden rows, filter then select all, clear on a new plan digest set, links reset the plan query); list row checkbox and keyboard (Space toggles); detail field choices; "Link as rename" calls the plan query with `links` and the Removed/New pair collapses into one Changed item; Skip button sends skips with fingerprints and removes them from the list after refetch; Apply opens the confirm dialog listing counts (new, changed, removed with kept/deleted wording); confirming sends one `apply` call even on double click; result dialog shows `applied`, `stale`, `notRewritable`, `deleted`, `detached`, `skipped`, `remaining` in plain words and a "Check again" button that runs the status refetch; a thrown apply error shows `syncErrorMessage`.
- [ ] **Step 2: Run to see failures, implement.** Selection state is a `Set`, rows are memoized and receive primitives only; do not keep selection in a Zustand store (tab-local). Removed items are never deleted unless the plan item says `edited === false`; the confirm dialog says which Removed items will be deleted and which kept.
- [ ] **Step 3: `yarn tsc --noEmit`, `yarn check`, `yarn test spec-sync`; commit.**

---

### Task 3: Header actions, interval, token and consent

**Files:**
- Modify: `src/components/collections/spec-sync/SpecSyncHeader.tsx`, `SpecSyncTab.tsx`, `SpecSyncNotice.tsx`, `src/components/collections/CollectionOverviewTab.tsx` (only if the notice needs the consent state)
- Create: `SpecSyncConsentBanner.tsx`, `SpecSyncSettings.tsx` (interval select and token field in a popover or collapsible section), `CheckNowButton.tsx`
- Test: `__tests__/SpecSyncConsentBanner.test.tsx`, `SpecSyncSettings.test.tsx`, `CheckNowButton.test.tsx`, extend `SpecSyncHeader.test.tsx`, `SpecSyncNotice.test.tsx`, `SpecSyncTab.test.tsx`

**Interfaces:**
- Consumes: Task 1 hooks and `syncErrorMessage`; status `consent`, `interval`, `lastCheckedAt`, `lastStatus`, `lastMessage`, `hasToken` (requested with `includeToken: true` only inside the settings section); the `SecretField` pattern in `src/components/**/AgentCredentialSection.tsx` (find it with grep) for the write-only token field.
- Produces: header shows the location, last check time, interval label and "Check now" (disabled while pending or when consent is not granted, with a tooltip why); `SpecSyncConsentBanner({ collection, location, consent })` shows "Trust this spec link" with the exact location text and an Approve button (calls `grantOpenApiSyncConsent(collection, location)` with that same text), and a sentence per state: `missing` ("This collection came from another place or was moved. Rocket does not contact the link until you approve it on this machine."), `locationChanged` ("The spec link in this collection changed. Approve the new link before Rocket contacts it."); the settings section holds the interval `Select` (off, hourly, daily, weekly, monthly), the token field (write-only, "A token is stored" with Clear), and "Stop trusting this link" (revoke); failed check shows `lastMessage` in an alert; the notice for a changed spec appears only when consent is granted and says "A new version of the spec was found", never "fetched from <link>" for a pending spec otherwise.

- [ ] **Step 1: Write failing tests**: consent banner texts and the grant call carries the displayed location text; Check now disabled without consent (tooltip), single call on double click, shows the outcome (unchanged / changed / failed with the fixed message and retry hint); interval select disabled until granted and calls `set_interval`; token field is a password input, never prefilled, cleared after save, saving without consent shows the consent sentence, "Clear" calls `clear_token`; `hasToken` is requested only when the settings section is open; Https location with a query shows the host and path with the query hidden behind "Show full link"; the notice behaviour above.
- [ ] **Step 2: Run to see failures, implement, run green.**
- [ ] **Step 3: `yarn tsc --noEmit`, `yarn check`, `yarn test spec-sync`, `yarn test CollectionOverviewTab`; commit.**
- [ ] **Step 4: Manual check list (record in the ledger, not blocking):** import a public link, approve it, Check now; set an interval and watch `.rocket/openapi-local.json`; copy the folder elsewhere and confirm the banner asks for approval and no request is made; edit `.rocket/openapi-source.yml` location and confirm `locationChanged`; apply a Changed item and see the request file update and the sidebar refresh; save a request during an apply and confirm no lost edit.
