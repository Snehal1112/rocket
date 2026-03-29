import { vi, describe, it, expect, beforeEach } from 'vitest';
import { usePaneStore } from '../pane-store';
import type { LeafNode, SplitNode, ResponseState, RequestTab, CollectionTab } from '@/types/pane-types';
import { isRequestTab, isGitTab } from '@/types/pane-types';
import { createDefaultRequest } from '@/lib/pane-utils';
import { scheduleAutoSave } from '@/lib/auto-save';

vi.mock('@/lib/auto-save', () => ({
  scheduleAutoSave: vi.fn(),
}))

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
        method: 'GET' as const,
        url: 'https://example.com',
        pathParams: [],
        queryParams: [],
        headers: [],
        body: { mode: 'none' as const, content: '', formData: [] },
        auth: { authType: 'none' as const },
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
    const tab = updated.tabs.find((t) => t.id === tabId)!;
    if (!isRequestTab(tab)) throw new Error('Expected request tab');
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
      sizeBytes: 11,
      activeView: 'pretty',
    };
    usePaneStore.getState().setResponse(tabId, response);
    const updated = getLeaf();
    const tab = updated.tabs.find((t) => t.id === tabId)!;
    if (!isRequestTab(tab)) throw new Error('Expected request tab');
    expect(tab.response).toEqual(response);
  });

  // ── markDirty / markClean ─────────────────────────────────────────────────

  it('markDirty sets isDirty to true', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().markDirty(tabId);
    const updated = getLeaf();
    expect(updated.tabs.find((t) => t.id === tabId)!.isDirty).toBe(true);
  });

  it('markClean sets isDirty to false after markDirty', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().markDirty(tabId);
    usePaneStore.getState().markClean(tabId);
    const updated = getLeaf();
    expect(updated.tabs.find((t) => t.id === tabId)!.isDirty).toBe(false);
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

  // ── isGitTab ──────────────────────────────────────────────────────────────

  it('isGitTab returns true for git tabs and false for others', () => {
    const gitTab = {
      id: 'git:test',
      title: 'Git',
      tabType: 'git' as const,
      collectionName: 'test',
      collectionPath: '/path/to/test',
      isDirty: false,
    };
    const requestTab = makeTab();
    expect(isGitTab(gitTab)).toBe(true);
    expect(isGitTab(requestTab)).toBe(false);
  });
});
