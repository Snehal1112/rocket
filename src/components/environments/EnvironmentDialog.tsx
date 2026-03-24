// src/components/environments/EnvironmentDialog.tsx
import { useState, useCallback, useRef } from 'react';
import { Plus, Trash2, Eye, EyeOff, Check, X } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import type { Variable, Environment } from '@/lib/tauri-api';

interface EnvironmentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function EnvironmentDialog({ open, onOpenChange }: EnvironmentDialogProps) {
  const environments = useEnvStore((s) => s.environments);
  const createEnvironment = useEnvStore((s) => s.createEnvironment);
  const updateEnvironment = useEnvStore((s) => s.updateEnvironment);
  const deleteEnvironment = useEnvStore((s) => s.deleteEnvironment);

  const [selectedName, setSelectedName] = useState<string | null>(
    environments[0]?.name ?? null,
  );
  const [isAddingEnv, setIsAddingEnv] = useState(false);
  const [newEnvName, setNewEnvName] = useState('');

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const selectedEnv = environments.find((e) => e.name === selectedName) ?? null;

  const handleAddEnv = useCallback(async () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) { setIsAddingEnv(false); return; }
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

  const saveEnv = useCallback(
    (env: Environment) => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void updateEnvironment(env);
      }, 500);
    },
    [updateEnvironment],
  );

  const updateVariable = useCallback(
    (idx: number, patch: Partial<Variable>) => {
      if (!selectedEnv) return;
      const variables = selectedEnv.variables.slice();
      variables[idx] = { ...variables[idx], ...patch };
      const updated = { ...selectedEnv, variables };
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) =>
          e.name === updated.name ? updated : e,
        ),
      }));
      saveEnv(updated);
    },
    [selectedEnv, saveEnv],
  );

  const addVariable = useCallback(() => {
    if (!selectedEnv) return;
    const variable: Variable = { key: '', value: '', enabled: true, secret: false };
    const updated = { ...selectedEnv, variables: [...selectedEnv.variables, variable] };
    useEnvStore.setState((s) => ({
      environments: s.environments.map((e) =>
        e.name === updated.name ? updated : e,
      ),
    }));
    saveEnv(updated);
  }, [selectedEnv, saveEnv]);

  const removeVariable = useCallback(
    (idx: number) => {
      if (!selectedEnv) return;
      const variables = selectedEnv.variables.filter((_, i) => i !== idx);
      const updated = { ...selectedEnv, variables };
      useEnvStore.setState((s) => ({
        environments: s.environments.map((e) =>
          e.name === updated.name ? updated : e,
        ),
      }));
      saveEnv(updated);
    },
    [selectedEnv, saveEnv],
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl p-0 gap-0">
        <DialogHeader className="p-4 pb-2">
          <DialogTitle>Manage Environments</DialogTitle>
        </DialogHeader>
        <div className="flex border-t border-border min-h-[350px]">
          {/* Left panel: environment list. */}
          <div className="w-[200px] border-r border-border flex flex-col">
            <ScrollArea className="flex-1">
              <div className="p-2 space-y-0.5">
                {environments.map((env) => (
                  <button
                    key={env.name}
                    type="button"
                    onClick={() => setSelectedName(env.name)}
                    className={cn(
                      'w-full text-left px-2 py-1.5 text-xs rounded-sm truncate',
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
                    className="h-7 text-xs"
                    placeholder="Environment name"
                    value={newEnvName}
                    onChange={(e) => setNewEnvName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') handleAddEnv();
                      if (e.key === 'Escape') { setIsAddingEnv(false); setNewEnvName(''); }
                    }}
                    onBlur={() => { setIsAddingEnv(false); setNewEnvName(''); }}
                  />
                )}
              </div>
            </ScrollArea>
            <div className="p-2 border-t border-border flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => setIsAddingEnv(true)}
                title="Add environment"
              >
                <Plus className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-destructive"
                onClick={handleDeleteEnv}
                disabled={!selectedName}
                title="Delete environment"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>

          {/* Right panel: variable editor. */}
          <div className="flex-1 flex flex-col">
            {selectedEnv ? (
              <>
                <ScrollArea className="flex-1 p-3">
                  <div className="space-y-1.5">
                    {selectedEnv.variables.map((variable, idx) => (
                      <div key={idx} className="flex gap-1.5 items-center">
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => updateVariable(idx, { enabled: !variable.enabled })}
                          className={cn(
                            'w-4 h-4 rounded border p-0 shrink-0',
                            variable.enabled
                              ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                              : 'border-gray-300 hover:bg-muted',
                          )}
                        >
                          {variable.enabled && <Check className="h-3 w-3" />}
                        </Button>
                        <Input
                          placeholder="Key"
                          value={variable.key}
                          onChange={(e) => updateVariable(idx, { key: e.target.value })}
                          className="flex-1 text-xs h-7"
                        />
                        <Input
                          placeholder="Value"
                          type={variable.secret ? 'password' : 'text'}
                          value={variable.value}
                          onChange={(e) => updateVariable(idx, { value: e.target.value })}
                          className="flex-1 text-xs h-7"
                        />
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 shrink-0"
                          onClick={() => updateVariable(idx, { secret: !variable.secret })}
                          title={variable.secret ? 'Show value' : 'Hide value'}
                        >
                          {variable.secret ? (
                            <EyeOff className="h-3 w-3" />
                          ) : (
                            <Eye className="h-3 w-3" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 shrink-0"
                          onClick={() => removeVariable(idx)}
                        >
                          <X className="h-3 w-3" />
                        </Button>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
                <div className="p-3 pt-0">
                  <Button variant="ghost" size="sm" onClick={addVariable} className="text-xs">
                    <Plus className="h-3 w-3 mr-1" />
                    Add Variable
                  </Button>
                </div>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground">
                Select or create an environment.
              </div>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
