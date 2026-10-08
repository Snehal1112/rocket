import { useQuery } from '@tanstack/react-query';
import { listFlows } from '@/lib/tauri-api';

export const flowKeys = {
  all: ['flows'] as const,
  collection: (collectionName: string) => ['flows', collectionName] as const,
};

/**
 * The flow names of one collection. Invalidate `flowKeys.collection(name)` after
 * create, rename and delete, because the app caches queries for 30 seconds.
 * Pass `enabled = false` to hold the fetch until the list is on screen.
 */
export function useFlows(collectionName: string | null, enabled = true) {
  return useQuery({
    queryKey: flowKeys.collection(collectionName ?? ''),
    queryFn: () => listFlows(collectionName ?? ''),
    enabled: !!collectionName && enabled,
  });
}
