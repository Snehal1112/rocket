import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { saveContractAsOpenApi } from '@/lib/contracts/exportOpenApi';
import type { Contract } from '@/types/contracts';

interface ExportContractDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  contracts: Contract[];
  collectionRoot: string;
}

export function ExportContractDialog({
  open,
  onOpenChange,
  contracts,
  collectionRoot,
}: ExportContractDialogProps) {
  const [selectedId, setSelectedId] = useState<string>(contracts[0]?.id ?? '');
  const [busy, setBusy] = useState(false);

  const handleExport = async () => {
    const contract = contracts.find((c) => c.id === selectedId);
    if (!contract) return;
    setBusy(true);
    try {
      await saveContractAsOpenApi(collectionRoot, contract.id, contract.name);
      onOpenChange(false);
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
          <DialogTitle>Export as OpenAPI</DialogTitle>
          <DialogDescription className='sr-only'>
            Export a contract as an OpenAPI 3.0 YAML file.
          </DialogDescription>
        </DialogHeader>
        <div className='py-2'>
          <Select value={selectedId} onValueChange={setSelectedId}>
            <SelectTrigger>
              <SelectValue placeholder='Select a contract' />
            </SelectTrigger>
            <SelectContent>
              {contracts.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleExport()} disabled={busy || !selectedId}>
            {busy ? 'Exporting…' : 'Export'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
