# Semantic Duplication Audit: Layout, Illustrations, and Panes

Date: 2026-09-19

## Scope

Audited complete implementations under:

- `src/components/illustrations`
- `src/components/layout`
- `src/components/panes`

The existing `docs/reports/component-duplication/function-catalog.json` was used only as an index. Candidate implementations, nearby call sites, existing utilities, and the two in-scope tests were inspected directly. Relevant out-of-scope implementations were checked when they were plausible survivors, especially `src/lib/pane-utils.ts` and `src/components/collections/RequestNode.tsx`.

## Executive summary

The strongest consolidation opportunities are not the illustration components. They are repeated pane-tree operations and repeated picker construction:

1. **Use the existing pane-tree search utilities instead of local recursive copies.**
2. **Add one shared leaf-ID traversal and remove the two exact copies.**
3. **Factor `BreadcrumbBar`'s six collection pickers and four repeated folder/request operations.**
4. **Parameterize the two “move tab to new split” handlers.**
5. **Consolidate the two halves of `EnvironmentSwitcher` only behind a small, explicit scope configuration; do not erase their error and selection differences.**

The illustration files are a deliberately separate asset family. Although many share a visual grammar and `RocketConstruction`/`RocketSleep` contain an exact rocket-body fragment, turning those paths into a generic configurable rocket component would increase coupling and make asset editing harder. Keep the assets separate unless the product explicitly adopts a shared illustration design system.

## Findings overview

| ID | Confidence | Priority | Duplicate intent | Recommendation / survivor |
|---|---:|---:|---|---|
| F1 | High | P0 | Pane lookup is reimplemented in `GitToolbarButton` despite existing tested utilities | Use `findTabInTree` (and `findLeaf` where independently needed) from `src/lib/pane-utils.ts` |
| F2 | High | P0 | `collectLeafGroupIds` is copied exactly in two components | Add/export `collectLeafGroupIds` in `src/lib/pane-utils.ts`; delete both local copies |
| F3 | High | P1 | `BreadcrumbBar` builds the same collection picker six times | Keep `deriveSegments` as orchestrator; introduce a local `collectionPicker(activeName, nav)` factory |
| F4 | High | P1 | `BreadcrumbBar` repeats folder traversal, request listing, request lookup, and tab construction | Extract local data helpers and one request-opening helper; preserve `deriveSegments` as survivor |
| F5 | High | P1 | Horizontal and vertical “move to new split” handlers are identical except direction | Add a local `moveToNewSplit(direction, tabId)` handler in `TabBar` |
| F6 | Medium-high | P2 | Collection/global environment creation and rendering duplicate one another | Extract a scope-neutral local section component/hook configured by collection/global differences |
| F7 | Medium | P2 | Workspace-section presentation metadata is duplicated and already disagrees for Audit | Create one pane-presentation metadata map used by `BreadcrumbBar` and `TabItem` |
| F8 | Medium-low | P3 | Collection lists are fetched and locally managed in multiple layout components | Introduce a query-backed collection-summary source only after refresh semantics are specified |

## Detailed findings

### F1 — Local pane lookup duplicates existing utilities

**Confidence:** High  
**Priority:** P0

#### Exact references

- `src/components/layout/GitToolbarButton.tsx:9-15` — local `findTabGroupId`
- `src/components/layout/GitToolbarButton.tsx:17-21` — local `findLeaf`
- `src/components/layout/GitToolbarButton.tsx:41-48` — performs a tab lookup, then a second traversal to recover the leaf and tab
- `src/lib/pane-utils.ts:153-159` — existing exported `findLeaf`
- `src/lib/pane-utils.ts:161-168` — existing exported `findTabInTree`
- `src/lib/__tests__/pane-utils.test.ts:39-56` — direct coverage for `findLeaf`; the pane utility suite is the established home for these traversals

#### Same intent

All implementations recursively search the binary `PaneNode` tree. `findTabGroupId` searches for a tab and returns its leaf ID; `findTabInTree` searches for the same tab and returns both the leaf and tab. The local `findLeaf` is semantically and structurally the same as the exported utility.

#### Differences

- Local `findTabGroupId` returns only `groupId`.
- Shared `findTabInTree` returns `{ leaf, tab }`, which is strictly more useful to `openGitPanel` and avoids its second traversal.
- The local code casts the recovered tab to `GitTab`; the shared result still needs a tab-kind check or a narrow cast because the ID convention, not the type system, establishes that it is a git tab.

#### Recommendation / survivor

Use `findTabInTree` from `src/lib/pane-utils.ts` as the survivor for the `openGitPanel` path. It can replace both local helpers in that component:

1. Find the existing tab once.
2. Inspect `result.tab` and `result.leaf.groupId`.
3. Close the stale pathless git tab when appropriate.

Retain the exported `findLeaf` utility for callers that genuinely search by group ID, but remove the local copy.

#### Why actionable

This is both duplicate code and duplicate traversal. Keeping local variants makes pane-tree behavior easier to drift when the node model changes.

---

### F2 — Exact copies of `collectLeafGroupIds`

**Confidence:** High  
**Priority:** P0

#### Exact references

- `src/components/panes/TabBar.tsx:35-39` — local `collectLeafGroupIds`
- `src/components/panes/TabBar.tsx:58-59` — computes move targets
- `src/components/panes/TabBar.tsx:212-228` — computes the leaf added by splitting
- `src/components/collections/RequestNode.tsx:50-54` — exact copy
- `src/components/collections/RequestNode.tsx:168-179` — computes the new split and other target panes
- `src/lib/pane-utils.ts:153-179` — existing cluster of pane-tree traversal helpers; best home for the missing traversal

#### Same intent

The two functions are textually identical and collect every leaf `groupId` depth-first.

#### Differences

None in implementation or return ordering. Their consumers both depend only on membership/difference, not on separate domain semantics.

#### Recommendation / survivor

There is no current single survivor. Add `collectLeafGroupIds` beside `findLeaf`, `findTabInTree`, and `findFirstLeaf` in `src/lib/pane-utils.ts`, with focused tests in `src/lib/__tests__/pane-utils.test.ts`. Import it from both call sites and remove both local definitions.

#### Why actionable

This is an exact duplicate in a domain that already has a canonical utility module and tests.

---

### F3 — Six copies of the collection picker in `BreadcrumbBar`

**Confidence:** High  
**Priority:** P1

#### Exact references

Within `src/components/panes/BreadcrumbBar.tsx`:

- `142-152` — request-tab collection segment
- `286-296` — collection-tab collection segment
- `354-364` — git-tab collection segment
- `375-385` — diff-tab collection segment
- `397-407` — conflict-tab collection segment
- `418-428` — contract-tab collection segment

Each block calls `listCollections()`, maps `{ name }` to `{ id, label, isActive }`, and calls `nav.switchCollection(item.id)` on selection.

#### Same intent

Every block exposes the same collection selector. The only varying value is the active collection name (`collection` or `tab.collectionName`).

#### Differences

- The source of the active name differs by tab type.
- Diff/conflict tabs derive the displayed name with `collectionBasename`; the picker still behaves identically after derivation.
- No block adds an icon or distinct selection behavior.

#### Recommendation / survivor

Keep `deriveSegments` as the orchestration survivor, but define one local factory such as `createCollectionPicker(activeCollection, nav)` or `collectionSegment(activeCollection, nav)`. The factory should own loading/mapping and selection, while each tab branch remains responsible for deriving its active collection label.

Do not move this to a global utility until a second component needs the exact `Picker` abstraction; today the duplication is entirely local to `BreadcrumbBar`.

#### Why actionable

Six copies make error handling, caching, sorting, and future picker fields likely to diverge. A local factory removes repetition without introducing a cross-component abstraction.

---

### F4 — Repeated folder resolution and request opening in `BreadcrumbBar`

**Confidence:** High  
**Priority:** P1

#### Exact references

Within `src/components/panes/BreadcrumbBar.tsx`:

- `163-185` and `226-248` — load a collection, walk `parentPath`, filter requests, and map picker items
- `186-211` and `249-274` — load the same collection, walk the same path, find a request by UID, construct the same request tab, and open it
- Repeated traversal core: `166-172`, `189-194`, `229-235`, `252-257`
- Repeated tab construction: `197-210`, `260-273`

#### Same intent

The folder breadcrumb pickers and final request breadcrumb picker both need “requests in this collection folder” and “open selected request from this folder.” The implementations are repeated four times inside one function.

#### Differences

- Folder-segment item loading marks every item inactive; the final request segment marks the current request active.
- Each folder segment captures a different `parentPath` based on its position.
- The final segment's `requestName` is available for active-item comparison.

#### Recommendation / survivor

Keep `deriveSegments`, but extract narrowly scoped helpers:

1. `resolveFolderItems(collection, parentPath)` — loads the collection and returns the resolved folder's items (or only request items).
2. `toRequestPickerItems(requests, activeFileName?)` — maps method badge, ID, label, and active state.
3. `openRequestPickerItem(collection, parentPath, itemId, nav)` — resolves the request and constructs/opens the tab.

If avoiding repeated backend reads during one popover interaction is desired, a later step can introduce a cached loader. Do not combine that behavioral change with the first deduplication patch.

#### Why actionable

The repeated path walk has subtle failure behavior (`break` leaves `items` at the deepest valid ancestor). Centralizing it prevents one picker from later handling missing folders differently from another.

---

### F5 — Duplicate “move to new split” event handlers

**Confidence:** High  
**Priority:** P1

#### Exact references

- `src/components/panes/TabBar.tsx:212-220` — move to a new horizontal/right split
- `src/components/panes/TabBar.tsx:223-231` — move to a new vertical/below split

#### Same intent

Both handlers snapshot leaf IDs, split the current group, identify the new leaf by set difference, and move the tab into it.

#### Differences

Only `direction` and menu label/icon differ.

#### Recommendation / survivor

Introduce a local handler such as `moveToNewSplit(tabId, direction)` and keep the two menu items as thin calls. This handler should use the shared `collectLeafGroupIds` from F2.

A stronger future API would make `splitGroup` return the created group ID, eliminating before/after tree scans. That is a store API change and should not be bundled into the initial deduplication.

---

### F6 — Collection/global halves of `EnvironmentSwitcher` repeat state and UI flow

**Confidence:** Medium-high  
**Priority:** P2

#### Exact references

Within `src/components/layout/EnvironmentSwitcher.tsx`:

- Parallel state and in-flight guards: `34-41`
- Parallel create handlers: `43-61` and `63-88`
- Parallel empty states: `158-188` and `279-307`
- Parallel radio lists: `190-230` and `308-348`
- Parallel inline create inputs: `231-249` and `349-367`
- Parallel configure/add footers: `251-274` and `369-389`

#### Same intent

Both tabs select zero or one environment, offer an inline create flow, show an empty-state CTA, and expose configure/add actions.

#### Important differences

- Collection creation requires `activeCollection`, selects the newly created environment, and shows user-facing toasts.
- Global creation logs failures but does not select the new environment.
- Collection selection updates Zustand synchronously; global selection invokes a mutation.
- Configure navigation differs: collection opens `EnvironmentDialog`; global opens workspace tabs and activates the environments tab.
- Labels and empty-state copy intentionally differ.

#### Recommendation / survivor

Do **not** merge the two persistence handlers into one generic callback with branching internals. Instead:

1. Extract a local `EnvironmentScopeSection` presentation component driven by explicit props: items, selected name, empty copy, `onSelect`, `onCreate`, `onConfigure`, and pending/creating state.
2. Optionally extract a small `useInlineCreate` hook for name state, in-flight protection, Escape/reset, and blur/Enter submission.
3. Keep `handleCreateCollection`, `handleCreateGlobal`, and configure handlers as distinct adapters so scope-specific behavior remains visible.

There is no single existing survivor; the current collection/global handlers should remain the behavioral adapters.

#### Why not higher confidence

The repeated shape is clear, but an over-generalized abstraction could hide meaningful policy differences. Consolidate presentation and interaction mechanics, not domain behavior.

---

### F7 — Workspace section metadata is duplicated and already inconsistent

**Confidence:** Medium  
**Priority:** P2

#### Exact references

- `src/components/panes/BreadcrumbBar.tsx:65-75` — section-to-label mapping
- `src/components/panes/BreadcrumbBar.tsx:104-115` — section-to-icon mapping
- `src/components/panes/BreadcrumbBar.tsx:314-345` — workspace section picker
- `src/components/panes/TabItem.tsx:72-92` — independent section-to-icon rendering
- `src/components/panes/EditorGroup.tsx:201-210` — section-to-content dispatch (related exhaustive branch, but not a presentation duplicate)

#### Same intent

`BreadcrumbBar` and `TabItem` both choose an icon for a `WorkspaceTabSection`.

#### Differences

- Icon size/classes differ by context and should remain caller-controlled.
- The Audit icon already differs: `BreadcrumbBar` uses `List`; `TabItem` uses `ShieldCheck`.
- `BreadcrumbBar` also owns user-facing labels; `TabItem` does not display those labels.
- `EditorGroup` maps sections to content components, which is routing rather than presentation metadata and should remain separate.

#### Recommendation / survivor

Create a small pane-presentation metadata module or exported map containing the canonical label and icon component type for each `WorkspaceTabSection`. Callers supply their own classes/ARIA props. Choose the intended Audit icon during remediation; the current code does not establish a consistent survivor.

Do not fold `EditorGroup`'s content dispatch into that metadata unless lazy loading and component prop requirements can remain type-safe.

---

### F8 — Repeated collection-summary loading across layout components

**Confidence:** Medium-low  
**Priority:** P3

#### Exact references

- `src/components/layout/CollectionDropdown.tsx:13-37` — local summaries state and `fetchCollections`; refreshes when opened
- `src/components/layout/CollectionsSidebar.tsx:49-82` — same local summaries state and fetch wrapper
- `src/components/layout/CollectionsSidebar.tsx:243-328` — extensive event/debounce invalidation
- `src/components/layout/GitToolbarButton.tsx:28-36` — one-off list lookup when the git path is missing
- `src/components/panes/BreadcrumbBar.tsx:142-149`, `286-293`, `354-361`, `375-382`, `397-404`, `418-425` — on-demand picker loading

#### Same intent

All paths retrieve collection summaries, and the dropdown/sidebar duplicate the basic state + try/catch fetch wrapper.

#### Differences

Refresh requirements are materially different:

- Sidebar must stay live across file-watch, workspace, git, import, delete, and environment events.
- Dropdown deliberately refreshes only on open.
- Breadcrumb pickers load on demand.
- Git toolbar needs a fallback lookup only when store state lacks a path.

#### Recommendation / survivor

Do not simply share `fetchCollections` or lift sidebar state into a component context. If collection summaries become a query-backed resource, make the sidebar's event listeners invalidate that query and let dropdown/pickers read/refetch under explicit stale-time policies. Until those semantics are designed, keep these call sites separate.

This is a staged architectural opportunity, not a safe mechanical deduplication.

## Deliberate separation / non-actionable repetition

### Illustration assets should remain separate

All exported illustration components have distinct product intent, composition, and usage guidance. Shared conventions include the `Props` shape, decorative SVG accessibility attributes, a common `200 × 200` canvas for most assets, background glows, stars, and variations of a rocket body. Those conventions are an asset style language, not evidence that the components have duplicate intent.

Notable overlap:

- `src/components/illustrations/RocketConstruction.tsx:20-36`
- `src/components/illustrations/RocketSleep.tsx:25-50`

These contain the same main body paths, fins, and nozzle. Differences include transform, window treatment, background scene, and state-specific details. Other assets use scaled or redrawn bodies rather than the exact fragment, for example:

- `RocketLock.tsx:20-36`
- `RocketPlug.tsx:20-36`
- `RocketClock.tsx:65-82`
- `RocketFolder.tsx:47-76`
- `RocketSpeed.tsx:76-104`
- `RocketTelescope.tsx:76-93`
- `RocketOrbit.tsx:38-59`
- `RocketStar.tsx:42-54`

**Recommendation:** keep every asset as a standalone component. Do not create a highly parameterized `RocketBody` merely to share SVG paths. Consider a primitive only if designers require synchronized rocket anatomy across the entire asset family and are willing to accept coupled visual changes.

The repeated root `<svg>` attributes are also not worth a wrapper: `RocketLiftOff` and `RocketMinimal` use different dimensions/view boxes, and a wrapper would obscure normal SVG authoring for little maintenance gain.

### The legacy illustration alias is a thin compatibility export

- `src/components/illustrations/index.ts:10-11` exports `RocketLaunch` as `RocketIllustration`.

This is not a duplicate component. Keep it while consumers may rely on the old name; remove it only through an explicit deprecation/search-and-migrate task. Current source search found no in-repository consumer, so removal could be considered separately if this package has no external consumers.

### `WorkspaceToolbar` is intentional composition

- `src/components/layout/WorkspaceToolbar.tsx:7-24`

This is a thin layout wrapper, not a duplicate of its children. It provides stable toolbar grouping and should remain.

### Sandbox and request-guard popovers share a shell, not behavior

- `src/components/layout/SandboxPopover.tsx:11-34`
- `src/components/layout/WorkspaceSecurityPopover.tsx:17-37`

Both use the same toolbar icon-button and popover-header style, but their state, controls, widths, and warning behavior differ. A generic “toolbar settings popover” would currently save only a few class strings and add indirection. Prefer design-system tokens or a primitive only after a third matching popover appears.

### `EditorGroup` empty-state variants are intentionally local

- `src/components/panes/EditorGroup.tsx:49-128`

The three variants share centering/gradient classes but represent different focus states and have different interaction semantics. Keeping them together in one local component makes those distinctions clear. A generic empty-state component is not justified by this file alone.

### `PaneRenderer` recursion is domain behavior, not duplicate traversal

- `src/components/panes/PaneRenderer.tsx:7-32`

It recursively renders the tree rather than searching or collecting it. It should not be forced through the search helpers from F1/F2.

### `ConsolePanel` row repetition is useful specialization

- `src/components/layout/ConsolePanel.tsx:34-60` — HTTP detail
- `src/components/layout/ConsolePanel.tsx:74-94` — script row
- `src/components/layout/ConsolePanel.tsx:96-120` — test row
- `src/components/layout/ConsolePanel.tsx:215-251` — HTTP row

The row types share timestamp/request-name layout, but their status, content, expansion, and semantics differ. A polymorphic row abstraction would likely be harder to read than the current discriminated rendering. The shared `formatTime` helper is already local and reused by all row variants.

### Collection deletion's recursive close is not a search-helper duplicate

- `src/components/layout/CollectionsSidebar.tsx:97-118`

This recursively visits **all** leaves and conditionally closes multiple tabs. `findLeaf`/`findTabInTree` return one match and are not suitable survivors. If a generic pane-tree visitor is introduced later for several mutation operations, this callback could adopt it, but there is no current duplicate with the same behavior.

## Staged remediation plan

### Stage 1 — Mechanical, low-risk pane utility consolidation

1. Add and test `collectLeafGroupIds` in `src/lib/pane-utils.ts`.
2. Replace local copies in `TabBar` and `RequestNode`.
3. Replace `GitToolbarButton`'s two local traversals with `findTabInTree`.
4. Run focused pane utility, tab bar, and relevant component tests, then `yarn tsc --noEmit`.

**Expected risk:** Low. Behavior should remain unchanged.

### Stage 2 — Local deduplication inside pane components

1. Add the collection-picker factory inside `BreadcrumbBar.tsx`.
2. Extract folder resolution, request-item mapping, and selected-request opening helpers.
3. Parameterize `TabBar`'s move-to-new-split handler.
4. Add focused `BreadcrumbBar` tests for request, collection, git/diff/conflict, and contract picker behavior before or with the refactor.

**Expected risk:** Low to medium. The primary risk is changing folder-path fallback or active-item behavior.

### Stage 3 — Presentation metadata alignment

1. Decide the canonical Audit icon.
2. Introduce typed workspace-section label/icon metadata.
3. Migrate `BreadcrumbBar` and `TabItem`; leave `EditorGroup` content routing separate.
4. Add an exhaustive type-level or unit test covering every `WorkspaceTabSection`.

**Expected risk:** Low, with a deliberate visible icon decision.

### Stage 4 — Environment switcher decomposition

1. Add tests covering collection/global selection, creation, blur/Enter/Escape behavior, configure navigation, and error handling.
2. Extract shared presentation and inline-create mechanics.
3. Keep scope-specific mutation/error/configure adapters explicit.

**Expected risk:** Medium. Blur and Enter can race without the current in-flight guards, and the two scopes intentionally differ after creation.

### Stage 5 — Optional collection-summary query architecture

1. Specify freshness and invalidation behavior for sidebar, dropdown, breadcrumb, imports, git changes, workspace changes, and file-watch events.
2. Introduce a query key/hook for collection summaries.
3. Convert event listeners to invalidation rather than component-local refresh where appropriate.
4. Migrate consumers incrementally and test event-driven refresh behavior.

**Expected risk:** Medium to high. Do not combine this with Stages 1-4.

## Suggested validation for future remediation

- `yarn test src/lib/__tests__/pane-utils.test.ts`
- `yarn test src/components/panes/__tests__/TabBar.test.tsx`
- Add and run focused tests for `BreadcrumbBar` and `EnvironmentSwitcher`
- `yarn tsc --noEmit`
- `yarn check`

## Final assessment

The audit found **five high-confidence actionable duplicate groups**, **two medium-confidence presentation/state groups**, and **one lower-confidence architectural repetition**. The highest-value changes are small and centered on pane-tree utilities and `BreadcrumbBar`. The illustration directory should be treated as an intentionally varied asset library, not as a refactoring target.