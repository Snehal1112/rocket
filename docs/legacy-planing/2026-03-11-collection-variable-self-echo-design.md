# Collection Variable Self-Echo Design

## Summary

`pm.variables.set(...)` currently persists collection-level variables
correctly, but the frontend still issues duplicate follow-up reads for
`/api/v1/collections/:name/variables`. The main issue is that the local
save path already updates store state, while the websocket `file_change`
event for `collection.bru` immediately triggers a second read for the
same change.

This design keeps external file-change refresh behavior intact while
suppressing the frontend's own websocket echo for collection-variable
writes.

## Current Problem

For one send action where the post-response script writes one
collection-scoped variable, the relevant flow is:

1. `useRequestBuilderState.ts` calls `saveCollectionVariables()`
2. `collections.ts` persists the variable and updates local
   `collectionVariables`
3. backend file watching emits a websocket `file_change` for
   `collection.bru`
4. `App.tsx` handles that event by calling `fetchCollectionVariables()`

The final `GET /collections/:name/variables` is redundant because the
store already holds the new value from the local save.

## Goals

- Remove duplicate collection-variable reads caused by local save echoes.
- Preserve live refresh for external edits to `collection.bru`.
- Keep the fix narrow to collection-variable writes.
- Preserve current behavior for environment-variable updates.

## Non-Goals

- Redesign the entire websocket sync model.
- Add backend-origin metadata to websocket events.
- Change script execution or variable precedence semantics.

## Recommended Approach

### 1. Track short-lived local save suppression

When the frontend saves collection variables for a collection, record a
short-lived suppression marker keyed by collection name.

Required behavior:

- marker is created immediately before or after the save succeeds
- marker is valid only for a short window
- marker is consumed by one matching websocket echo
- marker is scoped to `collection.bru` changes only

### 2. Ignore only the matching websocket echo

In `App.tsx`, when a websocket `file_change` arrives for
`collection.bru`:

- if it matches a recent local collection-variable save for the same
  collection, suppress the refetch
- otherwise, call `fetchCollectionVariables()` as today

This preserves external live updates while avoiding duplicate local
reads.

### 3. Leave other file-change handling intact

Other websocket-driven refresh paths should continue to behave as they
do today:

- environment file changes still refresh environments
- request or tree file changes still refresh collections/tree as needed

The suppression must not affect those paths.

## Expected Request Pattern

For one send where `pm.variables.set(...)` writes one collection
variable:

- one `GET /health`
- one `POST /api/v1/requests/send`
- one `GET /api/v1/history?limit=50`
- one `POST /api/v1/collections/:name/variables`
- no websocket-echo `GET /api/v1/collections/:name/variables`

For an external edit to `collection.bru`:

- websocket `file_change` still triggers one
  `GET /api/v1/collections/:name/variables`

## Error Handling

The suppression marker should only be registered for successful local
saves. If saving collection variables fails, websocket handling should
remain unchanged.

If the websocket echo does not arrive within the short suppression
window, the marker should expire automatically without affecting later
external edits.

## Testing

Add focused frontend tests for:

- registering suppression on successful `saveCollectionVariables()`
- skipping `fetchCollectionVariables()` for one matching websocket echo
- allowing refetch after the suppression window expires
- allowing refetch for a different collection
- allowing refetch for non-`collection.bru` file changes

Manual verification should repeat the reported send scenario and confirm
duplicate `variables` requests disappear while environment behavior
remains unchanged.
