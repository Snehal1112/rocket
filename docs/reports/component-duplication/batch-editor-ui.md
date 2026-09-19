# Semantic Duplication Audit: Editor and UI Components

## Scope and method

Audited:

- `src/components/editor/**`
- `src/components/ui/**`

`docs/reports/component-duplication/function-catalog.json` was used only as an index. Every authored production implementation in the two trees was read in full, including the complete Monaco and CodeMirror configuration files and the complete snippet/type registry in `rok-types.ts`. Relevant tests and the existing shared variable utilities in `src/lib/text-variables.ts` and `src/lib/url-variables.ts` were inspected to verify behavior and identify safe survivors. `chai-types.d.ts.txt` was sampled at both ends and treated as a vendored declaration payload rather than authored executable logic.

Test-only repetition is not reported as production duplication. In particular, the repeated `makeContext` helpers in editor tests are small fixture builders and do not justify a production abstraction.

Confidence below measures certainty that two or more implementations have the same semantic responsibility. Priority reflects remediation value and risk, not just similarity.

## Executive summary

| ID | Finding | Confidence | Priority | Disposition |
|---|---|---:|---:|---|
| F1 | Variable-token recognition and range calculation are implemented repeatedly | High | High | Consolidate around the existing text-variable parser |
| F2 | Script API-reference snippet registries repeat the same phase-shared entries | High | High | Introduce one canonical item registry and derive phase lists |
| F3 | Variable-source badge/label presentation is derived in multiple incompatible ways | High | Medium | Centralize presentation metadata beside `VariableSource` |
| F4 | Three CodeMirror extensions repeat the same decoration-plugin lifecycle | High | Low | Extract only after F1; keep each decoration builder separate |

No actionable production duplication was found in `src/components/ui`. Its strongest look-alikes are expected shadcn/Radix variants with different accessibility contracts and primitive types. They should not be collapsed into generic components.

## Detailed findings

### F1 — Variable-token recognition and range calculation are implemented repeatedly

**Confidence:** High

**Priority:** High

#### Exact references

- Existing shared parser and grammar: `src/lib/text-variables.ts:1-38`
- Monaco decoration scan and offset reconstruction: `src/components/editor/MonacoWrapper.tsx:121-164`
- Monaco hover scan and a second offset reconstruction: `src/components/editor/MonacoWrapper.tsx:180-243`
- Secret-mask complete-token scan: `src/components/editor/extensions/secret-mask.ts:63-85`
- URL token exclusion-range scan: `src/components/editor/extensions/url-tokens.ts:28-41`
- CodeMirror variable highlight scan: `src/components/editor/extensions/variable-highlight.ts:39-60`
- CodeMirror popover hit-test scan: `src/components/editor/extensions/variable-popover.ts:91-106`
- Autocomplete partial-token grammar: `src/components/editor/extensions/variable-autocomplete.ts:61-64`

#### Duplicate intent

All of these paths answer some subset of the same questions:

1. Is this text a `{{variable}}` token?
2. What is the variable name?
3. What source offsets does the token occupy?
4. Is an incomplete `{{...` opener present?

`MonacoWrapper` already uses `parseTextTokens`, but it reconstructs offsets twice by summing `rawLength`. The CodeMirror extensions independently use global regular expressions and manually reset `lastIndex`, then separately calculate `from`/`to` ranges.

This is not merely syntactic repetition. Highlighting, masking, click targeting, URL exclusion, hover behavior, and autocomplete can disagree when the token grammar changes.

#### Important differences to preserve

- `src/lib/text-variables.ts:1-4` deliberately allows whitespace inside braces; the comment says the URL bar is intentionally stricter.
- `secret-mask.ts:13-15,77-82` recognizes a trailing partial opener so typed variable text remains visible.
- `variable-autocomplete.ts:61-64` recognizes partial tokens rather than only complete tokens.
- `url-tokens.ts:11` does not currently include `$`, while the parser, highlighter, popover, and autocomplete grammars do. That difference may be accidental, but it should be characterized with tests before changing behavior.
- Monaco needs all text segments for decoration offsets; most CodeMirror consumers only need variable ranges.

#### Recommendation

Make `src/lib/text-variables.ts` the survivor and extend it with an offset-bearing scanner rather than introducing another editor-local regex helper. A suitable shape would be a pure API such as `scanVariableTokens(text, grammar)` returning `{ name, from, to, raw }`, with an explicit strict/whitespace-tolerant grammar option. If partial-openers are supported, expose them as a distinct result or helper rather than making complete-token consumers infer them.

Then:

- implement `parseTextTokens` in terms of the scanner, preserving its current public behavior;
- use scanner ranges directly in both Monaco loops;
- use the strict profile in single-line URL/editor extensions where strictness is intended;
- use a dedicated partial-token helper for autocomplete and secret masking.

Do **not** silently replace every regex with the whitespace-tolerant parser. The documented URL-bar grammar difference is intentional.

#### Survivor/shared abstraction

- **Survivor:** `src/lib/text-variables.ts` and its tested `parseTextTokens` behavior.
- **Shared abstraction:** offset-bearing variable token scanner plus an explicit partial-opener helper.
- **Consumers to retire:** complete-token scanning loops in `secret-mask.ts`, `url-tokens.ts`, `variable-highlight.ts`, and `variable-popover.ts`; manual Monaco offset accumulation for variable lookup.

#### Staged remediation

1. Add table-driven tests to `src/lib/__tests__/text-variables.test.ts` for offsets, `$` dynamic variables, dotted/hyphenated names, strict versus padded braces, adjacent tokens, and incomplete openers.
2. Add characterization tests for the current strict behavior in `url-tokens`, `variable-highlight`, and popover hit detection, especially `{{$guid}}` and `{{ key }}`.
3. Introduce the scanner while keeping `parseTextTokens` output unchanged.
4. Migrate `variable-highlight.ts` and `variable-popover.ts` first; their needs are simple complete-token ranges.
5. Migrate `url-tokens.ts` and `secret-mask.ts`, retaining their strict and partial-opener behavior explicitly.
6. Migrate both `MonacoWrapper` passes and verify decoration and hover ranges for multiline content.
7. Run the focused editor and text-variable tests, then `yarn tsc --noEmit`.

---

### F2 — Script API-reference snippet registries repeat the same phase-shared entries

**Confidence:** High

**Priority:** High

#### Exact references

- Tests-phase/default response and `rok` API lists: `src/components/editor/rok-types.ts:91-145`
- Post-response copies of response and `rok` API lists: `src/components/editor/rok-types.ts:247-308`
- Pre-request copy of the `rok` API list: `src/components/editor/rok-types.ts:343-409`
- Phase-specific availability tests: `src/components/editor/__tests__/rok-types.test.ts:50-66`

#### Duplicate intent

The same snippet item is repeatedly encoded as an independent `{ label, kind, code }` object:

- the entire `res.*` expression list is duplicated between the tests/default and post-response registries;
- core `rok.*` entries such as `getVar`, `setVar`, `getEnvVar`, `getCollectionVar`, `interpolate`, and `runner.setNextRequest` are repeated across all three registries;
- some entries appear in exactly two phases.

The repeated data is an API capability registry. Maintaining it by copying objects makes label/code drift likely when a scripting method is renamed or added.

#### Important differences to preserve

The phase differences are real and tested:

- `rok.runner.skipRequest()` is pre-request only (`rok-types.test.ts:59-66`).
- `rok.runner.setNextRequest()` is available in all phases (`rok-types.test.ts:50-57`).
- Request methods belong only in pre-request.
- Response methods belong in post-response and tests.
- The current `rok.*` lists are not identical: mutating environment/collection methods and `getEnvName()` vary by phase.
- Common templates differ by phase and should remain separate; only the API-reference item definitions are duplicated.

#### Recommendation

Define each API expression once with stable identity and explicit phase availability. Derive each phase's `ScriptSnippetSubGroup[]` from that registry while preserving the public exports `ROK_SNIPPETS`, `POST_RESPONSE_SNIPPETS`, and `PRE_REQUEST_SNIPPETS` so callers do not change.

A registry can use either:

- `phases: ScriptPhase[]` on each item; or
- canonical item maps plus phase-specific ordered ID arrays.

The second form is preferable if ordering is intentionally different by phase. It keeps capability policy and display order visible without copying labels and code strings.

#### Survivor/shared abstraction

- **Survivors:** the three exported phase arrays and `ScriptSnippetItem`/group types, because they are the established consumer API.
- **Shared abstraction:** canonical `REQ_API_ITEMS`, `RES_API_ITEMS`, and `ROK_API_ITEMS` definitions, with phase-specific composition by ID.
- **Do not consolidate:** phase-specific common-pattern/test templates.

#### Staged remediation

1. Extend `rok-types.test.ts` to assert exact API item IDs/order per phase, not only the two runner methods.
2. Introduce canonical item maps without changing exported arrays.
3. Derive the duplicated `res` and `rok` subgroups from phase-specific ID lists.
4. Compare generated labels and code against the existing arrays in a temporary characterization test.
5. Remove the copied item objects once parity is proven.
6. Run `yarn test src/components/editor/__tests__/rok-types.test.ts` and `yarn tsc --noEmit`.

---

### F3 — Variable-source badge/label presentation is derived in multiple incompatible ways

**Confidence:** High

**Priority:** Medium

#### Exact references

- Scope-specific labels embedded while building context: `src/lib/url-variables.ts:40-77`
- Shared badge color mapping: `src/lib/url-variables.ts:80-93`
- Explicit autocomplete badge letters: `src/components/editor/extensions/variable-autocomplete.ts:27-49`
- Popover badge letter derived from the source's first character: `src/components/editor/VariablePopover.tsx:118-145`
- Monaco hover source label derived by capitalization: `src/components/editor/MonacoWrapper.tsx:216-235`
- Monaco source-to-decoration-class map: `src/components/editor/MonacoWrapper.tsx:27-37`

#### Duplicate intent

Autocomplete, the edit popover, and Monaco hover all present a variable's source to the user, but each computes that presentation independently.

There is already visible drift:

- autocomplete intentionally renders request scope as `Q` (`variable-autocomplete.ts:35-38`);
- the popover uses `source.charAt(0).toUpperCase()`, so request and runtime both render as `R` (`VariablePopover.tsx:123-128`);
- Monaco generates labels mechanically, while `buildScopedContext` carries contextual labels such as the active environment name.

The color implementation should remain renderer-specific, but the semantic glyph/default name should not be reinvented by each renderer.

#### Important differences to preserve

- `VariableScopeEntry.label` may be contextual, particularly for environments; a static source label is only a fallback.
- Monaco and CodeMirror need different class names and theme mechanisms. Their CSS class maps are not interchangeable abstractions.
- `pathParam` is a UI-only pseudo-source in `VariablePopover`; it is not part of `VariableSource`.
- Dynamic values are regenerated for previews in some paths, so value formatting should not be folded into static source metadata.

#### Recommendation

Add canonical source presentation metadata beside `VariableSource`, for example `variableSourceBadge(source)` and `variableSourceLabel(source)`, or a `VARIABLE_SOURCE_META` record containing an unambiguous badge glyph and default label. Keep `sourceBadgeClass` as the color/style API.

Use the shared badge function in autocomplete and the popover. Use `entry.label` in Monaco when available, falling back to the canonical source label. Handle `pathParam` locally as a separate pseudo-source.

Do **not** centralize Monaco/CodeMirror CSS class names into this metadata; those classes belong to their renderers.

#### Survivor/shared abstraction

- **Survivor:** `src/lib/url-variables.ts`, because it owns `VariableSource`, `VariableScopeEntry`, and the existing `sourceBadgeClass` mapping.
- **Shared abstraction:** canonical source badge glyph and default source label helpers/metadata.
- **Local exception:** `pathParam` presentation remains in `VariablePopover`.

#### Staged remediation

1. Decide and test the canonical request badge (`Q` preserves the current autocomplete distinction between request and runtime).
2. Add exhaustive `Record<VariableSource, ...>` metadata and unit tests in `src/lib/__tests__/url-variables.test.ts`.
3. Replace `scopeBadge` in autocomplete.
4. Replace first-character derivation in `VariablePopover` while preserving path-parameter handling.
5. Update Monaco hover to prefer `entry.label` and use the canonical default only as fallback.
6. Run focused URL-variable and editor tests, then `yarn tsc --noEmit`.

---

### F4 — Three CodeMirror extensions repeat the same decoration-plugin lifecycle

**Confidence:** High

**Priority:** Low

#### Exact references

- Secret-mask plugin state/update wrapper: `src/components/editor/extensions/secret-mask.ts:49-61,108-112`
- URL-token plugin state/update wrapper: `src/components/editor/extensions/url-tokens.ts:94-114`
- Variable-highlight plugin state/update wrapper: `src/components/editor/extensions/variable-highlight.ts:20-37,67-71`

#### Duplicate intent

Each extension:

1. owns a `DecorationSet`;
2. builds it from an `EditorView` in the constructor;
3. rebuilds it in `update` when selected view/update conditions change; and
4. exposes it through `ViewPlugin.fromClass(..., { decorations })`.

Only the decoration builder, captured configuration, and invalidation predicate differ.

#### Important differences to preserve

- Secret masking and URL token decoration rebuild on document or viewport changes.
- Variable highlighting additionally rebuilds when `setVariableContextEffect` is dispatched.
- `urlTokens` closes over per-instance configuration and also returns a paste event handler.
- The actual builders have distinct responsibilities and should not be combined.

#### Recommendation

After F1 reduces the scanning duplication, consider a small editor-local helper such as `createDecorationPlugin(build, shouldRebuild)` that returns the `ViewPlugin` extension. Keep `buildMask`, URL decoration construction, and variable-highlight construction separate.

This is intentionally low priority: an abstraction that is more complex than the repeated lifecycle would be worse than the duplication. Extract only if the helper remains tiny, typed, and transparent.

#### Survivor/shared abstraction

- **Survivors:** all three domain-specific decoration builders.
- **Shared abstraction:** only the `DecorationSet` storage/constructor/update/`ViewPlugin.fromClass` plumbing, likely in `src/components/editor/extensions/decoration-plugin.ts`.
- **Do not absorb:** URL paste handling or variable-context state effects.

#### Staged remediation

1. Complete F1 first so token parsing and plugin lifecycle are not refactored simultaneously.
2. Ensure each extension has a test proving its invalidation behavior; variable highlighting already covers context effects at `variable-highlight.test.ts:86-96`.
3. Extract the helper for one simple plugin, preferably `secretMask`, and verify no type or lifecycle regression.
4. Migrate `variableHighlight`, passing its additional invalidation predicate.
5. Migrate only the decoration portion of `urlTokens`; keep paste handling in `urlTokens`.
6. Stop and retain the explicit classes if the helper requires casts or obscures CodeMirror update semantics.

## Intentional variants and non-findings

### Radix menu wrappers: structurally duplicated, intentionally separate

`src/components/ui/context-menu.tsx:13-182` and `src/components/ui/dropdown-menu.tsx:13-180` are near-parallel wrappers: sub-trigger, sub-content, content, item, checkbox item, radio item, label, separator, and shortcut.

This is an intentional shadcn pattern, not an actionable component merge:

- the wrappers bind different Radix primitive packages and event/accessibility semantics;
- their prop types are primitive-specific;
- dropdown content supplies `sideOffset=4` (`dropdown-menu.tsx:53-69`), while context-menu content does not (`context-menu.tsx:53-68`);
- focus text color, labels, shortcuts, and context-menu animation classes differ in small but potentially deliberate ways.

A generic component factory would make primitive typing and future shadcn updates harder. If visual drift becomes a product issue, share named class-string constants only; keep both public component families and their primitive bindings intact.

### Dialog, alert-dialog, and sheet wrappers: shared visual language, different contracts

- `src/components/ui/alert-dialog.tsx:10-115`
- `src/components/ui/dialog.tsx:11-94`
- `src/components/ui/sheet.tsx:10-77`

The overlay, title, description, header, and footer implementations visibly overlap. The variants nevertheless model different interactions:

- alert dialogs use `AlertDialogPrimitive.Action`/`Cancel` and intentionally constrain dismissal;
- dialogs include a close control and ordinary dialog semantics;
- sheets use dialog primitives but have directional layout and motion.

Do not collapse these into a generic modal wrapper. Shared style constants may be reasonable if design-system drift becomes costly, but there is no current behavioral duplication requiring remediation.

### Floating surface styles: expected design-system repetition

`ContextMenuSubContent`/`Content`, `DropdownMenuSubContent`/`Content`, `PopoverContent`, and `SelectContent` repeat border, translucent background, blur, shadow, and side-aware animation classes:

- `src/components/ui/context-menu.tsx:37-68`
- `src/components/ui/dropdown-menu.tsx:37-70`
- `src/components/ui/popover.tsx:9-29`
- `src/components/ui/select.tsx:62-95`

These components intentionally share a visual token language but differ in width, padding, positioning, viewport, and primitive state behavior. A shared Tailwind string would have low semantic value and could obscure per-primitive differences. Prefer theme tokens for cross-component visual changes.

### Input and textarea wrappers: intentional control variants

`src/components/ui/input.tsx:4-18` and `src/components/ui/textarea.tsx:4-17` share field chrome and focus/error states, but they wrap different native controls with different dimensions, background behavior, and file-input support. They should remain separate shadcn primitives.

### Monaco light/dark theme data: paired variants, not duplicate logic

`src/components/editor/monaco-config.ts:62-225` and `src/components/editor/monaco-config.ts:231-404` have parallel structures, but the data is an explicit light/dark port of upstream VS Code themes. Converting it into a generator would make source comparison and future theme updates harder. Keep both theme objects explicit.

### Editor skeleton versus the generic skeleton primitive

`src/components/editor/EditorSkeleton.tsx:5-26` and `src/components/ui/skeleton.tsx:3-5` share placeholder styling intent, but `EditorSkeleton` is a specialized composed loading layout with `motion-safe:animate-pulse`, line-number structure, and Monaco-sized content. Replacing each internal block with `Skeleton` would provide little semantic consolidation and would currently regress the motion-safe behavior because the generic primitive uses unconditional `animate-pulse`.

### Single-line and multi-line editor state

`SingleLineEditor` and `MonacoWrapper` both adapt controlled React values to imperative editor APIs, but the implementations are not duplicates:

- CodeMirror is manually constructed and requires explicit prop-to-document synchronization (`SingleLineEditor.tsx:203-255`);
- `@monaco-editor/react` owns Monaco value synchronization (`MonacoWrapper.tsx:251-261`);
- their plugin, disposal, and lifecycle models are materially different.

No shared editor-state hook is recommended.

## Folder/file summary with no actionable findings

| Area | Result |
|---|---|
| `src/components/ui/**` | No actionable duplication. Similar menu, modal, field, and floating-surface wrappers are intentional shadcn/Radix variants. |
| `src/components/editor/__tests__/**` | No production finding. Repeated fixture builders are test-local and small; tests were used to validate intended phase and editor behavior. |
| `src/components/editor/extensions/__tests__/**` | No production finding. Test setup repetition is conventional and keeps extension behavior isolated. |
| `src/components/editor/EditorSkeleton.tsx` | Specialized composition; generic `Skeleton` is not currently a behavior-preserving replacement. |
| `src/components/editor/SingleLineEditor.tsx` | No duplicate controlled-editor implementation after accounting for CodeMirror-specific lifecycle requirements. |
| `src/components/editor/monaco-config.ts` | Light/dark theme parallelism is intentional data; `detectLanguage`, CSS-variable conversion, and option definitions are distinct responsibilities. |
| `src/components/editor/monaco-setup.ts` and `useMonacoTheme.ts` | Initial theme registration and theme-change re-registration are different lifecycle stages. The returned `defineThemes` safety function appears unused, but dead-code cleanup is outside this duplication audit. |
| `src/components/editor/chai-types.d.ts.txt` | Vendored declaration payload; no authored implementation finding. |
| `src/components/editor/extensions/single-line-filter.ts` | Unique single-line transaction policy; no duplicate implementation in scope. |
| `src/components/editor/extensions/variable-context-facet.ts` | Unique mutable CodeMirror state-field adapter; no duplicate implementation in scope. |

## Recommended remediation order

1. **F1:** establish one tested variable-token scanner and migrate consumers incrementally.
2. **F2:** canonicalize script API snippet items while preserving phase-specific capability lists and public exports.
3. **F3:** centralize source badge/default-label metadata to remove the request/runtime badge ambiguity.
4. **F4:** optionally extract CodeMirror decoration lifecycle plumbing after F1; skip it if the helper is not simpler than the explicit classes.

The UI wrapper families should remain separate. Any future work there should target design-token consistency, not component unification.
