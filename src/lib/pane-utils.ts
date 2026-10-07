import { fromPersistedAuth } from '@/lib/persisted-auth';
import type {
  Request as ApiRequest,
  GraphQlRequest,
  GrpcRequest,
  RequestKind,
} from '@/lib/tauri-api';
import { extractPathParams, parseQueryParams } from '@/lib/url-params';
import { createDefaultWebSocketDraft } from '@/lib/websocket-messages';
import type {
  BodyState,
  FolderTab,
  GrpcState,
  LeafNode,
  PaneNode,
  RequestState,
  ScriptTab,
  SplitNode,
  Tab,
} from '@/types/pane-types';
import { isFolderTab, isScriptTab } from '@/types/pane-types';

// Maps an API Request (from the Tauri backend) to the frontend RequestState shape.
export function mapApiRequestToState(req: ApiRequest, fromCollection = false): RequestState {
  const auth = fromPersistedAuth(req.auth, fromCollection ? 'inherit' : 'none');

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
        ...(entry.entryType === 'file' ? { entryType: 'file' as const } : {}),
        ...(entry.contentType ? { contentType: entry.contentType } : {}),
      })),
      filePath: req.body.filePath,
      fileName: req.body.filePath?.split(/[\\/]/).pop(),
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
      value: req.pathParams?.find((p) => p.name === name)?.value ?? '',
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
    assertions: req.assertions ?? [],
    actions: req.actions ?? [],
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
    assertions: [],
    actions: [],
  };
}

// A valid document on every GraphQL server, used to seed a new request.
export const DEFAULT_GRAPHQL_QUERY = '{\n  __typename\n}\n';

// Maps a saved GraphQL request to the tab state. The shared fields reuse the HTTP mapping.
export function mapGraphQlToState(g: GraphQlRequest): RequestState {
  const { body, bodyVariants, ...rest } = g;
  const base = mapApiRequestToState(rest as ApiRequest, true);
  return {
    ...base,
    requestType: 'graphql',
    method: g.method as RequestState['method'],
    graphql: {
      query: body.query,
      variables: body.variables ?? '',
      ...(bodyVariants && bodyVariants.length > 0 ? { bodyVariants } : {}),
    },
  };
}

// The editor state of a gRPC request that has nothing saved yet.
export function createDefaultGrpcState(): GrpcState {
  return {
    method: '',
    methodType: 'unary',
    protoFilePath: '',
    messages: [{ id: crypto.randomUUID(), title: '', content: DEFAULT_GRPC_MESSAGE }],
    activeMessage: 0,
    passthrough: {},
  };
}

// An empty message is valid protobuf JSON for every message type.
export const DEFAULT_GRPC_MESSAGE = '{}';

// Maps a saved gRPC request to the tab state. The URL, metadata, auth, tags and docs reuse the
// HTTP fields, so the metadata and auth editors work unchanged.
export function mapGrpcToState(g: GrpcRequest): RequestState {
  const saved = g.messages ?? [];
  const messages = saved.map((m) => ({
    id: crypto.randomUUID(),
    title: m.title,
    content: m.content,
  }));
  const selected = saved.findIndex((m) => m.selected);
  return {
    ...createDefaultRequest(),
    requestType: 'grpc',
    method: 'POST',
    url: g.url,
    headers: (g.metadata ?? []).map((h) => ({
      id: crypto.randomUUID(),
      key: h.key,
      value: h.value,
      enabled: h.enabled,
    })),
    auth: fromPersistedAuth(g.auth, 'inherit'),
    tags: g.tags ?? [],
    docs: g.docs ?? null,
    assertions: g.assertions ?? [],
    grpc: {
      method: g.method ?? '',
      methodType: g.methodType,
      protoFilePath: g.protoFilePath ?? '',
      messages:
        messages.length > 0
          ? messages
          : [{ id: crypto.randomUUID(), title: '', content: DEFAULT_GRPC_MESSAGE }],
      activeMessage: selected >= 0 ? selected : 0,
      passthrough: { seq: g.seq, description: g.description, scripts: g.scripts },
    },
  };
}

// Builds a blank request of the given kind. GraphQL, gRPC and WebSocket have their own editor state.
export function createDefaultRequestFor(kind: RequestKind): RequestState {
  const base = createDefaultRequest();
  if (kind === 'graphql') {
    return {
      ...base,
      requestType: 'graphql',
      method: 'POST',
      graphql: { query: DEFAULT_GRAPHQL_QUERY, variables: '' },
    };
  }
  if (kind === 'websocket') {
    return { ...base, requestType: 'websocket', websocket: createDefaultWebSocketDraft() };
  }
  if (kind === 'grpc') {
    return { ...base, requestType: 'grpc', method: 'POST', grpc: createDefaultGrpcState() };
  }
  return { ...base, requestType: kind };
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

// True when `path` is `folder` itself or lives below it, matching whole path segments.
export function isPathWithin(path: string, folder: string): boolean {
  return path === folder || path.startsWith(`${folder}/`);
}

// Collects every open script tab of a collection whose path is within `path`.
export function findScriptTabsWithin(
  node: PaneNode,
  collection: string,
  path: string,
): ScriptTab[] {
  if (node.type !== 'leaf') {
    return [
      ...findScriptTabsWithin(node.children[0], collection, path),
      ...findScriptTabsWithin(node.children[1], collection, path),
    ];
  }
  return node.tabs.filter(
    (tab): tab is ScriptTab =>
      isScriptTab(tab) && tab.collectionName === collection && isPathWithin(tab.scriptPath, path),
  );
}

// Finds the open script tab for a collection-relative path, if any.
export function findScriptTab(
  node: PaneNode,
  collection: string,
  path: string,
): { leaf: LeafNode; tab: ScriptTab } | null {
  if (node.type === 'leaf') {
    for (const tab of node.tabs) {
      if (isScriptTab(tab) && tab.collectionName === collection && tab.scriptPath === path) {
        return { leaf: node, tab };
      }
    }
    return null;
  }
  return (
    findScriptTab(node.children[0], collection, path) ??
    findScriptTab(node.children[1], collection, path)
  );
}

// Collects every open folder tab of a collection whose folder is `folderPath` or below it.
export function findFolderTabsWithin(
  node: PaneNode,
  collection: string,
  folderPath: string,
): FolderTab[] {
  if (node.type !== 'leaf') {
    return [
      ...findFolderTabsWithin(node.children[0], collection, folderPath),
      ...findFolderTabsWithin(node.children[1], collection, folderPath),
    ];
  }
  return node.tabs.filter(
    (tab): tab is FolderTab =>
      isFolderTab(tab) &&
      tab.collectionName === collection &&
      isPathWithin(tab.folderPath, folderPath),
  );
}

// Finds the open folder tab for a collection and folder path, if any.
export function findFolderTab(
  node: PaneNode,
  collection: string,
  folderPath: string,
): { leaf: LeafNode; tab: FolderTab } | null {
  if (node.type === 'leaf') {
    for (const tab of node.tabs) {
      if (isFolderTab(tab) && tab.collectionName === collection && tab.folderPath === folderPath) {
        return { leaf: node, tab };
      }
    }
    return null;
  }
  return (
    findFolderTab(node.children[0], collection, folderPath) ??
    findFolderTab(node.children[1], collection, folderPath)
  );
}

// Returns the leftmost/topmost leaf in the tree.
export function findFirstLeaf(node: PaneNode): LeafNode {
  if (node.type === 'leaf') return node;
  return findFirstLeaf(node.children[0]);
}

// Collects all leaf groupIds from the pane tree.
export function collectLeafGroupIds(node: PaneNode): string[] {
  if (node.type === 'leaf') return [node.groupId];
  return [...collectLeafGroupIds(node.children[0]), ...collectLeafGroupIds(node.children[1])];
}

// Collects every tab from every leaf in the pane tree, in depth-first order.
export function collectAllTabs(node: PaneNode): Tab[] {
  if (node.type === 'leaf') return node.tabs;
  return [...collectAllTabs(node.children[0]), ...collectAllTabs(node.children[1])];
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
