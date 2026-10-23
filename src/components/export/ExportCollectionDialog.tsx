import { Download, Loader2 } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { base64ToBytes, saveBinaryFile } from '@/lib/save-file';
import {
  type ExportNote,
  type ExportReport,
  exportCollectionBruno,
  exportCollectionPostman,
} from '@/lib/tauri-api';

interface ExportCollectionDialogProps {
  open: boolean;
  collectionName: string;
  onClose: () => void;
}

const ACTIONS: ExportNote['action'][] = ['translated', 'blanked', 'skipped', 'preserved'];

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  return 'Export failed';
}

export function ExportCollectionDialog({
  open,
  collectionName,
  onClose,
}: ExportCollectionDialogProps) {
  const [format, setFormat] = useState('bruno');
  const [includeEnvironments, setIncludeEnvironments] = useState(true);
  const [preserveScripts, setPreserveScripts] = useState(false);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ExportReport | null>(null);

  const handleExport = async () => {
    setBusy(true);
    setReport(null);
    try {
      const isPostman = format === 'postman';
      const result = isPostman
        ? await exportCollectionPostman(collectionName, { preserveScripts, includeEnvironments })
        : await exportCollectionBruno(collectionName, includeEnvironments);
      // A Postman export with environments comes back as a ZIP, otherwise as one JSON file.
      const filter = result.fileName.endsWith('.zip')
        ? { name: 'ZIP archive', extensions: ['zip'] }
        : { name: 'JSON file', extensions: ['json'] };
      const saved = await saveBinaryFile(result.fileName, base64ToBytes(result.dataBase64), [
        filter,
      ]);
      if (saved) setReport(result.report);
    } catch (err) {
      toast.error(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const handleOpenChange = (next: boolean) => {
    if (next) return;
    setReport(null);
    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className='max-w-lg'>
        <DialogHeader>
          <DialogTitle>Export {collectionName}</DialogTitle>
          <DialogDescription>
            Auth fields, secret variables, URL passwords, and the values of headers, query
            parameters, form fields, OAuth2 parameters and variables whose name ends in token,
            secret, password or api key (for example Authorization, X-Api-Key, access_token) are
            blanked. Variable references are kept. Request bodies, scripts and descriptions are
            exported as written and are not scanned for secrets.
          </DialogDescription>
        </DialogHeader>
        <div className='space-y-4'>
          <div className='space-y-1.5'>
            <Label>Format</Label>
            <Select value={format} onValueChange={setFormat}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='bruno'>Bruno collection (ZIP)</SelectItem>
                <SelectItem value='postman'>Postman collection (v2.1)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {format === 'postman' && (
            <div className='flex items-center gap-2'>
              <Checkbox
                id='export-preserve-scripts'
                checked={preserveScripts}
                onCheckedChange={(v) => setPreserveScripts(v === true)}
              />
              <Label htmlFor='export-preserve-scripts'>Preserve scripts</Label>
            </div>
          )}
          <div className='flex items-center gap-2'>
            <Checkbox
              id='export-include-environments'
              checked={includeEnvironments}
              onCheckedChange={(v) => setIncludeEnvironments(v === true)}
            />
            <Label htmlFor='export-include-environments'>Include environments</Label>
          </div>
          {report && (
            <div className='max-h-64 space-y-3 overflow-y-auto text-sm'>
              <p className='text-muted-foreground'>Exported {report.exported} files.</p>
              {ACTIONS.map((action) => {
                const notes = report.items.filter((n) => n.action === action);
                if (notes.length === 0) return null;
                return (
                  <div key={action}>
                    <p className='font-medium capitalize'>
                      {action} ({notes.length})
                    </p>
                    <ul className='mt-1 space-y-1'>
                      {notes.map((n) => (
                        <li key={`${n.path}-${n.feature}-${n.detail ?? ''}`}>
                          <span>{n.feature}</span>
                          <span className='ml-2 text-xs text-muted-foreground'>{n.path}</span>
                          {n.detail && (
                            <span className='block text-xs text-muted-foreground'>{n.detail}</span>
                          )}
                        </li>
                      ))}
                    </ul>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant='outline' onClick={onClose}>
            Close
          </Button>
          <Button onClick={() => void handleExport()} disabled={busy}>
            {busy ? (
              <Loader2 className='h-4 w-4 mr-2 animate-spin' />
            ) : (
              <Download className='h-4 w-4 mr-2' />
            )}
            Export
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
