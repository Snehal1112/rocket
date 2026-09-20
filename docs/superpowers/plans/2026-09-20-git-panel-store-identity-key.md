# GitPanel Store Identity Key Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop a single `GitPanel` React instance (and its Zustand store) from being silently reused across two different repositories when the app switches which Git tab is active in the same pane slot.

**Architecture:** `EditorGroup.tsx` and `WorkspaceGitTab.tsx` render `<GitPanel repositoryId=... />` without a `key`. React therefore keeps the same `GitPanel` component instance — and the single `createGitStore()` instance it owns via `useState(() => createGitStore())` — when the user switches from one Git tab to another Git tab in the same pane (or from one workspace's Git tab to another's). The store's own `setRepository` has a `loadGeneration` guard that prevents a *stale async result* from clobbering a *newer* load, but it does nothing to clear stale synchronous state (`credentials`, `status`, `branches`, `stashes`, `remotes`, `commitLog`, `pendingNetworkOp`, `showCredentialsDialog`) the instant the switch starts — that state visibly persists from the previous repository until each individual refresh call resolves. Passing `key={repositoryId}` makes React unmount the old `GitPanel` (and everything under it, including the conditionally-rendered `GitCredentialsDialog`) and mount a brand-new one with a brand-new store whenever `repositoryId` changes, which is the cheapest correct fix and also eliminates any possibility of stale credentials from repository A surviving into repository B's credentials dialog.

**Tech Stack:** React 18, TypeScript, Zustand (vanilla store + `zustand/react` `useStore`), Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-01 "Singleton repository state is incompatible with path-bearing panes", F-05 "Credential dialog can retain and apply secrets from a previous workspace"). Note: this repo's `git-store.ts`/`git-store-context.tsx`/`GitPanel.tsx` have already been substantially reworked since that review was written — the store is now created per-`GitPanel`-instance (not a global singleton) and has a `loadGeneration` guard. The specific gap this plan closes — no `key` prop at the two `GitPanel` call sites — is a residual instance of the same underlying problem the spec describes, verified against the current source.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-toolbar-active-collection-race.md` next (plan 1 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Zustand: never fully destructure store state at component top level (`CLAUDE.md:91`) — not touched by this plan, but do not introduce a new violation.
- All UI components use shadcn/ui primitives only — not touched by this plan.
- Commits use conventional commits format (`feat:`, `fix:`, `chore:`, etc.).
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Key `GitPanel` by `repositoryId` at both call sites

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx:187-190`
- Modify: `src/components/workspace/WorkspaceGitTab.tsx:23`
- Test: `src/components/git/__tests__/GitPanel.test.tsx`

**Interfaces:**
- Consumes: `GitPanel` component signature — `{ repositoryId: string; repositoryLabel: string }` (`src/components/git/GitPanel.tsx:39-42`). Unchanged by this task.
- Produces: no new exports. Behavior change only: React remounts `GitPanel` (and its store) whenever `repositoryId` changes.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitPanel.test.tsx` (same file already has the `QueryClientProvider` + `tauri-api` mock scaffolding — extend it rather than duplicating):

```tsx
describe('GitPanel remount on repositoryId change', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitStatus).mockResolvedValue({
      branch: 'main',
      files: [],
      ahead: 0,
      behind: 0,
      isClean: true,
    });
    vi.mocked(tauriApi.gitBranches).mockResolvedValue({ current: 'main', local: [], remote: [] });
    vi.mocked(tauriApi.gitStashList).mockResolvedValue([]);
  });

  it('does not carry loaded credentials over when the mounted repository changes', async () => {
    vi.mocked(tauriApi.loadGitCredentials).mockImplementation(async (id: string) =>
      id === 'repo-a' ? { type: 'token', token: 'secret-a' } : null,
    );

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { rerender } = render(
      <QueryClientProvider client={queryClient}>
        <GitPanel key='repo-a' repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );
    await screen.findByText('Repo A');

    // Simulate the app switching the active Git tab to a different repository —
    // same JSX position, different key, exactly what EditorGroup/WorkspaceGitTab do.
    rerender(
      <QueryClientProvider client={queryClient}>
        <GitPanel key='repo-b' repositoryId='repo-b' repositoryLabel='Repo B' />
      </QueryClientProvider>,
    );
    await screen.findByText('Repo B');

    // Open the credentials dialog for repo-b and confirm it never shows repo-a's
    // loaded credentials — this proves the store instance was not reused.
    await userEvent.click(screen.getByRole('button', { name: /change ssh credentials|set credentials/i }));
    const tokenField = screen.queryByLabelText(/token/i);
    // repo-b has no saved credentials and the dialog defaults to SSH Key, so the
    // token field may not even be present; when it is, it must be empty.
    if (tokenField) {
      expect(tokenField).toHaveValue('');
    }
    expect(vi.mocked(tauriApi.loadGitCredentials)).toHaveBeenCalledWith('repo-b');
  });
});
```

Add the missing imports at the top of the test file: `userEvent` from `@testing-library/user-event`, and `rerender`/`screen` are already available from `@testing-library/react`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx -t "does not carry loaded credentials"`
Expected: FAIL — without a `key`, `rerender` reuses the same `GitPanel` instance and store; `loadGitCredentials` is called once for `repo-a` on first mount but the component may not re-run `setRepository('repo-b')` the same way, or the credentials dialog can still show stale state depending on timing. The point of this step is to see it fail against today's behavior before applying the fix — confirm the failure is about stale/incorrect state, not a test-authoring mistake (e.g. re-check the button's accessible name if the query fails to find it).

- [ ] **Step 3: Add `key={repositoryId}` at both `GitPanel` call sites**

In `src/components/panes/EditorGroup.tsx`, change:

```tsx
          ) : isGitTab(activeTab) ? (
            <GitPanel
              repositoryId={activeTab.repositoryId}
              repositoryLabel={activeTab.repositoryLabel}
            />
```

to:

```tsx
          ) : isGitTab(activeTab) ? (
            <GitPanel
              key={activeTab.repositoryId}
              repositoryId={activeTab.repositoryId}
              repositoryLabel={activeTab.repositoryLabel}
            />
```

In `src/components/workspace/WorkspaceGitTab.tsx`, change:

```tsx
  return <GitPanel repositoryId={repositoryId} repositoryLabel={workspace?.name ?? 'Workspace'} />;
```

to:

```tsx
  return (
    <GitPanel
      key={repositoryId}
      repositoryId={repositoryId}
      repositoryLabel={workspace?.name ?? 'Workspace'}
    />
  );
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx -t "does not carry loaded credentials"`
Expected: PASS

- [ ] **Step 5: Run the full existing GitPanel test suite and typecheck**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS (including the pre-existing "two panels for different repositories never share state" test)

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill (per user global instructions) instead of a freeform `git commit -m`. Stage:

```bash
git add src/components/panes/EditorGroup.tsx src/components/workspace/WorkspaceGitTab.tsx src/components/git/__tests__/GitPanel.test.tsx
```

Then invoke the commit skill with a message along the lines of: `fix(git): remount GitPanel on repository change instead of reusing its store`.
