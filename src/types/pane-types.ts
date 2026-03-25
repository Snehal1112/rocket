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
  tabType: 'request' | 'draft' | 'history';
  request: RequestState;
  response: ResponseState | null;
}

export interface CollectionTab extends BaseTab {
  tabType: 'collection';
  collectionName: string;
}

export type Tab = RequestTab | CollectionTab;

export function isRequestTab(tab: Tab): tab is RequestTab {
  return tab.tabType !== 'collection';
}

export interface RequestState {
  method: HttpMethod;
  url: string;
  pathParams: KeyValueEntry[];
  queryParams: KeyValueEntry[];
  headers: KeyValueEntry[];
  body: BodyState;
  auth: AuthState;
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
  authType: 'none' | 'basic' | 'bearer' | 'api-key' | 'oauth2' | 'aws-sig-v4';
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

export type HttpMethod =
  | 'GET'
  | 'POST'
  | 'PUT'
  | 'PATCH'
  | 'DELETE'
  | 'OPTIONS'
  | 'HEAD';
