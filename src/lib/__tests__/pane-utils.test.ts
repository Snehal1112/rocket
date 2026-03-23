import { describe, it, expect } from 'vitest';
import {
  findLeaf,
  findTabInTree,
  updateLeaf,
  removeLeaf,
  splitLeaf,
  createDefaultLeaf,
  createDefaultTab,
  findFirstLeaf,
  findActiveLeaf,
} from '../pane-utils';
import type { PaneNode, LeafNode } from '@/types/pane-types';

describe('pane-utils', () => {
  const leaf1 = createDefaultLeaf('g1');
  const leaf2 = createDefaultLeaf('g2');
  const splitTree: PaneNode = {
    type: 'split',
    id: 's1',
    direction: 'horizontal',
    children: [leaf1, leaf2],
    sizes: [50, 50],
  };

  // --- findLeaf ---

  it('findLeaf returns correct leaf by groupId', () => {
    const found = findLeaf(splitTree, 'g1');
    expect(found).toBeDefined();
    expect(found!.groupId).toBe('g1');
  });

  it('findLeaf returns null for missing groupId', () => {
    expect(findLeaf(splitTree, 'missing')).toBeNull();
  });

  it('findLeaf works on a single leaf node', () => {
    expect(findLeaf(leaf1, 'g1')).toBe(leaf1);
    expect(findLeaf(leaf1, 'g2')).toBeNull();
  });

  // --- findTabInTree ---

  it('findTabInTree locates a tab in the tree', () => {
    const tabId = leaf1.activeTabId;
    const result = findTabInTree(splitTree, tabId);
    expect(result).not.toBeNull();
    expect(result!.leaf.groupId).toBe('g1');
    expect(result!.tab.id).toBe(tabId);
  });

  it('findTabInTree returns null for unknown tabId', () => {
    expect(findTabInTree(splitTree, 'no-such-tab')).toBeNull();
  });

  // --- updateLeaf ---

  it('updateLeaf replaces a leaf immutably', () => {
    const updated = updateLeaf(splitTree, 'g1', (leaf) => ({
      ...leaf,
      activeTabId: 'new-tab',
    }));
    expect(updated).not.toBe(splitTree);
    const found = findLeaf(updated, 'g1') as LeafNode;
    expect(found.activeTabId).toBe('new-tab');
  });

  it('updateLeaf does not touch unrelated leaves', () => {
    const updated = updateLeaf(splitTree, 'g1', (leaf) => ({
      ...leaf,
      activeTabId: 'x',
    }));
    // g2 leaf must remain the exact same reference.
    expect(findLeaf(updated, 'g2')).toBe(leaf2);
  });

  it('updateLeaf returns same node when groupId not found', () => {
    const result = updateLeaf(splitTree, 'unknown', (leaf) => leaf);
    expect(result).toBe(splitTree);
  });

  // --- removeLeaf ---

  it('removeLeaf collapses parent split when left child is removed', () => {
    const result = removeLeaf(splitTree, 'g1');
    expect(result.type).toBe('leaf');
    expect((result as LeafNode).groupId).toBe('g2');
  });

  it('removeLeaf collapses parent split when right child is removed', () => {
    const result = removeLeaf(splitTree, 'g2');
    expect(result.type).toBe('leaf');
    expect((result as LeafNode).groupId).toBe('g1');
  });

  it('removeLeaf handles nested splits', () => {
    const leaf3 = createDefaultLeaf('g3');
    const inner: PaneNode = {
      type: 'split',
      id: 's2',
      direction: 'vertical',
      children: [leaf2, leaf3],
      sizes: [50, 50],
    };
    const nested: PaneNode = {
      type: 'split',
      id: 's1',
      direction: 'horizontal',
      children: [leaf1, inner],
      sizes: [50, 50],
    };
    const result = removeLeaf(nested, 'g2');
    // The inner split collapses to leaf3, so result is still a split of leaf1 + leaf3.
    expect(result.type).toBe('split');
    expect(findLeaf(result, 'g2')).toBeNull();
    expect(findLeaf(result, 'g3')).not.toBeNull();
    expect(findLeaf(result, 'g1')).not.toBeNull();
  });

  // --- splitLeaf ---

  it('splitLeaf wraps a leaf in a new split node', () => {
    const result = splitLeaf(leaf1, 'g1', 'vertical');
    expect(result.type).toBe('split');
    if (result.type === 'split') {
      expect(result.direction).toBe('vertical');
      expect(result.children[0]).toBe(leaf1);
      expect(result.children[1].type).toBe('leaf');
      expect(result.sizes).toEqual([50, 50]);
    }
  });

  it('splitLeaf does not modify unrelated leaves', () => {
    const result = splitLeaf(splitTree, 'g1', 'horizontal');
    // g2 must be the same reference.
    expect(findLeaf(result, 'g2')).toBe(leaf2);
  });

  it('splitLeaf returns same node when groupId not found', () => {
    const result = splitLeaf(leaf1, 'unknown', 'horizontal');
    expect(result).toBe(leaf1);
  });

  // --- findFirstLeaf ---

  it('findFirstLeaf returns leftmost leaf', () => {
    const first = findFirstLeaf(splitTree);
    expect(first.groupId).toBe('g1');
  });

  it('findFirstLeaf works on a plain leaf', () => {
    expect(findFirstLeaf(leaf1)).toBe(leaf1);
  });

  // --- findActiveLeaf ---

  it('findActiveLeaf returns matching leaf', () => {
    const active = findActiveLeaf(splitTree, 'g2');
    expect(active.groupId).toBe('g2');
  });

  it('findActiveLeaf falls back to first leaf when groupId not found', () => {
    const active = findActiveLeaf(splitTree, 'ghost');
    expect(active.groupId).toBe('g1');
  });

  // --- createDefaultTab ---

  it('createDefaultTab produces unique IDs', () => {
    const a = createDefaultTab();
    const b = createDefaultTab();
    expect(a.id).not.toBe(b.id);
  });

  it('createDefaultTab has expected defaults', () => {
    const tab = createDefaultTab();
    expect(tab.tabType).toBe('draft');
    expect(tab.response).toBeNull();
    expect(tab.isDirty).toBe(false);
    expect(tab.request.method).toBe('GET');
  });
});
