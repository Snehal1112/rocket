import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import { gitDiffStaged } from '@/lib/tauri-api';
import type { FileStatus } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { GitFileRow } from './GitFileRow';

export function GitStagedFiles() {
  const { status, unstageFiles, unstageAll, collectionPath } = useGitStore();
  const openDiffTab = usePaneStore((s) => s.openDiffTab);

  const handleFileClick = async (file: FileStatus) => {
    if (!collectionPath) return;
    try {
      const diff = await gitDiffStaged(collectionPath, file.path);
      openDiffTab({
        filePath: file.path,
        collectionPath,
        oldContent: diff.oldContent ?? '',
        newContent: diff.newContent ?? '',
        status: file.status,
        isStaged: true,
      });
    } catch {
      // Ignore errors silently.
    }
  };

  const staged = status?.files.filter((f) => f.staged) ?? [];

  if (staged.length === 0) return null;

  return (
    <div>
      <div className="flex items-center justify-between px-2 py-1">
        <span className="text-xs font-medium text-muted-foreground">
          Staged ({staged.length})
        </span>
        <Button variant="ghost" size="sm" className="h-5 text-xs" onClick={() => unstageAll()}>
          Unstage all
        </Button>
      </div>
      <div>
        {staged.map((file) => (
          <GitFileRow
            key={file.path}
            file={file}
            onUnstage={() => unstageFiles([file.path])}
            onClick={() => handleFileClick(file)}
          />
        ))}
      </div>
    </div>
  );
}
