import { GitPanel } from '@/components/git/GitPanel';
import { useWorkspaceStore } from '@/stores/workspace-store';

interface WorkspaceGitTabProps {
  workspaceId: string;
}

export function WorkspaceGitTab({ workspaceId }: WorkspaceGitTabProps) {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const workspace = workspaces.find((w) => w.id === workspaceId);
  const workspacePath = workspace?.path ?? null;

  if (!workspacePath) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        No workspace path configured.
      </div>
    );
  }

  return <GitPanel collectionPath={workspacePath} collectionName={workspace?.name ?? 'Collection'} />;
}
