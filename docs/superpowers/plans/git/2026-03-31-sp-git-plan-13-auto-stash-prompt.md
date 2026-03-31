# Plan 13: Auto-Stash Prompt Before Pull

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user clicks Pull with uncommitted changes, show a confirmation dialog offering to stash changes first, preventing data loss.

**Architecture:** The check lives in `GitLandingPanel`'s pull handler. Before calling `pull()`, it checks `status.isClean`. If dirty, it shows an `AlertDialog` with three options: "Stash & Pull" (auto-stash, pull, then pop), "Pull Anyway" (proceed without stashing), or "Cancel". No backend changes needed — all existing store actions (`saveStash`, `pull`, `popStash`) are reused.

**Tech Stack:** React, TypeScript, shadcn/ui AlertDialog, Zustand

---

## Chunk 1: Auto-stash prompt

### Task 1: Add auto-stash confirmation dialog to `GitLandingPanel`

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx`

- [ ] **Step 1: Add AlertDialog imports and stash store actions**

Add these imports to the top of `src/components/git/GitLandingPanel.tsx`:

```typescript
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
```

Update the store destructuring (line 17):

```typescript
// Old
const { status, push, pull, fetch } = useGitStore();

// New
const { status, push, pull, fetch, saveStash, popStash } = useGitStore();
```

Add a state variable for the dialog:

```typescript
const [showStashDialog, setShowStashDialog] = useState(false);
```

- [ ] **Step 2: Update `handlePull` to check for dirty state**

Replace the current `handlePull` function:

```typescript
  const handlePull = async () => {
    const { credentials } = useGitStore.getState();
    if (!credentials) {
      pull();
      return;
    }

    // Check if working tree has uncommitted changes.
    const { status: currentStatus } = useGitStore.getState();
    if (currentStatus && !currentStatus.isClean) {
      setShowStashDialog(true);
      return;
    }

    setPulling(true);
    try { await pull(); } finally { setPulling(false); }
  };
```

- [ ] **Step 3: Add "Stash & Pull" and "Pull Anyway" handlers**

Add two new handler functions after `handlePush`:

```typescript
  const handleStashAndPull = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await saveStash('Auto-stash before pull');
      await pull();
      await popStash(0);
    } catch {
      // If pop fails (conflict), stash is preserved — user can manually apply.
    } finally {
      setPulling(false);
    }
  };

  const handlePullAnyway = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try { await pull(); } finally { setPulling(false); }
  };
```

- [ ] **Step 4: Add the AlertDialog to the JSX**

Add the dialog just before the closing `</div>` of the component (before line 131):

```tsx
      {/* Auto-stash confirmation dialog. */}
      <AlertDialog open={showStashDialog} onOpenChange={setShowStashDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Uncommitted Changes</AlertDialogTitle>
            <AlertDialogDescription>
              You have uncommitted changes. Pulling may cause conflicts or data loss. Would you like to stash your changes first?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="outline" onClick={handlePullAnyway}>
              Pull Anyway
            </AlertDialogAction>
            <AlertDialogAction onClick={handleStashAndPull}>
              Stash & Pull
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
```

**Note:** If `AlertDialogAction` doesn't accept a `variant` prop (it depends on the shadcn/ui setup), use a regular `Button` wrapped in the dialog footer instead:

```tsx
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <Button variant="outline" onClick={handlePullAnyway}>Pull Anyway</Button>
            <AlertDialogAction onClick={handleStashAndPull}>Stash & Pull</AlertDialogAction>
          </AlertDialogFooter>
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

If `AlertDialogAction` doesn't accept `variant`, switch to the `Button` approach shown above.

- [ ] **Step 6: Verify the app builds**

Run: `yarn build`
Expected: builds successfully

- [ ] **Step 7: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "feat(frontend): add auto-stash confirmation dialog before pull with dirty state"
```
