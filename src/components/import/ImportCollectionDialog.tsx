import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import {
  ChevronDown,
  ChevronRight,
  FileCode,
  FileJson,
  Info,
  Loader2,
  Plus,
  Upload,
} from 'lucide-react';
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
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import {
  createWorkspace as apiCreateWorkspace,
  getAppDataDir,
  type ImportReport,
  importBruno,
  importBrunoZip,
  importOpenApiFile,
  importOpenApiUrl,
  importPostmanCollection,
  importPostmanEnvironment,
  importWsdl,
  type OpenApiGroupBy,
} from '@/lib/tauri-api';
import { switchWorkspaceSafely } from '@/lib/workspace-switch';
import {
  buildReportView,
  describeNoteGroup,
  notesSummary,
  reasonLabel,
  skippedSummary,
} from './importReportView';
import { describeSource, type SourceKind } from './importSources';

interface ImportCollectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceId: string;
  /** When true, a new workspace is created from the imported source. */
  createWorkspace?: boolean;
  onImportComplete?: () => void;
}

type ImportSource = 'bruno' | 'postman' | 'wsdl' | 'openapi';
type OpenApiMode = 'link' | 'file';
type DialogState = 'picking' | 'importing' | 'done';

const IMPORT_DESCRIPTIONS: Record<ImportSource, string> = {
  bruno:
    'Select a Bruno collection folder or ZIP archive. Collection or workspace is detected automatically.',
  postman: 'Select a Postman Collection JSON file (v2.0 or v2.1).',
  wsdl: 'Select a WSDL 1.1 file. You get one request per SOAP operation, with the envelope and SOAP headers filled in.',
  openapi:
    'Import an OpenAPI 3.0, 3.1 or Swagger 2.0 spec (JSON or YAML) from a link or a file. GitHub file links work, and private repositories need a token.',
};

const EMPTY_HEADLINES: Record<ImportSource, string> = {
  bruno: 'Drop a folder or ZIP here',
  postman: 'Choose a Postman Collection JSON file',
  wsdl: 'Choose a WSDL file',
  openapi: 'Choose an OpenAPI spec file',
};

const EMPTY_HINTS: Record<ImportSource, string> = {
  bruno: 'Collection export or extracted directory',
  postman: 'Exported via File → Export in Postman',
  wsdl: 'SOAP 1.1 or 1.2, local XSD imports are followed',
  openapi: 'JSON or YAML',
};

/** Glyph for a selected source. WSDL uses an icon, older kinds keep their emoji. */
function SourceGlyph({ kind }: { kind: SourceKind }) {
  if (kind === 'wsdl-file' || kind === 'openapi-file')
    return <FileCode className='h-4 w-4 text-muted-foreground' />;
  if (kind === 'zip') return <>🗜️</>;
  if (kind === 'postman-json') return <>📄</>;
  return <>📁</>;
}

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
  const [notesOpen, setNotesOpen] = useState(false);
  // Items that were not imported and notes about left-out parts are shown apart.
  const reportView = buildReportView(report ?? { skipped: [] });
  const skippedText = skippedSummary(reportView);
  const notesText = notesSummary(reportView);
  const [openapiMode, setOpenapiMode] = useState<OpenApiMode>('link');
  const [link, setLink] = useState('');
  // Held in state only. It is never shown again and is cleared on close and tab switch.
  const [token, setToken] = useState('');
  const [groupBy, setGroupBy] = useState<OpenApiGroupBy>('tags');
  const [warnings, setWarnings] = useState<string[]>([]);

  const isOpenApiLink = importSource === 'openapi' && openapiMode === 'link';
  const canImport = isOpenApiLink ? link.trim().length > 0 : source !== null;

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
      setOpenapiMode('link');
      setLink('');
      setToken('');
      setGroupBy('tags');
      setWarnings([]);
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
    setOpenapiMode('link');
    setLink('');
    setToken('');
    setGroupBy('tags');
  }

  function switchOpenApiMode(next: OpenApiMode) {
    setOpenapiMode(next);
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

  async function handleChooseWsdl() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'WSDL', extensions: ['wsdl', 'xml'] }],
    });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'wsdl-file', name });
      setError(null);
    }
  }

  async function handleChooseOpenApi() {
    const path = await openFilePicker({
      directory: false,
      multiple: false,
      filters: [{ name: 'OpenAPI', extensions: ['json', 'yaml', 'yml'] }],
    });
    if (typeof path === 'string') {
      const name = path.split('/').pop() ?? path;
      setSource({ path, kind: 'openapi-file', name });
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
    if (!canImport) return;
    setDialogState('importing');
    setError(null);
    try {
      let targetWsId = workspaceId;
      let newWsId: string | null = null;

      if (createWorkspace) {
        const wsName = (source?.name ?? 'import').replace(/\.(zip|wsdl|xml)$/i, '');
        const dataDir = await getAppDataDir();
        const sep = dataDir.includes('\\') ? '\\' : '/';
        const fullPath = dataDir.endsWith(sep) ? dataDir + wsName : dataDir + sep + wsName;
        const ws = await apiCreateWorkspace(wsName, fullPath);
        // Defer the workspace switch until after import — switching early fires
        // workspace-switched which blanks the UI (closeAll) while still importing.
        newWsId = ws.id;
        targetWsId = ws.id;
      }

      let result: ImportReport;
      let importWarnings: string[] = [];
      if (importSource === 'openapi') {
        const imported =
          openapiMode === 'link'
            ? await importOpenApiUrl(link.trim(), groupBy, targetWsId, token.trim() || undefined)
            : await importOpenApiFile(source?.path ?? '', groupBy, targetWsId);
        result = imported.report;
        importWarnings = imported.warnings;
      } else if (!source) {
        return;
      } else if (importSource === 'wsdl') {
        result = await importWsdl(source.path, targetWsId);
      } else if (importSource === 'postman') {
        result = await importPostmanCollection(source.path, targetWsId);
        if (envFilePath && result.createdCollections.length > 0) {
          await importPostmanEnvironment(envFilePath, result.createdCollections[0], targetWsId);
        }
      } else if (source.kind === 'zip') {
        result = await importBrunoZip(source.path, targetWsId, createWorkspace);
      } else {
        result = await importBruno(source.path, targetWsId, createWorkspace);
      }

      // Switch workspace only after import completes so the UI isn't blanked mid-import.
      if (newWsId) await switchWorkspaceSafely(newWsId);

      setReport(result);
      setWarnings(importWarnings);
      setToken('');
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
              <DialogDescription>{IMPORT_DESCRIPTIONS[importSource]}</DialogDescription>
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
                <Button
                  variant={importSource === 'wsdl' ? 'secondary' : 'ghost'}
                  size='sm'
                  className='h-7 px-3 text-xs'
                  onClick={() => switchImportSource('wsdl')}
                >
                  WSDL
                </Button>
                {!createWorkspace && (
                  <Button
                    variant={importSource === 'openapi' ? 'secondary' : 'ghost'}
                    size='sm'
                    className='h-7 px-3 text-xs'
                    onClick={() => switchImportSource('openapi')}
                  >
                    OpenAPI
                  </Button>
                )}
              </div>

              <p className='text-xs font-semibold uppercase tracking-wide text-muted-foreground'>
                Source
              </p>

              {importSource === 'openapi' && (
                <div className='flex w-fit gap-1 rounded-md border border-border p-0.5'>
                  <Button
                    variant={openapiMode === 'link' ? 'secondary' : 'ghost'}
                    size='sm'
                    className='h-7 px-3 text-xs'
                    onClick={() => switchOpenApiMode('link')}
                  >
                    Link
                  </Button>
                  <Button
                    variant={openapiMode === 'file' ? 'secondary' : 'ghost'}
                    size='sm'
                    className='h-7 px-3 text-xs'
                    onClick={() => switchOpenApiMode('file')}
                  >
                    File
                  </Button>
                </div>
              )}

              {isOpenApiLink && (
                <div className='space-y-3'>
                  <Input
                    value={link}
                    onChange={(e) => setLink(e.target.value)}
                    placeholder='https://github.com/owner/repo/blob/main/openapi.yaml'
                    aria-label='Spec link'
                  />
                  <div className='space-y-1'>
                    <Label htmlFor='openapi-token' className='text-xs font-medium'>
                      Access token (optional)
                    </Label>
                    <Input
                      id='openapi-token'
                      type='password'
                      autoComplete='off'
                      value={token}
                      onChange={(e) => setToken(e.target.value)}
                    />
                    <p className='text-[11px] text-muted-foreground'>
                      Only for private GitHub repositories. Kept in your system keychain, never in
                      the collection.
                    </p>
                  </div>
                </div>
              )}

              {/* Drop zone */}
              {!isOpenApiLink && (
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
                        <SourceGlyph kind={source.kind} />
                      </div>
                      <p className='text-sm font-semibold text-foreground'>{source.name}</p>
                      <p className='mt-0.5 text-xs text-muted-foreground'>
                        {describeSource(source.kind)}
                      </p>
                    </>
                  ) : (
                    <>
                      <div className='mb-2 flex h-9 w-9 items-center justify-center rounded-lg border bg-muted mx-auto'>
                        {importSource === 'wsdl' || importSource === 'openapi' ? (
                          <FileCode className='h-4 w-4 text-muted-foreground' />
                        ) : (
                          <Upload className='h-4 w-4 text-muted-foreground' />
                        )}
                      </div>
                      <p className='text-sm font-medium text-muted-foreground'>
                        {EMPTY_HEADLINES[importSource]}
                      </p>
                      <p className='mt-0.5 text-xs text-muted-foreground'>
                        {EMPTY_HINTS[importSource]}
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
                    ) : null}
                    {importSource === 'postman' && (
                      <button
                        type='button'
                        className='underline underline-offset-2 text-primary hover:text-primary/80 transition-colors'
                        onClick={() => void handleChoosePostmanJson()}
                      >
                        {source ? 'change file' : 'choose JSON file'}
                      </button>
                    )}
                    {importSource === 'openapi' && (
                      <Button
                        variant='link'
                        size='sm'
                        className='h-auto p-0 text-xs'
                        onClick={() => void handleChooseOpenApi()}
                      >
                        {source ? 'change spec file' : 'choose spec file'}
                      </Button>
                    )}
                    {importSource === 'wsdl' && (
                      <Button
                        variant='link'
                        size='sm'
                        className='h-auto p-0 text-xs'
                        onClick={() => void handleChooseWsdl()}
                      >
                        {source ? 'change WSDL file' : 'choose WSDL file'}
                      </Button>
                    )}
                  </div>
                </div>
              )}

              {/* Selected path row */}
              {source && (
                <div className='flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-1.5'>
                  <span className='text-xs'>
                    <SourceGlyph kind={source.kind} />
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

              {importSource === 'openapi' && (
                <div className='space-y-1.5'>
                  <Label className='text-xs font-medium text-muted-foreground'>
                    Group folders by
                  </Label>
                  <RadioGroup
                    value={groupBy}
                    onValueChange={(v) => setGroupBy(v as OpenApiGroupBy)}
                    className='flex gap-4'
                    aria-label='Group folders by'
                  >
                    <div className='flex items-center gap-1.5'>
                      <RadioGroupItem value='tags' id='openapi-group-tags' />
                      <Label htmlFor='openapi-group-tags' className='text-xs'>
                        Tag
                      </Label>
                    </div>
                    <div className='flex items-center gap-1.5'>
                      <RadioGroupItem value='paths' id='openapi-group-paths' />
                      <Label htmlFor='openapi-group-paths' className='text-xs'>
                        Path
                      </Label>
                    </div>
                  </RadioGroup>
                </div>
              )}

              {error && <p className='text-xs text-destructive'>{error}</p>}
            </div>

            <DialogFooter>
              <Button variant='ghost' onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={() => void handleImport()} disabled={!canImport}>
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

              {skippedText && (
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
                      {skippedText}
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <ul className='mt-1.5 space-y-1 max-h-40 overflow-y-auto'>
                      {reportView.skipped.map((item, i) => (
                        // biome-ignore lint/suspicious/noArrayIndexKey: stable list order after import
                        <li key={i} className='text-xs text-muted-foreground'>
                          <span className='font-mono'>{item.path}</span>
                          {' — '}
                          <span className='text-amber-500'>
                            {reasonLabel(item.reason)}
                            {item.reason.detail ? `: ${item.reason.detail}` : ''}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </CollapsibleContent>
                </Collapsible>
              )}

              {notesText && (
                <Collapsible open={notesOpen} onOpenChange={setNotesOpen}>
                  <CollapsibleTrigger asChild>
                    <Button
                      variant='ghost'
                      size='sm'
                      className='h-auto p-0 gap-1 text-xs text-muted-foreground hover:text-foreground hover:bg-transparent'
                    >
                      {notesOpen ? (
                        <ChevronDown className='h-3 w-3' />
                      ) : (
                        <ChevronRight className='h-3 w-3' />
                      )}
                      {notesText}
                    </Button>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <TooltipProvider>
                      <ul className='mt-1.5 space-y-1 max-h-40 overflow-y-auto'>
                        {reportView.notes.map((group) => (
                          <li key={group.key} className='text-xs text-muted-foreground'>
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <Button
                                  type='button'
                                  variant='ghost'
                                  size='sm'
                                  className='h-auto w-full items-start justify-start gap-1.5 whitespace-normal p-0 text-left text-xs font-normal text-muted-foreground hover:bg-transparent'
                                  aria-label={`Not loaded: ${describeNoteGroup(group)}`}
                                >
                                  <Info className='mt-0.5 h-3 w-3 shrink-0 text-amber-500' />
                                  <span>
                                    <span className='font-medium'>Not loaded</span>
                                    {' — '}
                                    {describeNoteGroup(group)}
                                  </span>
                                </Button>
                              </TooltipTrigger>
                              <TooltipContent className='max-w-sm break-words'>
                                {group.paths.join(', ')}: {group.details.join('; ')}
                              </TooltipContent>
                            </Tooltip>
                          </li>
                        ))}
                      </ul>
                    </TooltipProvider>
                  </CollapsibleContent>
                </Collapsible>
              )}

              {warnings.length > 0 && (
                <ul className='max-h-40 space-y-1 overflow-y-auto'>
                  {warnings.map((w) => (
                    <li key={w} className='text-xs text-amber-500'>
                      {w}
                    </li>
                  ))}
                </ul>
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
