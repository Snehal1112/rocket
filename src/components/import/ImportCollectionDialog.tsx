import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { ChevronDown, ChevronRight, FileJson, Loader2, Plus, Upload } from 'lucide-react';
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
import {
  createWorkspace as apiCreateWorkspace,
  switchWorkspace as apiSwitchWorkspace,
  getAppDataDir,
  type ImportReport,
  importBruno,
  importBrunoZip,
  importPostmanCollection,
  importPostmanEnvironment,
} from '@/lib/tauri-api';

interface ImportCollectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceId: string;
  /** When true, a new workspace is created from the imported source. */
  createWorkspace?: boolean;
  onImportComplete?: () => void;
}

type ImportSource = 'bruno' | 'postman';
type SourceKind = 'folder' | 'zip' | 'postman-json';
type DialogState = 'picking' | 'importing' | 'done';

interface SelectedSource {
  path: string;
  kind: SourceKind;
  name: string;
}

export function ImportCollectionDialog({
  open,
  onOpenChange,
  workspaceId,
  createWorkspace,
  onImportComplete,
}: ImportCollectionDialogProps) {
  const [importSource, setImportSource] = useState<ImportSource>('bruno');
  const [source, setSource] = useState<SelectedSource | null>(null);
  const [envFilePath, setEnvFilePath] = useState<string | null>(null);
  const [dialogState, setDialogState] = useState<DialogState>('picking');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [skippedOpen, setSkippedOpen] = useState(false);

  function handleClose() {
    onOpenChange(false);
    // Reset state after dialog animates out.
    setTimeout(() => {
      setImportSource('bruno');
      setSource(null);
      setEnvFilePath(null);
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

  function switchImportSource(next: ImportSource) {
    setImportSource(next);
    setSource(null);
    setEnvFilePath(null);
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

  async function handleChoosePostmanJson() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'Postman Collection', extensions: ['json'] }],
    });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'postman-json', name });
      setError(null);
    }
  }

  async function handleChooseEnvJson() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'Postman Environment', extensions: ['json'] }],
    });
    if (typeof path === 'string') setEnvFilePath(path);
  }

  async function handleImport() {
    if (!source) return;
    setDialogState('importing');
    setError(null);
    try {
      let targetWsId = workspaceId;

      // When importing as a new workspace, create it first and switch to it
      // so the import writes files into the new workspace directory.
      if (createWorkspace) {
        const wsName = source.name.replace(/\.zip$/i, '');
        const dataDir = await getAppDataDir();
        const sep = dataDir.includes('\\') ? '\\' : '/';
        const fullPath = dataDir.endsWith(sep) ? dataDir + wsName : dataDir + sep + wsName;
        const ws = await apiCreateWorkspace(wsName, fullPath);
        await apiSwitchWorkspace(ws.id);
        targetWsId = ws.id;
      }

      let result: ImportReport;
      if (importSource === 'postman') {
        result = await importPostmanCollection(source.path, targetWsId);
        if (envFilePath && result.createdCollections.length > 0) {
          await importPostmanEnvironment(envFilePath, result.createdCollections[0], targetWsId);
        }
      } else if (source.kind === 'zip') {
        result = await importBrunoZip(source.path, targetWsId, createWorkspace);
      } else {
        result = await importBruno(source.path, targetWsId, createWorkspace);
      }
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
      <DialogContent className='w-auto min-w-[28rem] max-w-[min(90vw,_42rem)]'>
        {dialogState === 'picking' && (
          <>
            <DialogHeader>
              <DialogTitle>Import Collection</DialogTitle>
              <DialogDescription>
                {importSource === 'bruno'
                  ? 'Select a Bruno collection folder or ZIP archive. Collection or workspace is detected automatically.'
                  : 'Select a Postman Collection JSON file (v2.0 or v2.1).'}
              </DialogDescription>
            </DialogHeader>

            <div className='space-y-3 py-2'>
              <div className='flex w-fit gap-1 rounded-md border border-border p-0.5'>
                <Button
                  variant={importSource === 'bruno' ? 'secondary' : 'ghost'}
                  size='sm'
                  className='h-7 px-3 text-xs'
                  onClick={() => switchImportSource('bruno')}
                >
                  Bruno
                </Button>
                <Button
                  variant={importSource === 'postman' ? 'secondary' : 'ghost'}
                  size='sm'
                  className='h-7 px-3 text-xs'
                  onClick={() => switchImportSource('postman')}
                >
                  Postman
                </Button>
              </div>

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
                      {source.kind === 'zip' ? '🗜️' : source.kind === 'postman-json' ? '📄' : '📁'}
                    </div>
                    <p className='text-sm font-semibold text-foreground'>{source.name}</p>
                    <p className='mt-0.5 text-xs text-muted-foreground'>
                      {source.kind === 'zip'
                        ? 'ZIP archive'
                        : source.kind === 'postman-json'
                          ? 'Postman Collection JSON'
                          : 'Folder'}
                    </p>
                  </>
                ) : (
                  <>
                    <div className='mb-2 flex h-9 w-9 items-center justify-center rounded-lg border bg-muted mx-auto'>
                      <Upload className='h-4 w-4 text-muted-foreground' />
                    </div>
                    <p className='text-sm font-medium text-muted-foreground'>
                      {importSource === 'bruno'
                        ? 'Drop a folder or ZIP here'
                        : 'Choose a Postman Collection JSON file'}
                    </p>
                    <p className='mt-0.5 text-xs text-muted-foreground'>
                      {importSource === 'bruno'
                        ? 'Collection export or extracted directory'
                        : 'Exported via File → Export in Postman'}
                    </p>
                  </>
                )}

                <div className='mt-3 flex items-center justify-center gap-1 text-xs text-muted-foreground'>
                  {importSource === 'bruno' ? (
                    source ? (
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
                    )
                  ) : (
                    <button
                      type='button'
                      className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                      onClick={() => void handleChoosePostmanJson()}
                    >
                      {source ? 'change file' : 'choose JSON file'}
                    </button>
                  )}
                </div>
              </div>

              {/* Selected path row */}
              {source && (
                <div className='flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-1.5'>
                  <span className='text-xs'>
                    {source.kind === 'zip' ? '🗜️' : source.kind === 'postman-json' ? '📄' : '📁'}
                  </span>
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

              {importSource === 'postman' && (
                <div className='space-y-1'>
                  <p className='text-[11px] font-medium text-muted-foreground'>
                    Additional Environment JSON{' '}
                    <span className='font-normal opacity-60'>
                      (optional — embedded environments are imported automatically)
                    </span>
                  </p>
                  {envFilePath ? (
                    <div className='flex items-center gap-2 rounded-md border border-border bg-muted/40 px-2 py-1.5'>
                      <FileJson className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
                      <span className='flex-1 truncate font-mono text-[10px] text-muted-foreground'>
                        {envFilePath}
                      </span>
                      <button
                        type='button'
                        className='shrink-0 text-muted-foreground transition-colors hover:text-foreground'
                        onClick={() => setEnvFilePath(null)}
                        aria-label='Clear environment file'
                      >
                        ✕
                      </button>
                    </div>
                  ) : (
                    <Button
                      variant='outline'
                      size='sm'
                      className='h-7 w-full text-xs'
                      onClick={() => void handleChooseEnvJson()}
                    >
                      <Plus className='mr-1 h-3 w-3' />
                      Select environment file
                    </Button>
                  )}
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
