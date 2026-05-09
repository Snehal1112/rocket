import { useCallback, useEffect, useRef } from 'react';
import { useContractsStore } from '@/stores/contracts/contractsSlice';

function useDebounced(fn: () => void, ms: number): () => void {
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  return useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(fn, ms);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fn, ms]);
}

/**
 * Subscribes to collection changes and triggers Rust drift recomputation.
 * Debounced at 250ms. Fires on tab focus (visibilitychange).
 *
 * Option B: calls recomputeDrift Tauri command (not frontend drift.ts engine).
 * Wire this hook into ContractsTab.
 */
export function useContractDrift(collectionId: string) {
  const recomputeDrift = useContractsStore((s) => s.recomputeDrift);

  const debounced = useDebounced(() => {
    recomputeDrift(collectionId);
  }, 250);

  // Fire on tab focus
  useEffect(() => {
    const onVisibility = () => {
      if (!document.hidden) debounced();
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, [debounced]);

  // Expose trigger for callers that want to fire manually
  return { triggerDrift: debounced };
}
