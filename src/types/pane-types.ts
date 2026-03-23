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

export interface Tab {
  id: string;
  title: string;
  tabType: 'request' | 'draft' | 'history';
  request: RequestState;
  response: ResponseState | null;
  isDirty: boolean;
  source?: { collection: string; path: string };
}

export interface RequestState {
  method: HttpMethod;
  url: string;
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
}

export interface AuthState {
  authType: 'none' | 'basic' | 'bearer' | 'api-key' | 'oauth2' | 'aws-sig-v4';
  basic?: { username: string; password: string };
  bearer?: { token: string };
  apiKey?: { key: string; value: string; addTo: 'header' | 'query' };
  oauth2?: {
    grantType: 'client_credentials' | 'password' | 'authorization_code';
    clientId: string;
    clientSecret: string;
    tokenUrl: string;
    scope: string;
    accessToken: string;
    refreshToken: string;
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
