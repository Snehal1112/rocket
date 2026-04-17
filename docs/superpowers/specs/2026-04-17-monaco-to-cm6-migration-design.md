# Monaco → CodeMirror v6 Migration — Design Spec

**Date:** 2026-04-17
**Status:** Draft
**Feature:** Replace all Monaco editor instances with CodeMirror v6 for a unified editor stack
**Depends on:** SingleLineEditor migration (Plans 01–08) — CM6 core packages already installed

---

## Problem Statement

RocketAPI runs two independent editor runtimes: CodeMirror v6 (for single-line variable-aware
inputs, after Plans 01–08) and Monaco (for multi-line body editing, response viewing, Git
diffs, and conflict resolution). This dual-stack causes three problems:

1. **Bundle size** — Monaco contributes ~1.5MB gzipped plus 5 web workers (JSON, CSS, HTML,
   TypeScript, editor core). CM6 with all needed language packages is ~120KB gzipped.
   Eliminating Monaco yields a ~12x reduction in editor-related bundle weight.

2. **Duplicated variable infrastructure** — `MonacoWrapper` has its own `{{var}}` decoration
   system (`ensureDecorationStyles`, `createDecorationsCollection`, `registerHoverProvider`)
   that duplicates what the CM6 `variableHighlight()` plugin already does. Two codepaths
   for the same feature means two places for bugs.

3. **Startup complexity** — Monaco requires async initialization in `main.tsx`: 5 web worker
   imports, `MonacoEnvironment.getWorker` dispatch, `loader.config({ monaco })`, and theme
   pre-definition inside an `import('monaco-editor').then(...)` wrapper that defers
   `ReactDOM.createRoot`. Removing this makes app startup synchronous and simpler.

---

## Feasibility: CM6 Feature Parity

Every Monaco feature currently used in RocketAPI has a direct CM6 equivalent:

| Monaco Feature | CM6 Equivalent | Package |
|---|---|---|
| JSON syntax highlighting | `json()` | `@codemirror/lang-json` |
| XML syntax highlighting | `xml()` | `@codemirror/lang-xml` |
| HTML syntax highlighting | `html()` | `@codemirror/lang-html` |
| JavaScript syntax highlighting | `javascript()` | `@codemirror/lang-javascript` |
| CSS syntax highlighting | `css()` | `@codemirror/lang-css` |
| YAML syntax highlighting | `yaml()` | `@codemirror/lang-yaml` (new — Monaco has no native YAML) |
| Line numbers | `lineNumbers()` | `@codemirror/view` |
| Code folding | `foldGutter()` | `@codemirror/language` |
| Bracket matching | `bracketMatching()` | `@codemirror/language` |
| Bracket pair colorization | `matchBrackets` + highlight | `@codemirror/language` |
| Word wrap | `EditorView.lineWrapping` | `@codemirror/view` |
| Read-only mode | `EditorState.readOnly.of(true)` | `@codemirror/state` |
| Auto-resize on container change | Native — no `automaticLayout` needed | Built-in |
| Side-by-side diff editor | `MergeView` | `@codemirror/merge` |
| Unified diff view | `unifiedMergeView` | `@codemirror/merge` |
| Inline `{{var}}` decorations | `variableHighlight()` | Reuse from Plans 01–05 |
| Hover tooltips on variables | `hoverTooltip()` | `@codemirror/view` |
| Search/replace (Ctrl+F) | `search()` | `@codemirror/search` |
| Undo/redo history | `history()` | `@codemirror/commands` |
| Custom themes (light/dark) | `EditorView.theme()` | Reuse from Plans 01–05 |

**Notable improvement:** CM6 adds YAML syntax highlighting via `@codemirror/lang-yaml`.
Monaco has no built-in YAML support, so `.yml` request files in the diff viewer currently
render as plaintext. After migration, they get proper syntax highlighting.

---

## Monaco Usage Sites — Migration Map

### Site 1: `MonacoWrapper` (request body editor)

**Current:** `@monaco-editor/react` `<Editor>` with `basicSetup`-equivalent options, variable
decorations via `createDecorationsCollection`, hover provider via `registerHoverProvider`.
Used by `BodyEditor` for JSON/XML/text body editing.

**Replacement:** New `MultiLineEditor` component — a CM6 `EditorView` with `basicSetup`,
language detection, and the **same** `variableHighlight()` + `variableContextFacet` extensions
from the SingleLineEditor work. Variable hover uses CM6's `hoverTooltip()` instead of Monaco's
`registerHoverProvider`.

**Key reuse:** The variable highlight plugin, context facet, theme, and CSS classes from
Plans 01–05 are used directly. Zero code duplication for `{{var}}` handling.

### Site 2: `ResponseBodyViewer` (read-only response display)

**Current:** Lazy-loaded `MonacoWrapper` with `readOnly={true}`. Displays pretty-printed
JSON, XML, HTML, JS, CSS, or plaintext.

**Replacement:** Lazy-loaded `MultiLineEditor` with `readOnly` prop. Same component as the
body editor, just in read-only mode. The `detectResponseLanguage` function maps content-type
headers to CM6 language extensions instead of Monaco language IDs.

### Site 3: `DiffViewer` (Git side-by-side diff)

**Current:** Monaco `<DiffEditor>` from `@monaco-editor/react` with both sides read-only,
`renderSideBySide: true`, language detection by file extension.

**Replacement:** `@codemirror/merge`'s `MergeView` — two CM6 editors side-by-side with
automatic diff highlighting and vertical alignment of unchanged lines. Both panes configured
read-only. Language extension selected by file extension.

`MergeView` provides all features currently used:
- Side-by-side layout with diff highlighting
- Vertical alignment of unchanged lines
- Gutter indicators for changed ranges
- `collapseUnchanged` option for large files
- Read-only configuration per pane
- Language-specific syntax highlighting in both panes

The `DiffHeader` and `VisualDiffView` components are unchanged — only the text diff
rendering engine swaps.

### Site 4: `ConflictResolver` (merge conflict editor)

**Current:** Two read-only Monaco `<Editor>` instances (ours/theirs) in split view, plus
one editable Monaco `<Editor>` for manual conflict resolution.

**Replacement:** `MultiLineEditor` in both modes — read-only for ours/theirs comparison,
editable for manual resolution. The component layout, action buttons, and resolution logic
are unchanged.

---

## Architecture

### New Components

```
MultiLineEditor (React wrapper)
├── EditorView (CM6 instance)
│
├── Extension: basicSetup (line numbers, folding, bracket matching, search, undo)
├── Extension: language (json/xml/html/js/css/yaml/plaintext — dynamic)
├── Extension: rocketTheme + rocketThemeDark (reuse from SingleLineEditor)
├── Extension: EditorView.lineWrapping
│
├── Extension: variableHighlight() (reuse from SingleLineEditor, optional)
├── Extension: variableContextFacet (reuse, optional)
├── Extension: variableHoverTooltip (new — CM6 hoverTooltip for {{var}} info)
│
└── Extension: EditorState.readOnly.of(true) (optional, for response viewer)
```

```
CM6DiffViewer (React wrapper)
├── MergeView (from @codemirror/merge)
│   ├── Pane A (old content, read-only)
│   │   └── Extensions: basicSetup, language, theme, readOnly
│   └── Pane B (new content, read-only)
│       └── Extensions: basicSetup, language, theme, readOnly
│
├── highlightChanges: true
├── gutter: true
└── collapseUnchanged: { margin: 3, minSize: 4 }
```

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│ BodyEditor                                                   │
│   ├── props.body.mode ──→ detectLanguage() ──→ lang extension│
│   ├── props.variableContext ──→ variableContextFacet         │
│   └── <MultiLineEditor value={body.content} onChange={...}   │
│         language="json" variableContext={ctx} />              │
│                                                              │
│ ResponseBodyViewer                                           │
│   └── <MultiLineEditor value={formatted} readOnly            │
│         language={detected} />                               │
│                                                              │
│ DiffViewer                                                   │
│   └── <CM6DiffViewer oldContent={old} newContent={new}       │
│         language={lang} />                                   │
│                                                              │
│ ConflictResolver                                             │
│   ├── <MultiLineEditor value={ours} readOnly />  (left)      │
│   ├── <MultiLineEditor value={theirs} readOnly /> (right)    │
│   └── <MultiLineEditor value={manual} onChange={...} />      │
└─────────────────────────────────────────────────────────────┘
```

---

## Props Interfaces

### MultiLineEditor

```typescript
interface MultiLineEditorProps {
  /** Current text content (controlled). */
  value: string;
  /** Called on every content change. Omit for read-only display. */
  onChange?: (value: string) => void;
  /** CM6 language identifier: 'json' | 'xml' | 'html' | 'javascript' | 'css' | 'yaml' | 'plaintext'. */
  language?: string;
  /** Auto-detect language from body mode (json/xml/text). */
  bodyMode?: string;
  /** Auto-detect language from Content-Type header. */
  contentType?: string;
  /** Read-only mode. */
  readOnly?: boolean;
  /** CSS height. */
  height?: string;
  /** Variable context for {{var}} highlighting + hover tooltips. */
  variableContext?: Map<string, VariableScopeEntry>;
}
```

This interface matches `MonacoWrapper`'s current props for drop-in replacement.

### CM6DiffViewer

```typescript
interface CM6DiffViewerProps {
  /** Original (old) content — left pane. */
  oldContent: string;
  /** Modified (new) content — right pane. */
  newContent: string;
  /** Language for syntax highlighting. */
  language: string;
}
```

---

## Extension Details

### Language Detection

The `detectLanguage` function currently returns Monaco language IDs. The CM6 version
returns a `LanguageSupport` extension:

```typescript
import { json } from '@codemirror/lang-json';
import { xml } from '@codemirror/lang-xml';
import { html } from '@codemirror/lang-html';
import { javascript } from '@codemirror/lang-javascript';
import { css } from '@codemirror/lang-css';
import { yaml } from '@codemirror/lang-yaml';

function getLanguageExtension(lang: string): Extension | null {
  switch (lang) {
    case 'json': return json();
    case 'xml': return xml();
    case 'html': return html();
    case 'javascript': return javascript();
    case 'css': return css();
    case 'yaml': return yaml();
    default: return null; // plaintext — no language extension
  }
}

function detectLanguage(bodyMode?: string, contentType?: string): string {
  if (bodyMode === 'json' || contentType?.includes('json')) return 'json';
  if (bodyMode === 'xml' || contentType?.includes('xml')) return 'xml';
  if (bodyMode === 'text') return 'plaintext';
  if (contentType?.includes('html')) return 'html';
  if (contentType?.includes('javascript')) return 'javascript';
  if (contentType?.includes('css')) return 'css';
  if (contentType?.includes('yaml')) return 'yaml';
  return 'plaintext';
}
```

### Variable Hover Tooltip (Multi-Line Only)

The current Monaco hover provider shows a markdown tooltip with variable name, source,
and resolved value. The CM6 equivalent uses `hoverTooltip()`:

```typescript
import { hoverTooltip, type Tooltip } from '@codemirror/view';

const variableHover = hoverTooltip((view, pos) => {
  const doc = view.state.doc.toString();
  // Find if pos is inside a {{var}} token
  const varToken = findVarTokenAt(doc, pos); // reuse from variable-popover.ts
  if (!varToken) return null;

  const context = view.state.facet(variableContextFacet);
  const entry = context.get(varToken.varName);

  return {
    pos: varToken.from,
    end: varToken.to,
    above: true,
    create: () => {
      const dom = document.createElement('div');
      dom.className = 'cm-var-hover';
      // Render: variable name, source label, resolved value
      dom.innerHTML = `
        <div class="font-mono font-bold">{{${varToken.varName}}}</div>
        <div>Source: ${entry ? entry.source.charAt(0).toUpperCase() + entry.source.slice(1) : 'Unresolved'}</div>
        <div>Value: ${entry?.secret ? '●●●●' : entry?.value ?? '(not set)'}</div>
      `;
      return { dom };
    },
  };
});
```

This provides the same information as Monaco's hover provider but as a native CM6 tooltip.
No `registerHoverProvider` per language, no disposable management — just one extension.

Note: unlike the SingleLineEditor popover (which has an editable input for changing variable
values), the multi-line body editor hover is **read-only display only**. This matches the
current Monaco behavior — the hover shows info but doesn't allow editing.

### DiffViewer Language Detection

The `DiffViewer` currently maps file extensions to Monaco language IDs via `getLanguage()`.
The CM6 version maps to `LanguageSupport` extensions:

```typescript
function getDiffLanguageExtension(filePath: string): Extension | null {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, () => Extension> = {
    json: () => json(),
    js: () => javascript(),
    ts: () => javascript({ typescript: true }),
    tsx: () => javascript({ typescript: true, jsx: true }),
    jsx: () => javascript({ jsx: true }),
    md: () => markdown(),
    yaml: () => yaml(),
    yml: () => yaml(),   // ← now gets proper syntax highlighting!
    xml: () => xml(),
    html: () => html(),
    css: () => css(),
  };
  const factory = map[ext];
  return factory ? factory() : null;
}
```

---

## Theme Integration

The `rocketTheme` and `rocketThemeDark` from the SingleLineEditor (`theme.ts`) already
define all variable token CSS classes (`.cm-var-environment`, `.cm-var-unresolved`, etc.).
The multi-line editor reuses the same theme extensions.

Additional multi-line-specific theme rules needed:

```typescript
const multiLineThemeExtras = EditorView.theme({
  '&': {
    fontSize: '13px',
    fontFamily: "'JetBrains Mono', monospace",
  },
  '.cm-scroller': {
    overflow: 'auto',  // multi-line allows scrolling (unlike single-line)
  },
  '.cm-content': {
    padding: '8px 0',  // match Monaco's padding.top/bottom: 8
  },
  '.cm-gutters': {
    borderRight: '1px solid hsl(var(--border))',
    background: 'hsl(var(--background))',
  },
  '.cm-lineNumbers .cm-gutterElement': {
    padding: '0 8px 0 16px',
    minWidth: '40px',
    color: 'hsl(var(--muted-foreground))',
  },
  '.cm-activeLine': {
    background: 'hsl(var(--primary) / 0.05)',
  },
  '.cm-foldGutter .cm-gutterElement': {
    padding: '0 4px',
  },
  // Variable hover tooltip
  '.cm-var-hover': {
    padding: '6px 10px',
    fontSize: '12px',
    lineHeight: '1.5',
    fontFamily: 'var(--font-sans)',
    color: 'hsl(var(--foreground))',
    background: 'hsl(var(--popover))',
    border: '1px solid hsl(var(--border))',
    borderRadius: '6px',
    boxShadow: '0 2px 8px rgba(0,0,0,0.1)',
    maxWidth: '300px',
  },
});
```

Dark mode works automatically via CSS custom properties.

---

## Files Created / Modified / Deleted

### New Files

| Path | Responsibility |
|---|---|
| `src/components/editor/MultiLineEditor.tsx` | CM6 multi-line editor with language detection, variable highlighting, hover tooltips |
| `src/components/editor/extensions/language-detect.ts` | `getLanguageExtension()` + `detectLanguage()` — maps identifiers to CM6 language extensions |
| `src/components/editor/extensions/variable-hover.ts` | `hoverTooltip` extension for `{{var}}` info display in multi-line editors |
| `src/components/editor/extensions/multi-line-theme.ts` | Multi-line-specific theme additions (gutters, active line, hover tooltip) |
| `src/components/git/CM6DiffViewer.tsx` | React wrapper for `@codemirror/merge` `MergeView` |
| `src/components/editor/__tests__/MultiLineEditor.test.tsx` | Component tests |
| `src/components/git/__tests__/CM6DiffViewer.test.tsx` | Diff viewer tests |

### Modified Files

| Path | Change |
|---|---|
| `package.json` | Add `@codemirror/merge`, `@codemirror/lang-json`, `@codemirror/lang-xml`, `@codemirror/lang-html`, `@codemirror/lang-javascript`, `@codemirror/lang-css`, `@codemirror/lang-yaml`, `@codemirror/language`, `@codemirror/search`, `codemirror` (basicSetup). Remove `monaco-editor`, `@monaco-editor/react`. |
| `src/components/request/BodyEditor.tsx` | Replace lazy `MonacoWrapper` with lazy `MultiLineEditor` |
| `src/components/response/ResponseBodyViewer.tsx` | Replace lazy `MonacoWrapper` with lazy `MultiLineEditor` (read-only) |
| `src/components/git/DiffViewer.tsx` | Replace Monaco `DiffEditor` with `CM6DiffViewer` |
| `src/components/git/ConflictResolver.tsx` | Replace Monaco `Editor` with `MultiLineEditor` |
| `src/main.tsx` | Remove all Monaco setup (workers, loader, themes). Make `ReactDOM.createRoot` synchronous. |
| `src/components/editor/EditorSkeleton.tsx` | Remove Monaco-specific colors, use CSS variables |

### Deleted Files

| Path | Reason |
|---|---|
| `src/components/editor/MonacoWrapper.tsx` | Replaced by `MultiLineEditor` |
| `src/components/editor/monaco-config.ts` | No longer needed — config lives in CM6 extensions |
| `src/components/editor/useMonacoTheme.ts` | No longer needed — CM6 theme uses CSS variables, no per-instance sync |

### Preserved / Reused from SingleLineEditor Work

| Path | What's Reused |
|---|---|
| `src/components/editor/extensions/variable-context-facet.ts` | Same facet for `MultiLineEditor` variable highlighting |
| `src/components/editor/extensions/variable-highlight.ts` | Same `ViewPlugin` decorates `{{var}}` in multi-line bodies |
| `src/components/editor/extensions/theme.ts` | `rocketTheme` + `rocketThemeDark` — base theme + variable CSS classes |
| `src/components/editor/extensions/variable-popover.ts` | `findVarTokenAt()` reused by the hover tooltip |
| `src/lib/url-variables.ts` | `VariableScopeEntry`, `VariableSource`, `sourceBadgeClass()`, `buildScopedContext()` |

---

## Implementation Phases

### Phase D: MultiLineEditor + Body/Response Migration

Build the multi-line editor component and migrate the two body editing call sites.

1. Install CM6 language packages + `@codemirror/merge` + `@codemirror/search` + `codemirror`
2. `language-detect.ts` — language ID → CM6 `LanguageSupport` mapper
3. `multi-line-theme.ts` — gutter, active line, hover tooltip styles
4. `variable-hover.ts` — `hoverTooltip` for `{{var}}` read-only display
5. `MultiLineEditor.tsx` — React wrapper with controlled value, language, variable support
6. Migrate `BodyEditor.tsx` — replace lazy `MonacoWrapper` with lazy `MultiLineEditor`
7. Migrate `ResponseBodyViewer.tsx` — replace lazy `MonacoWrapper` with lazy `MultiLineEditor`

### Phase E: DiffViewer + ConflictResolver Migration

Replace the Git-specific Monaco instances.

1. `CM6DiffViewer.tsx` — React wrapper around `MergeView` from `@codemirror/merge`
2. Migrate `DiffViewer.tsx` — replace `<DiffEditor>` with `<CM6DiffViewer>`
3. Migrate `ConflictResolver.tsx` — replace Monaco `<Editor>` with `<MultiLineEditor>`

### Phase F: Monaco Removal + Cleanup

Remove Monaco entirely from the project.

1. Delete `MonacoWrapper.tsx`, `monaco-config.ts`, `useMonacoTheme.ts`
2. Clean up `main.tsx` — remove all worker imports, `MonacoEnvironment`, `loader.config()`,
   theme pre-definition. Make `ReactDOM.createRoot()` synchronous.
3. Remove `monaco-editor` and `@monaco-editor/react` from `package.json`
4. Update `EditorSkeleton.tsx` — replace hardcoded Monaco colors with CSS variables
5. Run `yarn install` to prune Monaco from `node_modules`
6. Verify build size reduction
7. Final smoke test of all editor surfaces

---

## Bundle Size Impact

### Before (Monaco)

| Package | Approx gzipped |
|---|---|
| `monaco-editor` | ~1,500KB |
| `@monaco-editor/react` | ~12KB |
| 5 web workers (JSON, CSS, HTML, TS, editor) | ~200KB total |
| **Total Monaco** | **~1,712KB** |

### After (CM6 — including SingleLineEditor packages)

| Package | Approx gzipped |
|---|---|
| `@codemirror/state` | ~12KB |
| `@codemirror/view` | ~25KB |
| `@codemirror/autocomplete` | ~8KB |
| `@codemirror/commands` | ~3KB |
| `@codemirror/language` | ~12KB |
| `@codemirror/search` | ~5KB |
| `@codemirror/merge` | ~15KB |
| `@codemirror/lang-json` | ~5KB |
| `@codemirror/lang-xml` | ~5KB |
| `@codemirror/lang-html` | ~8KB |
| `@codemirror/lang-javascript` | ~12KB |
| `@codemirror/lang-css` | ~8KB |
| `@codemirror/lang-yaml` | ~4KB |
| `codemirror` (basicSetup) | ~2KB |
| **Total CM6** | **~124KB** |

**Net reduction: ~1,588KB gzipped (~93% smaller editor stack).**

No web workers needed — CM6 runs entirely in the main thread.

---

## Performance Considerations

### Main Thread Parsing

Monaco offloads JSON/JS/CSS/HTML parsing to web workers. CM6 uses Lezer parsers in the main
thread. For typical API request bodies (<10KB), this is imperceptible. For very large response
bodies (>1MB), incremental parsing may cause brief jank on initial load.

Mitigation: Lezer parsers are incremental — they only parse visible viewport + a buffer.
For large read-only responses, this is actually faster than Monaco's approach of parsing
the entire document upfront in a worker.

### Lazy Loading

Both `BodyEditor` and `ResponseBodyViewer` currently lazy-load `MonacoWrapper` via
`React.lazy()`. The same pattern applies to `MultiLineEditor`. The language packages
can also be dynamically imported for further code splitting:

```typescript
const langModule = await import(`@codemirror/lang-${langId}`);
```

This is a future optimization — for launch, static imports are simpler and the total
CM6 bundle is small enough that splitting isn't critical.

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| JSON linting (red squiggles for invalid JSON) missing | Medium | Medium | `@codemirror/lang-json` exports `jsonParseLinter()`; wire into `@codemirror/lint` |
| Large response body jank (>1MB) | Low | Medium | Lezer's incremental viewport parsing; test with large payloads |
| DiffViewer visual differences from Monaco DiffEditor | Medium | Low | `@codemirror/merge` is mature (v6.12.1); visual regression test |
| `main.tsx` sync boot — hidden dependency on async Monaco init | Low | High | Test app boots without the `.then()` wrapper |
| Loss of TypeScript IntelliSense in body editor | N/A | N/A | Never used — body editor is for JSON/XML/text, not code |
| CM6 `MergeView` scroll sync behavior differs from Monaco | Low | Low | Test with various diff sizes; `MergeView` has its own scroll sync |

---

## Testing Strategy

### Unit Tests

- `language-detect.test.ts` — given bodyMode/contentType, verify correct language extension
- `variable-hover.test.ts` — given cursor position in `{{var}}`, verify tooltip content

### Component Tests

- `MultiLineEditor.test.tsx` — renders with various languages, fires onChange, respects readOnly,
  shows variable highlighting when context provided
- `CM6DiffViewer.test.tsx` — renders with old/new content, shows diff highlighting

### Integration Tests (Manual)

- JSON body: type `{ "key": {{var}} }` → syntax highlighting + variable badge visible
- XML body: proper tag highlighting, folding works
- Response viewer: large JSON response pretty-prints correctly, read-only enforced
- Git diff: open changed `.yml` file → side-by-side diff with YAML highlighting
- Conflict resolver: "Edit manually" → editable editor, "Save resolution" works
- Dark mode: switch theme → all editors update immediately
- Ctrl+F: search works in body editor and response viewer
- Undo/redo: Ctrl+Z / Ctrl+Shift+Z works in body editor

---

## Out of Scope

| Item | Reason |
|---|---|
| JSON Schema validation | Monaco provided this via workers; CM6 equivalent (`@codemirror/lint`) is a future enhancement |
| TypeScript/JavaScript IntelliSense in body editor | Never used — body editor is for data formats, not code |
| GraphQL language support | Not currently implemented; add when SP3 scripting introduces GraphQL body mode |
| Three-way merge view | `@codemirror/merge` supports it but current ConflictResolver uses ours/theirs split, not three-way |
| Dynamic language package loading | Future optimization — static imports are fine for launch |
