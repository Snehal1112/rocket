// src/components/workspace/WorkspaceEnvironmentsTab.tsx

import { Check, Eye, EyeOff, Loader2, Plus, Save, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { InlineEnvName } from '@/components/environments/InlineEnvName';
import { RocketIdle } from '@/components/illustrations';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useSaveButton } from '@/hooks/use-save-button';
import type { Variable } from '@/lib/tauri-api';
import { deleteGlobalEnvironment, saveGlobalEnvironment } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

export function WorkspaceEnvironmentsTab() {
  const environments = useEnvStore((s) => s.globalEnvironments);
  const updateEnvironment = useEnvStore((s) => s.updateGlobalEnvironment);
  const deleteEnv = useEnvStore((s) => s.deleteGlobalEnvironment);
  const createEnvironment = useEnvStore((s) => s.createGlobalEnvironment);
  const loadGlobalEnvironments = useEnvStore((s) => s.loadGlobalEnvironments);

  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [editingVars, setEditingVars] = useState<Variable[]>([]);
  const [isAddingEnv, setIsAddingEnv] = useState(false);
  const [newEnvName, setNewEnvName] = useState('');
  const [isDirty, setIsDirty] = useState(false);

  const saveSettings = useCallback(async () => {
    if (!selectedName) return;
    const env = environments.find((e) => e.name === selectedName);
    if (!env) return;
    await updateEnvironment({ ...env, variables: editingVars });
    setIsDirty(false);
  }, [selectedName, environments, editingVars, updateEnvironment]);

  const { state: saveState, trigger: triggerSave } = useSaveButton(
    saveSettings,
    'Failed to save changes',
  );

  // Load workspace-level environments when the tab mounts.
  useEffect(() => {
    void loadGlobalEnvironments();
  }, [loadGlobalEnvironments]);

  // Select first env when list changes (e.g. after collection switch).
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

  // Apply a variable update at index.
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

  // Add an empty variable row.
  const addVar = useCallback(() => {
    if (!selectedName) return;
    const newVar: Variable = { key: '', value: '', enabled: true, secret: false };
    setEditingVars((prev) => [...prev, newVar]);
    setIsDirty(true);
  }, [selectedName]);

  // Remove a variable row by index.
  const removeVar = useCallback(
    (idx: number) => {
      if (!selectedName) return;
      setEditingVars((prev) => prev.filter((_, i) => i !== idx));
      setIsDirty(true);
    },
    [selectedName],
  );

  // Add a new environment by name.
  const handleAddEnv = useCallback(async () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) {
      setIsAddingEnv(false);
      setNewEnvName('');
      return;
    }
    try {
      await createEnvironment(trimmed);
      setSelectedName(trimmed);
    } catch (err) {
      console.error('[WorkspaceEnvironmentsTab] failed to create environment', err);
    }
    setIsAddingEnv(false);
    setNewEnvName('');
  }, [newEnvName, createEnvironment]);

  // Delete the selected environment.
  const handleDeleteEnv = useCallback(async () => {
    if (!selectedName) return;
    try {
      await deleteEnv(selectedName);
      setSelectedName(environments.find((e) => e.name !== selectedName)?.name ?? null);
    } catch (err) {
      console.error('[WorkspaceEnvironmentsTab] failed to delete environment', err);
    }
  }, [selectedName, environments, deleteEnv]);

  // Rename an environment.
  const handleRenameEnv = useCallback(
    async (oldName: string, newName: string) => {
      const env = environments.find((e) => e.name === oldName);
      if (!env) return;
      try {
        await saveGlobalEnvironment({ ...env, name: newName });
        await deleteGlobalEnvironment(oldName);
        const wasActive = useEnvStore.getState().globalEnvName === oldName;
        useEnvStore.setState((s) => ({
          globalEnvironments: s.globalEnvironments.map((e) =>
            e.name === oldName ? { ...e, name: newName } : e,
          ),
        }));
        if (wasActive) {
          await useEnvStore.getState().setGlobalEnv(newName);
        }
        setSelectedName(newName);
      } catch (err) {
        console.error('[WorkspaceEnvironmentsTab] rename failed:', err);
        toast.error('Failed to rename environment');
        throw err;
      }
    },
    [environments],
  );

  return (
    <div className='h-full flex'>
      {/* Left panel: environment list. */}
      <div className='w-52 border-r border-border flex flex-col bg-card/50'>
        <div className='px-3 pt-3 pb-1.5'>
          <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
            Environments
          </p>
        </div>
        <ScrollArea className='flex-1 px-2'>
          <div className='pb-2 space-y-0.5'>
            {environments.map((env) => (
              <InlineEnvName
                key={env.name}
                name={env.name}
                isSelected={selectedName === env.name}
                existingNames={environments.map((e) => e.name)}
                onClick={() => setSelectedName(env.name)}
                onRename={(newName) => handleRenameEnv(env.name, newName)}
              />
            ))}
            {isAddingEnv && (
              <Input
                autoFocus
                className='h-7 text-sm'
                placeholder='Environment name'
                value={newEnvName}
                onChange={(e) => setNewEnvName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleAddEnv();
                  if (e.key === 'Escape') {
                    setIsAddingEnv(false);
                    setNewEnvName('');
                  }
                }}
                onBlur={() => void handleAddEnv()}
              />
            )}
          </div>
        </ScrollArea>
        <div className='p-2 flex gap-1'>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={() => setIsAddingEnv(true)}
            title='Add environment'
          >
            <Plus className='h-3.5 w-3.5' />
          </Button>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7 text-destructive hover:text-destructive'
            onClick={() => void handleDeleteEnv()}
            disabled={!selectedName}
            title='Delete environment'
          >
            <Trash2 className='h-3.5 w-3.5' />
          </Button>
        </div>
      </div>

      {/* Right panel: variable editor. */}
      <div className='flex-1 flex flex-col min-w-0'>
        {selectedName ? (
          <div className='flex-1 flex flex-col min-w-0 overflow-hidden'>
            <div className='p-0 flex flex-col h-full'>
              {/* Column headers */}
              <div className='grid items-center gap-1.5 px-3 pt-3 pb-1.5 shrink-0' style={{ gridTemplateColumns: '20px 1fr 1fr 52px' }}>
                <div />
                <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
                  Key
                </p>
                <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
                  Value
                </p>
                <div />
              </div>
              <ScrollArea className='flex-1'>
                <div className='px-3 pt-2 pb-1 space-y-1'>
                  {editingVars.map((variable, idx) => {
                    return (
                      <div
                        // biome-ignore lint/suspicious/noArrayIndexKey: env variables may share keys; index is the correct identity
                        key={idx}
                        className={cn(
                          'grid items-center gap-1.5 h-8 group',
                          !variable.enabled && 'opacity-50',
                        )}
                        style={{ gridTemplateColumns: '20px 1fr 1fr 52px' }}
                      >
                        {/* Enabled toggle. */}
                        <Button
                          variant='ghost'
                          size='icon'
                          onClick={() => updateVar(idx, { enabled: !variable.enabled })}
                          className={cn(
                            'w-4 h-4 rounded border p-0 shrink-0',
                            variable.enabled
                              ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                              : 'border-border hover:bg-muted',
                          )}
                          title={variable.enabled ? 'Disable variable' : 'Enable variable'}
                        >
                          {variable.enabled && <Check className='h-3 w-3' />}
                        </Button>

                        {/* Key input. */}
                        <Input
                          placeholder='Key'
                          value={variable.key}
                          onChange={(e) => updateVar(idx, { key: e.target.value })}
                          className='text-xs h-7 font-mono'
                        />

                        {/* Value input, masked when secret. */}
                        <Input
                          placeholder='Value'
                          type={variable.secret ? 'password' : 'text'}
                          value={variable.value}
                          onChange={(e) => updateVar(idx, { value: e.target.value })}
                          className='text-xs h-7 font-mono'
                        />

                        {/* Secret toggle + delete row. */}
                        <div className='flex items-center gap-1 justify-end'>
                          <Button
                            variant='ghost'
                            size='icon'
                            className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
                            onClick={() => updateVar(idx, { secret: !variable.secret })}
                            title={variable.secret ? 'Show value' : 'Hide value'}
                          >
                            {variable.secret ? (
                              <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
                            ) : (
                              <Eye className='h-3.5 w-3.5 text-muted-foreground' />
                            )}
                          </Button>
                          <Button
                            variant='ghost'
                            size='icon'
                            className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
                            onClick={() => removeVar(idx)}
                            title='Delete variable'
                          >
                            <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </ScrollArea>

              <div className='px-3 py-2 border-t border-border border-0 shrink-0 flex items-center justify-between'>
                <Button
                  variant='ghost'
                  size='sm'
                  onClick={addVar}
                  className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
                >
                  <Plus className='h-3.5 w-3.5' />
                  Add Variable
                </Button>
                <Button
                  size='sm'
                  onClick={() => void triggerSave()}
                  disabled={!isDirty || saveState !== 'idle'}
                  className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
                >
                  {saveState === 'saving' ? (
                    <Loader2 className='h-3.5 w-3.5 animate-spin' />
                  ) : saveState === 'success' ? (
                    <Check className='h-3.5 w-3.5' />
                  ) : (
                    <Save className='h-3.5 w-3.5' />
                  )}
                  {saveState === 'success' ? 'Saved' : 'Save'}
                </Button>
              </div>
            </div>
          </div>
        ) : (
          <div className='flex-1 flex flex-col items-center justify-center gap-5 text-center px-8 bg-gradient-to-b from-background to-card/60'>
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
