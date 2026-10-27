# OpenAPI sync UX redesign

## Goal

Make OpenAPI sync a first-class, Bruno-like feature: its own tab with Overview, Spec updates and Your changes, a bulk review, Reset to spec with a restorable backup, View spec, clear notifications, and connecting an existing collection to a spec.

## Overview

OpenAPI import and sync work, but the user reports they are hard to use. The sync UI is the "Spec sync" section of the collection page (`src/components/collections/CollectionOverviewTab.tsx:127-133`, mounted at `:612-615`) next to Auth, Variables and Docs. The review list, the settings and the status share one panel, a notice on the overview is the only alert (`SpecSyncNotice.tsx:12`), and sync can only start by importing from a link (`entry.rs:147`). Bruno shows the target: a dedicated page opened from the collection menu with Overview, Collection Changes and Spec Updates tabs, Accept All / Skip All / Keep Current, Reset and View spec ([Bruno docs](https://docs.usebruno.com/open-api/openapi-sync)). The user chose that shape (see Decisions). Backend sync, consent and apply stay as built; this spec adds read models, a Reset path, a stored spec copy and a connect flow.

## Current state

| Area | Today | Reference |
|---|---|---|
| Entry | Section `spec-sync` of the collection tab; sidebar item "OpenAPI sync" opens it | `pane-types.ts:40`, `CollectionNode.tsx:310-320`, `:522-526` |
| Tab model | Dedicated tab types exist for `contract` and `git`; they are not persisted | `pane-types.ts:97-106`, `ui-state.ts:50`, `pane-store.ts:718-730` |
| Review | List + detail, per-item checkboxes, Skip, Apply, confirm and result dialogs | `SpecSyncTab.tsx:42`, `ApplyConfirmDialog.tsx:25` |
| Settings | Connection dialog (switch, frequency, token, trust, disconnect) | `SpecSyncConnectionDialog.tsx:293` |
| Alerts | Overview notice only; no sidebar badge, no toast; events consumed only inside the tab | `SpecSyncNotice.tsx:12`, `openapi-sync-queries.ts:289` |
| Spec copy | Only a pending copy while an update waits; `openapi-applied.spec` is reserved but never written | `file_tx.rs:10-16`, `openapi_sync_service.rs:695` |
| Your changes | Computable from source operations, baseline and the tree, but no command exposes it | `merge.rs:207`, `openapi_sync_apply.rs:47` |
| Reset / backup | None | survey section 4 |
| Connect existing | None; import always creates a new collection | `entry.rs:147` |

Gaps: hard to find, cramped, no overview, no Reset, no spec view, quiet updates, no way to connect a hand-made collection.

## Non-goals

- Changing consent, token or scheduler rules (Plans 9 and 11 stay as they are).
- Connecting from a local file (no remote location, so no sync).
- Diffing two spec versions; showing scripts, tests or assertions in Your changes.
- A multi-job redesign of the Import dialog.

## Design

Flow: sidebar menu or badge or toast opens the `openapi-sync` tab; the header shows status; Overview shows counts; Spec updates reviews and applies; Your changes lists differences and resets; View spec shows the stored spec; the sidebar menu connects a collection that has no source.

### 1. Dedicated tab

- `OpenApiSyncTab extends BaseTab { tabType: 'openapi-sync'; collectionName: string }`, guard `isOpenApiSyncTab`, added to the `Tab` union (`pane-types.ts:277`). `openOpenApiSyncTab(collectionName)` uses id `openapi-sync:${collectionName}` and title `OpenAPI sync — ${collectionName}`; it is added to the collection-name chain at `pane-store.ts:470-478`.
- Rendered lazily in `EditorGroup.tsx` before the fallthrough at `:322`; icon in `TabItem.tsx`; breadcrumb like the contract branch (`BreadcrumbBar.tsx:437-455`).
- Not persisted across restarts (same as `contract`). The `spec-sync` collection section, its `TABS` entry, `SpecSyncNotice` and `useSpecSyncAvailability` consumers in the overview are removed. A persisted `activeSection: 'spec-sync'` falls back to `overview` through `resolveCollectionSection` (`CollectionOverviewTab.tsx:111-125`).
- "Spec sync" becomes "OpenAPI sync" in every label.

### 2. Header and status chip

Header: collection name, chip, location, last checked, interval; buttons **Check now**, **View spec**, **···** (Edit connection settings, Disconnect). The settings dialog and consent banner are reused unchanged. Chip states, first match wins: Needs approval (amber, consent missing or location changed), Check failed (red, shows `lastMessage`), N updates (amber), Checks off (gray, interval off), Connected (green). The badge and toast use the same waiting count as the chip.

### 3. Overview tab

Four counts (Endpoints = source operations, In sync, Changed by you, Spec updates waiting), a "Review updates" banner when updates wait, and a facts table: spec link, spec version (`info.version` of the latest stored spec), last checked, automatic checks (link to Edit connection settings), last synced. Definitions: Endpoints = source operations; Changed by you = operations whose request differs from the baseline in spec-owned fields; In sync = Endpoints minus Changed by you minus operations with a waiting item; Spec updates waiting = plan items plus variable changes, skipped items not counted. When the baseline is missing or its `converter_version` differs, In sync and Changed by you show "–" (unknown) and the facts table says why. New command `openapi_sync_overview(collection)` returns these; it reads local files only (no network, no keychain). The waiting count is cached as `pending_count` in the local state by `check` and `apply`, so the badge command never parses a spec. `OpenApiLocalState` gains `last_synced_at: Option<String>`, `last_synced_updated: u32` and `pending_count: u32` (`#[serde(default)]`, skipped when default). `last_synced_*` are set on a finalized apply. The status DTO gains `pendingId` (first 12 hex of `pending_sha256`; new work, not reuse).

### 4. Spec updates tab (bulk-first)

Uses the existing plan and apply (`openapi_sync_plan`, `openapi_sync_apply`). Preselection on open: every New item and every Changed item without conflict is selected; Changed items with a Conflict field start unselected with "keep yours"; Removed items edited by the user are kept (never selected). Top bar: **Accept all** (select every New and Changed item and take the spec on every conflict field; the confirm warns "N of your edits will be overwritten"; a Removed item the user edited is still kept, never deleted), **Skip all** (existing skip, hidden until the spec changes), **Keep current** (select nothing now), and **Confirm and sync (N)**. Confirm opens the existing confirm dialog (created / updated / deleted / kept counts), then the existing result dialog. Per-endpoint detail, field choices, rename links and search stay as built.

### 5. Your changes tab

New command `openapi_sync_changes(collection)` returns `modified` (source operation whose current request differs from its baseline request, via `request_is_edited`-style comparison limited to spec-owned fields), `added` (requests whose uid is not in any source operation), `deleted` (source operation whose uid is gone), and `backups` (section 6). Rows show kind, method, path and the changed field names. Added and deleted rows have no Reset.

### 6. Reset to spec with a restorable backup

- **Fields reset** (spec-owned): name, method, url, description, headers, query, path params, form, body, auth. **Kept**: scripts, tests, assertions, actions, docs, variables, examples, settings, tags. (Call: reset targets the last synced spec, the baseline request, not the waiting version; waiting changes come through Spec updates. The baseline holds the spec's converted request for each key as of the last apply that applied that key (`apply.rs:366-377` replaces it with the spec request whatever fields the user kept), so Reset is correct; a skipped or unselected item keeps its older baseline, and Reset then returns to that older spec version, which the confirm names as "the last synced version".)
- Command `openapi_sync_reset(collection, keys: Vec<String>)` runs under the collection lock and one file transaction. For each key it writes a backup entry, then replaces the spec-owned fields of the request with the baseline request's fields. Unavailable (clear message) when the baseline is missing or `converter_version` differs.
- Backup file `.rocket/openapi-reset-backups.json` (added to `OWNED_STATE_FILES`, the `.rocket` git-ignore text, and disconnect): `{ version, entries: [{ key, uid, request_path, saved_at, request: Request }] }`, snake_case, `#[serde(default)]`.
- Your changes shows a **Reset by you** group: **Restore my version** (writes the backed-up request back, removes the entry) and **Discard backup**. A backup is removed when a sync updates that endpoint, on Restore, on Discard, and on Disconnect. No age limit. **Reset all to spec** asks one confirm with the count.
- Confirm text: "Reset GET /orders to the spec? URL and header Accept go back to the spec's version. Your scripts, tests, assertions and docs are kept. A backup is kept until the next sync."

### 7. View spec

- Stored copy `.rocket/openapi-applied.spec`: written at import (`entry.rs` after `sources.save`) and in the same transaction as a finalized apply (`openapi_sync_apply.rs:~249`); a partial apply (not finalized) leaves it and the pending copy as they are. Overview's spec version and View spec use the pending copy when present, else the applied copy. For an older import without the copy, a consented check that finds no pending spec fetches once without validators and stores the text as the applied copy only when its hash equals `source.spec_sha256`; otherwise the normal pending rules apply and no applied copy is written. Opening Overview or View spec never fetches.
- Command `openapi_sync_spec_view(collection)` returns `{ title, version, fetchedAt, state: 'in_sync' | 'updates_waiting', groups: [{ tag, operations: [{ key, method, path, summary, parameters, responses, isNew }] }] }` from the pending copy when present, else the applied copy (decision: latest fetched spec). `ParsedSpec` gets a public read accessor inside `rocket-import`.
- The viewer is a read-only page with tag groups, search and a parameters/responses detail; large specs list operations lazily (first 500 per tag, then "Show more").

### 8. Notifications

- Sidebar: a count badge on the collection row. A cheap command `openapi_sync_badges()` returns `{ collection: waitingCount }` from local files only; it refreshes on `openapi-sync-checked` and `openapi-sync-applied` through one always-mounted listener in `App.tsx`.
- Toast (`sonner`, `App.tsx:271`): when a check event reports `changed`, show "{Collection} has N spec updates" with **Review**; once per `pendingId` (first 12 hex of `pending_sha256`, exposed in the status DTO), remembered in `localStorage`.
- `SpecSyncNotice` is deleted.

### 9. Connect an existing collection

- Menu item **Connect to an OpenAPI spec** on collections without `openapi-source.yml` (HTTP requests only).
- Step 1 dialog: spec link (GitHub or https), optional token, group-by for new requests, "Check for spec updates" checkbox. `openapi_connect_preview(collection, link, token?)` fetches, bundles and parses (as `entry.rs:147-237`), matches, writes nothing, and returns `{ matched, suggested, inSpecOnly, yoursOnly, ambiguous }`.
- **Matching:** normalise a request URL (drop a leading `{{var}}` or the server origin, drop the query, drop a trailing slash, treat `:id`, `{id}` and `{{var}}` segments as equal path parameters) and compare `METHOD` + path with each operation's template. One request to one operation is **Matched**. A request equal to several operations, or an operation matched by several requests, is **Ambiguous** (the user picks). No path match but same method and name equal to the operation summary or operationId (case-insensitive) is **Suggested** (not preselected). Operations with no request are **In spec only** (default Add). Requests with no operation are **Yours only** (default Keep).
- Step 2 is the Initial sync review (wireframe agreed with the user): each row has a select to Link / Add / Skip / Keep. Skip on an In spec only row records a skip (key and fingerprint) in the local state so the operation does not come back as New; Keep on a Yours only row does nothing. For a Linked row, `SourceOperation.baseline_hash` is `baseline_hash(&spec_request)` from `converter/openapi.rs:156`, and the spec request is built exactly as at import (same `base_url`, `group_by`, `CONVERTER_VERSION`); `request_path` is the request's current path. `openapi_connect_confirm(collection, link, token?, rows, group_by, interval?)` runs under the collection lock and one transaction: it writes the source, the baseline (matched rows get `baseline.request` = the spec-converted request, so differences read as user edits), the applied spec copy, the new requests, consent for the typed link, and the optional interval. The files are one transaction (source, baseline, applied copy, new requests, skips) and are committed first; then consent is granted, then the interval is set, then the optional token is stored in the keychain (as `import_openapi_url` does). A failure after the commit never undoes the files; it adds a fixed warning ("Checks are not enabled yet", "The token was not saved"), so no compensating revoke is needed because consent is granted last. A failed commit writes nothing.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Collection has no source | No sidebar entry for sync; tab not openable; Connect offered instead |
| Consent missing / location changed | Banner in the header; Check now, interval, token disabled; Spec updates and Your changes still work |
| No baseline or converter version differs | Reset disabled with "These requests cannot be reset because the sync data is from an older version" |
| Reset while an apply runs | Conflict; fixed sentence |
| Request file changed on disk during Reset | Transaction recheck fails; nothing written; error shown |
| Restore when the request was deleted | Recreated at `request_path`; if the path is taken, a numeric suffix |
| Backup entry for an endpoint a sync updated | Entry removed in the same transaction |
| Old import without `openapi-applied.spec` | View spec says "No saved copy yet"; the next check stores it |
| Spec past the existing limits | Existing check failure behaviour; Connect preview shows the fixed limit sentence |
| Connect: link fetch fails | Preview shows the fixed failure sentence; nothing written |
| Connect: collection changed between preview and confirm | Digest of the request list is checked (sha256 over uid, request path, method and url of every HTTP request; the preview is stateless, confirm sends the rows and the link back); mismatch asks to run the preview again |
| Connect: zero matches | All rows In spec only / Yours only; user can still confirm |
| Toast for the same `pendingId` | Shown once per browser storage (`localStorage`) |
| Badge for a collection with a corrupt state file | No badge; tab shows the unreadable-state message |

## Testing

- `rocket-collection`: `OpenApiLocalState` round trip with `last_synced_*`; backups file round trip and old-file defaults; `OWNED_STATE_FILES` lists the backups file.
- `rocket-import`: matching tests (`matches_get_orders_with_variable_prefix`, `treats_colon_and_brace_params_as_equal`, `ambiguous_when_two_requests_match_one_operation`, `suggests_by_name_only_when_path_differs`, `ignores_graphql_and_grpc_requests`); reset field-set test (`reset_restores_only_spec_owned_fields`); spec view model test.
- `rocket-app` (`openapi_sync_changes`, `openapi_sync_reset`, `openapi_sync_overview` tests): modified/added/deleted lists, backup written then restored, backup dropped by sync, reset refuses without baseline, no write on preview, confirm rolls back on failure, import writes the applied copy, finalized apply refreshes it, check self-heals a missing copy.
- `src-tauri`: DTO shape tests and consent/lock behaviour for the new commands (no network without consent; preview needs a typed link).
- Frontend: tab registration and opener, header chip states, Overview counts, Accept all / Skip all / Keep current / preselection, confirm counts, Your changes lists, Reset and Restore flows with the exact confirm text, View spec lazy list, badge and toast once per `pendingId`, Connect step 1 and step 2, removal of the old section and notice.
- Commands: `cargo test -j4 -p <crate> <filter>` per crate, `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

## Files changed

- Rust: `crates/rocket-collection/src/{openapi_local.rs,file_tx.rs (`OWNED_STATE_FILES` grows from 5 to 6; the disconnect tests that list it are updated),openapi_backup.rs (new)}`, `crates/rocket-infra/src/{rocket_dir.rs,fs_openapi_backup_repo.rs (new)}`, `crates/rocket-import/src/openapi/{entry.rs,matching.rs (new),sync/spec.rs (public read accessor on `ParsedSpec`; `doc()` is `pub(crate)` today),sync/apply.rs}`, `crates/rocket-app/src/{openapi_sync_apply.rs,openapi_sync_service.rs,openapi_sync_view.rs,openapi_connect.rs (new)}`, `src-tauri/src/commands/{openapi_sync.rs,import.rs}`, `src-tauri/src/lib.rs`.
- TypeScript: `src/types/pane-types.ts`, `src/stores/pane-store.ts`, `src/components/panes/{EditorGroup,TabItem,BreadcrumbBar}.tsx`, `src/components/collections/{CollectionNode,CollectionOverviewTab}.tsx`, `src/components/collections/openapi-sync/*` (moved and extended from `spec-sync/`), `src/lib/{tauri-api.ts,queries/openapi-sync-queries.ts}`, `src/App.tsx`, `src/components/import/ImportCollectionDialog.tsx` (labels only).
- Docs: `crates/rocket-import/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, the plan index, and a feature doc `docs/features/openapi-sync-ux.md`.

## Constraints

- Tasks touching collection files, requests or the listed crates start with a read of `docs/superpowers/specs/opencollection-spec-reference.md`.
- Cargo with `-j4 -p <crate>`; no unwrap calls in production paths; camelCase serde on IPC DTOs only; new persisted fields use `#[serde(default)]`; shadcn/ui and lucide only; narrow Zustand selectors.
- Consent rules unchanged: no network, token read or interval change without matching consent; apply, skip, reset and restore never contact the network. Connect grants consent only for the link the user typed.
- No raw backend error text or spec content in IPC messages; fixed sentences only.
- All writes go through the existing file transaction and the shared collection lock.

## Acceptance criteria

- OpenAPI sync opens as its own tab from the sidebar menu, the badge click and the toast; the old section and notice are gone.
- The header shows the five chip states and Check now, View spec and the ··· menu.
- Overview shows the four counts, the banner and the facts table, with data from local files only.
- Spec updates preselects safe items, keeps conflicts as "keep yours", and offers Accept all, Skip all, Keep current and Confirm and sync (N).
- Your changes lists modified, added and deleted requests without any pending spec.
- Reset to spec restores only spec-owned fields, writes a backup, and Restore my version and Discard backup work; a sync that updates an endpoint removes its backup.
- View spec shows the latest stored spec offline with a header that says whether updates wait.
- The sidebar badge shows the waiting count; the toast shows once per new spec version.
- A collection without a source can be connected through the preview, the Initial sync review and a transactional confirm.
- All new commands honour the consent and lock rules; no new command returns raw error text.

## Out of scope

Connecting from a local file; version-to-version spec diff; script/test tracking in Your changes; changing the Import dialog layout; Bruno's irreversible full-overwrite sync.

## Follow-ups

- Cap the flag-driven refetch of incomplete remote refs at short intervals.
- Lenient read of the source id so disconnect can remove the token and consent of an unparsable source.
- A "continue without this file" action for stuck remote refs.
- GitHub Enterprise link detection.

## Decisions

- What bothers you most about the current Spec sync? Options: (A) hard to find and cramped inside the collection page, (B) the review and apply flow is confusing, (C) settings and status are scattered. Chosen: A. Decided by: user
- Where does OpenAPI sync live? Options: (A) its own tab with inner tabs, closest to Bruno, (B) stay a section of the collection page with a nicer layout. Chosen: A. Decided by: user
- How do review and apply work in Spec updates? Options: (A) bulk-first with Accept all, Skip all, Keep current and one Confirm and sync, safe changes preselected, conflicts start as keep yours, detail on demand, (B) per-endpoint review with nothing preselected, (C) a step-by-step wizard. Chosen: A, after seeing a mockup. Decided by: user
- How much of Bruno's page now? Options: (A) Overview, Spec updates and a read-only Your changes, (B) A plus Reset to spec per changed endpoint, (C) B plus View spec, a read-only spec browser. Chosen: C, after seeing a wireframe. Decided by: user
- How do users learn updates are waiting? Options: (A) sidebar count badge, a one-time toast per new spec version and a status chip in the tab, with the overview notice removed, (B) badge and chip only, (C) keep the overview notice and add the badge. Chosen: A, after seeing wireframes. Decided by: user
- How safe is Reset to spec? Options: (A) confirm only, git is the undo, (B) confirm plus a 30 second Undo toast, (C) confirm plus a backup kept until the next sync with a Restore button in a Reset by you group. Chosen: C, after seeing wireframes. Rules: the backup is kept until the next successful sync that touches that endpoint or until the user clicks Discard backup; Restore always puts the user's version back, even if the spec changed; no age limit; backups are local and git-ignored. Decided by: user
- Which spec version does View spec show? Options: (A) the latest spec Rocket has fetched, with a header that says whether updates are waiting, (B) the synced version with a switch to the waiting one. Chosen: A, after seeing wireframes. Rocket saves a local copy at import and after each sync so it works offline. Decided by: user
- Connecting an existing collection? Options: (A) not in this job, (B) a simple connect that only adds new requests, (C) full Bruno-style connect with matching and an Initial sync review. Chosen: C, after seeing wireframes. Decided by: user
- Reset restores the last synced version (baseline), not the waiting version. Decided by: call
- Connect accepts a link only (GitHub or https), not a local file. Decided by: call
- Matching rules (method + normalised path; name only as a suggestion; ambiguous goes to the user). Decided by: call
- Accept all takes the spec on conflicts and warns first; Keep current selects nothing. Decided by: call
- Toast dedupe by `pendingId` in `localStorage`; badge from a local-files-only command. Decided by: call
- The tab is not persisted across restarts, like `contract`. Decided by: call
- The work is split into two plan series because a chain holds at most 6 plans. Series 1 (`openapi-sync-ux`): read models (overview, changes, badges, `pendingId`, `last_synced_*`, `pending_count`), the dedicated tab, header and chip, Overview, Spec updates, a read-only Your changes list, badge and toast, label renames and removal of the old section. Series 2 (`openapi-sync-ux-2`): the applied spec copy and View spec, Reset with backups and Restore, Connect with matching. Decided by: call
- Chip precedence, count definitions with unknown shown as a dash, the Connect commit order (files, then consent, interval, token), and the stateless preview digest. Decided by: call
