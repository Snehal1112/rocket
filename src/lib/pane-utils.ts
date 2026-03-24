import type {
  PaneNode,
  SplitNode,
  LeafNode,
  Tab,
  RequestState,
} from '@/types/pane-types';

// Creates a blank GET request with no params, headers, body, or auth.
export function createDefaultRequest(): RequestState {
  return {
    method: 'GET',
    url: '',
    pathParams: [],
    queryParams: [],
    headers: [],
    body: {
      mode: 'none',
      content: '',
      formData: [],
    },
    auth: {
      authType: 'none',
    },
  };
}

// Creates a new unsaved draft tab with a fresh UUID.
export function createDefaultTab(): Tab {
  const id = crypto.randomUUID();
  return {
    id,
    title: 'New Request',
    tabType: 'draft',
    request: createDefaultRequest(),
    response: null,
    isDirty: false,
  };
}

// Creates a leaf pane containing one default tab.
export function createDefaultLeaf(groupId?: string): LeafNode {
  const tab = createDefaultTab();
  return {
    type: 'leaf',
    id: crypto.randomUUID(),
    groupId: groupId ?? crypto.randomUUID(),
    tabs: [tab],
    activeTabId: tab.id,
  };
}

// Searches the tree depth-first and returns the leaf matching groupId.
export function findLeaf(node: PaneNode, groupId: string): LeafNode | null {
  if (node.type === 'leaf') {
    return node.groupId === groupId ? node : null;
  }
  return findLeaf(node.children[0], groupId) ?? findLeaf(node.children[1], groupId);
}

// Searches all leaves depth-first for a tab by id.
export function findTabInTree(
  node: PaneNode,
  tabId: string,
): { leaf: LeafNode; tab: Tab } | null {
  if (node.type === 'leaf') {
    const tab = node.tabs.find((t) => t.id === tabId);
    return tab ? { leaf: node, tab } : null;
  }
  return findTabInTree(node.children[0], tabId) ?? findTabInTree(node.children[1], tabId);
}

// Returns the leftmost/topmost leaf in the tree.
export function findFirstLeaf(node: PaneNode): LeafNode {
  if (node.type === 'leaf') return node;
  return findFirstLeaf(node.children[0]);
}

// Returns the leaf matching activeGroupId, falling back to the first leaf.
export function findActiveLeaf(node: PaneNode, activeGroupId: string): LeafNode {
  return findLeaf(node, activeGroupId) ?? findFirstLeaf(node);
}

// Immutably replaces the leaf identified by groupId using the updater function.
export function updateLeaf(
  node: PaneNode,
  groupId: string,
  updater: (leaf: LeafNode) => LeafNode,
): PaneNode {
  if (node.type === 'leaf') {
    return node.groupId === groupId ? updater(node) : node;
  }
  const left = updateLeaf(node.children[0], groupId, updater);
  const right = updateLeaf(node.children[1], groupId, updater);
  // Skip creating a new object when nothing changed.
  if (left === node.children[0] && right === node.children[1]) return node;
  return { ...node, children: [left, right] } satisfies SplitNode;
}

// Removes a leaf by groupId and collapses the parent split to its sibling.
export function removeLeaf(node: PaneNode, groupId: string): PaneNode {
  if (node.type === 'leaf') {
    // Callers must not remove the root leaf directly.
    return node;
  }
  const [left, right] = node.children;

  // If the left child is the target, collapse to the right sibling.
  if (left.type === 'leaf' && left.groupId === groupId) return right;
  // If the right child is the target, collapse to the left sibling.
  if (right.type === 'leaf' && right.groupId === groupId) return left;

  // Recurse into the subtree and rebuild.
  return {
    ...node,
    children: [removeLeaf(left, groupId), removeLeaf(right, groupId)],
  } satisfies SplitNode;
}

// Splits the leaf identified by groupId into two panes along direction.
export function splitLeaf(
  node: PaneNode,
  groupId: string,
  direction: 'horizontal' | 'vertical',
): PaneNode {
  if (node.type === 'leaf') {
    if (node.groupId !== groupId) return node;
    const newLeaf = createDefaultLeaf();
    return {
      type: 'split',
      id: crypto.randomUUID(),
      direction,
      children: [node, newLeaf],
      sizes: [50, 50],
    } satisfies SplitNode;
  }
  const left = splitLeaf(node.children[0], groupId, direction);
  const right = splitLeaf(node.children[1], groupId, direction);
  if (left === node.children[0] && right === node.children[1]) return node;
  return { ...node, children: [left, right] } satisfies SplitNode;
}
