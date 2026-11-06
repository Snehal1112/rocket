import { type KeyboardEvent, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { DEFAULT_NETWORK_PORT, validateAddress } from '@/components/network/network-helpers';
import { Button } from '@/components/ui/button';
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
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { useNetworkStore } from '@/stores/network-store';

interface AddAddressDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// Adds a node by host and port, for a VPN or a Tailscale name. Pairing still needs the code.
export function AddAddressDialog({ open, onOpenChange }: AddAddressDialogProps) {
  const [host, setHost] = useState('');
  const [port, setPort] = useState(DEFAULT_NETWORK_PORT);
  // The form shows a failure under the fields, not as a toast.
  const { busy, error, setError, run, reset } = useAsyncAction('inline');

  // Every opening starts with a clean form.
  useEffect(() => {
    if (!open) return;
    setHost('');
    setPort(DEFAULT_NETWORK_PORT);
    reset();
  }, [open, reset]);

  async function submit() {
    if (busy) return;
    const check = validateAddress(host, port);
    if (!check.ok) {
      setError(check.error);
      return;
    }
    const added = await run(() => useNetworkStore.getState().addAddress(check.host, check.port));
    if (!added) return;
    toast.success(`Added ${check.host}:${check.port}.`);
    onOpenChange(false);
  }

  const submitOnEnter = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    void submit();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Add by address</DialogTitle>
          <DialogDescription>
            Enter the address of a Rocket node on a VPN or another network. You still pair with a
            code.
          </DialogDescription>
        </DialogHeader>
        <div className='grid grid-cols-[1fr_6rem] gap-3'>
          <div className='space-y-1.5'>
            <Label htmlFor='network-address-host'>Host</Label>
            <Input
              id='network-address-host'
              value={host}
              placeholder='10.0.0.5 or mac.tailnet.ts.net'
              onChange={(e) => setHost(e.target.value)}
              onKeyDown={submitOnEnter}
            />
          </div>
          <div className='space-y-1.5'>
            <Label htmlFor='network-address-port'>Port</Label>
            <Input
              id='network-address-port'
              inputMode='numeric'
              value={port}
              onChange={(e) => setPort(e.target.value)}
              onKeyDown={submitOnEnter}
            />
          </div>
        </div>
        {error && (
          <p role='alert' className='text-sm text-destructive'>
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant='outline' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={busy} onClick={() => void submit()}>
            Add
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
