import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { listCollections } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';
import { usePaneStore } from '@/stores/pane-store';
import type { GitTab, LeafNode, PaneNode } from '@/types/pane-types';

/** Return the groupId of the leaf that contains a tab with the given id. */
function findTabGroupId(node: PaneNode, tabId: string): string | null {
  if (node.type === 'leaf') {
    return node.tabs.some((t) => t.id === tabId) ? node.groupId : null;
  }
  return findTabGroupId(node.children[0], tabId) ?? findTabGroupId(node.children[1], tabId);
}

/** Return the leaf node with the given groupId. */
function findLeaf(node: PaneNode, groupId: string): LeafNode | null {
  if (node.type === 'leaf') return node.groupId === groupId ? node : null;
  return findLeaf(node.children[0], groupId) ?? findLeaf(node.children[1], groupId);
}

/** Open the git panel for the active collection. Can be called from keyboard shortcuts. */
export async function openGitPanel(): Promise<void> {
  const { activeCollection, openTab, root, closeTab } = usePaneStore.getState();
  if (!activeCollection) return;

  let path = useGitStore.getState().collectionPath ?? '';
  if (!path) {
    try {
      const summaries = await listCollections();
      const match = summaries.find((s) => s.name === activeCollection);
      path = match?.path ?? '';
    } catch {
      // Fall through — GitPanel will show appropriate state.
    }
  }

  const tabId = `git:${activeCollection}`;

  if (path) {
    const groupId = findTabGroupId(root, tabId);
    if (groupId) {
      const leaf = findLeaf(root, groupId);
      const existingTab = leaf?.tabs.find((t) => t.id === tabId) as GitTab | undefined;
      if (existingTab && !existingTab.collectionPath) {
        closeTab(tabId, groupId);
      }
    }
  }

  const tab: GitTab = {
    id: tabId,
    title: 'Git UI',
    tabType: 'git',
    collectionName: activeCollection,
    collectionPath: path,
    isDirty: false,
  };
  openTab(tab);
}

export function GitToolbarButton() {
  const activeCollection = usePaneStore((s) => s.activeCollection);

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={() => void openGitPanel()}
            disabled={!activeCollection}
            aria-label='Open Git panel'
          >
            <GitBranch className='h-3.5 w-3.5 text-muted-foreground' />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Open Git panel (⌘⇧G)</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
