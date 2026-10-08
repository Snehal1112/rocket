import { useEffect, useRef } from 'react';

interface FlowSaveShortcutProps {
  tabId: string;
  // Saves the flow. Resolves true on success.
  onSave: () => Promise<boolean>;
  // Reads the latest dirty state, since the listener outlives renders.
  isDirty: () => boolean;
}

// Renders nothing. The global Ctrl+S handler and the tab menu dispatch
// `rocket:save-draft` with a tab id, and this saves when the id is ours.
export function FlowSaveShortcut({ tabId, onSave, isDirty }: FlowSaveShortcutProps) {
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const isDirtyRef = useRef(isDirty);
  isDirtyRef.current = isDirty;
  // Blocks a second save while one is still pending.
  const savingRef = useRef(false);

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId?: string }>).detail;
      if (detail?.tabId !== tabId || savingRef.current || !isDirtyRef.current()) return;
      savingRef.current = true;
      void onSaveRef.current().finally(() => {
        savingRef.current = false;
      });
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tabId]);

  return null;
}
