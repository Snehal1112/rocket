# Tests Tab Authoring Assistance

**Date:** 2026-05-21
**Status:** Approved
**Scope:** Snippets sidebar + Monaco IntelliSense for the Tests sub-tab inside the Scripts tab in RocketApi.
**Out of scope:** Test results display, inline gutter markers, sub-tab badges, run-tests-only button.

---

## 1. Goals

- Help users discover what functions are available when writing tests in the Tests sub-tab.
- Provide quick-start snippet templates for common test patterns (task-oriented).
- Provide full API reference snippets for `res.*`, `rok.*`, and `expect` (developer-oriented).
- Provide Monaco IntelliSense (autocomplete + hover docs) scoped to the active script phase.

---

## 2. Layout

The Tests sub-tab changes from a full-width Monaco editor to a two-column layout:

```
┌─────────────────────────────────┬────────────────────┐
│                                 │  SNIPPETS SIDEBAR  │
│       Monaco JS editor          │                    │
│                                 │  Common Tests      │
│                                 │  ─────────────     │
│                                 │  > Status is 200   │
│                                 │  > Has property    │
│                                 │  > Response time   │
│                                 │  > Body equals     │
│                                 │                    │
│                                 │  API Reference     │
│                                 │  ─────────────     │
│                                 │  ▶ res.*           │
│                                 │  ▶ rok.*           │
│                                 │  ▶ expect          │
└─────────────────────────────────┴────────────────────┘
```

- The divider between editor and sidebar is **draggable** (resizable).
- Sidebar minimum width: **120px**. Maximum width: **50% of the panel**.
- Pre Request and Post Response sub-tabs remain full-width editors — sidebar only appears on Tests.

---

## 3. Snippets Sidebar

### 3.1 Common Tests section

Task-oriented, pre-built test blocks. Clicking inserts the full snippet at the current cursor position (appends if no active cursor).

| Label | Inserted code |
|---|---|
| Status is 200 | `test("Status is 200", () => { expect(res.getStatus()).to.equal(200); });` |
| Status is 2xx | `test("Status is 2xx", () => { expect(res.getStatus()).to.be.within(200, 299); });` |
| Response time < 200ms | `test("Response time < 200ms", () => { expect(res.getResponseTime()).to.be.below(200); });` |
| Body has property | `test("Body has property", () => { const body = res.getBody(); expect(body).to.have.property("key"); });` |
| Body equals value | `test("Body equals value", () => { const body = res.getBody(); expect(body.key).to.equal("value"); });` |
| Header exists | `test("Header exists", () => { expect(res.getHeader("content-type")).to.exist; });` |
| Status is 404 | `test("Status is 404", () => { expect(res.getStatus()).to.equal(404); });` |

### 3.2 API Reference section

Three collapsible groups. Each entry is a clickable chip that inserts the call expression at cursor (not a full test block).

**`res.*`**
- `res.getStatus()`
- `res.getStatusText()`
- `res.getHeader("name")`
- `res.getHeaders()`
- `res.getBody()`
- `res.getBody({ raw: true })`
- `res.getResponseTime()`

**`rok.*`**
- `rok.getVar("key")`
- `rok.setVar("key", value)`
- `rok.getEnvVar("key")`
- `rok.setEnvVar("key", value)`
- `rok.getCollectionVar("key")`
- `rok.getEnvName()`
- `rok.interpolate("{{template}}")`

**`expect`**
- `.to.equal(value)`
- `.to.exist`
- `.to.have.property("key")`
- `.to.be.within(min, max)`
- `.to.be.below(n)`
- `.to.include("str")`
- `.to.be.an("type")`

---

## 4. Monaco IntelliSense

A static `ROK_TYPE_DEFS` string (`.d.ts` content) is registered with Monaco via `monaco.languages.typescript.javascriptDefaults.addExtraLib()` on editor mount.

### 4.1 Phase-scoped completions

| Sub-tab | Completions active |
|---|---|
| Pre Request | `rok.*`, `req.*` |
| Post Response | `rok.*`, `res.*` |
| Tests | `rok.*`, `res.*`, `test()`, `expect` |

When the active sub-tab changes, the previous `addExtraLib` registration is disposed and the new phase's definitions are registered.

### 4.2 Type definitions coverage

The `.d.ts` covers:

- `res` global — all 7 methods with JSDoc descriptions
- `rok` global — variable API (12 methods) and runner API (3 methods) with JSDoc
- `req` global — all request mutation methods (Pre Request phase only)
- `test(name: string, fn: () => void): void` — global function
- `expect` — Chai subset: `equal`, `exist`, `property`, `within`, `below`, `include`, `be.an`, `not`

---

## 5. Component structure

### New files

| File | Purpose |
|---|---|
| `src/components/request/ScriptSnippetSidebar.tsx` | Resizable sidebar with collapsible Common Tests and API Reference sections |
| `src/components/editor/rok-types.ts` | Exports `ROK_TYPE_DEFS` (d.ts string) and `ROK_SNIPPETS` (snippet definitions array) |

### Modified files

| File | Change |
|---|---|
| `src/components/request/ScriptsTab.tsx` | Tests sub-tab renders two-column layout with `ScriptSnippetSidebar`; passes `onInsert` handler to editor |
| `src/components/editor/MonacoWrapper.tsx` | Accepts optional `phase` prop; registers/disposes IntelliSense providers when phase changes |

### Key interfaces

```typescript
// rok-types.ts
export interface ScriptSnippet {
  label: string;
  code: string;
  kind: 'template' | 'expression';
}

// ScriptSnippetSidebar.tsx
interface ScriptSnippetSidebarProps {
  onInsert: (code: string) => void;
}

// MonacoWrapper.tsx (extended)
interface MonacoWrapperProps {
  // ... existing props ...
  phase?: 'pre-request' | 'post-response' | 'tests';
  onEditorReady?: (editor: monacoNs.editor.IStandaloneCodeEditor) => void;
}
```

### Insert-at-cursor mechanism

`ScriptsTab` holds a ref to the Monaco editor instance via an `onEditorReady` callback added to `MonacoWrapper`. When `onInsert(code)` is called from the sidebar, it calls `editor.executeEdits('snippet-insert', [...])` to insert at the current cursor position. If the editor has no selection, it appends with a leading newline.

---

## 6. Resize behaviour

The sidebar uses a CSS `flex` layout with a drag handle div. Drag state is managed with `onMouseDown` / `onMouseMove` / `onMouseUp` on the document. The sidebar width is stored in local component state (not persisted — resets to default 220px on remount).

---

## 7. Verification

```bash
yarn tsc --noEmit   # type-check
yarn check          # biome lint
yarn test           # vitest
```

Manual: open any request → Scripts tab → Tests sub-tab. Verify sidebar renders, snippets insert at cursor, autocomplete fires on `res.` and `rok.`, hover docs appear.
