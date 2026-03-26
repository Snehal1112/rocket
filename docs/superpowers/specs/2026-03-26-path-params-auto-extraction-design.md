# Path Params Auto-Extraction — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Auto-extract `:paramName` from the URL into the Path Params list, sync bidirectionally, and substitute at send time — matching Postman/Bruno behavior.

## Current Problems

1. Typing `:userId` in URL highlights it but shows "unresolved" in red (no value)
2. Path params are only populated via manual "Add Path Param" button
3. `extractPathParams()` exists in `url-params.ts` but is never called
4. Path params are never substituted into the URL at send time

## Fix 1 — Auto-extract path params from URL

In `RequestPanel.handleUrlChange`, after syncing query params, also extract `:param` names from the URL and sync `request.pathParams`:

- **Add** entries for new params found in URL (key=paramName, value='', enabled=true)
- **Keep** existing entries whose key is still in the URL (preserve their user-entered values)
- **Remove** entries whose key is no longer in the URL

Uses the existing `extractPathParams(url)` function from `url-params.ts`.

## Fix 2 — Substitute path params at send time

In `execute-request.ts`, after resolving `{{variables}}` in the URL, also replace `:paramName` patterns with their values from `request.pathParams` (only enabled entries with non-empty values).

## What Already Works (no changes needed)

- Token highlighting in URL overlay — correctly shows `:id` tokens, just needs populated `pathParamMap`
- PathParamsPanel — renders correctly with KeyValueEditor
- Hover tooltip — shows `paramName = value` when `resolved` field is set

## Files Changed

| File | Changes |
|---|---|
| `src/components/request/RequestPanel.tsx` | Add path param auto-extraction in `handleUrlChange` |
| `src/lib/execute-request.ts` | Substitute `:paramName` in URL at send time |

## Out of Scope

- `{paramName}` brace-style path params (only `:paramName` colon-style)
- Path param autocomplete suggestions
- Path param validation (e.g., required params)
