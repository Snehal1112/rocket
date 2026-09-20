import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { WorkspaceGitTab } from '@/components/workspace/WorkspaceGitTab';
import * as workspaceQueries from '@/lib/queries/workspace-queries';

vi.mock('@/lib/queries/workspace-queries', async () => {
  const actual = await vi.importActual<typeof workspaceQueries>('@/lib/queries/workspace-queries');
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
