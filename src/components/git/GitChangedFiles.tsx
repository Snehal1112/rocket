import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import { GitFileRow } from './GitFileRow';

export function GitChangedFiles() {
  const { status, stageFiles, discardFiles, stageAll } = useGitStore();

  const unstaged = status?.files.filter((f) => !f.staged && f.status !== 'unchanged') ?? [];

  if (unstaged.length === 0) return null;

  return (
    <div>
      <div className="flex items-center justify-between px-2 py-1">
        <span className="text-xs font-medium text-muted-foreground">
          Changes ({unstaged.length})
        </span>
        <Button variant="ghost" size="sm" className="h-5 text-xs" onClick={() => stageAll()}>
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
          />
        ))}
      </div>
    </div>
  );
}
