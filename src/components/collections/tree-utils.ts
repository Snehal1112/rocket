import { isPathWithin } from '@/lib/pane-utils';
import type { PaneNode, Tab } from '@/types/pane-types';
import { isFlowTab, isFolderTab, isScriptTab } from '@/types/pane-types';

// Returns true if any active tab in the pane tree matches the given tabId.
export function isActiveRequest(node: PaneNode, tabId: string): boolean {
  if (node.type === 'leaf') return node.activeTabId === tabId;
  return isActiveRequest(node.children[0], tabId) || isActiveRequest(node.children[1], tabId);
}

// Describes the item targeted for deletion in the shared confirmation dialog.
export type DeleteTarget = {
  type: 'collection' | 'folder' | 'request' | 'script' | 'flow';
  collection: string;
  path?: string;
  name: string;
};

export { isPathWithin };

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
      // Flow tabs have no source. They match by collection and flow name.
      if (isFlowTab(tab)) {
        if (
          target.type === 'flow' &&
          tab.collectionName === target.collection &&
          tab.flowName === target.name
        ) {
          found.push({ tab, groupId: node.groupId });
        }
        continue;
      }
      // Folder tabs have no source. They match by collection and folder path.
      if (isFolderTab(tab)) {
        if (tab.collectionName !== target.collection) continue;
        const folderMatches =
          target.type === 'collection' ||
          (target.type === 'folder' && isPathWithin(tab.folderPath, target.path ?? ''));
        if (folderMatches) found.push({ tab, groupId: node.groupId });
        continue;
      }
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
