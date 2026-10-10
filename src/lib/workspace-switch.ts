import type { QueryClient } from '@tanstack/react-query';
import {
  flushAutoSaves,
  resumeAutoSaves,
  suspendAutoSaves,
  waitForAutoSaves,
} from '@/lib/auto-save';
import { flowKeys } from '@/lib/queries/flow-queries';
import { clearSavedRequestPreviewCache } from '@/lib/saved-request-preview';
import {
  closeWorkspace,
  deleteWorkspace,
  getActiveWorkspace,
  switchWorkspace,
  type Workspace,
} from '@/lib/tauri-api';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { useFolderAuthStore } from '@/stores/folder-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

// Workspace changes run one at a time, so a second quick switch cannot start while the
// first one still holds autosaves off.
let queue: Promise<unknown> = Promise.resolve();

function oneAtATime<T>(change: () => Promise<T>): Promise<T> {
  const run = queue.then(change, change);
  queue = run.catch(() => undefined);
  return run;
}

/**
 * Runs `change`, which makes another workspace active, with autosaves held off.
 *
 * The backend writes requests to whatever workspace is active. With `saveFirst`, every dirty
 * tab and pending autosave is saved into the current workspace first, and a failed save stops
 * the change so the edit is not lost. Without it (a delete, whose folder goes away), only
 * saves already running are awaited. `change` returns the new active workspace id, which is
 * set before autosaves resume, so a save stamped with the old id is dropped.
 */
async function changeActiveWorkspace(
  saveFirst: boolean,
  change: () => Promise<string>,
): Promise<void> {
  if (saveFirst) {
    const failed = await flushAutoSaves(usePaneStore.getState().dirtyRequestTabs());
    if (failed > 0) {
      throw new Error(
        `Could not save ${failed} request(s), so the workspace was not changed. Save them and try again.`,
      );
    }
  } else {
    await waitForAutoSaves();
  }
  suspendAutoSaves();
  try {
    const activeId = await change();
    useWorkspaceStore.getState().setActiveWorkspaceId(activeId);
  } finally {
    resumeAutoSaves();
  }
}

/** Switches the backend to another workspace without losing or misplacing an edit. */
export function switchWorkspaceSafely(id: string): Promise<Workspace> {
  return oneAtATime(async () => {
    let switched: Workspace | undefined;
    await changeActiveWorkspace(true, async () => {
      switched = await switchWorkspace(id);
      return switched.id;
    });
    if (!switched) throw new Error('The workspace switch returned no workspace.');
    return switched;
  });
}

/**
 * Closes or deletes a workspace. When it is the active one, the backend activates another
 * workspace and sends `workspace-switched`, so the change runs like a switch: open edits are
 * saved first for a close, and the new active id is set before autosaves resume.
 */
function removeWorkspaceSafely(
  id: string,
  remove: (id: string) => Promise<void>,
  saveFirst: boolean,
) {
  return oneAtATime(async () => {
    if (useWorkspaceStore.getState().activeWorkspaceId !== id) {
      await remove(id);
      return;
    }
    await changeActiveWorkspace(saveFirst, async () => {
      await remove(id);
      return (await getActiveWorkspace()).id;
    });
  });
}

export const closeWorkspaceSafely = (id: string) => removeWorkspaceSafely(id, closeWorkspace, true);

// The folder is removed with its requests, so open edits are not saved into it first.
export const deleteWorkspaceSafely = (id: string) =>
  removeWorkspaceSafely(id, deleteWorkspace, false);

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
