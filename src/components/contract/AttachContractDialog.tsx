import { type ChangeEvent, useState } from 'react';
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
import type { AttachContractInput, ContractScope } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';

interface AttachContractDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  collectionRoot: string;
  /**
   * Pre-fill scope based on where the user invoked the dialog.
   * In this sprint only `collection` scope is surfaced in the UI;
   * the discriminated union is accepted so future work can add
   * folder and request scopes without changing the dialog contract.
   */
  defaultScope: ContractScope;
}

const emptyForm = () => ({
  title: '',
  provider: '',
  consumer: '',
  project: '',
  version: '',
  effectiveDate: new Date().toISOString().slice(0, 10),
  expiryDate: '',
});

type FormState = ReturnType<typeof emptyForm>;
type FormField = keyof FormState;

export function AttachContractDialog({
  open,
  onOpenChange,
  collectionRoot,
  defaultScope,
}: AttachContractDialogProps) {
  const attachContract = useContractStore((s) => s.attachContract);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onField = (field: FormField) => (event: ChangeEvent<HTMLInputElement>) => {
    const value = event.target.value;
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  const reset = () => {
    setForm(emptyForm());
    setError(null);
    setSaving(false);
  };

  const handleOpenChange = (next: boolean) => {
    if (!next) reset();
    onOpenChange(next);
  };

  const handleSubmit = async () => {
    if (
      !form.title ||
      !form.provider ||
      !form.consumer ||
      !form.project ||
      !form.version ||
      !form.effectiveDate
    ) {
      setError('All fields except expiry date are required.');
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const input: AttachContractInput = {
        title: form.title,
        provider: form.provider,
        consumer: form.consumer,
        project: form.project,
        version: form.version,
        effectiveDate: form.effectiveDate,
        expiryDate: form.expiryDate || null,
        documentPath: null,
        scope: defaultScope,
        initialSnapshots: [],
      };
      await attachContract(collectionRoot, input);
      reset();
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Attach contract</DialogTitle>
        </DialogHeader>

        <div className='space-y-3 py-2'>
          {error && <p className='text-sm text-destructive'>{error}</p>}

          <div className='space-y-1'>
            <Label htmlFor='contract-title'>Title</Label>
            <Input
              id='contract-title'
              placeholder='Payments API v2.3'
              value={form.title}
              onChange={onField('title')}
            />
          </div>

          <div className='grid grid-cols-2 gap-3'>
            <div className='space-y-1'>
              <Label htmlFor='contract-provider'>Provider team</Label>
              <Input
                id='contract-provider'
                placeholder='Billing Team'
                value={form.provider}
                onChange={onField('provider')}
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='contract-consumer'>Consumer team</Label>
              <Input
                id='contract-consumer'
                placeholder='Platform Team'
                value={form.consumer}
                onChange={onField('consumer')}
              />
            </div>
          </div>

          <div className='grid grid-cols-2 gap-3'>
            <div className='space-y-1'>
              <Label htmlFor='contract-project'>Project</Label>
              <Input
                id='contract-project'
                placeholder='Checkout Revamp'
                value={form.project}
                onChange={onField('project')}
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='contract-version'>Version</Label>
              <Input
                id='contract-version'
                placeholder='v1.0'
                value={form.version}
                onChange={onField('version')}
              />
            </div>
          </div>

          <div className='grid grid-cols-2 gap-3'>
            <div className='space-y-1'>
              <Label htmlFor='contract-effective'>Effective date</Label>
              <Input
                id='contract-effective'
                type='date'
                value={form.effectiveDate}
                onChange={onField('effectiveDate')}
              />
            </div>
            <div className='space-y-1'>
              <Label htmlFor='contract-expiry'>Expiry date (optional)</Label>
              <Input
                id='contract-expiry'
                type='date'
                value={form.expiryDate}
                onChange={onField('expiryDate')}
              />
            </div>
          </div>

          <div className='text-xs text-muted-foreground pt-1'>
            Scope: <span className='font-medium'>{formatScope(defaultScope)}</span>
          </div>
        </div>

        <DialogFooter>
          <Button variant='outline' onClick={() => handleOpenChange(false)} disabled={saving}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={saving}>
            {saving ? 'Attaching…' : 'Attach contract'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatScope(scope: ContractScope): string {
  if (scope.type === 'collection') return 'Entire collection';
  if (scope.type === 'folder') return `Folder: ${scope.rel_path}`;
  return `Request: ${scope.rel_path}`;
}
