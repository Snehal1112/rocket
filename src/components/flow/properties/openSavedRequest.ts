import { findTabInTree, mapApiRequestToState } from '@/lib/pane-utils';
import { getRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

/**
 * Opens a saved request in a normal request tab, the same way the collection
 * sidebar does. An already-open tab is only focused. Rejects when the file
 * cannot be read, so the caller can show the error.
 */
export async function openSavedRequestTab(collection: string, path: string): Promise<void> {
  const request = await getRequest(collection, path);
  const store = usePaneStore.getState();
  const existing = findTabInTree(store.root, request.uid);
  if (existing) {
    store.openTab(existing.tab);
    return;
  }
  const tab: RequestTab = {
    id: request.uid,
    title: request.name,
    tabType: 'request',
    request: mapApiRequestToState(request, true),
    response: null,
    isDirty: false,
    source: { collection, path },
  };
  store.openTab(tab);
}
