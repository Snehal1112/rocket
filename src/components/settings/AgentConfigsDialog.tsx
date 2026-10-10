import { Loader2, Pencil, Plus, Trash2, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
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

interface EnvRow {
  key: string;
  name: string;
  value: string;
}

const RESERVED_ENV_VAR = 'CLAUDE_CONFIG_DIR';
const ENV_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

// Mirrors the backend rules for fast feedback. The backend stays the authority.
function validateEnvName(name: string): string | null {
  if (name.length === 0) return 'Name is required.';
  if (name.length > 128) return 'Name must be at most 128 characters.';
  if (!ENV_NAME_PATTERN.test(name)) {
    return 'Use letters, digits and underscore only, and do not start with a digit.';
  }
  if (name.toLowerCase() === RESERVED_ENV_VAR.toLowerCase()) {
    return `${RESERVED_ENV_VAR} is reserved.`;
  }
  return null;
}

const SECRET_VALUE_MESSAGE =
  'This looks like a secret value. This field takes only the name of an environment variable, for example ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN. Put the secret itself in the RocketVault secret below. If this is a real key, treat it as exposed and revoke it.';

// Heuristic only. The message never repeats the entered text.
function looksLikeSecret(text: string): boolean {
  return (
    /[-./]/.test(text) ||
    text.length > 64 ||
    ['sk-', 'ghp_', 'xox', 'AKIA', 'eyJ'].some((prefix) => text.startsWith(prefix))
  );
}

// An empty value is not flagged here. The save handler reports it as required.
function validateCredentialEnvVar(value: string): string | null {
  const text = value.trim();
  if (text.length === 0) return null;
  if (looksLikeSecret(text)) return SECRET_VALUE_MESSAGE;
  return validateEnvName(text);
}

function validateEnvRows(rows: EnvRow[], credentialEnvVar: string): (string | null)[] {
  const credential = credentialEnvVar.trim().toLowerCase();
  return rows.map((row, index) => {
    const lower = row.name.toLowerCase();
    const nameError = validateEnvName(row.name);
    if (nameError) return nameError;
    if (credential && lower === credential) {
      return 'Must differ from the credential env var.';
    }
    if (rows.findIndex((r) => r.name.toLowerCase() === lower) !== index) {
      return 'Duplicate variable name.';
    }
    return null;
  });
}

const emptyForm = {
  extraEnv: [] as EnvRow[],
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

// Maps the known backend rule violation to a friendlier text without echoing input.
function saveErrorMessage(e: unknown): string {
  const text = String(e);
  if (text.includes('credential_env_var may contain only')) {
    return 'The credential variable name is not valid. Use only letters, digits and underscores.';
  }
  return `Could not save agent: ${text}`;
}

export function AgentConfigsDialog({ open, onOpenChange }: AgentConfigsDialogProps) {
  const { data: configs = [], isLoading, isError, error, refetch } = useAgentConfigs();
  const {
    data: connections = [],
    isLoading: connectionsLoading,
    isError: connectionsError,
  } = useSecretManagerConnections();
  const saveMutation = useSaveAgentConfig();
  const deleteMutation = useDeleteAgentConfig();
  const testMutation = useTestAgentConfig();

  const [editing, setEditing] = useState<(typeof emptyForm & { isNew: boolean }) | null>(null);
  const [vaultSecrets, setVaultSecrets] = useState<ExternalSecretRef[]>([]);
  const [fetchingSecrets, setFetchingSecrets] = useState(false);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  // Bumped whenever the fetched secret list becomes stale. A fetch that resolves
  // after a bump is dropped, so it cannot leak into another edit session.
  const fetchGeneration = useRef(0);

  const resetVaultSecrets = () => {
    fetchGeneration.current += 1;
    setVaultSecrets([]);
    setFetchingSecrets(false);
  };

  useEffect(() => {
    if (!open) {
      setEditing(null);
      fetchGeneration.current += 1;
      setVaultSecrets([]);
      setFetchingSecrets(false);
      setDeletingId(null);
    }
  }, [open]);

  const startAdd = () => {
    resetVaultSecrets();
    setEditing({ ...emptyForm, id: crypto.randomUUID(), isNew: true });
  };
  const startEdit = (c: AgentConfig) => {
    resetVaultSecrets();
    setEditing({
      id: c.id,
      label: c.label,
      command: c.command,
      args: c.args.join(', '),
      workingDir: c.workingDir ?? '',
      credentialEnvVar: c.credentialEnvVar,
      extraEnv: (c.extraEnv ?? []).map((p) => ({ key: crypto.randomUUID(), ...p })),
      vaultConnectionId: c.vaultConnectionId,
      vaultName: c.vaultName,
      vaultSecretId: c.vaultSecretId,
      vaultSecretName: c.vaultSecretName,
      isNew: false,
    });
  };
  const cancelEdit = () => {
    resetVaultSecrets();
    setEditing(null);
  };

  const handleFetchSecrets = async () => {
    if (!editing?.vaultConnectionId || !editing.vaultName.trim()) {
      toast.error('Select a connection and enter a vault name first.');
      return;
    }
    const generation = ++fetchGeneration.current;
    setFetchingSecrets(true);
    try {
      const secrets = await fetchExternalSecretNames(
        editing.vaultConnectionId,
        editing.vaultName.trim(),
      );
      if (generation !== fetchGeneration.current) return;
      setVaultSecrets(secrets);
      if (secrets.length === 0) {
        toast.error('No secrets found in that vault.');
      }
    } catch (e) {
      if (generation !== fetchGeneration.current) return;
      toast.error(`Could not fetch secrets: ${String(e)}`);
    } finally {
      if (generation === fetchGeneration.current) setFetchingSecrets(false);
    }
  };

  const envErrors = editing ? validateEnvRows(editing.extraEnv, editing.credentialEnvVar) : [];
  const credentialError = editing ? validateCredentialEnvVar(editing.credentialEnvVar) : null;
  const envInvalid = envErrors.some((e) => e !== null) || credentialError !== null;

  const updateEnvRow = (key: string, patch: Partial<Pick<EnvRow, 'name' | 'value'>>) => {
    if (!editing) return;
    setEditing({
      ...editing,
      extraEnv: editing.extraEnv.map((r) => (r.key === key ? { ...r, ...patch } : r)),
    });
  };

  const handleSave = async () => {
    if (!editing || envInvalid) return;
    if (
      !editing.label.trim() ||
      !editing.command.trim() ||
      !editing.credentialEnvVar.trim() ||
      !editing.vaultConnectionId ||
      !editing.vaultName.trim() ||
      !editing.vaultSecretId
    ) {
      toast.error(
        'Label, command, credential env var, vault connection, vault name, and secret are required.',
      );
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
      extraEnv: editing.extraEnv.map(({ name, value }) => ({ name, value })),
      vaultConnectionId: editing.vaultConnectionId,
      vaultName: editing.vaultName.trim(),
      vaultSecretId: editing.vaultSecretId,
      vaultSecretName: editing.vaultSecretName,
    };
    try {
      await saveMutation.mutateAsync(config);
      cancelEdit();
    } catch (e) {
      toast.error(saveErrorMessage(e));
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
          connectionsLoading ? (
            <div className='flex items-center justify-center gap-2 py-4 text-sm text-muted-foreground'>
              <Loader2 className='h-4 w-4 animate-spin' />
              Loading vault connections…
            </div>
          ) : connectionsError || connections.length === 0 ? (
            <div className='space-y-3'>
              <p className='text-sm text-muted-foreground'>
                {connectionsError
                  ? 'Could not load RocketVault connections. Check the Secret Manager connections dialog, then try again.'
                  : "No RocketVault connections configured yet. Add a Secret Manager connection first, then come back to configure an agent's credential."}
              </p>
              <Button variant='outline' size='sm' onClick={cancelEdit}>
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
                  aria-invalid={credentialError !== null}
                  autoComplete='off'
                  spellCheck={false}
                />
                {credentialError && (
                  <p role='alert' className='mt-1 text-xs text-destructive'>
                    {credentialError}
                  </p>
                )}
                <p className='mt-1 text-xs text-muted-foreground'>
                  The NAME of the environment variable the agent reads its key from, for example
                  ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN. The key itself comes from the
                  RocketVault secret below.
                </p>
                <p className='mt-1 text-xs text-muted-foreground'>
                  If the vault secret is a short-lived token (e.g. an OAuth access token) rather
                  than a static API key, sessions may need to be restarted after it expires — the
                  value is only re-fetched when a new agent session starts.
                </p>
              </div>
              <div className='space-y-2'>
                <Label className='text-sm'>Environment variables</Label>
                <p className='text-xs text-muted-foreground'>
                  Values are stored as plain text. Put secrets in the RocketVault credential, not
                  here.
                </p>
                <p className='text-xs text-muted-foreground'>
                  A variable such as ANTHROPIC_BASE_URL can send the credential to another host.
                </p>
                {editing.extraEnv.map((row, index) => (
                  <div key={row.key} className='space-y-1'>
                    <div className='flex items-center gap-2'>
                      <Input
                        aria-label={`Variable name ${index + 1}`}
                        aria-invalid={envErrors[index] !== null}
                        value={row.name}
                        onChange={(e) => updateEnvRow(row.key, { name: e.target.value })}
                        placeholder='NAME'
                        className='h-8 flex-1 text-sm'
                      />
                      <Input
                        aria-label={`Variable value ${index + 1}`}
                        value={row.value}
                        onChange={(e) => updateEnvRow(row.key, { value: e.target.value })}
                        placeholder='value'
                        className='h-8 flex-1 text-sm'
                      />
                      <Button
                        variant='ghost'
                        size='icon'
                        aria-label={`Remove variable ${index + 1}`}
                        onClick={() =>
                          setEditing({
                            ...editing,
                            extraEnv: editing.extraEnv.filter((r) => r.key !== row.key),
                          })
                        }
                      >
                        <X className='h-3.5 w-3.5' aria-hidden='true' />
                      </Button>
                    </div>
                    {envErrors[index] && (
                      <p role='alert' className='text-xs text-destructive'>
                        {envErrors[index]}
                      </p>
                    )}
                  </div>
                ))}
                <Button
                  variant='outline'
                  size='sm'
                  onClick={() =>
                    setEditing({
                      ...editing,
                      extraEnv: [
                        ...editing.extraEnv,
                        { key: crypto.randomUUID(), name: '', value: '' },
                      ],
                    })
                  }
                >
                  <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
                  Add variable
                </Button>
              </div>
              <div>
                <Label className='text-sm'>Vault Connection</Label>
                <Select
                  value={editing.vaultConnectionId}
                  onValueChange={(value) => {
                    // Secrets fetched for the previous connection no longer apply.
                    resetVaultSecrets();
                    setEditing({
                      ...editing,
                      vaultConnectionId: value,
                      vaultSecretId: '',
                      vaultSecretName: '',
                    });
                  }}
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
                    onChange={(e) => {
                      // Secrets fetched for the previous vault no longer apply.
                      resetVaultSecrets();
                      setEditing({
                        ...editing,
                        vaultName: e.target.value,
                        vaultSecretId: '',
                        vaultSecretName: '',
                      });
                    }}
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
              {vaultSecrets.length === 0 && editing.vaultSecretId && (
                <p className='text-xs text-muted-foreground'>
                  Current secret:{' '}
                  <span className='font-medium text-foreground'>
                    {editing.vaultSecretName || editing.vaultSecretId}
                  </span>
                  . Fetch secrets to pick a different one.
                </p>
              )}
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
                <Button variant='outline' size='sm' onClick={cancelEdit}>
                  Cancel
                </Button>
                <Button
                  size='sm'
                  onClick={() => void handleSave()}
                  disabled={saveMutation.isPending || envInvalid}
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
