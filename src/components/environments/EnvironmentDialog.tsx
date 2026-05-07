// src/components/environments/EnvironmentDialog.tsx

import { Check, Eye, EyeOff, Loader2, Plus, Save, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { RocketIdle } from '@/components/illustrations';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useSaveButton } from '@/hooks/use-save-button';
import {
  useDeleteEnvironment,
  useEnvironments,
  useSaveEnvironment,
} from '@/lib/queries/environment-queries';
import type { Environment, Variable } from '@/lib/tauri-api';
import { deleteEnvironment as deleteEnvironmentApi, saveEnvironment } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import { InlineEnvName } from './InlineEnvName';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const EMPTY_ENVS: Environment[] = [];

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const setActiveEnvId = useEnvStore((s) => s.setActiveEnvId);

  const { data: environments = EMPTY_ENVS } = useEnvironments(activeCollection);
  const saveMutation = useSaveEnvironment(activeCollection);
  const deleteMutation = useDeleteEnvironment(activeCollection);

  const [selectedName, setSelectedName] = useState<string | null>(environments[0]?.name ?? null);
  const [isAddingEnv, setIsAddingEnv] = useState(false);
  const [newEnvName, setNewEnvName] = useState('');
  const [isDirty, setIsDirty] = useState(false);

  // Local in-flight edit state — avoids writing to the store mid-edit.
  const [localEnvs, setLocalEnvs] = useState<Environment[]>(environments);

  // Sync local env list when query refreshes, but only when not mid-edit to
  // avoid overwriting in-flight changes (and avoid the infinite-loop that a
  // bare reference-unstable `environments` array would cause).
  useEffect(() => {
    if (!isDirty) setLocalEnvs(environments);
  }, [environments, isDirty]);

  const selectedEnv = localEnvs.find((e) => e.name === selectedName) ?? null;

  const saveSettings = useCallback(async () => {
    if (!selectedEnv || !activeCollection) return;
    await saveMutation.mutateAsync(selectedEnv);
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

  const handleAddEnv = useCallback(async () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) {
      setIsAddingEnv(false);
      return;
    }
    await saveMutation.mutateAsync({ name: trimmed, variables: [] });
    setSelectedName(trimmed);
    setActiveEnvId(trimmed);
    setIsAddingEnv(false);
    setNewEnvName('');
  }, [newEnvName, saveMutation, setActiveEnvId]);

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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-3xl p-0 gap-0'>
        <DialogHeader className='px-5 py-4 border-b border-border/60'>
          <DialogTitle className='text-sm font-semibold'>Manage Environments</DialogTitle>
        </DialogHeader>
        <div className='flex min-h-[420px] max-h-[560px]'>
          {/* Left panel: environment list. */}
          <div className='w-52 border-r border-border/60 flex flex-col bg-card/50'>
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
                      if (e.key === 'Enter') handleAddEnv();
                      if (e.key === 'Escape') {
                        setIsAddingEnv(false);
                        setNewEnvName('');
                      }
                    }}
                    onBlur={() => {
                      setIsAddingEnv(false);
                      setNewEnvName('');
                    }}
                  />
                )}
              </div>
            </ScrollArea>
            <div className='p-2 border-t border-border/60 flex gap-1'>
              <Button
                variant='ghost'
                size='icon'
                className='h-7 w-7'
                onClick={() => setIsAddingEnv(true)}
                title='Add environment'
                aria-label='Add environment'
              >
                <Plus className='h-3.5 w-3.5' />
              </Button>
              <Button
                variant='ghost'
                size='icon'
                className='h-7 w-7 text-destructive hover:text-destructive'
                onClick={handleDeleteEnv}
                disabled={!selectedName}
                title='Delete environment'
                aria-label='Delete environment'
              >
                <Trash2 className='h-3.5 w-3.5' />
              </Button>
            </div>
          </div>

          {/* Right panel: variable editor. */}
          <div className='flex-1 flex flex-col min-w-0'>
            {selectedEnv ? (
              <Card className='flex-1 flex flex-col min-w-0 overflow-hidden'>
                <CardContent className='p-0 flex flex-col h-full'>
                  {/* Column headers */}
                  <div className='flex items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0'>
                    {/* checkbox placeholder */}
                    <div className='w-4 shrink-0' />
                    <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
                      Key
                    </p>
                    <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
                      Value
                    </p>
                    {/* action buttons placeholder */}
                    <div className='w-[52px] shrink-0' />
                  </div>
                  <ScrollArea className='flex-1'>
                    <div className='px-3 pt-2 pb-1 space-y-1'>
                      {selectedEnv.variables.map((variable, idx) => (
                        <div
                          // biome-ignore lint/suspicious/noArrayIndexKey: index is stable here — rows are not reordered
                          key={idx}
                          className={cn(
                            'flex gap-1.5 items-center rounded-sm px-0 py-0.5 group',
                            !variable.enabled && 'opacity-50',
                          )}
                        >
                          <Checkbox
                            checked={variable.enabled}
                            onCheckedChange={(checked) =>
                              updateVariable(idx, { enabled: !!checked })
                            }
                            aria-label={`${variable.enabled ? 'Disable' : 'Enable'} variable`}
                            className='shrink-0'
                          />
                          <Input
                            placeholder='Key'
                            value={variable.key}
                            onChange={(e) => updateVariable(idx, { key: e.target.value })}
                            className='flex-1 h-7 text-xs font-mono'
                            aria-label={`Variable key ${idx + 1}`}
                          />
                          <Input
                            placeholder='Value'
                            type={variable.secret ? 'password' : 'text'}
                            value={variable.value}
                            onChange={(e) => updateVariable(idx, { value: e.target.value })}
                            className='flex-1 h-7 text-xs font-mono'
                            aria-label={`Variable value ${idx + 1}`}
                          />
                          <Button
                            variant='ghost'
                            size='icon'
                            className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
                            onClick={() => updateVariable(idx, { secret: !variable.secret })}
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
                            onClick={() => removeVariable(idx)}
                            aria-label={`Delete variable ${idx + 1}`}
                          >
                            <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
                          </Button>
                        </div>
                      ))}
                    </div>
                  </ScrollArea>
                  <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
                    <Button
                      variant='ghost'
                      size='sm'
                      onClick={addVariable}
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
                </CardContent>
              </Card>
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
