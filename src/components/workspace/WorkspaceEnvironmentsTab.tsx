// src/components/workspace/WorkspaceEnvironmentsTab.tsx

import { AlertTriangle } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { EnvironmentSidebar } from '@/components/environments/EnvironmentSidebar';
import { extendsCandidates } from '@/components/environments/extends-candidates';
import { VariableTable } from '@/components/environments/VariableTable';
import { RocketIdle } from '@/components/illustrations';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useSaveButton } from '@/hooks/use-save-button';
import {
  useDeleteGlobalEnvironment,
  useGlobalEnvironmentName,
  useGlobalEnvironments,
  useProcessEnvVars,
  useResolvedEnvironment,
  useSaveGlobalEnvironment,
  useSetGlobalEnvironment,
} from '@/lib/queries/environment-queries';
import type { Environment, ResolvedVariable, Variable } from '@/lib/tauri-api';
import { deleteGlobalEnvironment, saveGlobalEnvironment } from '@/lib/tauri-api';
import { buildScopedContext, secretKeysOf } from '@/lib/url-variables';

const EMPTY_ENVS: Environment[] = [];
const EMPTY_ROWS: ResolvedVariable[] = [];
// Radix Select forbids an empty item value, so "None" uses a sentinel.
const NO_PARENT = '__none__';

export function WorkspaceEnvironmentsTab() {
  const { data: environments = EMPTY_ENVS } = useGlobalEnvironments();
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const saveMutation = useSaveGlobalEnvironment();
  const deleteMutation = useDeleteGlobalEnvironment();
  const setGlobalEnvMutation = useSetGlobalEnvironment();

  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [editingVars, setEditingVars] = useState<Variable[]>([]);
  const [editingExtends, setEditingExtends] = useState<string | undefined>(undefined);
  const [editingDotEnv, setEditingDotEnv] = useState<string | undefined>(undefined);
  const [isDirty, setIsDirty] = useState(false);

  const saveSettings = useCallback(async () => {
    if (!selectedName) return;
    const env = environments.find((e) => e.name === selectedName);
    if (!env) return;
    await saveMutation.mutateAsync({
      ...env,
      variables: editingVars,
      extends: editingExtends,
      dotEnvFilePath: editingDotEnv,
    });
    setIsDirty(false);
  }, [selectedName, environments, editingVars, editingExtends, editingDotEnv, saveMutation]);

  const { state: saveState, trigger: triggerSave } = useSaveButton(
    saveSettings,
    'Failed to save changes',
  );

  // Select first env when list changes.
  useEffect(() => {
    setSelectedName((prev) => {
      if (prev && environments.find((e) => e.name === prev)) return prev;
      return environments[0]?.name ?? null;
    });
  }, [environments]);

  // Sync editing vars when the selected env or its data changes.
  useEffect(() => {
    const env = environments.find((e) => e.name === selectedName);
    setEditingVars(env ? env.variables.slice() : []);
    setEditingExtends(env?.extends || undefined);
    setEditingDotEnv(env?.dotEnvFilePath || undefined);
    setIsDirty(false);
  }, [selectedName, environments]);

  const updateVar = useCallback(
    (idx: number, patch: Partial<Variable>) => {
      if (!selectedName) return;
      const updated = editingVars.slice();
      updated[idx] = { ...updated[idx], ...patch };
      setEditingVars(updated);
      setIsDirty(true);
    },
    [selectedName, editingVars],
  );

  const addVar = useCallback(() => {
    if (!selectedName) return;
    setEditingVars((prev) => [...prev, { key: '', value: '', enabled: true, secret: false }]);
    setIsDirty(true);
  }, [selectedName]);

  const removeVar = useCallback(
    (idx: number) => {
      if (!selectedName) return;
      setEditingVars((prev) => prev.filter((_, i) => i !== idx));
      setIsDirty(true);
    },
    [selectedName],
  );

  const handleAddEnv = useCallback(
    async (name: string) => {
      try {
        await saveMutation.mutateAsync({ name, variables: [], externalSecrets: [] });
        setSelectedName(name);
      } catch (err) {
        console.error('[WorkspaceEnvironmentsTab] failed to create environment', err);
      }
    },
    [saveMutation],
  );

  const handleDeleteEnv = useCallback(async () => {
    if (!selectedName) return;
    try {
      await deleteMutation.mutateAsync(selectedName);
      if (globalEnvName === selectedName) setGlobalEnvMutation.mutate(null);
      setSelectedName(environments.find((e) => e.name !== selectedName)?.name ?? null);
    } catch (err) {
      console.error('[WorkspaceEnvironmentsTab] failed to delete environment', err);
    }
  }, [selectedName, environments, deleteMutation, globalEnvName, setGlobalEnvMutation]);

  const handleRenameEnv = useCallback(
    async (oldName: string, newName: string) => {
      const env = environments.find((e) => e.name === oldName);
      if (!env) return;
      try {
        await saveGlobalEnvironment({ ...env, name: newName });
        await deleteGlobalEnvironment(oldName);
        // Invalidate via save mutation to trigger refetch.
        await saveMutation.mutateAsync({ ...env, name: newName });
        if (globalEnvName === oldName) setGlobalEnvMutation.mutate(newName);
        setSelectedName(newName);
      } catch (err) {
        console.error('[WorkspaceEnvironmentsTab] rename failed:', err);
        toast.error('Failed to rename environment');
        throw err;
      }
    },
    [environments, saveMutation, globalEnvName, setGlobalEnvMutation],
  );

  const candidates = useMemo(
    () => (selectedName ? extendsCandidates(environments, selectedName) : []),
    [environments, selectedName],
  );
  // A stored parent that no longer exists stays visible so the save keeps it.
  const missingParent =
    editingExtends && !environments.some((e) => e.name === editingExtends) ? editingExtends : null;

  const { data: resolvedEnv } = useResolvedEnvironment(null, selectedName);
  const inheritedRows = useMemo(() => {
    const own = new Set(editingVars.map((v) => v.key));
    return (resolvedEnv?.variables ?? EMPTY_ROWS).filter(
      (v) => v.source !== selectedName && !own.has(v.key),
    );
  }, [resolvedEnv, editingVars, selectedName]);
  const warnings = useMemo(
    () =>
      (resolvedEnv?.warnings ?? []).filter(
        (w) => !missingParent || !w.includes(`"${missingParent}"`),
      ),
    [resolvedEnv, missingParent],
  );

  const { data: processEnvVars = {} } = useProcessEnvVars(null);

  const variableContext = useMemo(() => {
    const envVars: Record<string, string> = {};
    for (const v of editingVars) if (v.enabled) envVars[v.key] = v.value;
    return buildScopedContext({
      envVars,
      envSecretKeys: secretKeysOf(editingVars),
      envLabel: selectedName ?? undefined,
      processEnvVars,
    });
  }, [editingVars, selectedName, processEnvVars]);

  return (
    <div className='h-full flex'>
      <EnvironmentSidebar
        environments={environments}
        selectedName={selectedName}
        activeName={globalEnvName}
        onSelect={setSelectedName}
        onRename={handleRenameEnv}
        onAdd={handleAddEnv}
        onDelete={() => void handleDeleteEnv()}
        canDelete={!!selectedName}
      />

      <div className='flex-1 flex flex-col min-w-0'>
        {selectedName ? (
          <>
            <div className='shrink-0 flex items-center gap-3 border-b px-3 py-2'>
              <Label className='text-xs shrink-0' htmlFor='global-env-extends'>
                Inherits from
              </Label>
              <Select
                value={editingExtends || NO_PARENT}
                onValueChange={(v) => {
                  setEditingExtends(v === NO_PARENT ? undefined : v);
                  setIsDirty(true);
                }}
                disabled={candidates.length === 0 && !missingParent}
              >
                <SelectTrigger
                  id='global-env-extends'
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
              <Label className='text-xs shrink-0' htmlFor='global-env-dotenv-path'>
                .env file
              </Label>
              <Input
                id='global-env-dotenv-path'
                aria-label='.env file path'
                className='h-7 min-w-0 flex-1 text-xs font-mono'
                placeholder='.env'
                value={editingDotEnv ?? ''}
                onChange={(e) => {
                  setEditingDotEnv(e.target.value || undefined);
                  setIsDirty(true);
                }}
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
            <VariableTable
              variables={editingVars}
              onChange={updateVar}
              onAdd={addVar}
              onRemove={removeVar}
              onSave={() => void triggerSave()}
              isDirty={isDirty}
              saveState={saveState}
              variableContext={variableContext}
              inheritedRows={inheritedRows}
            />
          </>
        ) : (
          <div className='flex-1 flex flex-col items-center justify-center gap-5 text-center px-8 bg-linear-to-b from-background to-card/60'>
            <RocketIdle className='w-36 h-36 opacity-70' />
            <div className='space-y-1.5'>
              <p className='text-sm font-medium text-foreground'>No environment selected</p>
              <p className='text-xs text-muted-foreground leading-relaxed'>
                Choose an environment from the list, or create one to start managing variables.
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
