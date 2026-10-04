import { Loader2, Pencil, Plus, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import {
  useDeleteSecretManagerConnection,
  useSaveSecretManagerConnection,
  useSecretManagerConnections,
  useTestSecretManagerConnection,
} from '@/lib/queries/secret-manager-queries';
import {
  connectionFieldLabel,
  connectionFieldPlaceholder,
  getProviderDescriptor,
  requiredFieldsMessage,
  SECRET_PROVIDERS,
} from '@/lib/secret-providers';
import type { ProviderConfig, SecretManagerConnection, SecretProviderKind } from '@/lib/tauri-api';

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
  provider: 'rocketvault' as SecretProviderKind,
  tenantId: '',
  authorityHost: '',
};

export function SecretManagerConnectionsDialog({
  open,
  onOpenChange,
}: SecretManagerConnectionsDialogProps) {
  const {
    data: connections = [],
    isLoading,
    isError,
    error,
    refetch,
  } = useSecretManagerConnections();
  const saveMutation = useSaveSecretManagerConnection();
  const deleteMutation = useDeleteSecretManagerConnection();
  const testMutation = useTestSecretManagerConnection();

  const [editing, setEditing] = useState<(typeof emptyForm & { isNew: boolean }) | null>(null);
  // Keyed by connection id, so each row keeps its own vault name.
  const [testVaultNames, setTestVaultNames] = useState<Record<string, string>>({});
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const descriptor = getProviderDescriptor(editing?.provider);
  const fields = editing ? descriptor.connectionFields : [];

  useEffect(() => {
    if (!open) {
      setEditing(null);
      setTestVaultNames({});
      setDeletingId(null);
    }
  }, [open]);

  const startAdd = () => setEditing({ ...emptyForm, id: crypto.randomUUID(), isNew: true });
  const startEdit = (c: SecretManagerConnection) =>
    setEditing({
      ...emptyForm,
      id: c.id,
      label: c.label,
      baseUrl: c.baseUrl,
      clientId: c.clientId,
      verifySsl: c.verifySsl,
      allowInsecureHttp: c.allowInsecureHttp,
      provider: c.provider ?? 'rocketvault',
      tenantId: c.config?.tenantId ?? '',
      authorityHost: c.config?.authorityHost ?? '',
      isNew: false,
    });

  const handleSave = async () => {
    if (!editing) return;
    const descriptorFields = descriptor.connectionFields;
    const needsBaseUrl = descriptorFields.includes('baseUrl');
    const needsTenantId = descriptorFields.includes('tenantId');
    const needsClientId = descriptorFields.includes('clientId');
    if (
      !editing.label.trim() ||
      (needsBaseUrl && !editing.baseUrl.trim()) ||
      (needsTenantId && !editing.tenantId.trim()) ||
      (needsClientId && !editing.clientId.trim())
    ) {
      toast.error(requiredFieldsMessage(descriptor));
      return;
    }
    if (needsBaseUrl && !/^https?:\/\/[^/\s]+/i.test(editing.baseUrl.trim())) {
      toast.error(
        `${connectionFieldLabel(descriptor, 'baseUrl')} must start with http:// or https://.`,
      );
      return;
    }
    if (
      descriptorFields.includes('clientSecret') &&
      editing.isNew &&
      !editing.clientSecret.trim()
    ) {
      toast.error('A client secret is required when adding a new connection.');
      return;
    }
    const config: ProviderConfig | undefined =
      editing.provider === 'azure'
        ? {
            kind: 'azure',
            tenantId: editing.tenantId.trim(),
            ...(editing.authorityHost ? { authorityHost: editing.authorityHost } : {}),
          }
        : undefined;
    const connection: SecretManagerConnection = {
      id: editing.id,
      label: editing.label.trim(),
      baseUrl: editing.baseUrl.trim(),
      clientId: editing.clientId.trim(),
      verifySsl: editing.verifySsl,
      allowInsecureHttp: editing.allowInsecureHttp,
      provider: editing.provider,
      ...(config ? { config } : {}),
    };
    try {
      await saveMutation.mutateAsync({
        connection,
        clientSecret: editing.clientSecret.trim() ? editing.clientSecret : undefined,
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
    } finally {
      setDeletingId(null);
    }
  };

  const handleTest = async (id: string) => {
    const vaultName = (testVaultNames[id] ?? '').trim();
    if (!vaultName) {
      toast.error('Enter a vault name to test against.');
      return;
    }
    try {
      await testMutation.mutateAsync({ id, vaultName });
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
              <Label htmlFor='sm-provider' className='text-sm'>
                Provider
              </Label>
              <Select
                value={editing.provider}
                onValueChange={(value) =>
                  setEditing({ ...editing, provider: value as SecretProviderKind })
                }
                // A saved connection keeps its provider, so its stored credential stays valid.
                disabled={!editing.isNew}
              >
                <SelectTrigger id='sm-provider' className='h-8 text-sm'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SECRET_PROVIDERS.map((p) => (
                    <SelectItem key={p.kind} value={p.kind} disabled={!p.selectable}>
                      {p.selectable ? p.label : `${p.label} (not available yet)`}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            {fields.includes('baseUrl') && (
              <div>
                <Label htmlFor='sm-base-url' className='text-sm'>
                  {connectionFieldLabel(descriptor, 'baseUrl')}
                </Label>
                <Input
                  id='sm-base-url'
                  value={editing.baseUrl}
                  onChange={(e) => setEditing({ ...editing, baseUrl: e.target.value })}
                  placeholder={
                    connectionFieldPlaceholder(descriptor, 'baseUrl') ??
                    'https://vault.internal:8774'
                  }
                  className='h-8 text-sm'
                />
              </div>
            )}
            {fields.includes('tenantId') && (
              <div>
                <Label htmlFor='sm-tenant-id' className='text-sm'>
                  {connectionFieldLabel(descriptor, 'tenantId')}
                </Label>
                <Input
                  id='sm-tenant-id'
                  value={editing.tenantId}
                  onChange={(e) => setEditing({ ...editing, tenantId: e.target.value })}
                  placeholder='Directory (tenant) ID or domain'
                  className='h-8 text-sm'
                />
              </div>
            )}
            {fields.includes('clientId') && (
              <div>
                <Label htmlFor='sm-client-id' className='text-sm'>
                  {connectionFieldLabel(descriptor, 'clientId')}
                </Label>
                <Input
                  id='sm-client-id'
                  value={editing.clientId}
                  onChange={(e) => setEditing({ ...editing, clientId: e.target.value })}
                  className='h-8 text-sm'
                />
              </div>
            )}
            {fields.includes('clientSecret') && (
              <div>
                <Label htmlFor='sm-client-secret' className='text-sm'>
                  {connectionFieldLabel(descriptor, 'clientSecret')}{' '}
                  {editing.isNew ? (
                    <span>(required)</span>
                  ) : (
                    <span className='text-muted-foreground'>
                      (leave blank to keep the existing secret)
                    </span>
                  )}
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
            )}
            {fields.includes('verifySsl') && (
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
            )}
            {fields.includes('allowInsecureHttp') && (
              <div className='flex items-center justify-between'>
                <Label htmlFor='sm-allow-insecure' className='text-sm'>
                  Allow insecure HTTP{' '}
                  <span className='text-muted-foreground'>
                    (permits non-loopback hosts over plain HTTP — loopback is always allowed)
                  </span>
                </Label>
                <Switch
                  id='sm-allow-insecure'
                  checked={editing.allowInsecureHttp}
                  onCheckedChange={(checked) =>
                    setEditing({ ...editing, allowInsecureHttp: checked })
                  }
                />
              </div>
            )}
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
            {isLoading ? (
              <div className='flex items-center justify-center gap-2 py-4 text-sm text-muted-foreground'>
                <Loader2 className='h-4 w-4 animate-spin' />
                Loading connections…
              </div>
            ) : isError ? (
              <div className='flex flex-col items-center justify-center gap-2 py-4 px-4 text-center'>
                <p className='text-sm text-destructive'>Failed to load connections.</p>
                <p className='text-xs text-muted-foreground wrap-break-word max-w-sm'>
                  {String(error)}
                </p>
                <Button variant='outline' size='sm' onClick={() => void refetch()}>
                  Retry
                </Button>
              </div>
            ) : (
              <>
                {connections.length === 0 && (
                  <p className='text-sm text-muted-foreground'>No connections configured.</p>
                )}
                {connections.map((c) => {
                  if (c.id === deletingId) {
                    return (
                      <div
                        key={c.id}
                        className='flex items-center gap-2 px-2 py-1.5 rounded-md bg-destructive/10 text-sm'
                      >
                        <span className='flex-1'>
                          Remove <span className='font-semibold'>{c.label}</span>?
                        </span>
                        <Button
                          size='sm'
                          variant='destructive'
                          className='h-7 text-xs'
                          disabled={deleteMutation.isPending}
                          onClick={() => void handleDelete(c.id)}
                        >
                          Remove
                        </Button>
                        <Button
                          size='sm'
                          variant='ghost'
                          className='h-7 text-xs'
                          onClick={() => setDeletingId(null)}
                        >
                          Cancel
                        </Button>
                      </div>
                    );
                  }

                  return (
                    <div key={c.id} className='flex items-center justify-between gap-2 text-sm'>
                      <div>
                        <div className='font-medium'>{c.label}</div>
                        <div className='text-xs text-muted-foreground'>{c.baseUrl}</div>
                      </div>
                      <div className='flex items-center gap-1'>
                        <Input
                          value={testVaultNames[c.id] ?? ''}
                          onChange={(e) =>
                            setTestVaultNames((prev) => ({ ...prev, [c.id]: e.target.value }))
                          }
                          placeholder={getProviderDescriptor(c.provider).scopePlaceholder}
                          aria-label={`Vault name to test ${c.label}`}
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
                          onClick={() => setDeletingId(c.id)}
                        >
                          <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
                        </Button>
                      </div>
                    </div>
                  );
                })}
                <Button size='sm' onClick={startAdd}>
                  <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
                  Add Connection
                </Button>
              </>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
