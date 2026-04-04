// Recursive split tree node — either a container split or a leaf with tabs.
export type PaneNode = SplitNode | LeafNode;

export interface SplitNode {
  type: 'split';
  id: string;
  direction: 'horizontal' | 'vertical';
  children: [PaneNode, PaneNode];
  // Sizes in percentage, must sum to 100 (e.g. [50, 50]).
  sizes: [number, number];
}

export interface LeafNode {
  type: 'leaf';
  id: string;
  groupId: string;
  tabs: Tab[];
  activeTabId: string;
}

interface BaseTab {
  id: string;
  title: string;
  isDirty: boolean;
  source?: { collection: string; path: string };
}

export interface RequestTab extends BaseTab {
  tabType: 'request' | 'history';
  request: RequestState;
  response: ResponseState | null;
}

export type CollectionSection = 'overview' | 'auth' | 'variables' | 'readme' | 'tags';

export interface CollectionTab extends BaseTab {
  tabType: 'collection';
  collectionName: string;
  activeSection?: CollectionSection;
}

export type WorkspaceTabSection = 'overview' | 'environments' | 'git';

export interface WorkspaceTab extends BaseTab {
  tabType: 'workspace';
  workspaceId: string;
  activeSection: WorkspaceTabSection;
}

export interface DiffState {
  filePath: string;
  collectionPath: string;
  oldContent: string;
  newContent: string;
  status: string;
  isStaged: boolean;
}

export interface DiffTab extends BaseTab {
  tabType: 'diff';
  diffState: DiffState;
}

export interface ConflictState {
  filePath: string;
  collectionPath: string;
  ours: string;
  theirs: string;
  ancestor: string | null;
}

export interface ConflictTab extends BaseTab {
  tabType: 'conflict';
  conflictState: ConflictState;
}

export interface GitTab extends BaseTab {
  tabType: 'git';
  collectionName: string;
  collectionPath: string;
}

export type Tab = RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab;

export function isWorkspaceTab(tab: Tab): tab is WorkspaceTab {
  return tab.tabType === 'workspace';
}

export function isRequestTab(tab: Tab): tab is RequestTab {
  return tab.tabType === 'request' || tab.tabType === 'history';
}

export function isDiffTab(tab: Tab): tab is DiffTab {
  return tab.tabType === 'diff';
}

export function isConflictTab(tab: Tab): tab is ConflictTab {
  return tab.tabType === 'conflict';
}

export function isGitTab(tab: Tab): tab is GitTab {
  return tab.tabType === 'git';
}

export interface RequestState {
  requestType: 'http' | 'graphql' | 'grpc' | 'websocket';
  method: HttpMethod;
  url: string;
  pathParams: KeyValueEntry[];
  queryParams: KeyValueEntry[];
  headers: KeyValueEntry[];
  body: BodyState;
  auth: AuthState;
  docs: string | null;
}

export interface KeyValueEntry {
  id: string;
  key: string;
  value: string;
  enabled: boolean;
}

export interface BodyState {
  mode: 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'binary';
  content: string;
  formData: KeyValueEntry[];
  filePath?: string;
  fileName?: string;
}

export interface AuthState {
  authType: 'inherit' | 'none' | 'basic' | 'bearer' | 'api-key' | 'oauth2' | 'aws-sig-v4';
  basic?: { username: string; password: string };
  bearer?: { token: string };
  apiKey?: { key: string; value: string; addTo: 'header' | 'query' };
  oauth2?: {
    grantType: 'client_credentials' | 'password' | 'authorization_code' | 'implicit';
    authorizationUrl: string;
    tokenUrl: string;
    callbackUrl: string;
    clientId: string;
    clientSecret: string;
    scope: string;
    state: string;
    username: string;
    password: string;
    clientAuthentication: 'header' | 'body';
    headerPrefix: string;
    addTokenTo: 'header' | 'queryParams';
    verifySsl: boolean;
    accessToken: string;
    refreshToken: string;
    expiresIn: number | null;
    tokenAcquiredAt: number | null;
  };
  awsSigV4?: {
    accessKey: string;
    secretKey: string;
    region: string;
    service: string;
    sessionToken: string;
  };
}

export interface ResponseState {
  status: number;
  statusText: string;
  headers: KeyValueEntry[];
  body: string;
  durationMs: number;
  sizeBytes: number;
  activeView: 'pretty' | 'raw' | 'preview' | 'headers';
}

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'OPTIONS' | 'HEAD';
