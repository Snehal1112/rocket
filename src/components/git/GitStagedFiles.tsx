import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import { GitFileRow } from './GitFileRow';

export function GitStagedFiles() {
  const { status, unstageFiles, unstageAll } = useGitStore();

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
          />
        ))}
      </div>
    </div>
  );
}
