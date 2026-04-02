import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { CollectionVariablesEditor } from '@/components/collections/CollectionVariablesEditor';
import { getRequestVariables, saveRequestVariables } from '@/lib/tauri-api';
import type { CollectionVariable } from '@/lib/tauri-api';

interface RequestVariablesPanelProps {
  collection: string;
  requestPath: string;
  onVarCountChange?: (count: number) => void;
}

export function RequestVariablesPanel({
  collection,
  requestPath,
  onVarCountChange,
}: RequestVariablesPanelProps) {
  const [vars, setVars] = useState<CollectionVariable[]>([]);
  const [saved, setSaved] = useState(false);

  // Load variables on mount or when the request path changes.
  useEffect(() => {
    getRequestVariables(collection, requestPath)
      .then((loaded) => {
        setVars(loaded);
        onVarCountChange?.(loaded.length);
      })
      .catch((err) => console.error('Failed to load request variables:', err));
  }, [collection, requestPath, onVarCountChange]);

  const handleChange = useCallback(
    (updated: CollectionVariable[]) => {
      setVars(updated);
      onVarCountChange?.(updated.length);
    },
    [onVarCountChange],
  );

  const handleSave = useCallback(() => {
    saveRequestVariables(collection, requestPath, vars)
      .then(() => {
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      })
      .catch((err) => console.error('Failed to save request variables:', err));
  }, [collection, requestPath, vars]);

  return (
    <div className="space-y-4">
      <div className="rounded-md border border-border bg-muted/30 px-3 py-2">
        <p className="text-xs text-muted-foreground">
          Request variables are available to this request only. They have higher priority
          than folder, environment, and collection variables.
        </p>
      </div>

      <CollectionVariablesEditor variables={vars} onChange={handleChange} />

      <div className="flex items-center gap-3">
        <Button size="sm" onClick={handleSave}>
          Save
        </Button>
        {saved && (
          <span className="text-xs text-green-600 dark:text-green-400">Saved</span>
        )}
      </div>
    </div>
  );
}
