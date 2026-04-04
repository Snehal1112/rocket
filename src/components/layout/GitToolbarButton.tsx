import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import { usePaneStore } from '@/stores/pane-store';
import type { GitTab } from '@/types/pane-types';

export function GitToolbarButton() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  const openTab = usePaneStore((s) => s.openTab);
  const collectionPath = useGitStore((s) => s.collectionPath);

  const handleClick = () => {
    if (!activeCollection) return;
    const tab: GitTab = {
      id: `git:${activeCollection}`,
      title: 'Git UI',
      tabType: 'git',
      collectionName: activeCollection,
      collectionPath: collectionPath ?? '',
      isDirty: false,
    };
    openTab(tab);
  };

  return (
    <Button
      variant='ghost'
      size='icon'
      className='h-7 w-7'
      onClick={handleClick}
      disabled={!activeCollection}
      title='Open Git panel'
    >
      <GitBranch className='h-3.5 w-3.5 text-muted-foreground' />
    </Button>
  );
}
