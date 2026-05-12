import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

interface Props {
  open: boolean;
  onConfirm: (name: string, email: string) => void;
  onCancel: () => void;
  initialName?: string;
  initialEmail?: string;
  confirmLabel?: string;
}

export function GitIdentityDialog({
  open,
  onConfirm,
  onCancel,
  initialName = '',
  initialEmail = '',
  confirmLabel = 'Save & Commit',
}: Props) {
  const [name, setName] = useState(initialName);
  const [email, setEmail] = useState(initialEmail);

  // Sync fields each time the dialog opens so pre-populated values are fresh.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally syncs only on open transition — including initialName/Email would reset the form while the user is mid-edit
  useEffect(() => {
    if (open) {
      setName(initialName);
      setEmail(initialEmail);
    }
  }, [open]);

  const isValid = name.trim().length > 0 && email.includes('@');

  const handleConfirm = () => {
    if (!isValid) return;
    onConfirm(name.trim(), email.trim());
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) onCancel();
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className='w-auto min-w-[22rem] max-w-[min(90vw,_36rem)]'>
        <DialogHeader>
          <DialogTitle>Git Author Identity</DialogTitle>
        </DialogHeader>

        <p className='text-sm text-muted-foreground'>
          Git needs your name and email to record commit authorship.
        </p>

        <div className='space-y-3'>
          <div>
            <Label htmlFor='git-identity-name' className='text-sm'>
              Name
            </Label>
            <Input
              id='git-identity-name'
              value={name}
              onChange={(e) => setName(e.target.value)}
              className='h-8 text-sm'
              placeholder='Your Name'
              autoComplete='name'
              autoFocus
            />
          </div>

          <div>
            <Label htmlFor='git-identity-email' className='text-sm'>
              Email
            </Label>
            <Input
              id='git-identity-email'
              type='email'
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className='h-8 text-sm'
              placeholder='you@example.com'
              autoComplete='email'
              onKeyDown={(e) => {
                if (e.key === 'Enter' && isValid) handleConfirm();
              }}
            />
          </div>

          <div className='flex gap-2'>
            <Button onClick={handleConfirm} disabled={!isValid} className='flex-1' size='sm'>
              {confirmLabel}
            </Button>
            <Button onClick={onCancel} variant='outline' className='flex-1' size='sm'>
              Cancel
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
