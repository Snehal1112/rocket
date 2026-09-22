// src/components/workspace/WorkspaceEnvironmentsTab.tsx

import { useCallback, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { EnvironmentSidebar } from '@/components/environments/EnvironmentSidebar';
import { VariableTable } from '@/components/environments/VariableTable';
import { RocketIdle } from '@/components/illustrations';
import { useSaveButton } from '@/hooks/use-save-button';
import {
  useDeleteGlobalEnvironment,
  useGlobalEnvironmentName,
  useGlobalEnvironments,
  useProcessEnvVars,
  useSaveGlobalEnvironment,
  useSetGlobalEnvironment,
} from '@/lib/queries/environment-queries';
import type { Variable } from '@/lib/tauri-api';
import { deleteGlobalEnvironment, saveGlobalEnvironment } from '@/lib/tauri-api';
import { buildScopedContext } from '@/lib/url-variables';

export function WorkspaceEnvironmentsTab() {
  const { data: environments = [] } = useGlobalEnvironments();
  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const saveMutation = useSaveGlobalEnvironment();
  const deleteMutation = useDeleteGlobalEnvironment();
  const setGlobalEnvMutation = useSetGlobalEnvironment();

  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [editingVars, setEditingVars] = useState<Variable[]>([]);
  const [isDirty, setIsDirty] = useState(false);

  const saveSettings = useCallback(async () => {
    if (!selectedName) return;
    const env = environments.find((e) => e.name === selectedName);
    if (!env) return;
    await saveMutation.mutateAsync({ ...env, variables: editingVars });
    setIsDirty(false);
  }, [selectedName, environments, editingVars, saveMutation]);

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

  const { data: processEnvVars = {} } = useProcessEnvVars();

  const variableContext = useMemo(() => {
    const envVars: Record<string, string> = {};
    for (const v of editingVars) if (v.enabled) envVars[v.key] = v.value;
    return buildScopedContext({
      envVars,
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
          <VariableTable
            variables={editingVars}
            onChange={updateVar}
            onAdd={addVar}
            onRemove={removeVar}
            onSave={() => void triggerSave()}
            isDirty={isDirty}
            saveState={saveState}
            variableContext={variableContext}
          />
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
