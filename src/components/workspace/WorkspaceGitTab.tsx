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
    <GitPanel
      key={repositoryId}
      repositoryId={repositoryId}
      repositoryLabel={workspace.name ?? 'Workspace'}
    />
  );
}
