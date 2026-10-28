import { findAffectedTabs } from '@/components/collections/tree-utils';
import { usePaneStore } from '@/stores/pane-store';

/** The forms a request path can have in a tab: with or without the file extension. */
function variants(path: string): string[] {
  const bare = path.replace(/\.(yml|yaml)$/, '');
  return [bare, `${bare}.yml`, `${bare}.yaml`];
}

/**
 * Closes the tabs of requests that were deleted from the sync page, so a tab cannot save a
 * deleted request back. Unsaved edits in such a tab go with it, like in the sidebar delete.
 */
export function closeDeletedRequestTabs(collection: string, paths: string[]): void {
  const store = usePaneStore.getState();
  for (const path of paths) {
    store.closeExampleTabs(collection, path);
    for (const candidate of variants(path)) {
      const target = { type: 'request' as const, collection, path: candidate, name: candidate };
      for (const { tab, groupId } of findAffectedTabs(usePaneStore.getState().root, target)) {
        store.closeTab(tab.id, groupId);
      }
    }
  }
}
