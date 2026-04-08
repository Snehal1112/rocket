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
  // Refs keep trigger stable (empty dep array) while always reading latest values.
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const stateRef = useRef<SaveButtonState>('idle');
  const errorMessageRef = useRef(errorMessage);
  errorMessageRef.current = errorMessage;
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const trigger = useCallback(async () => {
    if (stateRef.current !== 'idle') return;
    if (timerRef.current) clearTimeout(timerRef.current);
    stateRef.current = 'saving';
    setState('saving');
    try {
      await fnRef.current();
      stateRef.current = 'success';
      setState('success');
      timerRef.current = setTimeout(() => {
        stateRef.current = 'idle';
        setState('idle');
      }, 2000);
    } catch (err) {
      console.error(err);
      toast.error(errorMessageRef.current);
      stateRef.current = 'idle';
      setState('idle');
    }
  }, []);

  // Clear the success timer if the component unmounts.
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return { state, trigger };
}
