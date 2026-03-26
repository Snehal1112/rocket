# Path/Query Token Highlighting — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Extend the URL input overlay to highlight `:pathParam` tokens and `?query=value` segments alongside the existing `{{variable}}` highlights.

## Current State

- `VariableAwareUrlInput` highlights `{{variable}}` tokens with colored overlays
- `parseUrlTokens()` in `url-variables.ts` only recognizes `{{var}}` patterns
- Path params (`:id`) and query strings (`?key=value`) are not visually distinguished in the URL input

## Feature

Extend the token parser to recognize 3 token types:

| Token | Pattern | Example | Style |
|---|---|---|---|
| Variable | `{{name}}` | `{{host}}` | `bg-primary/15 text-primary` (existing) |
| Path param | `:name` (between `/` delimiters) | `:userId` | `bg-violet-500/15 text-violet-500` |
| Query key | `key=value` after `?` or `&` | `page=1` | `bg-amber-500/15 text-amber-500` (key only) |

### Token resolution

- **Variables**: Resolved against active environment (existing behavior)
- **Path params**: Resolved against `request.pathParams[]` entries
- **Query keys**: Resolved against `request.queryParams[]` entries

Resolved tokens show their value on hover. Unresolved `:pathParam` tokens show a warning style (destructive tint).

## Architecture

### 1. Extend `parseUrlTokens` in `src/lib/url-variables.ts`

Add new token types to `UrlToken`:
```ts
type: 'text' | 'variable' | 'pathParam' | 'queryKey' | 'queryValue'
```

The parser runs in order:
1. First pass: split on `{{var}}` patterns (existing)
2. Second pass: within text segments, split on `:paramName` in path portion
3. Third pass: within text segments after `?`, split on `key=value` pairs

### 2. Update VariableAwareUrlInput

Accept optional `pathParams` and `queryParams` props to resolve path/query tokens. Render additional token styles for the new types.

### 3. Wire props in RequestPanel

Pass `request.pathParams` and `request.queryParams` to `VariableAwareUrlInput`.

## Files Changed

| File | Changes |
|---|---|
| `src/lib/url-variables.ts` | Extend parser for path params and query tokens |
| `src/components/request/VariableAwareUrlInput.tsx` | Add pathParams/queryParams props, render new token styles |
| `src/components/request/RequestPanel.tsx` | Pass pathParams/queryParams to VariableAwareUrlInput |

## Out of Scope

- Editing path/query params from the hover popover (too complex, use the Params tab)
- Auto-adding path params to the params list when typed
- URL encoding/decoding visualization
