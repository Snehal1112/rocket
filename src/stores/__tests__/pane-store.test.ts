import { describe, it, expect, beforeEach } from 'vitest';
import { usePaneStore } from '../pane-store';
import type { LeafNode, SplitNode, ResponseState } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

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

// Helper: create a draft tab and return the leaf.
function setupWithTab(): LeafNode {
  usePaneStore.getState().newDraftTab();
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

  // ── newDraftTab ───────────────────────────────────────────────────────────

  it('newDraftTab adds tab to active group', () => {
    usePaneStore.getState().newDraftTab();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(1);
    expect(leaf.tabs[0].tabType).toBe('draft');
  });

  it('newDraftTab sets the new tab as active', () => {
    usePaneStore.getState().newDraftTab();
    const leaf = getLeaf();
    expect(leaf.activeTabId).toBe(leaf.tabs[0].id);
  });

  it('newDraftTab adds a second tab', () => {
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(2);
    expect(leaf.activeTabId).toBe(leaf.tabs[1].id);
  });

  it('newDraftTab with explicit groupId targets that group', () => {
    setupWithTab();
    const leaf = getLeaf();
    usePaneStore.getState().splitGroup(leaf.groupId, 'vertical');
    const split = getSplit();
    const rightLeaf = split.children[1] as LeafNode;

    usePaneStore.getState().newDraftTab(rightLeaf.groupId);

    const updatedSplit = getSplit();
    const updatedRight = updatedSplit.children[1] as LeafNode;
    expect(updatedRight.tabs).toHaveLength(1);
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
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
    const leaf = getLeaf();
    const firstTabId = leaf.tabs[0].id;

    usePaneStore.getState().openTab(leaf.tabs[0]);

    const updated = getLeaf();
    expect(updated.tabs).toHaveLength(2);
    expect(updated.activeTabId).toBe(firstTabId);
  });

  // ── closeTab ──────────────────────────────────────────────────────────────

  it('closeTab removes tab and keeps group if tabs remain', () => {
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
    const leaf = getLeaf();
    usePaneStore.getState().closeTab(leaf.tabs[0].id, leaf.groupId);
    const updated = getLeaf();
    expect(updated.tabs).toHaveLength(1);
  });

  it('closeTab activates the previous tab after removal', () => {
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
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
    usePaneStore.getState().newDraftTab(rightLeaf.groupId);
    const updatedSplit = getSplit();
    const updatedRight = updatedSplit.children[1] as LeafNode;
    usePaneStore.getState().closeTab(updatedRight.tabs[0].id, updatedRight.groupId);

    const { root } = usePaneStore.getState();
    expect(root.type).toBe('leaf');
  });

  // ── setActiveTab ──────────────────────────────────────────────────────────

  it('setActiveTab updates activeTabId and activeGroupId', () => {
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
    const leaf = getLeaf();
    const firstId = leaf.tabs[0].id;
    usePaneStore.getState().setActiveTab(firstId, leaf.groupId);
    const updated = getLeaf();
    expect(updated.activeTabId).toBe(firstId);
    expect(usePaneStore.getState().activeGroupId).toBe(leaf.groupId);
  });

  // ── moveTab ───────────────────────────────────────────────────────────────

  it('moveTab transfers a tab from one group to another', () => {
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
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
    usePaneStore.getState().newDraftTab(rightLeaf.groupId);
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
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().newDraftTab();
    usePaneStore.getState().reset();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(0);
  });
});
