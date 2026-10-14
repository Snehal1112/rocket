import { Loader2, Pencil, Plus, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { AgentConfigForm } from '@/components/settings/agent-config/AgentConfigForm';
import { vaultNameForSave } from '@/components/settings/agent-config/AgentCredentialSection';
import {
  type AgentFormState,
  emptyAgentForm,
  formatMissingRequired,
  formFromConfig,
  hasAgentFormErrors,
  isAgentFormComplete,
  missingRequiredFields,
  validateAgentForm,
} from '@/components/settings/agent-config/agent-config-form-state';
import { isBlankRow } from '@/components/settings/agent-config/agent-config-validation';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { StateBlock } from '@/components/ui/state-block';
import {
  useAgentConfigs,
  useDeleteAgentConfig,
  useSaveAgentConfig,
  useTestAgentConfig,
} from '@/lib/queries/agent-config-queries';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import {
  type AgentConfig,
  clearAgentCredential,
  hasAgentCredential,
  setAgentCredential,
} from '@/lib/tauri-api';

interface AgentConfigsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const NEW_AGENT_KEY = 'new-agent';
const FIELD_ERRORS_HELP = 'Fix the highlighted fields.';

// Maps the known backend rule violation to a friendlier text without echoing input.
function saveErrorMessage(e: unknown): string {
  const text = String(e);
  if (text.includes('credential_env_var may contain only')) {
    return 'The credential variable name is not valid. Use only letters, digits and underscores.';
  }
  return `Could not save agent: ${text}`;
}

// Maps the form to the persisted shape. The shape itself is unchanged.
function toAgentConfig(form: AgentFormState, vaultName: string): AgentConfig {
  const source = form.credentialSource;
  const idleText = form.idleTimeoutSecs.trim();
  const idleTimeoutSecs = idleText === '' ? undefined : Number(idleText);
  const reasoning = form.reasoning === 'high' ? undefined : form.reasoning;
  const model = form.kind === 'gemini' && form.model.trim() !== '' ? form.model.trim() : undefined;
  const tuning =
    idleTimeoutSecs === undefined && reasoning === undefined && model === undefined
      ? undefined
      : { idleTimeoutSecs, reasoning, model };
  return {
    id: form.id,
    label: form.label.trim(),
    kind: form.kind === 'claude' ? undefined : form.kind,
    tuning,
    command: form.command.trim(),
    args: form.args.map((row) => row.value.trim()).filter((arg) => arg.length > 0),
    // Hidden in the form, so the stored value goes back untouched.
    workingDir: form.workingDir === '' ? undefined : form.workingDir,
    credentialEnvVar: form.credentialEnvVar.trim(),
    // Blank rows (empty name and empty value) are dropped.
    extraEnv: form.extraEnv
      .filter((row) => !isBlankRow(row))
      .map(({ name, value }) => ({ name, value })),
    credentialSource: source,
    sourceEnvVar: source === 'env' ? form.sourceEnvVar.trim() : undefined,
    vaultConnectionId: source === 'vault' ? form.vaultConnectionId : '',
    vaultName: source === 'vault' ? vaultName.trim() : '',
    vaultSecretId: source === 'vault' ? form.vaultSecretId : '',
    vaultSecretName: source === 'vault' ? form.vaultSecretName : '',
  };
}

export function AgentConfigsDialog({ open, onOpenChange }: AgentConfigsDialogProps) {
  const { data: configs = [], isLoading, isError, error, refetch } = useAgentConfigs();
  const { data: connections = [] } = useSecretManagerConnections();
  const saveMutation = useSaveAgentConfig();
  const deleteMutation = useDeleteAgentConfig();
  const testMutation = useTestAgentConfig();

  const [editing, setEditing] = useState<AgentFormState | null>(null);
  // Set when a form opens. It stays put if a new agent turns into a stored one after a partial save.
  const [formKey, setFormKey] = useState('');
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [hasSavedCredential, setHasSavedCredential] = useState(false);
  // Id of the agent whose saved-credential check is current. A late answer for another id is dropped.
  const credentialCheckId = useRef<string | null>(null);

  useEffect(() => {
    if (!open) {
      setEditing(null);
      setDeletingId(null);
      credentialCheckId.current = null;
      setHasSavedCredential(false);
    }
  }, [open]);

  const startAdd = () => {
    credentialCheckId.current = null;
    setHasSavedCredential(false);
    setFormKey(NEW_AGENT_KEY);
    setEditing(emptyAgentForm(crypto.randomUUID()));
  };

  const startEdit = (c: AgentConfig) => {
    credentialCheckId.current = c.id;
    setHasSavedCredential(false);
    if (c.credentialSource === 'keychain') {
      hasAgentCredential(c.id).then(
        (saved) => {
          if (credentialCheckId.current === c.id) setHasSavedCredential(saved);
        },
        () => {
          if (credentialCheckId.current === c.id) setHasSavedCredential(false);
        },
      );
    }
    setFormKey(c.id);
    setEditing(formFromConfig(c));
  };

  const cancelEdit = () => {
    credentialCheckId.current = null;
    setHasSavedCredential(false);
    setEditing(null);
  };

  // Without the connection (still loading, or gone) the stored name is kept as it is.
  const resolveVaultName = (form: AgentFormState) =>
    vaultNameForSave(
      connections.find((c) => c.id === form.vaultConnectionId),
      form.vaultName,
    );

  const resolvedVaultName = editing ? resolveVaultName(editing) : '';
  const missing = editing ? missingRequiredFields(editing, resolvedVaultName) : [];
  const hasFieldErrors = editing ? hasAgentFormErrors(validateAgentForm(editing)) : false;
  const canSave =
    editing !== null && isAgentFormComplete(editing, resolvedVaultName) && !hasFieldErrors;
  // Shown only while Save is disabled. Missing fields come first, then field errors.
  const saveHelp = formatMissingRequired(missing) || (hasFieldErrors ? FIELD_ERRORS_HELP : '');

  const handleSave = async () => {
    if (!editing || !canSave) return;
    const source = editing.credentialSource;
    const config = toAgentConfig(editing, resolveVaultName(editing));
    try {
      await saveMutation.mutateAsync(config);
      if (source === 'keychain') {
        const typed = editing.localCredential.trim();
        try {
          if (typed) await setAgentCredential(config.id, typed);
          else if (editing.removeSavedCredential) await clearAgentCredential(config.id);
        } catch (e) {
          // The agent is saved. Stay in the form so the user can try again.
          setEditing((current) =>
            current ? { ...current, isNew: false, localCredential: '' } : current,
          );
          toast.error(`The agent was saved, but its credential was not: ${String(e)}`);
          return;
        }
      }
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

  const renderList = () => {
    if (isLoading) return <StateBlock tone='loading'>Loading agents…</StateBlock>;
    if (isError) {
      return (
        <StateBlock tone='error' action={{ label: 'Retry', onClick: () => void refetch() }}>
          {`Failed to load agents. ${String(error)}`}
        </StateBlock>
      );
    }
    return (
      <div className='space-y-3'>
        {configs.length === 0 && <StateBlock tone='empty'>No agents configured.</StateBlock>}
        {configs.map((c) => {
          if (c.id === deletingId) {
            return (
              <div
                key={c.id}
                className='flex items-center gap-2 rounded-md bg-destructive/10 px-2 py-1.5 text-sm'
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
              <div className='min-w-0'>
                <div className='font-medium'>{c.label}</div>
                <div className='truncate text-xs text-muted-foreground'>{c.command}</div>
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
      </div>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='flex max-h-[85vh] flex-col'>
        <DialogHeader>
          <DialogTitle>AI Agents</DialogTitle>
          <DialogDescription>
            {editing
              ? 'Set how Rocket starts this agent and where its credential comes from.'
              : 'The agents Rocket can start for the workspace assistant.'}
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          {editing ? (
            <AgentConfigForm
              key={formKey}
              value={editing}
              onChange={setEditing}
              hasSavedCredential={hasSavedCredential}
            />
          ) : (
            renderList()
          )}
        </DialogBody>
        {editing && (
          <DialogFooter>
            {saveHelp && <p className='mr-auto text-xs text-muted-foreground'>{saveHelp}</p>}
            <Button variant='outline' size='sm' onClick={cancelEdit}>
              Cancel
            </Button>
            <Button
              size='sm'
              onClick={() => void handleSave()}
              disabled={saveMutation.isPending || !canSave}
              aria-busy={saveMutation.isPending}
            >
              {saveMutation.isPending && <Loader2 className='h-3.5 w-3.5 animate-spin' />}
              Save
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
