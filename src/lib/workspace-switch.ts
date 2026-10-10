import type { QueryClient } from '@tanstack/react-query';
import { flowKeys } from '@/lib/queries/flow-queries';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';

/**
 * Drops cached data of the workspace the user just left.
 *
 * These caches are keyed by collection name, and two workspaces can hold
 * collections with the same name. Global environments, process env and trust
 * queries are handled by the `workspace-switched` listener itself.
 */
export function clearWorkspaceScopedCaches(qc: QueryClient): void {
  // Per-collection environments. Global and process env keys are reloaded elsewhere.
  void qc.invalidateQueries({
    predicate: (q) =>
      q.queryKey[0] === 'environments' && q.queryKey[1] !== 'global' && q.queryKey[1] !== 'process',
  });
  void qc.invalidateQueries({ queryKey: flowKeys.all });
  void qc.invalidateQueries({ queryKey: ['assistant', 'reference-tree'] });
  // OAuth2 tokens and auth held in memory must never reach another workspace's requests.
  useCollectionAuthStore.setState({ auths: new Map() });
  useFolderAuthStore.setState({ auths: {} });
  useFlowAuthStore.setState({ auths: {} });
}
