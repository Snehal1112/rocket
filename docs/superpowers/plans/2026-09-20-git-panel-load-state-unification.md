# GitPanel Load-State Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `GitPanel`'s duplicated local `isRepo` state (which silently swallows a failed initial load and renders the misleading "not a Git repository" screen instead of an error) with a single store-driven load state, and make `setRepository` clear all repository-scoped data synchronously the instant a new load starts.

**Architecture:** `GitPanel.tsx` keeps its own `useState<boolean | null>` for `isRepo` *in addition to* the store's `isRepo`/`loading` fields, populated by a `checkAndLoad` wrapper around `setRepository`. Because `setRepository` never throws (it catches its own errors and writes them to `store.error`), `checkAndLoad`'s own `try { await setRepository(id); ... } catch { setIsRepo(false); }` never actually reaches its `catch` — a thrown `gitIsRepo` leaves the store's `isRepo` at its default `false`, `checkAndLoad` reads that back and sets local `isRepo` to `false`, and `GitPanel` renders "This collection is not a Git repository." with Initialize/Clone buttons, while the real error sits invisibly in `store.error`. Separately, `setRepository` only touches `repositoryId`/`loading`/`error`/`isRepo` at the start of a load — `status`, `branches`, `remotes`, `stashes`, `commitLog`, `conflicts`, and `credentials` are left over from whatever was loaded before until each individual refresh call resolves. This plan (1) adds a single `loadStatus: 'idle' | 'loading' | 'ready' | 'not-repo' | 'error'` field to the store, set explicitly at every exit point of `setRepository` — including a case that only fires when the repository couldn't even be determined (as opposed to a background refresh failing after a successful determination) — and has `GitPanel` render off that instead of a local duplicate; and (2) makes `setRepository` clear every repository-scoped field synchronously the moment a new load starts, so no stale data from a previous repository can be visible even momentarily.

**Tech Stack:** TypeScript, Zustand vanilla store, React 18, Vitest, React Testing Library, `@/test/deferred`.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-06 "Repository loading leaves stale state and hides initialization errors", F-13 "Component boundaries duplicate ownership... `GitPanel` owns a local `isRepo` while also subscribing to store `isRepo` and `loading`"). Verified present in current `src/stores/git-store.ts:172-207` and `src/components/git/GitPanel.tsx:44-94,173-212`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-collection-listener-leak-fix.md` next (plan 3 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only.
- Rust: not touched by this plan (frontend-only).
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Add a single store-driven `loadStatus` and remove `GitPanel`'s local `isRepo` duplicate

**Files:**
- Modify: `src/stores/git-store.ts` (interface `GitState`, initial state, `setRepository`, `reset`)
- Modify: `src/components/git/GitPanel.tsx`
- Test: `src/stores/__tests__/git-store.test.ts`, `src/components/git/__tests__/GitPanel.test.tsx`

**Interfaces:**
- Produces: `GitState.loadStatus: 'idle' | 'loading' | 'ready' | 'not-repo' | 'error'`. `'error'` is set only when the repository's status could not be determined at all (the `gitIsRepo` call itself failed); a failure partway through loading a *known* repository (e.g. `gitStatus` throwing after `gitIsRepo` returned `true`) resolves to `'ready'` with `error` set, since we did successfully determine it's a repository.
- Consumes (`GitPanel.tsx`): `useStore(store, (state) => state.loadStatus)` and `useStore(store, (state) => state.error)` replace the local `useState<boolean | null>` + `checkAndLoad`.

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts`:

```ts
describe('git-store loadStatus', () => {
  it('sets loadStatus to "error" (not "not-repo") when gitIsRepo itself fails', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockRejectedValue(new Error('disk unreadable'));

    await store.getState().setRepository('repo-1');

    expect(store.getState().loadStatus).toBe('error');
    expect(store.getState().error).toBe('Error: disk unreadable');
    expect(store.getState().isRepo).toBe(false);
  });

  it('sets loadStatus to "ready" (not "error") when the repo loads but a later refresh fails', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitStatus).mockRejectedValue(new Error('status unavailable'));

    await store.getState().setRepository('repo-1');

    expect(store.getState().loadStatus).toBe('ready');
    expect(store.getState().isRepo).toBe(true);
    expect(store.getState().error).toBe('Error: status unavailable');
  });

  it('sets loadStatus to "not-repo" when gitIsRepo cleanly resolves false', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(false);

    await store.getState().setRepository('repo-1');

    expect(store.getState().loadStatus).toBe('not-repo');
    expect(store.getState().error).toBeNull();
  });
});
```

Add to `src/components/git/__tests__/GitPanel.test.tsx`:

```tsx
describe('GitPanel load error rendering', () => {
  it('renders a retryable error state, not the Initialize/Clone prompt, when the load fails', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockRejectedValue(new Error('disk unreadable'));
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    render(
      <QueryClientProvider client={queryClient}>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );

    expect(await screen.findByText(/failed to load this repository/i)).toBeInTheDocument();
    expect(screen.getByText(/disk unreadable/i)).toBeInTheDocument();
    expect(screen.queryByText('Initialize Git')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/stores/__tests__/git-store.test.ts src/components/git/__tests__/GitPanel.test.tsx`
Expected: FAIL — `loadStatus` doesn't exist yet on `GitState`; the "Initialize Git" prompt renders instead of an error screen.

- [ ] **Step 3: Add `loadStatus` to the store**

In `src/stores/git-store.ts`, add to `GitState`:

```ts
export interface GitState {
  isRepo: boolean;
  /** Single source of truth for what GitPanel should render, set at every exit
   *  point of `setRepository`. 'error' means the repository's status could not
   *  even be determined (distinct from a known repo whose background refresh
   *  failed, which stays 'ready' with `error` set). */
  loadStatus: 'idle' | 'loading' | 'ready' | 'not-repo' | 'error';
  repositoryId: string | null;
  ...
```

Add `loadStatus: 'idle'` to the store's initial state object (next to `isRepo: false,`):

```ts
    isRepo: false,
    loadStatus: 'idle',
    repositoryId: null,
```

Replace `setRepository`'s body:

```ts
    setRepository: async (repositoryId: string) => {
      const myGeneration = ++loadGeneration;
      set({ repositoryId, loading: true, error: null, loadStatus: 'loading' });
      try {
        const isRepo = await gitIsRepo(repositoryId);
        if (myGeneration !== loadGeneration) return;
        set({ isRepo });
        if (isRepo) {
          // Always reload repository-scoped credentials so switching repositories
          // picks up the right identity without requiring a manual re-entry.
          try {
            const saved = await loadGitCredentials(repositoryId);
            if (myGeneration !== loadGeneration) return;
            set({ credentials: saved ?? null });
          } catch {
            // Keychain unavailable — proceed without credentials.
          }
          if (myGeneration !== loadGeneration) return;
          const [status] = await Promise.all([
            gitStatus(repositoryId),
            get().refreshStashes(),
            get().refreshBranches(),
            get().refreshRemotes(),
          ]);
          if (myGeneration !== loadGeneration) return;
          set({ status, loading: false, loadStatus: 'ready' });
        } else {
          if (myGeneration !== loadGeneration) return;
          set({ status: null, loading: false, loadStatus: 'not-repo' });
        }
      } catch (e) {
        if (myGeneration !== loadGeneration) return;
        // If `isRepo` was already determined true before this failure (e.g. a
        // refresh inside the Promise.all threw), we know it's a repository —
        // only classify as a load failure when we never got that far.
        const stillUnknown = !get().isRepo;
        set({
          error: String(e),
          loading: false,
          loadStatus: stillUnknown ? 'error' : 'ready',
        });
      }
    },
```

Add `loadStatus: 'idle'` to `reset()`'s state object (next to `isRepo: false,`).

- [ ] **Step 4: Run the store tests to verify they pass**

Run: `yarn test src/stores/__tests__/git-store.test.ts`
Expected: PASS

- [ ] **Step 5: Rewrite `GitPanel` to render off `loadStatus`**

In `src/components/git/GitPanel.tsx`, remove the local `isRepo` state and `checkAndLoad`:

```tsx
export function GitPanel({ repositoryId, repositoryLabel }: GitPanelProps) {
  const [leftWidth, setLeftWidth] = useState(320);
  const [rightPanel, setRightPanel] = useState<RightPanelView>({
    kind: 'landing',
  });
  const [showRemotesDialog, setShowRemotesDialog] = useState(false);
  const [showCloneDialog, setShowCloneDialog] = useState(false);
  const [store] = useState(() => createGitStore());

  const showCredentialsDialog = useStore(store, (state) => state.showCredentialsDialog);
  const setRepository = useStore(store, (state) => state.setRepository);
  const refreshLog = useStore(store, (state) => state.refreshLog);
  const refreshStashes = useStore(store, (state) => state.refreshStashes);
  const refreshStatus = useStore(store, (state) => state.refreshStatus);
  const status = useStore(store, (state) => state.status);
  const loadedRepositoryId = useStore(store, (state) => state.repositoryId);
  const loadStatus = useStore(store, (state) => state.loadStatus);
  const loadError = useStore(store, (state) => state.error);
  const initRepo = useStore(store, (state) => state.initRepo);
  const showIdentitySetupDialog = useStore(store, (state) => state.showIdentitySetupDialog);
  const identitySetupInitialName = useStore(store, (state) => state.identitySetupInitialName);
  const identitySetupInitialEmail = useStore(store, (state) => state.identitySetupInitialEmail);
  const activatePendingCredentials = useStore(store, (state) => state.activatePendingCredentials);
  const discardPendingIdentitySetup = useStore(store, (state) => state.discardPendingIdentitySetup);
  const currentBranch = status?.branch ?? null;
  const hasConflicts = status?.files.some((f) => f.status === 'conflicted') ?? false;
  const conflictCount = status?.files.filter((f) => f.status === 'conflicted').length ?? 0;

  useEffect(() => {
    if (loadedRepositoryId === repositoryId) return;
    void setRepository(repositoryId);
  }, [repositoryId, loadedRepositoryId, setRepository]);
```

(`activatePendingCredentials`/`discardPendingIdentitySetup` — if `docs/superpowers/plans/2026-09-20-git-identity-cancel-semantics.md` has already been applied, keep both selectors and use `discardPendingIdentitySetup` in `handleIdentitySetupCancel` as that plan specifies; if not yet applied, keep only `activatePendingCredentials` exactly as today. This task does not change that handler either way.)

Replace the `if (isRepo === null) ... if (!isRepo) ...` block:

```tsx
  if (loadStatus === 'idle' || loadStatus === 'loading') {
    return <GitPanelSkeleton />;
  }

  if (loadStatus === 'error') {
    return (
      <GitStoreProvider store={store}>
        <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
          <AlertTriangle className='h-5 w-5 text-destructive' />
          <p className='text-sm text-destructive'>Failed to load this repository.</p>
          {loadError && (
            <p className='text-xs text-muted-foreground wrap-break-word max-w-sm'>{loadError}</p>
          )}
          <Button variant='outline' size='sm' onClick={() => void setRepository(repositoryId)}>
            Retry
          </Button>
        </div>
      </GitStoreProvider>
    );
  }

  if (loadStatus === 'not-repo') {
    return (
      <GitStoreProvider store={store}>
        <div className='flex flex-col items-center justify-center gap-3 h-full px-4 text-center'>
          <p className='text-sm text-muted-foreground'>This collection is not a Git repository.</p>
          {loadError && (
            <p className='text-xs text-destructive wrap-break-word max-w-sm'>{loadError}</p>
          )}
          <div className='flex gap-2'>
            <Button variant='outline' size='sm' onClick={() => void initRepo(repositoryId)}>
              Initialize Git
            </Button>
            <Button variant='outline' size='sm' onClick={() => setShowCloneDialog(true)}>
              Clone Repository
            </Button>
          </div>
          {showCredentialsDialog && <GitCredentialsDialog />}
          {showIdentitySetupDialog && (
            <GitIdentityDialog
              open={showIdentitySetupDialog}
              onConfirm={handleIdentitySetupConfirm}
              onCancel={handleIdentitySetupCancel}
              initialName={identitySetupInitialName}
              initialEmail={identitySetupInitialEmail}
              confirmLabel='Save Identity'
            />
          )}
          <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
        </div>
      </GitStoreProvider>
    );
  }

  // loadStatus === 'ready' from here.
```

The rest of the component (the full two-pane layout previously gated by `return ( <GitStoreProvider store={store}> ... )` at the bottom) is unchanged — it now implicitly only renders when `loadStatus === 'ready'`.

`AlertTriangle` is already imported at the top of the file (used for the merge-conflict banner) — no new icon import needed.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS (including the pre-existing "two panels for different repositories never share state" test)

- [ ] **Step 7: Run the full git test suite and typecheck**

Run: `yarn test src/components/git src/stores/__tests__/git-store.test.ts`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts src/components/git/GitPanel.tsx src/stores/__tests__/git-store.test.ts src/components/git/__tests__/GitPanel.test.tsx
```

Commit message: `fix(git): unify GitPanel's load state and stop hiding initial-load errors`.

---

### Task 2: Clear repository-scoped data synchronously when a new load starts

**Files:**
- Modify: `src/stores/git-store.ts` (`setRepository`)
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Consumes: `createDeferred` from `@/test/deferred` to hold `gitIsRepo` pending so the test can inspect state mid-load.
- Produces: no new exports — `setRepository` now clears `status`, `branches`, `remotes`, `stashes`, `commitLog`, `conflicts`, and `credentials` synchronously at the start of every call, in addition to the `repositoryId`/`loading`/`error`/`loadStatus`/`isRepo` fields it already reset.

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts`:

```ts
describe('git-store setRepository clears stale data synchronously', () => {
  it('clears prior repository data the instant a new load starts, before the new load resolves', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    await store.getState().setRepository('repo-a');
    // Sanity: repo-a actually loaded some data.
    expect(store.getState().isRepo).toBe(true);

    store.setState({
      status: { branch: 'repo-a-branch', files: [], ahead: 3, behind: 0, isClean: true },
      branches: { current: 'repo-a-branch', local: [], remote: [] },
      remotes: [{ name: 'origin', url: 'https://a.example.com' }],
      stashes: [{ index: 0, message: 'wip', timestamp: '2026-01-01', filesChanged: 1, insertions: 1, deletions: 0, changedFiles: ['a'], branch: 'repo-a-branch' }],
      commitLog: [{ id: 'abc', fullId: 'abc123', message: 'm', author: 'a', authorEmail: 'a@a.com', timestamp: '2026-01-01', filesChanged: 1 }],
      credentials: { type: 'token', token: 'repo-a-secret' },
    });

    const deferredIsRepo = createDeferred<boolean>();
    vi.mocked(tauriApi.gitIsRepo).mockReturnValue(deferredIsRepo.promise);

    const loadPromise = store.getState().setRepository('repo-b');

    // Before repo-b's gitIsRepo call even resolves, all of repo-a's data must
    // already be gone — not left visible until repo-b's refreshes complete.
    expect(store.getState().status).toBeNull();
    expect(store.getState().branches).toBeNull();
    expect(store.getState().remotes).toEqual([]);
    expect(store.getState().stashes).toEqual([]);
    expect(store.getState().commitLog).toEqual([]);
    expect(store.getState().credentials).toBeNull();
    expect(store.getState().isRepo).toBe(false);

    deferredIsRepo.resolve(false);
    await loadPromise;
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "clears prior repository data"`
Expected: FAIL — `status`, `branches`, `remotes`, `stashes`, `commitLog`, and `credentials` still hold repo-a's values while repo-b's `gitIsRepo` is pending.

- [ ] **Step 3: Clear repository-scoped fields at the start of `setRepository`**

In `src/stores/git-store.ts`, update the first `set(...)` call in `setRepository`:

```ts
    setRepository: async (repositoryId: string) => {
      const myGeneration = ++loadGeneration;
      set({
        repositoryId,
        loading: true,
        error: null,
        loadStatus: 'loading',
        isRepo: false,
        status: null,
        branches: null,
        remotes: [],
        stashes: [],
        commitLog: [],
        conflicts: [],
        credentials: null,
      });
      try {
```

(The rest of `setRepository` is unchanged from Task 1.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "clears prior repository data"`
Expected: PASS

- [ ] **Step 5: Run the full store suite and typecheck**

Run: `yarn test src/stores/__tests__/git-store.test.ts`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
```

Commit message: `fix(git): clear repository-scoped store data synchronously on repository change`.
