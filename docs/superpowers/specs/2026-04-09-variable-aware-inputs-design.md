# Variable-Aware Inputs Design

> **For agentic workers:** Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** Extend variable highlighting and popover editing to every text value input in the request panel, plus read-only highlighting in the Monaco body editor.

**Architecture:** One new `VariableAwareInput` component using the existing dual-layer overlay pattern; one new `parseTextTokens` utility for arbitrary text tokenization; `scopedContext` threaded from `RequestPanel` to all child editors; Monaco gets decorations + hover provider.

**Tech Stack:** React, TypeScript, Tailwind CSS, Radix UI Popover, Monaco Editor API (`deltaDecorations`, `registerHoverProvider`)

---

## Scope

### In scope
- `VariableAwareInput` component for single-line text value fields
- Value columns in: `KeyValueEditor` (headers, query params, form-data), `PathParamsPanel`, `AuthEditor` (all text value fields)
- Monaco raw body editor: decoration highlighting + read-only hover tooltip
- `parseTextTokens` utility in `src/lib/text-variables.ts`

### Out of scope
- Key columns in `KeyValueEditor` (header names, query param keys) — stay plain `<Input>`
- Inline editing widget in Monaco (read-only tooltip only)
- Collection overview tab headers (no full variable scope available there)
- Request/folder variables UI (already deferred separately)

---

## Variable Scope & Priority

Seven scope layers, lowest to highest priority (from `buildScopedContext` in `url-variables.ts`):

1. `process` — `process.env.*` system variables
2. `global` — global environment variables
3. `collection` — collection-level variables
4. `environment` — active environment variables
5. `folder` — folder-chain variables
6. `request` — request-level variables
7. `runtime` — set programmatically during execution

`RequestPanel` already calls `buildScopedContext()` to produce `Map<string, VariableScopeEntry>`. This same map is passed to all editors.

---

## New Files

### `src/lib/text-variables.ts`

```ts
export interface TextToken {
  type: 'text' | 'variable';
  content: string;   // raw text or variable name (without braces)
}

// Tokenize arbitrary text into plain-text and {{variable}} segments.
export function parseTextTokens(text: string): TextToken[]
```

- Regex: `/\{\{\s*([\w.]+)\s*\}\}/g` (same as `resolveWithContext`)
- Returns interleaved `text` and `variable` tokens
- Example: `"Bearer {{token}} extra"` → `[{type:'text', content:'Bearer '}, {type:'variable', content:'token'}, {type:'text', content:' extra'}]`

---

### `src/components/request/VariableAwareInput.tsx`

**Props:**
```ts
interface VariableAwareInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource, key: string) => void;
}
```

**Rendering:**
- Dual-layer: transparent `<input>` (receives keystrokes, shows caret) + absolutely-positioned overlay `<div>` (pointer-events-none except on variable tokens)
- Overlay maps `parseTextTokens(value)` to:
  - Plain text → unstyled `<span>`
  - Resolved variable → colored `<span>` via `sourceBadgeClass(entry.source)` (same palette as URL bar)
  - Unresolved variable → `bg-destructive/15 text-destructive` red styling

**Popover (per variable token, identical to URL bar):**
- Resolved value display (or "Not set" if missing)
- Source badge: C (collection), E (environment), G (global), F (folder), R (request), P (process)
- Editable `<Input>` for `environment` and `global` vars — calls env store update on blur/enter
- Secret variables shown as `●●●●`
- "→" nav link calls `onNavigateToSource(entry.source, varName)`
- Read-only display for `collection`, `folder`, `request`, `process` vars

**Fallback:** When `variableContext` is not provided, renders a plain `<Input>` with no overhead.

---

## Modified Files

### `src/components/request/KeyValueEditor.tsx`

Add optional props:
```ts
variableContext?: Map<string, VariableScopeEntry>;
onNavigateToSource?: (source: VariableSource, key: string) => void;
```

Swap value column `<Input>` → `<VariableAwareInput>` when `variableContext` is present.
Key column stays as plain `<Input>`.

### `src/components/request/HeadersEditor.tsx`

Pass `variableContext` and `onNavigateToSource` through to `KeyValueEditor`.

### `src/components/request/QueryParamsEditor.tsx`

Same pass-through as `HeadersEditor`.

### `src/components/request/PathParamsPanel.tsx`

Swap value field `<Input>` → `<VariableAwareInput>`. Add `variableContext` and `onNavigateToSource` props.

### `src/components/request/AuthEditor.tsx`

Add `variableContext` and `onNavigateToSource` props. Swap all text value `<Input>` fields → `<VariableAwareInput>`:
- Basic auth: username, password
- Bearer: token
- API Key: key, value
- OAuth2: tokenUrl, authorizationUrl, clientId, clientSecret, scope, and other text fields
- AWS SigV4: accessKey, secretKey, region, service, sessionToken

### `src/components/request/BodyEditor.tsx`

Add `variableContext` and `onNavigateToSource` props. Pass to:
- `KeyValueEditor` (form-data values)
- `MonacoWrapper` (decorations + hover)

### `src/components/editor/MonacoWrapper.tsx`

Add `variableContext?: Map<string, VariableScopeEntry>` prop.

**Decorations:**
- On mount and on `editor.onDidChangeModelContent()`: scan full document text via `parseTextTokens`
- Call `editor.deltaDecorations(oldDecorations, newDecorations)` to apply source-colored CSS classes
- Resolved vars: source color class; unresolved: red class
- Clear old decorations before applying new ones

**Hover provider:**
- Register once on mount for each supported language: `['json', 'xml', 'plaintext', 'sparql']` — one `registerHoverProvider` call per language ID
- On hover: check if cursor position falls within a `{{varName}}` token range in the model text
- Return `{ contents: [{ value: markdownString }] }` with variable name, resolved value, source label
- Secret vars shown as `●●●●` in hover content
- No nav link (Monaco hover is markdown-only; cannot trigger React callbacks)
- Store all `IDisposable` handles; dispose all on component unmount to prevent memory leaks

### `src/components/request/RequestPanel.tsx`

Pass `scopedContext` (already built) and `onNavigateToSource` (already exists) to:
- `HeadersEditor`
- `QueryParamsEditor`
- `PathParamsPanel`
- `AuthEditor`
- `BodyEditor`

No new computation — `scopedContext` is already constructed for the URL bar.

---

## Data Flow

```
RequestPanel
  scopedContext = buildScopedContext({ envVars, globalVars, collectionVars, ... })
  │
  ├── VariableAwareUrlInput      (unchanged — already receives scopedContext)
  ├── HeadersEditor → KeyValueEditor (value cells → VariableAwareInput)
  ├── QueryParamsEditor → KeyValueEditor (value cells → VariableAwareInput)
  ├── PathParamsPanel (value cells → VariableAwareInput)
  ├── AuthEditor (text value fields → VariableAwareInput)
  └── BodyEditor
        ├── KeyValueEditor form-data (value cells → VariableAwareInput)
        └── MonacoWrapper (decorations + hover provider)
```

---

## Visual Design

Variable token colors match the URL bar exactly (from `sourceBadgeClass` in `url-variables.ts`):

| Source | Color |
|---|---|
| `environment` | `bg-primary/15 text-primary` |
| `collection` | `bg-blue-500/15 text-blue-500` |
| `global` | `bg-teal-500/15 text-teal-500` |
| `folder` | `bg-amber-500/15 text-amber-500` |
| `request` | `bg-purple-500/15 text-purple-500` |
| `process` | `bg-muted text-muted-foreground` |
| `runtime` | `bg-orange-500/15 text-orange-500` |
| unresolved | `bg-destructive/15 text-destructive` |

---

## Testing

- Unit test `parseTextTokens` in `src/lib/__tests__/text-variables.test.ts`:
  - Plain text with no variables
  - Single variable
  - Multiple variables
  - Unresolved variable (not in context)
  - Variable with whitespace in braces: `{{ var }}`
  - Process env variable: `{{process.env.KEY}}`

- Unit test `VariableAwareInput` in `src/components/request/__tests__/VariableAwareInput.test.tsx`:
  - Renders plain input when `variableContext` is undefined
  - Renders colored span for resolved variable
  - Renders red span for unresolved variable
  - Popover shows resolved value and source badge
  - Popover allows editing for `environment` source
  - Popover is read-only for `collection` source
  - Secret variables shown as `●●●●`

- Monaco decorations are not unit-testable in jsdom — manual verification only.
