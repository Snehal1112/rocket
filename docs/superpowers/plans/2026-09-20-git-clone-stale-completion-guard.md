# GitCloneDialog Stale-Completion Guard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent an in-flight clone request from silently opening a workspace, showing the collection picker, or writing an error into a dialog that has since been closed and reopened with different inputs.

**Architecture:** `GitCloneDialog.tsx` already fixed the double-clone bug from the review (single `performClone` call path, gated by an `awaitingCredentials` flag that flips to `false` immediately so the credential-arrival effect cannot fire twice). What remains is that `performClone` and its continuation `handlePostClone` → `handleOpenWorkspace` have no request identity: if the user closes the dialog while a clone is in flight (the `open` effect resets all local state, including `step`, `destination`, `error`) and reopens it with a new URL/destination, the *old* clone's `await gitClone(...)` can still resolve afterward and call `handlePostClone`, which can call `handleOpenWorkspace`, which switches the active workspace out from under the new dialog session — with no visible indication to the user that the workspace switch came from a stale request. The fix is a monotonically increasing request id captured at the start of `performClone`; every step after an `await` checks it's still the current request before touching component state or triggering a workspace switch.

**Tech Stack:** React 18, TypeScript, Vitest, React Testing Library, `@/test/deferred` (`createDeferred`) for controlling promise resolution order in tests.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-02 "Clone starts twice when credentials already exist"). Verified against current source: the double-clone bug itself is already fixed (`awaitingCredentials` flag + single `performClone` path). This plan implements the remaining part of F-02's own recommended fix and focused-test list that is not yet covered: *"Close/reopen while a clone is pending; assert an old completion cannot change the new dialog state or open a workspace."*

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-identity-cancel-semantics.md` next (plan 5 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only — not touched by this plan.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Guard clone completion with a request id tied to dialog lifecycle

**Files:**
- Modify: `src/components/git/GitCloneDialog.tsx`
- Test: `src/components/git/__tests__/GitCloneDialog.test.tsx` (new file)

**Interfaces:**
- Consumes: `gitClone(url: string, capability: string, creds: GitCredentials): Promise<void>` and `detectClonedStructure(clonedPath: string): Promise<ClonedRepoStructure>` from `@/lib/tauri-api` (unchanged signatures).
- Produces: no new exports. `GitCloneDialog` behavior only — a stale clone's resolution becomes a no-op instead of mutating state or switching workspaces.

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitCloneDialog.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
import * as tauriApi from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitClone: vi.fn(),
    detectClonedStructure: vi.fn(),
    selectCloneDestination: vi.fn(),
  };
});

vi.mock('@/lib/queries/workspace-queries', async () => {
  const actual = await vi.importActual<typeof import('@/lib/queries/workspace-queries')>(
    '@/lib/queries/workspace-queries',
  );
  return {
    ...actual,
    useOpenWorkspaceFromDisk: () => ({ mutateAsync: openFromDiskMock }),
    useSwitchWorkspace: () => ({ mutate: switchWorkspaceMock }),
  };
});

const openFromDiskMock = vi.fn();
const switchWorkspaceMock = vi.fn();

function renderDialog(onOpenChange: (open: boolean) => void, open: boolean) {
  const store = createGitStore();
  store.setState({ credentials: { type: 'token', token: 'tok' } });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <GitStoreProvider store={store}>
        <GitCloneDialog open={open} onOpenChange={onOpenChange} />
      </GitStoreProvider>
    </QueryClientProvider>,
  );
}

describe('GitCloneDialog stale completion', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(tauriApi.selectCloneDestination).mockResolvedValue({
      capability: 'cap-1',
      displayPath: '/tmp/dest-1',
    });
  });

  it('ignores a clone that resolves after the dialog was closed and does not switch workspaces', async () => {
    const deferredClone = createDeferred<void>();
    vi.mocked(tauriApi.gitClone).mockReturnValue(deferredClone.promise);

    let open = true;
    const onOpenChange = vi.fn((next: boolean) => {
      open = next;
    });
    const { rerender } = renderDialog(onOpenChange, open);

    await userEvent.click(screen.getByRole('button', { name: /browse/i }));
    await userEvent.type(screen.getByPlaceholderText(/github.com/i), 'https://example.com/repo.git');
    await userEvent.click(screen.getByRole('button', { name: /^clone$/i }));

    // Dialog closes (e.g. user hits escape) while the clone is still in flight.
    rerender(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <GitStoreProvider store={createGitStore()}>
          <GitCloneDialog open={false} onOpenChange={onOpenChange} />
        </GitStoreProvider>
      </QueryClientProvider>,
    );

    // The stale clone now resolves.
    deferredClone.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(openFromDiskMock).not.toHaveBeenCalled();
    expect(switchWorkspaceMock).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitCloneDialog.test.tsx`
Expected: FAIL — `deferredClone.resolve()` drives `handlePostClone` → `detectClonedStructure` (mock returns `undefined` by default, which will throw inside `handlePostClone` on `structure.kind`, or if you additionally stub `detectClonedStructure` to resolve a `{kind: 'workspace', workspacePath: '/tmp/dest-1'}`-shaped value, `handleOpenWorkspace` runs and calls the mocked mutations even though the dialog is closed) — demonstrating the stale-completion gap. If the throw happens before reaching the assertions, add `vi.mocked(tauriApi.detectClonedStructure).mockResolvedValue({ kind: 'workspace', workspacePath: '/tmp/dest-1', collections: [] })` in the test's `beforeEach` so the test exercises the actual `handleOpenWorkspace` path rather than an unrelated crash.

- [ ] **Step 3: Add a request id guard**

In `src/components/git/GitCloneDialog.tsx`, add a ref-backed request counter and check it after every `await` in the clone continuation chain:

```tsx
import { useCallback, useEffect, useRef, useState } from 'react';
```

```tsx
  const [collections, setCollections] = useState<CollectionScanResult[]>([]);
  const [selectedCollection, setSelectedCollection] = useState<string | null>(null);

  // Bumped every time the dialog opens or a clone starts; a clone's async
  // continuation only touches state/navigation if it's still the current one
  // when it resolves. This stops a stale clone (dialog closed and reopened,
  // or reopened with a new URL/destination, while the old one was in flight)
  // from opening a workspace or writing into a dialog session it no longer owns.
  const requestIdRef = useRef(0);
```

Reset the guard whenever the dialog opens (so any clone from a previous open session is disowned even if it never got explicitly cancelled):

```tsx
  // Reset all state when dialog opens.
  useEffect(() => {
    if (open) {
      requestIdRef.current += 1;
      setStep('input');
      setRepoUrl('');
      setDestination(null);
      setAwaitingCredentials(false);
      setError(null);
      setCollections([]);
      setSelectedCollection(null);
    } else {
      // Closing also disowns any clone still in flight from this session.
      requestIdRef.current += 1;
    }
  }, [open]);
```

Capture the id at the start of `performClone` and gate every subsequent state/navigation write on it:

```tsx
  const performClone = useCallback(
    async (creds: GitCredentials) => {
      if (!destination) {
        setError('Select an empty destination folder before cloning.');
        setStep('input');
        return;
      }

      const myRequestId = requestIdRef.current;
      setError(null);
      setStep('progress');
      try {
        await gitClone(repoUrl.trim(), destination.capability, creds);
        if (requestIdRef.current !== myRequestId) return;
        await handlePostClone(destination.displayPath);
      } catch (e) {
        if (requestIdRef.current !== myRequestId) return;
        // Capabilities are one-time, including failed clone attempts.
        setDestination(null);
        setError(String(e));
        setStep('input');
      }
    },
    [destination, handlePostClone, repoUrl],
  );
```

`handlePostClone` itself awaits `detectClonedStructure` and then calls `handleOpenWorkspace`; add the same guard there since it can also resolve after the request was disowned:

```tsx
  const handlePostClone = useCallback(
    async (clonedPath: string) => {
      const myRequestId = requestIdRef.current;
      const structure: ClonedRepoStructure = await detectClonedStructure(clonedPath);
      if (requestIdRef.current !== myRequestId) return;

      if (structure.kind === 'workspace' && structure.workspacePath) {
        await handleOpenWorkspace(structure.workspacePath);
        return;
      }

      if (structure.kind === 'collection' && structure.collections.length === 1) {
        await handleOpenWorkspace(structure.collections[0].path);
        return;
      }

      if (structure.collections.length > 0) {
        setCollections(structure.collections);
        setStep('picker');
        return;
      }

      // Nothing detected — show empty picker.
      setCollections([]);
      setStep('picker');
    },
    [handleOpenWorkspace],
  );
```

Note `performClone`'s own guard after `await gitClone(...)` already covers the call into `handlePostClone`, and `handlePostClone`'s own guard covers the case where it's invoked directly in a context where `performClone`'s outer check already passed but the id changed again during `detectClonedStructure`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitCloneDialog.test.tsx`
Expected: PASS

- [ ] **Step 5: Run typecheck and the broader git component suite**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn test src/components/git`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitCloneDialog.tsx src/components/git/__tests__/GitCloneDialog.test.tsx
```

Commit message: `fix(git): ignore stale clone completions after dialog close/reopen`.
