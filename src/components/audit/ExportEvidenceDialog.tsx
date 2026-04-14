import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { exportAuditEvidence } from '@/lib/tauri-api';

interface ExportEvidenceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function isoStartOfDay(d: string) {
  return `${d}T00:00:00.000Z`;
}
function isoEndOfDay(d: string) {
  return `${d}T23:59:59.999Z`;
}

export function ExportEvidenceDialog({ open, onOpenChange }: ExportEvidenceDialogProps) {
  const today = new Date().toISOString().slice(0, 10);
  const [start, setStart] = useState(today);
  const [end, setEnd] = useState(today);
  const [busy, setBusy] = useState(false);

  const handleExport = async () => {
    setBusy(true);
    try {
      const result = await exportAuditEvidence(isoStartOfDay(start), isoEndOfDay(end));
      const path = await saveDialog({
        defaultPath: `rocket-audit-${start}_${end}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (path) {
        await writeTextFile(path, JSON.stringify(result, null, 2));
        toast.success(`Exported ${result.events.length} events`, {
          description: result.chainVerified ? 'Chain verified' : 'Chain verification failed',
        });
        onOpenChange(false);
      }
    } catch (e) {
      toast.error('Export failed', {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Export Evidence Pack</DialogTitle>
        </DialogHeader>
        <div className='space-y-4 py-2'>
          <div className='space-y-1.5'>
            <Label htmlFor='start-date'>From</Label>
            <Input
              id='start-date'
              type='date'
              value={start}
              onChange={(e) => setStart(e.target.value)}
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='end-date'>To</Label>
            <Input id='end-date' type='date' value={end} onChange={(e) => setEnd(e.target.value)} />
          </div>
          <p className='text-xs text-muted-foreground'>
            Output is a JSON file containing every event in the range plus a hash-chain verification
            flag. Suitable as SOC 2 / ISO 27001 evidence.
          </p>
        </div>
        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleExport()} disabled={busy}>
            {busy ? 'Exporting...' : 'Export'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
