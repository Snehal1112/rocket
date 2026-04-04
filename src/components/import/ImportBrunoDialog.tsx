import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { ChevronDown, ChevronRight, FolderOpen, Loader2 } from 'lucide-react';
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
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { type ImportReport, importBrunoCollection, importBrunoWorkspace } from '@/lib/tauri-api';

interface ImportBrunoDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceId: string;
  onImportComplete?: () => void;
}

type ImportMode = 'collection' | 'workspace';
type DialogState = 'picking' | 'importing' | 'done';

export function ImportBrunoDialog({
  open,
  onOpenChange,
  workspaceId,
  onImportComplete,
}: ImportBrunoDialogProps) {
  const [mode, setMode] = useState<ImportMode>('collection');
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [dialogState, setDialogState] = useState<DialogState>('picking');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [skippedOpen, setSkippedOpen] = useState(false);

  function handleClose() {
    onOpenChange(false);
    // Reset state after dialog animates out.
    setTimeout(() => {
      setMode('collection');
      setSelectedPath(null);
      setDialogState('picking');
      setReport(null);
      setError(null);
      setSkippedOpen(false);
    }, 200);
  }

  async function handleSelectDirectory() {
    const path = await openFilePicker({ directory: true, multiple: false });
    if (typeof path === 'string') {
      setSelectedPath(path);
    }
  }

  async function handleImport() {
    if (!selectedPath) return;
    setDialogState('importing');
    setError(null);
    try {
      let result: ImportReport;
      if (mode === 'collection') {
        result = await importBrunoCollection(selectedPath, workspaceId);
      } else {
        result = await importBrunoWorkspace(selectedPath, false, workspaceId);
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
      <DialogContent className='sm:max-w-md'>
        {dialogState === 'picking' && (
          <>
            <DialogHeader>
              <DialogTitle>Import from Bruno</DialogTitle>
              <DialogDescription>
                Select a Bruno collection or workspace directory to import.
              </DialogDescription>
            </DialogHeader>

            <div className='space-y-4 py-2'>
              <RadioGroup
                value={mode}
                onValueChange={(v) => setMode(v as ImportMode)}
                className='space-y-2'
              >
                <div className='flex items-center space-x-2'>
                  <RadioGroupItem value='collection' id='mode-collection' />
                  <Label htmlFor='mode-collection' className='cursor-pointer'>
                    Collection — import a single Bruno collection directory
                  </Label>
                </div>
                <div className='flex items-center space-x-2'>
                  <RadioGroupItem value='workspace' id='mode-workspace' />
                  <Label htmlFor='mode-workspace' className='cursor-pointer'>
                    Workspace — import all collections from a Bruno workspace
                  </Label>
                </div>
              </RadioGroup>

              <div className='space-y-1.5'>
                <Button
                  variant='outline'
                  size='sm'
                  className='w-full justify-start text-sm'
                  onClick={() => void handleSelectDirectory()}
                >
                  <FolderOpen className='h-3.5 w-3.5 mr-2 shrink-0' />
                  {selectedPath ? (
                    <span className='truncate text-foreground'>{selectedPath}</span>
                  ) : (
                    <span className='text-muted-foreground'>Select directory...</span>
                  )}
                </Button>
                {error && <p className='text-xs text-destructive'>{error}</p>}
              </div>
            </div>

            <DialogFooter>
              <Button variant='ghost' onClick={handleClose}>
                Cancel
              </Button>
              <Button onClick={() => void handleImport()} disabled={!selectedPath}>
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
