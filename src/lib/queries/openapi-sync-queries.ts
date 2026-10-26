import { type QueryClient, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import {
  applyOpenApiSync,
  checkOpenApiSyncNow,
  clearOpenApiSyncToken,
  getOpenApiSyncPlan,
  getOpenApiSyncStatus,
  grantOpenApiSyncConsent,
  type OpenApiApplyReport,
  type OpenApiSelection,
  type OpenApiSyncInterval,
  onOpenApiSyncApplied,
  onOpenApiSyncChecked,
  revokeOpenApiSyncConsent,
  setOpenApiSyncInterval,
  setOpenApiSyncToken,
} from '@/lib/tauri-api';
import { collectionKeys } from './collection-queries';

type Links = Array<[string, string]>;

const NO_LINKS: Links = [];

export const openApiSyncKeys = {
  all: ['openapi-sync'] as const,
  /** Matches every status query of the collection, whatever `includeToken` was. */
  statusAll: (collection: string) => ['openapi-sync', 'status', collection] as const,
  /** Matches every plan query of the collection, whatever the links were. */
  planAll: (collection: string) => ['openapi-sync', 'plan', collection] as const,
  status: (collection: string, includeToken = false) =>
    ['openapi-sync', 'status', collection, { includeToken }] as const,
  plan: (collection: string, links: Links = NO_LINKS) =>
    ['openapi-sync', 'plan', collection, { links }] as const,
};

/** The sync status of a collection, or null when it has no spec source. */
export function useOpenApiSyncStatus(collection: string | null | undefined, includeToken = false) {
  return useQuery({
    queryKey: openApiSyncKeys.status(collection ?? '', includeToken),
    queryFn: () => getOpenApiSyncStatus(collection ?? '', includeToken),
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

/** The plan of the pending spec. It only runs while enabled, and a new link set refetches. */
export function useOpenApiSyncPlan(
  collection: string | null | undefined,
  enabled: boolean,
  links: Links = NO_LINKS,
) {
  return useQuery({
    queryKey: openApiSyncKeys.plan(collection ?? '', links),
    queryFn: () => getOpenApiSyncPlan(collection ?? '', links),
    enabled: enabled && !!collection,
    staleTime: 0,
    retry: false,
  });
}

function invalidateSync(queryClient: QueryClient, collection: string): void {
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.statusAll(collection) });
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.planAll(collection) });
}

/** Runs a check now and refreshes the status and plan of the collection. */
export function useCheckNow(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => checkOpenApiSyncNow(collection),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useSetInterval(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (interval: OpenApiSyncInterval) => setOpenApiSyncInterval(collection, interval),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useSetToken(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (token: string) => setOpenApiSyncToken(collection, token),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useClearToken(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => clearOpenApiSyncToken(collection),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useGrantConsent(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (location: string) => grantOpenApiSyncConsent(collection, location),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useRevokeConsent(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => revokeOpenApiSyncConsent(collection),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

/**
 * Applies a selection and refreshes status, plan and the collection tree. Only one apply
 * runs at a time, a call while one is in flight is rejected.
 */
export function useApplySync(collection: string) {
  const queryClient = useQueryClient();
  const inFlight = useRef(false);
  return useMutation<OpenApiApplyReport, unknown, OpenApiSelection>({
    mutationFn: async (selection) => {
      if (inFlight.current) throw new Error('An apply is already running.');
      inFlight.current = true;
      try {
        return await applyOpenApiSync(collection, selection);
      } finally {
        inFlight.current = false;
      }
    },
    onSettled: () => {
      invalidateSync(queryClient, collection);
      void queryClient.invalidateQueries({ queryKey: collectionKeys.all });
    },
  });
}

/**
 * Refreshes the status and plan queries of the collection named in a sync check event, and
 * also the collection tree for an apply event. Mount it from the Spec sync tab and the
 * notice, never globally.
 */
export function useOpenApiSyncEvents(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unsubChecked = onOpenApiSyncChecked(({ collection }) => {
      invalidateSync(queryClient, collection);
    });
    const unsubApplied = onOpenApiSyncApplied(({ collection }) => {
      invalidateSync(queryClient, collection);
      void queryClient.invalidateQueries({ queryKey: collectionKeys.all });
    });
    return () => {
      void unsubChecked.then((fn) => fn()).catch(() => undefined);
      void unsubApplied.then((fn) => fn()).catch(() => undefined);
    };
  }, [queryClient]);
}
