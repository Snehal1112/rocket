import { FileCode, Save } from 'lucide-react';
import { lazy, Suspense, useCallback, useState } from 'react';
import { toast } from 'sonner';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { findTabInTree } from '@/lib/pane-utils';
import { saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { isScriptTab, type ScriptTab } from '@/types/pane-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

interface ScriptFilePaneProps {
  tab: ScriptTab;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Editor for a shared `.js` file. Scripts load it with `require('./path.js')`. */
export function ScriptFilePane({ tab }: ScriptFilePaneProps) {
  const updateScriptContent = usePaneStore((s) => s.updateScriptContent);
  const markScriptSaved = usePaneStore((s) => s.markScriptSaved);
  const [saving, setSaving] = useState(false);

  const save = useCallback(async () => {
    // Read the latest content from the store, since the keyboard handler can be stale.
    const latest = findTabInTree(usePaneStore.getState().root, tab.id)?.tab;
    if (!latest || !isScriptTab(latest) || !latest.isDirty) return;
    const content = latest.content;
    setSaving(true);
    try {
      await saveScriptFile(latest.collectionName, latest.scriptPath, content);
      markScriptSaved(latest.id, content);
    } catch (err) {
      toast.error(`Could not save "${latest.title}": ${errorMessage(err)}`);
    } finally {
      setSaving(false);
    }
  }, [tab.id, markScriptSaved]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      void save();
    }
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: captures Ctrl+S from the editor inside.
    <div className='flex h-full min-h-0 flex-col' onKeyDown={onKeyDown}>
      <div className='flex items-center gap-2 border-b px-3 py-1.5 text-xs'>
        <FileCode aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
        <span className='truncate font-medium'>{tab.scriptPath}</span>
        <span className='truncate text-muted-foreground'>{`require('./${tab.scriptPath}')`}</span>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='ml-auto h-6 gap-1 px-2 text-xs'
          disabled={!tab.isDirty || saving}
          onClick={() => void save()}
        >
          <Save aria-hidden='true' className='h-3 w-3' /> Save
        </Button>
      </div>
      <div className='min-h-0 flex-1'>
        <Suspense fallback={<EditorSkeleton />}>
          <MonacoWrapper
            language='javascript'
            value={tab.content}
            onChange={(value) => updateScriptContent(tab.id, value)}
            height='100%'
            phase='tests'
          />
        </Suspense>
      </div>
    </div>
  );
}
