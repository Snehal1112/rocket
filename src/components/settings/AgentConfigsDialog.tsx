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
import {
  useAgentConfigs,
  useDeleteAgentConfig,
  useSaveAgentConfig,
  useTestAgentConfig,
} from '@/lib/queries/agent-config-queries';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import {
  type AgentConfig,
  type ExternalSecretRef,
  fetchExternalSecretNames,
} from '@/lib/tauri-api';

interface AgentConfigsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const emptyForm = {
  id: '',
  label: '',
  command: '',
  args: '',
  workingDir: '',
  credentialEnvVar: '',
  vaultConnectionId: '',
  vaultName: '',
  vaultSecretId: '',
  vaultSecretName: '',
};

export function AgentConfigsDialog({ open, onOpenChange }: AgentConfigsDialogProps) {
  const { data: configs = [], isLoading, isError, error, refetch } = useAgentConfigs();
  const { data: connections = [] } = useSecretManagerConnections();
  const saveMutation = useSaveAgentConfig();
  const deleteMutation = useDeleteAgentConfig();
  const testMutation = useTestAgentConfig();

  const [editing, setEditing] = useState<(typeof emptyForm & { isNew: boolean }) | null>(null);
  const [vaultSecrets, setVaultSecrets] = useState<ExternalSecretRef[]>([]);
  const [fetchingSecrets, setFetchingSecrets] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setEditing(null);
      setVaultSecrets([]);
      setFetchingSecrets(false);
      setDeletingId(null);
    }
  }, [open]);

  const startAdd = () => setEditing({ ...emptyForm, id: crypto.randomUUID(), isNew: true });
  const startEdit = (c: AgentConfig) =>
    setEditing({
      id: c.id,
      label: c.label,
      command: c.command,
      args: c.args.join(', '),
      workingDir: c.workingDir ?? '',
      credentialEnvVar: c.credentialEnvVar,
      vaultConnectionId: c.vaultConnectionId,
      vaultName: c.vaultName,
      vaultSecretId: c.vaultSecretId,
      vaultSecretName: c.vaultSecretName,
      isNew: false,
    });

  const handleFetchSecrets = async () => {
    if (!editing?.vaultConnectionId || !editing.vaultName.trim()) {
      toast.error('Select a connection and enter a vault name first.');
      return;
    }
    setFetchingSecrets(true);
    try {
      const secrets = await fetchExternalSecretNames(
        editing.vaultConnectionId,
        editing.vaultName.trim(),
      );
      setVaultSecrets(secrets);
      if (secrets.length === 0) {
        toast.error('No secrets found in that vault.');
      }
    } catch (e) {
      toast.error(`Could not fetch secrets: ${String(e)}`);
    } finally {
      setFetchingSecrets(false);
    }
  };

  const handleSave = async () => {
    if (!editing) return;
    if (
      !editing.label.trim() ||
      !editing.command.trim() ||
      !editing.credentialEnvVar.trim() ||
      !editing.vaultConnectionId ||
      !editing.vaultSecretId
    ) {
      toast.error('Label, command, credential env var, vault connection, and secret are required.');
      return;
    }
    const config: AgentConfig = {
      id: editing.id,
      label: editing.label.trim(),
      command: editing.command.trim(),
      args: editing.args
        .split(',')
        .map((a) => a.trim())
        .filter((a) => a.length > 0),
      workingDir: editing.workingDir.trim() || undefined,
      credentialEnvVar: editing.credentialEnvVar.trim(),
      vaultConnectionId: editing.vaultConnectionId,
      vaultName: editing.vaultName.trim(),
      vaultSecretId: editing.vaultSecretId,
      vaultSecretName: editing.vaultSecretName,
    };
    try {
      await saveMutation.mutateAsync(config);
      setEditing(null);
    } catch (e) {
      toast.error(`Could not save agent: ${String(e)}`);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteMutation.mutateAsync(id);
    } catch (e) {
      toast.error(`Could not delete agent: ${String(e)}`);
    } finally {
      setDeletingId(null);
    }
  };

  const handleTest = async (id: string) => {
    try {
      await testMutation.mutateAsync(id);
      toast.success('Agent test succeeded.');
    } catch (e) {
      toast.error(`Agent test failed: ${String(e)}`);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='w-auto min-w-[28rem] max-w-[min(90vw,_48rem)]'>
        <DialogHeader>
          <DialogTitle>AI Agents</DialogTitle>
        </DialogHeader>

        {editing ? (
          connections.length === 0 ? (
            <div className='space-y-3'>
              <p className='text-sm text-muted-foreground'>
                No RocketVault connections configured yet. Add a Secret Manager connection first,
                then come back to configure an agent's credential.
              </p>
              <Button variant='outline' size='sm' onClick={() => setEditing(null)}>
                Cancel
              </Button>
            </div>
          ) : (
            <div className='space-y-3'>
              <div>
                <Label htmlFor='ac-label' className='text-sm'>
                  Label
                </Label>
                <Input
                  id='ac-label'
                  value={editing.label}
                  onChange={(e) => setEditing({ ...editing, label: e.target.value })}
                  className='h-8 text-sm'
                />
              </div>
              <div>
                <Label htmlFor='ac-command' className='text-sm'>
                  Command
                </Label>
                <Input
                  id='ac-command'
                  value={editing.command}
                  onChange={(e) => setEditing({ ...editing, command: e.target.value })}
                  placeholder='claude-agent-acp'
                  className='h-8 text-sm'
                />
              </div>
              <div>
                <Label htmlFor='ac-args' className='text-sm'>
                  Args <span className='text-muted-foreground'>(comma-separated)</span>
                </Label>
                <Input
                  id='ac-args'
                  value={editing.args}
                  onChange={(e) => setEditing({ ...editing, args: e.target.value })}
                  placeholder='--stdio'
                  className='h-8 text-sm'
                />
              </div>
              <div>
                <Label htmlFor='ac-working-dir' className='text-sm'>
                  Working Directory <span className='text-muted-foreground'>(optional)</span>
                </Label>
                <Input
                  id='ac-working-dir'
                  value={editing.workingDir}
                  onChange={(e) => setEditing({ ...editing, workingDir: e.target.value })}
                  className='h-8 text-sm'
                />
              </div>
              <div>
                <Label htmlFor='ac-credential-env-var' className='text-sm'>
                  Credential Env Var
                </Label>
                <Input
                  id='ac-credential-env-var'
                  value={editing.credentialEnvVar}
                  onChange={(e) => setEditing({ ...editing, credentialEnvVar: e.target.value })}
                  placeholder='ANTHROPIC_API_KEY'
                  className='h-8 text-sm'
                />
              </div>
              <div>
                <Label className='text-sm'>Vault Connection</Label>
                <Select
                  value={editing.vaultConnectionId}
                  onValueChange={(value) =>
                    setEditing({
                      ...editing,
                      vaultConnectionId: value,
                      vaultSecretId: '',
                      vaultSecretName: '',
                    })
                  }
                >
                  <SelectTrigger className='h-8 text-sm'>
                    <SelectValue placeholder='Select a connection…' />
                  </SelectTrigger>
                  <SelectContent>
                    {connections.map((c) => (
                      <SelectItem key={c.id} value={c.id}>
                        {c.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className='flex items-end gap-2'>
                <div className='flex-1'>
                  <Label htmlFor='ac-vault-name' className='text-sm'>
                    Vault Name
                  </Label>
                  <Input
                    id='ac-vault-name'
                    value={editing.vaultName}
                    onChange={(e) => setEditing({ ...editing, vaultName: e.target.value })}
                    placeholder='prod-vault'
                    className='h-8 text-sm'
                  />
                </div>
                <Button
                  variant='outline'
                  size='sm'
                  onClick={() => void handleFetchSecrets()}
                  disabled={fetchingSecrets}
                >
                  {fetchingSecrets && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
                  Fetch Secrets
                </Button>
              </div>
              {vaultSecrets.length > 0 && (
                <div>
                  <Label className='text-sm'>Secret</Label>
                  <Select
                    value={editing.vaultSecretId}
                    onValueChange={(value) => {
                      const picked = vaultSecrets.find((s) => s.secretId === value);
                      setEditing({
                        ...editing,
                        vaultSecretId: value,
                        vaultSecretName: picked?.name ?? '',
                      });
                    }}
                  >
                    <SelectTrigger className='h-8 text-sm'>
                      <SelectValue placeholder='Select a secret…' />
                    </SelectTrigger>
                    <SelectContent>
                      {vaultSecrets.map((s) => (
                        <SelectItem key={s.secretId} value={s.secretId}>
                          {s.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
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
          )
        ) : (
          <div className='space-y-3'>
            {isLoading ? (
              <div className='flex items-center justify-center gap-2 py-4 text-sm text-muted-foreground'>
                <Loader2 className='h-4 w-4 animate-spin' />
                Loading agents…
              </div>
            ) : isError ? (
              <div className='flex flex-col items-center justify-center gap-2 py-4 px-4 text-center'>
                <p className='text-sm text-destructive'>Failed to load agents.</p>
                <p className='text-xs text-muted-foreground wrap-break-word max-w-sm'>
                  {String(error)}
                </p>
                <Button variant='outline' size='sm' onClick={() => void refetch()}>
                  Retry
                </Button>
              </div>
            ) : (
              <>
                {configs.length === 0 && (
                  <p className='text-sm text-muted-foreground'>No agents configured.</p>
                )}
                {configs.map((c) => {
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
                        <div className='text-xs text-muted-foreground'>{c.command}</div>
                      </div>
                      <div className='flex items-center gap-1'>
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
                          aria-label='Edit agent'
                          onClick={() => startEdit(c)}
                        >
                          <Pencil className='h-3.5 w-3.5' aria-hidden='true' />
                        </Button>
                        <Button
                          variant='ghost'
                          size='icon'
                          aria-label='Delete agent'
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
                  Add Agent
                </Button>
              </>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
