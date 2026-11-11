import { useQuery } from '@tanstack/react-query';
import { listReceivedCollections, type ReceivedCollection } from '@/lib/tauri-api';

export const receivedKeys = {
  all: ['network', 'received'] as const,
};

const NONE: ReadonlyMap<string, ReceivedCollection> = new Map();

/** Every received collection by uid. A failed read counts as none: the source is only a hint. */
export function useReceivedOrigins(): ReadonlyMap<string, ReceivedCollection> {
  const { data } = useQuery({
    queryKey: receivedKeys.all,
    queryFn: listReceivedCollections,
    // A new entry only appears when a transfer ends, and the event bridge refreshes it then.
    staleTime: 5 * 60_000,
    select: (entries) => new Map(entries.map((e) => [e.collectionUid, e])),
  });
  return data ?? NONE;
}

/** The sender of one collection, or undefined when it was not received over the network. */
export function useReceivedOrigin(uid: string | undefined): ReceivedCollection | undefined {
  const origins = useReceivedOrigins();
  return uid ? origins.get(uid) : undefined;
}
