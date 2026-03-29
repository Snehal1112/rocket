import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import { gitDiff, gitConflicts } from '@/lib/tauri-api';
import type { FileStatus } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { GitFileRow } from './GitFileRow';

export function GitChangedFiles() {
  const { status, stageFiles, discardFiles, stageAll, collectionPath } = useGitStore();
  const openDiffTab = usePaneStore((s) => s.openDiffTab);
  const openConflictTab = usePaneStore((s) => s.openConflictTab);

  const handleFileClick = async (file: FileStatus) => {
    if (!collectionPath) return;
    try {
      if (file.status === 'conflicted') {
        const conflicts = await gitConflicts(collectionPath);
        const conflict = conflicts.find((c) => c.path === file.path);
        if (conflict) {
          openConflictTab({
            filePath: conflict.path,
            collectionPath,
            ours: conflict.ours,
            theirs: conflict.theirs,
            ancestor: conflict.ancestor ?? null,
          });
        }
        return;
      }
      const diff = await gitDiff(collectionPath, file.path);
      openDiffTab({
        filePath: file.path,
        collectionPath,
        oldContent: diff.oldContent ?? '',
        newContent: diff.newContent ?? '',
        status: file.status,
        isStaged: false,
      });
    } catch {
      // Ignore errors silently.
    }
  };

  const unstaged = status?.files.filter((f) => !f.staged && f.status !== 'unchanged') ?? [];

  if (unstaged.length === 0) return null;

  return (
    <div>
      <div className="flex items-center justify-between px-2 py-1">
        <span className="text-sm font-medium text-muted-foreground">
          Changes ({unstaged.length})
        </span>
        <Button variant="ghost" size="sm" className="h-5 text-sm" onClick={() => stageAll()}>
          Stage all
        </Button>
      </div>
      <div>
        {unstaged.map((file) => (
          <GitFileRow
            key={file.path}
            file={file}
            onStage={() => stageFiles([file.path])}
            onDiscard={() => discardFiles([file.path])}
            onClick={() => handleFileClick(file)}
          />
        ))}
      </div>
    </div>
  );
}
