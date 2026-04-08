// src/components/environments/EnvironmentDialog.tsx

import { Eye, EyeOff, Plus, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { SavedPill } from '@/components/ui/saved-pill';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { Environment, Variable } from '@/lib/tauri-api';
import { saveEnvironment } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  const environments = useEnvStore((s) => s.environments);
  const createEnvironment = useEnvStore((s) => s.createEnvironment);
  const deleteEnvironment = useEnvStore((s) => s.deleteEnvironment);

  const [selectedName, setSelectedName] = useState<string | null>(environments[0]?.name ?? null);
  const [isAddingEnv, setIsAddingEnv] = useState(false);
  const [newEnvName, setNewEnvName] = useState('');

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  // Clear the save pill when switching environments.
  // biome-ignore lint/correctness/useExhaustiveDependencies: selectedName is the intentional trigger, not used inside the body.
  useEffect(() => {
    setSavedAt(null);
  }, [selectedName]);

  const selectedEnv = environments.find((e) => e.name === selectedName) ?? null;

  const handleAddEnv = useCallback(async () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) {
      setIsAddingEnv(false);
      return;
    }
    await createEnvironment(trimmed);
    setSelectedName(trimmed);
    setIsAddingEnv(false);
    setNewEnvName('');
  }, [newEnvName, createEnvironment]);

  const handleDeleteEnv = useCallback(async () => {
    if (!selectedName) return;
    await deleteEnvironment(selectedName);
    setSelectedName(environments.find((e) => e.name !== selectedName)?.name ?? null);
  }, [selectedName, deleteEnvironment, environments]);

  // Persist to backend only — the store is already updated optimistically
  // by updateVariable. Using updateEnvironment here would overwrite the
  // live store with a stale snapshot captured at debounce time.
  const activeCollection = useEnvStore((s) => s.activeCollection);
  const saveEnv = useCallback(
    (env: Environment) => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        if (activeCollection) {
          saveEnvironment(activeCollection, env)
            .then(() => setSavedAt(Date.now()))
            .catch((err) => {
              console.error('[EnvironmentDialog] save failed:', err);
              toast.error('Failed to save changes');
            });
        }
      }, 500);
    },
    [activeCollection],
  );

  const updateVariable = useCallback(
    (idx: number, patch: Partial<Variable>) => {
      if (!selectedEnv) return;
      const variables = selectedEnv.variables.slice();
      variables[idx] = { ...variables[idx], ...patch };
      const updated = { ...selectedEnv, variables };
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) => (e.name === updated.name ? updated : e)),
      }));
      saveEnv(updated);
    },
    [selectedEnv, saveEnv],
  );

  const addVariable = useCallback(() => {
    if (!selectedEnv) return;
    const variable: Variable = {
      key: '',
      value: '',
      enabled: true,
      secret: false,
    };
    const updated = {
      ...selectedEnv,
      variables: [...selectedEnv.variables, variable],
    };
    useEnvStore.setState((s) => ({
      environments: s.environments.map((e) => (e.name === updated.name ? updated : e)),
    }));
    saveEnv(updated);
  }, [selectedEnv, saveEnv]);

  const removeVariable = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      const variables = selectedEnv.variables.filter((_, i) => i !== idx);
      const updated = { ...selectedEnv, variables };
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) => (e.name === updated.name ? updated : e)),
      }));
      saveEnv(updated);
    },
    [selectedEnv, saveEnv],
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-2xl p-0 gap-0 backdrop-blur-sm'>
        <DialogHeader className='p-4 pb-2'>
          <DialogTitle>Manage Environments</DialogTitle>
        </DialogHeader>
        <div className='flex border-t  border-border min-h-87.5'>
          {/* Left panel: environment list. */}
          <div className='w-50 border-r border-border flex flex-col'>
            <ScrollArea className='flex-1'>
              <div className='p-2 space-y-0.5'>
                {environments.map((env) => (
                  <button
                    key={env.name}
                    type='button'
                    onClick={() => setSelectedName(env.name)}
                    className={cn(
                      'w-full text-left px-2 py-1.5 text-sm rounded-sm truncate',
                      selectedName === env.name
                        ? 'bg-accent text-accent-foreground'
                        : 'text-foreground hover:bg-muted/60',
                    )}
                  >
                    {env.name}
                  </button>
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
            <div className='p-2 border-t border-border flex gap-1'>
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
                className='h-7 w-7 text-destructive'
                onClick={handleDeleteEnv}
                disabled={!selectedName}
                title='Delete environment'
              >
                <Trash2 className='h-3.5 w-3.5' />
              </Button>
            </div>
          </div>

          {/* Right panel: variable editor. */}
          <div className='flex-1 flex flex-col'>
            {selectedEnv ? (
              <>
                <ScrollArea className='flex-1 p-3'>
                  <div className='space-y-1.5'>
                    {selectedEnv.variables.map((variable, idx) => (
                      // biome-ignore lint/suspicious/noArrayIndexKey: index is stable here — rows are not reordered
                      <div key={idx} className='flex gap-1.5 items-center'>
                        <Checkbox
                          checked={variable.enabled}
                          onCheckedChange={(checked) => updateVariable(idx, { enabled: !!checked })}
                          aria-label={`${variable.enabled ? 'Disable' : 'Enable'} variable`}
                        />
                        <Input
                          placeholder='Key'
                          value={variable.key}
                          onChange={(e) => updateVariable(idx, { key: e.target.value })}
                          className='flex-1 text-sm'
                        />
                        <Input
                          placeholder='Value'
                          type={variable.secret ? 'password' : 'text'}
                          value={variable.value}
                          onChange={(e) => updateVariable(idx, { value: e.target.value })}
                          className='flex-1 text-sm'
                        />
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-6 w-6 shrink-0'
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
                          className='h-6 w-6 shrink-0'
                          onClick={() => removeVariable(idx)}
                        >
                          <X className='h-3.5 w-3.5' />
                        </Button>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
                <div className='p-3 pt-0 flex items-center justify-between'>
                  <Button variant='ghost' size='sm' onClick={addVariable} className='text-sm'>
                    <Plus className='h-3.5 w-3.5 mr-1' />
                    Add Variable
                  </Button>
                  {savedAt !== null && <SavedPill key={savedAt} />}
                </div>
              </>
            ) : (
              <div className='flex-1 flex items-center justify-center text-sm text-muted-foreground'>
                Select or create an environment.
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
