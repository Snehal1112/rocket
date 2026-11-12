import { open } from '@tauri-apps/plugin-dialog';
import { FileSpreadsheet, TriangleAlert, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { type IterationData, loadIterationData } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';

type Preview =
  | { kind: 'loading' }
  | { kind: 'none' }
  | { kind: 'loaded'; data: IterationData }
  | { kind: 'error'; message: string };

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function rowsLabel(count: number): string {
  return `${count} ${count === 1 ? 'row' : 'rows'}`;
}

// The data file of a Runner tab: pick a CSV or JSON file, or fall back to the
// default saved in the collection settings. One pass runs per row.
export function RunnerDataFileRow({ tab, collection }: { tab: RunnerTab; collection: string }) {
  const setRunnerDataFile = usePaneStore((s) => s.setRunnerDataFile);
  const [preview, setPreview] = useState<Preview>({ kind: 'loading' });
  const dataFilePath = tab.dataFilePath;
  const isRunning = tab.runState === 'running';

  useEffect(() => {
    let cancelled = false;
    setPreview({ kind: 'loading' });
    loadIterationData(collection, dataFilePath)
      .then((data) => {
        if (!cancelled) setPreview(data ? { kind: 'loaded', data } : { kind: 'none' });
      })
      .catch((err) => {
        if (!cancelled) setPreview({ kind: 'error', message: errorMessage(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [collection, dataFilePath]);

  const pick = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Data files', extensions: ['csv', 'json'] }],
      });
      if (typeof selected === 'string') setRunnerDataFile(tab.id, selected);
    } catch (err) {
      console.error('[RunnerDataFileRow] failed to open the file dialog', err);
    }
  };

  const error = tab.dataFileError ?? (preview.kind === 'error' ? preview.message : undefined);
  const hasOverride = Boolean(dataFilePath);

  let status: string;
  if (preview.kind === 'loaded') {
    const { data } = preview;
    const lead =
      data.source === 'savedDefault' ? `Using saved default: ${data.fileName}` : data.fileName;
    status = `${lead}, ${rowsLabel(data.rows.length)} (${data.columns.join(', ')})`;
  } else if (preview.kind === 'none') {
    status = 'No data file, the run goes once.';
  } else {
    status = hasOverride ? 'Reading the file' : '';
  }

  return (
    <div className='flex items-center gap-2 px-3 py-2 border-b shrink-0 text-sm'>
      <FileSpreadsheet className='h-4 w-4 shrink-0 text-muted-foreground' />
      <span className='shrink-0 font-medium'>Data file</span>
      {error ? (
        <span
          role='alert'
          className='flex min-w-0 items-center gap-1.5 text-destructive'
          title={error}
        >
          <TriangleAlert className='h-3.5 w-3.5 shrink-0' />
          <span className='truncate'>
            {hasOverride ? error : `The saved default data file cannot be read: ${error}`}
          </span>
        </span>
      ) : (
        <span className='min-w-0 truncate text-muted-foreground' title={status}>
          {status}
        </span>
      )}
      <div className='ml-auto flex shrink-0 items-center gap-1'>
        {hasOverride && (
          <Button
            size='sm'
            variant='ghost'
            disabled={isRunning}
            aria-label='Use saved default'
            title='Use saved default'
            onClick={() => setRunnerDataFile(tab.id, undefined)}
          >
            <X className='h-3.5 w-3.5' />
          </Button>
        )}
        <Button
          size='sm'
          variant='outline'
          disabled={isRunning}
          aria-label='Choose data file'
          onClick={() => void pick()}
        >
          Choose file
        </Button>
      </div>
    </div>
  );
}
