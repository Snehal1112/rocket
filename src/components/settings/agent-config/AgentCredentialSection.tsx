import { AlertTriangle, RefreshCw } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';
import { SecretField } from '@/components/ui/secret-field';
import { SegmentedControl } from '@/components/ui/segmented-control';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { StateBlock } from '@/components/ui/state-block';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import { getProviderDescriptor } from '@/lib/secret-providers';
import type { SecretManagerConnection } from '@/lib/tauri-api';
import type { AgentKindValue, CredentialErrors, CredentialFormState } from './form-types';
import { useSecretListing } from './use-secret-listing';

// Re-exported so plan 3 imports the whole credential contract from one module.
export { EMPTY_CREDENTIAL_FORM } from './form-types';
export { SCOPE_DEBOUNCE_MS } from './use-secret-listing';

type CredentialSourceValue = CredentialFormState['credentialSource'];
type Patch = Partial<CredentialFormState>;

const SOURCE_OPTIONS: { value: CredentialSourceValue; label: string }[] = [
  { value: 'vault', label: 'Secret manager' },
  { value: 'env', label: 'Environment' },
  { value: 'keychain', label: 'Keychain' },
];

// Azure ignores the scope, so the connection label stands in for the vault name.
export function vaultNameForSave(
  connection: SecretManagerConnection | undefined,
  typedScope: string,
): string {
  if (connection?.provider === 'azure') return connection.label.trim();
  return typedScope.trim();
}

export function credentialEnvPlaceholder(kind: AgentKindValue): string {
  if (kind === 'gemini') return 'GEMINI_API_KEY';
  if (kind === 'generic') return 'MY_AGENT_API_KEY';
  return 'ANTHROPIC_API_KEY';
}

export function sourceEnvPlaceholder(kind: AgentKindValue): string {
  if (kind === 'gemini') return 'GEMINI_API_KEY';
  if (kind === 'generic') return 'MY_AGENT_API_KEY';
  return 'CLAUDE_CODE_OAUTH_TOKEN';
}

function Warning({ children }: { children: ReactNode }) {
  return (
    <p role='status' className='flex items-start gap-1.5 text-xs text-foreground'>
      <AlertTriangle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-warning' aria-hidden='true' />
      <span>{children}</span>
    </p>
  );
}

interface VaultFieldsProps {
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
}

function VaultFields({ value, onChange, errors }: VaultFieldsProps) {
  const { data: connections = [], isLoading, isError, refetch } = useSecretManagerConnections();
  const connection = connections.find((c) => c.id === value.vaultConnectionId);
  const descriptor = getProviderDescriptor(connection?.provider);
  const isAzure = connection?.provider === 'azure';
  const usable = connection !== undefined && descriptor.selectable;
  const scope = vaultNameForSave(connection, value.vaultName);
  const listing = useSecretListing({
    connectionId: value.vaultConnectionId,
    scope,
    enabled: usable && scope !== '',
  });

  if (isLoading) {
    return <StateBlock tone='loading'>Loading secret manager connections…</StateBlock>;
  }
  if (isError) {
    return (
      <StateBlock tone='error' action={{ label: 'Retry', onClick: () => void refetch() }}>
        Could not load secret manager connections. Check the Secret Managers settings, then retry.
      </StateBlock>
    );
  }
  if (connections.length === 0) {
    return (
      <StateBlock tone='empty'>
        No secret manager connections yet. Add one in the Secret Managers settings, then come back
        to choose a secret.
      </StateBlock>
    );
  }

  const handleConnection = (id: string) => {
    const next = connections.find((c) => c.id === id);
    const patch: Patch = { vaultConnectionId: id, vaultSecretId: '', vaultSecretName: '' };
    if (next?.provider === 'azure') patch.vaultName = next.label;
    else if (isAzure) patch.vaultName = '';
    onChange(patch);
  };

  const chosenMissing =
    value.vaultSecretId !== '' && !listing.secrets.some((s) => s.id === value.vaultSecretId);
  const options = chosenMissing
    ? [
        { id: value.vaultSecretId, name: value.vaultSecretName || value.vaultSecretId },
        ...listing.secrets,
      ]
    : listing.secrets;
  const missingName = value.vaultSecretName || value.vaultSecretId;
  const listActive = usable && scope !== '';
  const deleted = value.vaultConnectionId !== '' && connection === undefined;

  const connectionHelp = isAzure
    ? 'The saved secret manager connection that holds the credential. Azure uses the vault set on the connection, so no vault name is needed.'
    : 'The saved secret manager connection that holds the credential. Add or edit connections in the Secret Managers settings.';

  return (
    <div className='space-y-4'>
      <FormField
        id='ac-vault-connection'
        label='Connection'
        help={connectionHelp}
        error={errors.vaultConnectionId}
      >
        {(a11y) => (
          <Select value={value.vaultConnectionId} onValueChange={handleConnection}>
            <SelectTrigger {...a11y} className='h-8 text-sm'>
              <SelectValue placeholder='Select a connection…' />
            </SelectTrigger>
            <SelectContent>
              {connections.map((c) => {
                const provider = getProviderDescriptor(c.provider);
                const suffix = provider.selectable ? '' : ' - not available yet';
                return (
                  <SelectItem key={c.id} value={c.id} disabled={!provider.selectable}>
                    {`${c.label} (${provider.label})${suffix}`}
                  </SelectItem>
                );
              })}
            </SelectContent>
          </Select>
        )}
      </FormField>
      {deleted && <Warning>The saved connection no longer exists. Pick another one.</Warning>}
      {connection && !descriptor.selectable && (
        <Warning>{`${descriptor.label} is not available yet. Pick another connection.`}</Warning>
      )}
      {usable && !isAzure && (
        <FormField
          id='ac-vault-scope'
          label={descriptor.scopeLabel}
          help={`The ${descriptor.scopeLabel.toLowerCase()} to list secrets from. The list loads once it is filled in, and a wrong value gives an empty list or an error.`}
          error={errors.vaultName}
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.vaultName}
              onChange={(e) => onChange({ vaultName: e.target.value })}
              placeholder={descriptor.scopePlaceholder}
              className='h-8 text-sm'
              autoComplete='off'
              spellCheck={false}
            />
          )}
        </FormField>
      )}
      {listActive && (
        <>
          <FormField
            id='ac-vault-secret'
            label='Secret'
            help='The secret whose value the agent receives. The list loads on its own. Use the refresh button after you create a new secret.'
            error={errors.vaultSecretId}
          >
            {(a11y) => (
              <div className='flex items-center gap-2'>
                <Select
                  value={value.vaultSecretId}
                  onValueChange={(id) => {
                    const picked = options.find((s) => s.id === id);
                    onChange({ vaultSecretId: id, vaultSecretName: picked?.name ?? '' });
                  }}
                >
                  <SelectTrigger {...a11y} className='h-8 flex-1 text-sm'>
                    <SelectValue placeholder='Select a secret…' />
                  </SelectTrigger>
                  <SelectContent>
                    {options.map((s) => (
                      <SelectItem key={s.id} value={s.id}>
                        {s.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant='outline'
                      size='icon'
                      aria-label='Refresh secrets'
                      disabled={listing.status === 'loading'}
                      onClick={listing.refresh}
                    >
                      <RefreshCw
                        className={`h-3.5 w-3.5${listing.status === 'loading' ? ' animate-spin' : ''}`}
                        aria-hidden='true'
                      />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Refresh secrets</TooltipContent>
                </Tooltip>
              </div>
            )}
          </FormField>
          {listing.status === 'loading' && <StateBlock tone='loading'>Loading secrets…</StateBlock>}
          {listing.status === 'error' && (
            <StateBlock tone='error' action={{ label: 'Retry', onClick: listing.refresh }}>
              {`Could not load secrets: ${listing.error ?? 'unknown error'}`}
            </StateBlock>
          )}
          {listing.status === 'ready' && listing.secrets.length === 0 && (
            <StateBlock tone='empty'>
              No secrets found here. Create one in the secret manager, then use the refresh button.
            </StateBlock>
          )}
          {listing.status === 'ready' && chosenMissing && (
            <Warning>
              {`The chosen secret ${missingName} is no longer in this list. Pick another one, or refresh if it was just created.`}
            </Warning>
          )}
        </>
      )}
    </div>
  );
}

interface EnvFieldsProps {
  kind: AgentKindValue;
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
  onTouch: () => void;
}

function EnvFields({ kind, value, onChange, errors, onTouch }: EnvFieldsProps) {
  const source = value.sourceEnvVar.trim();
  const agent = value.credentialEnvVar.trim();
  const shellNote =
    'It must be exported in the shell Rocket was started from. An app started from a desktop icon may not see it.';
  const help =
    source && agent
      ? `Rocket reads ${source} and gives it to the agent as ${agent}. ${shellNote}`
      : `The NAME of the variable in Rocket's own environment that holds the secret. ${shellNote}`;
  return (
    <FormField
      id='ac-source-env-var'
      label='Rocket reads it from'
      help={help}
      error={errors.sourceEnvVar}
    >
      {(a11y) => (
        <Input
          {...a11y}
          value={value.sourceEnvVar}
          onChange={(e) => {
            onTouch();
            onChange({ sourceEnvVar: e.target.value });
          }}
          placeholder={sourceEnvPlaceholder(kind)}
          className='h-8 text-sm'
          autoComplete='off'
          spellCheck={false}
        />
      )}
    </FormField>
  );
}

interface KeychainFieldsProps {
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
  hasSavedCredential: boolean;
}

function KeychainFields({ value, onChange, errors, hasSavedCredential }: KeychainFieldsProps) {
  const keepSaved = hasSavedCredential && !value.removeSavedCredential;
  const help = keepSaved
    ? 'A credential is stored in the OS keychain on this computer. Use Replace to change it or Remove to delete it.'
    : 'Stored in the OS keychain on this computer. It is never written to a file and never shown again.';
  // With a saved credential there is no single input to label, so the field is a group.
  const isGroup = hasSavedCredential || value.removeSavedCredential;
  return (
    <FormField
      id='ac-local-credential'
      label='Keychain credential'
      group={isGroup}
      help={help}
      error={errors.localCredential}
    >
      {(a11y) => (
        <SecretField
          id={a11y.id}
          aria-invalid={a11y['aria-invalid']}
          aria-describedby={a11y['aria-describedby']}
          value={value.localCredential}
          onValueChange={(next) => onChange({ localCredential: next })}
          hasSaved={hasSavedCredential}
          markedForRemoval={value.removeSavedCredential}
          onToggleRemove={() =>
            onChange({ removeSavedCredential: !value.removeSavedCredential, localCredential: '' })
          }
          placeholder='Paste the token'
        />
      )}
    </FormField>
  );
}

interface AgentCredentialSectionProps {
  kind: AgentKindValue;
  value: CredentialFormState;
  onChange: (patch: Patch) => void;
  errors: CredentialErrors;
  hasSavedCredential: boolean;
}

export function AgentCredentialSection({
  kind,
  value,
  onChange,
  errors,
  hasSavedCredential,
}: AgentCredentialSectionProps) {
  // A saved, different value counts as already edited, so it is never overwritten.
  const [sourceTouched, setSourceTouched] = useState(
    () => value.sourceEnvVar.trim() !== '' && value.sourceEnvVar !== value.credentialEnvVar,
  );

  // Until the user edits it, "Rocket reads it from" follows "Agent reads it as".
  useEffect(() => {
    if (value.credentialSource !== 'env' || sourceTouched) return;
    if (value.sourceEnvVar !== value.credentialEnvVar) {
      onChange({ sourceEnvVar: value.credentialEnvVar });
    }
  }, [value.credentialSource, value.credentialEnvVar, value.sourceEnvVar, sourceTouched, onChange]);

  const handleSource = (next: CredentialSourceValue) => {
    onChange({
      credentialSource: next,
      ...(next === 'keychain' ? {} : { localCredential: '', removeSavedCredential: false }),
    });
  };

  return (
    <TooltipProvider>
      <div className='space-y-4'>
        <FormField
          id='ac-source'
          label='Source'
          group
          help="Where Rocket gets the key when an agent session starts. Secret manager reads it from a connected manager, Environment reads a variable from Rocket's own environment, Keychain stores it in your OS keychain."
        >
          {() => (
            <SegmentedControl
              aria-label='Credential source'
              value={value.credentialSource}
              onValueChange={handleSource}
              options={SOURCE_OPTIONS}
            />
          )}
        </FormField>
        {value.credentialSource === 'vault' && (
          <VaultFields value={value} onChange={onChange} errors={errors} />
        )}
        {value.credentialSource === 'env' && (
          <EnvFields
            kind={kind}
            value={value}
            onChange={onChange}
            errors={errors}
            onTouch={() => setSourceTouched(true)}
          />
        )}
        {value.credentialSource === 'keychain' && (
          <KeychainFields
            value={value}
            onChange={onChange}
            errors={errors}
            hasSavedCredential={hasSavedCredential}
          />
        )}
        <FormField
          id='ac-credential-env-var'
          label='Agent reads it as'
          help={
            <>
              <span className='block'>
                The NAME of the environment variable the agent reads its key from, not the key
                itself. Examples: ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN. The secret comes from
                the source above.
              </span>
              <span className='block'>
                If the credential is a short-lived token, such as an OAuth access token, sessions
                may need a restart after it expires. The value is only read when a new agent session
                starts.
              </span>
            </>
          }
          error={errors.credentialEnvVar}
        >
          {(a11y) => (
            <Input
              {...a11y}
              value={value.credentialEnvVar}
              onChange={(e) => onChange({ credentialEnvVar: e.target.value })}
              placeholder={credentialEnvPlaceholder(kind)}
              className='h-8 text-sm'
              autoComplete='off'
              spellCheck={false}
            />
          )}
        </FormField>
      </div>
    </TooltipProvider>
  );
}
