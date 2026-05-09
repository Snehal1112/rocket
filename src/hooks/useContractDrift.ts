import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef } from 'react';
import { useContractsStore } from '@/stores/contracts/contractsSlice';

const DRIFT_DEBOUNCE_MS = 250;

/**
 * Subscribes to collection-changed Tauri events and triggers
 * Rust drift recomputation debounced at 250ms.
 *
 * Uses a stable ref-based debounce so the timer is never reset by
 * re-renders. Also fires on visibilitychange (user returns to the app).
 *
 * Option B: calls recompute_drift Tauri command — never uses frontend drift.ts.
 */
export function useContractDrift(collectionId: string) {
  const recomputeDrift = useContractsStore((s) => s.recomputeDrift);

  // Stable refs to current values — avoids stale closures without resetting timer
  const recomputeRef = useRef(recomputeDrift);
  const collectionIdRef = useRef(collectionId);
  useEffect(() => {
    recomputeRef.current = recomputeDrift;
  }, [recomputeDrift]);
  useEffect(() => {
    collectionIdRef.current = collectionId;
  }, [collectionId]);

  // Stable debounced trigger (ref-based, never reset by re-renders)
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const trigger = useRef(() => {
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(
      () => recomputeRef.current(collectionIdRef.current),
      DRIFT_DEBOUNCE_MS,
    );
  });

  // Subscribe to collection-changed Tauri events (fired when requests are saved)
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen('collection-changed', () => {
      trigger.current();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.warn('[useContractDrift] failed to subscribe:', err));

    return () => {
      unlisten?.();
      clearTimeout(timerRef.current);
    };
  }, []); // empty deps — subscribed once, uses refs for current values

  // Also fire on tab focus (user returns to the app)
  useEffect(() => {
    function onVisibility() {
      if (!document.hidden) trigger.current();
    }
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, []);

  // Expose for manual trigger
  return { triggerDrift: useCallback(() => trigger.current(), []) };
}
