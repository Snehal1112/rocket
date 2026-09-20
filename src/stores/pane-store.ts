import { create } from 'zustand';
import { scheduleAutoSave } from '@/lib/auto-save';
import {
  createDefaultLeaf,
  createDefaultRequest,
  findActiveLeaf,
  findTabInTree,
  removeLeaf,
  splitLeaf,
  updateLeaf,
} from '@/lib/pane-utils';
import { executeRunnerEntry } from '@/lib/runner-execute';
import { flattenRunnerEntries } from '@/lib/runner-flatten';
import { getCollection, renameRequest } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import type {
  CollectionSection,
  CollectionTab,
  ContractTab,
  LeafNode,
  PaneNode,
  RequestState,
  RequestTab,
  ResponseState,
  RunnerRequestEntry,
  RunnerTab,
  SplitNode,
  Tab,
  WorkspaceTab,
  WorkspaceTabSection,
} from '@/types/pane-types';
import { isRequestTab, isRunnerTab } from '@/types/pane-types';

// Recursively finds a tab by id and applies an updater function to it.
function updateTabInTree(node: PaneNode, tabId: string, updater: (tab: Tab) => Tab): PaneNode {
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
function updateSplitSizes(node: PaneNode, splitId: string, sizes: [number, number]): PaneNode {
  if (node.type === 'leaf') return node;
  if (node.id === splitId) return { ...node, sizes } satisfies SplitNode;
  const left = updateSplitSizes(node.children[0], splitId, sizes);
  const right = updateSplitSizes(node.children[1], splitId, sizes);
  if (left === node.children[0] && right === node.children[1]) return node;
  return { ...node, children: [left, right] } satisfies SplitNode;
}

// Builds the initial store state with one empty leaf.
function buildInitialState(): Pick<
  PaneState,
  'root' | 'activeGroupId' | 'activeCollection' | 'collectionTabState'
> {
  const leaf = createDefaultLeaf();
  return {
    root: leaf,
    activeGroupId: leaf.groupId,
    activeCollection: null,
    collectionTabState: {},
  };
}

export interface PaneState {
  root: PaneNode;
  activeGroupId: string;
  activeCollection: string | null;
  collectionTabState: Record<string, { tabs: Tab[]; activeTabId: string }>;

  // Tab actions.
  openTab: (tab: Tab, groupId?: string) => void;
  openEphemeralTab: (requestType?: 'http' | 'graphql' | 'grpc' | 'websocket') => void;
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

  // Collection-keyed tab state actions.
  setActiveCollection: (name: string) => void;
  switchCollection: (name: string) => void;
  getOpenTabCount: (collection: string) => number;

  // Workspace tabs.
  openWorkspaceTabs: (workspaceId: string, section?: WorkspaceTabSection) => void;
  isWorkspaceMode: () => boolean;

  // Contract tab.
  openContractTab: (collectionName: string, collectionRoot: string) => void;

  // Focus tracking.
  setActiveGroup: (groupId: string) => void;

  // Utility.
  reset: () => void;
  closeAll: () => void;

  updateTabSource: (tabId: string, source: { collection: string; path: string }) => void;
  updateTabTitle: (tabId: string, title: string) => void;
  updateCollectionSection: (tabId: string, section: CollectionSection) => void;
  /** Opens or focuses the collection tab for `collection` and navigates to `section`.
   *  Returns false if no collection tab is currently open for that collection. */
  openCollectionTab: (collection: string, section: CollectionSection) => boolean;

  // Runner tab.
  openRunnerTab: (collectionName: string | null, folderPath?: string) => Promise<void>;
  toggleRunnerEntry: (tabId: string, requestPath: string) => void;
  startRun: (tabId: string) => Promise<void>;
  stopRun: (tabId: string) => void;
  rerunAll: (tabId: string) => Promise<void>;
}

// Monotonic source for RunnerTab.runId. Module-level (not per-tab) is
// intentional: any two runs started anywhere in the session, even on
// different tabs, get distinct ids, so a stale loop can never coincide with
// a fresh one by chance.
let runIdCounter = 0;

export const usePaneStore = create<PaneState>((set, get) => ({
  ...buildInitialState(),

  openTab(tab, groupId) {
    // Exit workspace mode when a non-workspace tab is opened.
    if (tab.tabType !== 'workspace' && get().isWorkspaceMode()) {
      get().closeAll();
    }

    // Derive the collection name from the tab so the dropdown updates.
    const collectionName =
      tab.tabType === 'collection'
        ? (tab as CollectionTab).collectionName
        : tab.tabType === 'contract'
          ? (tab as ContractTab).collectionName
          : (tab.source?.collection ?? null);
    if (collectionName && collectionName !== get().activeCollection) {
      get().switchCollection(collectionName);
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

  openEphemeralTab(requestType = 'http' as const) {
    const tab: RequestTab = {
      id: crypto.randomUUID(),
      title: 'Untitled',
      tabType: 'request',
      request: { ...createDefaultRequest(), requestType },
      response: null,
      isDirty: false,
    };
    get().openTab(tab);
  },

  closeTab(tabId, groupId) {
    // Save the tab before closing if it's dirty.
    const { root } = get();
    const found = findTabInTree(root, tabId);
    if (found?.tab.isDirty && found.tab.source && isRequestTab(found.tab)) {
      scheduleAutoSave(
        tabId,
        found.tab.source.collection,
        found.tab.source.path,
        found.tab.title,
        found.tab.request,
      );
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
        scheduleAutoSave(
          prevTab.id,
          prevTab.source.collection,
          prevTab.source.path,
          prevTab.title,
          prevTab.request,
        );
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

  setActiveGroup(groupId) {
    set({ activeGroupId: groupId });
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

  openContractTab(collectionName, collectionRoot) {
    const id = `contract:${collectionRoot}`;
    const tab: ContractTab = {
      id,
      title: `Contracts — ${collectionName}`,
      tabType: 'contract',
      collectionName,
      collectionRoot,
      isDirty: false,
    };
    get().openTab(tab);
  },

  async openRunnerTab(collectionName, folderPath) {
    let requests: RunnerRequestEntry[] = [];
    if (collectionName) {
      try {
        const collection = await getCollection(collectionName);
        requests = flattenRunnerEntries(collection, folderPath);
      } catch (err) {
        console.error('[pane-store] openRunnerTab: failed to load collection', err);
      }
    }
    const label = folderPath ? folderPath.split('/').pop() : collectionName;
    const tab: RunnerTab = {
      id: crypto.randomUUID(),
      title: label ? `Run: ${label}` : 'Runner',
      isDirty: false,
      tabType: 'runner',
      collectionName,
      folderPath,
      runState: 'idle',
      requests,
    };
    get().openTab(tab);
  },

  toggleRunnerEntry(tabId, requestPath) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRunnerTab(tab) || tab.runState === 'running') return tab;
        return {
          ...tab,
          requests: tab.requests.map((e) =>
            e.requestPath === requestPath ? { ...e, included: !e.included } : e,
          ),
        };
      }),
    });
  },

  async startRun(tabId) {
    const found = findTabInTree(get().root, tabId);
    if (!found || !isRunnerTab(found.tab) || !found.tab.collectionName) return;
    const collectionName = found.tab.collectionName;
    const entries = found.tab.requests;

    // Assigning a fresh id here, and making every subsequent write in this
    // run conditional on it still matching, is what makes it safe to press
    // Stop and then immediately Re-run: rerunAll's own startRun call assigns
    // a new id, so this (now-superseded) loop's writes below become no-ops
    // instead of racing the new run and overwriting its results.
    const myRunId = ++runIdCounter;

    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isRunnerTab(tab) ? { ...tab, runState: 'running', runId: myRunId } : tab,
      ),
    });

    const environmentName = useEnvStore.getState().activeEnvId ?? undefined;

    for (const entry of entries) {
      const live = findTabInTree(get().root, tabId);
      if (
        !live ||
        !isRunnerTab(live.tab) ||
        live.tab.runId !== myRunId ||
        live.tab.runState !== 'running'
      )
        break;
      if (!entry.included) continue;

      set({
        root: updateTabInTree(get().root, tabId, (tab) => {
          if (!isRunnerTab(tab) || tab.runId !== myRunId) return tab;
          return {
            ...tab,
            requests: tab.requests.map((e) =>
              e.requestPath === entry.requestPath ? { ...e, status: 'running' } : e,
            ),
          };
        }),
      });

      const outcome = await executeRunnerEntry(
        collectionName,
        entry.requestPath,
        entry.request,
        environmentName,
      );

      set({
        root: updateTabInTree(get().root, tabId, (tab) => {
          if (!isRunnerTab(tab) || tab.runId !== myRunId) return tab;
          return {
            ...tab,
            requests: tab.requests.map((e) =>
              e.requestPath === entry.requestPath
                ? { ...e, status: outcome.status, result: outcome.result, error: outcome.error }
                : e,
            ),
          };
        }),
      });
    }

    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRunnerTab(tab) || tab.runId !== myRunId) return tab;
        const requests = tab.requests.map((e) =>
          e.status === 'pending' ? { ...e, status: 'skipped' as const } : e,
        );
        return { ...tab, runState: tab.runState === 'stopped' ? 'stopped' : 'done', requests };
      }),
    });
  },

  stopRun(tabId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isRunnerTab(tab) && tab.runState === 'running' ? { ...tab, runState: 'stopped' } : tab,
      ),
    });
  },

  async rerunAll(tabId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRunnerTab(tab)) return tab;
        return {
          ...tab,
          runState: 'idle',
          requests: tab.requests.map((e) => ({
            ...e,
            status: 'pending' as const,
            result: undefined,
            error: undefined,
          })),
        };
      }),
    });
    await get().startRun(tabId);
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

    // Sync active collection into env-store so useEnvironments() queries fire.
    useEnvStore.getState().setActiveCollection(name);

    // Restore active env selection from localStorage.
    const stored = localStorage.getItem(`rocket-api:active-env:${name}`);
    useEnvStore.getState().setActiveEnvId(stored ?? null);
  },

  getOpenTabCount(collection) {
    const { activeCollection, collectionTabState, root, activeGroupId } = get();
    if (collection === activeCollection) {
      const leaf = findActiveLeaf(root, activeGroupId);
      return leaf.tabs.length;
    }
    return collectionTabState[collection]?.tabs.length ?? 0;
  },

  openWorkspaceTabs(workspaceId, section) {
    const { root, activeGroupId, activeCollection, collectionTabState } = get();

    // Snapshot the current collection's tabs so they survive the switch.
    const updatedState = { ...collectionTabState };
    if (activeCollection) {
      const activeLeaf = findActiveLeaf(root, activeGroupId);
      updatedState[activeCollection] = {
        tabs: activeLeaf.tabs,
        activeTabId: activeLeaf.activeTabId,
      };
    }

    // Flush dirty tabs before resetting the pane tree.
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

    // Build workspace tabs.
    const sections: WorkspaceTabSection[] = ['overview', 'environments', 'git', 'audit'];
    const tabs: WorkspaceTab[] = sections.map((s) => ({
      id: `workspace:${workspaceId}:${s}`,
      title:
        s === 'git'
          ? 'Git UI'
          : s === 'audit'
            ? 'Audit Log'
            : s.charAt(0).toUpperCase() + s.slice(1),
      isDirty: false,
      tabType: 'workspace',
      workspaceId,
      activeSection: s,
    }));

    // Reset pane tree to a single leaf with workspace tabs.
    const leaf = createDefaultLeaf();
    const targetTab = section
      ? (tabs.find((t) => t.activeSection === section) ?? tabs[0])
      : tabs[0];
    const newRoot = updateLeaf(leaf, leaf.groupId, (l) => ({
      ...l,
      tabs,
      activeTabId: targetTab.id,
    }));
    set({
      root: newRoot,
      activeGroupId: leaf.groupId,
      activeCollection: null,
      collectionTabState: updatedState,
    });
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
      renameRequest(found.tab.source.collection, found.tab.source.path, title).catch((err) =>
        console.error('[pane-store] rename failed:', err),
      );
    }
  },

  openCollectionTab(collection, section) {
    const { root } = get();

    // Walk the pane tree to find an open tab for this collection.
    const findTarget = (node: PaneNode): { groupId: string; tabId: string } | null => {
      if (node.type === 'leaf') {
        const found = node.tabs.find(
          (t) => t.tabType === 'collection' && (t as CollectionTab).collectionName === collection,
        );
        return found ? { groupId: node.groupId, tabId: found.id } : null;
      }
      return findTarget(node.children[0]) ?? findTarget(node.children[1]);
    };

    // Only searches the live pane tree. Tabs snapshotted in collectionTabState
    // (from a previous collection switch) are not considered "open" here.
    const target = findTarget(root);
    if (!target) return false;

    // Activate the tab and navigate to the requested section.
    get().updateCollectionSection(target.tabId, section);
    // Re-read root after updateCollectionSection has written its own set().
    const updatedRoot = get().root;
    const newRoot = updateLeaf(updatedRoot, target.groupId, (l) => ({
      ...l,
      activeTabId: target.tabId,
    }));
    set({ root: newRoot, activeGroupId: target.groupId });
    // Note: activeCollection is not updated here. This is safe because CollectionTabs
    // are only present in the tree when their collection is already active.
    return true;
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
