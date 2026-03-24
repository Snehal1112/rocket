import { create } from 'zustand';
import type { PaneNode, Tab, ResponseState, RequestState, LeafNode, SplitNode } from '@/types/pane-types';
import { scheduleAutoSave, cancelAutoSave } from '@/lib/auto-save';
import {
  createDefaultTab,
  createDefaultLeaf,
  findTabInTree,
  findActiveLeaf,
  updateLeaf,
  removeLeaf,
  splitLeaf,
} from '@/lib/pane-utils';

// Recursively finds a tab by id and applies an updater function to it.
function updateTabInTree(
  node: PaneNode,
  tabId: string,
  updater: (tab: Tab) => Tab,
): PaneNode {
  if (node.type === 'leaf') {
    const idx = node.tabs.findIndex((t) => t.id === tabId);
    if (idx === -1) return node;
    const tabs = node.tabs.slice();
    tabs[idx] = updater(tabs[idx]);
    return { ...node, tabs } satisfies LeafNode;
  }
  const left = updateTabInTree(node.children[0], tabId, updater);
  const right = updateTabInTree(node.children[1], tabId, updater);
  if (left === node.children[0] && right === node.children[1]) return node;
  return { ...node, children: [left, right] } satisfies SplitNode;
}

// Recursively finds a split node by id and updates its sizes.
function updateSplitSizes(
  node: PaneNode,
  splitId: string,
  sizes: [number, number],
): PaneNode {
  if (node.type === 'leaf') return node;
  if (node.id === splitId) return { ...node, sizes } satisfies SplitNode;
  const left = updateSplitSizes(node.children[0], splitId, sizes);
  const right = updateSplitSizes(node.children[1], splitId, sizes);
  if (left === node.children[0] && right === node.children[1]) return node;
  return { ...node, children: [left, right] } satisfies SplitNode;
}

// Builds the initial store state with one leaf containing one draft tab.
function buildInitialState(): Pick<PaneState, 'root' | 'activeGroupId'> {
  const leaf = createDefaultLeaf();
  return { root: leaf, activeGroupId: leaf.groupId };
}

export interface PaneState {
  root: PaneNode;
  activeGroupId: string;

  // Tab actions.
  newDraftTab: (groupId?: string) => void;
  openTab: (tab: Tab, groupId?: string) => void;
  closeTab: (tabId: string, groupId: string) => void;
  setActiveTab: (tabId: string, groupId: string) => void;
  moveTab: (tabId: string, fromGroupId: string, toGroupId: string) => void;

  // Split actions.
  splitGroup: (groupId: string, direction: 'horizontal' | 'vertical') => void;
  resizePane: (splitId: string, sizes: [number, number]) => void;

  // Request/response state actions.
  updateRequest: (tabId: string, patch: Partial<RequestState>) => void;
  setResponse: (tabId: string, response: ResponseState) => void;
  markDirty: (tabId: string) => void;
  markClean: (tabId: string) => void;

  // Utility.
  reset: () => void;

  updateTabSource: (tabId: string, source: { collection: string; path: string }) => void;
}

export const usePaneStore = create<PaneState>((set, get) => ({
  ...buildInitialState(),

  newDraftTab(groupId) {
    const { root, activeGroupId } = get();
    const targetGroupId = groupId ?? activeGroupId;
    const tab = createDefaultTab();
    const newRoot = updateLeaf(root, targetGroupId, (leaf) => ({
      ...leaf,
      tabs: [...leaf.tabs, tab],
      activeTabId: tab.id,
    }));
    set({ root: newRoot, activeGroupId: targetGroupId });
  },

  openTab(tab, groupId) {
    const { root, activeGroupId } = get();
    // If the tab already exists anywhere in the tree, just activate it.
    const existing = findTabInTree(root, tab.id);
    if (existing) {
      const newRoot = updateLeaf(root, existing.leaf.groupId, (leaf) => ({
        ...leaf,
        activeTabId: tab.id,
      }));
      set({ root: newRoot, activeGroupId: existing.leaf.groupId });
      return;
    }
    const targetGroupId = groupId ?? activeGroupId;
    const newRoot = updateLeaf(root, targetGroupId, (leaf) => ({
      ...leaf,
      tabs: [...leaf.tabs, tab],
      activeTabId: tab.id,
    }));
    set({ root: newRoot, activeGroupId: targetGroupId });
  },

  closeTab(tabId, groupId) {
    cancelAutoSave(tabId);
    const { root } = get();
    const leaf = (() => {
      const result = findActiveLeaf(root, groupId);
      return result.groupId === groupId ? result : null;
    })();

    if (!leaf) return;

    // Remove the tab from the leaf.
    const remaining = leaf.tabs.filter((t) => t.id !== tabId);

    if (remaining.length === 0) {
      // Collapse the group unless it is the only leaf (root).
      if (root.type === 'leaf') {
        // Root leaf — keep group alive but replace with a fresh draft.
        const fresh = createDefaultTab();
        set({
          root: { ...root, tabs: [fresh], activeTabId: fresh.id },
        });
      } else {
        const newRoot = removeLeaf(root, groupId);
        const firstLeaf = (() => {
          let n: PaneNode = newRoot;
          while (n.type !== 'leaf') n = n.children[0];
          return n;
        })();
        set({ root: newRoot, activeGroupId: firstLeaf.groupId });
      }
      return;
    }

    // Activate the tab just before the closed one, or fall back to the first.
    const closedIdx = leaf.tabs.findIndex((t) => t.id === tabId);
    const nextActive = remaining[Math.max(0, closedIdx - 1)].id;

    const newRoot = updateLeaf(root, groupId, () => ({
      ...leaf,
      tabs: remaining,
      activeTabId: nextActive,
    }));
    set({ root: newRoot });
  },

  setActiveTab(tabId, groupId) {
    const { root } = get();
    const newRoot = updateLeaf(root, groupId, (leaf) => ({
      ...leaf,
      activeTabId: tabId,
    }));
    set({ root: newRoot, activeGroupId: groupId });
  },

  moveTab(tabId, fromGroupId, toGroupId) {
    if (fromGroupId === toGroupId) return;
    const { root } = get();

    // Find the tab in the source group.
    const found = findTabInTree(root, tabId);
    if (!found || found.leaf.groupId !== fromGroupId) return;

    const { tab } = found;
    const sourceLeaf = found.leaf;
    const remaining = sourceLeaf.tabs.filter((t) => t.id !== tabId);

    let newRoot: PaneNode;

    if (remaining.length === 0 && root.type !== 'leaf') {
      // Collapse the source group, then add tab to the destination.
      newRoot = removeLeaf(root, fromGroupId);
    } else if (remaining.length === 0) {
      // Source is root leaf — keep it but swap in a fresh draft.
      const fresh = createDefaultTab();
      newRoot = updateLeaf(root, fromGroupId, (leaf) => ({
        ...leaf,
        tabs: [fresh],
        activeTabId: fresh.id,
      }));
    } else {
      // Activate previous tab in source, then add to destination below.
      const closedIdx = sourceLeaf.tabs.findIndex((t) => t.id === tabId);
      const nextActive = remaining[Math.max(0, closedIdx - 1)].id;
      newRoot = updateLeaf(root, fromGroupId, (leaf) => ({
        ...leaf,
        tabs: remaining,
        activeTabId: nextActive,
      }));
    }

    // Add tab to destination group.
    newRoot = updateLeaf(newRoot, toGroupId, (leaf) => ({
      ...leaf,
      tabs: [...leaf.tabs, tab],
      activeTabId: tab.id,
    }));

    set({ root: newRoot, activeGroupId: toGroupId });
  },

  splitGroup(groupId, direction) {
    const { root } = get();
    const newRoot = splitLeaf(root, groupId, direction);
    set({ root: newRoot });
  },

  resizePane(splitId, sizes) {
    const { root } = get();
    set({ root: updateSplitSizes(root, splitId, sizes) });
  },

  updateRequest(tabId, patch) {
    const { root } = get();
    const newRoot = updateTabInTree(root, tabId, (tab) => {
      const updatedTab = {
        ...tab,
        request: { ...tab.request, ...patch },
        isDirty: true,
      };
      // Auto-save for collection-owned tabs.
      if (tab.source) {
        scheduleAutoSave(
          tabId,
          tab.source.collection,
          tab.source.path,
          tab.title,
          updatedTab.request,
        );
      }
      return updatedTab;
    });
    set({ root: newRoot });
  },

  setResponse(tabId, response) {
    const { root } = get();
    const newRoot = updateTabInTree(root, tabId, (tab) => ({
      ...tab,
      response,
    }));
    set({ root: newRoot });
  },

  markDirty(tabId) {
    const { root } = get();
    set({ root: updateTabInTree(root, tabId, (tab) => ({ ...tab, isDirty: true })) });
  },

  markClean(tabId) {
    const { root } = get();
    set({ root: updateTabInTree(root, tabId, (tab) => ({ ...tab, isDirty: false })) });
  },

  reset() {
    set(buildInitialState());
  },

  updateTabSource(tabId, source) {
    const { root } = get();
    set({
      root: updateTabInTree(root, tabId, (tab) => ({
        ...tab,
        source,
      })),
    });
  },
}));
