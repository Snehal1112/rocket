# Semantic duplication audit: `contract` and `contracts`

## Scope and method

- Audited production code under:
  - `src/components/contract`
  - `src/components/contracts`
- Used `docs/reports/component-duplication/function-catalog.json` as the function index, then read every production implementation in both folders in full.
- Included duplication within each folder and between the two folders.
- Excluded test-only duplication. Tests were considered only as evidence about survivor coverage.
- Checked repository call sites to distinguish active code from an older parallel implementation.

## Executive summary

The dominant duplication is not a set of isolated helpers: it is two parallel contract UIs built around two contract stores/models. The active pane uses `src/components/contracts/ContractsTab.tsx` and `src/components/contracts/ContractDiffPane.tsx` from `src/components/panes/EditorGroup.tsx:191-201`. No external production caller was found for `src/components/contract/ContractTab.tsx`; its child components are reachable only through that legacy tab. The notable exception is `src/components/contract/ContractBadge.tsx`, which is still imported by collection, folder, and request tree nodes.

Recommended direction: treat `src/components/contracts` as the survivor, port the still-needed sidebar badge to the modern contract model/store, preserve any genuinely missing behavior such as document attachments or live preview only if product requirements still call for it, and then retire the unused singular-folder feature stack.

### Priority summary

| Priority | Finding | Confidence | Recommended survivor |
|---|---|---:|---|
| P0 | Parallel contract feature stacks | High | `src/components/contracts` |
| P0 | Create/edit forms and submit mapping | High | `NewContractModal` |
| P1 | Contract cards | High | `contracts/ContractCard.tsx` |
| P1 | Changelog detail tables | High | `ContractDiffPane` for tabular diff; drawer remains a separate timeline view |
| P1 | Party ID/build helpers | High | Modern normalized party builder, extracted from `NewContractModal` |
| P1 | Version increment logic | High | One shared pure helper |
| P1 | Repeated contract-group rendering | High | One data-driven group renderer in `ContractsTab` |
| P2 | Contract empty states | High | `ContractsEmptyState` |
| P2 | Month/day date formatting | High | One shared pure formatter |
| P2 | Change-kind chips | High | `ChangeChip` |
| P2 | Relative-time formatting | Medium | Shared formatter with explicit display modes |
| P2 | Avatar rendering | Medium-high | Generalized `PartyAvatar`/shared avatar primitive |
| P3 | Repeated more-actions controls | High | One local `MoreActions` component |
| P3 | Changelog kind aggregation | Medium-high | One pure count helper if both views temporarily remain |

---

## Findings

### 1. Parallel top-level contract feature stacks

**Confidence:** High  
**Scope:** Between folders; architectural duplication

**Exact references**

- Legacy collection-scoped tab and all view orchestration: `src/components/contract/ContractTab.tsx:52-361`
- Modern collection-scoped tab and all view orchestration: `src/components/contracts/ContractsTab.tsx:32-553`
- Active pane routing to the modern tab and diff pane: `src/components/panes/EditorGroup.tsx:191-201`
- Legacy sidebar entry point that still opens the same contract pane: `src/components/contract/ContractBadge.tsx:24-84`

**Intent**

Both tabs load contracts for a collection, render an empty/list state, create and edit contracts, delete contracts, and expose changelog/diff navigation.

**Differences**

- The legacy tab uses `useContractStore` and IPC-shaped types from `@/lib/tauri-api`; the modern tab uses `useContractsStore` and `@/types/contracts`.
- The modern tab adds status grouping, summary counts, filtering/sorting, keyboard navigation, telemetry, export, lifecycle actions, drift recomputation, a modal editor, and a changelog drawer.
- The legacy tab adds document attachments, a live card preview, and a direct collection-change subscription that reloads changelogs.
- Repository call-site search found the modern tab wired into `EditorGroup`; no external production caller was found for the legacy `ContractTab`.
- `ContractBadge` is not dead with the rest of the singular folder: collection tree components still import it.

**Recommendation / survivor**

Keep `src/components/contracts/ContractsTab.tsx` and the modern contracts store/model. Do not merge the two tab components. Instead, identify whether attachments, live preview, and watcher-driven refresh are still required; port only those requirements into the modern flow. Migrate `ContractBadge` separately before deleting the singular folder.

**Staged remediation**

1. Add a small characterization checklist/test for modern create, edit, delete, diff, and collection switching.
2. Decide explicitly whether legacy attachments, live preview, and collection-change refresh remain product requirements.
3. Port approved missing behavior to modern components and modern store/types.
4. Rewrite `ContractBadge` against `useContractsStore` and modern `Contract` status semantics.
5. Remove `ContractTab` and legacy-only children once production imports and pane routing are clean.
6. Remove the legacy store/model only after a repository-wide usage audit outside these folders.

### 2. Create/edit contract forms and submit mapping

**Confidence:** High  
**Scope:** Between folders; nested inside finding 1 but independently actionable

**Exact references**

- Legacy form model and UI: `src/components/contract/ContractForm.tsx:15-294`
- Legacy edit hydration and submit validation/mapping: `src/components/contract/ContractTab.tsx:145-229`
- Modern form model, validation, edit hydration, and submit mapping: `src/components/contracts/NewContractModal.tsx:37-123`, `src/components/contracts/NewContractModal.tsx:125-308`
- Modern form UI, including duplicated scope controls: `src/components/contracts/NewContractModal.tsx:340-637`

**Intent**

Both implementations collect contract title/name, version, provider, consumers, scope, effective date, and expiry; hydrate edit state; validate required fields; build contract parties and scope; and dispatch create/update operations.

**Differences**

- Legacy supports one consumer and document attachments; modern supports comma-separated multiple consumers and policy/SLA fields.
- Legacy hard-codes strict policy and 30 notice days; modern exposes policy, notice days, SLA, draft/publish, telemetry, and drift recomputation.
- Legacy version examples permit `v1.0`; modern validation expects a semver-like `x.y.z` prefix.
- Scope selector markup and collection-tree loading are nearly the same, but are owned by different parent flows.
- Error handling differs: one global message in legacy versus field errors/warnings in modern.

**Recommendation / survivor**

Keep `NewContractModal` as the feature owner. Extract pure form conversion/validation helpers from it before adding any legacy-only fields. If attachments are retained, add them to the modern form rather than preserving `ContractForm` as a second editor.

**Staged remediation**

1. Extract and test `contractToFormState`, validation, scope construction, and party construction from `NewContractModal`.
2. Reconcile accepted version syntax and consumer cardinality with backend/domain rules.
3. Port attachments only if still required, including edit retention/removal semantics.
4. Add create/edit integration coverage for collection, folder, and request scopes.
5. Delete `ContractForm` and the create/edit branches of legacy `ContractTab` with the rest of the legacy stack.

### 3. Contract card implementations

**Confidence:** High  
**Scope:** Between folders

**Exact references**

- Legacy card: `src/components/contract/ContractCard.tsx:20-220`
- Modern card: `src/components/contracts/ContractCard.tsx:184-638`
- Legacy live preview coupling: `src/components/contract/ContractLivePreview.tsx:21-56`

**Intent**

Both render the principal contract summary: name/title, version, status, provider/consumer relationship, dates, scope, changelog state, and edit/delete/changelog actions.

**Differences**

- Modern card supports the full status machine, multiple consumers, policy/SLA metadata, grouped context actions, keyboard focus, telemetry, mini changelog, and status-specific CTAs.
- Legacy card supports attached-document display/opening and a `preview` mode.
- Legacy derives a three-state status through `useContractStore`; modern consumes the richer persisted status and shared primitives (`ContractStatusChip`, `ContractParties`, `ScopeTag`, etc.).
- Modern card has dedicated tests; legacy card itself has no colocated production-facing test in the audited folder.

**Recommendation / survivor**

Keep `src/components/contracts/ContractCard.tsx`. If document attachments remain supported, add an attachment subcomponent to the modern card. Do not make the modern card absorb legacy preview construction unless live preview is confirmed as a current requirement.

**Staged remediation**

1. Record which legacy card capabilities are still visible in the shipped UI.
2. Add attachment display/open behavior to modern types and card only if required.
3. If preview remains desired, introduce an explicit modern preview DTO/variant rather than fabricating a legacy `Contract`.
4. Move any missing behavior under modern card tests.
5. Remove the legacy card and `ContractLivePreview` after the legacy form/tab is retired.

### 4. Contract empty states

**Confidence:** High  
**Scope:** Between folders

**Exact references**

- Legacy empty state: `src/components/contract/ContractEmptyState.tsx:9-26`
- Modern empty state: `src/components/contracts/ContractsEmptyState.tsx:8-37`
- Legacy use: `src/components/contract/ContractTab.tsx:237-260`
- Modern use: `src/components/contracts/ContractsTab.tsx:420-431`

**Intent**

Both fill the contracts pane when a collection has no contracts, explain contract snapshots/change tracking, and invoke the create flow through a primary CTA.

**Differences**

- Copy and illustration differ.
- Modern exposes a disabled OpenAPI import affordance and scope hint.
- The callback names differ but trigger the same create-modal intent.

**Recommendation / survivor**

Keep `ContractsEmptyState`. Its copy aligns with the active modern workflow and it is already wired to telemetry by its parent.

**Staged remediation**

1. Confirm final product copy and whether the disabled import control should remain.
2. Ensure modern empty-state CTA coverage remains in `ContractsTab` tests.
3. Delete `ContractEmptyState` with the legacy tab.

### 5. Tabular changelog/diff views

**Confidence:** High  
**Scope:** Between folders

**Exact references**

- Legacy changelog table: `src/components/contract/ChangelogTable.tsx:16-94`
- Legacy changelog loading/view: `src/components/contract/ContractTab.tsx:316-359`
- Modern diff table and loading/accept workflow: `src/components/contracts/ContractDiffPane.tsx:41-188`
- Separate modern timeline view: `src/components/contracts/ChangelogDrawer/ChangelogTimeline.tsx:48-109`

**Intent**

`ChangelogTable` and `ContractDiffPane` both fetch/receive the IPC changelog entry shape and present timestamp, field/change kind, old value, and new value in a table.

**Differences**

- Modern table includes request path, breaking-change flags, loading/error/retry states, truncation, and an accept/re-sign CTA.
- Legacy table receives already-loaded entries and displays complete old/new values in separate columns.
- The modern drawer timeline is not a duplicate of the table: it is a compact, grouped, filterable history presentation backed by the modern in-memory changelog model.

**Recommendation / survivor**

Keep `ContractDiffPane` for the tabular review workflow and keep the drawer timeline as an intentionally different presentation. Retire `ChangelogTable`; if complete-value visibility matters, add expandable cells or reuse `ChangelogDiffBlock` in `ContractDiffPane`.

**Staged remediation**

1. Add tests for mapping IPC `changeType` to modern `ChangeKind`, empty/error states, and breaking counts.
2. Decide whether truncation needs tooltip/expansion to preserve legacy visibility.
3. Route all “review diff/open contract” actions to `ContractDiffPane`.
4. Delete `ChangelogTable` and the legacy changelog branch.

### 6. Party ID normalization and party construction

**Confidence:** High  
**Scope:** Between folders

**Exact references**

- Legacy single-party conversion: `src/components/contract/ContractTab.tsx:19-26`
- Legacy create/update use: `src/components/contract/ContractTab.tsx:185-221`
- Modern slug and multi-party builder: `src/components/contracts/NewContractModal.tsx:58-72`
- Modern provider/consumer construction: `src/components/contracts/NewContractModal.tsx:232-237`

**Intent**

Both convert user-entered party names into `{ id, name, kind: 'team' }` objects for contract persistence.

**Differences**

- Legacy lowercases and replaces literal spaces only; it does not trim, collapse whitespace, or remove punctuation.
- Modern trims, collapses whitespace, strips non-ASCII alphanumeric punctuation, and supports comma-separated consumers.
- Modern provider construction duplicates the object creation performed for each consumer instead of calling one shared party builder.
- Different normalization can create different IDs for the same visible name, e.g. leading spaces, repeated spaces, or punctuation.

**Recommendation / survivor**

Use the modern normalization behavior, but extract a single `partyFromName`/`partyIdFromName` helper in the contract domain/frontend adapter layer and use it for both provider and consumers. Confirm Unicode expectations before freezing the current ASCII-only regex.

**Staged remediation**

1. Add table-driven tests for whitespace, punctuation, Unicode, and empty names.
2. Choose and document canonical party-ID normalization compatible with persisted data.
3. Extract one pure helper and make `buildParties` delegate to it.
4. Migrate provider construction and any remaining legacy call sites.
5. Remove the legacy helper only after persisted-ID compatibility is confirmed.

### 7. Month/day date formatter implemented three times

**Confidence:** High  
**Scope:** Within `contracts`

**Exact references**

- `formatMonthDay`: `src/components/contracts/ContractCard.tsx:82-91`
- `formatSinceDate`: `src/components/contracts/MiniChangelog.tsx:45-54`
- `formatSince`: `src/components/contracts/ChangelogDrawer/ChangelogDrawerFooter.tsx:53-62`

**Intent**

All three convert a date-only ISO string to US English abbreviated month plus numeric day.

**Differences**

There is no meaningful behavioral difference; the implementations are line-for-line equivalent apart from function names.

**Recommendation / survivor**

Extract one pure formatter, e.g. `formatContractMonthDay`, into the existing contract utility area and replace all three local functions. No component-local function is a materially better survivor.

**Staged remediation**

1. Add tests for valid date-only input and chosen invalid-input behavior.
2. Extract the helper without changing locale/output.
3. Replace all three call sites.
4. Remove the local helpers.

### 8. Change-kind chip duplicated inside the changelog drawer

**Confidence:** High  
**Scope:** Within `contracts`

**Exact references**

- Shared change chip: `src/components/contracts/ChangeChip.tsx:9-35`
- Local duplicate `KindChip`: `src/components/contracts/ChangelogDrawer/ChangelogEntry.tsx:35-55`
- Local duplicate use: `src/components/contracts/ChangelogDrawer/ChangelogEntry.tsx:87-89`
- Existing shared uses: `src/components/contracts/ContractDiffPane.tsx:152-154`, `src/components/contracts/MiniChangelog.tsx:112-118`

**Intent**

Both map `add`/`remove`/`modify` to `ADD`/`REM`/`MOD` and render a compact color-coded kind indicator.

**Differences**

- `ChangeChip` includes a border, mono font, `shrink-0`, and shared `className` support.
- `KindChip` uses slightly different opacity, rounding, and font weight and has no border.
- The semantic mapping and labels are identical.

**Recommendation / survivor**

Keep `ChangeChip`. Add an optional visual variant only if the drawer genuinely needs a borderless treatment; otherwise use the default shared rendering for consistency.

**Staged remediation**

1. Compare the intended drawer styling against the design spec/screenshots.
2. Add a `variant` prop only if the visual difference is intentional.
3. Replace `KindChip` with `ChangeChip`.
4. Delete `KindChip` and retain one label/color mapping.

### 9. Version increment logic is duplicated and behavior has diverged

**Confidence:** High  
**Scope:** Within `contracts`

**Exact references**

- Card preview helper: `src/components/contracts/ContractCard.tsx:110-121`
- Display use for accept/propose label: `src/components/contracts/ContractCard.tsx:489-509`
- Actual accept-drift version mutation: `src/components/contracts/ContractsTab.tsx:135-145`

**Intent**

Both bump the last dot-separated numeric version segment while preserving an optional `v` prefix.

**Differences**

- `nextVersion` returns `${version}-next` when the final segment is not numeric.
- The inline `ContractsTab` implementation leaves a nonnumeric version unchanged.
- Therefore the version preview shown on the card can disagree with the version actually submitted by `acceptDrift`.
- Both use permissive `parseInt`, so a segment such as `1x` is treated as `1`; this should be an explicit policy rather than accidental behavior.

**Recommendation / survivor**

Extract one tested `nextContractVersion` helper and use it for both display and mutation. Choose the fallback behavior deliberately; the displayed value and persisted value must always match.

**Staged remediation**

1. Define expected behavior for semver, `v`-prefixed versions, shortened versions, prereleases, and malformed values.
2. Add table-driven tests.
3. Extract one helper and replace both implementations.
4. Consider using the same validation policy in `NewContractModal`.

### 10. Relative-time formatting is reimplemented with inconsistent contracts

**Confidence:** Medium  
**Scope:** Within `contracts`

**Exact references**

- Compact recent-entry timestamp: `src/components/contracts/MiniChangelog.tsx:25-43`
- Drawer “ago” timestamp: `src/components/contracts/ChangelogDrawer/ChangelogEntry.tsx:17-33`
- Card drift/review/footer timestamps: `src/components/contracts/ContractCard.tsx:123-172`

**Intent**

All implementations turn changelog/contract timestamps into concise human-readable relative time.

**Differences**

- `entryTimestamp` omits “ago,” compacts units, and switches to an absolute month/day date after 30 days.
- `timeAgo` always includes “ago” and approximates months/years manually.
- Card helpers use `date-fns`, selectively remove “about,” and vary `addSuffix` by call site.
- Invalid inputs fall back to either `—`, “No changes recorded,” or “Awaiting review.” Those are domain-specific empty/error messages, not just formatting differences.

**Recommendation / survivor**

Do not collapse these into one hard-coded output. Extract a shared relative-time formatter with explicit modes such as `compact`, `withSuffix`, and `absoluteAfterDays`; keep domain fallback copy in each caller.

**Staged remediation**

1. Document expected output examples for each surface and boundary (59 minutes, 24 hours, 30 days, one year).
2. Add deterministic tests using a fixed clock.
3. Implement one formatter around `date-fns` with explicit options.
4. Replace the three calculations while preserving caller-specific fallback copy.

### 11. Author avatar rendering duplicates the party avatar primitive

**Confidence:** Medium-high  
**Scope:** Within `contracts`

**Exact references**

- Shared party avatar: `src/components/contracts/PartyAvatar.tsx:9-20`
- Duplicate author color/initial derivation: `src/components/contracts/ChangelogDrawer/ChangelogEntry.tsx:70-74`
- Duplicate author avatar markup: `src/components/contracts/ChangelogDrawer/ChangelogEntry.tsx:124-135`

**Intent**

Both derive deterministic background color and initials from a name and render a circular text avatar.

**Differences**

- `PartyAvatar` requires a full `Party`, defaults to 20 px, and accepts an explicit `avatarColor`.
- Changelog entries have only author name/seed, render at 16 px, and use `?`/gray fallback behavior.
- The changelog can use `authorAvatarSeed`, which is not equivalent to `party.name`.

**Recommendation / survivor**

Generalize the shared avatar primitive to accept `name`, optional color seed/override, size, and fallback text; keep `PartyAvatar` as a thin adapter if that preserves a useful domain API. Do not fabricate a `Party` in the changelog solely to reuse JSX.

**Staged remediation**

1. Extract a generic name-avatar primitive with tests for seed, override color, initials, and missing name.
2. Make `PartyAvatar` delegate to it.
3. Replace changelog author markup with the generic primitive.
4. Preserve the current 16 px/20 px visual differences through props.

### 12. Contract group rendering repeats the same card-list algorithm four times

**Confidence:** High  
**Scope:** Within `contracts`, inside one function

**Exact references**

- “Needs attention” block: `src/components/contracts/ContractsTab.tsx:443-460`
- “Active” block: `src/components/contracts/ContractsTab.tsx:462-480`
- “Inactive” block: `src/components/contracts/ContractsTab.tsx:481-499`
- “Archived” block: `src/components/contracts/ContractsTab.tsx:500-521`

**Intent**

Each block conditionally renders a `ContractsGroupHeader`, maps contracts to the same `ContractCard` props, computes a global keyboard-navigation index, stores the card ref, and checks focus.

**Differences**

Only the group label, contract array, and cumulative index offset differ.

**Recommendation / survivor**

Replace the four blocks with a group descriptor array and one renderer. Preserve the current flattened ordering and global index because keyboard shortcuts depend on `allCards` using the same order.

**Staged remediation**

1. Add/retain a keyboard-navigation test spanning group boundaries.
2. Build descriptors such as `{ label, contracts }` in the exact current order.
3. Render groups while maintaining a deterministic cumulative index (or build an indexed flattened representation once).
4. Verify refs, focused state, and `allCards` order remain aligned.

### 13. More-actions dropdown trigger is repeated across card status branches

**Confidence:** High  
**Scope:** Within `contracts`, inside one function

**Exact references**

- Paused branch: `src/components/contracts/ContractCard.tsx:423-452`
- Drift/breach branch: `src/components/contracts/ContractCard.tsx:495-526`
- In-review branch: `src/components/contracts/ContractCard.tsx:534-563`
- Default branch: `src/components/contracts/ContractCard.tsx:577-607`

**Intent**

Each branch renders the same `ContractDropdownMenu` around the same icon button, with identical props, classes, click propagation handling, accessibility label, and icon.

**Differences**

There is no meaningful difference among the four copies. The expired branch intentionally does not show the dropdown.

**Recommendation / survivor**

Extract a local `MoreActions` component or helper in `ContractCard.tsx` that owns `ContractDropdownMenu` plus its trigger button. Render it only in statuses that currently include it.

**Staged remediation**

1. Extract the exact current markup without changing status conditions.
2. Replace the four copies.
3. Verify click propagation and menu actions in the existing card tests.
4. Consider a later, separate cleanup of the larger status-specific footer; do not combine that behavioral refactor with this mechanical deduplication.

### 14. Changelog kind aggregation is recomputed in parallel views

**Confidence:** Medium-high  
**Scope:** Between folders and within the modern changelog surface

**Exact references**

- Legacy total/removed/added/changed counts: `src/components/contract/ChangelogSummaryBar.tsx:7-38`
- Modern total/breaking/add/remove/modify counts: `src/components/contracts/ChangelogDrawer/ChangelogDrawerToolbar.tsx:66-76`
- Modern summary consumes separately precomputed change counts: `src/components/contracts/ContractsSummaryRow.tsx:41-45`

**Intent**

The first two implementations aggregate changelog entries by change kind for presentation; the summary row presents the same add/remove/modify taxonomy at collection level from store-provided counts.

**Differences**

- Legacy names are `added`/`removed`/`changed`; modern names are `add`/`remove`/`modify` and add `isBreaking`.
- The legacy summary is passive metrics; the modern toolbar uses counts in filter controls.
- `ContractsSummaryRow` operates on collection-wide counts rather than one contract’s entries, so it should not directly own entry reduction.

**Recommendation / survivor**

If the legacy view is removed promptly, no new abstraction is needed for it. Within modern code, centralize entry aggregation only if another per-contract view needs the same counts; keep collection-wide selectors as the owner of `ContractCounts`.

**Staged remediation**

1. Remove the legacy summary with `ContractTab` if no migration is needed.
2. If per-contract counts are reused, add one typed `countChangelogKinds(entries)` helper returning total/add/remove/modify/breaking.
3. Keep collection-wide count calculation in store selectors; do not mix collection aggregation with per-contract entry reduction.

---

## Naming overlap that is not duplication

These names or concepts overlap, but their intent is materially different and they should not be consolidated merely because of naming.

### `ContractBadge` vs `ContractStatusChip`

- Sidebar navigation/coverage badge: `src/components/contract/ContractBadge.tsx:24-84`
- In-card status label: `src/components/contracts/ContractStatusChip.tsx:40-58`

`ContractBadge` is an interactive tree affordance that opens the contracts pane and summarizes one or more covering contracts. `ContractStatusChip` is a non-navigation status display for one contract. The legacy data dependency in `ContractBadge` should be modernized, but the component should not be replaced by `ContractStatusChip`.

### `ChangelogSummaryBar` vs `ContractsSummaryRow`

- Per-contract changelog counts: `src/components/contract/ChangelogSummaryBar.tsx:7-52`
- Collection-wide contract health and 30-day changes: `src/components/contracts/ContractsSummaryRow.tsx:38-70`

They share a metric-card visual pattern but summarize different entities and answer different questions. A generic metric-card primitive could be considered for visual consistency, but the aggregation functions are not interchangeable.

### Contract empty states vs changelog empty state

- No contracts exist: `src/components/contract/ContractEmptyState.tsx:9-26`, `src/components/contracts/ContractsEmptyState.tsx:8-37`
- Current changelog filters have no matches: `src/components/contracts/ChangelogDrawer/ChangelogEmptyState.tsx:7-17`

The two contract empty states are duplicates (finding 4). `ChangelogEmptyState` is not: it is a recoverable filter-empty condition with a reset action.

### `ChangelogTable`/`ContractDiffPane` vs `ChangelogTimeline`

- Tabular field-level review: `src/components/contract/ChangelogTable.tsx:16-94`, `src/components/contracts/ContractDiffPane.tsx:41-188`
- Grouped historical timeline: `src/components/contracts/ChangelogDrawer/ChangelogTimeline.tsx:48-109`

The two tables duplicate one another’s intent. The timeline is a deliberately different presentation supporting browsing and filtering; it should remain separate while sharing low-level primitives such as change chips and date formatters.

### `ContractCard` vs `ContractCardSkeleton`

- Content component: `src/components/contracts/ContractCard.tsx:184-638`
- Loading placeholder: `src/components/contracts/ContractCardSkeleton.tsx:3-31`

The skeleton mirrors card geometry by design but performs a separate loading-state role. Structural similarity is intentional, not semantic duplication.

### `ConsumerTree.tsx`, `ContractParties`, `PartyPill`, and `PartyAvatar`

- Party relationship layout: `src/components/contracts/ConsumerTree.tsx:32-124`
- Party pill: `src/components/contracts/PartyPill.tsx:14-37`
- Avatar primitive: `src/components/contracts/PartyAvatar.tsx:9-20`

These are composition layers, not duplicate implementations: relationship layout uses pills, and pills use avatars. The filename `ConsumerTree.tsx` does not match its exported `ContractParties` name, but that is a naming/organization issue rather than semantic duplication.

### Multiple export entry points

- Header export button: `src/components/contracts/ContractsTab.tsx:331-355`
- Context-menu export: `src/components/contracts/ContractContextMenu.tsx:102-113`
- Multi-contract selection dialog: `src/components/contracts/ExportContractDialog.tsx:29-88`

These are distinct UI entry points and all delegate to `saveContractAsOpenApi`; the core export behavior is already consolidated. Do not merge the UI solely because each initiates export.

### Context menu vs dropdown menu wrappers

- Shared item renderer: `src/components/contracts/ContractContextMenu.tsx:54-175`
- Context wrapper: `src/components/contracts/ContractContextMenu.tsx:215-249`
- Dropdown wrapper: `src/components/contracts/ContractContextMenu.tsx:251-285`

The wrappers target different Radix primitives and interaction modes. Their menu item semantics are already correctly centralized through `renderItems`.

---

## Recommended overall remediation sequence

1. **Protect the active flow.** Add focused coverage for modern list/create/edit/diff behavior, keyboard group traversal, version bumping, and party normalization.
2. **Extract risky shared pure logic.** Consolidate version increment, party normalization, date formatting, and optionally relative-time formatting before deleting legacy code.
3. **Modernize the surviving sidebar badge.** Move `ContractBadge` to the modern store/model while preserving collection/folder/request coverage behavior and its existing interaction tests.
4. **Port only approved legacy capabilities.** Attachments, live preview, and collection-change-driven refresh should be explicit product decisions, not reasons to keep an entire second stack.
5. **Perform mechanical modern-folder cleanup.** Replace `KindChip`, collapse group rendering, and extract the repeated more-actions trigger.
6. **Retire the singular-folder feature stack.** Remove the unused legacy tab, form, card, empty state, preview, top bar, and changelog views after imports and requirements are resolved.
7. **Run focused and broad validation.** Run contract component tests, `yarn tsc --noEmit`, and `yarn check`; then verify the collection tree badge and pane routing manually or with integration coverage.

## Audit limitations

- This report does not propose deleting stores, hooks, domain models, or IPC functions outside the two requested folders; those require a follow-up repository-wide dependency audit.
- Runtime feature flags or string-based dynamic imports could theoretically evade static call-site search, but no such mechanism was observed in the inspected pane routing.
- No production code was changed as part of this audit.
