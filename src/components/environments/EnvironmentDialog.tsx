// src/components/environments/EnvironmentDialog.tsx

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { RocketIdle } from '@/components/illustrations';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useSaveButton } from '@/hooks/use-save-button';
import {
  useDeleteEnvironment,
  useEnvironments,
  useGlobalEnvironment,
  useGlobalEnvironmentName,
  useProcessEnvVars,
  useSaveEnvironment,
} from '@/lib/queries/environment-queries';
import type { Environment, Variable } from '@/lib/tauri-api';
import { deleteEnvironment as deleteEnvironmentApi, saveEnvironment } from '@/lib/tauri-api';
import { buildScopedContext } from '@/lib/url-variables';
import { useEnvStore } from '@/stores/env-store';
import { EnvironmentSidebar } from './EnvironmentSidebar';
import { VariableTable } from './VariableTable';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const EMPTY_ENVS: Environment[] = [];

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

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const setActiveEnvId = useEnvStore((s) => s.setActiveEnvId);

  const { data: environments = EMPTY_ENVS } = useEnvironments(activeCollection);
  const saveMutation = useSaveEnvironment(activeCollection);
  const deleteMutation = useDeleteEnvironment(activeCollection);

  const [selectedName, setSelectedName] = useState<string | null>(environments[0]?.name ?? null);
  const [isDirty, setIsDirty] = useState(false);

  // Local in-flight edit state — avoids writing to the store mid-edit.
  const [localEnvs, setLocalEnvs] = useState<Environment[]>(environments);

  // Sync local env list when query refreshes, but only when not mid-edit to
  // avoid overwriting in-flight changes (and avoid the infinite-loop that a
  // bare reference-unstable `environments` array would cause).
  useEffect(() => {
    if (!isDirty) setLocalEnvs(environments);
  }, [environments, isDirty]);

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

  // Reset dirty flag when switching environments.
  // biome-ignore lint/correctness/useExhaustiveDependencies: selectedName is the intentional trigger
  useEffect(() => {
    setIsDirty(false);
  }, [selectedName]);

  const handleAddEnv = useCallback(
    async (name: string) => {
      await saveMutation.mutateAsync({ name, variables: [] });
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

  const { data: globalEnvName = null } = useGlobalEnvironmentName();
  const { data: globalEnv = null } = useGlobalEnvironment(globalEnvName);
  const { data: processEnvVars = {} } = useProcessEnvVars();

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
      envLabel: selectedEnv?.name,
      globalVars,
      processEnvVars,
    });
  }, [selectedEnv, globalEnv, processEnvVars]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-3xl p-0 gap-0'>
        <DialogHeader className='px-5 py-4 border-b border-border/60'>
          <DialogTitle className='text-sm font-semibold'>Manage Environments</DialogTitle>
          <DialogDescription className='sr-only'>
            Create, edit, and manage environment variables for this collection.
          </DialogDescription>
        </DialogHeader>
        <div className='flex min-h-[420px] max-h-[560px]'>
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
          <div className='flex-1 flex flex-col min-w-0'>
            {selectedEnv ? (
              <VariableTable
                variables={selectedEnv.variables}
                onChange={updateVariable}
                onAdd={addVariable}
                onRemove={removeVariable}
                onSave={() => void triggerSave()}
                isDirty={isDirty}
                saveState={saveState}
                variableContext={variableContext}
              />
            ) : (
              <div className='flex-1 flex flex-col items-center justify-center gap-4 text-center px-6 bg-gradient-to-b from-background to-card/60'>
                <RocketIdle className='w-24 h-24 opacity-70' />
                <div className='space-y-1'>
                  <p className='text-sm font-medium text-foreground'>No environment selected</p>
                  <p className='text-xs text-muted-foreground leading-relaxed'>
                    Pick one from the list or create a new environment.
                  </p>
                </div>
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
