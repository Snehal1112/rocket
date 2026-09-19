# Semantic Duplication Audit: `src/components/request`

## Scope and method

- **Scope:** production code under `src/components/request` only, including `load-test/`, `oauth2/`, and `runner/`.
- **Inventory inspected:** 43 production files, 7,231 lines, and 62 callable units indexed by `docs/reports/component-duplication/function-catalog.json`.
- **Method:** the catalog was used only as an index. Every production file in scope was read in full before classifying candidates. Repository-wide searches were used only to verify call sites for in-scope components.
- **Tests:** test-only repetition was excluded. The existing `LoadTestDialog` tests corroborate that the dialog is active production behavior, but the tests are not themselves reported as duplication.
- **Confidence meaning:** confidence measures how certain the code has duplicate semantic intent, not how urgent or easy consolidation is.

## Executive summary

| # | Confidence | Candidate | Classification |
|---|---|---|---|
| 1 | HIGH | Request-tab persistence payload construction | Consolidate; divergent copies already serialize different data |
| 2 | HIGH | Legacy load-test dialog versus load-test tab | Consolidate/remove one complete feature surface |
| 3 | HIGH | Request-variable loading/editing lifecycle | Consolidate active double-loading; remove unreferenced legacy dialog |
| 4 | HIGH | Resizable-panel state and drag mechanics | Consolidate into a configurable hook/separator primitive |
| 5 | HIGH | OAuth token-response application and JWT decoding | Consolidate duplicated async post-processing |
| 6 | HIGH | OAuth access-token and ID-token disclosures | Consolidate repeated token/claim presentation primitives |
| 7 | MEDIUM | Assertion and post-response-variable row editors | Share row-list mechanics, not domain-specific columns |
| 8 | MEDIUM | OAuth additional-parameter list versus `KeyValueEditor` | Extend/share a row editor carefully |
| 9 | HIGH | Form-data and form-urlencoded body branches | Collapse exact duplicate rendering in place |

The highest-risk duplication is finding 1: saving an existing request and initially saving an unsourced tab use separate serializers with materially different field coverage. Findings 2 and 3 also show likely legacy paths coexisting with newer replacements.

---

## Findings

### 1. Request-tab persistence payload construction has divergent copies

**Confidence: HIGH**

**References**

- `src/components/request/SaveRequestButton.tsx:16-50` — `authForSave` maps all request auth variants.
- `src/components/request/SaveRequestButton.tsx:53-81` — `buildPayloadFromTab` serializes a sourced `RequestTab` for `saveRequest`.
- `src/components/request/SaveRequestButton.tsx:88-99` — existing-request save path.
- `src/components/request/SaveToCollectionDialog.tsx:75-86` — independently constructs a payload from the same `RequestTab` shape.
- `src/components/request/SaveToCollectionDialog.tsx:88-96` — initial save-to-collection path.

**Semantic intent**

Both paths convert the current `RequestTab` state into the persisted API request passed to `saveRequest`. The destination selection and post-save UI behavior differ, but request serialization should have one semantic definition.

**Important differences and edge cases**

- `SaveRequestButton` preserves mapped Basic, Bearer, API key, OAuth 2.0, and AWS SigV4 auth; `SaveToCollectionDialog` always writes `{ authType: 'none' }` (`SaveToCollectionDialog.tsx:84`). Initial save can therefore silently discard configured auth.
- `SaveRequestButton` includes tags, settings, docs, pre-request script, post-response script, tests, and assertions (`SaveRequestButton.tsx:66-80`). `SaveToCollectionDialog` omits all of them.
- Neither serializer currently includes `request.actions`; that is a separate completeness issue to verify against the API type/spec before consolidation, not a reason to preserve two serializers.
- Initial save must override the persisted display name and filename using dialog values, while normal save uses `tab.title` and the existing source path.
- `inherit` currently maps to `none` in `authForSave` (`SaveRequestButton.tsx:18-20`). Preserve existing behavior initially, then verify whether persistence should retain inheritance as a distinct value.

**Recommended survivor/shared abstraction**

Extract `authForSave` and `buildPayloadFromTab` into a single request-persistence mapper, for example `buildRequestSavePayload(tab, overrides?)`. Treat the `SaveRequestButton` implementation as the survivor because it has the broader field coverage. Allow explicit overrides for `name` and `fileName`; destination selection should remain in `SaveToCollectionDialog`.

**Staged remediation**

1. Add focused mapper tests covering every auth variant and all optional request fields.
2. Extract the current `SaveRequestButton` mapper without changing behavior.
3. Change `SaveRequestButton` to call the shared mapper.
4. Change `SaveToCollectionDialog` to call it with `{ name: trimmedName, fileName: fsName }`.
5. Add an integration/component test proving initial save preserves auth, settings, docs, scripts, assertions, and tags.
6. Separately verify whether `actions` and `inherit` belong in the persisted payload, then update the single mapper once.

---

### 2. Two complete load-test experiences are exposed for the same request

**Confidence: HIGH**

**References**

- `src/components/request/LoadTestDialog.tsx:33-78` — local simple-load-test state and direct execution.
- `src/components/request/LoadTestDialog.tsx:98-143` — concurrency, total-request, and interval configuration.
- `src/components/request/LoadTestDialog.tsx:158-191` — local result summary.
- `src/components/request/load-test/LoadTestTab.tsx:79-98` — store-backed simple/advanced load-test state and execution controls.
- `src/components/request/load-test/LoadTestTab.tsx:189-228` — the tab's simple-mode configuration, covering the same core settings.
- `src/components/request/load-test/LoadTestTab.tsx:319-331` — run/stop/export actions.
- `src/components/request/RequestPanel.tsx:964-973` — toolbar button opens the dialog.
- `src/components/request/RequestPanel.tsx:1018-1021` — a separate “Load test” section renders the tab.
- `src/components/request/RequestPanel.tsx:1337-1344` — dialog remains mounted and wired.

**Semantic intent**

Both surfaces configure and run a load test for the current request and present its result. The tab is a superset: it includes simple mode, advanced phased mode, live results, stopping, and export.

**Important differences and edge cases**

- The dialog owns transient local state and calls `resolveRequestFields`/`runLoadTest` directly; the tab delegates execution to `useLoadTestStore`.
- The dialog expresses delay in seconds and converts to milliseconds (`LoadTestDialog.tsx:69`); the tab edits milliseconds directly (`LoadTestTab.tsx:210-216`). Migration must choose one user-facing unit or label the distinction explicitly.
- The dialog exposes a fixed concurrency selector and no stop action. The tab accepts integer input, supports a duration cap, status threshold, advanced phases, stop, and export.
- The dialog's compact final summary includes min/average/max and p50/p95/p99. The tab's dashboard distributes result information across `StatBar`, charts, and request log; feature parity should be checked before deleting the dialog.
- `LoadTestDialog` has dedicated tests. Those tests should be migrated to the surviving simple-mode tab/store flow rather than deleted without replacement.

**Recommended survivor/shared abstraction**

Use `load-test/LoadTestTab.tsx` plus the load-test store as the survivor. The toolbar `Zap` action should navigate to/activate the existing `load-test` section instead of opening an independent dialog. If a compact quick-run UX is required, build it as a thin view over the same store/configuration rather than retaining direct execution logic.

**Staged remediation**

1. Document a feature-parity checklist: units, final percentile summary, failure breakdown, validation, cancellation, and error display.
2. Add tests for the tab's simple mode corresponding to the valuable `LoadTestDialog` tests.
3. Change the toolbar action to select the `load-test` section.
4. Remove `showLoadTest`, the dialog mounting, and then `LoadTestDialog.tsx` once parity is covered.
5. Remove or migrate dialog-specific tests and ensure there is only one execution/configuration state owner.

---

### 3. Request-variable data is loaded in multiple active places, plus an unreferenced legacy dialog

**Confidence: HIGH**

**References**

- `src/components/request/RequestPanel.tsx:204-227` — loads request and folder variables to build the editor resolution context.
- `src/components/request/RequestPanel.tsx:454-483` — consumes `requestVariables` in `buildScopedContext`.
- `src/components/request/RequestPanel.tsx:1107-1113` — mounts `RequestVariablesPanel` and only receives values after saves.
- `src/components/request/RequestVariablesPanel.tsx:33-61` — independently loads the same request variables.
- `src/components/request/RequestVariablesPanel.tsx:63-80` — debounced save and cleanup behavior.
- `src/components/request/RequestVariablesDialog.tsx:35-56` — third load/save implementation.
- `src/components/request/RequestVariablesDialog.tsx:74-82` and `RequestVariablesPanel.tsx:86-94` — both render `CollectionVariablesEditor` with `showDescription={false}`.

**Call-site verification**

A source search found no production reference to `RequestVariablesDialog`; all occurrences are within its own file. `RequestVariablesPanel` is the active editor.

**Semantic intent**

All three code paths obtain request-scoped variables for a collection/request path. The panel and dialog also edit the same model with the same editor. In the active UI, `RequestPanel` and `RequestVariablesPanel` separately fetch the same request-variable list when a sourced request is opened.

**Important differences and edge cases**

- `RequestPanel` needs request variables even when the Variables section is not active because they participate in URL/header/body/auth resolution.
- `RequestVariablesPanel` implements 400 ms auto-save, stale-request protection, and an unmount flush; these semantics are more complex than the dialog's explicit Save/Cancel flow.
- The active parent only updates its context through `onSaved`, so the child currently needs to report committed values. Passing parent-owned values down would remove duplicate fetching but must not make the resolution context advertise unsaved values unless that is intentional.
- The dialog reports counts and closes after explicit save, but it is unreferenced. Dynamic import usage was not found under `src`.
- The panel cleanup only flushes when a timer exists (`RequestVariablesPanel.tsx:47-59`). Preserve behavior during extraction first; assess races separately.

**Recommended survivor/shared abstraction**

Keep `RequestVariablesPanel` as the UI survivor and delete `RequestVariablesDialog` after confirming no external/plugin entry point imports it. Introduce a request-variable data hook/controller owned by `RequestPanel`, or pass loaded variables into the panel with an `onCommitted` callback. The key goal is one fetch owner and one save lifecycle.

**Staged remediation**

1. Add tests around load, debounced save, path changes, and unmount flush.
2. Extract the panel's persistence lifecycle into `useRequestVariables(collection, requestPath)` or lift it into `RequestPanel`.
3. Feed the same committed variable state to both `buildScopedContext` and `CollectionVariablesEditor`.
4. Remove the duplicate `getRequestVariables` call from either parent or child.
5. Delete the unreferenced dialog and any obsolete exports after a final import search.

---

### 4. Resizable-panel mechanics are reimplemented five times

**Confidence: HIGH**

**References**

- `src/components/request/RequestPanel.tsx:229-303` — horizontal and vertical request/response keyboard and pointer handlers.
- `src/components/request/RequestPanel.tsx:1381-1410` and `RequestPanel.tsx:1437-1465` — two separator renderings.
- `src/components/request/ScriptSnippetSidebar.tsx:109-174` — clamping, pointer capture, dragging state, and keyboard resizing.
- `src/components/request/ScriptSnippetSidebar.tsx:176-206` — accessible vertical separator.
- `src/components/request/load-test/LoadTestTab.tsx:100-150` — persisted horizontal-axis mouse drag for sidebar width.
- `src/components/request/load-test/LoadTestTab.tsx:335-342` — load-test sidebar separator.
- `src/components/request/load-test/LiveDashboard.tsx:53-102` — persisted vertical-axis mouse drag for log height.
- `src/components/request/load-test/LiveDashboard.tsx:157-169` — dashboard log separator and panel.

**Semantic intent**

Each implementation lets the user drag a separator to resize an adjacent panel, clamps the value to bounds, tracks drag lifecycle, and removes global listeners. Three implementations also expose or partly expose keyboard/accessibility behavior.

**Important differences and edge cases**

- Units differ: `RequestPanel` uses percentages; the sidebar/log implementations use pixels.
- Axis and sign differ depending on whether the resizable panel is before/after or above/below the separator.
- Persistence differs: load-test sidebar/log use `localStorage`; request/response and snippets are session-only.
- Event models differ: `RequestPanel` and snippets use pointer events; load-test components use mouse events. Pointer events and pointer capture are generally the stronger survivor behavior.
- Accessibility differs materially: `RequestPanel` and snippets expose `role="separator"`, orientation, ARIA values, and keyboard handling; load-test separators are `aria-hidden` and mouse-only.
- `RequestPanel` uses one `isDragging` flag for both orientations; a shared hook should keep independent instances if simultaneous/nested resize state matters.

**Recommended survivor/shared abstraction**

Extract a configurable `useResizablePanel` hook backed by pointer events, with options for axis, min/max, initial value, unit, direction, step, and optional persistence key. Pair it with a small `ResizableSeparator` primitive that standardizes ARIA and keyboard behavior. Use the `ScriptSnippetSidebar` pointer-capture/accessibility behavior as the implementation baseline, while retaining `RequestPanel`'s percentage calculations as a supported mode.

**Staged remediation**

1. Unit-test clamping, direction, keyboard steps, persistence restoration, and listener cleanup in a hook.
2. Migrate one low-risk pixel-based consumer (`LoadTestTab` or `LiveDashboard`).
3. Migrate `ScriptSnippetSidebar`, preserving pointer capture and Home/End behavior.
4. Add percentage support and migrate the two `RequestPanel` separators.
5. Remove component-local global-listener/ref machinery only after interaction tests pass.

---

### 5. OAuth token acquisition and refresh duplicate token-response application

**Confidence: HIGH**

**References**

- `src/components/request/oauth2/OAuth2AuthEditor.tsx:79-161` — acquire-token request, state patch, access-token decode, and ID-token decode.
- `src/components/request/oauth2/OAuth2AuthEditor.tsx:165-216` — refresh-token request followed by nearly the same state patch and decode sequence.
- Specifically, result normalization is repeated at `OAuth2AuthEditor.tsx:126-137` and `OAuth2AuthEditor.tsx:187-196`.
- Best-effort access-token and ID-token decoding is repeated at `OAuth2AuthEditor.tsx:142-155` and `OAuth2AuthEditor.tsx:197-209`.

**Semantic intent**

After either OAuth operation returns a token response, update the same OAuth state fields and best-effort decode access and ID token claims.

**Important differences and edge cases**

- Initial acquisition clears absent refresh/ID token values; refresh preserves the old refresh token and ID token when the response omits replacements.
- Initial acquisition resets `forceReauth`; refresh does not use that flag.
- Initial acquisition scrolls the token display into view; refresh does not.
- Both paths set acquisition time and clear access-token claims before decoding.
- Decode failures are intentionally non-fatal. A shared helper must preserve that behavior and avoid turning opaque access tokens into errors.
- Patching is incremental through `patchOAuth2Ref`; combining patches must not reintroduce stale closure behavior.

**Recommended survivor/shared abstraction**

Extract an async helper local to the OAuth editor module, such as `applyTokenResponse(result, { preserveRefreshToken, preserveIdToken })`, or a `useOAuthTokenResponse` hook if UI side effects remain coupled. The helper should normalize response fields and perform best-effort claim decoding; callers retain operation-specific request construction, `forceReauth`, scrolling, and error handling.

**Staged remediation**

1. Add tests for omitted refresh/ID tokens on both acquisition and refresh.
2. Extract pure response-to-patch normalization first.
3. Extract best-effort claim decoding into a second helper returning claim patches.
4. Rewire both handlers and compare resulting patches in tests.
5. Keep operation-specific side effects in the handlers until behavior parity is established.

---

### 6. OAuth access-token and ID-token displays substantially repeat disclosure and claims UI

**Confidence: HIGH**

**References**

- `src/components/request/oauth2/OAuth2TokenDisplay.tsx:37-79` — parallel open/raw/copy state and token-change effects.
- `src/components/request/oauth2/OAuth2TokenDisplay.tsx:87-209` — access-token disclosure, copy control, claim list, and raw payload toggle.
- `src/components/request/oauth2/OAuth2TokenDisplay.tsx:212-315` — ID-token disclosure, copy control, claim list, and raw payload toggle.
- Claim rows for subject, issuer, audience, expiry, issued-at, and algorithm are duplicated at `OAuth2TokenDisplay.tsx:139-189` and `OAuth2TokenDisplay.tsx:232-272`.
- Raw-payload controls are duplicated at `OAuth2TokenDisplay.tsx:191-204` and `OAuth2TokenDisplay.tsx:274-287`.

**Semantic intent**

Both branches render a collapsible token section, synchronize expansion when a new token arrives, show decoded JWT claims, optionally reveal raw payload, and provide token-copy behavior.

**Important differences and edge cases**

- Access token displays an expiry badge based on acquisition metadata; ID token does not.
- Access-token claims include `scope`; ID-token claims currently do not render it.
- Access token always renders its raw token text before claims. ID token renders the raw token/copy row only when decoded claims are unavailable.
- The two sections have slightly different border/rounding rules depending on whether both exist.
- Copy feedback is shared through one discriminated state; extraction should preserve independent labels while avoiding timer races.

**Recommended survivor/shared abstraction**

Extract two focused primitives rather than one over-configured component:

1. `JwtClaimsList` for the shared claims and raw-payload toggle, with an option to include scope.
2. `TokenDisclosure` for controlled header/open state and optional token value/copy/expiry content.

Keep `OAuth2TokenDisplay` as the orchestrator that supplies the access-versus-ID differences.

**Staged remediation**

1. Add rendering tests for access-only, ID-only, both tokens, opaque tokens, decoded tokens, and expiry.
2. Extract `JwtClaimsList` and preserve field ordering.
3. Extract the repeated disclosure shell.
4. Recheck copy feedback and auto-expand-on-new-token behavior.
5. Only then simplify the parallel local state if a small keyed state model remains clearer.

---

### 7. Assertions and post-response variables repeat editable-row table mechanics

**Confidence: MEDIUM**

**References**

- `src/components/request/AssertionsTab.tsx:102-114` — index-based update/remove/add operations.
- `src/components/request/AssertionsTab.tsx:116-128` — empty state and add action.
- `src/components/request/AssertionsTab.tsx:130-223` — enabled switch, editable row, delete control, and footer add action.
- `src/components/request/VarsTab.tsx:67-92` — equivalent update/remove/add operations.
- `src/components/request/VarsTab.tsx:112-124` — parallel empty state and add action.
- `src/components/request/VarsTab.tsx:126-235` — parallel header/body/footer table structure, switch, delete control, and add action.

**Semantic intent**

Both components manage an ordered array of rows without stable IDs: patch by index, remove by index, append a default row, toggle `disabled`, render an empty state, and expose an add action at the bottom.

**Important differences and edge cases**

- Assertion operator changes have unary-operator-specific value semantics (`AssertionsTab.tsx:143-203`).
- Variable actions have nested `selector`/`variable` patches and response-backed test execution (`VarsTab.tsx:76-108`, `VarsTab.tsx:194-223`).
- Removing a variable action must reindex test results and errors (`VarsTab.tsx:42-57`, `VarsTab.tsx:84-87`); a generic remove helper cannot hide this side effect.
- Column layouts and controls are domain-specific. A generic full table component would likely become render-prop-heavy and obscure behavior.

**Recommended survivor/shared abstraction**

Do not merge the domain components. Share only a narrow `useIndexedRows` helper (or pure `updateAt`/`removeAt` utilities) and, if visual drift becomes a maintenance issue, small `EditableRowsEmptyState`/footer primitives. Keep row rendering and variable test state local.

**Staged remediation**

1. Add unit tests for immutable update/remove helpers and the variable-result reindex behavior.
2. Extract only array patch/remove/append operations.
3. Migrate both tabs while preserving their domain-specific side effects.
4. Evaluate visual primitives separately; stop if the abstraction requires domain conditionals.

---

### 8. OAuth additional parameters reimplement most of `KeyValueEditor`

**Confidence: MEDIUM**

**References**

- `src/components/request/KeyValueEditor.tsx:21-47` — update/remove/add CRUD for key/value/enabled rows.
- `src/components/request/KeyValueEditor.tsx:49-100` — checkbox, key input, variable-aware value editor, remove button, and add button.
- `src/components/request/oauth2/OAuth2AdditionalParams.tsx:35-52` — equivalent CRUD for OAuth parameter rows.
- `src/components/request/oauth2/OAuth2AdditionalParams.tsx:54-123` — equivalent row UI plus a `sendIn` selector.

**Semantic intent**

Both edit repeatable enabled key/value pairs with a variable-aware value, removal, and append behavior. OAuth parameters add one placement column.

**Important differences and edge cases**

- `KeyValueEntry` has a stable `id`; `OAuth2AdditionalParam` does not and is keyed/updated by index.
- OAuth rows require `sendIn: 'queryparams' | 'body'` and default to `body`.
- Standard `KeyValueEditor` has configurable labels/placeholders but no extension-column API.
- Forcing OAuth parameters into `KeyValueEntry` would create a lossy or misleading model. The abstraction should support identity and an extra cell rather than cast types.

**Recommended survivor/shared abstraction**

Use `KeyValueEditor` as the visual/behavioral survivor, but extract or extend its row core generically: configurable `getKey`, `createRow`, and `renderExtraCell` are sufficient. An alternative is a private `EditableKeyValueRows<T>` used by both wrappers, leaving public props unchanged.

**Staged remediation**

1. Add tests for OAuth placement changes and standard stable-ID behavior.
2. Extract the shared row shell without changing either public component.
3. Supply ID-based identity for normal entries and index/future generated IDs for OAuth rows.
4. Render `sendIn` through an extension cell.
5. Keep `KeyValueEditor` and `OAuth2AdditionalParams` as domain-facing wrappers.

---

### 9. Form-data and form-urlencoded body modes render the exact same editor branch

**Confidence: HIGH**

**References**

- `src/components/request/BodyEditor.tsx:83-93` — `formdata` branch.
- `src/components/request/BodyEditor.tsx:95-105` — `formurlencoded` branch.

**Semantic intent**

Both modes render `KeyValueEditor` over `body.formData` with identical callbacks, labels, variable context, and navigation behavior.

**Important differences and edge cases**

- The persisted/wire body type is different even though the current editor state and UI are the same. Consolidate rendering only; do not merge body modes.
- If multipart form data later supports file-valued fields, the UI will need to diverge again. A combined condition remains easy to split when that capability is introduced.

**Recommended survivor/shared abstraction**

No new component is needed. Replace the two branches with one condition covering both modes and keep `setFormData` as the shared callback.

**Staged remediation**

1. Add/retain a rendering test for each mode.
2. Collapse the conditional branches.
3. Leave serialization and mode selection untouched.

---

## Legitimate parallel UI and non-findings

### Top-level request editors and dialogs

- `HeadersEditor` and `QueryParamsEditor` are intentionally thin domain wrappers around the existing `KeyValueEditor` (`HeadersEditor.tsx:12-28`, `QueryParamsEditor.tsx:14-40`). This is successful reuse, not harmful duplication.
- `PathParamsPanel` resembles `KeyValueEditor`, but its keys are URL-derived and immutable and rows cannot be added/removed (`PathParamsPanel.tsx:17-20`, `PathParamsPanel.tsx:57-80`). Keeping a separate read-only panel is credible domain-specific UI.
- `CreateRequestDialog` and `SaveToCollectionDialog` both derive `fsName` and the “Saved as” hint (`CreateRequestDialog.tsx:58-62`, `SaveToCollectionDialog.tsx:52-55`), but both already delegate filename rules to `sanitizeFilename`. The remaining three-line composition is too small to justify a hook. Their request-persistence overlap is covered by finding 1.
- `RequestDocsPanel` has no credible duplicate within this folder. Its edit/preview and source-required behavior is request-specific.
- `RocketTabBar`, `ScriptsTab`, and `ScriptSnippetSidebar` have no duplicate-intent peer beyond the generic resize mechanics captured in finding 4.

### Load-test subarea

- `ConcurrencyChart`, `ErrorRateChart`, `LatencyChart`, `ThroughputChart`, and `HistogramChart` repeat Recharts framing and tiny style constants, but they visualize different series, chart types, axes, domains, legends, and target overlays. They are legitimate parallel chart components. A shared `ChartFrame` or exported style constants would be cosmetic, not a meaningful semantic consolidation.
- `LoadTestDialog.Stat` (`LoadTestDialog.tsx:208-223`) and `load-test/StatBar.tsx:10-17` both render label/value metrics, but their container styling and layouts differ. This low-value duplication disappears naturally if finding 2 removes the legacy dialog; do not create a shared abstraction first.
- `PhaseBuilder` repeats basic update/remove/append array operations seen in other row editors (`PhaseBuilder.tsx:38-52`), but drag ordering, generated row identities, phase target unions, and compact card layout make it a legitimate specialized editor.

### OAuth 2.0 subarea

- Repeated `Label` + `SingleLineEditor` markup across `OAuth2ConfigSection`, `OAuth2TokenSection`, and `OAuth2AdvancedSection` is declarative form composition with different visibility rules, secret handling, layouts, and semantics. A schema-driven field renderer would add indirection without eliminating meaningful logic.
- `OAuth2SectionHeader` already centralizes the repeated section-heading presentation. No further credible heading duplication remains.
- The two collapsible section headers in `OAuth2AuthEditor.tsx:324-392` are visually parallel but control different conditional sections and are short enough that extraction is optional; they are not a primary consolidation candidate.

### Runner subarea

- **No credible semantic duplicate found.** `RunnerRequestList` is an inclusion selector, while `RunnerResultsList` is an expandable execution-result view. They share request method/name presentation and the same empty-state sentence (`RunnerRequestList.tsx:10-15`, `RunnerResultsList.tsx:88-95`), but their interaction, status, detail, and layout semantics are materially different. A shared row component would likely be more complex than the repeated markup.
- `RunnerSummaryHeader` has no duplicate-intent function or component in this folder.

### Test-only code

- **No test-only duplication is reported.** Test helpers and fixtures were excluded as requested. Existing tests were considered only as migration coverage signals, especially for the legacy load-test dialog.

## Suggested remediation order

1. **Unify request serialization** (finding 1) to prevent data loss on initial save.
2. **Choose one load-test surface** (finding 2) and migrate coverage before deletion.
3. **Unify request-variable ownership** and remove the dead dialog (finding 3).
4. **Extract OAuth token processing** (finding 5), then token presentation (finding 6).
5. **Introduce reusable resize behavior** incrementally (finding 4).
6. Apply the small exact body-branch cleanup (finding 9).
7. Consider the medium-confidence row-editor abstractions (findings 7 and 8) only with focused tests and stop if the APIs become overly generic.

## Overall folder assessment

The folder generally uses domain wrappers well (`KeyValueEditor`, OAuth section components, load-test chart components). The credible duplication is concentrated in **parallel feature generations** (load-test dialog versus tab; request-variable dialog versus panel), **cross-cutting interaction mechanics** (resizing), and **state-to-persistence/token-response transformations**. Consolidating those areas would reduce behavioral drift more than broad visual abstraction work. No production code was modified as part of this audit.
