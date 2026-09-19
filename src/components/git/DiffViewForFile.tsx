import { Loader2 } from 'lucide-react';
import { lazy, Suspense, useEffect, useState } from 'react';
import type { FileStatus } from '@/lib/tauri-api';
import { gitDiff, gitDiffStaged } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';

const DiffViewer = lazy(() => import('./DiffViewer').then((m) => ({ default: m.DiffViewer })));

interface DiffViewForFileProps {
  file: FileStatus;
  repositoryId: string;
  repositoryLabel: string;
}

export function DiffViewForFile({ file, repositoryId, repositoryLabel }: DiffViewForFileProps) {
  const [diffState, setDiffState] = useState<DiffState | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setDiffState(null);

    const fetchDiff = async () => {
      try {
        const diff = file.staged
          ? await gitDiffStaged(repositoryId, file.path)
          : await gitDiff(repositoryId, file.path);
        if (cancelled) return;
        setDiffState({
          filePath: file.path,
          repositoryId,
          repositoryLabel,
          oldContent: diff.oldContent ?? '',
          newContent: diff.newContent ?? '',
          status: file.status,
          isStaged: file.staged,
        });
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    void fetchDiff();
    return () => {
      cancelled = true;
    };
  }, [file.path, file.staged, file.status, repositoryId, repositoryLabel]);

  if (loading) {
    return (
      <div className='flex items-center justify-center h-full'>
        <Loader2 className='h-5 w-5 animate-spin text-muted-foreground' />
      </div>
    );
  }

  if (error) {
    return (
      <div className='flex items-center justify-center h-full'>
        <p className='text-sm text-muted-foreground'>{error}</p>
      </div>
    );
  }

  if (!diffState) return null;

  return (
    <Suspense fallback={<Loader2 className='h-5 w-5 animate-spin text-muted-foreground m-auto' />}>
      <DiffViewer key={`${diffState.filePath}:${diffState.isStaged}`} diffState={diffState} />
    </Suspense>
  );
}
