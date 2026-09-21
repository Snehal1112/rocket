# Git Credentials Saving State and Conflict Selector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `GitCredentialsDialog`'s Connect button a real saving/disabled state, and remove the duplicated ad hoc conflict-file computation in `GitPanel` in favor of one shared store selector.

**Architecture:** `GitCredentialsDialog.handleConnect` currently has no in-flight state at all — the Connect button stays clickable and shows no feedback while `saveGitCredentials` is awaited. This plan adds a local `saving` boolean spanning the whole async operation, without changing the existing "always call `setCredentials` even if the keychain save failed" behavior. Separately, `git-store.ts` already exports `selectHasConflicts`, but `GitPanel.tsx` independently re-derives the same conflicted-file list inline for its `conflictCount` badge. This plan adds a `selectConflictFiles` selector that `selectHasConflicts` is rebuilt on top of, and points `GitPanel` at it so there is exactly one place that knows what "conflicted" means.

**Tech Stack:** React 18 + TypeScript, Zustand, Vitest + @testing-library/react + @testing-library/user-event.

**Spec:** docs/reports/git-integration-review/02-frontend-architecture.md (Finding F-05's "Recommended fix" item "Disable Connect while saving and return an explicit result"; "Duplicate logic and boundary observations" item #6: "Conflict detection is duplicated: GitPanel directly scans status.files ... while GitLandingPanel calls the store method ... Prefer one selector.")

## Global Constraints

- Zustand: never fully destructure store state at component top level (CLAUDE.md).
- No raw `<button>`/`<input>` — this plan only adds a loading spinner (`lucide-react` `Loader2`, already used elsewhere in these files) to an existing shadcn `Button`.
- Preserve `GitCredentialsDialog.handleConnect`'s existing behavior of calling `setCredentials(creds)` regardless of whether the keychain save succeeded — this plan adds a loading/disabled state around that behavior, it does not change when credentials get activated.

---

### Task 1: Add a saving state to `GitCredentialsDialog`'s Connect button

**Files:**
- Modify: `src/components/git/GitCredentialsDialog.tsx`
- Test: `src/components/git/__tests__/GitCredentialsDialog.test.tsx` (new file — this component currently has no test coverage)

**Interfaces:**
- No `GitState`/store changes. Purely local component state (`saving: boolean`).

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitCredentialsDialog.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import * as tauriApi from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';
import { createDeferred } from '@/test/deferred';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listSshKeyPaths: vi.fn().mockResolvedValue([]),
    getDefaultSshKeyPath: vi.fn().mockResolvedValue(null),
    loadGitCredentials: vi.fn().mockResolvedValue(null),
    saveGitCredentials: vi.fn(),
  };
});

function renderDialog() {
  const store = createGitStore();
  store.setState({ repositoryId: 'repo-1', showCredentialsDialog: true });
  render(
    <GitStoreProvider store={store}>
      <GitCredentialsDialog />
    </GitStoreProvider>,
  );
  return store;
}

describe('GitCredentialsDialog saving state', () => {
  it('disables Connect and shows a busy state while credentials are being saved', async () => {
    const deferred = createDeferred<void>();
    vi.mocked(tauriApi.saveGitCredentials).mockReturnValue(deferred.promise);
    renderDialog();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^connect$/i }));

    expect(screen.getByRole('button', { name: /connecting/i })).toBeDisabled();
    deferred.resolve();
    await vi.waitFor(() =>
      expect(screen.getByRole('button', { name: /^connect$/i })).not.toBeDisabled(),
    );
  });

  it('re-enables Connect and shows the keychain error after a failed save', async () => {
    vi.mocked(tauriApi.saveGitCredentials).mockRejectedValueOnce(new Error('keychain locked'));
    renderDialog();
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^connect$/i }));

    expect(await screen.findByText(/keychain locked/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^connect$/i })).not.toBeDisabled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitCredentialsDialog.test.tsx`
Expected: FAIL — the "Connecting…" button never appears (no disabled/busy state exists yet).

- [ ] **Step 3: Implement the saving state**

In `GitCredentialsDialog.tsx`, add `Loader2` to the `lucide-react` import (currently only `FolderOpen`).

Add local state alongside the existing `useState` declarations:

```ts
  const [saving, setSaving] = useState(false);
```

Replace `handleConnect`:

```ts
  const handleConnect = async () => {
    let creds: GitCredentials;
    switch (authType) {
      case 'sshKey':
        creds = { type: 'sshKey', privateKeyPath, passphrase: passphrase || undefined };
        break;
      case 'sshAgent':
        creds = { type: 'sshAgent' };
        break;
      case 'userPass':
        creds = { type: 'userPass', username, password };
        break;
      case 'token':
        creds = { type: 'token', token };
        break;
    }

    setSaving(true);
    setSaveError(null);
    try {
      // Persist repository-scoped credentials when a repository is active. Clone flows
      // have no repository yet, but still activate the credentials for the pending operation.
      if (repositoryId) {
        try {
          await saveGitCredentials(repositoryId, creds);
        } catch (e) {
          setSaveError(`Could not save credentials to keychain: ${String(e)}`);
        }
      }

      setCredentials(creds);
    } finally {
      setSaving(false);
    }
  };
```

Replace the Connect button:

```tsx
          <Button onClick={handleConnect} className='w-full' size='sm' disabled={saving} aria-busy={saving}>
            {saving && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
            {saving ? 'Connecting…' : 'Connect'}
          </Button>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitCredentialsDialog.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Run the type checker**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/GitCredentialsDialog.tsx src/components/git/__tests__/GitCredentialsDialog.test.tsx
git commit -m "fix(git): disable Connect and show progress while saving credentials"
```

---

### Task 2: Consolidate conflict-file detection into one shared selector

**Files:**
- Modify: `src/stores/git-store.ts`
- Modify: `src/components/git/GitPanel.tsx`
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Produces: `export function selectConflictFiles(state: GitState): FileStatus[]` from `git-store.ts`. `selectHasConflicts` keeps its existing signature but is reimplemented on top of it, so no consumer of `selectHasConflicts` needs to change.

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts`, inside (or directly after) the existing `describe('selectHasConflicts', ...)` block:

```ts
describe('selectConflictFiles', () => {
  it('returns only the conflicted files from status', () => {
    const state = {
      status: {
        branch: 'main',
        files: [
          { path: 'a.txt', staged: false, status: 'conflicted' },
          { path: 'b.txt', staged: false, status: 'modified' },
          { path: 'c.txt', staged: false, status: 'conflicted' },
        ],
        ahead: 0,
        behind: 0,
        isClean: false,
      },
    } as GitState;

    expect(selectConflictFiles(state).map((f) => f.path)).toEqual(['a.txt', 'c.txt']);
  });

  it('returns an empty array when there is no status', () => {
    expect(selectConflictFiles({ status: null } as GitState)).toEqual([]);
  });
});
```

Add `selectConflictFiles` to the existing `import { createGitStore, type GitState, selectHasConflicts } from '../git-store';` line.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "selectConflictFiles"`
Expected: FAIL — `selectConflictFiles is not exported`.

- [ ] **Step 3: Implement `selectConflictFiles` and rebuild `selectHasConflicts` on it**

In `git-store.ts`, replace the existing `selectHasConflicts` export at the bottom of the file:

```ts
/** All files with conflicted status in the current repository's status.
 *  Shared by GitPanel and GitLandingPanel so conflict detection isn't
 *  computed independently in two places (see
 *  docs/reports/git-integration-review/02-frontend-architecture.md, dup-obs #6). */
export function selectConflictFiles(state: GitState): FileStatus[] {
  return state.status?.files.filter((f) => f.status === 'conflicted') ?? [];
}

/** True when the repository's current status has any conflicted file. */
export function selectHasConflicts(state: GitState): boolean {
  return selectConflictFiles(state).length > 0;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "selectConflictFiles|selectHasConflicts"`
Expected: PASS — the two new tests, plus the pre-existing `selectHasConflicts` tests continue to pass unchanged (same behavior, now built on `selectConflictFiles`).

- [ ] **Step 5: Update `GitPanel.tsx`**

Add `selectConflictFiles` to the existing `import { createGitStore, selectHasConflicts } from '@/stores/git-store';` line.

Replace:

```ts
  const hasConflicts = useStore(store, selectHasConflicts);
  const conflictCount = status?.files.filter((f) => f.status === 'conflicted').length ?? 0;
```

with:

```ts
  const conflictFiles = useStore(store, selectConflictFiles);
  const hasConflicts = conflictFiles.length > 0;
  const conflictCount = conflictFiles.length;
```

(This also removes the last remaining direct read of `selectHasConflicts` in this file in favor of deriving both values from the same selector subscription, avoiding two separate store subscriptions for what is now one piece of derived state.)

- [ ] **Step 6: Run the existing test suite**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS — no behavior change, `conflictCount` and the merge-in-progress banner (`hasConflicts`) are computed identically to before.

- [ ] **Step 7: Run the type checker and linter**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts src/components/git/GitPanel.tsx
git commit -m "refactor(git): derive conflict count and hasConflicts from one shared selector"
```

---

## Final verification

- [ ] Run `yarn tsc --noEmit` — no errors.
- [ ] Run `yarn check` — no lint/format violations.
- [ ] Run `yarn test src/components/git/__tests__/GitCredentialsDialog.test.tsx src/stores/__tests__/git-store.test.ts src/components/git/__tests__/GitPanel.test.tsx` — full green.
