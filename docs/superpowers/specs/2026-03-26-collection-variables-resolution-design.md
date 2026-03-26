# Collection Variables Resolution — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Wire collection-level variables into the `{{var}}` resolution system so they resolve at send time alongside environment variables, with environment taking precedence.

## Current State

- `CollectionVariable` type exists: `{ key, value, initialValue, enabled, secret }`
- `CollectionSettings` includes `variables: CollectionVariable[]`
- Tauri API: `getCollectionSettings(name)` reads variables from `collection.json`
- `CollectionOverviewTab` loads/saves collection variables via the settings API
- `useEnvStore.resolveVariables(text)` resolves ONLY environment variables
- `sendRequest()` in `execute-request.ts` uses `resolveVariables` but has no awareness of collection variables
- `VariableAwareUrlInput` highlights `{{var}}` tokens using only environment variables

## Variable Precedence

```
{{var}} resolution order:
1. Environment variable (active env) — wins if both exist
2. Collection variable (enabled, value field) — fallback
3. Unresolved — left as {{var}} literal
```

Only `enabled: true` variables participate. The `value` field is used (not `initialValue`).

## Section 1 — Resolution in execute-request.ts

### Current flow

```
sendRequest(tabId, request)
  → resolve = useEnvStore.getState().resolveVariables
  → resolvedUrl = resolve(request.url)
  → resolvedHeaders = resolve(...)
  → resolvedBody = resolve(...)
  → resolvedAuth = resolve(...)
  → executeRequest(...)
```

### New flow

```
sendRequest(tabId, request)
  → look up tab.source?.collection via findTabInTree
  → if collection exists: await getCollectionSettings(collection) → collectionVars
  → build merged vars: collection vars (lower), env vars (higher)
  → resolve = (text) => text.replace(/\{\{([\w.-]+)\}\}/g, ...)
  → rest unchanged
```

The merged resolution replaces `useEnvStore.resolveVariables` with a local function that checks env vars first, then collection vars.

## Section 2 — VariableAwareUrlInput update

The overlay needs to know about collection variables for accurate highlighting. A variable resolved from the collection (but not in the env) should show as resolved with source "Collection" instead of the environment name.

### Changes

- Accept new optional prop: `collectionName?: string`
- If `collectionName` is provided, fetch collection settings (or accept pre-fetched variables)
- Merge collection variables into the resolution map (env wins on conflict)
- Pass merged map + source labels to `parseUrlTokens`

### Approach

To avoid an async fetch inside the component, accept `collectionVariables` as an optional prop passed from `RequestPanel` (which knows the collection name and can fetch settings).

Update `parseUrlTokens` to accept an optional second variables map with a different source label.

## Section 3 — RequestPanel wiring

Pass `tab.source?.collection` context to `VariableAwareUrlInput`. Fetch collection variables once when the tab loads (or when `tab.source` changes) and pass them down.

## Files Changed

| File | Changes |
|---|---|
| `src/lib/url-variables.ts` | Update `parseUrlTokens` to accept optional collection variables map |
| `src/lib/execute-request.ts` | Fetch collection variables, build merged resolution function |
| `src/components/request/VariableAwareUrlInput.tsx` | Accept `collectionVariables` prop, merge into resolution |
| `src/components/request/RequestPanel.tsx` | Fetch collection variables, pass to VariableAwareUrlInput |

## Out of Scope

- Collection variable inheritance (nested folder scoping)
- Collection auth/headers resolution (separate feature)
- Secret variable masking in URL overlay (just hide value, show key)
- Creating/editing collection variables from the URL popover
