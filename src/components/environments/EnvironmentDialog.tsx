// src/components/environments/EnvironmentDialog.tsx

import { AlertTriangle } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useSaveButton } from '@/hooks/use-save-button';
import { validateClientCertificates } from '@/lib/certificate-validation';
import { validateExternalSecretBindings } from '@/lib/external-secrets';
import {
  useDeleteEnvironment,
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
  useResolvedEnvironment,
  useSaveEnvironment,
} from '@/lib/queries/environment-queries';
import { useSecretManagerConnections } from '@/lib/queries/secret-manager-queries';
import { canAddVaultCertificate } from '@/lib/secret-providers';
import type {
  ClientCertificate,
  Environment,
  ExternalSecretBinding,
  ResolvedVariable,
  Variable,
} from '@/lib/tauri-api';
import { deleteEnvironment as deleteEnvironmentApi, saveEnvironment } from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';
import { CertificatesTab } from './CertificatesTab';
import { EnvironmentSidebar } from './EnvironmentSidebar';
import { ExternalSecretsTab } from './ExternalSecretsTab';
import { extendsCandidates } from './extends-candidates';
import { VariableTable } from './VariableTable';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const EMPTY_ENVS: Environment[] = [];
const EMPTY_ROWS: ResolvedVariable[] = [];
// Radix Select forbids an empty item value, so "None" uses a sentinel.
const NO_PARENT = '__none__';

// Collapses variables sharing a key, keeping the last-edited row's value and
// its original position. Backstop for Save: even if a duplicate ever makes
// it into local state, the file on disk never gets two rows for one key.
function dedupeVariables(variables: Variable[]): Variable[] {
  const indexByKey = new Map<string, number>();
  const result: Variable[] = [];
  for (const variable of variables) {
    if (variable.key === '') {
      result.push(variable);
      continue;
    }
    const existingIdx = indexByKey.get(variable.key);
    if (existingIdx !== undefined) {
      result[existingIdx] = variable;
    } else {
      indexByKey.set(variable.key, result.length);
      result.push(variable);
    }
  }
  return result;
}

// `externalSecrets` is absent on the wire for any environment with no
// bindings (the backend skips serializing an empty vec), so `Environment`
// declares it optional. Narrow it to always-present once, here, so every
// downstream reader in this file (and `ExternalSecretsTab`'s required
// `bindings` prop) can rely on a real array both at runtime and in the type
// system.
type NormalizedEnvironment = Environment & {
  externalSecrets: ExternalSecretBinding[];
  clientCertificates: ClientCertificate[];
};

function normalizeEnv(env: Environment): NormalizedEnvironment {
  return {
    ...env,
    externalSecrets: env.externalSecrets ?? [],
    clientCertificates: env.clientCertificates ?? [],
  };
}

type DialogTab = 'variables' | 'external-secrets' | 'certificates';

// A new, empty certificate of the given type. A vault entry writes its PEM default out.
function emptyCertificate(type: ClientCertificate['type']): ClientCertificate {
  switch (type) {
    case 'pem':
      return { type: 'pem', domain: '', certificateFilePath: '', privateKeyFilePath: '' };
    case 'pkcs12':
      return { type: 'pkcs12', domain: '', pkcs12FilePath: '' };
    case 'vault':
      return { type: 'vault', domain: '', binding: '', certificate: '', format: 'pem' };
  }
}

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const setActiveEnvId = useEnvStore((s) => s.setActiveEnvId);

  const { data: rawEnvironments = EMPTY_ENVS, isPending: environmentsPending } =
    useEnvironments(activeCollection);
  const environments = useMemo(() => rawEnvironments.map(normalizeEnv), [rawEnvironments]);
  const saveMutation = useSaveEnvironment(activeCollection);
  const deleteMutation = useDeleteEnvironment(activeCollection);

  const [selectedName, setSelectedName] = useState<string | null>(environments[0]?.name ?? null);
  const [isDirty, setIsDirty] = useState(false);
  const [activeDialogTab, setActiveDialogTab] = useState<DialogTab>('variables');

  // Local in-flight edit state — avoids writing to the store mid-edit.
  const [localEnvs, setLocalEnvs] = useState<NormalizedEnvironment[]>(environments);

  // Sync local env list when query refreshes, but only when not mid-edit to
  // avoid overwriting in-flight changes (and avoid the infinite-loop that a
  // bare reference-unstable `environments` array would cause).
  useEffect(() => {
    if (!isDirty) setLocalEnvs(environments);
  }, [environments, isDirty]);

  // `selectedName`'s initializer reads `environments[0]?.name`, but the
  // environments query is still loading on first mount (no cached data yet),
  // so that initial read is always empty. Once the list arrives, select the
  // first environment if nothing is selected yet, so opening the dialog
  // doesn't strand the user on the empty state when environments exist.
  useEffect(() => {
    if (selectedName === null && environments.length > 0) {
      setSelectedName(environments[0].name);
    }
  }, [selectedName, environments]);

  // The dialog stays mounted between opens (only `open` toggles visibility),
  // so a stale `isDirty`/`localEnvs` from an abandoned edit could otherwise
  // survive indefinitely and mask writes made elsewhere (e.g. a script's
  // rok.setEnvVar) while the dialog was closed. Force a fresh copy of the
  // live data every time it's (re)opened.
  const wasOpen = useRef(open);
  useEffect(() => {
    if (open && !wasOpen.current) {
      setLocalEnvs(environments);
      setIsDirty(false);
    }
    wasOpen.current = open;
  }, [open, environments]);

  const selectedEnv = localEnvs.find((e) => e.name === selectedName) ?? null;

  const saveSettings = useCallback(async () => {
    if (!selectedEnv || !activeCollection) return;
    // Backstop: collapse same-keyed rows before persisting, so a duplicate
    // that slipped into local state can never be written to disk.
    await saveMutation.mutateAsync({
      ...selectedEnv,
      variables: dedupeVariables(selectedEnv.variables),
    });
    setIsDirty(false);
  }, [selectedEnv, activeCollection, saveMutation]);

  const { state: saveState, trigger: triggerSave } = useSaveButton(
    saveSettings,
    'Failed to save changes',
  );

  // All tabs share one save, so an unfinished binding or certificate blocks a Variables save too.
  // Point the user at the tab that holds the problem instead of failing with a generic toast.
  const handleSave = useCallback(() => {
    if (!selectedEnv) return;
    const error = validateExternalSecretBindings(selectedEnv.externalSecrets);
    if (error) {
      toast.error(error);
      setActiveDialogTab('external-secrets');
      return;
    }
    // Certificate references are checked against the bindings above, which are now known valid.
    const issues = validateClientCertificates(
      selectedEnv.clientCertificates,
      selectedEnv.externalSecrets,
    );
    if (issues.errors.length > 0) {
      toast.error(issues.errors[0]);
      setActiveDialogTab('certificates');
      return;
    }
    // Warnings (a literal passphrase) never block the save.
    if (issues.warnings.length > 0) toast.warning(issues.warnings[0]);
    void triggerSave();
  }, [selectedEnv, triggerSave]);

  // Reset dirty flag when switching environments.
  // biome-ignore lint/correctness/useExhaustiveDependencies: selectedName is the intentional trigger
  useEffect(() => {
    setIsDirty(false);
    setActiveDialogTab('variables');
  }, [selectedName]);

  const handleAddEnv = useCallback(
    async (name: string) => {
      await saveMutation.mutateAsync({ name, variables: [], externalSecrets: [] });
      setSelectedName(name);
      setActiveEnvId(name);
    },
    [saveMutation, setActiveEnvId],
  );

  const handleDeleteEnv = useCallback(async () => {
    if (!selectedName) return;
    await deleteMutation.mutateAsync(selectedName);
    if (activeEnvId === selectedName) setActiveEnvId(null);
    setSelectedName(environments.find((e) => e.name !== selectedName)?.name ?? null);
  }, [selectedName, deleteMutation, environments, activeEnvId, setActiveEnvId]);

  const handleRenameEnv = useCallback(
    async (oldName: string, newName: string) => {
      const env = localEnvs.find((e) => e.name === oldName);
      if (!env || !activeCollection) return;
      try {
        await saveEnvironment(activeCollection, { ...env, name: newName });
        await deleteEnvironmentApi(activeCollection, oldName);
        // Invalidate to refetch fresh list.
        await saveMutation.mutateAsync({ ...env, name: newName });
        if (activeEnvId === oldName) setActiveEnvId(newName);
        setSelectedName(newName);
      } catch (err) {
        console.error('[EnvironmentDialog] rename failed:', err);
        toast.error('Failed to rename environment');
        throw err;
      }
    },
    [localEnvs, activeCollection, saveMutation, activeEnvId, setActiveEnvId],
  );

  const updateEnvField = useCallback(
    (patch: Partial<Pick<Environment, 'extends' | 'dotEnvFilePath'>>) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => (e.name === selectedEnv.name ? { ...e, ...patch } : e)),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const candidates = useMemo(
    () => (selectedEnv ? extendsCandidates(environments, selectedEnv.name) : []),
    [environments, selectedEnv],
  );
  // A stored parent that no longer exists stays visible so the save keeps it.
  const missingParent =
    selectedEnv?.extends && !environments.some((e) => e.name === selectedEnv.extends)
      ? selectedEnv.extends
      : null;

  // Inherited rows come from the saved, flattened environment. Rows the child
  // defines (saved or in-flight) are its own, so they are not listed here.
  const { data: resolvedEnv } = useResolvedEnvironment(activeCollection, selectedName);
  const inheritedRows = useMemo(() => {
    if (!selectedEnv) return EMPTY_ROWS;
    const own = new Set(selectedEnv.variables.map((v) => v.key));
    return (resolvedEnv?.variables ?? EMPTY_ROWS).filter(
      (v) => v.source !== selectedEnv.name && !own.has(v.key),
    );
  }, [resolvedEnv, selectedEnv]);
  const warnings = useMemo(
    () =>
      (resolvedEnv?.warnings ?? []).filter(
        (w) => !missingParent || !w.includes(`"${missingParent}"`),
      ),
    [resolvedEnv, missingParent],
  );

  const updateVariable = useCallback(
    (idx: number, patch: Partial<Variable>) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          const variables = e.variables.slice();
          variables[idx] = { ...variables[idx], ...patch };
          return { ...e, variables };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const addVariable = useCallback(() => {
    if (!selectedEnv) return;
    setLocalEnvs((prev) =>
      prev.map((e) => {
        if (e.name !== selectedEnv.name) return e;
        return {
          ...e,
          variables: [...e.variables, { key: '', value: '', enabled: true, secret: false }],
        };
      }),
    );
    setIsDirty(true);
  }, [selectedEnv]);

  const removeVariable = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          return { ...e, variables: e.variables.filter((_, i) => i !== idx) };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const updateExternalSecret = useCallback(
    (idx: number, patch: Partial<ExternalSecretBinding>) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          const externalSecrets = e.externalSecrets.slice();
          externalSecrets[idx] = { ...externalSecrets[idx], ...patch };
          return { ...e, externalSecrets };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const addExternalSecret = useCallback(() => {
    if (!selectedEnv) return;
    setLocalEnvs((prev) =>
      prev.map((e) => {
        if (e.name !== selectedEnv.name) return e;
        return {
          ...e,
          externalSecrets: [
            ...e.externalSecrets,
            { alias: '', connectionId: '', vaultName: '', secretNames: [] },
          ],
        };
      }),
    );
    setIsDirty(true);
  }, [selectedEnv]);

  const removeExternalSecret = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          return { ...e, externalSecrets: e.externalSecrets.filter((_, i) => i !== idx) };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const updateClientCertificate = useCallback(
    (idx: number, patch: Partial<ClientCertificate>) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          const clientCertificates = e.clientCertificates.slice();
          clientCertificates[idx] = { ...clientCertificates[idx], ...patch } as ClientCertificate;
          return { ...e, clientCertificates };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const addClientCertificate = useCallback(
    (type: ClientCertificate['type']) => {
      if (!selectedEnv) return;
      const fresh = emptyCertificate(type);
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          return { ...e, clientCertificates: [...e.clientCertificates, fresh] };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const removeClientCertificate = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          return { ...e, clientCertificates: e.clientCertificates.filter((_, i) => i !== idx) };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  // The first matching certificate wins, so the order is part of the data.
  const moveClientCertificate = useCallback(
    (idx: number, direction: -1 | 1) => {
      if (!selectedEnv) return;
      setLocalEnvs((prev) =>
        prev.map((e) => {
          if (e.name !== selectedEnv.name) return e;
          const target = idx + direction;
          if (target < 0 || target >= e.clientCertificates.length) return e;
          const clientCertificates = e.clientCertificates.slice();
          [clientCertificates[idx], clientCertificates[target]] = [
            clientCertificates[target],
            clientCertificates[idx],
          ];
          return { ...e, clientCertificates };
        }),
      );
      setIsDirty(true);
    },
    [selectedEnv],
  );

  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars(activeCollection);
  const { data: secretConnections = [], isSuccess: secretConnectionsLoaded } =
    useSecretManagerConnections();

  const variableContext = useMemo(() => {
    const envVars: Record<string, string> = {};
    if (selectedEnv) {
      for (const v of selectedEnv.variables) if (v.enabled) envVars[v.key] = v.value;
    }
    const globalVars: Record<string, string> = globalEnv
      ? Object.fromEntries(
          globalEnv.variables.filter((v) => v.enabled).map((v) => [v.key, v.value]),
        )
      : {};
    return buildScopedContext({
      envVars,
      envSecretKeys: secretKeysOf(selectedEnv?.variables),
      envLabel: selectedEnv?.name,
      globalVars,
      globalSecretKeys: secretKeysOf(globalEnv?.variables),
      processEnvVars,
    });
  }, [selectedEnv, globalEnv, processEnvVars]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-3xl p-0 gap-0 grid-cols-[minmax(0,1fr)]'>
        <DialogHeader className='px-5 py-4 border-b border-border/60'>
          <DialogTitle className='text-sm font-semibold'>Manage Environments</DialogTitle>
          <DialogDescription className='sr-only'>
            Create, edit, and manage environment variables for this collection.
          </DialogDescription>
        </DialogHeader>
        <div className='flex min-h-[420px] max-h-[560px] min-w-0'>
          <EnvironmentSidebar
            environments={environments}
            selectedName={selectedName}
            activeName={activeEnvId}
            onSelect={setSelectedName}
            onRename={handleRenameEnv}
            onAdd={handleAddEnv}
            onDelete={handleDeleteEnv}
            canDelete={!!selectedName}
          />

          {/* Right panel: variable editor. */}
          <div className='flex-1 flex flex-col min-w-0 min-h-0'>
            {selectedEnv ? (
              <Tabs
                value={activeDialogTab}
                onValueChange={(v) => setActiveDialogTab(v as DialogTab)}
                className='flex-1 flex flex-col min-h-0 min-w-0'
              >
                <div className='shrink-0 flex items-center gap-3 border-b px-3 py-2'>
                  <Label className='text-xs shrink-0' htmlFor='env-extends'>
                    Inherits from
                  </Label>
                  <Select
                    value={selectedEnv.extends || NO_PARENT}
                    onValueChange={(v) =>
                      updateEnvField({ extends: v === NO_PARENT ? undefined : v })
                    }
                    disabled={candidates.length === 0 && !missingParent}
                  >
                    <SelectTrigger
                      id='env-extends'
                      className='h-7 w-[150px] shrink-0 text-xs'
                      aria-label='Inherits from'
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={NO_PARENT} className='text-xs'>
                        None
                      </SelectItem>
                      {missingParent && (
                        <SelectItem value={missingParent} className='text-xs'>
                          {missingParent}
                        </SelectItem>
                      )}
                      {candidates.map((name) => (
                        <SelectItem key={name} value={name} className='text-xs'>
                          {name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <Label className='text-xs shrink-0' htmlFor='env-dotenv-path'>
                    .env file
                  </Label>
                  <Input
                    id='env-dotenv-path'
                    aria-label='.env file path'
                    className='h-7 min-w-0 flex-1 text-xs font-mono'
                    placeholder='.env'
                    value={selectedEnv.dotEnvFilePath ?? ''}
                    onChange={(e) =>
                      updateEnvField({ dotEnvFilePath: e.target.value || undefined })
                    }
                  />
                </div>
                {missingParent && (
                  <p className='shrink-0 flex items-center gap-1.5 px-3 py-1.5 text-xs text-amber-500 border-b'>
                    <AlertTriangle className='h-3.5 w-3.5 shrink-0' />
                    {`Parent "${missingParent}" was not found. Its variables are not inherited.`}
                  </p>
                )}
                {warnings.map((warning) => (
                  <p
                    key={warning}
                    className='shrink-0 flex items-center gap-1.5 px-3 py-1.5 text-xs text-amber-500 border-b'
                  >
                    <AlertTriangle className='h-3.5 w-3.5 shrink-0' />
                    {warning}
                  </p>
                ))}
                <TabsList className='shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2'>
                  <TabsTrigger value='variables' className='text-xs'>
                    Variables
                  </TabsTrigger>
                  <TabsTrigger value='external-secrets' className='text-xs'>
                    External Secrets
                  </TabsTrigger>
                  <TabsTrigger value='certificates' className='text-xs'>
                    Certificates
                  </TabsTrigger>
                </TabsList>
                <TabsContent value='variables' className='flex-1 flex flex-col min-h-0 min-w-0 m-0'>
                  <VariableTable
                    variables={selectedEnv.variables}
                    onChange={updateVariable}
                    onAdd={addVariable}
                    onRemove={removeVariable}
                    onSave={handleSave}
                    isDirty={isDirty}
                    saveState={saveState}
                    variableContext={variableContext}
                    inheritedRows={inheritedRows}
                  />
                </TabsContent>
                <TabsContent
                  value='external-secrets'
                  className='flex-1 flex flex-col min-h-0 min-w-0 m-0'
                >
                  <ExternalSecretsTab
                    bindings={selectedEnv.externalSecrets}
                    onChange={updateExternalSecret}
                    onAdd={addExternalSecret}
                    onRemove={removeExternalSecret}
                    onSave={handleSave}
                    isDirty={isDirty}
                    saveState={saveState}
                  />
                </TabsContent>
                <TabsContent
                  value='certificates'
                  className='flex-1 flex flex-col min-h-0 min-w-0 m-0'
                >
                  <CertificatesTab
                    certificates={selectedEnv.clientCertificates}
                    bindings={selectedEnv.externalSecrets}
                    onChange={updateClientCertificate}
                    onAdd={addClientCertificate}
                    onRemove={removeClientCertificate}
                    onMove={moveClientCertificate}
                    onSave={handleSave}
                    isDirty={isDirty}
                    saveState={saveState}
                    variableContext={variableContext}
                    canAddVaultCertificate={canAddVaultCertificate(
                      selectedEnv.externalSecrets,
                      secretConnections,
                      secretConnectionsLoaded,
                    )}
                  />
                </TabsContent>
              </Tabs>
            ) : (
              <div className='flex flex-1 flex-col bg-gradient-to-b from-background to-card/60'>
                {!activeCollection ? (
                  <EmptyState
                    className='flex-1'
                    cause='needs-setup'
                    title='No collection selected'
                    reason='Open a collection to manage its environments.'
                  />
                ) : environmentsPending ? (
                  <EmptyState className='flex-1' cause='none-yet' title='Loading environments' />
                ) : environments.length === 0 ? (
                  <EmptyState
                    className='flex-1'
                    cause='none-yet'
                    title='No environments yet'
                    reason='Use the + button in the list to create one.'
                  />
                ) : (
                  <EmptyState
                    className='flex-1'
                    cause='none-yet'
                    title='No environment selected'
                    reason='Choose one from the list to edit its variables.'
                  />
                )}
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
