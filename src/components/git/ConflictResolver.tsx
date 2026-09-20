import '@/components/editor/monaco-setup';
import Editor from '@monaco-editor/react';
import { AlertCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
import type { ConflictState } from '@/types/pane-types';

interface ConflictResolverProps {
  conflictState: ConflictState;
  /** Called after a resolve completes without error — lets the parent
   *  return to a landing view instead of leaving this resolver mounted
   *  and re-armed against a conflict that no longer exists. */
  onResolved?: () => void;
}

export function ConflictResolver({ conflictState, onResolved }: ConflictResolverProps) {
  const [manualMode, setManualMode] = useState(false);
  const [manualContent, setManualContent] = useState(conflictState.ours);
  const [busy, setBusy] = useState(false);
  const [showAbortConfirm, setShowAbortConfirm] = useState(false);
  const resolveConflict = useGitStore((s) => s.resolveConflict);
  const abortMerge = useGitStore((s) => s.abortMerge);
  const error = useGitStore((s) => s.error);
  const clearError = useGitStore((s) => s.clearError);
  const gitStoreApi = useGitStoreApi();
  const { themeName } = useMonacoTheme();

  // Reset manual-editing state whenever the target conflict changes — without
  // this, selecting a different conflicted file while in manual mode kept
  // editing the previous file's content and could save it into the new one.
  // biome-ignore lint/correctness/useExhaustiveDependencies: filePath is the intentional trigger — ours alone could coincidentally match across two different conflicted files and fail to reset.
  useEffect(() => {
    setManualMode(false);
    setManualContent(conflictState.ours);
  }, [conflictState.filePath, conflictState.ours]);

  const handleConfirmAbort = async () => {
    if (busy) return;
    setShowAbortConfirm(false);
    setBusy(true);
    try {
      await abortMerge();
    } finally {
      setBusy(false);
    }
  };

  const handleResolve = async (resolution: 'ours' | 'theirs' | 'custom', content?: string) => {
    if (busy) return;
    setBusy(true);
    try {
      const res =
        resolution === 'custom'
          ? { resolution: 'custom' as const, content: content ?? '' }
          : { resolution };
      clearError();
      await resolveConflict(conflictState.filePath, res);
      // resolveConflict never throws — a failed resolve surfaces as `error`
      // state instead. Only leave the resolver (via onResolved) when the
      // call actually succeeded, so a failure — including a repeat of the
      // exact same error as before — doesn't navigate the user away from
      // the error they need to see.
      if (!gitStoreApi.getState().error) {
        onResolved?.();
      }
    } finally {
      setBusy(false);
    }
  };

  const abortConfirmDialog = (
    <AlertDialog open={showAbortConfirm} onOpenChange={setShowAbortConfirm}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Abort Merge?</AlertDialogTitle>
          <AlertDialogDescription>
            This resets the working tree and index to <span className='font-mono'>HEAD</span>,
            discarding any staged or unstaged tracked changes made since the merge started,
            including any conflict resolutions you've already saved. Untracked files are not
            affected. This cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={handleConfirmAbort} disabled={busy}>
            Confirm Abort
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );

  if (manualMode) {
    return (
      <div className='flex flex-col h-full'>
        <div className='flex items-center gap-2 border-b px-3 py-1.5'>
          <Badge variant='destructive' className='text-[9px]'>
            Conflict
          </Badge>
          <span className='font-mono text-sm truncate'>{conflictState.filePath}</span>
          <div className='ml-auto flex gap-1'>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm text-destructive'
              onClick={() => setShowAbortConfirm(true)}
              disabled={busy}
            >
              Abort Merge
            </Button>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm'
              onClick={() => setManualMode(false)}
              disabled={busy}
            >
              Back
            </Button>
            <Button
              size='sm'
              className='h-6 text-sm'
              onClick={() => handleResolve('custom', manualContent)}
              disabled={busy}
            >
              Save Resolution
            </Button>
          </div>
        </div>
        {error && (
          <div className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
            <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
            <span className='flex-1 wrap-break-word'>{error}</span>
            <button
              type='button'
              className='shrink-0 hover:opacity-70 leading-none'
              onClick={clearError}
              aria-label='Dismiss error'
            >
              ×
            </button>
          </div>
        )}
        <div className='flex-1'>
          <Editor
            value={manualContent}
            onChange={(v) => setManualContent(v ?? '')}
            theme={themeName}
            options={{ minimap: { enabled: false }, fontSize: 12, scrollBeyondLastLine: false }}
          />
        </div>
        {abortConfirmDialog}
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full'>
      <div className='flex items-center gap-2 border-b px-3 py-1.5'>
        <Badge variant='destructive' className='text-[9px]'>
          Conflict
        </Badge>
        <span className='font-mono text-sm truncate'>{conflictState.filePath}</span>
        <div className='ml-auto'>
          <Button
            variant='outline'
            size='sm'
            className='h-6 text-sm text-destructive'
            onClick={() => setShowAbortConfirm(true)}
            disabled={busy}
          >
            Abort Merge
          </Button>
        </div>
      </div>
      {error && (
        <div className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
          <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
          <span className='flex-1 wrap-break-word'>{error}</span>
          <button
            type='button'
            className='shrink-0 hover:opacity-70 leading-none'
            onClick={clearError}
            aria-label='Dismiss error'
          >
            ×
          </button>
        </div>
      )}
      <div className='flex flex-1 min-h-0'>
        <div className='flex-1 flex flex-col border-r'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Ours</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.ours}
              theme={themeName}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        </div>
        <div className='flex-1 flex flex-col'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Theirs</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.theirs}
              theme={themeName}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        </div>
      </div>
      <div className='flex items-center gap-2 border-t px-3 py-2'>
        <Button variant='outline' size='sm' onClick={() => handleResolve('ours')} disabled={busy}>
          Accept Ours
        </Button>
        <Button variant='outline' size='sm' onClick={() => handleResolve('theirs')} disabled={busy}>
          Accept Theirs
        </Button>
        <Button variant='secondary' size='sm' onClick={() => setManualMode(true)} disabled={busy}>
          Edit Manually
        </Button>
      </div>
      {abortConfirmDialog}
    </div>
  );
}
