// src/components/workspace/WorkspaceEnvironmentsTab.tsx
import { useState, useEffect, useCallback, useRef } from 'react';
import { Plus, Trash2, Eye, EyeOff, Check, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import { listEnvironments, saveEnvironment, deleteEnvironment } from '@/lib/tauri-api';
import type { Variable, Environment } from '@/lib/tauri-api';

interface WorkspaceEnvironmentsTabProps {
  workspaceId: string;
}

export function WorkspaceEnvironmentsTab({ workspaceId }: WorkspaceEnvironmentsTabProps) {
  const [environments, setEnvironments] = useState<Environment[]>([]);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [editingVars, setEditingVars] = useState<Variable[]>([]);
  const [isAddingEnv, setIsAddingEnv] = useState(false);
  const [newEnvName, setNewEnvName] = useState('');

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load environments when workspace changes.
  useEffect(() => {
    listEnvironments()
      .then((envs) => {
        setEnvironments(envs);
        setSelectedName(envs[0]?.name ?? null);
      })
      .catch((err) => {
        console.error('[WorkspaceEnvironmentsTab] failed to load environments', err);
      });
  }, [workspaceId]);

  // Sync editing vars when selected env changes.
  useEffect(() => {
    const env = environments.find((e) => e.name === selectedName);
    setEditingVars(env ? env.variables.slice() : []);
  }, [selectedName, environments]);

  // Persist env to backend with debounce.
  const persistEnv = useCallback((env: Environment) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      void saveEnvironment(env).catch((err) => {
        console.error('[WorkspaceEnvironmentsTab] failed to save environment', err);
      });
    }, 400);
  }, []);

  // Apply a variable update at index and persist.
  const updateVar = useCallback(
    (idx: number, patch: Partial<Variable>) => {
      if (!selectedName) return;
      const updated = editingVars.slice();
      updated[idx] = { ...updated[idx], ...patch };
      setEditingVars(updated);
      const env = environments.find((e) => e.name === selectedName);
      if (env) {
        const updatedEnv = { ...env, variables: updated };
        setEnvironments((prev) => prev.map((e) => (e.name === selectedName ? updatedEnv : e)));
        persistEnv(updatedEnv);
      }
    },
    [selectedName, editingVars, environments, persistEnv],
  );

  // Add an empty variable row.
  const addVar = useCallback(() => {
    if (!selectedName) return;
    const newVar: Variable = { key: '', value: '', enabled: true, secret: false };
    const updated = [...editingVars, newVar];
    setEditingVars(updated);
    const env = environments.find((e) => e.name === selectedName);
    if (env) {
      const updatedEnv = { ...env, variables: updated };
      setEnvironments((prev) => prev.map((e) => (e.name === selectedName ? updatedEnv : e)));
      persistEnv(updatedEnv);
    }
  }, [selectedName, editingVars, environments, persistEnv]);

  // Remove a variable row by index.
  const removeVar = useCallback(
    (idx: number) => {
      if (!selectedName) return;
      const updated = editingVars.filter((_, i) => i !== idx);
      setEditingVars(updated);
      const env = environments.find((e) => e.name === selectedName);
      if (env) {
        const updatedEnv = { ...env, variables: updated };
        setEnvironments((prev) => prev.map((e) => (e.name === selectedName ? updatedEnv : e)));
        persistEnv(updatedEnv);
      }
    },
    [selectedName, editingVars, environments, persistEnv],
  );

  // Add a new environment by name.
  const handleAddEnv = useCallback(async () => {
    const trimmed = newEnvName.trim();
    if (!trimmed) {
      setIsAddingEnv(false);
      setNewEnvName('');
      return;
    }
    const newEnv: Environment = { name: trimmed, variables: [] };
    try {
      await saveEnvironment(newEnv);
      setEnvironments((prev) => [...prev, newEnv]);
      setSelectedName(trimmed);
    } catch (err) {
      console.error('[WorkspaceEnvironmentsTab] failed to create environment', err);
    }
    setIsAddingEnv(false);
    setNewEnvName('');
  }, [newEnvName]);

  // Delete the selected environment.
  const handleDeleteEnv = useCallback(async () => {
    if (!selectedName) return;
    try {
      await deleteEnvironment(selectedName);
      const remaining = environments.filter((e) => e.name !== selectedName);
      setEnvironments(remaining);
      setSelectedName(remaining[0]?.name ?? null);
    } catch (err) {
      console.error('[WorkspaceEnvironmentsTab] failed to delete environment', err);
    }
  }, [selectedName, environments]);

  return (
    <div className="h-full flex">
      {/* Left panel: environment list. */}
      <div className="w-52 border-r flex flex-col">
        <ScrollArea className="flex-1">
          <div className="p-2 space-y-0.5">
            {environments.map((env) => (
              <button
                key={env.name}
                type="button"
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
                className="h-7 text-sm"
                placeholder="Environment name"
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
        <div className="p-2 border-t flex gap-1">
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
            onClick={() => void handleDeleteEnv()}
            disabled={!selectedName}
            title="Delete environment"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {/* Right panel: variable editor. */}
      <div className="flex-1 flex flex-col">
        {selectedName ? (
          <>
            <ScrollArea className="flex-1 p-3">
              <div className="space-y-1.5">
                {editingVars.map((variable, idx) => (
                  <div key={idx} className="flex gap-1.5 items-center">
                    {/* Enabled toggle. */}
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => updateVar(idx, { enabled: !variable.enabled })}
                      className={cn(
                        'w-4 h-4 rounded border p-0 shrink-0',
                        variable.enabled
                          ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                          : 'border-gray-300 hover:bg-muted',
                      )}
                      title={variable.enabled ? 'Disable variable' : 'Enable variable'}
                    >
                      {variable.enabled && <Check className="h-3.5 w-3.5" />}
                    </Button>

                    {/* Key input. */}
                    <Input
                      placeholder="Key"
                      value={variable.key}
                      onChange={(e) => updateVar(idx, { key: e.target.value })}
                      className="flex-1 text-sm h-7"
                    />

                    {/* Value input, masked when secret. */}
                    <Input
                      placeholder="Value"
                      type={variable.secret ? 'password' : 'text'}
                      value={variable.value}
                      onChange={(e) => updateVar(idx, { value: e.target.value })}
                      className="flex-1 text-sm h-7"
                    />

                    {/* Secret toggle. */}
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 shrink-0"
                      onClick={() => updateVar(idx, { secret: !variable.secret })}
                      title={variable.secret ? 'Show value' : 'Hide value'}
                    >
                      {variable.secret ? (
                        <EyeOff className="h-3.5 w-3.5 text-muted-foreground" />
                      ) : (
                        <Eye className="h-3.5 w-3.5 text-muted-foreground" />
                      )}
                    </Button>

                    {/* Delete row. */}
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 shrink-0"
                      onClick={() => removeVar(idx)}
                      title="Delete variable"
                    >
                      <X className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                ))}
              </div>
            </ScrollArea>

            <div className="p-3 pt-0">
              <Button variant="ghost" size="sm" onClick={addVar} className="text-sm">
                <Plus className="h-3 w-3 mr-1" />
                Add Variable
              </Button>
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
            Select an environment
          </div>
        )}
      </div>
    </div>
  );
}
