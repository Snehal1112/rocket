import { useMemo } from 'react';
import { useCollections } from '@/lib/queries/collection-queries';

/** The names of the collections in the active workspace, or null while the list loads. */
export function useWorkspaceCollectionNames(): string[] | null {
  const { data } = useCollections();
  return useMemo(() => (data ? data.map((c) => c.name) : null), [data]);
}
