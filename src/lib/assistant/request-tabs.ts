import { collectAllTabs } from '@/lib/pane-utils';
import { isRequestTab, type PaneNode, type RequestTab } from '@/types/pane-types';

/**
 * The open request tab that shows `path` of `collection`, in any pane. Its request
 * state holds the user's unsaved edits, scripts included.
 */
export function findRequestTab(
  root: PaneNode,
  collection: string,
  path: string,
): RequestTab | undefined {
  return collectAllTabs(root).find(
    (tab): tab is RequestTab =>
      isRequestTab(tab) && tab.source?.collection === collection && tab.source.path === path,
  );
}
