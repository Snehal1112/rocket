import { useEffect, useState } from 'react';
import { DiffViewer } from '@/components/git/DiffViewer';
import { StateBlock } from '@/components/ui/state-block';
import {
  type ConflictFiles,
  type ConflictView,
  getNetworkSyncConflictFiles,
} from '@/lib/tauri-api';

interface SyncConflictDiffProps {
  syncId: string;
  conflict: ConflictView;
  /** The display name of the other node, already passed through `peerName`. */
  peerName: string;
}

type LoadState =
  | { status: 'loading' }
  | { status: 'ready'; files: ConflictFiles }
  | { status: 'error' };

function caption(files: ConflictFiles, peerName: string): string {
  if (files.mine === null) return `You have no copy of this file. Below: the copy of ${peerName}.`;
  if (files.theirs === null) return `${peerName} has no copy of this file. Below: your copy.`;
  return `Left: your copy. Right: the copy of ${peerName}.`;
}

// A missing side is a whole-file add or delete, which the viewer shows inline.
function diffStatus(files: ConflictFiles): string {
  if (files.mine === null) return 'added';
  if (files.theirs === null) return 'deleted';
  return 'modified';
}

/**
 * Loads both versions of one conflicting file and shows them in the read-only git diff viewer,
 * mine on the left, theirs on the right. It changes nothing: the choices live in the dialog.
 */
export function SyncConflictDiff({ syncId, conflict, peerName }: SyncConflictDiffProps) {
  const [state, setState] = useState<LoadState>({ status: 'loading' });
  const { path } = conflict;

  useEffect(() => {
    let cancelled = false;
    setState({ status: 'loading' });
    getNetworkSyncConflictFiles(syncId, path)
      .then((files) => {
        if (!cancelled) setState({ status: 'ready', files });
      })
      .catch(() => {
        // The rejection text may name a path or a file, so it is not shown.
        if (!cancelled) setState({ status: 'error' });
      });
    return () => {
      cancelled = true;
    };
  }, [syncId, path]);

  if (state.status === 'loading')
    return <StateBlock tone='loading'>Loading the files...</StateBlock>;
  if (state.status === 'error') {
    return (
      <StateBlock tone='error'>The files could not be loaded. You can still choose.</StateBlock>
    );
  }
  return (
    <div className='space-y-1'>
      <p className='text-2xs text-muted-foreground'>{caption(state.files, peerName)}</p>
      <div className='h-72 overflow-hidden rounded-md border'>
        {/* The key is needed because DiffViewer copies its prop into state once. */}
        <DiffViewer
          key={`${syncId}:${path}`}
          hideStageToggle
          diffState={{
            filePath: path,
            repositoryId: '',
            repositoryLabel: 'Sync',
            oldContent: state.files.mine ?? '',
            newContent: state.files.theirs ?? '',
            status: diffStatus(state.files),
            isStaged: false,
          }}
        />
      </div>
    </div>
  );
}
