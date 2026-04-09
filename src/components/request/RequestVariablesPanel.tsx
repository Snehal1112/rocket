import { Check, Info } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { CollectionVariablesEditor } from '@/components/collections/CollectionVariablesEditor';
import { Button } from '@/components/ui/button';
import type { CollectionVariable } from '@/lib/tauri-api';
import { getRequestVariables, saveRequestVariables } from '@/lib/tauri-api';

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
  // Timer ref to cancel the "Saved" indicator on unmount.
  const savedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load variables on mount or when the request path changes.
  useEffect(() => {
    let active = true;
    getRequestVariables(collection, requestPath)
      .then((loaded) => {
        if (!active) return;
        setVars(loaded);
        onVarCountChange?.(loaded.length);
      })
      .catch((err) => console.error('[RequestVariablesPanel] load failed:', err));
    return () => {
      active = false;
    };
  }, [collection, requestPath, onVarCountChange]);

  // Clear the "Saved" timer on unmount.
  useEffect(() => {
    return () => {
      if (savedTimerRef.current !== null) clearTimeout(savedTimerRef.current);
    };
  }, []);

  const handleChange = useCallback(
    (updated: CollectionVariable[]) => {
      setVars(updated);
      onVarCountChange?.(updated.length);
    },
    [onVarCountChange],
  );

  const handleSave = useCallback(() => {
    void saveRequestVariables(collection, requestPath, vars)
      .then(() => {
        setSaved(true);
        if (savedTimerRef.current !== null) clearTimeout(savedTimerRef.current);
        savedTimerRef.current = setTimeout(() => setSaved(false), 2000);
      })
      .catch((err) => console.error('[RequestVariablesPanel] save failed:', err));
  }, [collection, requestPath, vars]);

  return (
    <div className='space-y-3'>
      {/* Info banner. */}
      <div className='flex items-start gap-2.5 rounded-md border border-border bg-muted/20 px-3 py-2.5'>
        <Info className='h-3.5 w-3.5 text-muted-foreground/60 mt-0.5 shrink-0' />
        <p className='text-[11px] text-muted-foreground leading-relaxed'>
          Request variables are scoped to this request only and take priority over folder,
          environment, and collection variables.
        </p>
      </div>

      {/* showDescription=false avoids duplicating the built-in info banner. */}
      <CollectionVariablesEditor variables={vars} onChange={handleChange} showDescription={false} />

      {/* Save footer. */}
      <div className='flex items-center gap-2.5 pt-1'>
        <Button size='sm' onClick={handleSave} className='gap-1.5'>
          {saved ? (
            <>
              <Check className='h-3.5 w-3.5' />
              Saved
            </>
          ) : (
            'Save'
          )}
        </Button>
      </div>
    </div>
  );
}
