import { type ApiOAuth2Auth, apiAuthToOAuth2State } from '@/lib/oauth2-mapping';
import type { Request as ApiRequest } from '@/lib/tauri-api';
import { extractPathParams, parseQueryParams } from '@/lib/url-params';
import type {
  AuthState,
  BodyState,
  LeafNode,
  PaneNode,
  RequestState,
  SplitNode,
  Tab,
} from '@/types/pane-types';

// Maps an API Request (from the Tauri backend) to the frontend RequestState shape.
export function mapApiRequestToState(req: ApiRequest, fromCollection = false): RequestState {
  // Map auth from the tagged-union API type to the frontend AuthState.
  let auth: AuthState;
  switch (req.auth.authType) {
    case 'basic':
      auth = {
        authType: 'basic',
        basic: { username: req.auth.username, password: req.auth.password },
      };
      break;
    case 'bearer':
      auth = { authType: 'bearer', bearer: { token: req.auth.token } };
      break;
    case 'api-key':
      auth = {
        authType: 'api-key',
        apiKey: { key: req.auth.key, value: req.auth.value, addTo: req.auth.placement },
      };
      break;
    case 'o-auth2':
      auth = {
        authType: 'oauth2',
        oauth2: apiAuthToOAuth2State(req.auth as unknown as ApiOAuth2Auth),
      };
      break;
    case 'aws-sig-v4': {
      const a = req.auth as Record<string, unknown>;
      auth = {
        authType: 'aws-sig-v4',
        awsSigV4: {
          accessKey: (a.accessKey as string) ?? '',
          secretKey: (a.secretKey as string) ?? '',
          region: (a.region as string) ?? '',
          service: (a.service as string) ?? '',
          sessionToken: (a.sessionToken as string) ?? '',
        },
      };
      break;
    }
    default:
      auth = { authType: fromCollection ? 'inherit' : 'none' };
  }

  // Map body from the optional API Body to the always-present frontend BodyState.
  let body: BodyState;
  if (req.body) {
    body = {
      mode: req.body.mode as BodyState['mode'],
      content: req.body.content ?? '',
      formData: (req.body.formData ?? []).map((entry) => ({
        id: crypto.randomUUID(),
        key: entry.key,
        value: entry.value,
        enabled: entry.enabled,
      })),
    };
  } else {
    body = { mode: 'none', content: '', formData: [] };
  }

  return {
    requestType: 'http',
    method: req.method as RequestState['method'],
    url: req.url,
    queryParams: parseQueryParams(req.url),
    pathParams: extractPathParams(req.url).map((name) => ({
      id: crypto.randomUUID(),
      key: name,
      value: '',
      enabled: true,
    })),
    headers: req.headers.map((h) => ({
      id: crypto.randomUUID(),
      key: h.key,
      value: h.value,
      enabled: h.enabled,
    })),
    body,
    auth,
    settings: {
      verifySsl: req.settings?.verifySsl ?? true,
      followRedirects: req.settings?.followRedirects ?? true,
      maxRedirects: req.settings?.maxRedirects ?? 5,
      timeoutMs: req.settings?.timeout ?? 0,
      encodeUrl: req.settings?.encodeUrl ?? true,
    },
    tags: req.tags ?? [],
    docs: req.docs ?? null,
    preRequestScript: req.preRequestScript ?? undefined,
    postResponseScript: req.postResponseScript ?? undefined,
    testsScript: req.tests ?? undefined,
  };
}

// Creates a blank GET request with no params, headers, body, or auth.
export function createDefaultRequest(): RequestState {
  return {
    requestType: 'http',
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
    settings: {
      verifySsl: true,
      followRedirects: true,
      maxRedirects: 5,
      timeoutMs: 0,
      encodeUrl: true,
    },
    tags: [],
    docs: null,
  };
}

// Creates a leaf pane, empty by default (shows branded empty state).
export function createDefaultLeaf(groupId?: string): LeafNode {
  return {
    type: 'leaf',
    id: crypto.randomUUID(),
    groupId: groupId ?? crypto.randomUUID(),
    tabs: [],
    activeTabId: '',
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
export function findTabInTree(node: PaneNode, tabId: string): { leaf: LeafNode; tab: Tab } | null {
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
