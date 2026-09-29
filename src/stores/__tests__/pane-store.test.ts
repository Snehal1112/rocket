import { beforeEach, describe, expect, it, vi } from 'vitest';
import { scheduleAutoSave } from '@/lib/auto-save';
import { createDefaultRequest } from '@/lib/pane-utils';
import { type FlowNode, getFlow } from '@/lib/tauri-api';
import type {
  CollectionTab,
  FlowTab,
  LeafNode,
  PaneNode,
  RequestTab,
  ResponseState,
  SplitNode,
  Tab,
} from '@/types/pane-types';
import { isFlowTab, isRequestTab, isRunnerTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({
  scheduleAutoSave: vi.fn(),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

vi.mock('@/lib/runner-execute', () => ({
  executeRunnerEntry: vi.fn(),
}));

// Helper: assert the root is a leaf and return it.
function getLeaf(): LeafNode {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  return root;
}

// Helper: assert the root is a split and return it.
function getSplit(): SplitNode {
  const { root } = usePaneStore.getState();
  if (root.type !== 'split') throw new Error('Expected root to be a split');
  return root;
}

// Helper: create a request tab via openTab and return the leaf.
function makeTab(): RequestTab {
  return {
    id: crypto.randomUUID(),
    title: 'Test Request',
    tabType: 'request' as const,
    request: createDefaultRequest(),
    response: null,
    isDirty: false,
  };
}

function setupWithTab(): LeafNode {
  usePaneStore.getState().openTab(makeTab());
  return getLeaf();
}

// Helper: find the first flow tab anywhere in the pane tree.
function findFirstFlowTab(): FlowTab | undefined {
  function search(node: PaneNode): FlowTab | undefined {
    if (node.type === 'leaf') {
      return node.tabs.find(isFlowTab);
    }
    return search(node.children[0]) ?? search(node.children[1]);
  }
  return search(usePaneStore.getState().root);
}

// Test-only mirror of pane-store.ts's module-private updateTabInTree —
// applies an updater to one tab by id, anywhere in the pane tree.
function updateTabInTreeForTest(
  node: PaneNode,
  tabId: string,
  updater: (tab: Tab) => Tab,
): PaneNode {
  if (node.type === 'leaf') {
    const idx = node.tabs.findIndex((t) => t.id === tabId);
    if (idx === -1) return node;
    const tabs = node.tabs.slice();
    tabs[idx] = updater(tabs[idx]);
    return { ...node, tabs };
  }
  const left = updateTabInTreeForTest(node.children[0], tabId, updater);
  const right = updateTabInTreeForTest(node.children[1], tabId, updater);
  if (left === node.children[0] && right === node.children[1]) return node;
  return { ...node, children: [left, right] };
}

describe('pane-store', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  // ── Initial state ─────────────────────────────────────────────────────────

  it('starts with one empty leaf and no tabs', () => {
    const { root } = usePaneStore.getState();
    expect(root.type).toBe('leaf');
    if (root.type === 'leaf') {
      expect(root.tabs).toHaveLength(0);
      expect(root.activeTabId).toBe('');
    }
  });

  it('initial activeGroupId matches root leaf groupId', () => {
    const leaf = getLeaf();
    expect(usePaneStore.getState().activeGroupId).toBe(leaf.groupId);
  });

  // ── openTab ───────────────────────────────────────────────────────────────

  it('openTab adds a new tab to the active group', () => {
    const { openTab } = usePaneStore.getState();
    const tab = {
      id: crypto.randomUUID(),
      title: 'My Request',
      tabType: 'request' as const,
      request: {
        requestType: 'http' as const,
        method: 'GET' as const,
        url: 'https://example.com',
        pathParams: [],
        queryParams: [],
        headers: [],
        body: { mode: 'none' as const, content: '', formData: [] },
        auth: { authType: 'none' as const },
        settings: {
          verifySsl: true,
          followRedirects: true,
          maxRedirects: 5,
          timeoutMs: 0,
          encodeUrl: true,
        },
        tags: [],
        docs: null,
        assertions: [],
        actions: [],
      },
      response: null,
      isDirty: false,
    };
    openTab(tab);
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(1);
    expect(leaf.activeTabId).toBe(tab.id);
  });

  it('openTab on existing tab just activates it without duplicating', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    const leaf = getLeaf();
    const firstTabId = leaf.tabs[0].id;

    usePaneStore.getState().openTab(leaf.tabs[0]);

    const updated = getLeaf();
    expect(updated.tabs).toHaveLength(2);
    expect(updated.activeTabId).toBe(firstTabId);
  });

  // ── closeTab ──────────────────────────────────────────────────────────────

  it('closeTab removes tab and keeps group if tabs remain', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    const leaf = getLeaf();
    usePaneStore.getState().closeTab(leaf.tabs[0].id, leaf.groupId);
    const updated = getLeaf();
    expect(updated.tabs).toHaveLength(1);
  });

  it('closeTab activates the previous tab after removal', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    const leaf = getLeaf();
    const lastId = leaf.tabs[2].id;
    const prevId = leaf.tabs[1].id;
    usePaneStore.getState().closeTab(lastId, leaf.groupId);
    const updated = getLeaf();
    expect(updated.activeTabId).toBe(prevId);
  });

  it('closeTab on last tab leaves root leaf empty', () => {
    const leaf = setupWithTab();
    usePaneStore.getState().closeTab(leaf.tabs[0].id, leaf.groupId);
    const updated = getLeaf();
    expect(updated.tabs).toHaveLength(0);
    expect(updated.activeTabId).toBe('');
  });

  it('closeTab collapses an empty non-root group', () => {
    setupWithTab();
    const initialLeaf = getLeaf();
    usePaneStore.getState().splitGroup(initialLeaf.groupId, 'horizontal');
    const split = getSplit();
    const rightLeaf = split.children[1] as LeafNode;

    // Right leaf is empty by default from split — add a tab then close it.
    usePaneStore.getState().openTab(makeTab(), rightLeaf.groupId);
    const updatedSplit = getSplit();
    const updatedRight = updatedSplit.children[1] as LeafNode;
    usePaneStore.getState().closeTab(updatedRight.tabs[0].id, updatedRight.groupId);

    const { root } = usePaneStore.getState();
    expect(root.type).toBe('leaf');
  });

  // ── setActiveTab ──────────────────────────────────────────────────────────

  it('setActiveTab updates activeTabId and activeGroupId', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    const leaf = getLeaf();
    const firstId = leaf.tabs[0].id;
    usePaneStore.getState().setActiveTab(firstId, leaf.groupId);
    const updated = getLeaf();
    expect(updated.activeTabId).toBe(firstId);
    expect(usePaneStore.getState().activeGroupId).toBe(leaf.groupId);
  });

  // ── moveTab ───────────────────────────────────────────────────────────────

  it('moveTab transfers a tab from one group to another', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    const leaf = getLeaf();
    usePaneStore.getState().splitGroup(leaf.groupId, 'vertical');
    const split = getSplit();
    const leftLeaf = split.children[0] as LeafNode;
    const rightLeaf = split.children[1] as LeafNode;

    const tabToMove = leftLeaf.tabs[0];
    usePaneStore.getState().moveTab(tabToMove.id, leftLeaf.groupId, rightLeaf.groupId);

    const updatedSplit = getSplit();
    const updatedLeft = updatedSplit.children[0] as LeafNode;
    const updatedRight = updatedSplit.children[1] as LeafNode;

    expect(updatedLeft.tabs.map((t) => t.id)).not.toContain(tabToMove.id);
    expect(updatedRight.tabs.map((t) => t.id)).toContain(tabToMove.id);
  });

  it('moveTab collapses empty source group when it is not root', () => {
    setupWithTab();
    const initialLeaf = getLeaf();
    usePaneStore.getState().splitGroup(initialLeaf.groupId, 'vertical');
    const split = getSplit();
    const rightLeaf = split.children[1] as LeafNode;
    const leftLeaf = split.children[0] as LeafNode;

    // Add a tab to right so we can move it.
    usePaneStore.getState().openTab(makeTab(), rightLeaf.groupId);
    const updatedSplit = getSplit();
    const updatedRight = updatedSplit.children[1] as LeafNode;

    const tabToMove = updatedRight.tabs[0];
    usePaneStore.getState().moveTab(tabToMove.id, updatedRight.groupId, leftLeaf.groupId);

    const { root } = usePaneStore.getState();
    expect(root.type).toBe('leaf');
  });

  // ── splitGroup / resizePane ───────────────────────────────────────────────

  it('splitGroup creates a split node', () => {
    const root = usePaneStore.getState().root;
    if (root.type === 'leaf') {
      usePaneStore.getState().splitGroup(root.groupId, 'vertical');
      const updated = usePaneStore.getState().root;
      expect(updated.type).toBe('split');
    }
  });

  it('splitGroup sets direction correctly', () => {
    const leaf = getLeaf();
    usePaneStore.getState().splitGroup(leaf.groupId, 'horizontal');
    const split = getSplit();
    expect(split.direction).toBe('horizontal');
  });

  it('splitGroup initialises new pane with equal sizes', () => {
    const leaf = getLeaf();
    usePaneStore.getState().splitGroup(leaf.groupId, 'vertical');
    const split = getSplit();
    expect(split.sizes).toEqual([50, 50]);
  });

  it('resizePane updates split sizes', () => {
    const leaf = getLeaf();
    usePaneStore.getState().splitGroup(leaf.groupId, 'horizontal');
    const split = getSplit();
    usePaneStore.getState().resizePane(split.id, [30, 70]);
    const updated = getSplit();
    expect(updated.sizes).toEqual([30, 70]);
  });

  // ── updateRequest ─────────────────────────────────────────────────────────

  it('updateRequest merges patch into tab request and marks dirty', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().updateRequest(tabId, { url: 'https://api.test', method: 'POST' });
    const updated = getLeaf();
    const tab = updated.tabs.find((t) => t.id === tabId);
    if (!tab || !isRequestTab(tab)) throw new Error('Expected request tab');
    expect(tab.request.url).toBe('https://api.test');
    expect(tab.request.method).toBe('POST');
    expect(tab.isDirty).toBe(true);
  });

  // ── setResponse ───────────────────────────────────────────────────────────

  it('setResponse stores the response on the correct tab', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    const response: ResponseState = {
      status: 200,
      statusText: 'OK',
      headers: [],
      body: '{"ok":true}',
      durationMs: 123,
      ttfbMs: 0,
      sizeBytes: 11,
      activeView: 'pretty',
    };
    usePaneStore.getState().setResponse(tabId, response);
    const updated = getLeaf();
    const tab = updated.tabs.find((t) => t.id === tabId);
    if (!tab || !isRequestTab(tab)) throw new Error('Expected request tab');
    expect(tab.response).toEqual(response);
  });

  // ── markDirty / markClean ─────────────────────────────────────────────────

  it('markDirty sets isDirty to true', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().markDirty(tabId);
    const updated = getLeaf();
    expect(updated.tabs.find((t) => t.id === tabId)?.isDirty).toBe(true);
  });

  it('markClean sets isDirty to false after markDirty', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().markDirty(tabId);
    usePaneStore.getState().markClean(tabId);
    const updated = getLeaf();
    expect(updated.tabs.find((t) => t.id === tabId)?.isDirty).toBe(false);
  });

  // ── reset ─────────────────────────────────────────────────────────────────

  it('reset returns to single empty leaf', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().reset();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(0);
  });

  // ── closeAll ──────────────────────────────────────────────────────────────

  it('closeAll resets the pane tree to a single empty leaf', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().closeAll();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(0);
  });

  it('closeAll auto-saves only dirty request tabs that have a source', () => {
    const mockSave = vi.mocked(scheduleAutoSave);
    mockSave.mockClear();

    const dirtyWithSource: RequestTab = {
      ...makeTab(),
      isDirty: true,
      source: { collection: 'my-col', path: 'req1' },
    };
    const dirtyNoSource: RequestTab = {
      ...makeTab(),
      isDirty: true,
    };
    const cleanWithSource: RequestTab = {
      ...makeTab(),
      isDirty: false,
      source: { collection: 'my-col', path: 'req2' },
    };

    usePaneStore.getState().openTab(dirtyWithSource);
    usePaneStore.getState().openTab(dirtyNoSource);
    usePaneStore.getState().openTab(cleanWithSource);
    usePaneStore.getState().closeAll();

    expect(mockSave).toHaveBeenCalledTimes(1);
    expect(mockSave).toHaveBeenCalledWith(
      dirtyWithSource.id,
      'my-col',
      'req1',
      dirtyWithSource.title,
      dirtyWithSource.request,
    );
  });

  it('closeAll flushes dirty tabs in a split pane layout', () => {
    const mockSave = vi.mocked(scheduleAutoSave);
    mockSave.mockClear();

    // Open one tab so we can split.
    usePaneStore.getState().openTab(makeTab());
    const initialLeaf = getLeaf();
    usePaneStore.getState().splitGroup(initialLeaf.groupId, 'horizontal');

    // Get the right pane's groupId.
    const { root } = usePaneStore.getState();
    if (root.type !== 'split') throw new Error('Expected split');
    const rightLeaf = root.children[1] as LeafNode;

    // Add a dirty tab with source to the right pane.
    const dirtyTab: RequestTab = {
      ...makeTab(),
      isDirty: true,
      source: { collection: 'col', path: 'req' },
    };
    usePaneStore.getState().openTab(dirtyTab, rightLeaf.groupId);
    usePaneStore.getState().closeAll();

    expect(mockSave).toHaveBeenCalledWith(
      dirtyTab.id,
      'col',
      'req',
      dirtyTab.title,
      dirtyTab.request,
    );
  });

  it('closeAll does not call scheduleAutoSave for non-request tabs', () => {
    const mockSave = vi.mocked(scheduleAutoSave);
    mockSave.mockClear();

    const collectionTab: CollectionTab = {
      id: crypto.randomUUID(),
      title: 'My Collection',
      isDirty: true,
      tabType: 'collection',
      collectionName: 'my-col',
    };
    usePaneStore.getState().openTab(collectionTab);
    usePaneStore.getState().closeAll();

    expect(mockSave).not.toHaveBeenCalled();
  });

  // ── switchCollection ────────────────────────────────────────────────

  it('switchCollection snapshots current tabs and restores target', () => {
    const tab1 = makeTab();
    const tab2 = makeTab();
    usePaneStore.getState().openTab(tab1);
    usePaneStore.getState().setActiveCollection('collectionA');

    // Switch to collectionB (no tabs yet)
    usePaneStore.getState().switchCollection('collectionB');
    const leafAfterSwitch = getLeaf();
    expect(leafAfterSwitch.tabs).toHaveLength(0);
    expect(usePaneStore.getState().activeCollection).toBe('collectionB');

    // Open a tab in collectionB
    usePaneStore.getState().openTab(tab2);

    // Switch back to collectionA — should restore tab1
    usePaneStore.getState().switchCollection('collectionA');
    const leafBack = getLeaf();
    expect(leafBack.tabs).toHaveLength(1);
    expect(leafBack.tabs[0].id).toBe(tab1.id);
    expect(leafBack.activeTabId).toBe(tab1.id);
  });

  it('switchCollection deletes the stale collectionTabState entry after restoring it', () => {
    const tab1 = makeTab();
    usePaneStore.getState().openTab(tab1);
    usePaneStore.getState().setActiveCollection('collectionA');

    // Switch away — collectionA gets snapshotted.
    usePaneStore.getState().switchCollection('collectionB');
    expect(usePaneStore.getState().collectionTabState['collectionA']).toBeDefined();

    // Switch back — the snapshot is restored into root and must not linger,
    // since it now duplicates what is live in root.
    usePaneStore.getState().switchCollection('collectionA');
    expect(usePaneStore.getState().collectionTabState['collectionA']).toBeUndefined();
  });

  it('regression: a tab closed after switching away and back has no stale snapshot copy to "activate"', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const tab1 = makeTab();
    usePaneStore.getState().openTab(tab1);
    usePaneStore.getState().setActiveCollection('collectionA');
    const tabId = tab1.id;

    // Start an agent session (status 'starting', no real session id yet).
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');

    // Switch away (snapshots collectionA with the 'starting' tab) and back
    // (restores it into root). The restored snapshot must be dropped —
    // otherwise it lingers as a stale duplicate.
    usePaneStore.getState().switchCollection('collectionB');
    usePaneStore.getState().switchCollection('collectionA');
    expect(usePaneStore.getState().collectionTabState['collectionA']).toBeUndefined();

    // Close the tab. Its session is still 'starting', so closeTab does not
    // end it yet — that's correct, since the backend hasn't handed back a
    // real session id.
    const leaf = getLeaf();
    usePaneStore.getState().closeTab(tabId, leaf.groupId);
    expect(endAgentSession).not.toHaveBeenCalled();

    // Simulate the session's start call resolving after the tab was closed.
    // With the stale snapshot gone, there is nothing left to find and
    // "activate" — the tab is genuinely gone.
    expect(usePaneStore.getState().activateAgentSession(tabId, 'session-1')).toBe(false);
  });

  it('switchCollection to never-opened collection shows empty tabs', () => {
    usePaneStore.getState().setActiveCollection('existingCol');
    usePaneStore.getState().openTab(makeTab());

    usePaneStore.getState().switchCollection('brandNewCol');
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(0);
    expect(leaf.activeTabId).toBe('');
  });

  it('switchCollection from null activeCollection does not throw and sets active collection', () => {
    // Fresh store has activeCollection === null
    expect(usePaneStore.getState().activeCollection).toBeNull();

    usePaneStore.getState().switchCollection('firstCol');
    expect(usePaneStore.getState().activeCollection).toBe('firstCol');
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(0);
  });

  // ── openEphemeralTab ──────────────────────────────────────────────────────

  describe('openEphemeralTab', () => {
    it('opens a request tab with no source and title "Untitled"', () => {
      usePaneStore.getState().openEphemeralTab();
      const leaf = getLeaf();
      expect(leaf.tabs).toHaveLength(1);
      const tab = leaf.tabs[0];
      expect(tab.tabType).toBe('request');
      expect(tab.title).toBe('Untitled');
      if (isRequestTab(tab)) {
        expect(tab.source).toBeUndefined();
        expect(tab.isDirty).toBe(false);
        expect(tab.request.requestType).toBe('http');
      }
    });

    it('openEphemeralTab with "graphql" sets requestType correctly', () => {
      usePaneStore.getState().openEphemeralTab('graphql');
      const leaf = getLeaf();
      const tab = leaf.tabs[0];
      expect(isRequestTab(tab)).toBe(true);
      if (isRequestTab(tab)) {
        expect(tab.request.requestType).toBe('graphql');
      }
    });
  });

  it('getOpenTabCount returns correct count per collection', () => {
    usePaneStore.getState().setActiveCollection('colA');
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());

    usePaneStore.getState().switchCollection('colB');
    usePaneStore.getState().openTab(makeTab());

    expect(usePaneStore.getState().getOpenTabCount('colA')).toBe(2);
    expect(usePaneStore.getState().getOpenTabCount('colB')).toBe(1);
    expect(usePaneStore.getState().getOpenTabCount('colC')).toBe(0);
  });
});

describe('Runner tab actions', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('openRunnerTab opens a runner tab scoped to a collection, populated with its requests', async () => {
    const { getCollection } = await import('@/lib/tauri-api');
    vi.mocked(getCollection).mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [] } as never,
      root: {
        uid: 'root',
        name: 'demo',
        items: [
          {
            type: 'request',
            uid: 'r1',
            name: 'Ping',
            method: 'GET',
            url: 'https://example.com/ping',
            headers: [],
            auth: { authType: 'none' },
            fileName: 'ping.yml',
          },
        ],
      },
    });

    await usePaneStore.getState().openRunnerTab('demo');

    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(1);
    const tab = leaf.tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.collectionName).toBe('demo');
    expect(tab.runState).toBe('idle');
    expect(tab.requests).toHaveLength(1);
    expect(tab.requests[0].requestPath).toBe('ping.yml');
  });

  it('openRunnerTab opens an empty picker tab when collectionName is null', async () => {
    await usePaneStore.getState().openRunnerTab(null);

    const leaf = getLeaf();
    const tab = leaf.tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.collectionName).toBeNull();
    expect(tab.requests).toEqual([]);
  });

  async function openTwoRequestRunnerTab(): Promise<string> {
    const { getCollection } = await import('@/lib/tauri-api');
    vi.mocked(getCollection).mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [] } as never,
      root: {
        uid: 'root',
        name: 'demo',
        items: [
          {
            type: 'request',
            uid: 'r1',
            name: 'First',
            method: 'GET',
            url: 'https://example.com/1',
            headers: [],
            auth: { authType: 'none' },
            fileName: 'first.yml',
          },
          {
            type: 'request',
            uid: 'r2',
            name: 'Second',
            method: 'GET',
            url: 'https://example.com/2',
            headers: [],
            auth: { authType: 'none' },
            fileName: 'second.yml',
          },
        ],
      },
    });
    await usePaneStore.getState().openRunnerTab('demo');
    const tab = getLeaf().tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    return tab.id;
  }

  it('toggleRunnerEntry flips included for one entry', async () => {
    const tabId = await openTwoRequestRunnerTab();
    usePaneStore.getState().toggleRunnerEntry(tabId, 'first.yml');

    const tab = getLeaf().tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.requests.find((e) => e.requestPath === 'first.yml')?.included).toBe(false);
    expect(tab.requests.find((e) => e.requestPath === 'second.yml')?.included).toBe(true);
  });

  it('startRun executes every included entry in order and marks the tab done', async () => {
    const { executeRunnerEntry } = await import('@/lib/runner-execute');
    vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'passed', result: undefined });

    const tabId = await openTwoRequestRunnerTab();
    await usePaneStore.getState().startRun(tabId);

    expect(executeRunnerEntry).toHaveBeenCalledTimes(2);
    const tab = getLeaf().tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.runState).toBe('done');
    expect(tab.requests.every((e) => e.status === 'passed')).toBe(true);
  });

  it('startRun skips excluded entries', async () => {
    const { executeRunnerEntry } = await import('@/lib/runner-execute');
    vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'passed', result: undefined });

    const tabId = await openTwoRequestRunnerTab();
    usePaneStore.getState().toggleRunnerEntry(tabId, 'first.yml');
    await usePaneStore.getState().startRun(tabId);

    expect(executeRunnerEntry).toHaveBeenCalledTimes(1);
    expect(executeRunnerEntry).toHaveBeenCalledWith(
      'demo',
      'second.yml',
      expect.anything(),
      undefined,
    );
  });

  it('stopRun halts the run and marks remaining entries skipped', async () => {
    const { executeRunnerEntry } = await import('@/lib/runner-execute');
    let resolveFirst: (() => void) | undefined;
    vi.mocked(executeRunnerEntry).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveFirst = () => resolve({ status: 'passed', result: undefined });
        }),
    );

    const tabId = await openTwoRequestRunnerTab();
    const runPromise = usePaneStore.getState().startRun(tabId);

    usePaneStore.getState().stopRun(tabId);
    resolveFirst?.();
    await runPromise;

    const tab = getLeaf().tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.runState).toBe('stopped');
    expect(tab.requests.find((e) => e.requestPath === 'first.yml')?.status).toBe('passed');
    expect(tab.requests.find((e) => e.requestPath === 'second.yml')?.status).toBe('skipped');
    expect(executeRunnerEntry).toHaveBeenCalledTimes(1);
  });

  it('rerunAll resets every entry to pending and runs again', async () => {
    const { executeRunnerEntry } = await import('@/lib/runner-execute');
    vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'failed', error: 'boom' });

    const tabId = await openTwoRequestRunnerTab();
    await usePaneStore.getState().startRun(tabId);
    vi.mocked(executeRunnerEntry).mockClear();
    vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'passed', result: undefined });

    await usePaneStore.getState().rerunAll(tabId);

    expect(executeRunnerEntry).toHaveBeenCalledTimes(2);
    const tab = getLeaf().tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.requests.every((e) => e.status === 'passed')).toBe(true);
  });

  it('stopping then immediately re-running does not let the stopped run dispatch further requests or overwrite the new run', async () => {
    // Regression test for a real race found by final review: stopRun only
    // flips runState to 'stopped', and the run loop's continuation guard
    // checked runState alone. rerunAll's own startRun call resets runState
    // back to 'running', so a still-in-flight old loop resumed as if it had
    // never been stopped -- dispatching a request the user believed they had
    // cancelled, and racing the new run's writes with stale results.
    const { executeRunnerEntry } = await import('@/lib/runner-execute');

    const calls: string[] = [];
    let resolveRun1Second: (() => void) | undefined;
    vi.mocked(executeRunnerEntry).mockImplementation(async (_collection, requestPath) => {
      calls.push(requestPath);
      if (requestPath === 'second.yml' && calls.filter((p) => p === 'second.yml').length === 1) {
        // Run 1's second.yml (its first-ever call) hangs until
        // resolveRun1Second() is invoked -- this is the request still in
        // flight when Stop is pressed.
        return new Promise((resolve) => {
          resolveRun1Second = () => resolve({ status: 'failed', error: 'STALE-FROM-RUN-1' });
        });
      }
      return { status: 'passed', result: undefined };
    });

    // Deterministically waits for the Nth call to executeRunnerEntry to have
    // been recorded, without assuming how many microtask ticks any given
    // mock resolution takes internally.
    async function waitForCallCount(n: number): Promise<void> {
      for (let i = 0; i < 100 && calls.length < n; i++) {
        await Promise.resolve();
      }
      expect(calls.length).toBeGreaterThanOrEqual(n);
    }

    const tabId = await openTwoRequestRunnerTab();
    const run1Promise = usePaneStore.getState().startRun(tabId);

    // Let run 1 dispatch first.yml (resolves immediately) and reach
    // second.yml (hangs), without letting second.yml resolve yet.
    await waitForCallCount(2);

    usePaneStore.getState().stopRun(tabId);
    const run2Promise = usePaneStore.getState().rerunAll(tabId);

    // Let run 2 dispatch its own first.yml before resolving run 1's stale
    // second.yml call, so the interleaving matches the reviewer's repro:
    // the old run's late result arrives after the new run has already taken
    // the tab over.
    await waitForCallCount(3);
    resolveRun1Second?.();

    await Promise.all([run1Promise, run2Promise]);

    const tab = getLeaf().tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    // Run 2 must win cleanly: both entries passed (run 2's outcome), the run
    // finished 'done', and run 1's stale 'failed' write never lands.
    expect(tab.runState).toBe('done');
    expect(tab.requests.find((e) => e.requestPath === 'first.yml')?.status).toBe('passed');
    expect(tab.requests.find((e) => e.requestPath === 'second.yml')?.status).toBe('passed');
    // Exactly 4 real dispatches: run 1's first.yml + second.yml, run 2's
    // first.yml + second.yml. Critically, second.yml is only ever dispatched
    // once per run -- run 1 never gets a second chance to dispatch anything
    // after being superseded.
    expect(executeRunnerEntry).toHaveBeenCalledTimes(4);
    expect(calls.filter((p) => p === 'second.yml')).toHaveLength(2);
  });
});

describe('Flow tab actions', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('openFlowTab with no flowName opens a picker-state tab', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const tab = findFirstFlowTab();
    expect(tab?.tabType).toBe('flow');
    expect(tab?.flowName).toBeNull();
    expect(tab?.nodes).toEqual([]);
  });

  it('openFlowTab with a flowName loads nodes/edges immediately', async () => {
    vi.mocked(getFlow).mockResolvedValue({
      name: 'My Flow',
      nodes: [{ id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
      edges: [],
    });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    const tab = findFirstFlowTab();
    expect(tab?.flowName).toBe('My Flow');
    expect(tab?.nodes).toHaveLength(1);
  });

  it('openFlowTab falls back to picker state if getFlow rejects', async () => {
    vi.mocked(getFlow).mockRejectedValue(new Error('not found'));
    await usePaneStore.getState().openFlowTab('my-collection', 'Missing Flow');
    const tab = findFirstFlowTab();
    expect(tab?.nodes).toEqual([]);
  });

  it('patchFlowNodeStatus updates only the targeted node', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const openedTab = findFirstFlowTab();
    if (!openedTab) throw new Error('Expected a flow tab');
    const tabId = openedTab.id;
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'running');
    const tab = findFirstFlowTab();
    expect(tab?.nodeStatus.n1).toBe('running');
  });

  it('patchFlowNodeStatus for an unknown node id is a safe no-op', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const openedTab = findFirstFlowTab();
    if (!openedTab) throw new Error('Expected a flow tab');
    const tabId = openedTab.id;
    expect(() =>
      usePaneStore.getState().patchFlowNodeStatus(tabId, 'does-not-exist', 'running'),
    ).not.toThrow();
    expect(findFirstFlowTab()?.nodeStatus['does-not-exist']).toBeUndefined();
  });

  it('keeps node status separate for the same flow opened twice', async () => {
    vi.mocked(getFlow).mockResolvedValue({
      name: 'My Flow',
      nodes: [{ id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
      edges: [],
    });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    const root = usePaneStore.getState().root;
    if (root.type !== 'leaf') throw new Error('Expected a single leaf');
    const [first, second] = root.tabs.filter(isFlowTab);
    expect(first.id).not.toBe(second.id);

    usePaneStore.getState().patchFlowNodeStatus(first.id, 'n1', 'success');
    const after = usePaneStore.getState().root;
    if (after.type !== 'leaf') throw new Error('Expected a single leaf');
    const [firstAfter, secondAfter] = after.tabs.filter(isFlowTab);
    expect(firstAfter.nodeStatus.n1).toBe('success');
    expect(secondAfter.nodeStatus.n1).toBeUndefined();
  });

  it('keeps the collection on the picker tab when getFlow rejects', async () => {
    vi.mocked(getFlow).mockRejectedValue('Not found: flow');
    await usePaneStore.getState().openFlowTab('my-collection', 'Missing Flow');
    const tab = findFirstFlowTab();
    expect(tab?.flowName).toBeNull();
    expect(tab?.collectionName).toBe('my-collection');
  });

  it('setFlowRunState stores the run id and state on the tab', async () => {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-123');
    const tab = findFirstFlowTab();
    expect(tab?.runState).toBe('running');
    expect(tab?.runId).toBe('run-123');
  });

  it('patchFlowNodeStatus records optional detail alongside the status', async () => {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', {
      statusCode: 200,
      durationMs: 184,
    });
    const tab = findFirstFlowTab();
    expect(tab?.nodeStatus.n1).toBe('success');
    expect(tab?.nodeDetail?.n1).toEqual({ statusCode: 200, durationMs: 184 });
  });

  it('patchFlowNodeStatus keeps skip reason, branch and value in the detail', async () => {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    usePaneStore
      .getState()
      .patchFlowNodeStatus(tabId, 'n1', 'skipped', { skipReason: 'branch_not_taken' });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ skipReason: 'branch_not_taken' });

    usePaneStore
      .getState()
      .patchFlowNodeStatus(tabId, 'n1', 'success', { branch: 'true', value: '42' });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ branch: 'true', value: '42' });
  });

  async function flowTabWithNode() {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    return tabId;
  }

  it('patchFlowNodeProgress merges progress into the node detail', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'running', { statusCode: 202 });
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'n1', 'attempt 3/30');
    const tab = findFirstFlowTab();
    expect(tab?.nodeStatus.n1).toBe('running');
    expect(tab?.nodeDetail?.n1).toEqual({ statusCode: 202, progress: 'attempt 3/30' });
  });

  it('a completed status patch clears the progress text', async () => {
    const tabId = await flowTabWithNode();
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'n1', 'attempt 3/30');
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', { statusCode: 200 });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ statusCode: 200 });
  });

  it('patchFlowNodeProgress for an unknown node id is a safe no-op', async () => {
    const tabId = await flowTabWithNode();
    const before = findFirstFlowTab();
    usePaneStore.getState().patchFlowNodeProgress(tabId, 'does-not-exist', 'attempt 1/2');
    expect(findFirstFlowTab()?.nodeDetail).toEqual(before?.nodeDetail);
  });

  it('setFlowRunState clears the last run results when a new run starts', async () => {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'failed', { error: 'boom' });
    usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1');
    expect(findFirstFlowTab()?.nodeStatus.n1).toBe('failed');

    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    const tab = findFirstFlowTab();
    expect(tab?.nodeStatus).toEqual({});
    expect(tab?.nodeDetail).toEqual({});
  });

  it('updateFlowGraph replaces nodes and edges in a single store update', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    const node: FlowNode = {
      id: 'n1',
      kind: { kind: 'Output', label: 'Out' },
      position: { x: 0, y: 0 },
    };
    const listener = vi.fn();
    const unsubscribe = usePaneStore.subscribe(listener);

    usePaneStore.getState().updateFlowGraph(tabId, [node], []);
    unsubscribe();

    expect(listener).toHaveBeenCalledTimes(1);
    const tab = findFirstFlowTab();
    expect(tab?.nodes).toEqual([node]);
    expect(tab?.edges).toEqual([]);
    expect(tab?.isDirty).toBe(true);
  });

  it('openFlowTab loads the callback host', async () => {
    vi.mocked(getFlow).mockResolvedValue({
      name: 'My Flow',
      nodes: [],
      edges: [],
      callbackHost: 'host.docker.internal',
    });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    expect(findFirstFlowTab()?.callbackHost).toBe('host.docker.internal');
  });

  it('setFlowCallbackHost stores the host and marks the tab dirty', async () => {
    vi.mocked(getFlow).mockResolvedValue({ name: 'My Flow', nodes: [], edges: [] });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    const tabId = findFirstFlowTab()?.id ?? '';

    usePaneStore.getState().setFlowCallbackHost(tabId, '10.0.0.5');

    const tab = findFirstFlowTab();
    expect(tab?.callbackHost).toBe('10.0.0.5');
    expect(tab?.isDirty).toBe(true);
  });
});

describe('Agent chat session actions', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  function getRequestTab(): RequestTab {
    const leaf = getLeaf();
    const tab = leaf.tabs[0];
    if (!isRequestTab(tab)) throw new Error('Expected a request tab');
    return tab;
  }

  it('beginAgentSession sets status starting with an empty session id', () => {
    const leaf = setupWithTab();
    usePaneStore.getState().beginAgentSession(leaf.tabs[0].id, 'agent-1');
    const tab = getRequestTab();
    expect(tab.agentSession).toEqual({
      agentConfigId: 'agent-1',
      sessionId: '',
      status: 'starting',
      messages: [],
    });
  });

  it('activateAgentSession sets the real session id and status active', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    expect(usePaneStore.getState().activateAgentSession(tabId, 'session-1')).toBe(true);
    const tab = getRequestTab();
    expect(tab.agentSession?.sessionId).toBe('session-1');
    expect(tab.agentSession?.status).toBe('active');
  });

  it('activateAgentSession returns false and changes nothing when the session is not starting', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    const before = usePaneStore.getState().root;

    expect(usePaneStore.getState().activateAgentSession(tabId, 'session-2')).toBe(false);
    expect(usePaneStore.getState().root).toBe(before);
    expect(getRequestTab().agentSession?.sessionId).toBe('session-1');
  });

  it('activateAgentSession returns false and changes nothing when the tab has no session', () => {
    const leaf = setupWithTab();
    const before = usePaneStore.getState().root;

    expect(usePaneStore.getState().activateAgentSession(leaf.tabs[0].id, 'session-1')).toBe(false);
    expect(usePaneStore.getState().root).toBe(before);
    expect(getRequestTab().agentSession).toBeUndefined();
  });

  it('activateAgentSession returns false and changes nothing for an unknown tab id', () => {
    setupWithTab();
    const before = usePaneStore.getState();

    expect(usePaneStore.getState().activateAgentSession('no-such-tab', 'session-1')).toBe(false);
    expect(usePaneStore.getState().root).toBe(before.root);
    expect(usePaneStore.getState().collectionTabState).toBe(before.collectionTabState);
  });

  it('activateAgentSession activates a starting session parked in a collection snapshot', () => {
    const tab = makeTab();
    tab.agentSession = {
      agentConfigId: 'agent-1',
      sessionId: '',
      status: 'starting',
      messages: [],
    };
    usePaneStore.setState({
      collectionTabState: { 'col-a': { tabs: [tab], activeTabId: tab.id } },
    });

    expect(usePaneStore.getState().activateAgentSession(tab.id, 'session-1')).toBe(true);
    const parked = usePaneStore.getState().collectionTabState['col-a'].tabs[0];
    expect(isRequestTab(parked) && parked.agentSession?.status).toBe('active');
    expect(isRequestTab(parked) && parked.agentSession?.sessionId).toBe('session-1');
  });

  it('appendAgentChatChunk reaches a tab parked in a collection snapshot', () => {
    const tab = makeTab();
    tab.agentSession = {
      agentConfigId: 'agent-1',
      sessionId: 'session-1',
      status: 'active',
      messages: [{ id: 'm1', role: 'agent', text: 'a', streaming: true }],
    };
    usePaneStore.setState({
      collectionTabState: { 'col-a': { tabs: [tab], activeTabId: tab.id } },
    });

    usePaneStore.getState().appendAgentChatChunk(tab.id, 'm1', 'b');
    const parked = usePaneStore.getState().collectionTabState['col-a'].tabs[0];
    expect(isRequestTab(parked) && parked.agentSession?.messages[0].text).toBe('ab');
  });

  it('appendAgentChatMessage appends to the messages list', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore.getState().appendAgentChatMessage(tabId, { id: 'm1', role: 'user', text: 'hi' });
    const tab = getRequestTab();
    expect(tab.agentSession?.messages).toEqual([{ id: 'm1', role: 'user', text: 'hi' }]);
  });

  it('appendAgentChatChunk appends text onto the matching message only', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore.getState().appendAgentChatMessage(tabId, { id: 'm1', role: 'user', text: 'hi' });
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm2', role: 'agent', text: '', streaming: true });
    usePaneStore.getState().appendAgentChatChunk(tabId, 'm2', 'Hello');
    usePaneStore.getState().appendAgentChatChunk(tabId, 'm2', ' there');
    const tab = getRequestTab();
    expect(tab.agentSession?.messages).toEqual([
      { id: 'm1', role: 'user', text: 'hi' },
      { id: 'm2', role: 'agent', text: 'Hello there', streaming: true },
    ]);
  });

  it('completeAgentChatMessage marks the message not streaming', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm2', role: 'agent', text: 'done', streaming: true });
    usePaneStore.getState().completeAgentChatMessage(tabId, 'm2');
    const tab = getRequestTab();
    expect(tab.agentSession?.messages[0].streaming).toBe(false);
  });

  it('failAgentChatMessage marks status error, sets session error, and appends the error to the message', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm2', role: 'agent', text: 'partial', streaming: true });
    usePaneStore.getState().failAgentChatMessage(tabId, 'm2', 'agent crashed');
    const tab = getRequestTab();
    expect(tab.agentSession?.status).toBe('error');
    expect(tab.agentSession?.error).toBe('agent crashed');
    expect(tab.agentSession?.messages[0]).toEqual({
      id: 'm2',
      role: 'agent',
      text: 'partial\n\nError: agent crashed',
      streaming: false,
    });
  });

  it('markAgentSessionEnded sets status ended', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore.getState().markAgentSessionEnded(tabId);
    expect(getRequestTab().agentSession?.status).toBe('ended');
  });

  it('clearAgentSession removes the agent session entirely', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().clearAgentSession(tabId);
    expect(getRequestTab().agentSession).toBeUndefined();
  });
});

describe('closeTab — ends the agent session for an active chat', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('calls endAgentSession when the closed tab has an active session', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore.getState().closeTab(tabId, leaf.groupId);
    expect(endAgentSession).toHaveBeenCalledWith('session-1');
  });

  it('does not call endAgentSession when the session is still starting (no real session id yet)', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().closeTab(tabId, leaf.groupId);
    expect(endAgentSession).not.toHaveBeenCalled();
  });

  it('does not call endAgentSession when there is no agent session at all', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    usePaneStore.getState().closeTab(leaf.tabs[0].id, leaf.groupId);
    expect(endAgentSession).not.toHaveBeenCalled();
  });
});

describe('closeAll/openWorkspaceTabs — end agent sessions of dropped tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  function tabWithActiveSession(sessionId: string): RequestTab {
    return {
      ...makeTab(),
      agentSession: { agentConfigId: 'agent-1', sessionId, status: 'active', messages: [] },
    };
  }

  it('closeAll ends an active session found in the pane tree', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = getLeaf();
    usePaneStore.getState().splitGroup(leaf.groupId, 'horizontal');
    const second = getSplit().children[1] as LeafNode;
    usePaneStore.getState().openTab(tabWithActiveSession('session-split'), second.groupId);

    usePaneStore.getState().closeAll();

    expect(endAgentSession).toHaveBeenCalledWith('session-split');
  });

  it('closeAll ends an active session found in a collection snapshot', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const parked = tabWithActiveSession('session-parked');
    usePaneStore.setState({
      collectionTabState: { 'col-b': { tabs: [parked], activeTabId: parked.id } },
    });

    usePaneStore.getState().closeAll();

    expect(endAgentSession).toHaveBeenCalledWith('session-parked');
  });

  it('closeAll does not end a session that is still starting', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    usePaneStore.getState().beginAgentSession(leaf.tabs[0].id, 'agent-1');

    usePaneStore.getState().closeAll();

    expect(endAgentSession).not.toHaveBeenCalled();
  });

  it('openWorkspaceTabs ends an active session in a dropped non-active pane tab', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    usePaneStore.getState().setActiveCollection('col-a');
    const kept = tabWithActiveSession('session-kept');
    usePaneStore.getState().openTab(kept);
    const firstGroupId = getLeaf().groupId;
    usePaneStore.getState().splitGroup(firstGroupId, 'horizontal');
    const second = getSplit().children[1] as LeafNode;
    usePaneStore.getState().openTab(tabWithActiveSession('session-dropped'), second.groupId);
    usePaneStore.getState().setActiveGroup(firstGroupId);

    usePaneStore.getState().openWorkspaceTabs('ws-1');

    expect(endAgentSession).toHaveBeenCalledWith('session-dropped');
    expect(endAgentSession).not.toHaveBeenCalledWith('session-kept');
    const snapshot = usePaneStore.getState().collectionTabState['col-a'];
    expect(snapshot.tabs.map((t) => t.id)).toEqual([kept.id]);
  });
});
