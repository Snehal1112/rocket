import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { findTabInTree } from '@/lib/pane-utils';
import { listCollections } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';
import { usePaneStore } from '@/stores/pane-store';
import type { GitTab } from '@/types/pane-types';

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
    const found = findTabInTree(root, tabId);
    if (found) {
      const existingTab = found.tab as GitTab;
      if (!existingTab.collectionPath) {
        closeTab(tabId, found.leaf.groupId);
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
            className='h-7 w-7 hover:bg-toolbar-hover'
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
