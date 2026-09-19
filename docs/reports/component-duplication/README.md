# Component duplication audit — management index

**Audit date:** 2026-09-19
**Scope:** all production code under `src/components` — 18 folders plus `ErrorBoundary.tsx` and `SplashScreen.tsx`. The discovery catalog contains **394 callable/component entries**; this is an index count, not a claim that only cataloged functions were reviewed.

## Executive summary

The audit identifies **49 deduplicated actionable work packages**: **41 `CONSOLIDATE`** and **8 `INVESTIGATE`**. The first work should protect persisted request, collection, environment, and contract behavior. Several findings show plausible data-loss or spec-correctness risk, but the audit does **not** establish that every divergence is a user-visible bug; characterization tests and product/spec decisions are required before behavior changes.

Most important risks:

1. Manual save, initial save, and auto-save do not use one persistence-specific auth/header mapping. AWS SigV4 auth and disabled headers may be lost in some paths.
2. The unused `CollectionSettingsDialog` can construct an incomplete replacement snapshot if it is reintroduced.
3. Initial request save has materially less field coverage than normal save.
4. Environment duplicate-key handling disagrees across editing, diagnostics, and persistence.
5. Two contract feature stacks use different models and normalization rules; retirement requires an explicit capability and persisted-ID compatibility check.

## Counting method and totals

A **work package** is one independently manageable remediation intent, not one function, file, or report heading. Counts below:

- merge cross-folder supersets with their batch findings (for example, relative time is counted once under X3; resize once under X6);
- collapse nested legacy-stack findings into one ownership/migration package where separate refactors would be throwaway work (contract findings 1–5);
- keep distinct leaf adapters separate when they have different persistence contracts (auth, headers, and full request payload);
- exclude `KEEP SEPARATE` patterns from actionable totals;
- assign a normalized P0–P3 priority where a batch used words rather than numbers: P0 data/spec risk, P1 high-value correctness/core behavior, P2 bounded consistency/maintenance, P3 local or optional cleanup.

Confidence measures confidence in **shared intent**, not certainty that consolidation is safe or that a bug exists.

### By disposition and confidence

| Disposition | High | Medium-high | Medium | Medium-low | Total |
|---|---:|---:|---:|---:|---:|
| `CONSOLIDATE` | 37 | 2 | 2 | 0 | **41** |
| `INVESTIGATE` | 2 | 3 | 2 | 1 | **8** |
| **Actionable total** | **39** | **5** | **4** | **1** | **49** |

### By normalized priority

| Priority | Meaning | Count |
|---|---|---:|
| P0 | Protect persisted data/spec behavior or settle active feature ownership first | **8** |
| P1 | High-value correctness/core behavior | **19** |
| P2 | Bounded consistency, UX, or maintenance value | **14** |
| P3 | Local, optional, or lower-value cleanup | **8** |
| **Total** |  | **49** |

## Methodology and limitations

- The six batch reports reviewed every production implementation in their assigned component scope; the cross-folder review then reconciled overlaps and checked new cross-batch candidates against current source.
- `function-catalog.json` and `categorized-functions.json` were discovery/scope indexes only. They were not treated as proof of duplication.
- Generic React structure, similar styling, same names, and test-fixture repetition were excluded unless they encoded the same behavior and maintenance risk.
- Tests were used as evidence about intended behavior and migration safety; test-only duplication was not audited as production duplication.
- Static call-site searches can miss runtime feature flags or string-based dynamic imports. No such mechanism was observed for the cited legacy surfaces, but final deletion work still needs a fresh reference search.
- The catalog was stale for recently changed environment files during the audit. Its **394** count is therefore a scope indicator, not an exact current-source inventory.
- Line references describe the audited snapshot and may move. Stable links below target report headings, which contain the detailed references and nuances.
- No finding is a guaranteed bug report. Terms such as “may drop,” “can diverge,” and “risk” intentionally preserve uncertainty until tests or reproduction confirm behavior.

## Folder coverage

Catalog counts are callable/component entries, not file counts. Every listed area was covered by a batch report; cross-folder behavior was additionally reviewed in `cross-folder-review.md`.

| Area | Catalog entries | Primary detailed report | Coverage outcome |
|---|---:|---|---|
| Root files (`ErrorBoundary.tsx`, `SplashScreen.tsx`) | 4 | [Remaining components](./batch-remaining.md#assigned-root-files-with-no-credible-duplication) | Reviewed; no credible duplicate intent |
| `audit/` | 8 | [Remaining components](./batch-remaining.md#folders-with-no-credible-duplication) | No within-folder finding; participates in cross-folder relative time |
| `collections/` | 27 | [Collections, Git, environments](./batch-collections-git.md#detailed-findings) | Persistence, traversal, creation, rename, and menu findings |
| `contract/` | 10 | [Contract stacks](./batch-contracts.md#findings) | Mostly legacy parallel stack; badge remains active |
| `contracts/` | 63 | [Contract stacks](./batch-contracts.md#findings) | Modern survivor plus local utility/presentation findings |
| `editor/` | 31 | [Editor/UI](./batch-editor-ui.md#detailed-findings) | Token grammar, snippet registry, source metadata, optional plugin plumbing |
| `environments/` | 3* | [Collections, Git, environments](./batch-collections-git.md#detailed-findings) | Duplicate-key and typed variable-editor findings; catalog known stale here |
| `git/` | 27 | [Collections, Git, environments](./batch-collections-git.md#detailed-findings) | Diff, operations, errors, dialogs; relative time merged cross-folder |
| `history/` | 3 | [Remaining components](./batch-remaining.md#folders-with-no-credible-duplication) | Reviewed; clock formatter intentionally distinct |
| `illustrations/` | 21 | [Layout/illustrations/panes](./batch-layout-illustrations.md#illustration-assets-should-remain-separate) | Deliberately separate asset family |
| `import/` | 1 | [Remaining components](./batch-remaining.md#r-2--import-source-picker-handlers-repeat-the-same-normalization-flow) | Host-path and picker orchestration findings |
| `layout/` | 18 | [Layout/illustrations/panes](./batch-layout-illustrations.md#detailed-findings) | Pane lookup, environment switcher, collection-summary architecture |
| `panes/` | 15 | [Layout/illustrations/panes](./batch-layout-illustrations.md#detailed-findings) | Pane traversal, breadcrumbs, split action, section metadata |
| `request/` | 62 | [Request](./batch-request.md#findings) | Persistence, legacy surfaces, OAuth, resize, and row/editor findings |
| `response/` | 14 | [Remaining components](./batch-remaining.md#r-4--clipboard-copy-with-transient-success-state-is-duplicated-in-the-response-ui) | Clipboard feedback finding |
| `status-bar/` | 1 | [Remaining components](./batch-remaining.md#r-3--workspace-resolution-is-repeated-across-four-components) | Workspace resolution finding |
| `title-bar/` | 3 | [Remaining components](./batch-remaining.md#r-3--workspace-resolution-is-repeated-across-four-components) | Workspace resolution finding |
| `ui/` | 78 | [Editor/UI](./batch-editor-ui.md#intentional-variants-and-non-findings) | No actionable wrapper duplication; existing resizable primitive affects X6 |
| `workspace/` | 5 | [Remaining components](./batch-remaining.md#r-1--workspace-child-path-construction-is-duplicated) | Host-path and workspace resolution findings |

\* Known undercount: the catalog did not include newly added/refactored environment implementations at audit time.

## Prioritized backlog

Suggested status values: `Not started`, `Characterizing`, `Ready`, `In progress`, `Blocked`, `Done`, `Rejected`. Owners are intentionally unassigned. Check the box only after focused validation and report-link acceptance criteria pass.

### `CONSOLIDATE`

| Done | ID | Pri. | Intent | Affected areas | Confidence | Risk/value | Recommended action | Status | Owner | Detail |
|---|---|---:|---|---|---|---|---|---|---|---|
| [ ] | CD-001 | P0 | Canonical persisted auth conversion | collections, request, auto-save | High | Possible AWS/OAuth/inherit drift or loss | Add bidirectional persistence adapters with explicit missing/empty policy; never reuse execution mapping | Not started | — | [X1](./cross-folder-review.md#x1--persisted-auth-conversion-is-duplicated-across-collection-defaults-manual-request-save-request-loading-and-auto-save) |
| [ ] | CD-002 | P0 | Canonical persisted header mapping | collections, request, auto-save | High | Disabled rows may be dropped; drafts may persist | Add `toPersistedHeaders`; preserve named disabled rows and keep execution filtering separate | Not started | — | [X2](./cross-folder-review.md#x2--persisted-header-row-serialization-is-repeated-and-auto-save-drops-disabled-headers) |
| [ ] | CD-003 | P0 | One request-tab save payload mapper | request | High | Initial save may omit auth/scripts/settings/docs/tags/assertions | Extract the broad normal-save mapper, allow name/file overrides, and use CD-001/CD-002 | Not started | — | [Request 1](./batch-request.md#1-request-tab-persistence-payload-construction-has-divergent-copies) |
| [ ] | CD-004 | P0 | Retire incomplete collection settings editor | collections | High | Dead surface can erase variables/docs/auth if restored | Add preservation coverage, verify no dynamic entry point, remove `CollectionSettingsDialog` | Not started | — | [Collections F1](./batch-collections-git.md#f1--competing-collection-settings-editors-can-persist-incompatible-snapshots) |
| [ ] | CD-005 | P0 | One environment duplicate-key policy | environments | High | Editor, resolution, and persisted YAML may disagree | Centralize key canonicalization/normalization; explicitly choose block versus documented last-wins UX | Not started | — | [Environments F2](./batch-collections-git.md#f2--environment-duplicate-key-policy-is-implemented-three-ways-and-disagrees-at-the-ui-boundary) |
| [ ] | CD-006 | P0 | Converge on modern contract feature stack | contract, contracts | High | Two models/flows can drift; deletion can lose approved capabilities | Keep `contracts/`; decide attachments/preview/watch needs, migrate badge, then remove legacy-only tab/form/card/empty/diff surfaces | Not started | — | [Contracts 1](./batch-contracts.md#1-parallel-top-level-contract-feature-stacks) |
| [ ] | CD-007 | P0 | Reuse canonical pane lookup | layout, pane utilities | High | Duplicate traversal can drift | Replace local Git toolbar searches with `findTabInTree`/`findLeaf` | Not started | — | [Layout F1](./batch-layout-illustrations.md#f1--local-pane-lookup-duplicates-existing-utilities) |
| [ ] | CD-008 | P0 | Canonical leaf-group ID traversal | panes, collections | High | Exact duplicate in a canonical utility domain | Add tested `collectLeafGroupIds` to `pane-utils` and remove both copies | Not started | — | [Layout F2](./batch-layout-illustrations.md#f2--exact-copies-of-collectleafgroupids) |
| [ ] | CD-009 | P1 | One variable-token scanner/grammar API | editor, text variables | High | Highlight/mask/hover/autocomplete grammar can diverge | Extend the existing parser with offsets and explicit strict/partial profiles; migrate consumers incrementally | Not started | — | [Editor F1](./batch-editor-ui.md#f1--variable-token-recognition-and-range-calculation-are-implemented-repeatedly) |
| [ ] | CD-010 | P1 | Canonical script API snippet registry | editor | High | Phase capability labels/code can drift | Define API items once and derive ordered phase lists without changing exports | Not started | — | [Editor F2](./batch-editor-ui.md#f2--script-api-reference-snippet-registries-repeat-the-same-phase-shared-entries) |
| [ ] | CD-011 | P1 | One load-test experience/state owner | request/load-test | High | Parallel execution/configuration paths and units | Keep tab/store, migrate useful dialog coverage and toolbar entry, then remove dialog | Not started | — | [Request 2](./batch-request.md#2-two-complete-load-test-experiences-are-exposed-for-the-same-request) |
| [ ] | CD-012 | P1 | One request-variable load/save lifecycle | request | High | Duplicate fetches and legacy save paths can drift | Keep panel, establish one committed-state owner, remove unreferenced dialog after search | Not started | — | [Request 3](./batch-request.md#3-request-variable-data-is-loaded-in-multiple-active-places-plus-an-unreferenced-legacy-dialog) |
| [ ] | CD-013 | P1 | Canonical contract party ID construction | contracts | High | Visible-equivalent names may persist different IDs | Define compatibility-tested normalization and one party builder; confirm Unicode policy | Not started | — | [Contracts 6](./batch-contracts.md#6-party-id-normalization-and-party-construction) |
| [ ] | CD-014 | P1 | One contract version increment rule | contracts | High | Displayed next version can differ from submitted value | Specify malformed/prerelease behavior and use one tested helper for preview and mutation | Not started | — | [Contracts 9](./batch-contracts.md#9-version-increment-logic-is-duplicated-and-behavior-has-diverged) |
| [ ] | CD-015 | P1 | Canonical collection-item walker | collections | High | Counts/lists can disagree on request/summary semantics | Add ancestry-preserving DFS visitor; retain explicit reducer predicates | Not started | — | [Collections F4](./batch-collections-git.md#f4--recursive-collection-item-traversal-is-independently-reimplemented-four-times) |
| [ ] | CD-016 | P1 | Shared inline-rename interaction mechanics | collections, environments | High | Double submit/failure-exit drift around filesystem operations | Extract draft/in-flight/key/blur behavior only; retain resource persistence adapters | Not started | — | [Collections F5](./batch-collections-git.md#f5--inline-rename-interaction-state-is-duplicated-across-four-resource-types) |
| [ ] | CD-017 | P1 | One create-save-open request transaction | collections | High | Root/folder paths or default payloads can drift | Extract transaction with explicit collection-relative path; keep ancestry derivation local | Not started | — | [Collections F6](./batch-collections-git.md#f6--root-and-folder-request-creation-duplicate-the-same-create-save-open-transaction) |
| [ ] | CD-018 | P1 | One action model per tree node | collections | High | Dropdown/context actions already differ in expansion/await behavior | Resolve behavior first, then render shared descriptors through primitive-specific adapters | Not started | — | [Collections F7](./batch-collections-git.md#f7--tree-node-dropdown-and-context-menus-duplicate-the-same-action-sets) |
| [ ] | CD-019 | P1 | Canonical Git diff adapters/loader | git | High | Added/deleted/empty classification can misrepresent YAML diffs | Extract status/normalization adapters and staged/working loader; keep caller error policy | Not started | — | [Git F9](./batch-collections-git.md#f9--git-diff-loading-and-normalization-are-repeated-across-three-components) |
| [ ] | CD-020 | P1 | Cross-platform host-path basename | import, workspace, panes, collections | High | Slash-only copies are Windows-unsafe | Add pure dual-separator helper with trailing-separator tests; do not apply to logical collection paths | Not started | — | [X4](./cross-folder-review.md#x4--host-path-basename-extraction-is-repeated-and-several-copies-are-windows-unsafe) |
| [ ] | CD-021 | P1 | Cross-platform host-path append helper | import, workspace | High | Workspace child paths may be malformed | Add tested `appendHostPathSegment`; keep name derivation and workspace creation local | Not started | — | [Remaining R1](./batch-remaining.md#r-1--workspace-child-path-construction-is-duplicated) |
| [ ] | CD-022 | P1 | Parameterize import source selection | import | High | Picker behavior/name extraction can drift | Share primary picker/cancellation/metadata flow; keep environment JSON domain distinction | Not started | — | [Remaining R2](./batch-remaining.md#r-2--import-source-picker-handlers-repeat-the-same-normalization-flow) |
| [ ] | CD-023 | P1 | One local breadcrumb collection picker | panes | High | Six copies can diverge in loading/sorting/errors | Add a local picker factory; retain tab-specific active-name derivation | Not started | — | [Layout F3](./batch-layout-illustrations.md#f3--six-copies-of-the-collection-picker-in-breadcrumbbar) |
| [ ] | CD-024 | P1 | One breadcrumb folder/request open path | panes | High | Missing-folder fallback and tab construction can drift | Extract narrow folder resolution, item mapping, and open helpers | Not started | — | [Layout F4](./batch-layout-illustrations.md#f4--repeated-folder-resolution-and-request-opening-in-breadcrumbbar) |
| [ ] | CD-025 | P1 | Parameterize move-to-new-split | panes | High | Exact handler duplication | Use one direction-parameterized local handler backed by CD-008 | Not started | — | [Layout F5](./batch-layout-illustrations.md#f5--duplicate-move-to-new-split-event-handlers) |
| [ ] | CD-026 | P1 | Data-driven contract group rendering | contracts | High | Repeated index math can break keyboard order | Render ordered descriptors with deterministic cumulative indexes | Not started | — | [Contracts 12](./batch-contracts.md#12-contract-group-rendering-repeats-the-same-card-list-algorithm-four-times) |
| [ ] | CD-027 | P2 | One OAuth token-response application path | request/oauth2 | High | Acquire/refresh patches and claim decoding can drift | Extract response normalization and best-effort decoding; preserve operation-specific side effects | Not started | — | [Request 5](./batch-request.md#5-oauth-token-acquisition-and-refresh-duplicate-token-response-application) |
| [ ] | CD-028 | P2 | Shared OAuth token disclosure/claims primitives | request/oauth2 | High | Repeated display logic and state can drift | Extract focused claims and disclosure primitives; keep access/ID differences in orchestrator | Not started | — | [Request 6](./batch-request.md#6-oauth-access-token-and-id-token-displays-substantially-repeat-disclosure-and-claims-ui) |
| [ ] | CD-029 | P2 | One option-driven relative-time utility | audit, contracts, git | High | Future/invalid/old-date labels disagree | Add fixed-clock modes; migrate X3 as one package and preserve caller fallback copy | Not started | — | [X3](./cross-folder-review.md#x3--relative-time-formatting-spans-audit-contracts-and-git-and-should-be-one-option-driven-utility) |
| [ ] | CD-030 | P2 | Timer-safe keyed clipboard feedback | response, request/oauth2 | High | False success and timer races | Add rejection-aware keyed hook with one timer and cleanup; preserve caller payload/UX | Not started | — | [X5](./cross-folder-review.md#x5--keyed-clipboard-success-feedback-is-duplicated-between-response-and-oauth-surfaces) |
| [ ] | CD-031 | P2 | Query-layer workspace resolution | status-bar, title-bar, workspace | High | Source-of-truth/fallback and fetch behavior can drift | Specify explicit-ID semantics, then use/adapt canonical query hooks without extra fetch flicker | Not started | — | [Remaining R3](./batch-remaining.md#r-3--workspace-resolution-is-repeated-across-four-components) |
| [ ] | CD-032 | P2 | Canonical variable-source presentation metadata | editor, URL variables | High | Request/runtime badges already conflict | Add exhaustive badge/default-label metadata; keep renderer CSS and contextual labels local | Not started | — | [Editor F3](./batch-editor-ui.md#f3--variable-source-badgelabel-presentation-is-derived-in-multiple-incompatible-ways) |
| [ ] | CD-033 | P2 | One contract month/day formatter | contracts | High | Three exact date-only formatters | Extract tested locale-preserving helper; keep separate from relative time | Not started | — | [Contracts 7](./batch-contracts.md#7-monthday-date-formatter-implemented-three-times) |
| [ ] | CD-034 | P2 | Reuse shared contract change chip | contracts | High | Duplicate labels/colors can drift | Replace local `KindChip` with `ChangeChip`; add variant only if intentional | Not started | — | [Contracts 8](./batch-contracts.md#8-change-kind-chip-duplicated-inside-the-changelog-drawer) |
| [ ] | CD-035 | P2 | Shared presentational Git error banner | git | High | Error treatment/dismiss wiring drifts | Extract exact conflict banner first, then migrate compatible placements without owning store error policy | Not started | — | [Git F11](./batch-collections-git.md#f11--dismissible-git-error-banners-are-copied-across-modes-and-components) |
| [ ] | CD-036 | P2 | Narrow Git operation runners and typed outcomes | git | Medium-high | Repeated identical error can be mistaken for success | Deduplicate local wrappers, then return typed store results before any broad hook | Not started | — | [Git F10](./batch-collections-git.md#f10--git-operation-handlers-repeat-store-error-inference-and-busy-state-orchestration) |
| [ ] | CD-037 | P2 | Canonical workspace-section label/icon metadata | panes | Medium | Audit icon already disagrees | Choose intended icon; add exhaustive presentation map while keeping routing separate | Not started | — | [Layout F7](./batch-layout-illustrations.md#f7--workspace-section-metadata-is-duplicated-and-already-inconsistent) |
| [ ] | CD-038 | P3 | Collapse identical form body render branches | request | High | Low-risk local duplication | Combine `formdata`/`formurlencoded` render condition; keep body modes distinct | Not started | — | [Request 9](./batch-request.md#9-form-data-and-form-urlencoded-body-modes-render-the-exact-same-editor-branch) |
| [ ] | CD-039 | P3 | One contract more-actions trigger | contracts | High | Repeated identical local markup | Extract local trigger without changing status conditions | Not started | — | [Contracts 13](./batch-contracts.md#13-more-actions-dropdown-trigger-is-repeated-across-card-status-branches) |
| [ ] | CD-040 | P3 | Render global Git dialogs once | git | Medium | Branch wiring can drift | Put shared dialogs after branch-specific content; retain repo-only remotes condition | Not started | — | [Git F12](./batch-collections-git.md#f12--gitpanel-duplicates-global-dialog-rendering-across-repository-branches) |
| [ ] | CD-041 | P2 | Generalize deterministic name avatar primitive | contracts | Medium-high | Low-risk duplicated derivation/markup | Add generic name/seed/size/fallback primitive; keep `PartyAvatar` adapter | Not started | — | [Contracts 11](./batch-contracts.md#11-author-avatar-rendering-duplicates-the-party-avatar-primitive) |

### `INVESTIGATE` before consolidating

These items have credible shared intent, but the abstraction boundary or required behavior is not yet sufficiently settled. “Investigate” should end in either a tested consolidation plan or an explicit rejection recorded here.

| Done | ID | Pri. | Question / intent | Affected areas | Confidence | Risk/value | Recommended next action | Status | Owner | Detail |
|---|---|---:|---|---|---|---|---|---|---|---|
| [ ] | INV-001 | P2 | Can existing resizable primitives replace manual request splits? | request, ui, panes | Medium-high | Avoid a second framework; accessibility/persistence risk | Characterize behavior and prototype `RequestPanel`; add a narrow fallback hook only for unsupported pixel cases | Not started | — | [X6](./cross-folder-review.md#x6--request-resize-mechanics-duplicate-an-existing-shared-resizable-panel-primitive) |
| [ ] | INV-002 | P1 | How much variable-table presentation can safely be shared? | collections, environments | High | Untyped merge could lose `initialValue` or secret semantics | Add model-preservation tests; extract only typed stateless row primitives if boundaries remain explicit | Not started | — | [Collections F3](./batch-collections-git.md#f3--collection-and-environment-variable-editors-duplicate-table-mechanics-but-represent-different-persistence-models) |
| [ ] | INV-003 | P2 | Can environment switcher halves share presentation? | layout, environments | Medium-high | Over-generalization can hide selection/error differences | Characterize both scopes; share a local section/inline-create shell only | Not started | — | [Layout F6](./batch-layout-illustrations.md#f6--collectionglobal-halves-of-environmentswitcher-repeat-state-and-ui-flow) |
| [ ] | INV-004 | P3 | Are tiny indexed-row operations worth sharing? | request assertions/variables | Medium | Generic table would obscure domain side effects | Test immutable operations and result reindexing; stop at small helpers | Not started | — | [Request 7](./batch-request.md#7-assertions-and-post-response-variables-repeat-editable-row-table-mechanics) |
| [ ] | INV-005 | P3 | Can OAuth params reuse the key/value row core? | request/oauth2 | Medium | Identity and `sendIn` must remain typed | Prototype private generic row shell with extension cell; keep domain wrappers | Not started | — | [Request 8](./batch-request.md#8-oauth-additional-parameters-reimplement-most-of-keyvalueeditor) |
| [ ] | INV-006 | P3 | Is per-contract changelog aggregation still needed? | contract, contracts | Medium-high | May disappear with legacy retirement | Retire legacy summary first; add helper only if multiple modern consumers remain | Not started | — | [Contracts 14](./batch-contracts.md#14-changelog-kind-aggregation-is-recomputed-in-parallel-views) |
| [ ] | INV-007 | P3 | Should collection summaries become a query-backed resource? | layout, panes | Medium-low | Freshness/event invalidation semantics are complex | Specify stale/invalidation behavior before introducing shared query state | Not started | — | [Layout F8](./batch-layout-illustrations.md#f8--repeated-collection-summary-loading-across-layout-components) |
| [ ] | INV-008 | P3 | Is CodeMirror decoration lifecycle extraction simpler? | editor | High | Abstraction may obscure lifecycle/type behavior | Complete CD-009 first; extract only if a tiny typed helper survives one-plugin trial | Not started | — | [Editor F4](./batch-editor-ui.md#f4--three-codemirror-extensions-repeat-the-same-decoration-plugin-lifecycle) |

## `KEEP SEPARATE` / rejected patterns

These are explicit guardrails, not unreviewed omissions.

| ID | Pattern | Decision | Source |
|---|---|---|---|
| KS-001 | One serializer for requests and collection settings | Share only auth/header leaf adapters; resources have different persistence and inheritance contracts | [Cross-folder rejection](./cross-folder-review.md#one-serializer-for-request-payloads-and-collection-settings) |
| KS-002 | Reuse execution auth/header conversion for persistence | Reject: execution resolves OAuth/AWS and filters disabled headers by design | [Cross-folder rejection](./cross-folder-review.md#reusing-execution-authheader-conversion-for-persistence) |
| KS-003 | One universal editable-row/table component | Reject: variables, assertions, OAuth params, headers, and actions have different identity/validation/persistence | [Cross-folder rejection](./cross-folder-review.md#all-editable-row-tables-as-one-generic-component) |
| KS-004 | One universal recursive-tree abstraction | Reject: collection items, pane trees, breadcrumb path resolution, and close-all traversal are different algorithms | [Cross-folder rejection](./cross-folder-review.md#one-universal-collection-tree-traversal) |
| KS-005 | Merge similarly named clock/relative/date formatters | Keep clock precision, date-only, and elapsed-time APIs distinct | [Cross-folder rejection](./cross-folder-review.md#date-only-formatting-versus-relative-time-formatting) |
| KS-006 | Generic empty states, dialogs, shadcn/Radix wrappers, or floating surfaces | Keep primitive-specific accessibility and interaction contracts | [Editor/UI non-findings](./batch-editor-ui.md#intentional-variants-and-non-findings) |
| KS-007 | Parameterize the rocket illustration family | Keep assets independently editable; shared visual grammar is not duplicate behavior | [Illustration decision](./batch-layout-illustrations.md#illustration-assets-should-remain-separate) |
| KS-008 | Merge contract badges/status chips or table/timeline views | Keep different state machines, navigation, and presentation intents | [Contract naming overlap](./batch-contracts.md#naming-overlap-that-is-not-duplication) |
| KS-009 | Treat every clipboard write as clipboard-feedback duplication | Keep one-shot no-feedback actions separate unless UX requirements change | [Cross-folder rejection](./cross-folder-review.md#bare-clipboard-writes-and-feedback-producing-copy-controls) |
| KS-010 | Generic legacy-to-modern feature adapter | Retire each old surface through its own characterization/capability migration | [Cross-folder relationship](./cross-folder-review.md#i-parallel-legacy-feature-surfaces-share-a-deletion-pattern-not-a-reusable-abstraction) |
| KS-011 | Merge collection and environment persistence models | Share typed presentation only; preserve collection overrides and environment secret/duplicate policy | [Collections F3](./batch-collections-git.md#f3--collection-and-environment-variable-editors-duplicate-table-mechanics-but-represent-different-persistence-models) |
| KS-012 | Merge UI by visual resemblance alone | Similar charts, editors, popovers, skeletons, status badges, and specialized rows remain separate | [Cross-folder rejection](./cross-folder-review.md#empty-states-dialogs-shadcn-wrappers-and-illustrations-across-folders) |

## Small-step remediation roadmap

Each step is intended to be independently reviewable and testable. For any step touching collections, environments, requests, auth, or `.yml` persistence, first read `docs/superpowers/specs/opencollection-spec-reference.md`.

1. **Characterize auth/header persistence (CD-001, CD-002).** Add table-driven tests without changing production behavior. Cover every auth variant, `none`/`inherit`, OAuth aliases, AWS session tokens, disabled headers, blank keys, and save/reload.
   Validation: `yarn test <auth-mapping-test-pattern>`, `yarn test <header-mapping-test-pattern>`, `yarn tsc --noEmit`.
2. **Land leaf persistence adapters (CD-001, CD-002).** Migrate manual save and collection overview first; then auto-save. Compare exact payloads and keep execution mapping separate.
   Validation: the focused mapper suites, focused auto-save/request-save tests, `yarn tsc --noEmit`, `yarn check`.
3. **Unify the request payload mapper (CD-003).** Migrate normal save, then initial save with explicit overrides; verify all optional fields.
   Validation: `yarn test <request-save-pattern>`, `yarn test <save-to-collection-pattern>`, `yarn tsc --noEmit`.
4. **Remove destructive/dead persistence surfaces (CD-004, CD-005).** Add preservation/duplicate-policy tests, remove only the verified-dead collection dialog, and make environment diagnostics/save consume one policy.
   Validation: `yarn test <collection-overview-pattern>`, `yarn test <environment-variable-pattern>`, `yarn tsc --noEmit`, `yarn check`.
5. **Converge feature ownership one feature at a time (CD-006, CD-011, CD-012).** Make and test a capability checklist before each deletion; do not combine contracts, load testing, and request variables in one patch.
   Validation: `yarn test <contracts-pattern>`, then separately `yarn test <load-test-pattern>` and `yarn test <request-variables-pattern>`; run `yarn tsc --noEmit` after each patch.
6. **Land pure path/pane utilities (CD-007, CD-008, CD-020, CD-021).** Add unit tests first, then migrate call sites with no UX changes.
   Validation: `yarn test src/lib/__tests__/pane-utils.test.ts`, `yarn test <path-utils-pattern>`, `yarn tsc --noEmit`.
7. **Consolidate core parsers and pure domain helpers (CD-009, CD-010, CD-013, CD-014, CD-015, CD-019).** One helper family per patch, preserving public exports and explicit inclusion policies.
   Validation: `yarn test src/lib/__tests__/text-variables.test.ts`, `yarn test src/components/editor/__tests__/rok-types.test.ts`, plus focused contract/collection/Git utility tests and `yarn tsc --noEmit`.
8. **Consolidate local orchestration (CD-016–CD-018, CD-022–CD-026).** Characterize interaction/callback arguments first; keep filesystem paths and domain callbacks visible.
   Validation: focused collection-node, breadcrumb, tab-bar, import, and contract keyboard-navigation suites; then `yarn tsc --noEmit`.
9. **Normalize bounded UI behavior (CD-027–CD-037, CD-041).** Use fixed clocks/fake timers for relative time and clipboard; preserve OAuth fallback behavior and Git operation policy.
   Validation: focused OAuth tests, relative-time unit tests, clipboard hook tests, Git component/store tests, `yarn tsc --noEmit`, `yarn check`.
10. **Resolve investigations individually (INV-001–INV-008).** Start with a characterization test or written behavioral matrix. Promote to `CONSOLIDATE` only when the proposed abstraction is smaller and behavior-preserving; otherwise mark `Rejected` and add a `KEEP SEPARATE` entry.
    Validation: the focused command named in the linked report; for resize specifically include pointer, keyboard, orientation, min/max, and persistence tests.
11. **Finish mechanical P3 cleanup (CD-038–CD-040).** Keep these separate from behavior-changing work so review remains trivial.
    Validation: focused component tests, `yarn tsc --noEmit`, `yarn check`.
12. **Run broad frontend validation after each completed tranche.**
    Validation: `yarn test`, `yarn tsc --noEmit`, and `yarn check`. Manual smoke checks remain appropriate for pane drag behavior, native path pickers, and feature routing where automated coverage is absent.

## Source reports

- [Request](./batch-request.md)
- [Contracts](./batch-contracts.md)
- [Editor and UI](./batch-editor-ui.md)
- [Collections, Git, and environments](./batch-collections-git.md)
- [Layout, illustrations, and panes](./batch-layout-illustrations.md)
- [Remaining components](./batch-remaining.md)
- [Cross-folder reconciliation](./cross-folder-review.md)
