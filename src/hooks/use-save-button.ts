import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';

export type SaveButtonState = 'idle' | 'saving' | 'success';

/**
 * Manages the idle → saving → success → idle state machine for a save button.
 * Shows toast.error on failure; caller renders success state visually (no success toast).
 * Pass any async fn — it does not need to be memoized by the caller.
 */
export function useSaveButton(fn: () => Promise<void>, errorMessage = 'Failed to save') {
  const [state, setState] = useState<SaveButtonState>('idle');
  // Keep a ref to the latest fn so callers don't need useCallback.
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const trigger = useCallback(async () => {
    if (state !== 'idle') return;
    if (timerRef.current) clearTimeout(timerRef.current);
    setState('saving');
    try {
      await fnRef.current();
      setState('success');
      timerRef.current = setTimeout(() => setState('idle'), 2000);
    } catch (err) {
      console.error(err);
      toast.error(errorMessage);
      setState('idle');
    }
  }, [state, errorMessage]);

  // Clear the success timer if the component unmounts.
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return { state, trigger };
}
