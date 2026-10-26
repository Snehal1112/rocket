import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { getOpenApiSyncPlan, getOpenApiSyncStatus, onOpenApiSyncChecked } from '@/lib/tauri-api';

export const openApiSyncKeys = {
  all: ['openapi-sync'] as const,
  status: (collection: string) => ['openapi-sync', 'status', collection] as const,
  plan: (collection: string) => ['openapi-sync', 'plan', collection] as const,
};

/** The sync status of a collection, or null when it has no spec source. */
export function useOpenApiSyncStatus(collection: string | null | undefined) {
  return useQuery({
    queryKey: openApiSyncKeys.status(collection ?? ''),
    queryFn: () => getOpenApiSyncStatus(collection ?? ''),
    enabled: !!collection,
    // A background check changes the answer, so it is never trusted for long.
    staleTime: 0,
    // An unreadable state file is a result to show, not a hiccup to retry.
    retry: false,
  });
}

/**
 * Whether the Spec sync section is offered for a collection. It is true when the status
 * resolved to a source, or errored, so an unreadable state file still shows its message.
 * `pending` is true while the status loads.
 */
export function useSpecSyncAvailability(collection: string | null | undefined): {
  available: boolean;
  pending: boolean;
} {
  const { data, isError, isPending, fetchStatus } = useOpenApiSyncStatus(collection);
  return {
    available: !!data || isError,
    pending: !!collection && isPending && fetchStatus !== 'idle',
  };
}

/** The plan of the pending spec. It only runs while enabled. */
export function useOpenApiSyncPlan(collection: string | null | undefined, enabled: boolean) {
  return useQuery({
    queryKey: openApiSyncKeys.plan(collection ?? ''),
    queryFn: () => getOpenApiSyncPlan(collection ?? ''),
    enabled: enabled && !!collection,
    staleTime: 0,
    retry: false,
  });
}

/**
 * Refreshes the status and plan queries of the collection named in a sync check event.
 * Mount it from the Spec sync tab and the notice, never globally.
 */
export function useOpenApiSyncEvents(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unsub = onOpenApiSyncChecked(({ collection }) => {
      void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.status(collection) });
      void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.plan(collection) });
    });
    return () => {
      void unsub.then((fn) => fn()).catch(() => undefined);
    };
  }, [queryClient]);
}
