# Design: Close All Tabs on Workspace Switch

**Date:** 2026-03-28
**Status:** Approved

## Problem

Switching workspaces leaves tabs from the previous workspace open. Those tabs reference collections and requests that belong to the old workspace, which is confusing and can cause stale state. All open tabs should be closed when the user switches workspaces, with dirty (unsaved) tabs auto-saved first.

## Scope

Two files, ~15 lines of new code:

- `src/stores/pane-store.ts`
- `src/stores/workspace-store.ts`

## Design

### `pane-store.ts` — add `closeAll()`

Add `closeAll()` to the `PaneState` interface and implement it in the store.

The implementation:
1. Walks the current pane tree and collects every tab across all leaf groups.
2. For each tab that is dirty and is a request tab with a `source`, calls `scheduleAutoSave` (identical to the logic already in `closeTab`).
3. Calls the existing `reset()` to collapse the pane tree to a single empty leaf.

This keeps all auto-save and pane-reset logic inside the store that owns it. No pane internals leak to callers.

### `workspace-store.ts` — call `closeAll()` on workspace switch

The existing `workspace-switched` event listener sets `activeWorkspaceId`. Add a call to `usePaneStore.getState().closeAll()` in that handler before updating the active workspace id.

```
user switches workspace
  → backend emits workspace-switched
  → workspace-store listener fires
  → closeAll() auto-saves dirty tabs, resets pane tree to empty leaf
  → activeWorkspaceId updated in workspace-store
```

## Error Handling

`scheduleAutoSave` is fire-and-forget, consistent with the existing behavior in `closeTab`. A failed auto-save does not block the pane reset.

## Testing

- TypeScript check (`yarn tsc --noEmit`) must pass.
- Manual E2E: open requests in tabs, make edits to create dirty state, switch workspace, verify tabs close and dirty changes are persisted to disk.
