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

export interface ChatMessage {
  id: string;
  role: 'user' | 'agent';
  text: string;
  streaming?: boolean;
}

export interface AgentChatSession {
  agentConfigId: string;
  sessionId: string;
  status: 'starting' | 'active' | 'ended' | 'error';
  messages: ChatMessage[];
  error?: string;
}

export interface RequestTab extends BaseTab {
  tabType: 'request' | 'history';
  request: RequestState;
  response: ResponseState | null;
  agentSession?: AgentChatSession;
}

export type CollectionSection = 'overview' | 'auth' | 'variables' | 'documentation';

export interface CollectionTab extends BaseTab {
  tabType: 'collection';
  collectionName: string;
  activeSection?: CollectionSection;
}

export type FolderSection = 'headers' | 'script' | 'test' | 'vars' | 'auth' | 'docs';

export interface FolderTab extends BaseTab {
  tabType: 'folder';
  collectionName: string;
  /** Folder path relative to the collection root, for example `auth/oauth`. */
  folderPath: string;
  activeSection: FolderSection;
}

export type WorkspaceTabSection = 'overview' | 'environments' | 'git' | 'audit';

export interface WorkspaceTab extends BaseTab {
  tabType: 'workspace';
  workspaceId: string;
  activeSection: WorkspaceTabSection;
}

export interface DiffState {
  filePath: string;
  repositoryId: string;
  repositoryLabel: string;
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
  repositoryId: string;
  repositoryLabel: string;
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
  repositoryId: string;
  repositoryLabel: string;
}

export interface ContractTab extends BaseTab {
  tabType: 'contract';
  collectionName: string;
  collectionRoot: string; // absolute path — required for all IPC calls
  initialScope?: import('@/lib/tauri-api').ContractScope;
}

export function isContractTab(tab: Tab): tab is ContractTab {
  return tab.tabType === 'contract';
}

export interface ContractDiffTab extends BaseTab {
  tabType: 'contract_diff';
  collectionId: string; // absolute path — passed as collectionRoot to IPC
  contractId: string;
}

export function isContractDiffTab(tab: Tab): tab is ContractDiffTab {
  return tab.tabType === 'contract_diff';
}

export type RunnerRunState = 'idle' | 'running' | 'stopped' | 'done';

export interface RunnerRequestEntry {
  requestPath: string;
  // Full backend Request, not just name/method — startRun (Plan C) needs
  // the whole object to call executeRunnerEntry. Components read
  // entry.request.name / entry.request.method for display.
  request: import('@/lib/tauri-api').Request;
  /** Present for a GraphQL item; `request` is then only its display and HTTP-shaped form. */
  graphql?: import('@/lib/tauri-api').GraphQlRequest;
  included: boolean;
  status: 'pending' | 'running' | 'passed' | 'failed' | 'skipped';
  result?: import('@/lib/tauri-api').ExecuteRequestResponse;
  error?: string;
}

export interface RunnerTab extends BaseTab {
  tabType: 'runner';
  collectionName: string | null;
  folderPath?: string;
  runState: RunnerRunState;
  requests: RunnerRequestEntry[];
  // Monotonic id for the current/most-recent run, assigned fresh by every
  // startRun call. A run loop's writes (per-entry status/result, the
  // finalizer) only apply while this still matches the id the loop was
  // started with — this is what makes Stop followed immediately by Re-run
  // safe: the old loop's next write sees a mismatched id and becomes a
  // no-op instead of racing the new run and overwriting its results.
  runId?: number;
}

export function isRunnerTab(tab: Tab): tab is RunnerTab {
  return tab.tabType === 'runner';
}

/** Per-node result of the last run. Every field is optional. */
export interface FlowNodeDetail {
  statusCode?: number;
  durationMs?: number;
  error?: string;
  /** Captured value shown by an Output node. */
  value?: string;
  skipReason?: import('@/lib/tauri-api').FlowSkipReason;
  /** Exit a routing node took. */
  branch?: string;
  /** Progress text of a running node, such as "attempt 3/30". */
  progress?: string;
  /** Attempts a repeat-until Request node made. */
  attempts?: number;
  /** Masked request and response of the last run, for Request and Wait nodes. */
  exchange?: import('@/lib/tauri-api').FlowDebugRequest;
  /** Script console output of the last run. */
  logs?: import('@/lib/tauri-api').FlowLogEntry[];
}

/** Outcome of the last finished run, shown in the run-result strip. */
export interface FlowLastRun {
  runId: string;
  /** `completed`, `cancelled`, `error`, or another backend reason. */
  stoppedReason: string;
  /** Wall-clock time in ms. Null when this client did not time the run. */
  totalMs: number | null;
  /** First node that failed. Absent for a clean or cancelled run. */
  failedNodeId?: string;
  /** Label of that node when the run ended, kept even if the node is renamed later. */
  failedLabel?: string;
  failedCount: number;
  /** Nodes skipped because an upstream node failed. Not-taken branches are excluded. */
  skippedCount: number;
}

export interface FlowTab extends BaseTab {
  tabType: 'flow';
  collectionName: string | null;
  flowName: string | null;
  nodes: import('@/lib/tauri-api').FlowNode[];
  edges: import('@/lib/tauri-api').FlowEdge[];
  /** Host used in callback URLs. Null or absent means this machine's LAN IP. */
  callbackHost?: string | null;
  nodeStatus: Record<string, import('@/lib/tauri-api').FlowNodeStatus>;
  nodeDetail?: Record<string, FlowNodeDetail>;
  runState: 'idle' | 'running' | 'done';
  runId?: string;
  /** Undo and redo steps for the graph. In memory only, never saved. */
  history?: import('@/lib/flow-history').FlowHistory;
  /** Result of the last finished run. Cleared when the next run starts. */
  lastRun?: FlowLastRun;
}

export function isFlowTab(tab: Tab): tab is FlowTab {
  return tab.tabType === 'flow';
}

export interface ScriptTab extends BaseTab {
  tabType: 'script';
  collectionName: string;
  /** Collection-relative path, for example `lib/utils.js`. */
  scriptPath: string;
  content: string;
  savedContent: string;
}

export function isScriptTab(tab: Tab): tab is ScriptTab {
  return tab.tabType === 'script';
}

export function isFolderTab(tab: Tab): tab is FolderTab {
  return tab.tabType === 'folder';
}

export function isCollectionTab(tab: Tab): tab is CollectionTab {
  return tab.tabType === 'collection';
}

export type Tab =
  | RequestTab
  | CollectionTab
  | WorkspaceTab
  | DiffTab
  | ConflictTab
  | GitTab
  | ContractTab
  | ContractDiffTab
  | RunnerTab
  | FlowTab
  | ScriptTab
  | FolderTab;

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

export interface RequestSettings {
  verifySsl: boolean;
  followRedirects: boolean;
  maxRedirects: number;
  timeoutMs: number;
  encodeUrl: boolean;
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
  settings: RequestSettings;
  docs: string | null;
  tags: string[];
  preRequestScript?: string;
  postResponseScript?: string;
  testsScript?: string;
  assertions: import('@/lib/tauri-api').AssertionEntry[];
  actions: import('@/lib/tauri-api').ActionEntry[];
  /** Present when `requestType` is 'graphql'. */
  graphql?: GraphQlState;
  /** Present when `requestType` is 'websocket'. */
  websocket?: WebSocketDraft;
  /** Present when `requestType` is 'grpc'. The URL, metadata (as `headers`) and auth live in the shared fields. */
  grpc?: GrpcState;
}

/** One saved message of a gRPC request. */
export interface GrpcMessageState {
  id: string;
  title: string;
  /** Protobuf JSON text. */
  content: string;
}

/** Saved fields the gRPC editor never changes. They are sent back unchanged, so a save loses nothing. */
export interface GrpcPassthrough {
  seq?: number;
  description?: unknown;
  scripts?: import('@/lib/tauri-api').GrpcScript[];
}

export interface GrpcState {
  /** `package.Service/Method`. Empty until a method is picked. */
  method: string;
  methodType: import('@/lib/tauri-api').GrpcMethodType;
  /** Path of the `.proto` file. Empty means use server reflection. */
  protoFilePath: string;
  messages: GrpcMessageState[];
  /** Index of the message the editor shows. It is also the one a save marks as selected. */
  activeMessage: number;
  passthrough: GrpcPassthrough;
}

export interface WebSocketDraftMessage {
  id: string;
  title: string;
  selected: boolean;
  kind: import('@/lib/tauri-api').WebSocketMessageKind;
  /** Text as is. For `binary` this is base64. */
  data: string;
}

/**
 * Saved WebSocket fields with no editor yet. They are kept as loaded and written back unchanged,
 * so saving from the UI never drops them. Runtime variables are not here: they are edited and
 * saved through their own commands, and the backend keeps them when a save carries none.
 */
export interface WebSocketPassthrough {
  description?: unknown;
  seq?: number;
  runtimeAuth?: import('@/lib/tauri-api').Auth;
  scripts?: { scriptType: string; code: string }[];
}

export interface WebSocketDraft {
  messages: WebSocketDraftMessage[];
  /** Connect timeout in ms, or 'inherit'. */
  timeoutMs: number | 'inherit';
  /** Ms between pings, or 'inherit' (none). */
  keepAliveMs: number | 'inherit';
  passthrough: WebSocketPassthrough;
}

export interface GraphQlState {
  query: string;
  /** JSON text. Empty means no variables. */
  variables: string;
  /** The operation to run when the document defines several. Session state, never saved. */
  operationName?: string;
  /** JSON object text sent with `connection_init` for subscriptions. Session state, never saved. */
  connectionParams?: string;
  /** Stored body variants, round-tripped so a save keeps them. */
  bodyVariants?: import('@/lib/tauri-api').GraphQlBodyVariant[];
}

export interface KeyValueEntry {
  id: string;
  key: string;
  value: string;
  enabled: boolean;
  /** Multipart rows only: whether `value` is text or the path of a file to upload. */
  entryType?: 'text' | 'file';
  /** Multipart rows only: the part's Content-Type. Empty means "decide automatically". */
  contentType?: string;
}

export interface BodyState {
  mode: 'none' | 'json' | 'xml' | 'text' | 'sparql' | 'formdata' | 'formurlencoded' | 'binary';
  content: string;
  formData: KeyValueEntry[];
  filePath?: string;
  fileName?: string;
}

export interface OAuth2AdditionalParam {
  key: string;
  value: string;
  sendIn: 'queryparams' | 'body';
  enabled: boolean;
}

export interface OAuth2JwtClaims {
  subject: string | null;
  issuer: string | null;
  audience: string | null;
  expiry: number | null;
  issuedAt: number | null;
  scope: string | null;
  tokenType: string | null;
  algorithm: string | null;
  rawPayload: string;
}

export interface AuthState {
  authType:
    | 'inherit'
    | 'none'
    | 'basic'
    | 'bearer'
    | 'api-key'
    | 'oauth2'
    | 'aws-sig-v4'
    | 'digest'
    | 'wsse'
    | 'ntlm'
    | 'oauth1';
  basic?: { username: string; password: string };
  digest?: { username: string; password: string };
  wsse?: { username: string; password: string };
  ntlm?: { username: string; password: string; domain: string };
  // Persisted OAuth 1.0 fields as stored. Unknown fields are kept on save.
  oauth1?: Record<string, unknown>;
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

    // Options
    usePkce: boolean;
    useSystemBrowser: boolean;

    // Token section
    tokenSource: 'accessToken' | 'idToken';
    tokenId: string;

    // Advanced
    refreshTokenUrl: string;

    // Settings
    autoFetchToken: boolean;
    autoRefreshToken: boolean;

    // Additional parameters
    authParams: OAuth2AdditionalParam[];
    tokenParams: OAuth2AdditionalParam[];
    refreshParams: OAuth2AdditionalParam[];

    // Token response storage (ephemeral — NOT persisted).
    idToken: string;
    tokenType: string;
    responseScope: string;
    idTokenClaims: OAuth2JwtClaims | null;
    accessTokenClaims: OAuth2JwtClaims | null;
    forceReauth?: boolean;
  };
  awsSigV4?: {
    accessKey: string;
    secretKey: string;
    region: string;
    service: string;
    sessionToken: string;
    /** Profile of the shared AWS credentials file, used when the keys above are empty. */
    profileName?: string;
  };
}

export interface ResponseState {
  status: number;
  statusText: string;
  headers: KeyValueEntry[];
  body: string;
  durationMs: number;
  ttfbMs: number;
  sizeBytes: number;
  isBinary?: boolean;
  bodyBase64?: string;
  activeView: 'pretty' | 'raw' | 'preview' | 'headers' | 'tests' | 'data' | 'errors';
  /** Set for a response to a GraphQL request, which adds the Data and Errors tabs. */
  protocol?: 'graphql';
  testResults?: import('@/lib/tauri-api').TestResult[];
  consoleEntries?: import('@/lib/tauri-api').ConsoleEntry[];
  scriptError?: string | null;
}

// Standard methods or any custom method token, as the backend serializes it.
export type HttpMethod = string;
