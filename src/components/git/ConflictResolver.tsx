import Editor, { loader, type OnMount } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';

loader.config({ monaco });

import { useState } from 'react';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { gitResolveConflict } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';
import type { ConflictState } from '@/types/pane-types';

interface ConflictResolverProps {
  conflictState: ConflictState;
}

export function ConflictResolver({ conflictState }: ConflictResolverProps) {
  const [manualMode, setManualMode] = useState(false);
  const [manualContent, setManualContent] = useState(conflictState.ours);
  const { refreshStatus, abortMerge } = useGitStore();
  const { themeName, defineThemes } = useMonacoTheme();
  const handleMount: OnMount = (_editor, monaco) => {
    defineThemes(monaco);
  };

  const handleAbort = async () => {
    await abortMerge();
  };

  const handleResolve = async (resolution: 'ours' | 'theirs' | 'custom', content?: string) => {
    try {
      const res =
        resolution === 'custom'
          ? { resolution: 'custom' as const, content: content ?? '' }
          : { resolution };
      await gitResolveConflict(conflictState.collectionPath, conflictState.filePath, res);
      await refreshStatus();
    } catch {
      // Handle silently.
    }
  };

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
              onClick={handleAbort}
            >
              Abort Merge
            </Button>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm'
              onClick={() => setManualMode(false)}
            >
              Back
            </Button>
            <Button
              size='sm'
              className='h-6 text-sm'
              onClick={() => handleResolve('custom', manualContent)}
            >
              Save Resolution
            </Button>
          </div>
        </div>
        <div className='flex-1'>
          <Editor
            value={manualContent}
            onChange={(v) => setManualContent(v ?? '')}
            theme={themeName}
            onMount={handleMount}
            options={{ minimap: { enabled: false }, fontSize: 12, scrollBeyondLastLine: false }}
          />
        </div>
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
            onClick={handleAbort}
          >
            Abort Merge
          </Button>
        </div>
      </div>
      <div className='flex flex-1 min-h-0'>
        <div className='flex-1 flex flex-col border-r'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Ours</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.ours}
              theme={themeName}
              onMount={handleMount}
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
              onMount={handleMount}
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
        <Button variant='outline' size='sm' onClick={() => handleResolve('ours')}>
          Accept Ours
        </Button>
        <Button variant='outline' size='sm' onClick={() => handleResolve('theirs')}>
          Accept Theirs
        </Button>
        <Button variant='secondary' size='sm' onClick={() => setManualMode(true)}>
          Edit Manually
        </Button>
      </div>
    </div>
  );
}
