import { useCallback, useEffect, useRef, useState } from 'react';
import { CollectionVariablesEditor } from '@/components/collections/CollectionVariablesEditor';
import type { CollectionVariable } from '@/lib/tauri-api';
import { getRequestVariables, saveRequestVariables } from '@/lib/tauri-api';

interface RequestVariablesPanelProps {
  collection: string;
  requestPath: string;
  /** Called after each auto-save so the parent can refresh its variable context. */
  onSaved?: (vars: CollectionVariable[]) => void;
}

export function RequestVariablesPanel({
  collection,
  requestPath,
  onSaved,
}: RequestVariablesPanelProps) {
  const [vars, setVars] = useState<CollectionVariable[]>([]);
  // Track whether vars have been loaded from disk at least once.
  const loadedRef = useRef(false);
  // Debounce timer for auto-save.
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Latest vars that have not yet been flushed to disk. Null means no pending write.
  const pendingVarsRef = useRef<CollectionVariable[] | null>(null);
  // Stable refs so the unmount flush can access the latest values without stale closures.
  const collectionRef = useRef(collection);
  const requestPathRef = useRef(requestPath);
  const onSavedRef = useRef(onSaved);
  collectionRef.current = collection;
  requestPathRef.current = requestPath;
  onSavedRef.current = onSaved;

  // Load variables on mount or when the request path changes.
  useEffect(() => {
    let active = true;
    loadedRef.current = false;
    pendingVarsRef.current = null;
    getRequestVariables(collection, requestPath)
      .then((loaded) => {
        if (!active) return;
        loadedRef.current = true;
        setVars(loaded);
      })
      .catch((err) => console.error('[RequestVariablesPanel] load failed:', err));
    return () => {
      active = false;
      if (saveTimerRef.current !== null) {
        clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
        // Flush any pending unsaved changes before unmounting so a concurrent
        // save_request call cannot overwrite them with stale disk content.
        if (pendingVarsRef.current !== null) {
          const pending = pendingVarsRef.current;
          void saveRequestVariables(collectionRef.current, requestPathRef.current, pending)
            .then(() => onSavedRef.current?.(pending))
            .catch((err) => console.error('[RequestVariablesPanel] flush-on-unmount failed:', err));
          pendingVarsRef.current = null;
        }
      }
    };
  }, [collection, requestPath]);

  // Auto-save with 400ms debounce whenever vars change after the initial load.
  useEffect(() => {
    if (!loadedRef.current) return;
    pendingVarsRef.current = vars;
    if (saveTimerRef.current !== null) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      saveTimerRef.current = null;
      const toSave = pendingVarsRef.current;
      if (toSave === null) return;
      pendingVarsRef.current = null;
      void saveRequestVariables(collection, requestPath, toSave)
        .then(() => onSaved?.(toSave))
        .catch((err) => console.error('[RequestVariablesPanel] auto-save failed:', err));
    }, 400);
    return () => {
      if (saveTimerRef.current !== null) clearTimeout(saveTimerRef.current);
    };
  }, [vars, collection, requestPath, onSaved]);

  const handleChange = useCallback((updated: CollectionVariable[]) => {
    setVars(updated);
  }, []);

  return (
    <div className='flex flex-col h-full overflow-hidden'>
      <div className='flex-1 overflow-y-auto overflow-x-hidden min-h-0'>
        <CollectionVariablesEditor
          variables={vars}
          onChange={handleChange}
          showDescription={false}
        />
      </div>
    </div>
  );
}
