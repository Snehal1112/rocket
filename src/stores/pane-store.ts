import { create } from 'zustand';
import type { PaneNode, Tab, ResponseState, RequestState, LeafNode, SplitNode, CollectionSection, DiffTab, DiffState, ConflictTab, ConflictState, WorkspaceTab, WorkspaceTabSection } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';
import { scheduleAutoSave } from '@/lib/auto-save';
import { renameRequest } from '@/lib/tauri-api';
import {
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

// Builds the initial store state with one empty leaf.
function buildInitialState(): Pick<PaneState, 'root' | 'activeGroupId' | 'activeCollection' | 'collectionTabState'> {
  const leaf = createDefaultLeaf();
  return { root: leaf, activeGroupId: leaf.groupId, activeCollection: null, collectionTabState: {} };
}

export interface PaneState {
  root: PaneNode;
  activeGroupId: string;
  activeCollection: string | null;
  collectionTabState: Record<string, { tabs: Tab[]; activeTabId: string }>;

  // Tab actions.
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

  // Diff tab action.
  openDiffTab: (diffState: DiffState) => void;

  // Conflict tab action.
  openConflictTab: (conflictState: ConflictState) => void;

  // Collection-keyed tab state actions.
  setActiveCollection: (name: string) => void;
  switchCollection: (name: string) => void;
  getOpenTabCount: (collection: string) => number;

  // Workspace tabs.
  openWorkspaceTabs: (workspaceId: string, isDefault: boolean) => void;
  isWorkspaceMode: () => boolean;

  // Utility.
  reset: () => void;
  closeAll: () => void;

  updateTabSource: (tabId: string, source: { collection: string; path: string }) => void;
  updateTabTitle: (tabId: string, title: string) => void;
  updateCollectionSection: (tabId: string, section: CollectionSection) => void;
}

export const usePaneStore = create<PaneState>((set, get) => ({
  ...buildInitialState(),

  openTab(tab, groupId) {
    // Exit workspace mode when a non-workspace tab is opened.
    if (tab.tabType !== 'workspace' && get().isWorkspaceMode()) {
      get().closeAll();
    }
    const { root, activeGroupId } = get();
    // Match by uid — if the tab is already open anywhere, activate it.
    const existing = findTabInTree(root, tab.id);
    if (existing) {
      const newRoot = updateLeaf(root, existing.leaf.groupId, (leaf) => ({
        ...leaf,
        activeTabId: existing.tab.id,
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
    // Save the tab before closing if it's dirty.
    const { root } = get();
    const found = findTabInTree(root, tabId);
    if (found?.tab.isDirty && found.tab.source && isRequestTab(found.tab)) {
      scheduleAutoSave(tabId, found.tab.source.collection, found.tab.source.path, found.tab.title, found.tab.request);
    }
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
        // Root leaf — show empty state (no tabs).
        set({
          root: { ...root, tabs: [], activeTabId: '' },
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
    // Save the previously active tab if it's dirty.
    const leaf = findActiveLeaf(root, groupId);
    if (leaf.groupId === groupId) {
      const prevTab = leaf.tabs.find((t) => t.id === leaf.activeTabId);
      if (prevTab?.isDirty && prevTab.source && isRequestTab(prevTab)) {
        scheduleAutoSave(prevTab.id, prevTab.source.collection, prevTab.source.path, prevTab.title, prevTab.request);
      }
    }
    const newRoot = updateLeaf(root, groupId, (l) => ({
      ...l,
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
      // Source is root leaf — show empty state.
      newRoot = updateLeaf(root, fromGroupId, (leaf) => ({
        ...leaf,
        tabs: [],
        activeTabId: '',
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
      if (!isRequestTab(tab)) return tab;
      return { ...tab, request: { ...tab.request, ...patch }, isDirty: true };
    });
    set({ root: newRoot });
  },

  setResponse(tabId, response) {
    const { root } = get();
    const newRoot = updateTabInTree(root, tabId, (tab) => {
      if (!isRequestTab(tab)) return tab;
      return { ...tab, response };
    });
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

  openDiffTab(diffState) {
    const tabId = `diff:${diffState.collectionPath}/${diffState.filePath}:${diffState.isStaged ? 'staged' : 'working'}`;
    const tab: DiffTab = {
      id: tabId,
      title: `${diffState.filePath} (${diffState.isStaged ? 'Staged' : 'Working'})`,
      isDirty: false,
      tabType: 'diff',
      diffState,
    };
    get().openTab(tab);
  },

  openConflictTab(conflictState) {
    const tabId = `conflict:${conflictState.collectionPath}/${conflictState.filePath}`;
    const tab: ConflictTab = {
      id: tabId,
      title: `${conflictState.filePath} (Conflict)`,
      isDirty: false,
      tabType: 'conflict',
      conflictState,
    };
    get().openTab(tab);
  },

  setActiveCollection(name) {
    set({ activeCollection: name });
  },

  switchCollection(name) {
    const { root, activeGroupId, activeCollection, collectionTabState } = get();
    // No-op if already on this collection.
    if (name === activeCollection) return;
    // Only the active leaf is snapshotted. In split-pane layouts, tabs in
    // non-active panes are not included. This is an accepted design limitation
    // for the current feature scope.
    const activeLeaf = findActiveLeaf(root, activeGroupId);

    // Snapshot current collection's tabs into the keyed state map.
    const updatedState = { ...collectionTabState };
    if (activeCollection) {
      updatedState[activeCollection] = {
        tabs: activeLeaf.tabs,
        activeTabId: activeLeaf.activeTabId,
      };
    }

    // Restore target collection's tabs (or empty if never visited).
    const targetState = updatedState[name];
    const restoredTabs = targetState?.tabs ?? [];
    const restoredActiveTabId = targetState?.activeTabId ?? '';

    const newRoot = updateLeaf(root, activeGroupId, (leaf) => ({
      ...leaf,
      tabs: restoredTabs,
      activeTabId: restoredActiveTabId,
    }));

    set({
      root: newRoot,
      activeCollection: name,
      collectionTabState: updatedState,
    });
  },

  getOpenTabCount(collection) {
    const { activeCollection, collectionTabState, root, activeGroupId } = get();
    if (collection === activeCollection) {
      const leaf = findActiveLeaf(root, activeGroupId);
      return leaf.tabs.length;
    }
    return collectionTabState[collection]?.tabs.length ?? 0;
  },

  openWorkspaceTabs(workspaceId, isDefault) {
    get().closeAll();
    const sections: WorkspaceTabSection[] = ['overview', 'environments'];
    if (!isDefault) sections.push('git');

    const tabs: WorkspaceTab[] = sections.map((section) => ({
      id: `workspace:${workspaceId}:${section}`,
      title: section.charAt(0).toUpperCase() + section.slice(1),
      isDirty: false,
      tabType: 'workspace',
      workspaceId,
      activeSection: section,
    }));

    const overviewId = tabs[0].id;
    const { root } = get();
    // Get the root leaf after closeAll() which resets to a single leaf.
    const rootLeaf = root as LeafNode;
    const newRoot = updateLeaf(root, rootLeaf.groupId, (leaf) => ({
      ...leaf,
      tabs,
      activeTabId: overviewId,
    }));
    set({ root: newRoot, activeGroupId: rootLeaf.groupId });
  },

  isWorkspaceMode() {
    const hasWorkspaceTab = (node: PaneNode): boolean => {
      if (node.type === 'leaf') {
        return node.tabs.some((t) => t.tabType === 'workspace');
      }
      return hasWorkspaceTab(node.children[0]) || hasWorkspaceTab(node.children[1]);
    };
    return hasWorkspaceTab(get().root);
  },

  reset() {
    set(buildInitialState());
  },

  closeAll() {
    const { root } = get();
    const flush = (node: PaneNode): void => {
      if (node.type === 'leaf') {
        for (const tab of node.tabs) {
          if (tab.isDirty && tab.source && isRequestTab(tab)) {
            scheduleAutoSave(
              tab.id,
              tab.source.collection,
              tab.source.path,
              tab.title,
              tab.request,
            );
          }
        }
      } else {
        flush(node.children[0]);
        flush(node.children[1]);
      }
    };
    flush(root);
    get().reset();
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

  updateTabTitle(tabId, title) {
    const { root } = get();
    // Find the tab to check if it has a collection source.
    const found = findTabInTree(root, tabId);
    // Skip if the title hasn't actually changed.
    if (found?.tab.title === title) return;
    set({
      root: updateTabInTree(root, tabId, (tab) => ({
        ...tab,
        title,
        // source.path stays unchanged — the filename on disk doesn't change,
        // only the name field inside the JSON is updated.
      })),
    });
    // Persist rename to disk. The file watcher detects the write and
    // emits collection-changed, which refreshes the sidebar automatically.
    if (found?.tab.source) {
      renameRequest(found.tab.source!.collection, found.tab.source!.path, title)
        .catch((err) => console.error('[pane-store] rename failed:', err));
    }
  },

  updateCollectionSection(tabId, section) {
    const { root } = get();
    set({
      root: updateTabInTree(root, tabId, (tab) => {
        if (tab.tabType !== 'collection') return tab;
        return { ...tab, activeSection: section };
      }),
    });
  },
}));
