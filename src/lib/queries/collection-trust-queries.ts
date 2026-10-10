import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { environmentKeys } from '@/lib/queries/environment-queries';
import {
  type CollectionCapability,
  type CollectionTrust,
  getCollectionTrust,
  grantRequestedCapabilities,
  onCollectionChanged,
  onCollectionTrustChanged,
  onGitChanged,
  type RequestedCapability,
  revokeCollectionTrust,
  setCollectionCapability,
} from '@/lib/tauri-api';

/** Readable text of a failed trust call. The backend sends plain messages. */
export function trustErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object' && 'message' in err && typeof err.message === 'string') {
    return err.message;
  }
  return 'The change could not be saved.';
}

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
      // The host environment a collection may use depends on its trust.
      void queryClient.invalidateQueries({ queryKey: environmentKeys.processAll });
    };
    const unsubs = [
      onCollectionTrustChanged(refresh),
      onCollectionChanged(refresh),
      onGitChanged(refresh),
    ];
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
    // A refused or failed call may mean the cache is stale, so always refetch.
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: collectionTrustKeys.one(collection) });
      void queryClient.invalidateQueries({ queryKey: environmentKeys.processAll });
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
