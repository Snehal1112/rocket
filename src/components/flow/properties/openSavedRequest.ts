import { findTabInTree, mapApiRequestToState } from '@/lib/pane-utils';
import { getRequest } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

export type OpenSavedRequestResult = 'opened' | 'other-collection';

/**
 * Opens a saved request in a normal request tab, the same way the collection
 * sidebar does. An already-open tab is only focused. Rejects when the file
 * cannot be read, so the caller can show the error.
 *
 * Opening a tab from another collection would switch collections, which hides
 * the flow tab, so that case is reported instead of opened.
 */
export async function openSavedRequestTab(
  collection: string,
  path: string,
): Promise<OpenSavedRequestResult> {
  const inOtherCollection = () => {
    const active = usePaneStore.getState().activeCollection;
    return active !== null && active !== collection;
  };
  if (inOtherCollection()) return 'other-collection';

  const request = await getRequest(collection, path);
  // The active collection may have changed while the file loaded.
  if (inOtherCollection()) return 'other-collection';
  const store = usePaneStore.getState();
  // With no active collection, openTab would switch and drop the open tabs,
  // the flow tab among them. Adopt the collection in place instead, and sync
  // the env store the way switchCollection does.
  if (store.activeCollection === null) {
    store.setActiveCollection(collection);
    useEnvStore.getState().setActiveCollection(collection);
    const storedEnv = localStorage.getItem(`rocket-api:active-env:${collection}`);
    useEnvStore.getState().setActiveEnvId(storedEnv ?? null);
  }

  const existing = findTabInTree(store.root, request.uid);
  if (existing) {
    store.openTab(existing.tab);
    return 'opened';
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
  return 'opened';
}
