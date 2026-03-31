import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { PaneNode } from '@/types/pane-types';

export function GitToolbarButton() {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const isWorkspaceMode = usePaneStore((s) => s.isWorkspaceMode);
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const root = usePaneStore((s) => s.root);

  const handleClick = () => {
    if (!activeWorkspaceId) return;
    const gitTabId = `workspace:${activeWorkspaceId}:git`;

    // Find which editor group contains the git tab.
    const groupId = findGroupWithTab(root, gitTabId);
    if (groupId) {
      setActiveTab(gitTabId, groupId);
    }
  };

  const enabled = isWorkspaceMode() && !!activeWorkspaceId;

  return (
    <Button
      variant="ghost"
      size="icon"
      className="h-7 w-7"
      onClick={handleClick}
      disabled={!enabled}
      title="Open Git panel"
    >
      <GitBranch className="h-4 w-4" />
    </Button>
  );
}

// Walk the pane tree to find which group contains the given tab id.
function findGroupWithTab(node: PaneNode, tabId: string): string | null {
  if (node.type === 'leaf') {
    return node.tabs.some((t) => t.id === tabId) ? node.groupId : null;
  }
  return findGroupWithTab(node.children[0], tabId) ?? findGroupWithTab(node.children[1], tabId);
}
