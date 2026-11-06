import { useCallback, useState } from 'react';
import { errorText, toastError } from '@/lib/error-text';

export type ErrorReport = 'toast' | 'inline';

export interface AsyncAction {
  /** True while a call started by `run` is in flight. */
  busy: boolean;
  /** The cleaned error text of the last failed call. Only set in `inline` mode. */
  error: string | null;
  /** Lets a form show its own validation text in the same place. */
  setError: (message: string | null) => void;
  /** Resolves true when the call succeeded and false when it failed. It never rejects. */
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  /** Clears the error and the busy flag, for example when a dialog opens again. */
  reset: () => void;
}

export function useAsyncAction(report: ErrorReport = 'toast'): AsyncAction {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (action: () => Promise<unknown>) => {
      setBusy(true);
      setError(null);
      try {
        await action();
        return true;
      } catch (err) {
        if (report === 'inline') setError(errorText(err));
        else toastError(err);
        return false;
      } finally {
        setBusy(false);
      }
    },
    [report],
  );

  const reset = useCallback(() => {
    setBusy(false);
    setError(null);
  }, []);

  return { busy, error, setError, run, reset };
}
