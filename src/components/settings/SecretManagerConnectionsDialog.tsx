import { Loader2, Pencil, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  useDeleteSecretManagerConnection,
  useSaveSecretManagerConnection,
  useSecretManagerConnections,
  useTestSecretManagerConnection,
} from '@/lib/queries/secret-manager-queries';
import type { SecretManagerConnection } from '@/lib/tauri-api';

interface SecretManagerConnectionsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const emptyForm = {
  id: '',
  label: '',
  baseUrl: '',
  clientId: '',
  verifySsl: true,
  allowInsecureHttp: false,
  clientSecret: '',
};

export function SecretManagerConnectionsDialog({
  open,
  onOpenChange,
}: SecretManagerConnectionsDialogProps) {
  const { data: connections = [] } = useSecretManagerConnections();
  const saveMutation = useSaveSecretManagerConnection();
  const deleteMutation = useDeleteSecretManagerConnection();
  const testMutation = useTestSecretManagerConnection();

  const [editing, setEditing] = useState<typeof emptyForm | null>(null);
  const [testVaultName, setTestVaultName] = useState('');

  const startAdd = () => setEditing({ ...emptyForm, id: crypto.randomUUID() });
  const startEdit = (c: SecretManagerConnection) => setEditing({ ...c, clientSecret: '' });

  const handleSave = async () => {
    if (!editing) return;
    const connection: SecretManagerConnection = {
      id: editing.id,
      label: editing.label,
      baseUrl: editing.baseUrl,
      clientId: editing.clientId,
      verifySsl: editing.verifySsl,
      allowInsecureHttp: editing.allowInsecureHttp,
    };
    try {
      await saveMutation.mutateAsync({
        connection,
        clientSecret: editing.clientSecret || undefined,
      });
      setEditing(null);
    } catch (e) {
      toast.error(`Could not save connection: ${String(e)}`);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteMutation.mutateAsync(id);
    } catch (e) {
      toast.error(`Could not delete connection: ${String(e)}`);
    }
  };

  const handleTest = async (id: string) => {
    if (!testVaultName) {
      toast.error('Enter a vault name to test against.');
      return;
    }
    try {
      await testMutation.mutateAsync({ id, vaultName: testVaultName });
      toast.success('Connection succeeded.');
    } catch (e) {
      toast.error(`Connection failed: ${String(e)}`);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='w-auto min-w-[28rem] max-w-[min(90vw,_48rem)]'>
        <DialogHeader>
          <DialogTitle>Secret Manager Connections</DialogTitle>
        </DialogHeader>

        {editing ? (
          <div className='space-y-3'>
            <div>
              <Label htmlFor='sm-label' className='text-sm'>
                Label
              </Label>
              <Input
                id='sm-label'
                value={editing.label}
                onChange={(e) => setEditing({ ...editing, label: e.target.value })}
                className='h-8 text-sm'
              />
            </div>
            <div>
              <Label htmlFor='sm-base-url' className='text-sm'>
                Base URL
              </Label>
              <Input
                id='sm-base-url'
                value={editing.baseUrl}
                onChange={(e) => setEditing({ ...editing, baseUrl: e.target.value })}
                placeholder='https://vault.internal:8774'
                className='h-8 text-sm'
              />
            </div>
            <div>
              <Label htmlFor='sm-client-id' className='text-sm'>
                Client ID
              </Label>
              <Input
                id='sm-client-id'
                value={editing.clientId}
                onChange={(e) => setEditing({ ...editing, clientId: e.target.value })}
                className='h-8 text-sm'
              />
            </div>
            <div>
              <Label htmlFor='sm-client-secret' className='text-sm'>
                Client Secret{' '}
                <span className='text-muted-foreground'>
                  (leave blank to keep the existing secret)
                </span>
              </Label>
              <Input
                id='sm-client-secret'
                type='password'
                value={editing.clientSecret}
                onChange={(e) => setEditing({ ...editing, clientSecret: e.target.value })}
                className='h-8 text-sm'
                autoComplete='new-password'
              />
            </div>
            <div className='flex items-center justify-between'>
              <Label htmlFor='sm-verify-ssl' className='text-sm'>
                Verify SSL
              </Label>
              <Switch
                id='sm-verify-ssl'
                checked={editing.verifySsl}
                onCheckedChange={(checked) => setEditing({ ...editing, verifySsl: checked })}
              />
            </div>
            <div className='flex items-center justify-between'>
              <Label htmlFor='sm-allow-insecure' className='text-sm'>
                Allow insecure HTTP{' '}
                <span className='text-muted-foreground'>(loopback only recommended)</span>
              </Label>
              <Switch
                id='sm-allow-insecure'
                checked={editing.allowInsecureHttp}
                onCheckedChange={(checked) =>
                  setEditing({ ...editing, allowInsecureHttp: checked })
                }
              />
            </div>
            <div className='flex gap-2'>
              <Button variant='outline' size='sm' onClick={() => setEditing(null)}>
                Cancel
              </Button>
              <Button
                size='sm'
                onClick={() => void handleSave()}
                disabled={saveMutation.isPending}
                aria-busy={saveMutation.isPending}
              >
                {saveMutation.isPending && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
                Save
              </Button>
            </div>
          </div>
        ) : (
          <div className='space-y-3'>
            {connections.length === 0 && (
              <p className='text-sm text-muted-foreground'>No connections configured.</p>
            )}
            {connections.map((c) => (
              <div key={c.id} className='flex items-center justify-between gap-2 text-sm'>
                <div>
                  <div className='font-medium'>{c.label}</div>
                  <div className='text-xs text-muted-foreground'>{c.baseUrl}</div>
                </div>
                <div className='flex items-center gap-1'>
                  <Input
                    value={testVaultName}
                    onChange={(e) => setTestVaultName(e.target.value)}
                    placeholder='vault name'
                    className='h-7 w-28 text-xs'
                  />
                  <Button
                    variant='outline'
                    size='sm'
                    onClick={() => void handleTest(c.id)}
                    disabled={testMutation.isPending}
                  >
                    Test
                  </Button>
                  <Button
                    variant='ghost'
                    size='icon'
                    aria-label='Edit connection'
                    onClick={() => startEdit(c)}
                  >
                    <Pencil className='h-3.5 w-3.5' aria-hidden='true' />
                  </Button>
                  <Button
                    variant='ghost'
                    size='icon'
                    aria-label='Delete connection'
                    onClick={() => void handleDelete(c.id)}
                  >
                    <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
                  </Button>
                </div>
              </div>
            ))}
            <Button size='sm' onClick={startAdd}>
              <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
              Add Connection
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
