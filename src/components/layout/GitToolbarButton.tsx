import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
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

export function GitToolbarButton() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  const openTab = usePaneStore((s) => s.openTab);
  const collectionPath = useGitStore((s) => s.collectionPath);

  const handleClick = async () => {
    if (!activeCollection) return;

    // Prefer the git store's path; fall back to fetching from the collections API.
    // This handles the case where the git store was never initialized (e.g. on
    // app start when the collection was not selected via the dropdown).
    let path = collectionPath ?? '';
    if (!path) {
      try {
        const summaries = await listCollections();
        const match = summaries.find((s) => s.name === activeCollection);
        path = match?.path ?? '';
      } catch {
        // Fall through with empty path — GitPanel will show an appropriate state.
      }
    }

    const tabId = `git:${activeCollection}`;

    // If there is an existing git tab with an empty (stale) path and we now have
    // the correct path, close it first so we can reopen with the right path.
    if (path) {
      const { root, closeTab } = usePaneStore.getState();
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
  };

  return (
    <Button
      variant='ghost'
      size='icon'
      className='h-7 w-7'
      onClick={() => void handleClick()}
      disabled={!activeCollection}
      title='Open Git panel'
    >
      <GitBranch className='h-3.5 w-3.5 text-muted-foreground' />
    </Button>
  );
}
