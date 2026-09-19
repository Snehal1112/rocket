# P0 duplication remediation — verified findings and implementation plan

**Source:** `docs/reports/component-duplication/` audit (2026-09-19), P0 items CD-001–CD-008.
**Method:** each item independently re-verified against current source (not taken on the
report's word) by a dedicated investigation pass, per `decomposing-investigations`. Findings
below supersede the report where they disagree with it.

## Headline change from the audit

**A new, more urgent bug was found during verification, outside the original report's scope:**
editing any contract through the current (modern) contract editor **permanently deletes all of
that contract's document attachments**, unconditionally, on every save. This is live data loss
happening today, independent of the CD-006 consolidation question. It should be fixed first,
ahead of the rest of this plan.

## Priority order for implementation

1. **[NEW-P0] Contract attachment deletion bug** — fix immediately, independent of everything else.
2. **CD-001 + CD-002** — same root cause (auto-save reusing execution-oriented auth/header
   conversion instead of a persistence-oriented one); plan and land together.
3. **CD-003** — request save payload mapper unification.
4. **CD-004** — delete dead `CollectionSettingsDialog` (trivial, safe, do anytime).
5. **CD-007, CD-008** — pane utility deduplication (trivial, safe, do anytime).
6. **CD-006** — partial legacy contract retirement now; full retirement blocked on a product
   decision about attachments/folder-badge/live-preview.
7. **CD-005** — downgraded to P2/P3. The disagreement that justified P0 no longer exists (this
   session's earlier `EnvironmentDialog`/`VariableTable` rewrite already fixed it). Remaining
   work is optional DRY cleanup, not urgent.

---

## [NEW-P0] Contract attachment deletion bug

**Verified by:** CD-006 investigation (unprompted discovery).

**Problem:** The modern contract editor (`NewContractModal.tsx`, used for both create and edit)
hardcodes `newDocumentPaths: []` and `keptDocumentPaths: []`
(`src/stores/contracts/contractsActions.ts:125-126`) because it has no attachment UI at all.
`crates/rocket-app/src/contract_service.rs:242-256` deletes every attachment file not present in
`kept_paths` on update. Since the only reachable editor always sends an empty `keptDocumentPaths`,
**every edit through the UI wipes all existing attachments from disk**, regardless of whether the
user touched attachments at all.

**Fix plan:**
1. Add a minimal attachment-preserving fix first, decoupled from any UI work: when
   `NewContractModal` is editing an existing contract, populate `keptDocumentPaths` from the
   contract's current `documentPaths` (fetched from the contract being edited) instead of `[]`.
   This alone stops the silent deletion without requiring new UI.
2. Add a characterization test asserting: editing a contract with existing attachments via
   `NewContractModal`'s save path does not delete them (mock/stub `contract_service` boundary or
   test at the `contractsActions.ts` unit level).
3. Track full attachment UI (pick/add/remove/display/open) as a separate, explicitly-scoped
   follow-up (see CD-006 below) — not required to stop the active data loss.

**Files:** `src/stores/contracts/contractsActions.ts:101,125-126`, `src/components/contracts/NewContractModal.tsx`, `crates/rocket-app/src/contract_service.rs:242-256`.

**Validation:** new characterization test + `cargo test -p rocket-app` + manual check: attach a
document to a contract (via direct YAML/fixture if no UI exists yet), edit an unrelated field via
the modal, confirm the file still exists on disk after save.

---

## CD-001 + CD-002 — canonical persisted auth/header adapters

**Verified:** CONFIRMED (both). CD-001 is reproducible today: editing any field on a request with
AWS SigV4 auth, then switching or closing the tab, triggers autosave, which routes through
`execute-request.ts`'s `toApiAuth` (execution-only, intentionally lossy) and writes
`auth:{authType:'none'}` to disk via a full-file overwrite
(`crates/rocket-infra/src/fs_collection/requests.rs:57-97`). CD-002: the same auto-save path
filters headers by `enabled` (dropping every disabled header) instead of by non-blank key (every
other save path's rule), and inversely persists blank-key draft rows that other paths discard.

**Root cause (shared):** `src/lib/auto-save.ts` was written against `execute-request.ts`'s
converters, which are correct for *sending a request* but wrong for *persisting* one. There is no
dedicated persistence-oriented auth/header mapper — `SaveRequestButton.tsx`,
`CollectionOverviewTab.tsx`, and `SaveToCollectionDialog.tsx` each hand-roll their own instead of
sharing one.

**Fix plan:**
1. Add `src/lib/persisted-auth.ts` (or extend `oauth2-mapping.ts`'s pattern) exporting
   `toPersistedAuth(state: AuthState): Auth` and `fromPersistedAuth(auth: Auth): AuthState`,
   covering none/inherit/basic/bearer/api-key/oauth2 (delegate to existing
   `oauth2-mapping.ts` — already canonical)/aws-sig-v4 (including `sessionToken` and
   `profileName`, currently unhandled anywhere in the frontend — new gap, not just a
   reconciliation). Pick one explicit empty-value policy for `sessionToken`
   (`''` vs. omit) and apply it everywhere.
2. Add `src/lib/persisted-headers.ts` exporting `toPersistedHeaders(headers): Header[]`, filtering
   by non-blank key (matching manual save, not auto-save's current `enabled`-based filter), always
   preserving the `enabled` flag verbatim.
3. Migrate call sites one at a time, verifying output payloads are unchanged for the
   non-auto-save paths: `CollectionOverviewTab.tsx`, `SaveRequestButton.tsx`, `pane-utils.ts`
   (load side), then `auto-save.ts` last (this is the one whose *output* should change —
   confirm the fix by testing that toggling a header off and letting autosave fire no longer
   drops it, and that AWS SigV4 fields round-trip through autosave).
4. Explicitly do NOT reuse `execute-request.ts`'s `toApiAuth`/header filtering for any
   persistence path — that converter's OAuth→bearer and AWS→none behavior is intentional and
   correct only for the wire request.

**Files:** `src/lib/auto-save.ts:9-24`, `src/lib/execute-request.ts:80-116,210-212`,
`src/components/collections/CollectionOverviewTab.tsx:80-173`,
`src/components/request/SaveRequestButton.tsx:15-51`, `src/lib/pane-utils.ts:14-109`,
`src/lib/oauth2-mapping.ts:85-289` (reuse, don't duplicate).

**Validation:** table-driven tests covering every auth variant (including AWS SigV4 with a
session token) and header enabled/disabled/blank-key combinations, saved and reloaded through
each of the four paths; `yarn tsc --noEmit`.

---

## CD-003 — one request-tab save payload mapper

**Verified:** CONFIRMED. `SaveToCollectionDialog.tsx:75-86` (initial save) hardcodes
`auth: {authType:'none'}` and omits `tags`, `settings`, `preRequestScript`, `postResponseScript`,
`tests`, `assertions` — all of which are fully editable on an unsourced tab before the first save
(confirmed via `RequestPanel.tsx` tab wiring) — while `SaveRequestButton.tsx`'s
`buildPayloadFromTab` (normal save) includes all of them. There is no backfill: `markClean` is
called immediately after the incomplete initial save, disabling the normal Save button until
another edit is made. `docs` is the one field whose omission is inert — `RequestDocsPanel`
prevents docs from being set on an unsourced tab at all — so it doesn't need special handling.

**Fix plan:**
1. Extract `buildPayloadFromTab` + `authForSave` from `SaveRequestButton.tsx` into a shared
   `src/lib/request-save-mapper.ts`, exporting
   `buildRequestSavePayload(tab: RequestTab, overrides?: { name?: string; fileName?: string }): ApiRequest`.
   `overrides.name` defaults to `tab.title`; `fileName` is only set when provided (already
   optional on `ApiRequest`).
2. Point `SaveRequestButton.tsx` at the shared function with no overrides — output must be
   byte-identical to today.
3. Point `SaveToCollectionDialog.tsx` at the shared function with `{ name, fileName }` overrides,
   removing its inline partial payload literal. Destination-selection logic (collection picker,
   create-new-collection) stays local to the dialog.

**Files:** `src/components/request/SaveRequestButton.tsx:16-82`,
`src/components/request/SaveToCollectionDialog.tsx:57-104`, new `src/lib/request-save-mapper.ts`.

**Validation:** a test asserting `SaveRequestButton`'s payload is unchanged after the extraction
(snapshot or explicit field assertions), plus a new test asserting `SaveToCollectionDialog` now
includes auth/tags/settings/scripts/assertions when saving a tab that has them configured;
`yarn tsc --noEmit`.

---

## CD-004 — delete dead `CollectionSettingsDialog`

**Verified:** CONFIRMED. Zero live call sites anywhere in `src/` (only its own definition file).
It hardcodes `variables: []` and drops OAuth2/AWS SigV4/docs from any save — would silently erase
collection data if ever reintroduced/reconnected.

**Fix plan:** delete `src/components/collections/CollectionSettingsDialog.tsx` and its test (if
any) outright. No migration needed — nothing references it.

**Validation:** `grep -rn "CollectionSettingsDialog" src/` returns nothing after deletion;
`yarn tsc --noEmit`; `yarn test`.

---

## CD-007 — swap `GitToolbarButton`'s local pane lookup for `findTabInTree`

**Verified:** CONFIRMED. `GitToolbarButton.tsx:10-21` defines `findTabGroupId`/`findLeaf` as an
exact duplicate of `pane-utils.ts`'s canonical `findLeaf`, then does two traversals plus a manual
`.find()` to reconstruct what one call to `pane-utils.ts`'s `findTabInTree` already returns.

**Fix plan:**
1. Replace the two local helpers and the manual leaf/tab lookup in `openGitPanel`
   (`GitToolbarButton.tsx:41-50`) with a single `findTabInTree(root, tabId)` call.
2. Preserve the existing `Tab` → `GitTab` narrowing/cast the current code does — `findTabInTree`
   returns the union `Tab` type, so the cast is still needed, just applied to its result instead
   of to a manually-found tab.
3. Delete the now-unused local `findTabGroupId`/`findLeaf` functions.

**Files:** `src/components/layout/GitToolbarButton.tsx:10-21,41-50`, `src/lib/pane-utils.ts:154-167`.

**Validation:** existing Git-toolbar tests (if any) still pass; manual check that opening the Git
panel from the toolbar still finds the right tab; `yarn tsc --noEmit`.

---

## CD-008 — consolidate `collectLeafGroupIds`

**Verified:** CONFIRMED. Byte-for-byte identical definitions at `src/components/panes/TabBar.tsx:36-39`
and `src/components/collections/RequestNode.tsx:51-54`. `src/lib/pane-utils.ts` (with existing
test coverage in `src/lib/__tests__/pane-utils.test.ts`) is the natural shared home.

**Fix plan:**
1. Add `collectLeafGroupIds` to `src/lib/pane-utils.ts`, with a unit test alongside the existing
   `pane-utils.test.ts` coverage.
2. Replace both local definitions with an import from `pane-utils`.

**Files:** `src/components/panes/TabBar.tsx:36-39`, `src/components/collections/RequestNode.tsx:51-54`,
`src/lib/pane-utils.ts`, `src/lib/__tests__/pane-utils.test.ts`.

**Validation:** `yarn test src/lib/__tests__/pane-utils.test.ts`; `yarn tsc --noEmit`.

---

## CD-006 — legacy `contract/` retirement (partial now, rest blocked on a product decision)

**Verified:** PARTIALLY CONFIRMED — re-scoped from the original report.

**What's safe to do now (zero behavior loss, no decision needed):** 8 of 9 files under
`src/components/contract/` are fully dead — `ContractTab.tsx` has zero importers anywhere, and
everything else in the folder is only reachable through it. Pane routing
(`EditorGroup.tsx:191-201`, `pane-store.ts:387-398 openContractTab()`) always lands on the modern
`ContractsTab`; no dynamic import evades this. Delete: `ContractTab.tsx`, `ContractTabTopBar.tsx`,
`ContractForm.tsx`, `ContractLivePreview.tsx`, legacy `ContractCard.tsx`, `ContractEmptyState.tsx`,
`ChangelogSummaryBar.tsx`, `ChangelogTable.tsx`.

**What's still live and needs its own migration step:** `ContractBadge.tsx` (rendered in
`CollectionNode.tsx`, `FolderNode.tsx`, `RequestNode.tsx`) and the legacy `contract-store.ts` it
and `FolderNode`/`RequestNode` read from (`contractsForScope`, `contractStatus`).
- At `CollectionNode`, the modern "lock pin" (`:348-390`, from `useContractsStore`) already
  duplicates the badge's function — migration there is closer to deleting the legacy badge than
  rewriting anything.
- At `RequestNode`, the modern status dot (`:224-235`) is display-only (no click-to-open); the
  badge's click affordance needs to be added to it before the badge can be removed.
- At `FolderNode`, there is no modern equivalent at all — a modern folder-scope indicator needs
  to be built first. Note a real behavioral difference to resolve during that work: legacy uses
  exact `rel_path` equality for folder scope, modern's nested-request coverage uses
  `path.startsWith(scope.rel_path)` — these disagree for nested paths and the plan needs to pick
  one.

**Blocked on a product decision (do not delete these legacy pieces until decided):**
1. **Document attachments** — legacy has full pick/add/remove/display/open UI; modern has none.
   (The active data-loss bug from this is fixed independently above — that fix does not require
   resolving this larger product question.)
2. **Create/edit live preview card** — cosmetic, likely droppable without much discussion.
3. **Tolerant `v1.0`-style version syntax** — affects editing pre-existing contracts created
   before the semver validation rule; a data-compatibility decision, not a feature gap.

**Explicitly downgrade from the original report:** no contract-ID compatibility mapping is
needed — IDs pass through the IPC boundary unchanged (`contractsActions.ts:388-424`,
`contract.rs:70-71,146-147`). Party-ID normalization differs between stacks (CD-013) but party
IDs are never used as a lookup/join key anywhere in the codebase (only as React `key` props and an
OpenAPI export passthrough) — divergent IDs produce cosmetically different slugs, not broken
references. The report's "persisted-ID compatibility check" risk should be removed from CD-006's
scope. Also: the report's claim that legacy has a changelog-refresh capability modern lacks is
incorrect — `useContractDrift.ts` already provides an equivalent (arguably better) watcher-driven
refresh; no work needed there.

**Fix plan (sequenced):**
1. Delete the 8 fully-dead files (safe today, no decision needed).
2. Decide attachments/live-preview/version-syntax as a product question (separate from this
   plan's execution).
3. Build the missing `FolderNode` modern indicator and add click-to-open to `RequestNode`'s dot.
4. Remove `ContractBadge.tsx` from all three call sites and delete it plus `contract-store.ts`
   once nothing reads from it.

**Validation:** `grep -rn "contract-store" src/` returns nothing after step 4; existing
`contracts/` test suite plus `ContractBadge.test.tsx` removed/replaced by equivalent modern
coverage; `yarn tsc --noEmit`; `yarn test`.

---

## CD-005 — environment duplicate-key policy (downgraded to P2/P3)

**Verified:** PARTIALLY CONFIRMED, and largely already resolved. This session's earlier
`EnvironmentDialog.tsx` rewrite (extracting `VariableTable.tsx`/`EnvironmentSidebar.tsx`) already
deleted the block-with-toast duplicate-key check that disagreed with save-time collapse. Current
state: editing UI (`VariableTable.tsx`) allows duplicates and warns inline, persistence
(`dedupeVariables`) collapses to last-wins keeping first position, and the Rust resolver's
`HashMap` collect is also last-wins. **All three now agree.** The cross-boundary disagreement
that justified P0 severity no longer exists.

**What's left (optional, not urgent):** the policy is still implemented three separate times
(TypeScript UI counting, TypeScript dedup-on-save, Rust HashMap collect) rather than once. This is
now ordinary code-organization duplication, not a data-loss risk — track as P2/P3 cleanup, not on
this plan's critical path.

---

## Suggested execution order

1. Contract attachment deletion fix (NEW-P0) — smallest, most urgent, fully independent.
2. CD-004, CD-007, CD-008 — trivial, safe, no risk, can land same day.
3. CD-001 + CD-002 — highest-value correctness fix, needs the table-driven test suite described
   above before touching auto-save.
4. CD-003 — builds on the same "extract a shared mapper" pattern as CD-001/002.
5. CD-006 step 1 (delete dead files) — safe, do anytime; steps 2-4 depend on a product decision.
6. CD-005 — no longer urgent; revisit as general cleanup whenever convenient.
