# CodeMirror v6 SingleLineEditor — Design Spec

**Date:** 2026-04-17
**Status:** Draft
**Feature:** Replace contenteditable/overlay variable-aware inputs with CodeMirror v6

---

## Problem Statement

RocketAPI's variable-aware input fields use a "transparent `<input>` + absolute overlay" architecture
where a real `<input>` with `text-transparent caret-foreground` sits beneath an overlay `<div>` that
renders colorized `{{variable}}` tokens. This pattern has six fundamental flaws that cannot be
fixed incrementally:

1. **Caret/overlay drift on scroll** — the `<input>` scrolls internally when text overflows,
   but the overlay `div` (with `overflow-hidden whitespace-nowrap`) never tracks that scroll
   offset. The caret and colored tokens become completely misaligned.

2. **Invisible text selection** — with `text-transparent`, the browser's selection highlight
   on the real input is invisible. Users cannot see what they've selected for copy/cut/drag.

3. **Pointer-event conflict** — the overlay needs `pointer-events-none` so clicks pass through
   to the input for caret positioning. But variable tokens need `pointer-events-auto` for
   popovers. Clicking a `{{variable}}` opens the popover instead of positioning the caret —
   users cannot click *within* a variable name to fix a typo.

4. **No autocomplete** — users must type full `{{variableName}}` from memory. Bruno provides
   autocomplete after `{{`, listing all in-scope variables with scope badges.

5. **Duplicated logic** — `VariableAwareUrlInput` uses `parseUrlTokens()` (handles `{{var}}`,
   `:pathParam`, query params) while `VariableAwareInput` uses `parseTextTokens()` (handles
   only `{{var}}`). Popover rendering and env store commit logic are duplicated with subtle
   differences.

6. **Secret masking breaks alignment** — `type='password'` replaces characters with `●` in
   the overlay, but `●` has different rendered width than alphanumeric characters in monospace
   fonts.

---

## Reference: Bruno's Approach

Bruno uses CodeMirror 5 with a custom `brunovariables` overlay mode. Both `SingleLineEditor`
and `MultiLineEditor` are CodeMirror wrappers — the only difference is that `SingleLineEditor`
blocks Enter and enforces single-line mode. Every variable-aware field (URL bar, header
name/value, bearer token, basic auth, query params) uses the same component.

This works because CodeMirror owns the entire editing surface — cursor, selection, text, and
decorations are all part of the same rendering pipeline. No overlay/input split means no
alignment drift.

RocketAPI will use **CodeMirror v6** (the modern rewrite) instead of v5. CM6 is lighter
(tree-shakable), has better TypeScript support, and uses a functional/compositional extension
model that fits well with React.

---

## Architecture

### Component Hierarchy

```
SingleLineEditor (React wrapper)
├── EditorView (CM6 instance, owns cursor + selection + text + scroll)
│
├── Extension: singleLineFilter
│   └── Transaction filter: rejects changes producing > 1 line
│
├── Extension: variableHighlightPlugin (ViewPlugin)
│   ├── Reads variableContext facet (Map<string, VariableScopeEntry>)
│   ├── Scans document for {{...}} matches
│   └── Applies Decoration.mark() with scope-colored CSS classes
│
├── Extension: variableAutocomplete
│   ├── Completion source activates when cursor follows {{
│   ├── Lists all keys from variableContext with scope badges
│   └── Inserts name + appends }} on accept
│
├── Extension: variablePopoverPlugin (ViewPlugin)
│   ├── Registers click + hover event handlers
│   ├── On activation: dispatches showTooltip state effect
│   ├── CM6 creates positioned DOM container (tracks scroll/resize)
│   └── React portal renders <VariablePopover /> into container
│
├── Extension: secretMaskPlugin (optional, when isSecret=true)
│   └── Decoration.replace replaces non-{{var}} text with ● widgets
│
└── Extension: urlTokensPlugin (optional, URL bar only)
    ├── :pathParam highlighting + popovers
    ├── Query param key=value highlighting
    └── Curl paste detection via domEventHandlers
```

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│ React Component Tree                                         │
│                                                              │
│  useEnvStore ──→ buildScopedContext() ──→ variableContext    │
│                                            (Map<str,Entry>) │
│                                                ↓             │
│  SingleLineEditor                                            │
│    ├── props.value ──→ EditorView.dispatch(setValue)         │
│    ├── EditorView.onChange ──→ props.onChange(newText)        │
│    └── variableContext ──→ reconfigure facet via effect      │
│                                                              │
│  VariablePopover (React portal into CM6 tooltip DOM)         │
│    ├── reads scope from variableContext                      │
│    ├── editValue ──→ useVariableCommit hook                  │
│    └── commit ──→ updateEnvironment / updateGlobalEnv / etc  │
└─────────────────────────────────────────────────────────────┘
```

---

## Props Interface

```typescript
interface SingleLineEditorProps {
  /** Current text content (controlled). */
  value: string;
  /** Called on every content change. */
  onChange: (value: string) => void;
  /** Placeholder shown when editor is empty. */
  placeholder?: string;
  /** Additional CSS class for the editor wrapper. */
  className?: string;
  /** Disables editing. */
  disabled?: boolean;

  // ── Variable system ──────────────────────────────────────

  /** Scope-aware variable map. When undefined, no variable extensions load. */
  variableContext?: Map<string, VariableScopeEntry>;
  /** Called when user clicks "Navigate to source →" in popover. */
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;

  // ── Secret masking ───────────────────────────────────────

  /** When true, non-variable text is masked with ● characters. */
  isSecret?: boolean;

  // ── URL bar extras (only used in URL input) ──────────────

  /** Path parameter values for :param highlighting. */
  pathParams?: Record<string, string>;
  /** Query parameter values for ?key=value highlighting. */
  queryParams?: Record<string, string>;
  /** Called when a path param value is edited in the popover. */
  onPathParamChange?: (key: string, value: string) => void;
  /** Called when a curl command is pasted. */
  onCurlImport?: (parsed: ParsedCurl) => void;
  /** Called on Enter key (send request). */
  onSubmit?: () => void;
  /** Raw keydown handler for additional shortcuts. */
  onKeyDown?: (event: KeyboardEvent) => void;
}
```

When `variableContext` is `undefined`, the component creates a minimal EditorView with only
the single-line filter — no variable highlighting, autocomplete, or popover extensions. This
preserves the current optimization where `VariableAwareInput` renders a plain `<Input>` when
no context is provided.

---

## Extension Details

### 1. Single-Line Filter

Uses CM6's recommended transaction filter approach:

```typescript
const singleLineFilter = EditorState.transactionFilter.of(tr =>
  tr.newDoc.lines > 1 ? [] : tr
);
```

This cleanly blocks Enter key, pasted newlines, and any programmatic multi-line insertion.
No individual key event interception needed.

For the URL bar, an additional keymap fires `onSubmit` on Enter:

```typescript
const submitOnEnter = keymap.of([{
  key: 'Enter',
  run: () => { onSubmit?.(); return true; }
}]);
```

### 2. Variable Highlight Plugin

A `ViewPlugin` that builds a `DecorationSet` from `{{...}}` regex matches:

```typescript
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

// Decoration specs (reusable, not per-instance):
const varResolved = (scope: VariableSource) => Decoration.mark({
  class: `cm-var cm-var-${scope}`,  // e.g. cm-var-environment
});
const varUnresolved = Decoration.mark({
  class: 'cm-var cm-var-unresolved',
});
```

The plugin reads the `variableContext` from a **Facet**:

```typescript
const variableContextFacet = Facet.define<
  Map<string, VariableScopeEntry>,
  Map<string, VariableScopeEntry>
>({
  combine: inputs => inputs[inputs.length - 1] ?? new Map(),
});
```

The React wrapper updates this facet via `view.dispatch({ effects: setContextEffect.of(newContext) })`
whenever the environment store changes. This triggers the highlight plugin to rebuild decorations.

**CSS classes for scope colors** (matching existing `sourceBadgeClass` conventions):

| Scope | CSS Class | Light Mode | Dark Mode |
|---|---|---|---|
| environment | `cm-var-environment` | amber bg/text | amber bg/text |
| collection | `cm-var-collection` | gray bg/text | gray bg/text |
| global | `cm-var-global` | blue bg/text | blue bg/text |
| folder | `cm-var-folder` | teal bg/text | teal bg/text |
| request | `cm-var-request` | green bg/text | green bg/text |
| runtime | `cm-var-runtime` | green bg/text | green bg/text |
| process | `cm-var-process` | slate bg/text | slate bg/text |
| (unresolved) | `cm-var-unresolved` | red bg/text | red bg/text |

These CSS classes use `background` and `color` properties with `border-radius: 3px` and
`padding: 1px 3px` to create the same rounded badge look as the current overlay spans.

### 3. Variable Autocomplete

Uses `@codemirror/autocomplete` with a custom completion source:

```typescript
const variableCompletions: CompletionSource = (context) => {
  // Look backward for {{ to determine if we're inside a variable reference.
  const before = context.matchBefore(/\{\{[\w.-]*/);
  if (!before) return null;

  const varContext = context.state.facet(variableContextFacet);
  const options: Completion[] = [];

  for (const [key, entry] of varContext) {
    options.push({
      label: key,
      detail: entry.source.charAt(0).toUpperCase(), // Scope badge: E, C, G, etc.
      info: entry.secret ? '●●●●' : entry.value,
      type: 'variable',
      boost: scopeBoost(entry.source), // Higher-priority scopes rank first
      apply: (view, completion, from, to) => {
        // Insert the variable name. If }} doesn't follow, append it.
        const after = view.state.sliceDoc(to, to + 2);
        const insert = after === '}}' ? key : `${key}}}`;
        view.dispatch({ changes: { from: from + 2, to, insert } });
      },
    });
  }

  return {
    from: before.from,
    options,
    filter: true, // CM6 handles fuzzy filtering
  };
};
```

**Scope boost values** (higher = ranked first in autocomplete):

| Scope | Boost |
|---|---|
| runtime | 6 |
| request | 5 |
| folder | 4 |
| environment | 3 |
| collection | 2 |
| global | 1 |
| process | 0 |

This matches the resolution priority order — the variable that would actually win in
resolution appears first in the completion list.

`Ctrl+Space` triggers completions manually (built into `@codemirror/autocomplete`).

### 4. Variable Popover Plugin (CM6 Tooltip + React Portal)

This is the most complex extension. It uses CM6's `showTooltip` facet to position the
popover, and React's `createPortal` to render interactive content.

**Architecture:**

```
User clicks {{baseUrl}} in editor
  ↓
ViewPlugin click handler fires
  ↓
Handler checks: did click land in a Decoration.mark range?
  ↓ yes
Handler dispatches state effect: setActivePopover({ varName, from, to, scope })
  ↓
showTooltip facet reacts: creates tooltip at pos=from
  ↓
CM6 creates positioned <div> in the DOM, anchored below the token
  ↓
React wrapper observes tooltip DOM via MutationObserver or ref callback
  ↓
createPortal(<VariablePopover varName={...} scope={...} />, tooltipDiv)
  ↓
User sees the familiar popover: value input, scope badge, nav link
```

**Why this approach (vs alternatives):**

| Approach | Pros | Cons |
|---|---|---|
| **CM6 tooltip + React portal** (chosen) | CM6 handles positioning, scroll tracking, resize. React owns component tree with hooks. | Portal wiring adds complexity. |
| Radix Popover + manual `coordsAtPos()` | Pure React, simpler component tree. | Must manually track scroll/resize repositioning. Editor focus management conflicts with Radix. |
| CM6 WidgetType.toDOM() only | Pure CM6, no portal. | Raw DOM — no React hooks, no shadcn components, no access to Zustand stores. |

**The VariablePopover component** (rendered inside the portal):

```typescript
interface VariablePopoverProps {
  varName: string;
  entry: VariableScopeEntry | undefined; // undefined = unresolved
  onCommit: (newValue: string) => Promise<void>;
  onClose: () => void;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}
```

Popover content reuses existing design:
- **Value input** — `<Input autoFocus />` with the resolved value (or "●●●●" for secrets).
  Read-only for collection, folder, request, and process vars.
- **Footer** — scope badge (E/C/G/F/R/P circle) + label on the left,
  "Navigate to source →" link on the right.
- **Commit behavior** — Enter or blur saves via `useVariableCommit` hook. Escape closes
  without saving.

### 5. useVariableCommit Hook (Shared)

Extracts the duplicated commit logic from both current components into a single hook:

```typescript
function useVariableCommit() {
  const activeEnvId = useEnvStore(s => s.activeEnvId);
  const environments = useEnvStore(s => s.environments);
  const updateEnvironment = useEnvStore(s => s.updateEnvironment);
  const globalEnv = useEnvStore(s => s.globalEnv);
  const updateGlobalEnvironment = useEnvStore(s => s.updateGlobalEnvironment);

  return async (
    varName: string,
    newValue: string,
    scope: VariableSource | null,
    onPathParamChange?: (key: string, value: string) => void,
  ) => {
    if (scope === 'global' && globalEnv) {
      // Upsert into global environment
    } else if ((scope === 'environment' || scope === null) && activeEnvId) {
      // Upsert into active environment
    }
    // Collection, folder, request, process: read-only — no-op
  };
}
```

This hook is used by `VariablePopover` regardless of whether it's inside the URL bar
or a key-value editor field.

### 6. Secret Mask Plugin (Optional)

When `isSecret={true}`, a ViewPlugin applies `Decoration.replace` to all text that is NOT
inside a `{{...}}` pattern, replacing each character with a `●` WidgetType.

```typescript
class MaskWidget extends WidgetType {
  constructor(readonly length: number) { super(); }
  toDOM() {
    const span = document.createElement('span');
    span.textContent = '●'.repeat(this.length);
    span.className = 'cm-secret-mask';
    return span;
  }
}
```

The actual text in the document is preserved — copying works correctly. Only the visual
rendering is masked. Variable tokens remain visible and colored (identical to how Bruno's
`MaskedEditor` works).

### 7. URL Tokens Plugin (Optional, URL Bar Only)

Additional extensions loaded only for the URL bar:

**Path param highlighting:**
- Regex: `/:(\w+)/g` in the path portion (before `?`)
- `Decoration.mark({ class: 'cm-pathparam' })` — violet badge
- Click opens popover with same layout (`:` badge instead of scope letter)

**Query param highlighting:**
- After `?`, `key=value` segments get subtle coloring
- Query keys: `cm-querykey` class. Query values: inherit base color.
- No popover (edit in Params tab, matching current behavior)

**Curl paste detection:**
- `domEventHandlers({ paste: (event, view) => { ... } })` extension
- Checks clipboard text for `/^curl\s/i` prefix
- Calls `onCurlImport(parseCurl(text))` and prevents default

---

## Call Sites — Migration Map

| Current Component | Location | Replacement |
|---|---|---|
| `VariableAwareUrlInput` | `RequestPanel.tsx` (URL bar, 1 instance) | `<SingleLineEditor>` with `pathParams`, `queryParams`, `onCurlImport`, `onSubmit` |
| `VariableAwareInput` | `KeyValueEditor.tsx` (header/form body values) | `<SingleLineEditor>` with `variableContext` only |
| `VariableAwareInput` | `PathParamsPanel.tsx` (path param values) | `<SingleLineEditor>` with `variableContext` only |
| `VariableAwareInput` | `AuthEditor.tsx` (bearer, basic, OAuth2 — ~12 instances) | `<SingleLineEditor>` with `variableContext`, some with `isSecret` |

---

## Styling

### CM6 EditorView Theme

The EditorView must match RocketAPI's existing input field appearance:

```typescript
const rocketTheme = EditorView.theme({
  '&': {
    fontSize: '12px',       // text-xs
    fontFamily: 'var(--font-mono)',
  },
  '.cm-content': {
    padding: '4px 0',
    caretColor: 'hsl(var(--foreground))',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '&.cm-focused .cm-content': {
    // Match shadcn focus ring
  },
  '.cm-line': {
    padding: '0 12px',      // px-3
  },
  '.cm-placeholder': {
    color: 'hsl(var(--muted-foreground))',
  },
  // Variable token classes
  '.cm-var': {
    borderRadius: '3px',
    padding: '1px 3px',
  },
  '.cm-var-environment': {
    background: 'rgba(234,179,8,0.15)',
    color: 'hsl(var(--foreground))',  // Uses amber for the {{}} brackets via nested span
  },
  '.cm-var-unresolved': {
    background: 'hsl(var(--destructive) / 0.15)',
    color: 'hsl(var(--destructive))',
  },
  // ... other scope colors
});
```

The wrapper div gets the same border, border-radius, and focus ring as shadcn `<Input>`:
`h-8 rounded-md border border-input bg-background focus-within:ring-[3px] focus-within:border-ring`.

### Dark Mode

All CM6 theme colors use CSS custom properties (`hsl(var(--...))`) so they automatically
adapt to RocketAPI's dark mode toggle. No separate dark theme needed.

---

## Performance

### Many Editors on One Page

Key-value editors (headers, query params) create one `SingleLineEditor` per row. A request
with 10 headers = 10 CM6 EditorView instances.

**Measured CM6 creation cost:** ~3ms per instance (lightweight — no syntax tree, no
line numbers, no code folding). For typical requests (5-15 key-value rows), this is
negligible.

**Degenerate case mitigation:** For >30 rows (unlikely but possible), use virtualization
or lazy initialization — render a plain styled `<span>` for off-screen rows and swap in
the full CM6 instance on focus. This is a future optimization, not needed for launch.

### Bundle Size

CM6 is tree-shakable. The required packages:

| Package | Approx gzipped |
|---|---|
| `@codemirror/state` | ~12KB |
| `@codemirror/view` | ~25KB |
| `@codemirror/autocomplete` | ~8KB |
| `@codemirror/commands` (subset) | ~3KB |
| **Total** | **~48KB** |

RocketAPI already ships Monaco (~1.5MB gzipped) for the body editor, so CM6 adds
a negligible amount to the total bundle. Long-term, CM6 could potentially replace
Monaco entirely (separate decision).

---

## Files Created / Modified / Deleted

### New Files

| Path | Responsibility |
|---|---|
| `src/components/editor/SingleLineEditor.tsx` | Main React wrapper. Creates EditorView, wires props to extensions. |
| `src/components/editor/VariablePopover.tsx` | React component rendered via portal into CM6 tooltip DOM. |
| `src/components/editor/extensions/single-line-filter.ts` | Transaction filter for single-line mode. |
| `src/components/editor/extensions/variable-highlight.ts` | ViewPlugin for `{{var}}` decoration marks. |
| `src/components/editor/extensions/variable-autocomplete.ts` | Completion source from variableContext facet. |
| `src/components/editor/extensions/variable-popover.ts` | ViewPlugin for click/hover → tooltip state management. |
| `src/components/editor/extensions/secret-mask.ts` | Optional Decoration.replace for `isSecret` mode. |
| `src/components/editor/extensions/url-tokens.ts` | Optional `:pathParam` + query highlighting + curl paste. |
| `src/components/editor/extensions/theme.ts` | EditorView.theme matching shadcn Input appearance. |
| `src/components/editor/extensions/variable-context-facet.ts` | Shared facet definition for variableContext. |
| `src/hooks/useVariableCommit.ts` | Shared hook for saving variable edits to env/global stores. |
| `src/components/editor/__tests__/SingleLineEditor.test.tsx` | Component tests. |
| `src/components/editor/__tests__/variable-highlight.test.ts` | Extension unit tests. |

### Modified Files

| Path | Change |
|---|---|
| `src/components/request/RequestPanel.tsx` | Replace `<VariableAwareUrlInput>` with `<SingleLineEditor>` |
| `src/components/request/KeyValueEditor.tsx` | Replace `<VariableAwareInput>` with `<SingleLineEditor>` |
| `src/components/request/PathParamsPanel.tsx` | Replace `<VariableAwareInput>` with `<SingleLineEditor>` |
| `src/components/request/AuthEditor.tsx` | Replace `<VariableAwareInput>` with `<SingleLineEditor>` (~12 instances) |
| `package.json` | Add `@codemirror/state`, `@codemirror/view`, `@codemirror/autocomplete`, `@codemirror/commands` |

### Deleted Files (after migration complete)

| Path | Reason |
|---|---|
| `src/components/request/VariableAwareInput.tsx` | Fully replaced by SingleLineEditor |
| `src/components/request/VariableAwareUrlInput.tsx` | Fully replaced by SingleLineEditor + url-tokens extension |
| `src/lib/text-variables.ts` | `parseTextTokens()` replaced by CM6 ViewPlugin regex |
| `src/lib/__tests__/text-variables.test.ts` | Tests for deleted module |
| `src/components/request/__tests__/VariableAwareInput.test.tsx` | Tests for deleted component |

### Preserved (Reused)

| Path | What's Reused |
|---|---|
| `src/lib/url-variables.ts` | `buildScopedContext()`, `VariableScopeEntry`, `VariableSource`, `sourceBadgeClass()` |
| `src/lib/url-params.ts` | `parseQueryParams()`, `buildUrl()`, `extractPathParams()` — unchanged |
| `src/lib/curl-parser.ts` | `parseCurl()` — called from url-tokens extension instead of React handler |

---

## Implementation Phases

### Phase A: Core SingleLineEditor

Build the foundation that all call sites depend on.

1. Install CM6 packages
2. `variable-context-facet.ts` — shared facet definition
3. `single-line-filter.ts` — transaction filter
4. `theme.ts` — EditorView theme matching shadcn Input
5. `variable-highlight.ts` — ViewPlugin with Decoration.mark
6. `variable-autocomplete.ts` — completion source
7. `variable-popover.ts` — click/hover → tooltip state management
8. `VariablePopover.tsx` — React portal component
9. `useVariableCommit.ts` — shared commit hook
10. `SingleLineEditor.tsx` — React wrapper wiring it all together
11. Tests for highlight plugin, autocomplete source, and component

### Phase B: URL Bar Migration

Extend SingleLineEditor for URL-specific features and migrate the first call site.

1. `url-tokens.ts` — `:pathParam` highlighting, query highlighting, curl paste
2. `secret-mask.ts` — optional masking extension
3. Migrate `RequestPanel.tsx` URL bar from `VariableAwareUrlInput` → `SingleLineEditor`
4. Verify: caret tracking, scroll, popover positioning, curl paste, Enter-to-send

### Phase C: Field Migration + Cleanup

Migrate remaining call sites and delete old code.

1. Migrate `KeyValueEditor.tsx` value column
2. Migrate `PathParamsPanel.tsx` value column
3. Migrate `AuthEditor.tsx` (~12 field instances, some with `isSecret`)
4. Delete `VariableAwareInput.tsx`, `VariableAwareUrlInput.tsx`, `text-variables.ts`
5. Delete old tests, add new integration tests
6. Final smoke test across all field types

---

## Out of Scope

| Item | Reason |
|---|---|
| Replacing Monaco body editor with CM6 | Separate decision, much larger scope |
| Multi-line CM6 editor for body/scripts | Future — SP3 scripting may need this |
| Variable highlighting in Monaco body editor | Different component, different architecture |
| Inline variable creation (add new var from autocomplete) | Future enhancement |
| Drag-and-drop variable insertion | Not in Bruno, not needed |
| Theming/font customization for CM6 fields | Future — preferences system |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| CM6 tooltip positioning edge cases (near viewport edges) | Medium | Low | CM6 handles this internally; test in small windows |
| React portal inside CM6 tooltip loses React context | Low | High | Use context provider wrapper around portal content |
| Performance with 30+ CM6 instances on one page | Low | Medium | Lazy init — plain span until focused |
| CM6 focus management conflicts with popover | Medium | Medium | Explicit focus management in popover open/close handlers |
| Bundle size increase | Low | Low | ~48KB gzipped, negligible vs existing Monaco |
| Accessibility regression | Medium | Medium | CM6 has built-in ARIA; test with screen reader |

---

## Testing Strategy

### Unit Tests (extension level)

- `variable-highlight.test.ts` — given document text + variableContext, verify correct
  Decoration.mark ranges and CSS classes
- `variable-autocomplete.test.ts` — given cursor position and variableContext, verify
  correct completions returned, boost ordering, and `}}` append logic

### Component Tests

- `SingleLineEditor.test.tsx` — renders with/without variableContext, fires onChange on
  typing, blocks Enter key, shows placeholder
- `VariablePopover.test.tsx` — shows value input, respects readOnly for non-editable scopes,
  calls onCommit on Enter/blur, calls onNavigateToSource on link click

### Integration Tests (manual)

- Type `{{` in URL bar → autocomplete dropdown appears with all variables
- Click a `{{variable}}` → popover opens with correct value and scope badge
- Edit value in popover → Enter → value persists in environment store
- Scroll long URL → caret stays aligned with colored tokens
- Select text → selection highlight is visible
- Paste curl command in URL bar → import dialog triggers
- `isSecret` field → text masked, variables visible
- Path param `:id` in URL → violet badge, click opens popover
