# Semantic Duplication Audit: Collections, Git, and Environments

Date: 2026-09-19

## Scope and method

Audited complete implementations under:

- `src/components/collections`
- `src/components/git`
- `src/components/environments`

`CLAUDE.md` and `docs/superpowers/specs/opencollection-spec-reference.md` were read before analysis. The existing `docs/reports/component-duplication/function-catalog.json` was used as an index, not as a substitute for source inspection. Every production file and the in-scope `MarkdownEditor` test were read directly.

The catalog was stale relative to the working tree during this audit: it did not index the newly added `src/components/environments/EnvironmentSidebar.tsx` or `src/components/environments/VariableTable.tsx`, and `EnvironmentDialog.tsx` was refactored while the audit was in progress. The findings below use the latest on-disk source after that refactor. No production code was edited.

Plausible shared survivors and call sites outside the three folders were searched where needed. In particular, the audit checked `src/lib/url-variables.ts`, component call sites, and existing pane-utility reporting before recommending new abstractions.

## Executive summary

The highest-risk duplication is in collection/environment persistence paths, not in Git presentation code:

1. **Retire `CollectionSettingsDialog`.** It is an unreferenced, behaviorally incomplete duplicate of `CollectionOverviewTab` and can erase persisted variables if reintroduced.
2. **Establish one environment-variable duplicate-key policy.** Validation, warning, and save-time normalization currently encode three versions of the same rule.
3. **Share variable-editor mechanics without conflating collection and environment persistence models.** The two editors look and behave similarly, but collection `initialValue` and secret-variable semantics make a single untyped editor unsafe.
4. **Centralize collection-tree traversal and request creation.** Several recursive scans and two create-and-open flows independently encode request/summary/path rules.
5. **Extract interaction mechanics from tree renaming and duplicated action menus, while keeping collection/folder/request persistence adapters separate.** Their UI lifecycle is duplicated; their filesystem semantics are not interchangeable.
6. **Consolidate Git relative-time formatting and diff normalization/fetch selection.** These are clear, low-risk utility extractions.
7. **Reduce repeated Git operation wrappers and error banners locally before considering store API changes.** The duplicate patterns are real, but operation-specific policy must remain explicit.

## Findings overview

| ID | Confidence | Priority | Duplicate intent | Recommendation / survivor |
|---|---:|---:|---|---|
| F1 | High | P0 | Two collection-settings editors persist the same resource with incompatible coverage | Keep `CollectionOverviewTab`; remove unreferenced `CollectionSettingsDialog` after a focused regression test |
| F2 | High | P0 | Environment duplicate-key detection/normalization is implemented three ways | Keep save-time normalization as the safety backstop; define one shared policy used by editor and persistence adapter |
| F3 | High | P1 | Collection and environment variable tables duplicate row editing mechanics | Keep model-specific containers; extract typed row/table primitives, using `VariableTable`'s `SingleLineEditor` behavior as the UI survivor |
| F4 | High | P1 | Four recursive collection-item walkers independently encode request/folder/summary traversal | Add one canonical collection-tree walker; keep aggregators as small callbacks |
| F5 | High | P1 | Collection, folder, request, and environment inline rename flows duplicate state and event handling | Extract rename interaction state only; retain distinct persistence callbacks |
| F6 | High | P1 | Root and folder request creation independently build, save, and open the same default request | Add one `createAndOpenRequest` helper parameterized by collection-relative path |
| F7 | High | P1 | Dropdown and context menus duplicate the same tree-node actions within three components | Define one action model per node and render it through menu-specific adapters |
| F8 | High | P2 | Git commit and stash timestamps use duplicate relative-time formatters | Keep one shared Git relative-time formatter with an explicit old-date policy |
| F9 | High | P1 | Git diff source selection, content normalization, and status derivation are repeated | Extract pure diff adapters plus one staged/working diff loader; keep `DiffViewer` as renderer |
| F10 | Medium-high | P2 | Branch and stash handlers repeat “run action, inspect store error, update local UI” orchestration | Add narrow local runners; later make store actions return typed results |
| F11 | High | P2 | The same dismissible Git error banner is rendered repeatedly | Introduce `GitErrorBanner`; preserve placement and wrapping options |
| F12 | Medium | P3 | `GitPanel` renders the same global dialogs in both repository branches | Move shared dialogs to one unconditional shell around branch-specific content |

## Detailed findings

### F1 — Competing collection-settings editors can persist incompatible snapshots

**Confidence:** High  
**Priority:** P0  
**Scope:** Within `collections`

#### References

- `src/components/collections/CollectionSettingsDialog.tsx:34-65` — second settings editor and duplicate scoped-variable-context assembly
- `src/components/collections/CollectionSettingsDialog.tsx:74-102` — independently maps auth and persists collection settings
- `src/components/collections/CollectionSettingsDialog.tsx:98` — always persists `variables: []`
- `src/components/collections/CollectionOverviewTab.tsx:81-183` — broader auth and header adapters
- `src/components/collections/CollectionOverviewTab.tsx:229-247` — scoped context including collection variables
- `src/components/collections/CollectionOverviewTab.tsx:293-365` — loads and persists auth, headers, docs, and variables
- Repository-wide reference search found no import or call site for `CollectionSettingsDialog`; only its declaration remains.

#### Intent

Both components edit collection-level authorization and default headers and call `saveCollectionSettings` for the same persisted collection resource.

#### Behavior differences

- `CollectionOverviewTab` loads the current collection before editing; `CollectionSettingsDialog` initializes empty auth and headers and never loads persisted values.
- `CollectionOverviewTab` supports Basic, Bearer, API key, OAuth 2.0, and AWS SigV4 mappings. `CollectionSettingsDialog` persists only Basic, Bearer, and API key; every other type becomes `undefined`.
- `CollectionOverviewTab` preserves and saves docs and collection variables. `CollectionSettingsDialog` omits docs and explicitly sends `variables: []`.
- `CollectionOverviewTab` includes collection variables in `buildScopedContext`; `CollectionSettingsDialog` does not.
- `CollectionOverviewTab` synchronizes collection auth with the in-memory auth store so ephemeral OAuth tokens survive tab changes. The dialog has no equivalent behavior.
- The dialog auto-closes after a success delay; the overview is a durable editor. That presentation difference does not justify a second persistence implementation.

#### Persistence and OpenCollection risks

This is the highest-risk duplicate in scope. If `CollectionSettingsDialog` is wired back into the UI, saving only auth or headers can erase `request.variables[]` by writing `variables: []`. It can also silently remove persisted OAuth 2.0 or AWS SigV4 configuration because unsupported auth types map to `undefined`.

Those fields correspond to collection request defaults in `opencollection.yml`. A partial UI must not construct a replacement snapshot that drops unrelated OpenCollection fields. The risk is data loss, not merely inconsistent presentation.

#### Recommendation / survivor

Use `CollectionOverviewTab` as the behavioral survivor. Do not extract mappings from the obsolete dialog. If a compact settings surface is still required, make it a thin view over a single collection-settings draft/controller whose save operation starts from the loaded collection and preserves fields outside the compact view.

#### Staged remediation

1. Add a focused regression test around the survivor proving that changing headers preserves variables, docs, OAuth/AWS auth fields, and any other loaded settings.
2. Confirm with product/navigation owners that no dynamic import or pending feature expects `CollectionSettingsDialog`.
3. Delete `CollectionSettingsDialog` and its dead exports/imports.
4. If a compact editor is later needed, build it on the survivor's tested adapters and merge-based save semantics rather than restoring this implementation.

---

### F2 — Environment duplicate-key policy is implemented three ways and disagrees at the UI boundary

**Confidence:** High  
**Priority:** P0  
**Scope:** Within `environments`

#### References

- `src/components/environments/EnvironmentDialog.tsx:36-55` — `dedupeVariables`, keeping the last value in the first occurrence's position
- `src/components/environments/EnvironmentDialog.tsx:96-105` — save-time normalization
- `src/components/environments/EnvironmentDialog.tsx:154-165` — edit-time duplicate prevention after trimming the proposed key
- `src/components/environments/VariableTable.tsx:38-42` — independent duplicate counting without trimming
- `src/components/environments/VariableTable.tsx:76-103` — duplicate visual state
- `src/components/environments/VariableTable.tsx:157-160` — message that “only the last one will be saved”

#### Intent

All three paths answer the same domain question: what should happen when an environment contains more than one variable with the same key?

#### Behavior differences

- `updateVariable` blocks a duplicate key and shows a toast.
- `VariableTable` allows duplicates in its input model, highlights every duplicate, and tells the user the last one will be saved.
- `dedupeVariables` permits duplicates in state but collapses them during save.
- Edit-time validation trims the candidate for comparison but writes the untrimmed patch if accepted. Table duplicate counting and save-time deduplication compare exact untrimmed strings.
- Save-time normalization replaces the first matching slot with the last matching object. Thus “last value wins,” but ordering remains the first occurrence's position.
- Empty keys are intentionally not deduplicated.

#### Persistence and OpenCollection risks

OpenCollection describes `variables` as an array and does not, by itself, establish RocketAPI's duplicate-key resolution behavior. RocketAPI later converts enabled variables into maps in multiple places, where duplicate names necessarily collapse. Inconsistent trimming and winner rules can therefore make the editor preview, variable resolution, and persisted `.yml` disagree.

The policy also interacts with secret variables: collapsing two rows must preserve the winning row's `secret` and type semantics, not merge fields from two representations.

#### Recommendation / survivor

Keep save-time normalization as a defensive survivor, but move it to a named, tested domain-facing frontend utility such as `normalizeVariablesForPersistence`. Define one key canonicalization function and one duplicate policy. The table and edit handler should consume the same policy.

A reasonable current-compatible policy is:

- empty keys may coexist while drafting;
- non-empty keys compare using one explicitly chosen normalization rule;
- duplicates are visible and block save, or are allowed with a clearly documented last-wins save rule—but not both.

Do not silently change to case-insensitive matching without a product/spec decision.

#### Staged remediation

1. Add tests for exact duplicates, whitespace variants, empty keys, enabled/disabled rows, and secret rows.
2. Extract key canonicalization and normalization as pure functions, preserving current last-wins persistence behavior initially.
3. Make `EnvironmentDialog` validation and `VariableTable` diagnostics use those functions.
4. Choose one UX policy—block duplicates or allow-and-collapse—and update the message and save enablement together.
5. Only after compatibility tests pass, consider migrating already persisted duplicate rows.

---

### F3 — Collection and environment variable editors duplicate table mechanics but represent different persistence models

**Confidence:** High  
**Priority:** P1  
**Scope:** Between `collections` and `environments`

#### References

- `src/components/collections/CollectionVariablesEditor.tsx:20-40` — update/remove/add array operations
- `src/components/collections/CollectionVariablesEditor.tsx:53-140` — enabled, key, initial/current value, secret toggle, and delete rows
- `src/components/environments/VariableTable.tsx:28-42` — analogous table setup and duplicate analysis
- `src/components/environments/VariableTable.tsx:44-166` — analogous enabled, key, value, secret toggle, and delete rows
- `src/components/environments/VariableTable.tsx:169-193` — add/save footer and shared save-state presentation
- `src/components/environments/EnvironmentDialog.tsx:179-205` — parent-owned add/remove mutations
- `src/components/collections/FolderVariablesPopover.tsx:70-80` — reuses the collection editor for folder variables

#### Intent

Both components edit ordered variable rows with the same core interactions: enable/disable, edit key/value, mask/unmask secret values, add, remove, and show an empty state.

#### Behavior differences

- Collection/folder rows are `CollectionVariable` and expose both `initialValue` and current `value`; environment rows are `Variable` and expose one value field.
- `CollectionVariablesEditor` owns immutable array updates. `VariableTable` delegates row mutations to its parent.
- `VariableTable` uses `SingleLineEditor` for variable-aware values and accepts a variable context. `CollectionVariablesEditor` uses plain `Input` controls.
- Environment editing has duplicate-key diagnostics and save controls; collection editing has neither in the component.
- The collection editor includes scope-specific explanatory copy and is reused for folder variables.

#### Persistence and OpenCollection risks

A naive single `VariableEditor` would be dangerous. RocketAPI's collection convention uses `value ?? initialValue`, where `initialValue` is shared/Git-committed and `value` is a local override. Environment `Variable` and OpenCollection `SecretVariable` have different allowed fields; the spec says a secret variable has `secret: true` and no `value` field, while the current frontend model exposes masking through `secret` plus editable values.

Consolidating by coercing both models into one loose object could drop `initialValue`, serialize a secret incorrectly, or flatten collection/folder/environment scope differences.

#### Recommendation / survivor

Keep model-specific containers and persistence adapters. Extract only typed presentation primitives:

- a row shell for enabled/key/action controls;
- a secret-toggle action;
- empty-state/add controls;
- optionally a generic table driven by a typed column definition.

Use `VariableTable`'s `SingleLineEditor` value-cell behavior as the UI survivor for variable-aware single-line values. Add collection-specific columns for initial/current values rather than reducing collection rows to environment rows.

#### Staged remediation

1. Add tests around collection `initialValue`/`value` preservation and environment secret rows before extracting UI.
2. Extract stateless row/action primitives with no knowledge of persistence types.
3. Migrate `CollectionVariablesEditor` to those primitives and `SingleLineEditor` while retaining its two value columns.
4. Keep `EnvironmentDialog` and collection/folder containers responsible for model conversion, duplicate policy, and save behavior.
5. Revisit the frontend secret representation separately against the OpenCollection persistence adapter; do not mix that migration into the visual deduplication.

---

### F4 — Recursive collection-item traversal is independently reimplemented four times

**Confidence:** High  
**Priority:** P1  
**Scope:** Within `collections`

#### References

- `src/components/collections/CollectionOverviewTab.tsx:56-73` — `countRequests` and `countFolders`
- `src/components/collections/MethodBreakdown.tsx:10-21` — `countMethods`
- `src/components/collections/RequestList.tsx:24-56` — `groupItems` plus `flattenRequests`
- `src/components/collections/TagsList.tsx:9-27` — `collectTags`

#### Intent

Each helper recursively walks the same `CollectionItem[]` folder tree and aggregates request/folder metadata for collection overview UI.

#### Behavior differences

- `countRequests` and `countMethods` treat both `request` and `summary` as requests.
- `flattenRequests` intentionally excludes `summary` because summaries have no request body for the detailed list.
- `collectTags` considers only full requests and only when tags exist.
- `countFolders` counts every nested folder, whereas `groupItems` preserves only top-level folder grouping and flattens descendants into that group.
- Aggregation and output shapes differ: scalar counts, method map, request array, and tag map.

#### Persistence and OpenCollection risks

These are read-only helpers, so immediate persistence risk is low. The semantic risk is request-type drift. OpenCollection supports multiple request kinds and a recursive folder model; RocketAPI also uses a `summary` item variant. If one walker is updated for a new item representation while another is not, overview counts, method charts, request lists, and tags will disagree.

A generic flattening helper must not erase folder ancestry or start treating summaries as full persisted requests. In unbundled layout, paths and folder ancestry remain important.

#### Recommendation / survivor

Add one canonical depth-first visitor/walker in an existing collection utility module, yielding each item with ancestry/depth. Keep each aggregation as a small local reducer with explicit inclusion rules such as `includeSummaries` rather than forcing every view through one flattened array.

There is no complete current survivor. `sortItemsFoldersFirst` already establishes `src/lib/collection-utils` as a plausible home.

#### Staged remediation

1. Add fixtures covering root requests, nested folders, summaries, tags, and multiple methods.
2. Introduce and test a pure walker that preserves item identity and ancestry without changing ordering.
3. Migrate `countRequests`/`countFolders` first and compare rendered totals.
4. Migrate method and tag aggregation with explicit request/summary predicates.
5. Migrate `flattenRequests` last because its deliberate omission of summaries and top-level grouping semantics differ most.

---

### F5 — Inline rename interaction state is duplicated across four resource types

**Confidence:** High  
**Priority:** P1  
**Scope:** Within `collections` and between `collections`/`environments`

#### References

- `src/components/collections/CollectionNode.tsx:91-95`, `201-217`, `325-337` — collection rename state, in-flight guard, and input events
- `src/components/collections/FolderNode.tsx:78-87`, `94-120`, `184-199` — folder rename with additional blur-cancellation guard
- `src/components/collections/RequestNode.tsx:126-146`, `203-215` — request rename state and input events
- `src/components/environments/InlineEnvName.tsx:28-58`, `60-94` — environment rename state, validation, accept/cancel buttons, and blur handling

#### Intent

All four implementations manage a draft name, trim/no-op validation, Enter-to-accept, Escape-to-cancel, async persistence, and edit-mode exit.

#### Behavior differences

- Collection and request renames call dedicated backend commands; folder rename computes a sibling path and calls `moveItem`.
- Environment rename delegates persistence and rejects names already present in `existingNames`.
- Folder rename has `renameCancelled` specifically to prevent an unmount-triggered blur from submitting twice. Collection and request rename do not have the same guard.
- Environment blur always cancels, while collection/folder/request blur attempts to save.
- Environment exposes explicit accept/cancel buttons and keeps edit mode open when persistence fails. Tree nodes log failures and close edit mode in `finally`.
- Collection rename changes the collection root; folder/request rename changes unbundled `.yml` paths. These are not equivalent domain operations.

#### Persistence and OpenCollection risks

The shared UI behavior sits directly in front of filesystem-sensitive operations. OpenCollection's unbundled layout requires slugified collection/folder/request names and `.yml` files. A generic rename implementation that manipulates paths in the frontend could bypass backend slugging, fail to update open tab sources, or move an item to the wrong folder.

Conversely, leaving event handling duplicated risks double submissions on blur/Enter or divergent failure behavior. Folder already contains a workaround absent from the other tree nodes.

#### Recommendation / survivor

Extract a small `useInlineRename` interaction hook or controlled `InlineRenameField` that owns draft state, in-flight protection, Enter/Escape/blur policy, and failure retention. Keep resource-specific persistence callbacks outside the hook.

Use the folder implementation's explicit cancellation/in-flight handling as the tree-input survivor, but make blur-submit versus blur-cancel a required option. Keep environment duplicate-name validation as an environment adapter.

#### Staged remediation

1. Add interaction tests for Enter, Escape followed by blur, repeated Enter, persistence failure, and unchanged/blank names.
2. Extract the interaction state with an injected `onCommit(trimmedName)` and explicit `blurBehavior`.
3. Migrate collection and request first; verify one backend call per user action.
4. Migrate folder while preserving path construction and `moveItem` outside the hook.
5. Migrate `InlineEnvName` only if explicit buttons and duplicate-name validation remain intact.

---

### F6 — Root and folder request creation duplicate the same create-save-open transaction

**Confidence:** High  
**Priority:** P1  
**Scope:** Within `collections`

#### References

- `src/components/collections/CollectionNode.tsx:249-282` — root request creation
- `src/components/collections/FolderNode.tsx:122-156` — folder request creation
- `src/components/collections/CollectionNode.tsx:559-574` and `src/components/collections/FolderNode.tsx:365-380` — duplicate inline name-entry UI

#### Intent

Both handlers trim a name, generate a UUID, create the same default GET payload, call `saveRequest`, construct the same request tab using `createDefaultRequest`, and open it.

#### Behavior differences

- The root path is the request name; the folder path is `${basePath}/${reqName}`.
- Error labels differ.
- Root fallback source path is `${name}.yml`; folder fallback is `${path}.yml`.
- `CollectionNode` also has a `CreateRequestDialog` path, while its inline `creatingRequest` flow appears separate from the menu path. `FolderNode` uses only the inline flow.

#### Persistence and OpenCollection risks

The relative path difference is essential in the unbundled layout. A helper that accepts only a display name could accidentally save a folder request at collection root. Fallback paths also assume `.yml`, which is correct and must remain so.

The duplicated default payload can drift from the canonical request model or from `CreateRequestDialog`, producing different persisted defaults depending on where the request is created.

#### Recommendation / survivor

Create one tested `createAndOpenRequest` application/UI helper that accepts:

- collection name;
- collection-relative request path;
- display name;
- optionally a tab target.

It should own UUID generation, the canonical default payload, `saveRequest`, and tab construction. Keep path derivation in `CollectionNode`/`FolderNode`, where ancestry is visible. Also decide whether `CreateRequestDialog` is the intended root survivor; do not retain two root creation UIs without a product reason.

#### Staged remediation

1. Test root and nested-folder paths, returned `fileName`, `.yml` fallback, and opened tab source.
2. Extract the create-save-open transaction without changing either UI.
3. Migrate folder creation, then root creation, comparing backend arguments.
4. Reconcile the root inline flow with `CreateRequestDialog` and remove the unused/duplicate interaction only after UI behavior is confirmed.
5. Update the helper when canonical request defaults evolve, rather than editing each node.

---

### F7 — Tree-node dropdown and context menus duplicate the same action sets

**Confidence:** High  
**Priority:** P1  
**Scope:** Within `collections`

#### References

- `src/components/collections/CollectionNode.tsx:401-460` and `464-516` — duplicate dropdown/right-click actions
- `src/components/collections/FolderNode.tsx:211-264` and `267-309` — duplicate dropdown/right-click actions
- `src/components/collections/RequestNode.tsx:239-321` and `325-389` — duplicate dropdown/right-click actions, including move and pane targeting

#### Intent

Each node exposes the same commands through a hover dropdown and a right-click context menu. Within each component, labels, icons, enablement, and callbacks are repeated.

#### Behavior differences

- The Radix dropdown and context-menu primitives require different item/submenu component types.
- Collection's dropdown “New Folder” awaits completion and expands the node; its context-menu equivalent fires `void onNewFolder(...)` and does not explicitly expand.
- Collection overview opening is implemented through `handleDoubleClick` in one menu and inline tab construction in the other.
- Request menu pane actions are repeated in full, including one-versus-many target-pane branching.
- Destructive action callbacks are equivalent within each node type, but resource payloads differ across node types.

#### Persistence and OpenCollection risks

The collection “New Folder” divergence already demonstrates behavioral drift. For move/delete/duplicate operations, a menu-only divergence can target the wrong collection-relative path or omit a refresh/expansion step. Those operations affect the unbundled collection directory and `.yml` item files.

#### Recommendation / survivor

Define one action descriptor list per node render—label, icon, disabled state, callback, destructive/separator metadata, and optional children. Render that list through small dropdown-menu and context-menu adapters. For actions whose behavior differs, first choose and test the intended behavior; do not encode divergence in renderer branches.

Keep resource-specific action creation in each node. A single universal collection/folder/request action factory would obscure important path and pane differences.

#### Staged remediation

1. Add tests that invoke each action from both surfaces and assert equivalent callbacks/arguments.
2. Resolve the collection “New Folder” expansion/await discrepancy and overview-handler discrepancy.
3. Extract request pane-target action descriptors first because they have the largest duplicate block.
4. Add menu adapters, then migrate folder and collection actions.
5. Keep visual primitive differences in the adapters and domain callbacks in node-specific code.

---

### F8 — Git relative-time formatting has two near-identical implementations with an old-date divergence

**Confidence:** High  
**Priority:** P2  
**Scope:** Within `git`

#### References

- `src/components/git/GitCommitLog.tsx:8-21` — `relativeTime`
- `src/components/git/GitCommitLog.tsx:81-86` — commit usage
- `src/components/git/GitStashSection.tsx:26-37` — `formatAge`
- `src/components/git/GitStashSection.tsx:274-277` — stash usage

#### Intent

Both convert a Git timestamp to `just now`, minutes, hours, days, and an older-date label.

#### Behavior differences

- For 30 days and newer, behavior is effectively identical.
- Older commits use an unbounded month count such as `14mo ago`.
- Older stashes use a locale-sensitive calendar date containing month and day but no year.
- Neither explicitly handles invalid timestamps or future timestamps.

#### Persistence and spec risks

There is no collection/environment persistence risk. The user-visible risk is inconsistent Git chronology, especially around old dates and locale/year ambiguity. A malformed timestamp can render `NaNmo ago` or an invalid date string.

#### Recommendation / survivor

Create one Git-specific `formatRelativeTime` utility with tests and an explicit policy for older dates. Prefer `Intl.RelativeTimeFormat` for relative ranges plus an absolute date including year when sufficiently old. Neither current function is a complete survivor; `formatAge`'s switch to calendar dates is more readable but must include the year when relevant.

#### Staged remediation

1. Add fixed-clock tests for boundaries, future values, invalid timestamps, and dates older than one year.
2. Implement one pure formatter without changing rendered component structure.
3. Migrate commit log and stash section together so labels remain consistent.
4. Remove both local functions.

---

### F9 — Git diff loading and normalization are repeated across three components

**Confidence:** High  
**Priority:** P1  
**Scope:** Within `git`

#### References

- `src/components/git/CommitDiffView.tsx:14-29` — maps `FileDiff` to `DiffState` and separately derives the same status for the file list
- `src/components/git/DiffViewForFile.tsx:19-50` — chooses staged/working IPC call and normalizes nullable content into `DiffState`
- `src/components/git/DiffViewer.tsx:70-87` — repeats staged/working IPC selection and nullable-content normalization when toggling modes
- `src/components/git/DiffViewer.tsx:89-104` — renderer mode selection

#### Intent

These paths all convert backend diff data into renderable old/new strings and status metadata. Two paths also choose between `gitDiffStaged` and `gitDiff` based on a staged flag.

#### Behavior differences

- Commit diffs infer status solely from nullable old/new content and force `isStaged: true`; working-tree diffs receive status from `FileStatus`.
- `CommitDiffView.fileStatus` and `fileDiffToDiffState` independently repeat the same nullable-content status rule.
- `DiffViewForFile` exposes loading/error state. `DiffViewer` silently retains the previous state when a staged/working toggle fetch fails.
- `DiffViewer` owns local state initialized from props; commit rendering disables the stage toggle.

#### Persistence and OpenCollection risks

Diff rendering is read-only, but visual mode parses `.yml` request files. Wrong added/deleted classification or inconsistent empty-content normalization can misrepresent OpenCollection file changes. An empty file is not the same as a missing side; nullability must remain available for status inference before conversion to `''`.

#### Recommendation / survivor

Extract two pure adapters and one loader:

- `statusFromFileDiff(diff)`;
- `toDiffState(diff, metadata)` that infers status before null normalization;
- `loadWorkingTreeDiff(collectionPath, filePath, isStaged)`.

Keep `DiffViewer` as the rendering survivor and keep loading/error policy in callers. Do not make the pure adapter decide whether an IPC failure should be shown or ignored.

#### Staged remediation

1. Add adapter tests for added, deleted, modified, and genuinely empty files.
2. Replace `CommitDiffView.fileStatus` and `fileDiffToDiffState` with the tested adapter.
3. Extract the staged/working loader and use it in `DiffViewForFile` without changing UI behavior.
4. Use the same loader in `DiffViewer`, preserving its current “retain on error” policy initially.
5. In a separate UX change, decide whether toggle failures should surface an error.

---

### F10 — Git operation handlers repeat store-error inference and busy-state orchestration

**Confidence:** Medium-high  
**Priority:** P2  
**Scope:** Within `git`

#### References

- `src/components/git/BranchSelector.tsx:41-52` — create branch and compare prior/current global error
- `src/components/git/BranchSelector.tsx:54-76` — switch and remote-checkout handlers with identical success/error flow
- `src/components/git/BranchSelector.tsx:82-96` — merge variant of the same flow with conflict-specific closing
- `src/components/git/GitStashSection.tsx:88-119` — three batch handlers with identical busy/clear/run/check/clear-selection flow
- `src/components/git/GitLandingPanel.tsx:53-67`, `83-89`, `139-173` — repeated operation-specific busy wrappers

#### Intent

Each cluster runs a store action, derives success from shared store state, and then updates local UI such as closing a popover, clearing input/selection, or toggling a spinner.

#### Behavior differences

- Branch creation preserves the popover and shows `createError`; switch/checkout close on success; merge closes on conflicts but shows other errors inline.
- Stash batch handlers differ only by action (`apply`, `pop`, `drop`).
- Landing operations have meaningful safety policies: pull may auto-stash; push may require fetch; fetch updates a timestamp. Those policies should not be collapsed into one opaque generic runner.
- Some handlers call async store actions without `await` when credentials are missing, relying on the store to open a dialog.

#### Persistence and spec risks

Git operations can alter every `.yml` file in a collection. The principal risk is false success detection: comparing a previous global error string with the next one treats a repeated identical failure as success. Clearing UI state after such a false success can hide an operation that did not occur.

For stash/pull flows, over-generalization could also alter conflict safety or restore a stash onto a conflicted index.

#### Recommendation / survivor

First extract narrow local runners:

- in `BranchSelector`, `runBranchAction(action, successBehavior, errorBehavior)` while keeping merge conflict policy explicit;
- in `GitStashSection`, `runBatch(action)` parameterized by the store method.

The stronger long-term survivor should be typed store action results (`{ ok: true } | { ok: false; error, kind }`) rather than inferring outcomes from mutable global error text. Do not generalize `GitLandingPanel`'s safety workflows until the store result contract exists.

#### Staged remediation

1. Add tests for repeated identical errors, conflicts, successful closure, and selection retention on failure.
2. Deduplicate the three stash batch handlers locally.
3. Deduplicate switch/checkout flow, leaving merge as an explicit policy wrapper.
4. Change store actions incrementally to return typed outcomes.
5. Migrate UI handlers away from before/after global-error comparisons; only then consider a shared async-operation hook.

---

### F11 — Dismissible Git error banners are copied across modes and components

**Confidence:** High  
**Priority:** P2  
**Scope:** Within `git`

#### References

- `src/components/git/ConflictResolver.tsx:67-80` — manual-mode error banner
- `src/components/git/ConflictResolver.tsx:111-124` — exact second copy in side-by-side mode
- `src/components/git/GitLandingPanel.tsx:291-304` — same icon/message/dismiss structure
- `src/components/git/GitStashSection.tsx:158-164` — closely related non-dismissible banner
- `src/components/git/BranchSelector.tsx:113-117` and `241-245` — compact inline error variants

#### Intent

All render Git operation failures with a destructive color treatment, alert icon, wrapped message, and sometimes a clear action.

#### Behavior differences

- Conflict resolver and landing panel are dismissible through `clearError`; stash is not despite reading the same store error.
- Placement and margins differ.
- Stash uses `break-all`; other banners use `wrap-break-word`.
- Branch selector has compact popover-specific variants and separate create/switch messages.

#### Persistence and spec risks

There is no direct persistence-format risk. Operationally, inconsistent dismissal can leave stale errors visible or clear a shared store error from one surface while another operation is active. A shared banner should not itself decide error ownership.

#### Recommendation / survivor

Introduce a presentational `GitErrorBanner` accepting `message`, optional `onDismiss`, compactness, and class overrides. Replace the exact two copies in `ConflictResolver` first, then the landing and stash forms. Keep branch popover errors local unless the compact variant remains simple.

#### Staged remediation

1. Extract the exact `ConflictResolver` banner without changing store interactions.
2. Add optional placement/wrapping props and migrate `GitLandingPanel`.
3. Decide whether stash errors should be dismissible before migrating that call site.
4. Leave operation error ownership and `clearError` timing in the parent components.

---

### F12 — `GitPanel` duplicates global dialog rendering across repository branches

**Confidence:** Medium  
**Priority:** P3  
**Scope:** Within `git`

#### References

- `src/components/git/GitPanel.tsx:177-209` — non-repository branch renders credentials, identity, and clone dialogs
- `src/components/git/GitPanel.tsx:344-357` — repository branch renders credentials, identity, remotes, and clone dialogs
- `src/components/git/GitPanel.tsx:107-118` — shared identity callbacks used by both branches

#### Intent

Both branches keep global Git dialogs mounted and wired to the same store state/callbacks.

#### Behavior differences

- Remotes are meaningful only for an initialized repository and appear only in the repository branch.
- Clone is available in both branches.
- Credentials and identity setup can be triggered by store operations in either branch.

#### Persistence and spec risks

There is no OpenCollection serialization risk. The maintenance risk is wiring drift: one branch can miss updated initial values, callback behavior, or a newly required dialog. Because credential/identity state gates Git operations, a stale branch can strand a pending operation.

#### Recommendation / survivor

Refactor `GitPanel` to choose branch-specific main content and render one shared dialog shell afterward. Keep `GitRemotesDialog` conditionally available only when `isRepo === true`. The repository branch's dialog block is the more complete wiring survivor.

#### Staged remediation

1. Add a rendering test for credential and identity prompts in both repo states.
2. Extract a local `dialogs` fragment or small `GitPanelDialogs` component.
3. Render it once after branch-specific content, conditionally including remotes.
4. Verify clone-close and pending-credential activation behavior in both states.

## Deliberate non-findings

The following similarities should not be consolidated now:

- **`GitCloneDialog` and `GitCredentialsDialog`:** both are multi-step forms, but one orchestrates clone/detection/workspace opening while the other manages secret credential types and keychain persistence. Shared form styling is already provided by UI primitives; a domain abstraction would add coupling.
- **`GitIdentityDialog` and environment inline naming:** both validate text fields, but identity is a two-field modal with email semantics and environment naming is an inline resource rename. Only generic UI primitives are shared.
- **`VisualDiffView.DiffField` and `KVTable`:** both render structured changes, but one represents scalar before/after fields and the other keyed row-set changes. Their data models and layouts are intentionally different.
- **Collection contract status computations:** `CollectionNode` aggregates collection-level counts while `RequestNode` computes highest severity for covering scopes. They use related data but do not have the same intent.
- **Git fetch/pull/push handlers:** they share busy-state scaffolding, but pull/stash conflict safety and fetch-before-push policy are materially different. F10 recommends typed results and narrow runners, not one generic “run Git action” function.
- **Markdown edit/preview and diff text/visual tabs:** both toggle modes, but their state, persistence, and rendering lifecycles differ; shared `Tabs` primitives are sufficient.

## Recommended remediation order

### Stage 1 — Protect persistence behavior

1. Add preservation tests and retire `CollectionSettingsDialog` (F1).
2. Test and centralize environment duplicate-key policy (F2).
3. Add model-preservation tests for collection/folder/environment variable rows before sharing presentation (F3).

### Stage 2 — Consolidate collection-tree behavior

1. Add a canonical collection-item walker and migrate read-only aggregators (F4).
2. Extract request create-save-open orchestration with explicit relative paths (F6).
3. Extract inline-rename interaction mechanics while retaining persistence adapters (F5).
4. Unify action descriptors for dropdown and context menus after parity tests (F7).

### Stage 3 — Consolidate low-risk Git utilities

1. Unify relative-time formatting (F8).
2. Extract diff status/normalization adapters and staged/working loader (F9).
3. Extract the shared Git error banner (F11).
4. Render `GitPanel` dialogs once (F12).

### Stage 4 — Improve Git operation contracts

1. Deduplicate local branch and stash wrappers (F10).
2. Make store operations return typed results.
3. Remove UI inference based on mutable global error strings.
4. Revisit broader operation hooks only after conflict and credential outcomes are explicit.

## Validation guidance for future remediation

Because this audit made no production changes, no build or test command was required for the report itself. Future patches should validate narrowly first:

- collection settings and variables: focused Vitest suites plus `yarn tsc --noEmit`;
- collection tree creation/rename/actions: component tests asserting backend call arguments and tab source paths;
- environment normalization: pure utility tests with secret/empty/duplicate cases;
- Git diff/time utilities: pure unit tests with fixed timestamps and null/empty diff sides;
- Git operation orchestration: store/component tests for repeated errors, conflicts, credential prompts, and selection retention;
- final frontend validation: `yarn check` and the relevant `yarn test` patterns.
