# Plan 14: Fetch-Before-Push Safety Prompt

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user clicks Push without having fetched first (or when behind the remote), show a confirmation dialog suggesting they fetch first to avoid conflicts.

**Architecture:** The check lives in `GitLandingPanel`'s push handler. It uses the existing `lastFetched` local state (null means never fetched this session) and `status.behind` to determine if a fetch-first prompt is needed. An `AlertDialog` offers "Fetch & Push" (auto-fetch then push), "Push Anyway", or "Cancel". No backend changes needed.

**Tech Stack:** React, TypeScript, shadcn/ui AlertDialog, Zustand

---

## Chunk 1: Fetch-before-push prompt

### Task 1: Add fetch-before-push confirmation dialog to `GitLandingPanel`

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx`

- [ ] **Step 1: Add dialog state**

In `src/components/git/GitLandingPanel.tsx`, add a new state variable after `showStashDialog` (line 33):

```typescript
const [showFetchFirstDialog, setShowFetchFirstDialog] = useState(false);
```

- [ ] **Step 2: Update `handlePush` to check fetch state**

Replace the current `handlePush` function (lines 89-97):

```typescript
  const handlePush = async () => {
    const { credentials } = useGitStore.getState();
    if (!credentials) {
      push();
      return;
    }

    // Suggest fetching first if never fetched this session or behind remote.
    const { status: currentStatus } = useGitStore.getState();
    if (!lastFetched || (currentStatus && currentStatus.behind > 0)) {
      setShowFetchFirstDialog(true);
      return;
    }

    setPushing(true);
    try { await push(); } finally { setPushing(false); }
  };
```

- [ ] **Step 3: Add "Fetch & Push" and "Push Anyway" handlers**

Add these two functions after `handlePush`:

```typescript
  const handleFetchAndPush = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      await fetch();
      setLastFetched(new Date().toLocaleTimeString());
      // Re-check status after fetch — if now behind, abort push.
      const { status: freshStatus } = useGitStore.getState();
      if (freshStatus && freshStatus.behind > 0) {
        setPushing(false);
        return;
      }
      await push();
    } finally {
      setPushing(false);
    }
  };

  const handlePushAnyway = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try { await push(); } finally { setPushing(false); }
  };
```

- [ ] **Step 4: Add the AlertDialog JSX**

Add a second `AlertDialog` after the existing auto-stash dialog (before the closing `</div>` of the component, around line 186):

```tsx
      {/* Fetch-before-push confirmation dialog. */}
      <AlertDialog open={showFetchFirstDialog} onOpenChange={setShowFetchFirstDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Fetch Before Push</AlertDialogTitle>
            <AlertDialogDescription>
              {(status?.behind ?? 0) > 0
                ? `Your branch is ${status?.behind} commits behind the remote. Fetching first ensures you have the latest changes and reduces the risk of conflicts.`
                : 'You have not fetched from the remote yet. Fetching first ensures you have the latest changes and reduces the risk of conflicts.'}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePushAnyway}>Push Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleFetchAndPush}>Fetch & Push</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 6: Verify the app builds**

Run: `yarn build`
Expected: builds successfully

- [ ] **Step 7: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "feat(frontend): add fetch-before-push safety prompt"
```
