import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { ChevronDown, ChevronRight, Loader2, Upload } from 'lucide-react';
import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { type ImportReport, importBruno, importBrunoZip } from '@/lib/tauri-api';

interface ImportBrunoDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceId: string;
  onImportComplete?: () => void;
}

type SourceKind = 'folder' | 'zip';
type DialogState = 'picking' | 'importing' | 'done';

interface SelectedSource {
  path: string;
  kind: SourceKind;
  name: string;
}

export function ImportBrunoDialog({
  open,
  onOpenChange,
  workspaceId,
  onImportComplete,
}: ImportBrunoDialogProps) {
  const [source, setSource] = useState<SelectedSource | null>(null);
  const [dialogState, setDialogState] = useState<DialogState>('picking');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [skippedOpen, setSkippedOpen] = useState(false);

  function handleClose() {
    onOpenChange(false);
    // Reset state after dialog animates out.
    setTimeout(() => {
      setSource(null);
      setDialogState('picking');
      setReport(null);
      setError(null);
      setSkippedOpen(false);
    }, 200);
  }

  function clearSource() {
    setSource(null);
    setError(null);
  }

  async function handleChooseFolder() {
    const path = await openFilePicker({ directory: true, multiple: false });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'folder', name });
      setError(null);
    }
  }

  async function handleChooseZip() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'ZIP Archives', extensions: ['zip'] }],
    });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'zip', name });
      setError(null);
    }
  }

  async function handleImport() {
    if (!source) return;
    setDialogState('importing');
    setError(null);
    try {
      const result =
        source.kind === 'zip'
          ? await importBrunoZip(source.path, workspaceId)
          : await importBruno(source.path, workspaceId);
      setReport(result);
      setDialogState('done');
      onImportComplete?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setDialogState('picking');
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) handleClose();
      }}
    >
      <DialogContent className='sm:max-w-md'>
        {dialogState === 'picking' && (
          <>
            <DialogHeader>
              <DialogTitle>Import from Bruno</DialogTitle>
              <DialogDescription>
                Supports Bruno 2.x and 3.x formats. Collection or workspace is detected
                automatically.
              </DialogDescription>
            </DialogHeader>

            <div className='space-y-3 py-2'>
              <p className='text-xs font-semibold uppercase tracking-wide text-muted-foreground'>
                Source
              </p>

              {/* Drop zone */}
              <div
                className={[
                  'rounded-lg border-[1.5px] border-dashed px-5 py-6 text-center transition-colors',
                  source
                    ? 'border-primary bg-primary/5'
                    : 'border-border hover:border-muted-foreground/40',
                ].join(' ')}
              >
                {source ? (
                  <>
                    <div className='mb-2 flex h-9 w-9 items-center justify-center rounded-lg border bg-primary/10 mx-auto text-lg'>
                      {source.kind === 'zip' ? '🗜️' : '📁'}
                    </div>
                    <p className='text-sm font-semibold text-foreground'>{source.name}</p>
                    <p className='mt-0.5 text-xs text-muted-foreground'>
                      {source.kind === 'zip' ? 'ZIP archive' : 'Folder'}
                    </p>
                  </>
                ) : (
                  <>
                    <div className='mb-2 flex h-9 w-9 items-center justify-center rounded-lg border bg-muted mx-auto'>
                      <Upload className='h-4 w-4 text-muted-foreground' />
                    </div>
                    <p className='text-sm font-medium text-muted-foreground'>
                      Drop a folder or ZIP here
                    </p>
                    <p className='mt-0.5 text-xs text-muted-foreground'>
                      Bruno export or extracted directory
                    </p>
                  </>
                )}

                <div className='mt-3 flex items-center justify-center gap-1 text-xs text-muted-foreground'>
                  {source ? (
                    <>
                      change:
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseFolder()}
                      >
                        folder
                      </button>
                      <span className='text-border'>·</span>
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseZip()}
                      >
                        ZIP
                      </button>
                    </>
                  ) : (
                    <>
                      or browse:
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseFolder()}
                      >
                        choose folder
                      </button>
                      <span className='text-border'>·</span>
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChooseZip()}
                      >
                        choose ZIP
                      </button>
                    </>
                  )}
                </div>
              </div>

              {/* Selected path row */}
              {source && (
                <div className='flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-1.5'>
                  <span className='text-xs'>{source.kind === 'zip' ? '🗜️' : '📁'}</span>
                  <span className='flex-1 truncate font-mono text-[10px] text-muted-foreground'>
                    {source.path}
                  </span>
                  <button
                    type='button'
                    className='shrink-0 text-muted-foreground hover:text-foreground transition-colors'
                    onClick={clearSource}
                    aria-label='Clear selection'
                  >
                    ✕
                  </button>
                </div>
              )}

              {error && <p className='text-xs text-destructive'>{error}</p>}
            </div>

            <DialogFooter>
              <Button variant='ghost' onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={() => void handleImport()} disabled={!source}>
                Import
              </Button>
            </DialogFooter>
          </>
        )}

        {dialogState === 'importing' && (
          <>
            <DialogHeader>
              <DialogTitle>Importing...</DialogTitle>
              <DialogDescription>
                Please wait while your collection is being imported.
              </DialogDescription>
            </DialogHeader>
            <div className='flex items-center justify-center py-8'>
              <Loader2 className='h-8 w-8 animate-spin text-muted-foreground' />
            </div>
          </>
        )}

        {dialogState === 'done' && report && (
          <>
            <DialogHeader>
              <DialogTitle>Import complete</DialogTitle>
              <DialogDescription>
                {report.imported} of {report.totalFiles} request
                {report.totalFiles !== 1 ? 's' : ''} imported successfully.
              </DialogDescription>
            </DialogHeader>

            <div className='space-y-3 py-2'>
              {/* Detected type badge */}
              {report.detectedType && (
                <div className='flex items-center gap-2'>
                  <span className='text-xs text-muted-foreground'>Imported as</span>
                  <Badge
                    variant='secondary'
                    className={
                      report.detectedType === 'workspace'
                        ? 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/20'
                        : 'bg-blue-500/15 text-blue-400 border border-blue-500/20'
                    }
                  >
                    {report.detectedType === 'workspace' ? 'Workspace' : 'Collection'}
                  </Badge>
                </div>
              )}

              {report.createdCollections.length > 0 && (
                <div className='space-y-1.5'>
                  <p className='text-xs font-medium text-muted-foreground'>Created collections</p>
                  <div className='flex flex-wrap gap-1.5'>
                    {report.createdCollections.map((name) => (
                      <Badge key={name} variant='secondary'>
                        {name}
                      </Badge>
                    ))}
                  </div>
                </div>
              )}

              {report.skipped.length > 0 && (
                <Collapsible open={skippedOpen} onOpenChange={setSkippedOpen}>
                  <CollapsibleTrigger asChild>
                    <Button
                      variant='ghost'
                      size='sm'
                      className='h-auto p-0 gap-1 text-xs text-muted-foreground hover:text-foreground hover:bg-transparent'
                    >
                      {skippedOpen ? (
                        <ChevronDown className='h-3 w-3' />
                      ) : (
                        <ChevronRight className='h-3 w-3' />
                      )}
                      {report.skipped.length} item{report.skipped.length !== 1 ? 's' : ''} skipped
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <ul className='mt-1.5 space-y-1 max-h-40 overflow-y-auto'>
                      {report.skipped.map((item, i) => (
                        // biome-ignore lint/suspicious/noArrayIndexKey: stable list order after import
                        <li key={i} className='text-xs text-muted-foreground'>
                          <span className='font-mono'>{item.path}</span>
                          {' — '}
                          <span className='text-amber-500'>
                            {item.reason.type}
                            {item.reason.detail ? `: ${item.reason.detail}` : ''}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </CollapsibleContent>
                </Collapsible>
              )}
            </div>

            <DialogFooter>
              <Button onClick={handleClose}>Done</Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
