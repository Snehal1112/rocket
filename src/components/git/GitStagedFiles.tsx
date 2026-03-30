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
      <div className="flex items-center justify-between px-2 py-1.5">
        <span className="text-[11px] uppercase tracking-[0.06em] font-semibold text-muted-foreground">
          Staged Changes
          <span className="ml-1.5 font-mono normal-case tracking-normal opacity-70">{staged.length}</span>
        </span>
        <Button variant="ghost" size="sm" className="h-5 px-1.5 text-[11px] text-muted-foreground" onClick={() => unstageAll()}>
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
