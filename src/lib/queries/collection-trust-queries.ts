import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import {
  type CollectionCapability,
  type CollectionTrust,
  getCollectionTrust,
  grantRequestedCapabilities,
  onCollectionChanged,
  onCollectionTrustChanged,
  type RequestedCapability,
  revokeCollectionTrust,
  setCollectionCapability,
} from '@/lib/tauri-api';

export const collectionTrustKeys = {
  all: ['collection-trust'] as const,
  one: (collection: string) => ['collection-trust', collection] as const,
};

/** The requested, allowed and effective capabilities of a collection. */
export function useCollectionTrust(collection: string | null | undefined) {
  return useQuery({
    queryKey: collectionTrustKeys.one(collection ?? ''),
    queryFn: () => getCollectionTrust(collection ?? ''),
    enabled: !!collection,
    // A file pull or a grant elsewhere changes the answer, so it is never trusted for long.
    staleTime: 0,
  });
}

/**
 * Refreshes every trust query after a grant or a file change. A pull or checkout reaches
 * the app as a collection-changed event, so no git specific hook is needed. Mount once.
 */
export function useCollectionTrustEvents(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const refresh = () => {
      void queryClient.invalidateQueries({ queryKey: collectionTrustKeys.all });
    };
    const unsubs = [onCollectionTrustChanged(refresh), onCollectionChanged(refresh)];
    return () => {
      for (const unsub of unsubs) void unsub.then((fn) => fn()).catch(() => undefined);
    };
  }, [queryClient]);
}

function useTrustMutation<V>(collection: string, run: (vars: V) => Promise<CollectionTrust>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSuccess: (trust) => {
      queryClient.setQueryData(collectionTrustKeys.one(collection), trust);
    },
  });
}

export function useSetCapability(collection: string) {
  return useTrustMutation(collection, (v: { capability: CollectionCapability; enabled: boolean }) =>
    setCollectionCapability(collection, v.capability, v.enabled),
  );
}

export function useGrantRequested(collection: string) {
  return useTrustMutation(
    collection,
    (v: { capabilities: RequestedCapability[]; fingerprint: string }) =>
      grantRequestedCapabilities(collection, v.capabilities, v.fingerprint),
  );
}

export function useRevokeTrust(collection: string) {
  return useTrustMutation<void>(collection, () => revokeCollectionTrust(collection));
}
