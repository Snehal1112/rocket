# Git Operation Error Surface

**Date:** 2026-04-03  
**Status:** Approved  
**Scope:** `src/components/git/GitLandingPanel.tsx` only

## Overview

Three UX gaps in `GitLandingPanel` cause push/pull failures to be invisible or confusing:

1. The git store sets `error` on every failed push/pull/fetch, but `GitLandingPanel` never reads or displays it — errors are silently swallowed.
2. The Push button is enabled even when the repo is in merge-in-progress state (conflicted files), allowing the user to trigger a `NotFastForward` push that was always going to fail.
3. Pull internally performs a fetch, but `lastFetched` is only updated via the Fetch button — so the "Fetch Before Push" dialog appears immediately after a successful pull.

## Affected Files

| File | Change |
|------|--------|
| `src/components/git/GitLandingPanel.tsx` | Read `error` and `status` from store; show inline error alert; disable Push when `hasConflicts`; set `lastFetched` after pull |

## Section 1: Inline Error Alert

Read `error` from `useGitStore`. When non-null, render a dismissible inline alert below the Fetch/Pull/Push button row:

```tsx
{error && (
  <div className="flex items-start gap-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive">
    <AlertCircle className="h-3.5 w-3.5 shrink-0 mt-0.5" />
    <span className="flex-1 break-words">{error}</span>
    <button
      className="shrink-0 hover:opacity-70"
      onClick={() => clearError()}
    >
      ×
    </button>
  </div>
)}
```

`clearError` calls `useGitStore.getState().set({ error: null })` — or more precisely, add a `clearError` action to the store that sets `error: null`. Each new push/pull/fetch operation clears the error before starting (already happens implicitly since the store overwrites `error` on each `set({ error: ... })` call, but it should also clear on success by resetting error to null after `refreshStatus()`).

`AlertCircle` is already imported in `GitLandingPanel`.

## Section 2: Disable Push During Merge

Add `status` to the `useGitStore` destructure in `GitLandingPanel`. Derive:

```ts
const hasConflicts = (status?.files.some((f) => f.status === "conflicted")) ?? false;
```

On the Push button:
- Add `disabled={pushing || hasConflicts}`

No tooltip is needed — the in-merge banner already visible in `GitPanel`'s left panel explains the conflict state.

## Section 3: Set lastFetched After Pull

In `handlePull`, after a successful `await pull()`, set `lastFetched`:

```ts
await pull();
setLastFetched(new Date().toLocaleTimeString());
```

This prevents the "Fetch Before Push" dialog from appearing immediately after a pull (which internally fetches).

## Store Change: clearError action

Add `clearError: () => void` to the git store that sets `error: null`. Also, set `error: null` at the start of each push/pull/fetch operation (before the try block) so a stale error from a previous operation doesn't persist while a new one is in flight.

## Out of Scope

- Changes to `GitPanel.tsx` — error display lives in `GitLandingPanel` alongside the action buttons.
- Toast notifications — inline alert is sufficient and consistent with the existing UI.
- Errors from staging/unstaging/discarding files — those are shown elsewhere or handled separately.
- Fetch does not need `lastFetched` changes — it already sets it.
