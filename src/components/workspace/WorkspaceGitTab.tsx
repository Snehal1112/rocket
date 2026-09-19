import { GitPanel } from '@/components/git/GitPanel';
import { useWorkspaces } from '@/lib/queries/workspace-queries';
import { useWorkspaceStore } from '@/stores/workspace-store';

interface WorkspaceGitTabProps {
  workspaceId: string;
}

export function WorkspaceGitTab({ workspaceId }: WorkspaceGitTabProps) {
  const { data: workspaces = [] } = useWorkspaces();
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspace = workspaces.find((w) => w.id === (workspaceId || activeWorkspaceId));
  const repositoryId = workspace?.repositoryId ?? null;

  if (!repositoryId) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        No workspace repository configured.
      </div>
    );
  }

  return <GitPanel repositoryId={repositoryId} repositoryLabel={workspace?.name ?? 'Workspace'} />;
}
