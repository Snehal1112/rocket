# WorkspaceGitTab Query Lifecycle States Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `WorkspaceGitTab` from rendering the same "No workspace repository configured." message for three different situations that need different handling: the workspace list is still loading, it failed to load, or the workspace genuinely has no repository configured.

**Architecture:** `WorkspaceGitTab.tsx` calls `useWorkspaces()` and defaults its data to `[]` (`const { data: workspaces = [] } = useWorkspaces();`), which collapses "still loading" and "query failed" into the same empty-array shape as "loaded, zero workspaces" — the component never reads `isLoading`/`isError` at all. It then looks up `workspaces.find((w) => w.id === (workspaceId || activeWorkspaceId))`; since `workspaceId` is a required, non-empty `string` prop and its only caller (`EditorGroup.tsx`, via `WorkspaceTab.workspaceId: string`) always supplies it, the `|| activeWorkspaceId` fallback is dead code that never actually triggers. This plan makes the component read `isLoading`/`isError`/`refetch` from the query and render a distinct state for each, and treats `workspaceId` as the sole, authoritative lookup key.

**Tech Stack:** React 18, TypeScript, TanStack Query, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-14 "Workspace Git tab conflates query loading/error with 'no path'"). Verified present in current `src/components/workspace/WorkspaceGitTab.tsx:1-24`. Note: the review's claim that the fallback display name is "Collection" is stale — the current code already uses `workspace?.name ?? 'Workspace'`, which this plan keeps.

**Depends on:** `docs/superpowers/plans/2026-09-20-git-panel-store-identity-key.md` (adds `key={repositoryId}` to the `<GitPanel>` call in this file) — the code shown below already includes that `key` prop. If that plan hasn't been applied yet, omit `key={repositoryId}` from the final `<GitPanel>` render below; this plan's own changes are otherwise independent of it.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-diff-mode-toggle-hardening.md` next (plan 18 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only — the new loading/error states use `Loader2` (already the project's spinner icon, from `lucide-react`) and shadcn `Button` for retry.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Render distinct loading, error, missing-workspace, and no-repository states

**Files:**
- Modify: `src/components/workspace/WorkspaceGitTab.tsx`
- Test: `src/components/workspace/__tests__/WorkspaceGitTab.test.tsx` (new file — check first with `ls src/components/workspace/__tests__/` in case one already exists to extend instead)

**Interfaces:**
- Consumes: `useWorkspaces()` from `@/lib/queries/workspace-queries` — its full `UseQueryResult`-shaped return (`data`, `isLoading`, `isError`, `error`, `refetch`), not just `data`.
- Produces: no new exports. `WorkspaceGitTab` renders one of four states (loading / error / not-found / no-repository) before falling through to the existing `GitPanel` render.

- [ ] **Step 1: Write the failing test**

Create `src/components/workspace/__tests__/WorkspaceGitTab.test.tsx` (or extend the existing one if `ls` found it):

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { WorkspaceGitTab } from '@/components/workspace/WorkspaceGitTab';
import * as workspaceQueries from '@/lib/queries/workspace-queries';

vi.mock('@/lib/queries/workspace-queries', async () => {
  const actual =
    await vi.importActual<typeof workspaceQueries>('@/lib/queries/workspace-queries');
  return { ...actual, useWorkspaces: vi.fn() };
});

vi.mock('@/components/git/GitPanel', () => ({
  GitPanel: ({ repositoryLabel }: { repositoryLabel: string }) => (
    <div>Git panel for {repositoryLabel}</div>
  ),
}));

function mockUseWorkspaces(partial: Partial<ReturnType<typeof workspaceQueries.useWorkspaces>>) {
  vi.mocked(workspaceQueries.useWorkspaces).mockReturnValue(
    partial as ReturnType<typeof workspaceQueries.useWorkspaces>,
  );
}

describe('WorkspaceGitTab query lifecycle', () => {
  it('shows a loading state while the workspace list is loading', () => {
    mockUseWorkspaces({ data: undefined, isLoading: true, isError: false, error: null });
    render(<WorkspaceGitTab workspaceId='ws-1' />);
    expect(screen.getByText(/loading workspace/i)).toBeInTheDocument();
    expect(screen.queryByText(/no workspace repository configured/i)).not.toBeInTheDocument();
  });

  it('shows a retryable error state when the query fails', async () => {
    const refetch = vi.fn();
    mockUseWorkspaces({
      data: undefined,
      isLoading: false,
      isError: true,
      error: new Error('network down'),
      refetch,
    });
    render(<WorkspaceGitTab workspaceId='ws-1' />);
    expect(screen.getByText(/failed to load workspaces/i)).toBeInTheDocument();
    expect(screen.getByText(/network down/i)).toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /retry/i }));
    expect(refetch).toHaveBeenCalled();
  });

  it('shows a not-found state when the workspace id has no match', () => {
    mockUseWorkspaces({ data: [], isLoading: false, isError: false, error: null });
    render(<WorkspaceGitTab workspaceId='ws-missing' />);
    expect(screen.getByText(/workspace not found/i)).toBeInTheDocument();
  });

  it('shows the no-repository message only when the workspace is actually loaded and lacks one', () => {
    mockUseWorkspaces({
      data: [{ id: 'ws-1', name: 'My Workspace', repositoryId: null } as never],
      isLoading: false,
      isError: false,
      error: null,
    });
    render(<WorkspaceGitTab workspaceId='ws-1' />);
    expect(screen.getByText(/no workspace repository configured/i)).toBeInTheDocument();
  });

  it('renders the Git panel once the workspace and its repository are resolved', () => {
    mockUseWorkspaces({
      data: [{ id: 'ws-1', name: 'My Workspace', repositoryId: 'repo-1' } as never],
      isLoading: false,
      isError: false,
      error: null,
    });
    render(<WorkspaceGitTab workspaceId='ws-1' />);
    expect(screen.getByText('Git panel for My Workspace')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/workspace/__tests__/WorkspaceGitTab.test.tsx`
Expected: FAIL — the loading/error/not-found states don't exist yet; `data: undefined` defaults to `[]` via the current destructure, so the loading and error tests instead render "No workspace repository configured." (a false negative for both).

- [ ] **Step 3: Implement the lifecycle states**

Replace `src/components/workspace/WorkspaceGitTab.tsx`:

```tsx
import { Loader2 } from 'lucide-react';
import { GitPanel } from '@/components/git/GitPanel';
import { Button } from '@/components/ui/button';
import { useWorkspaces } from '@/lib/queries/workspace-queries';

interface WorkspaceGitTabProps {
  workspaceId: string;
}

export function WorkspaceGitTab({ workspaceId }: WorkspaceGitTabProps) {
  const { data: workspaces, isLoading, isError, error, refetch } = useWorkspaces();

  if (isLoading) {
    return (
      <div className='flex items-center justify-center gap-2 h-full text-sm text-muted-foreground'>
        <Loader2 className='h-4 w-4 animate-spin' />
        Loading workspace…
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex flex-col items-center justify-center gap-2 h-full px-4 text-center'>
        <p className='text-sm text-destructive'>Failed to load workspaces.</p>
        <p className='text-xs text-muted-foreground wrap-break-word max-w-sm'>{String(error)}</p>
        <Button variant='outline' size='sm' onClick={() => void refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  const workspace = workspaces?.find((w) => w.id === workspaceId);

  if (!workspace) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        Workspace not found.
      </div>
    );
  }

  const repositoryId = workspace.repositoryId ?? null;

  if (!repositoryId) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        No workspace repository configured.
      </div>
    );
  }

  return (
    <GitPanel key={repositoryId} repositoryId={repositoryId} repositoryLabel={workspace.name ?? 'Workspace'} />
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/workspace/__tests__/WorkspaceGitTab.test.tsx`
Expected: PASS

- [ ] **Step 5: Typecheck**

Run: `yarn tsc --noEmit`
Expected: no errors — if `Workspace`'s `repositoryId`/`name` field types don't match the `as never` casts used in the test fixtures above, replace those casts with a real `Workspace` object built from the actual type's required fields (check `src/lib/tauri-api.ts`'s `Workspace` interface) rather than loosening the cast further.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/workspace/WorkspaceGitTab.tsx src/components/workspace/__tests__/WorkspaceGitTab.test.tsx
```

Commit message: `fix(workspace): distinguish loading/error/not-found from no-repository-configured in the Git tab`.
