# Cross-folder semantic duplication review

Date: 2026-09-19

## Scope and method

This review reconciles the six batch audits for all of `src/components` and concentrates on duplication that crosses their assigned boundaries. It is not a concatenation of those reports.

Inputs read in full:

- `CLAUDE.md`
- `docs/superpowers/specs/opencollection-spec-reference.md`
- `docs/reports/component-duplication/categorized-functions.json`
- all six `docs/reports/component-duplication/batch-*.md` reports

The categorized catalog was used as an index. Every implementation cited as a **new** cross-batch candidate below was checked in current source, and the references below are current line ranges. Existing batch findings are cited by report ID only when discussing overlap; they are not restated as new findings.

Confidence indicates confidence that the implementations own the same reusable responsibility. Priority reflects data-loss risk, behavioral drift, and remediation value.

## Executive summary of new cross-batch groups

| ID | Confidence | Priority | Cross-batch duplicate intent | Recommended survivor/direction |
|---|---:|---:|---|---|
| X1 | High | P0 | Convert between frontend `AuthState` and persisted request/default auth | Introduce one persistence-specific bidirectional auth adapter; preserve request-versus-default empty/inherit policy in thin wrappers |
| X2 | High | P0 | Convert editable header rows to persisted headers | One `toPersistedHeaders` helper that retains nonblank disabled rows; keep execution filtering separate |
| X3 | High | P2 | Format timestamps as concise relative time | One tested formatter with explicit display modes and future/invalid/old-date policies |
| X4 | High | P1 | Extract the final segment from host filesystem paths | One cross-platform `hostPathBasename`; keep collection-relative path operations separate |
| X5 | High | P2 | Copy text and expose keyed transient success feedback | One timer-safe, rejection-aware keyed clipboard-feedback hook |
| X6 | Medium-high | P2 | Implement accessible resizable panel separators | Prefer the existing `ui/resizable` primitive where it preserves behavior; add custom hook logic only for unsupported pixel/persistence cases |

---

## 1. Verified new cross-batch duplicate groups

### X1 — Persisted auth conversion is duplicated across collection defaults, manual request save, request loading, and auto-save

**Confidence:** High  
**Priority:** P0

#### Exact current references

Component implementations:

- `src/components/collections/CollectionOverviewTab.tsx:81-132` — converts persisted collection-default auth into frontend `AuthState`.
- `src/components/collections/CollectionOverviewTab.tsx:136-172` — converts frontend `AuthState` back to optional persisted collection-default auth.
- `src/components/request/SaveRequestButton.tsx:16-50` — independently converts the same frontend auth variants into persisted request auth.

Existing shared/non-component paths that establish the duplication and drift:

- `src/lib/pane-utils.ts:14-56` — request-load conversion from persisted auth to `AuthState`; substantially overlaps `CollectionOverviewTab.toAuthState`.
- `src/lib/auto-save.ts:9-13` — auto-save uses a third persistence path: OAuth gets `oauth2StateToApiAuth`, while all other variants pass through execution-oriented `toApiAuth`.
- `src/lib/execute-request.ts:80-115` — `toApiAuth` is explicitly execution-oriented: OAuth becomes a bearer token and AWS SigV4 becomes `none`. It is not a safe persistence survivor.
- `src/lib/oauth2-mapping.ts:85-202` and `src/lib/oauth2-mapping.ts:203-295` — the existing canonical OAuth-specific bidirectional mapping.

#### Shared intent

All cited conversion paths translate between the frontend's nested `AuthState` and the flat IPC/persistence auth representation. Collection defaults and requests need different handling for “no local auth,” but Basic, Bearer, API key, OAuth 2.0, and AWS SigV4 field mapping should have one definition.

#### Material differences and current risk

- Collection defaults return `undefined` for `none` and `inherit`; manual request save returns `{ authType: 'none' }`.
- `CollectionOverviewTab` accepts both `o-auth2` and `oauth2` when loading; `mapApiRequestToState` handles only `o-auth2`.
- Empty OAuth state becomes `undefined` for collection defaults but `none` for request save.
- AWS `sessionToken` is omitted when empty by the collection mapper but persisted as an empty string by request save.
- Request loading uses `fromCollection` to choose `inherit` versus `none` for an unrecognized/default auth case; the collection mapper always chooses `none`.
- Most importantly, `src/lib/auto-save.ts:9-13` sends AWS SigV4 through `toApiAuth`, whose execution behavior deliberately returns `none` at `src/lib/execute-request.ts:106-109`. A request that manual save preserves can therefore lose AWS auth under auto-save.
- The OpenCollection reference distinguishes persisted inheritance (`"inherit"`) from concrete auth. Consolidation must not silently normalize omission, `none`, and inheritance into one value without checking the backend DTO/persistence contract.

#### Recommendation / survivor

There is no complete current single survivor. Create a persistence-specific adapter, separate from execution resolution, in a shared auth mapping module:

- `persistedAuthToState(auth, { missing: 'none' | 'inherit' })`
- `authStateToPersistedAuth(auth, { empty: 'none' | 'omit' | 'inherit' })`

Use the complete variant coverage in `SaveRequestButton.authForSave` and `CollectionOverviewTab.authStateToApi` as behavioral inputs, the alias tolerance in `CollectionOverviewTab.toAuthState`, and the existing OAuth functions in `src/lib/oauth2-mapping.ts`. Keep `toApiAuth` exclusively for execution; its OAuth and AWS semantics prove that it must not be reused for persistence.

#### Staged remediation

1. Add table-driven tests for every auth variant, both OAuth discriminator spellings, missing OAuth state, empty AWS session tokens, unknown auth, and request-versus-collection `none`/`inherit` behavior.
2. Extract persisted-auth-to-state conversion from `pane-utils` and `CollectionOverviewTab` into one pure adapter.
3. Extract state-to-persisted-auth conversion with an explicit empty-auth policy; do not infer policy from the caller type.
4. Migrate `SaveRequestButton` and `CollectionOverviewTab` first and compare exact IPC payloads.
5. Migrate `auto-save` away from execution-oriented `toApiAuth`, specifically verifying AWS SigV4 and OAuth persistence.
6. Only after parity is established, reconcile current `none`/omission behavior with the OpenCollection `inherit` requirement as a separate compatibility change.

---

### X2 — Persisted header-row serialization is repeated and auto-save drops disabled headers

**Confidence:** High  
**Priority:** P0

#### Exact current references

- `src/components/collections/CollectionOverviewTab.tsx:350-365` — filters blank keys and persists `key`, `value`, and `enabled` for collection-default headers.
- `src/components/collections/CollectionSettingsDialog.tsx:93-99` — repeats the same mapping in the obsolete settings surface.
- `src/components/request/SaveRequestButton.tsx:53-65` — repeats the same mapping for normal request save.
- `src/components/request/SaveToCollectionDialog.tsx:75-86` — repeats it for initial save.
- `src/lib/auto-save.ts:17-25` — maps the same rows but filters by `enabled` instead of by nonblank key.
- `src/lib/execute-request.ts:202-212` — correctly filters disabled headers for **execution**, demonstrating why execution and persistence policies must remain separate.

#### Shared intent

These paths turn editable frontend header rows into the persisted header DTO. The operation should discard unfinished blank-key rows while preserving the enabled/disabled state of named rows.

#### Material differences and current risk

- Manual request save, initial save, and collection-default save retain disabled named headers by persisting `enabled: false`.
- Auto-save removes every disabled header because it filters on `h.enabled`. The OpenCollection header model has a `disabled` field, so disabled headers are persisted data, not disposable UI state.
- Auto-save does not filter blank keys, so it can persist an enabled draft row that manual save would discard.
- Execution intentionally filters disabled rows. Reusing an execution helper for persistence would preserve the existing data-loss bug.
- `CollectionSettingsDialog` should still be retired under collections F1; it should not be migrated merely to make dead code share a helper.

#### Recommendation / survivor

Extract one pure persistence mapper such as `toPersistedHeaders(entries)` from the behavior shared by `CollectionOverviewTab` and `SaveRequestButton`: retain rows with a nonblank key, map only persisted fields, and preserve `enabled` exactly. Keep execution-time filtering in `execute-request.ts` separate and clearly named.

The long-term request serializer from request finding 1 should call this helper rather than own another header loop. Collection-default saving can use the same leaf mapper without sharing the full request serializer.

#### Staged remediation

1. Add tests for enabled, disabled, empty-key, whitespace-key, and variable-containing headers; decide explicitly whether key validity uses raw or trimmed emptiness.
2. Extract `toPersistedHeaders` without changing the current manual-save behavior.
3. Make the shared request payload mapper proposed by request finding 1 use it, covering both normal and initial save.
4. Migrate `CollectionOverviewTab`; do not invest in `CollectionSettingsDialog` before its planned removal.
5. Fix `auto-save` to use the persistence mapper and verify that disabled rows survive save/reload.
6. Retain a separately named execution mapper/filter that sends only enabled headers.

---

### X3 — Relative-time formatting spans audit, contracts, and Git and should be one option-driven utility

**Confidence:** High  
**Priority:** P2

#### Exact current references

- `src/components/audit/AuditEventRow.tsx:10-23` — seconds through years, supports future timestamps, returns the input on invalid dates.
- `src/components/contracts/ChangelogDrawer/ChangelogEntry.tsx:17-33` — minutes through years, past-only `ago` output, nominal `try/catch` fallback.
- `src/components/contracts/MiniChangelog.tsx:25-43` — `date-fns` compact relative output under 30 days, then absolute month/day.
- `src/components/contracts/ContractCard.tsx:123-172` — three related `date-fns` relative labels with caller-specific copy and suffix behavior.
- `src/components/contracts/ContractsTab.tsx:280-283` — another direct `formatDistanceToNow` label.
- `src/components/git/GitCommitLog.tsx:8-21` — manual minutes/hours/days/months formatter.
- `src/components/git/GitStashSection.tsx:26-37` — the same recent ranges, then absolute month/day.

#### Shared intent

All paths convert an instant into a concise human-readable age for list/card metadata. Contract finding 10 and Git finding F8 are two parts of the same catalog-wide formatter family; the audit formatter is a third implementation that neither report incorporated.

#### Material differences

- “Now” is rendered as `now`, `just now`, or `0s ago`.
- Audit uses rounding and handles future values with `from now`; most other manual implementations use floor and render future timestamps as `now`/`just now` because negative minutes satisfy `< 1`.
- Older values remain relative in audit and the changelog drawer, become absolute in stash and mini changelog, and become unbounded months in commit log.
- Invalid-date handling differs. `new Date(...).getTime()` normally yields `NaN` rather than throwing, so the contract `try/catch` implementations do not reliably produce their documented fallback.
- Some contract surfaces intentionally omit the suffix or embed the formatted value inside domain copy. Those caller-level differences should remain.

#### Recommendation / survivor

Build one pure formatter around the already-installed `date-fns` dependency with explicit options, for example:

- `style: 'compact' | 'long'`
- `suffix: boolean`
- `absoluteAfterDays?: number`
- `future: 'relative' | 'clamp-now'`
- injected/reference `now` for deterministic tests

Keep invalid-value and empty-state copy in callers. Preserve audit's explicit future handling as the semantic baseline; preserve the stash/mini-changelog absolute cutoff as an option rather than forcing it on every surface.

#### Staged remediation

1. Document expected outputs at seconds/minutes/hours/days/30-days/one-year boundaries, plus future and invalid timestamps.
2. Add fixed-clock tests for each required display mode.
3. Replace the two manual Git formatters and `ChangelogEntry.timeAgo` first; they have the closest contracts.
4. Migrate `MiniChangelog` and direct contract-card/tab calculations using explicit mode options while retaining caller copy.
5. Migrate audit with future support enabled.
6. Remove local unit calculations only after rendered strings are intentionally approved.

---

### X4 — Host-path basename extraction is repeated and several copies are Windows-unsafe

**Confidence:** High  
**Priority:** P1

#### Exact current references

- `src/components/import/ImportCollectionDialog.tsx:86-119` — three source pickers derive names with `path.split('/')`.
- `src/components/workspace/CreateWorkspaceDialog.tsx:46-54` — derives a picked folder name with `split(/[\\/]/)`, the strongest current cross-platform behavior.
- `src/components/panes/BreadcrumbBar.tsx:59-63` — `collectionBasename` claims an absolute path but recognizes only `/`.
- `src/components/collections/CollectionNode.tsx:151-170` — collection-change handling recognizes only `/` even though its comment says the event may contain a full filesystem path.
- Legacy contract attachment display also repeats slash-only extraction at `src/components/contract/ContractCard.tsx:144-165` and `src/components/contract/ContractForm.tsx:240-266`; these call sites should disappear if the singular contract stack is retired.

#### Shared intent

Each implementation extracts the last meaningful segment of a path received from a native picker, Tauri event, or backend DTO for display/comparison.

#### Material differences and current risk

- Tauri can return Windows paths containing `\`; the import dialog, breadcrumb helper, collection event listener, and legacy attachment display fail to extract their final segment in that case.
- `BreadcrumbBar` filters empty slash-separated segments and therefore handles a trailing `/`; the regex split in `CreateWorkspaceDialog` handles both separator styles but can return an empty final segment for a trailing separator.
- Collection-relative item paths elsewhere intentionally use `/` as a logical separator. Those operations should not be rewritten through a host-filesystem helper.
- Remaining finding R-1's “append one host path segment” is related path normalization but is not the same operation as basename extraction.

#### Recommendation / survivor

Add a small pure `hostPathBasename(path)` helper in a frontend path utility module. Start from `CreateWorkspaceDialog`'s dual-separator recognition, add trailing-separator handling, and avoid platform APIs that interpret paths according to the webview host rather than the path string's own style.

Keep a separate `appendHostPathSegment(base, segment)` helper for remaining finding R-1. Co-location is useful; conflating basename and joining into one ambiguous helper is not.

#### Staged remediation

1. Add tests for Unix and Windows paths, trailing separators, roots, empty input, and a plain collection name.
2. Extract `hostPathBasename` and migrate the three import pickers through the parameterized chooser proposed by remaining finding R-2.
3. Migrate `BreadcrumbBar.collectionBasename` and the `CollectionNode` event comparison.
4. Reuse it in any surviving attachment display only if the legacy contract stack remains after the contracts migration.
5. Leave collection-relative request/folder path splitting unchanged unless it independently needs a logical-path utility.

---

### X5 — Keyed clipboard success feedback is duplicated between response and OAuth surfaces

**Confidence:** High  
**Priority:** P2

#### Exact current references

- `src/components/response/ResponseHeadersTable.tsx:19-37` — keyed copied-row state, awaited clipboard write, 1000 ms guarded reset.
- `src/components/response/ResponseBodyViewer.tsx:181-223` — boolean copied state, awaited write, 1500 ms reset.
- `src/components/request/oauth2/OAuth2TokenDisplay.tsx:37-50` — keyed access/ID token state, unawaited write, 1500 ms reset.

Related one-shot actions that do not currently expose feedback:

- `src/components/request/oauth2/OAuth2ConfigSection.tsx:86-92`
- `src/components/contracts/ContractContextMenu.tsx:112-116`
- `src/components/git/GitCommitLog.tsx:66-69`

#### Shared intent

The three primary implementations copy text and temporarily identify the successful target so a copy icon/label can switch to a success state.

#### Material differences and current risk

- Body copy has one boolean target; headers and OAuth need keyed targets.
- Durations differ intentionally or accidentally (1000 versus 1500 ms).
- Response copies await and ignore rejection; OAuth marks success immediately even if the Clipboard API rejects.
- OAuth's unconditional timeout can clear feedback for a newer copy. The header guard protects different keys, but repeating the same key before the first timeout expires can still allow the old timeout to clear the newer success early.
- None of the local implementations explicitly cancels its timer on unmount.
- One-shot clipboard actions without visible feedback do not need to adopt a stateful hook solely for syntactic consistency.

#### Recommendation / survivor

Create a small `useClipboardFeedback<T>()` hook with keyed identity, configurable duration, rejection handling, one active/cancelled timer, and unmount cleanup. The keyed API in `ResponseHeadersTable` is the best behavioral starting point, but no current implementation fully handles repeated same-key copies and cleanup.

Keep payload selection and button rendering in callers. Do not make the hook impose toast policy unless product UX explicitly standardizes clipboard failures.

#### Staged remediation

1. Add fake-timer tests for success, rejection, second copy before expiry, repeated same-key copy, key changes, and unmount cleanup.
2. Implement the hook with a timer ref or generation token.
3. Migrate `ResponseHeadersTable`, then adapt `ResponseBodyViewer` with a constant key.
4. Migrate `OAuth2TokenDisplay` and ensure success appears only after a fulfilled write.
5. Evaluate one-shot copy controls separately; adopt a stateless `copyText` helper only if consistent error handling is desired.

---

### X6 — Request resize mechanics duplicate an existing shared resizable-panel primitive

**Confidence:** Medium-high  
**Priority:** P2

#### Exact current references

Existing shared implementation in the editor/UI and layout/panes batches:

- `src/components/ui/resizable.tsx:1-41` — project wrapper over `react-resizable-panels` group, panel, and accessible separator.
- `src/components/panes/PaneRenderer.tsx:14-31` — active horizontal/vertical use with size persistence into pane state.

Manual request implementations:

- `src/components/request/RequestPanel.tsx:229-303` — duplicate percentage-based horizontal and vertical keyboard/pointer handlers.
- `src/components/request/RequestPanel.tsx:1381-1415` and `src/components/request/RequestPanel.tsx:1437-1467` — two custom accessible separator renderings.
- `src/components/request/ScriptSnippetSidebar.tsx:104-206` — pixel width, pointer capture, keyboard support, and custom separator.
- `src/components/request/load-test/LoadTestTab.tsx:100-150` and `src/components/request/load-test/LoadTestTab.tsx:335-342` — persisted pixel sidebar with mouse-only, `aria-hidden` separator.
- `src/components/request/load-test/LiveDashboard.tsx:53-102` and `src/components/request/load-test/LiveDashboard.tsx:157-169` — persisted pixel log height with mouse-only, `aria-hidden` separator.

#### Shared intent

All implementations provide a draggable separator between adjacent panels, constrain sizes, update layout, and in some cases support keyboard interaction and persistence.

#### Material differences

- `RequestPanel` and `PaneRenderer` use proportional layouts; snippet/load-test sidebars use pixel dimensions.
- Load-test dimensions persist in `localStorage`; pane sizes persist through Zustand; other sizes are session-only.
- The custom request separators have distinct visual affordances and conditional mounting.
- Accessibility is inconsistent: request/snippet separators are keyboard-operable, while both load-test separators are hidden from assistive technology.
- The existing wrapper's actual support for required pixel bounds and storage semantics must be proven before replacing those implementations.

#### Recommendation / survivor

Revise request finding 4's recommendation: first attempt to use the existing `ResizablePanelGroup`/`ResizablePanel`/`ResizableHandle`, especially for `RequestPanel`'s percentage-based split. Do not create a second project-wide resize abstraction until the installed primitive is shown unable to preserve a consumer's constraints.

For pixel-sized persisted sidebars, either configure the existing library with tested bounds/storage or retain a narrow pointer-based hook only for that unsupported case. The fallback hook should use `ScriptSnippetSidebar`'s pointer capture and keyboard behavior, not the load-test mouse-only implementations.

#### Staged remediation

1. Add characterization tests for orientation, min/max, keyboard steps, conditional panel removal, and saved-size restoration.
2. Prototype `RequestPanel` with the existing primitive; verify both vertical and horizontal orientations and current 20/80 bounds.
3. If parity holds, migrate `RequestPanel` and preserve its visual handle through `ResizableHandle` styling/props.
4. Test whether the primitive can express pixel-constrained persisted sidebars/logs. Migrate load-test consumers only if exact restoration and bounds remain stable.
5. If pixel behavior cannot be preserved, extract one narrow pointer/keyboard/persistence hook for `ScriptSnippetSidebar`, `LoadTestTab`, and `LiveDashboard` rather than a competing general panel system.
6. Remove mouse-only separators or make them keyboard-accessible during migration.

---

## 2. Overlaps and relationships among batch findings

### A. Request serialization, collection settings, auth mapping, and header mapping are one layered persistence problem

The following findings overlap but should not be merged into one giant serializer:

- request finding 1: request-tab persistence payloads;
- collections F1: competing collection settings editors;
- new X1: persisted auth conversion;
- new X2: persisted header conversion.

The correct layering is:

1. shared leaf adapters for auth and header rows;
2. one request payload mapper owning request-only fields;
3. one collection-settings controller owning collection defaults/docs/variables;
4. separate execution-time resolution/filtering.

`CollectionSettingsDialog` is not a survivor and should not drive shared behavior. Conversely, a request payload helper should not be used to save collection defaults merely because both contain auth and headers.

### B. Contract relative-time finding 10 and Git F8 are the same catalog-wide family

They should be tracked as one remediation item, X3, extended to include `AuditEventRow`. Contract-local fallback copy remains local. The month/day-only formatter in contracts finding 7 is adjacent but separate: it formats date-only values, not elapsed time.

### C. Remaining R-4 and the OAuth token display are one clipboard-feedback family

Remaining R-4 already joins response body and header copy behavior. Request finding 6 discusses token-display duplication but does not normalize its copy timer with response UI. X5 is the cross-batch superset. The token disclosure/claims extraction from request finding 6 remains independently actionable.

### D. Remaining R-1/R-2 and X4 should share a path-utility home, not one function

- R-1: append a child segment while preserving host path style;
- R-2: choose/import a source and derive display metadata;
- X4: extract a host path basename.

`appendHostPathSegment` and `hostPathBasename` are complementary pure helpers. R-2's picker orchestration should consume the basename helper. Logical collection paths should remain slash-based and separate.

### E. Request resize finding 4 must account for the existing UI primitive

The request report recommends a new `useResizablePanel` plus separator primitive. X6 narrows that recommendation: use `src/components/ui/resizable.tsx` first, and create only a constrained fallback hook if pixel/persistence requirements cannot be represented. This avoids two competing project-wide resize systems.

### F. Variable/key-value row findings form a presentation family, but not one persistence model

Related findings:

- collections F3: collection/folder and environment variable tables;
- request finding 7: assertions and response-variable indexed rows;
- request finding 8: OAuth additional parameters and `KeyValueEditor`.

Verified representative references:

- `src/components/collections/CollectionVariablesEditor.tsx:20-40`, `src/components/collections/CollectionVariablesEditor.tsx:53-140`
- `src/components/environments/VariableTable.tsx:38-42`, `src/components/environments/VariableTable.tsx:62-166`
- `src/components/request/KeyValueEditor.tsx:31-47`, `src/components/request/KeyValueEditor.tsx:49-101`
- `src/components/request/oauth2/OAuth2AdditionalParams.tsx:35-52`, `src/components/request/oauth2/OAuth2AdditionalParams.tsx:54-123`

Normalize the recommendations as follows:

1. extract/test tiny immutable row operations only where they remove repeated state code;
2. let `KeyValueEditor` and OAuth parameters share a key/value row shell with an extension cell;
3. let collection/environment editors share stateless enabled/key/value/secret presentation only if their typed models remain explicit;
4. do not route assertions, collection variables, environment variables, and OAuth parameters through one universal schema/render-prop editor.

Collection `initialValue`, secret persistence, environment duplicate policy, OAuth `sendIn`, assertion operators, and response-variable test state are meaningful boundaries.

### G. Layout F2 is already a genuine cross-batch finding and should not be counted again

`collectLeafGroupIds` at `src/components/panes/TabBar.tsx:35-39` and `src/components/collections/RequestNode.tsx:51-55` is the clearest exact cross-boundary duplicate in the reports. Keep layout F2's recommendation: add the utility to `src/lib/pane-utils.ts` and remove both local copies. This review does not assign it a new X-number because it was already explicitly verified as a two-folder group.

### H. Collection tree aggregation and breadcrumb folder resolution are related traversals, not one algorithm

- collections F4 walks every recursive item for counts/tags/methods/lists;
- layout F4 resolves a specific folder ancestry and opens a selected request.

They can live near shared collection-tree utilities, but should remain separate primitives: a depth-first visitor does not replace deterministic path resolution, and path resolution should not flatten summaries/folder ancestry.

### I. Parallel legacy feature surfaces share a deletion pattern, not a reusable abstraction

- collections F1: `CollectionSettingsDialog` versus `CollectionOverviewTab`;
- contracts findings 1-5: singular versus plural contract stacks;
- request findings 2-3: load-test dialog versus tab, and request-variable dialog versus panel.

Each needs characterization, migration of genuinely missing behavior, call-site verification, and deletion. Do not build a generic “legacy-to-modern feature adapter”; the resource models and missing capabilities differ.

### J. Inline naming findings should share interaction tests before sharing code

Collections F5 covers rename behavior across collection/folder/request/environment resources. Layout F6 covers collection/global environment creation sections. Both contain trim, Enter/Escape, blur, and in-flight patterns, but create and rename failure/selection semantics differ. A tiny tested interaction hook may eventually serve both; the batch recommendations should first align behavioral contracts rather than immediately merge callbacks.

---

## 3. False positives intentionally rejected

### Same function name: `formatTime` in history and console

- `src/components/history/HistoryPanel.tsx:27-36` renders a locale clock with hour/minute and an empty fallback.
- `src/components/layout/ConsolePanel.tsx:24-32` renders a forced 24-hour clock with seconds and no fallback.

The shared name is not enough. The precision, locale policy, and failure contract are intentionally different. If a product-wide timestamp policy is later adopted they could consume it, but there is no current behavior-preserving survivor.

### One universal collection-tree traversal

Recursive overview aggregation, pane-tree traversal, breadcrumb path resolution, and collection deletion's “visit all leaves and close matches” are different algorithms over different tree types. Share narrowly named visitors/searches only within the correct domain; do not create a generic recursive-tree framework.

### One serializer for request payloads and collection settings

Requests and collection defaults share leaf auth/header shapes but persist different resources with different required/optional fields. X1/X2 deliberately stop at leaf adapters. A universal “save API resource” serializer would obscure OpenCollection inheritance and increase data-loss risk.

### Reusing execution auth/header conversion for persistence

`src/lib/execute-request.ts:80-115` resolves OAuth to bearer and drops unsupported AWS for dispatch; `src/lib/execute-request.ts:210-212` removes disabled headers. Those are correct execution concerns and incorrect persistence semantics. Similar field shapes do not make these persistence survivors.

### All editable row tables as one generic component

The catalog contains many enabled/key/value/add/remove tables, but identity, columns, validation, persistence, and side effects vary. Share row mechanics and presentation in layers; reject a universal table covering variables, assertions, OAuth parameters, headers, and response actions.

### Bare clipboard writes and feedback-producing copy controls

Contract-link, commit-hash, and callback-URL actions currently perform one-shot copies with no copied-state UI. They are not automatically members of X5. A stateful feedback hook is justified only where feedback exists or is explicitly added as a UX change.

### Collection-summary loading and workspace resolution

Layout F8 concerns freshness/invalidation of collection summaries. Remaining R-3 concerns selecting a workspace object by active or explicit ID. Both belong near query infrastructure, but they do not fetch the same entity or have the same cache semantics.

### Empty states, dialogs, shadcn wrappers, and illustrations across folders

Catalog-wide visual resemblance is not semantic duplication. The batch reports correctly reject generic empty-state/modal factories, collapsing Radix wrapper families, and parameterizing the rocket illustration library. Keep those rejections.

### Status badges across Git, contracts, and collection navigation

`GitStatusBadge`, `ContractStatusChip`, `ChangeChip`, and `ContractBadge` all render compact state, but their state machines and interactions differ. Only the exact/local `KindChip` versus `ChangeChip` mapping from contracts finding 8 should be consolidated.

### Date-only formatting versus relative-time formatting

Contracts finding 7's month/day formatter, OAuth's absolute token timestamp, and X3's elapsed-time formatter answer different questions. They may share low-level date validation later, but should not be forced through one output API.

---

## 4. Concise prioritized remediation sequence

1. **Protect persisted data first.** Add characterization tests for request/collection auth and headers; implement X1 and X2; then complete request finding 1. Verify manual save, initial save, and auto-save produce compatible payloads and preserve AWS/OAuth auth plus disabled headers.
2. **Remove known destructive/dead settings behavior.** Complete collections F1 and F2: retire `CollectionSettingsDialog` after preservation coverage and centralize environment duplicate-key policy.
3. **Converge active feature ownership.** Choose the modern contract stack, load-test tab, and request-variable panel; port only approved missing behavior and remove legacy surfaces after call-site checks.
4. **Land small canonical utilities.** Complete layout F1/F2 pane searches, X4 host-path helpers, collections F4's collection walker, and request/collection request-creation mapping with focused tests.
5. **Consolidate parser and persistence leaf logic before UI shells.** Complete editor F1's variable scanner, editor F2's snippet registry, contract version/party/date helpers, and Git diff adapters.
6. **Normalize shared feedback/formatting.** Implement X3 relative time, X5 clipboard feedback, Git error banners, and variable-source metadata.
7. **Consolidate interaction mechanics cautiously.** Reuse the existing resizable primitive per X6, then address inline rename, environment-switcher presentation, menu action descriptors, and row-editor shells while retaining domain adapters.
8. **Finish lower-risk local repetition.** Collapse contract group/more-actions rendering, body-editor duplicate branches, Git dialog mounting, and optional decoration-plugin plumbing.

## Validation guidance for future remediation

This review changes documentation only, so no application build or test run is required for the report itself. Future implementation should validate in risk order:

- persistence adapters: pure table-driven tests plus component/integration tests for manual, initial, and auto-save payloads;
- path helpers: Unix/Windows unit cases;
- relative time and clipboard hooks: fixed-clock/fake-timer tests;
- resize migrations: pointer, keyboard, min/max, orientation, and persistence interaction tests;
- focused component suites, followed by `yarn tsc --noEmit` and `yarn check`.
