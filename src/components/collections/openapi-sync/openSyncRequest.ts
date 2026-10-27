import { findTabInTree, mapApiRequestToState } from '@/lib/pane-utils';
import { getRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

interface OpenSyncRequestArgs {
  collection: string;
  /** The request's uid. A file without one falls back to the loaded request's uid. */
  uid: string;
  /** The path the sync page knows, without the file extension. */
  requestPath: string;
}

/** Opens a request of the collection in a tab, or focuses the tab that is already open. */
export async function openSyncRequest({
  collection,
  uid,
  requestPath,
}: OpenSyncRequestArgs): Promise<'opened' | 'missing'> {
  const { root, openTab } = usePaneStore.getState();
  const existing = uid ? findTabInTree(root, uid) : null;
  if (existing) {
    openTab(existing.tab);
    return 'opened';
  }
  let path = requestPath;
  let full: Awaited<ReturnType<typeof getRequest>>;
  try {
    full = await getRequest(collection, path);
  } catch {
    // The sync page drops the extension, and a Bruno 3 request file ends in .yaml.
    try {
      path = `${requestPath}.yaml`;
      full = await getRequest(collection, path);
    } catch {
      return 'missing';
    }
  }
  openTab({
    id: uid || full.uid,
    title: full.name,
    tabType: 'request',
    request: mapApiRequestToState(full, true),
    response: null,
    isDirty: false,
    source: { collection, path },
  });
  return 'opened';
}
