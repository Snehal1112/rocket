import { type UseQueryResult, useQueries } from '@tanstack/react-query';
import { useMemo } from 'react';
import type { ReferenceItem } from '@/lib/assistant/types';
import { useCollections } from '@/lib/queries/collection-queries';
import { environmentKeys } from '@/lib/queries/environment-queries';
import {
  type Collection,
  type Environment,
  getCollectionSummaries,
  listEnvironments,
} from '@/lib/tauri-api';
import { environmentReferences, flattenCollectionTree } from './reference-source';

// Module-level combiners keep the combined arrays stable while the data is unchanged.
const treesOf = (results: UseQueryResult<Collection>[]) => results.map((result) => result.data);
const environmentsOf = (results: UseQueryResult<Environment[]>[]) =>
  results.map((result) => result.data);

/**
 * Every collection, folder, HTTP request and environment of the workspace, for the
 * `#` list. Trees load with the same lightweight summaries call as the sidebar, and
 * environments share the `useEnvironments` cache key.
 */
export function useReferenceItems(): ReferenceItem[] {
  const { data: collections } = useCollections();
  const names = useMemo(() => (collections ?? []).map((c) => c.name), [collections]);
  const trees = useQueries({
    queries: names.map((name) => ({
      queryKey: ['assistant', 'reference-tree', name],
      queryFn: () => getCollectionSummaries(name),
    })),
    combine: treesOf,
  });
  const environments = useQueries({
    queries: names.map((name) => ({
      queryKey: environmentKeys.collection(name),
      queryFn: () => listEnvironments(name),
    })),
    combine: environmentsOf,
  });
  return useMemo(
    () =>
      names.flatMap((name, i) => {
        const tree = trees[i];
        const envs = environments[i];
        const fallback: ReferenceItem = { kind: 'collection', collection: name, label: name };
        return [
          ...(tree ? flattenCollectionTree(name, tree.root) : [fallback]),
          ...(envs ? environmentReferences(name, envs) : []),
        ];
      }),
    [names, trees, environments],
  );
}
