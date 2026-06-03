# Post Response Snippet Sidebar

**Date:** 2026-06-03
**Status:** Approved

## Summary

Add a snippet sidebar to the Post Response tab in `ScriptsTab.tsx`, giving users quick access to common post-processing patterns and the `res.*` / `rok.*` API reference — the same authoring-assistance experience that the Tests tab provides.

## Background

The Tests tab (`phase='tests'`) already renders a `ScriptSnippetSidebar` with two groups:
- **Common Tests** — full `test()` block templates
- **API Reference** — `res.*`, `rok.*`, and `expect` expression snippets

The Post Response tab (`phase='post-response'`) has a Monaco editor with IntelliSense (`ROK_DEFS + RES_DEFS`) but no snippet sidebar. Users writing post-response scripts (extracting tokens, setting variables from responses) have no discoverability aid.

## Design

### Layout

The sidebar is **hidden by default** and toggled by a "Snippets" button at the right end of the tab bar, visible only when the Post Response tab is active. This gives the editor full width until the user explicitly opens the sidebar.

Toggle state is local (`useState<boolean>`, initialised to `false`) — it resets to closed every time the component mounts. No persistence between tab switches.

When open, the sidebar is drag-resizable and sits to the right of the Monaco editor, identical mechanics to the Tests sidebar.

### Snippet Content

A new `POST_RESPONSE_SNIPPETS: ScriptSnippetGroup[]` export is added to `rok-types.ts`. It does not replace or modify `ROK_SNIPPETS`.

**Group 1 — Common Patterns** (`kind: 'template'`)

| Label | Code |
|---|---|
| Save body field to env var | `const value = res.getBody().field;\nrok.setEnvVar("key", value);` |
| Save header to env var | `const value = res.getHeader("header-name");\nrok.setEnvVar("key", value);` |
| Log response body | `console.log(res.getBody());` |
| Set collection var from body | `const value = res.getBody().field;\nrok.setCollectionVar("key", value);` |
| Set var only if 2xx | `if (res.getStatus() >= 200 && res.getStatus() < 300) {\n  rok.setEnvVar("key", res.getBody().field);\n}` |

**Group 2 — API Reference** (`kind: 'expression'`)

Reuses the `res.*` and `rok.*` sub-groups verbatim from `ROK_SNIPPETS`. The `expect` sub-group is excluded (not relevant outside tests).

### Component Changes

**`rok-types.ts`**
- Add `POST_RESPONSE_SNIPPETS` const (no changes to existing exports).

**`ScriptSnippetSidebar.tsx`**
- Add optional `snippets` prop (`ScriptSnippetGroup[]`, defaults to `ROK_SNIPPETS`).
- No structural or behavioural changes.

**`ScriptsTab.tsx`**
- Add `showPostResponseSidebar` state (`useState(false)`).
- Add a "Snippets" toggle button inside `TabsList`, styled consistently with existing triggers, shown only when on the post-response tab.
- The Post Response `TabsContent` renders `<ScriptSnippetSidebar snippets={POST_RESPONSE_SNIPPETS} onInsert={handleInsert} />` conditionally when `showPostResponseSidebar` is true.
- The existing `editorRef` / `handleEditorReady` / `handleInsert` are shared — the Post Response editor passes `onEditorReady={handleEditorReady}` (same as Tests).

### What Does Not Change

- Pre Request tab: no sidebar, unchanged.
- Tests tab: always-visible sidebar with `ROK_SNIPPETS`, unchanged.
- Monaco IntelliSense: `phase='post-response'` already provides `ROK_DEFS + RES_DEFS` — no changes needed.
- Rust backend: no changes.

## Verification

```bash
yarn tsc --noEmit
yarn check
```

Manual: open Post Response tab, click Snippets button, confirm sidebar opens; click a snippet, confirm it inserts at cursor. Confirm Tests tab sidebar is unaffected.
