import type { QueryClient } from '@tanstack/react-query';
import { flushAutoSaves, resumeAutoSaves, suspendAutoSaves } from '@/lib/auto-save';
import { flowKeys } from '@/lib/queries/flow-queries';
import { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';
import { switchWorkspace, type Workspace } from '@/lib/tauri-api';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

/**
 * Switches the backend to another workspace without losing or misplacing an edit.
 *
 * The backend writes requests to whatever workspace is active, so every dirty tab and
 * pending autosave is saved into the current workspace first. Autosaves are held off
 * while the switch runs. A save that fails stops the switch, so the edit is not lost.
 */
export async function switchWorkspaceSafely(id: string): Promise<Workspace> {
  const failed = await flushAutoSaves(usePaneStore.getState().dirtyRequestTabs());
  if (failed > 0) {
    throw new Error(
      `Could not save ${failed} request(s), so the workspace was not switched. Save them and try again.`,
    );
  }
  suspendAutoSaves();
  try {
    const workspace = await switchWorkspace(id);
    // Set at once, so an autosave stamped with the old id is dropped from here on.
    useWorkspaceStore.getState().setActiveWorkspaceId(workspace.id);
    return workspace;
  } finally {
    resumeAutoSaves();
  }
}

/**
 * Drops cached data of the workspace the user just left.
 *
 * These caches are keyed by collection name, and two workspaces can hold
 * collections with the same name. Global environments, process env and trust
 * queries are handled by the `workspace-switched` listener itself.
 */
export function clearWorkspaceScopedCaches(qc: QueryClient): void {
  // Removed, not invalidated, so no screen shows the old workspace's data while it reloads.
  // Global and process env keys are reloaded by the listener.
  qc.removeQueries({
    predicate: (q) =>
      q.queryKey[0] === 'environments' && q.queryKey[1] !== 'global' && q.queryKey[1] !== 'process',
  });
  qc.removeQueries({ queryKey: flowKeys.all });
  qc.removeQueries({ queryKey: ['assistant', 'reference-tree'] });
  // Flow node previews of saved requests, keyed by collection name and path.
  clearSavedRequestPreviewCache();
  // OAuth2 tokens and auth held in memory must never reach another workspace's requests.
  useCollectionAuthStore.setState({ auths: new Map() });
  useFolderAuthStore.setState({ auths: {} });
  useFlowAuthStore.setState({ auths: {} });
}
