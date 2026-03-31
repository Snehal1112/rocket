import { useState, useEffect } from 'react';
import { Loader2 } from 'lucide-react';
import { DiffViewer } from './DiffViewer';
import { gitDiff, gitDiffStaged } from '@/lib/tauri-api';
import type { FileStatus } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';

interface DiffViewForFileProps {
  file: FileStatus;
  collectionPath: string;
}

export function DiffViewForFile({ file, collectionPath }: DiffViewForFileProps) {
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
          ? await gitDiffStaged(collectionPath, file.path)
          : await gitDiff(collectionPath, file.path);
        if (cancelled) return;
        setDiffState({
          filePath: file.path,
          collectionPath,
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
  }, [file.path, file.staged, collectionPath]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-sm text-muted-foreground">{error}</p>
      </div>
    );
  }

  if (!diffState) return null;

  return <DiffViewer key={`${diffState.filePath}:${diffState.isStaged}`} diffState={diffState} />;
}
