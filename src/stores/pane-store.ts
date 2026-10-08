import { create } from 'zustand';
import { scheduleAutoSave } from '@/lib/auto-save';
import { mergeRunResult } from '@/lib/flow-run-result';
import {
  collectAllTabs,
  createDefaultLeaf,
  createDefaultRequestFor,
  findActiveLeaf,
  findFolderTab,
  findFolderTabsWithin,
  findScriptTab,
  findScriptTabsWithin,
  findTabInTree,
  removeLeaf,
  splitLeaf,
  updateLeaf,
} from '@/lib/pane-utils';
import { executeRunnerEntry } from '@/lib/runner-execute';
import { flattenRunnerEntries } from '@/lib/runner-flatten';
import { releaseStreamingTab } from '@/lib/streaming-release';
import {
  endAgentSession,
  type Flow,
  type FlowEdge,
  type FlowNode,
  type FlowNodeStatus,
  getCollection,
  getFlow,
  readScriptFile,
  renameRequest,
} from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import type {
  ChatMessage,
  CollectionSection,
  CollectionTab,
  ContractTab,
  FlowLastRun,
  FlowNodeDetail,
  FlowTab,
  FolderSection,
  FolderTab,
  LeafNode,
  PaneNode,
  RequestState,
  RequestTab,
  ResponseState,
  RunnerRequestEntry,
  RunnerTab,
  ScriptTab,
  SplitNode,
  Tab,
  WorkspaceTab,
  WorkspaceTabSection,
} from '@/types/pane-types';
import { isFlowTab, isFolderTab, isRequestTab, isRunnerTab, isScriptTab } from '@/types/pane-types';

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

type CollectionTabState = Record<string, { tabs: Tab[]; activeTabId: string }>;

// Applies an updater to one tab by id inside every collection snapshot.
// Returns the same object when no snapshot holds that tab.
function updateTabInSnapshots(
  state: CollectionTabState,
  tabId: string,
  updater: (tab: Tab) => Tab,
): CollectionTabState {
  let changed = false;
  const next: CollectionTabState = {};
  for (const [key, entry] of Object.entries(state)) {
    const idx = entry.tabs.findIndex((t) => t.id === tabId);
    if (idx === -1) {
      next[key] = entry;
      continue;
    }
    const tabs = entry.tabs.slice();
    tabs[idx] = updater(tabs[idx]);
    next[key] = { ...entry, tabs };
    changed = true;
  }
  return changed ? next : state;
}

// Finds a tab by id inside the collection snapshots.
function findTabInSnapshots(state: CollectionTabState, tabId: string): Tab | undefined {
  for (const entry of Object.values(state)) {
    const tab = entry.tabs.find((t) => t.id === tabId);
    if (tab) return tab;
  }
  return undefined;
}

// Applies an updater to one tab by id in the live pane tree and in every
// collection snapshot. Agent session events can arrive while the owning tab
// is parked in a snapshot after a collection switch, so the agent session
// actions must reach it there too.
function updateTabEverywhere(
  state: Pick<PaneState, 'root' | 'collectionTabState'>,
  tabId: string,
  updater: (tab: Tab) => Tab,
): Pick<PaneState, 'root' | 'collectionTabState'> {
  return {
    root: updateTabInTree(state.root, tabId, updater),
    collectionTabState: updateTabInSnapshots(state.collectionTabState, tabId, updater),
  };
}

// Best-effort backend cleanup for a tab that is about to be discarded.
// Subproject B only sweeps sessions on whole-app exit. A session still
// mid-handshake has no real session id yet, so there is nothing to end.
function endSessionIfActive(tab: Tab): void {
  releaseStreamingTab(tab);
  if (isRequestTab(tab) && tab.agentSession?.status === 'active') {
    Promise.resolve(endAgentSession(tab.agentSession.sessionId)).catch((err) => {
      console.error('[pane-store] failed to end agent session', err);
    });
  }
}

// Ends every active agent session among tabs that are about to be discarded.
// The same session can appear twice (a live tab plus a stale snapshot copy),
// so each session id is ended only once.
function endActiveSessions(tabs: Tab[]): void {
  const seen = new Set<string>();
  for (const tab of tabs) {
    // Stream sessions are keyed by tab id, so the same tab id is released once.
    if (
      isRequestTab(tab) &&
      (tab.request.requestType === 'websocket' ||
        tab.request.requestType === 'graphql' ||
        tab.request.requestType === 'grpc') &&
      !seen.has(tab.id)
    ) {
      seen.add(tab.id);
      releaseStreamingTab(tab);
    }
    if (!isRequestTab(tab) || tab.agentSession?.status !== 'active') continue;
    if (seen.has(tab.agentSession.sessionId)) continue;
    seen.add(tab.agentSession.sessionId);
    endSessionIfActive(tab);
  }
}

// Clears the in-memory Auth tokens of every flow shown by a dropped tab, unless
// a remaining tab still shows the same flow.
function clearFlowAuthForDroppedTabs(dropped: Tab[], remaining: Tab[] = []): void {
  const shown = (t: Tab) => (isFlowTab(t) && t.collectionName && t.flowName ? t : null);
  const kept = new Set(
    remaining.flatMap((t) => {
      const f = shown(t);
      return f ? [`${f.collectionName}\u0000${f.flowName}`] : [];
    }),
  );
  for (const tab of dropped) {
    const f = shown(tab);
    if (!f?.collectionName || !f.flowName) continue;
    if (kept.has(`${f.collectionName}\u0000${f.flowName}`)) continue;
    useFlowAuthStore.getState().clearFlow(f.collectionName, f.flowName);
  }
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
  collectionTabState: CollectionTabState;

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

  // Agent chat session actions.
  beginAgentSession: (tabId: string, agentConfigId: string) => void;
  /** Moves a 'starting' session to 'active'. Returns false without changing
   *  state when the tab is gone or its session is no longer 'starting', so the
   *  caller knows it must end the backend session itself. */
  activateAgentSession: (tabId: string, sessionId: string) => boolean;
  appendAgentChatMessage: (tabId: string, message: ChatMessage) => void;
  appendAgentChatChunk: (tabId: string, messageId: string, text: string) => void;
  completeAgentChatMessage: (tabId: string, messageId: string) => void;
  failAgentChatMessage: (tabId: string, messageId: string, error: string) => void;
  markAgentSessionEnded: (tabId: string) => void;
  clearAgentSession: (tabId: string) => void;

  // Collection-keyed tab state actions.
  setActiveCollection: (name: string) => void;
  switchCollection: (name: string) => void;
  getOpenTabCount: (collection: string) => number;

  // Workspace tabs.
  openWorkspaceTabs: (workspaceId: string, section?: WorkspaceTabSection) => void;
  isWorkspaceMode: () => boolean;

  // Contract tab.
  openContractTab: (collectionName: string, collectionRoot: string) => void;
  openScriptTab: (collectionName: string, path: string) => Promise<void>;
  updateScriptContent: (tabId: string, content: string) => void;
  markScriptSaved: (tabId: string, content: string) => void;
  renameScriptTabs: (collection: string, oldPath: string, newPath: string) => void;
  /** Opens or focuses the settings tab of a folder. Returns true when an open tab was reused. */
  openFolderTab: (collection: string, folderPath: string, section?: FolderSection) => boolean;
  updateFolderSection: (tabId: string, section: FolderSection) => void;
  /** Retargets open folder tabs when a folder is renamed, matching whole path segments. */
  renameFolderTabs: (collection: string, oldPath: string, newPath: string) => void;

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

  // Flow tab.
  openFlowTab: (collectionName: string | null, flowName?: string) => Promise<void>;
  updateFlowNodes: (tabId: string, nodes: FlowNode[]) => void;
  updateFlowEdges: (tabId: string, edges: FlowEdge[]) => void;
  /** Replaces nodes and edges together, so dependent edits land in one update. */
  updateFlowGraph: (tabId: string, nodes: FlowNode[], edges: FlowEdge[]) => void;
  setFlowCallbackHost: (tabId: string, host: string | null) => void;
  patchFlowNodeStatus: (
    tabId: string,
    nodeId: string,
    status: FlowNodeStatus,
    detail?: FlowNodeDetail,
  ) => void;
  patchFlowNodeProgress: (tabId: string, nodeId: string, message: string) => void;
  setFlowRunState: (tabId: string, runState: 'idle' | 'running' | 'done', runId?: string) => void;
  /** Stores the finished run's result. Pass undefined to clear it. */
  setFlowRunResult: (tabId: string, lastRun: FlowLastRun | undefined) => void;
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
          : tab.tabType === 'folder'
            ? tab.collectionName
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
      request: createDefaultRequestFor(requestType),
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

    // Best-effort session cleanup for the tab being closed.
    if (found) endSessionIfActive(found.tab);

    const leaf = (() => {
      const result = findActiveLeaf(root, groupId);
      return result.groupId === groupId ? result : null;
    })();

    if (!leaf) return;

    // A flow's in-memory Auth tokens go with its last open tab. Another tab
    // of the same flow keeps them.
    if (found) {
      clearFlowAuthForDroppedTabs(
        [found.tab],
        collectAllTabs(root).filter((t) => t.id !== tabId),
      );
    }

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

  beginAgentSession(tabId, agentConfigId) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab)) return tab;
        return {
          ...tab,
          agentSession: { agentConfigId, sessionId: '', status: 'starting', messages: [] },
        };
      }),
    );
  },

  activateAgentSession(tabId, sessionId) {
    const state = get();
    const tab =
      findTabInTree(state.root, tabId)?.tab ?? findTabInSnapshots(state.collectionTabState, tabId);
    if (!tab || !isRequestTab(tab) || tab.agentSession?.status !== 'starting') return false;
    set(
      updateTabEverywhere(state, tabId, (t) => {
        if (!isRequestTab(t) || t.agentSession?.status !== 'starting') return t;
        return { ...t, agentSession: { ...t.agentSession, sessionId, status: 'active' } };
      }),
    );
    return true;
  },

  appendAgentChatMessage(tabId, message) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            messages: [...tab.agentSession.messages, message],
          },
        };
      }),
    );
  },

  appendAgentChatChunk(tabId, messageId, text) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            messages: tab.agentSession.messages.map((m) =>
              m.id === messageId ? { ...m, text: m.text + text } : m,
            ),
          },
        };
      }),
    );
  },

  completeAgentChatMessage(tabId, messageId) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            messages: tab.agentSession.messages.map((m) =>
              m.id === messageId ? { ...m, streaming: false } : m,
            ),
          },
        };
      }),
    );
  },

  failAgentChatMessage(tabId, messageId, error) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        // Ignore late failures for a session that already ended or a reply that already settled.
        if (tab.agentSession.status !== 'active') return tab;
        if (!tab.agentSession.messages.some((m) => m.id === messageId && m.streaming)) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            status: 'error',
            error,
            messages: tab.agentSession.messages.map((m) =>
              m.id === messageId
                ? { ...m, text: `${m.text}\n\nError: ${error}`, streaming: false }
                : m,
            ),
          },
        };
      }),
    );
  },

  markAgentSessionEnded(tabId) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return { ...tab, agentSession: { ...tab.agentSession, status: 'ended' } };
      }),
    );
  },

  clearAgentSession(tabId) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isRequestTab(tab)) return tab;
        return { ...tab, agentSession: undefined };
      }),
    );
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

  async openScriptTab(collectionName, path) {
    // An open tab only needs focusing, so skip reading the file again.
    const existing = findScriptTab(get().root, collectionName, path);
    if (existing) {
      get().openTab(existing.tab);
      return;
    }
    const content = await readScriptFile(collectionName, path);
    // A concurrent call may have opened the same file while the read was in flight.
    const raced = findScriptTab(get().root, collectionName, path);
    if (raced) {
      get().openTab(raced.tab);
      return;
    }
    const tab: ScriptTab = {
      id: `script:${crypto.randomUUID()}`,
      title: path.split('/').pop() ?? path,
      tabType: 'script',
      collectionName,
      scriptPath: path,
      content,
      savedContent: content,
      isDirty: false,
      source: { collection: collectionName, path },
    };
    get().openTab(tab);
  },

  updateScriptContent(tabId, content) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isScriptTab(tab)) return tab;
        return { ...tab, content, isDirty: content !== tab.savedContent };
      }),
    );
  },

  markScriptSaved(tabId, content) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isScriptTab(tab)) return tab;
        return { ...tab, savedContent: content, isDirty: tab.content !== content };
      }),
    );
  },

  renameScriptTabs(collection, oldPath, newPath) {
    // Matches the file itself or any script below a renamed folder, by whole segments.
    // Matching by tab id keeps the id stable, so panes keep their active tab.
    const tabs = findScriptTabsWithin(get().root, collection, oldPath);
    if (tabs.length === 0) return;
    let next = get();
    for (const found of tabs) {
      const target = `${newPath}${found.scriptPath.slice(oldPath.length)}`;
      next = {
        ...next,
        ...updateTabEverywhere(next, found.id, (tab) => {
          if (!isScriptTab(tab)) return tab;
          return {
            ...tab,
            scriptPath: target,
            title: target.split('/').pop() ?? target,
            source: { collection, path: target },
          };
        }),
      };
    }
    set({ root: next.root, collectionTabState: next.collectionTabState });
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
        entry.graphql,
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

  async openFlowTab(collectionName, flowName) {
    let nodes: FlowNode[] = [];
    let edges: FlowEdge[] = [];
    let callbackHost: string | null = null;
    let resolvedFlowName: string | null = flowName ?? null;
    if (collectionName && flowName) {
      try {
        const flow: Flow = await getFlow(collectionName, flowName);
        nodes = flow.nodes;
        edges = flow.edges;
        callbackHost = flow.callbackHost ?? null;
      } catch (err) {
        console.error('[pane-store] openFlowTab: failed to load flow', err);
        resolvedFlowName = null;
      }
    }
    const tab: FlowTab = {
      id: crypto.randomUUID(),
      title: resolvedFlowName ? `Flow: ${resolvedFlowName}` : 'Flow',
      isDirty: false,
      tabType: 'flow',
      collectionName,
      flowName: resolvedFlowName,
      nodes,
      edges,
      callbackHost,
      nodeStatus: {},
      runState: 'idle',
    };
    get().openTab(tab);
  },

  updateFlowNodes(tabId, nodes) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, nodes, isDirty: true } : tab,
      ),
    });
  },

  updateFlowEdges(tabId, edges) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, edges, isDirty: true } : tab,
      ),
    });
  },

  updateFlowGraph(tabId, nodes, edges) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, nodes, edges, isDirty: true } : tab,
      ),
    });
  },

  setFlowCallbackHost(tabId, host) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, callbackHost: host, isDirty: true } : tab,
      ),
    });
  },

  patchFlowNodeStatus(tabId, nodeId, status, detail) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (!tab.nodes.some((n) => n.id === nodeId)) return tab;
        return {
          ...tab,
          nodeStatus: { ...tab.nodeStatus, [nodeId]: status },
          nodeDetail: detail ? { ...tab.nodeDetail, [nodeId]: detail } : tab.nodeDetail,
        };
      }),
    });
  },

  // Merges progress into the node's detail and leaves its status alone. The
  // next status patch with a detail replaces the detail, which clears it.
  patchFlowNodeProgress(tabId, nodeId, message) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (!tab.nodes.some((n) => n.id === nodeId)) return tab;
        const previous = tab.nodeDetail?.[nodeId];
        return {
          ...tab,
          nodeDetail: { ...tab.nodeDetail, [nodeId]: { ...previous, progress: message } },
        };
      }),
    });
  },

  setFlowRunState(tabId, runState, runId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        // A new run starts from a clean canvas. Otherwise the last run's
        // results stay on nodes this run skips or never reaches.
        if (runState === 'running') {
          return { ...tab, runState, runId, nodeStatus: {}, nodeDetail: {}, lastRun: undefined };
        }
        return { ...tab, runState, runId };
      }),
    });
  },

  setFlowRunResult(tabId, lastRun) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (!lastRun) return { ...tab, lastRun: undefined };
        // A late result from an older run must not show during a newer run.
        if (tab.runState === 'running' && tab.runId !== undefined && tab.runId !== lastRun.runId) {
          return tab;
        }
        return { ...tab, lastRun: mergeRunResult(tab.lastRun, lastRun) };
      }),
    });
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
    } else {
      // With no active collection there is no snapshot to keep the active
      // leaf's tabs, so they are dropped. End their agent sessions first.
      endActiveSessions(activeLeaf.tabs);
      clearFlowAuthForDroppedTabs(activeLeaf.tabs);
    }

    // Restore target collection's tabs (or empty if never visited).
    const targetState = updatedState[name];
    const restoredTabs = targetState?.tabs ?? [];
    const restoredActiveTabId = targetState?.activeTabId ?? '';

    // The restored snapshot is now redundant: its tabs are about to become
    // live in `root`, so keeping it around would leave a stale duplicate
    // that `updateTabEverywhere` could find and "activate" after the tab is
    // closed, orphaning a credentialed backend process. Drop it.
    delete updatedState[name];

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
    const preservedTabIds = new Set<string>();
    if (activeCollection) {
      const activeLeaf = findActiveLeaf(root, activeGroupId);
      updatedState[activeCollection] = {
        tabs: activeLeaf.tabs,
        activeTabId: activeLeaf.activeTabId,
      };
      for (const tab of activeLeaf.tabs) preservedTabIds.add(tab.id);
    }

    // Every other tab in the pane tree is dropped below. End its agent
    // session so no credentialed backend process is left orphaned.
    const droppedTabs = collectAllTabs(root).filter((tab) => !preservedTabIds.has(tab.id));
    endActiveSessions(droppedTabs);
    clearFlowAuthForDroppedTabs(
      droppedTabs,
      collectAllTabs(root).filter((tab) => preservedTabIds.has(tab.id)),
    );

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
    // Every tab in the pane tree and in the collection snapshots is dropped.
    // The snapshot for the active collection is skipped because it is a
    // stale copy: its live tabs are the ones in the pane tree.
    const { root, activeCollection, collectionTabState } = get();
    const snapshotTabs = Object.entries(collectionTabState)
      .filter(([key]) => key !== activeCollection)
      .flatMap(([, entry]) => entry.tabs);
    endActiveSessions([...collectAllTabs(root), ...snapshotTabs]);
    clearFlowAuthForDroppedTabs([...collectAllTabs(root), ...snapshotTabs]);
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
    // Script files are renamed from the sidebar, never through the request rename.
    if (found?.tab.source && !isScriptTab(found.tab)) {
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

  openFolderTab(collection, folderPath, section) {
    // Switch first, so a tab parked in another collection's snapshot is found and not duplicated.
    if (!get().isWorkspaceMode() && get().activeCollection !== collection) {
      get().switchCollection(collection);
    }
    const existing = findFolderTab(get().root, collection, folderPath);
    if (existing) {
      if (section) get().updateFolderSection(existing.tab.id, section);
      // The id is all openTab reads for an open tab, so it only activates it.
      get().openTab(existing.tab);
      return true;
    }
    const tab: FolderTab = {
      id: `folder:${crypto.randomUUID()}`,
      title: folderPath.split('/').pop() ?? folderPath,
      tabType: 'folder',
      collectionName: collection,
      folderPath,
      activeSection: section ?? 'headers',
      isDirty: false,
    };
    get().openTab(tab);
    return false;
  },

  updateFolderSection(tabId, section) {
    set(
      updateTabEverywhere(get(), tabId, (tab) =>
        isFolderTab(tab) ? { ...tab, activeSection: section } : tab,
      ),
    );
  },

  renameFolderTabs(collection, oldPath, newPath) {
    // Matches the folder itself or any folder below it, by whole segments.
    // Matching by tab id keeps the id stable, so panes keep their active tab.
    const tabs = findFolderTabsWithin(get().root, collection, oldPath);
    if (tabs.length === 0) return;
    let next = get();
    for (const found of tabs) {
      const target = `${newPath}${found.folderPath.slice(oldPath.length)}`;
      next = {
        ...next,
        ...updateTabEverywhere(next, found.id, (tab) => {
          if (!isFolderTab(tab)) return tab;
          return { ...tab, folderPath: target, title: target.split('/').pop() ?? target };
        }),
      };
    }
    set({ root: next.root, collectionTabState: next.collectionTabState });
  },
}));
