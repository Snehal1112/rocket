import { useMemo } from 'react';
import type { NetworkLink } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

export interface CollectionSyncEntry {
  link: NetworkLink;
  /** The peer is paired and online now. */
  online: boolean;
}

const NONE: CollectionSyncEntry[] = [];

/**
 * The links of one collection, matched by collection name in the active workspace, with the
 * peer's online flag. A collection without links gets the same empty array each time.
 */
export function useCollectionSyncLinks(collectionName: string): CollectionSyncEntry[] {
  const links = useNetworkStore((s) => s.links);
  // Only the ids of the online paired nodes matter here. A string keeps a presence update that
  // changes nothing relevant from re-rendering every collection row.
  const onlineKey = useNetworkStore((s) =>
    s.nodes
      .filter((n) => n.group === 'paired' && n.status === 'online')
      .map((n) => n.deviceId)
      .sort()
      .join('\n'),
  );

  return useMemo(() => {
    const mine = links.filter((l) => l.collectionName === collectionName);
    if (mine.length === 0) return NONE;
    const online = new Set(onlineKey === '' ? [] : onlineKey.split('\n'));
    return mine.map((link) => ({ link, online: online.has(link.deviceId) }));
  }, [links, onlineKey, collectionName]);
}
