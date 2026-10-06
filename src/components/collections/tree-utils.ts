import type { PaneNode, Tab } from '@/types/pane-types';
import { isScriptTab } from '@/types/pane-types';

// Returns true if any active tab in the pane tree matches the given tabId.
export function isActiveRequest(node: PaneNode, tabId: string): boolean {
  if (node.type === 'leaf') return node.activeTabId === tabId;
  return isActiveRequest(node.children[0], tabId) || isActiveRequest(node.children[1], tabId);
}

// Describes the item targeted for deletion in the shared confirmation dialog.
export type DeleteTarget = {
  type: 'collection' | 'folder' | 'request' | 'script';
  collection: string;
  path?: string;
  name: string;
};

// True when `path` is `folder` itself or lives below it, matching whole path segments.
export function isPathWithin(path: string, folder: string): boolean {
  return path === folder || path.startsWith(`${folder}/`);
}

export interface AffectedTab {
  tab: Tab;
  groupId: string;
}

// Collects the open tabs that a delete of `target` would remove.
export function findAffectedTabs(root: PaneNode, target: DeleteTarget): AffectedTab[] {
  const found: AffectedTab[] = [];
  const visit = (node: PaneNode): void => {
    if (node.type !== 'leaf') {
      visit(node.children[0]);
      visit(node.children[1]);
      return;
    }
    for (const tab of node.tabs) {
      if (!tab.source || tab.source.collection !== target.collection) continue;
      const path = tab.source.path;
      const matches =
        target.type === 'collection' ||
        ((target.type === 'request' || target.type === 'script') && path === target.path) ||
        (target.type === 'folder' && isPathWithin(path, target.path ?? ''));
      if (matches) found.push({ tab, groupId: node.groupId });
    }
  };
  visit(root);
  return found;
}

// True when deleting `target` would discard unsaved edits in an open script tab.
export function hasDirtyScriptTabs(root: PaneNode, target: DeleteTarget): boolean {
  return findAffectedTabs(root, target).some(({ tab }) => isScriptTab(tab) && tab.isDirty);
}
