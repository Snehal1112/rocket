# Git Invalidation Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close three specific invalidation gaps: (1) the diff view for a selected file goes stale after that file is staged/unstaged/discarded elsewhere, because `GitPanel` stores a one-time `FileStatus` snapshot instead of deriving the current one from live status; (2) `pull` doesn't refresh the commit log, even though a successful pull can bring in new commits; (3) `ConflictResolver` has an `onResolved` callback for exactly this purpose, but `GitPanel` never passes it, so a resolved or aborted conflict leaves the resolver mounted showing content that no longer applies.

**Architecture:** `GitFileList.tsx` calls `onFileClick(file)` with a `FileStatus` object captured at click time; `GitPanel.tsx` stores that whole object in `rightPanel: {kind: 'diff', file}`. Nothing re-derives it when `status` (a live store selector already held by `GitPanel`) changes, so staging/unstaging/discarding the currently-open file — or a file-watcher-driven status refresh — never updates what's displayed. The fix is to store only the file's `path` in `rightPanel` and look the current `FileStatus` up from `status.files` at render time, falling back to a "no longer has changes" message if it's gone (e.g. after a commit). `git-store.ts`'s `pull` already refreshes `status`, `conflicts`, and `branches` after every attempt but not `commitLog`; add that one call. `ConflictResolver.tsx` already accepts an optional `onResolved?: () => void` invoked after a successful resolve, but `GitPanel.tsx` never passes it, and `handleConfirmAbort` doesn't call it either — wire both.

**Tech Stack:** React 18, TypeScript, Zustand, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-07 "Refresh/invalidation is incomplete, and open detail views remain stale"). Verified present in current `src/components/git/GitPanel.tsx:31-37,246-249,316-322`, `src/stores/git-store.ts:676-697`, `src/components/git/ConflictResolver.tsx:29-59` (the `onResolved` prop exists but is unused by its only caller).

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-duplicate-action-guards.md` next (plan 11 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Derive the open diff's file from live status instead of a stale snapshot

**Files:**
- Modify: `src/components/git/GitPanel.tsx`
- Test: `src/components/git/__tests__/GitPanel.test.tsx`

**Interfaces:**
- Produces: `RightPanelView`'s `'diff'` variant changes from `{ kind: 'diff'; file: FileStatus }` to `{ kind: 'diff'; filePath: string }`. `DiffViewForFile` (unchanged) still receives a full `FileStatus` — now derived fresh from `status.files.find((f) => f.path === rightPanel.filePath)` on every render instead of a captured snapshot.
- Consumes: `status: RepoStatus | null` (already a `GitPanel` selector).

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitPanel.test.tsx`. This test needs `gitDiff`/`gitDiffStaged`/`gitStage` mocked and a `gitStatus` mock whose result changes between calls (unstaged, then staged) to simulate the file being staged elsewhere while its diff view is open:

```tsx
describe('GitPanel diff view invalidation', () => {
  it('reflects the staged/unstaged status of the currently open file after it changes', async () => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitBranches).mockResolvedValue({ current: 'main', local: [], remote: [] });
    vi.mocked(tauriApi.gitStashList).mockResolvedValue([]);

    let staged = false;
    vi.mocked(tauriApi.gitStatus).mockImplementation(async () => ({
      branch: 'main',
      files: [{ path: 'a.txt', staged, status: 'modified' }],
      ahead: 0,
      behind: 0,
      isClean: false,
    }));
    vi.mocked(tauriApi.gitDiff).mockResolvedValue({ oldContent: 'old', newContent: 'new-working' });
    vi.mocked(tauriApi.gitDiffStaged).mockResolvedValue({ oldContent: 'old', newContent: 'new-staged' });
    vi.mocked(tauriApi.gitStage).mockImplementation(async () => {
      staged = true;
    });

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );

    const user = userEvent.setup();
    await user.click(await screen.findByText('a.txt'));
    // Confirm the working-tree diff loaded for the file the user clicked.
    await vi.waitFor(() => expect(tauriApi.gitDiff).toHaveBeenCalledWith('repo-a', 'a.txt'));

    // Stage the file from elsewhere in the UI (the file-list row's Stage button).
    await user.click(screen.getByRole('button', { name: 'Stage' }));

    // The diff view must now load the staged variant for the same file, proving
    // it re-derived the open file from fresh status instead of the stale snapshot
    // captured when the user first clicked it.
    await vi.waitFor(() => expect(tauriApi.gitDiffStaged).toHaveBeenCalledWith('repo-a', 'a.txt'));
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx -t "reflects the staged"`
Expected: FAIL — `gitDiffStaged` is never called; `rightPanel.file` still holds the original unstaged `FileStatus` snapshot from when the user clicked, so `DiffViewForFile` never re-fetches.

- [ ] **Step 3: Implement the fix**

In `src/components/git/GitPanel.tsx`, change the `RightPanelView` union:

```tsx
type RightPanelView =
  | { kind: 'landing' }
  | { kind: 'diff'; filePath: string }
  | { kind: 'conflict'; conflictFile: ConflictFile }
  | { kind: 'commits' }
  | { kind: 'commitDiff'; commit: CommitInfo; diffs: FileDiff[] }
  | { kind: 'stashes' };
```

Update the file-list click handler:

```tsx
            <GitFileList
              onFileClick={(file) => setRightPanel({ kind: 'diff', filePath: file.path })}
              onConflictClick={(conflictFile) => setRightPanel({ kind: 'conflict', conflictFile })}
            />
```

Update the breadcrumb text:

```tsx
                  {rightPanel.kind === 'diff' && rightPanel.filePath}
```

Update the right-panel content render to derive the current `FileStatus` from live `status`:

```tsx
              {rightPanel.kind === 'diff' &&
                (() => {
                  const file = status?.files.find((f) => f.path === rightPanel.filePath);
                  if (!file) {
                    return (
                      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
                        This file no longer has changes.
                      </div>
                    );
                  }
                  return (
                    <DiffViewForFile
                      file={file}
                      repositoryId={repositoryId}
                      repositoryLabel={repositoryLabel}
                    />
                  );
                })()}
```

`FileStatus` remains imported from `@/lib/tauri-api` as before (still used by the `DiffViewForFile` prop type); no import changes are needed since the type is inferred from `status.files`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx -t "reflects the staged"`
Expected: PASS

- [ ] **Step 5: Run the full GitPanel suite**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitPanel.tsx src/components/git/__tests__/GitPanel.test.tsx
```

Commit message: `fix(git): re-derive the open diff's file from live status instead of a stale snapshot`.

---

### Task 2: `pull` refreshes the commit log

**Files:**
- Modify: `src/stores/git-store.ts:676-697` (`pull`)
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Consumes: `refreshLog: (limit?: number) => Promise<void>` (already exported on `GitState`, already called elsewhere by `pull`'s neighbors are not — this is the first cross-call).
- Produces: no new exports. `pull`'s post-attempt refresh set grows from `refreshStatus`/`refreshConflicts`/`refreshBranches` to also include `refreshLog`.

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts`:

```ts
describe('git-store pull refreshes the commit log', () => {
  it('calls refreshLog after a successful pull', async () => {
    store.setState({ repositoryId: 'repo-1', isRepo: true, credentials: { type: 'token', token: 'tok' } });
    vi.mocked(tauriApi.gitPull).mockResolvedValue(undefined);
    vi.mocked(tauriApi.gitLog).mockResolvedValue([]);

    await store.getState().pull();

    expect(tauriApi.gitLog).toHaveBeenCalledWith('repo-1', 50);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "calls refreshLog after a successful pull"`
Expected: FAIL — `gitLog` is never called by `pull`.

- [ ] **Step 3: Implement the fix**

In `src/stores/git-store.ts`, update `pull`'s post-attempt refresh block:

```ts
      // Always refresh status, conflicts, branches, and the commit log after a
      // pull attempt — whether it succeeded or produced merge conflicts — so
      // the UI reflects the real repo state (behind count, conflict files,
      // incoming commits, etc.).
      await get().refreshStatus();
      await get().refreshConflicts();
      await get().refreshBranches();
      await get().refreshLog();
    },
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "calls refreshLog after a successful pull"`
Expected: PASS

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
```

Commit message: `fix(git): refresh the commit log after pull`.

---

### Task 3: Close the conflict resolver after a successful resolve or abort

**Files:**
- Modify: `src/components/git/GitPanel.tsx`
- Modify: `src/components/git/ConflictResolver.tsx`
- Test: `src/components/git/__tests__/ConflictResolver.test.tsx`

**Interfaces:**
- Consumes: `ConflictResolver`'s existing `onResolved?: () => void` prop.
- Produces: no new exports. `GitPanel` passes `onResolved={() => setRightPanel({ kind: 'landing' })}`; `ConflictResolver.handleConfirmAbort` now also calls `onResolved?.()` after a successful abort (previously only `handleResolve` did).

- [ ] **Step 1: Write the failing test**

`src/components/git/__tests__/ConflictResolver.test.tsx` already exists — read it first to match its existing render helper and mocking conventions, then add:

```tsx
describe('ConflictResolver onResolved for abort', () => {
  it('calls onResolved after a successful abort, not just a successful resolve', async () => {
    const onResolved = vi.fn();
    const store = createGitStore();
    store.setState({
      abortMerge: async () => {
        store.setState({ error: null });
      },
    });
    render(
      <GitStoreProvider store={store}>
        <ConflictResolver
          conflictState={{
            filePath: 'a.txt',
            repositoryId: 'repo-1',
            repositoryLabel: 'Repo',
            ours: 'ours',
            theirs: 'theirs',
            ancestor: null,
          }}
          onResolved={onResolved}
        />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /abort merge/i }));
    await user.click(screen.getByRole('button', { name: /confirm abort/i }));

    expect(onResolved).toHaveBeenCalledTimes(1);
  });
});
```

Adapt the exact import names/render helper (`createGitStore`, `GitStoreProvider`, `render`, `screen`, `userEvent`, `vi`) to whatever the existing `ConflictResolver.test.tsx` already imports — do not duplicate imports it already has.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/ConflictResolver.test.tsx -t "calls onResolved after a successful abort"`
Expected: FAIL — `handleConfirmAbort` never calls `onResolved`.

- [ ] **Step 3: Implement the fix**

In `src/components/git/ConflictResolver.tsx`, update `handleConfirmAbort`:

```tsx
  const handleConfirmAbort = async () => {
    if (busy) return;
    setShowAbortConfirm(false);
    setBusy(true);
    const prevError = gitStoreApi.getState().error;
    try {
      await abortMerge();
      const nextError = gitStoreApi.getState().error;
      if (!nextError || nextError === prevError) {
        onResolved?.();
      }
    } finally {
      setBusy(false);
    }
  };
```

In `src/components/git/GitPanel.tsx`, pass `onResolved` where `ConflictResolver` is rendered:

```tsx
              {rightPanel.kind === 'conflict' && (
                <Suspense fallback={null}>
                  <ConflictResolver
                    conflictState={{
                      filePath: rightPanel.conflictFile.path,
                      repositoryId,
                      repositoryLabel,
                      ours: rightPanel.conflictFile.ours,
                      theirs: rightPanel.conflictFile.theirs,
                      ancestor: rightPanel.conflictFile.ancestor ?? null,
                    }}
                    onResolved={() => setRightPanel({ kind: 'landing' })}
                  />
                </Suspense>
              )}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/ConflictResolver.test.tsx`
Expected: PASS

- [ ] **Step 5: Run the full git component suite and typecheck**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitPanel.tsx src/components/git/ConflictResolver.tsx src/components/git/__tests__/ConflictResolver.test.tsx
```

Commit message: `fix(git): return to the landing view after a conflict is resolved or the merge is aborted`.
