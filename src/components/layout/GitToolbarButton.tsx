import { GitBranch } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { findTabInTree } from '@/lib/pane-utils';
import { type CollectionSummary, listCollections } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { GitTab } from '@/types/pane-types';

/** Open the git panel for the active collection. Can be called from keyboard shortcuts. */
export async function openGitPanel(): Promise<void> {
  const { activeCollection, openTab, root, closeTab } = usePaneStore.getState();
  if (!activeCollection) return;

  let summary: CollectionSummary | undefined;
  try {
    const summaries = await listCollections();
    summary = summaries.find((candidate) => candidate.name === activeCollection);
  } catch {
    toast.error('Failed to open Git panel: could not load collections.');
    return;
  }

  // The active collection may have changed while listCollections() was in
  // flight — don't open a Git tab for a collection the user has since
  // navigated away from.
  if (usePaneStore.getState().activeCollection !== activeCollection) return;

  if (!summary) {
    toast.error('Failed to open Git panel: collection not found.');
    return;
  }

  const tabId = `git:${activeCollection}`;
  const found = findTabInTree(root, tabId);
  if (found) {
    const existingTab = found.tab as Partial<GitTab>;
    if (
      existingTab.repositoryId !== summary.repositoryId ||
      existingTab.repositoryLabel !== summary.name
    ) {
      closeTab(tabId, found.leaf.groupId);
    }
  }

  const tab: GitTab = {
    id: tabId,
    title: 'Git UI',
    tabType: 'git',
    repositoryId: summary.repositoryId,
    repositoryLabel: summary.name,
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
