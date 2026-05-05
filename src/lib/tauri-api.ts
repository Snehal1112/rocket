/**
 * TypeScript bridge — wraps every Tauri command with a typed invoke() call.
 * All types mirror the Rust structs (camelCase via serde rename_all).
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { OAuth2AdditionalParam, OAuth2JwtClaims } from '@/types/pane-types';

// ============================================================
// Domain types (mirror Rust structs)
// ============================================================

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'OPTIONS' | 'HEAD';

export interface Header {
  key: string;
  value: string;
  enabled: boolean;
}

export type BodyMode = 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'binary';

export interface FormDataEntry {
  key: string;
  value: string;
  entryType: 'text' | 'file';
  enabled: boolean;
}

export interface Body {
  mode: BodyMode;
  content?: string;
  formData?: FormDataEntry[];
}

export type Auth =
  | { authType: 'none' }
  | { authType: 'basic'; username: string; password: string }
  | { authType: 'bearer'; token: string }
  | { authType: 'api-key'; key: string; value: string; placement: 'header' | 'query' }
  | { authType: 'o-auth2'; [key: string]: unknown }
  | { authType: 'aws-sig-v4'; [key: string]: unknown };

export interface RequestOptions {
  followRedirects: boolean;
  timeoutMs: number;
  verifySsl: boolean;
}

export interface CollectionVariable {
  key: string;
  value: string;
  initialValue: string;
  enabled: boolean;
  secret: boolean;
}

export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
}

export interface CollectionSummary {
  uid: string;
  name: string;
  path: string;
  requestCount: number;
  modifiedAt?: string;
  /** "embedded" (default) or "external" — set by the workspace layer. */
  refType?: string;
}

/** Per-request execution settings persisted to the collection YAML. */
export interface ApiRequestSettings {
  /** Timeout in milliseconds. */
  timeout?: number;
  followRedirects?: boolean;
  verifySsl?: boolean;
  maxRedirects?: number;
  encodeUrl?: boolean;
}

export interface Request {
  uid: string;
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  body?: Body;
  auth: Auth;
  fileName?: string;
  tags?: string[];
  docs?: string | null;
  settings?: ApiRequestSettings;
}

export interface Folder {
  uid: string;
  name: string;
  /** Actual on-disk directory name. Use this (not name) for move/rename paths. */
  dirName?: string;
  items: CollectionItem[];
}

/** Lightweight sidebar placeholder — has no body/auth. Call getRequest to load the full item. */
export interface RequestSummary {
  uid: string;
  name: string;
  method: string;
  url: string;
  fileName?: string;
}

export type CollectionItem =
  | ({ type: 'request' } & Request)
  | ({ type: 'folder' } & Folder)
  | ({ type: 'summary' } & RequestSummary);

export interface Collection {
  name: string;
  root: Folder;
  settings: CollectionSettings;
}

export interface Variable {
  key: string;
  value: string;
  enabled: boolean;
  secret: boolean;
}

export interface Environment {
  name: string;
  variables: Variable[];
}

export interface Template {
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  body?: Body;
}

export interface HistoryEntry {
  id: string;
  method: string;
  url: string;
  status: number;
  durationMs: number;
  responseSize: number;
  timestamp: string;
  collection?: string;
  requestName?: string;
}

export interface HistoryFilter {
  method?: string;
  urlContains?: string;
  statusMin?: number;
  statusMax?: number;
}

export interface Cookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  expires?: string;
}

export interface CookieJar {
  domain: string;
  cookies: Cookie[];
}

export interface HttpResponse {
  status: number;
  statusText: string;
  headers: Header[];
  body: string;
  durationMs: number;
  ttfbMs: number;
  sizeBytes: number;
}

export interface QueryParam {
  key: string;
  value: string;
  enabled: boolean;
}

export interface ExecuteRequestInput {
  method: HttpMethod;
  url: string;
  headers: Header[];
  queryParams: QueryParam[];
  body?: Body;
  auth: Auth;
  options: RequestOptions;
  environmentName?: string;
  collection?: string;
  requestName?: string;
  /** Path of the request file relative to the collection root (e.g. "auth/login.yml"). */
  requestPath?: string;
}

export interface FileChangedEvent {
  path: string;
  eventType: 'create' | 'modify' | 'remove';
  collection?: string;
}

// ============================================================
// Git types
// ============================================================

export type GitStatusKind =
  | 'modified'
  | 'added'
  | 'deleted'
  | 'renamed'
  | 'untracked'
  | 'conflicted'
  | 'unchanged';

export interface FileStatus {
  path: string;
  status: GitStatusKind;
  staged: boolean;
}

export interface RepoStatus {
  branch: string;
  files: FileStatus[];
  ahead: number;
  behind: number;
  isClean: boolean;
}

export type LineType = 'context' | 'add' | 'remove';

export interface DiffLine {
  content: string;
  lineType: LineType;
}

export interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  oldContent?: string;
  newContent?: string;
  hunks: DiffHunk[];
}

export interface CommitInfo {
  id: string;
  fullId: string;
  message: string;
  author: string;
  authorEmail: string;
  timestamp: string;
  filesChanged: number;
}

export interface Branch {
  name: string;
  isHead: boolean;
  isRemote: boolean;
  upstream?: string;
}

export interface BranchList {
  current: string;
  local: Branch[];
  remote: Branch[];
}

export interface StashEntry {
  index: number;
  message: string;
  timestamp: string;
  branch: string;
  filesChanged: number;
  insertions: number;
  deletions: number;
  changedFiles: string[];
}

export interface ConflictFile {
  path: string;
  ours: string;
  theirs: string;
  ancestor?: string;
}

export type ConflictResolution =
  | { resolution: 'ours' }
  | { resolution: 'theirs' }
  | { resolution: 'custom'; content: string };

export type GitCredentials =
  | { type: 'sshKey'; privateKeyPath: string; passphrase?: string }
  | { type: 'sshAgent' }
  | { type: 'userPass'; username: string; password: string }
  | { type: 'token'; token: string };

export interface RemoteInfo {
  name: string;
  url: string;
}

export interface FetchResult {
  updatedRefs: string[];
  receivedObjects: number;
  receivedBytes: number;
}

export interface CollectionScanResult {
  name: string;
  path: string;
}

export interface ClonedRepoStructure {
  kind: 'workspace' | 'collection' | 'multi_collection' | 'unknown';
  workspacePath: string | null;
  collections: CollectionScanResult[];
}

// ============================================================
// Workspace types
// ============================================================

export interface Workspace {
  id: string;
  name: string;
  path: string;
  description?: string | null;
  pinned: boolean;
}

export interface CollectionReference {
  name: string;
  type: 'embedded' | 'external';
  path?: string;
}

export interface WorkspaceEnvironmentsConfig {
  activeEnvironment?: string | null;
}

export interface WorkspaceConfig {
  name: string;
  description?: string | null;
  collections: CollectionReference[];
  environments: WorkspaceEnvironmentsConfig;
}

// ============================================================
// Collections
// ============================================================

export const listCollections = () => invoke<CollectionSummary[]>('list_collections');

export const getCollection = (name: string) => invoke<Collection>('get_collection', { name });

export const createCollection = (name: string) => invoke<Collection>('create_collection', { name });

export const deleteCollection = (name: string) => invoke<void>('delete_collection', { name });

export const renameCollection = (oldName: string, newName: string) =>
  invoke<void>('rename_collection', { oldName, newName });

export const saveRequest = (collection: string, path: string, request: Request) =>
  invoke<Request>('save_request', { collection, path, request });

export const renameRequest = (collection: string, oldPath: string, newName: string) =>
  invoke<void>('rename_request', { collection, oldPath, newName });

export const deleteRequest = (collection: string, path: string) =>
  invoke<void>('delete_request', { collection, path });

export const createFolder = (collection: string, path: string) =>
  invoke<void>('create_folder', { collection, path });

export const deleteFolder = (collection: string, path: string) =>
  invoke<void>('delete_folder', { collection, path });

export const moveItem = (
  srcCollection: string,
  srcPath: string,
  dstCollection: string,
  dstPath: string,
) =>
  invoke<void>('move_item', {
    srcCollection,
    srcPath,
    dstCollection,
    dstPath,
  });

export async function reorderItems(
  collection: string,
  folderPath: string,
  orderedNames: string[],
): Promise<void> {
  return invoke('reorder_items', { collection, folderPath, orderedNames });
}

export const getCollectionSettings = (name: string) =>
  invoke<CollectionSettings>('get_collection_settings', { name });

export const saveCollectionSettings = (collection: string, settings: Partial<CollectionSettings>) =>
  invoke<void>('save_collection_settings', { collection, settings });

// ============================================================
// Environments
// ============================================================

export const listEnvironments = (collection: string) =>
  invoke<Environment[]>('list_environments', { collection });

export const getEnvironment = (collection: string, name: string) =>
  invoke<Environment>('get_environment', { collection, name });

export const saveEnvironment = (collection: string, env: Environment) =>
  invoke<void>('save_environment', { collection, env });

export const deleteEnvironment = (collection: string, name: string) =>
  invoke<void>('delete_environment', { collection, name });

export interface LoadTestConfig {
  concurrency: number;
  totalRequests: number;
  intervalMs: number;
  durationCapSecs?: number;
}

export interface LoadTestResult {
  totalRequests: number;
  succeeded: number;
  failed: number;
  failedTransport: number;
  failedStatus: number;
  minLatencyMs: number;
  avgLatencyMs: number;
  p50LatencyMs: number;
  p95LatencyMs: number;
  p99LatencyMs: number;
  maxLatencyMs: number;
  requestsPerSecond: number;
  totalDurationMs: number;
  phaseTimeline?: PhaseMarker[];
  requestLog?: RequestLogEntry[];
  timeSeries?: TimeSeriesPoint[];
}

// ---- Load Test v2 types ----

export type PhaseKind = 'RampUp' | 'Hold' | 'RampDown';

export type TargetUnit = 'concurrency' | 'rps';

export type PhaseTarget = { kind: 'concurrency'; value: number } | { kind: 'rps'; value: number };

export interface LoadTestPhase {
  kind: PhaseKind;
  durationSecs: number;
  target: PhaseTarget;
}

export interface SuccessRule {
  statusBelow: number;
}

export interface LoadTestConfigV2 {
  phases: LoadTestPhase[];
  successRule: SuccessRule;
  ringBufferSize: number;
}

export interface LoadTestProgressEvent {
  elapsedMs: number;
  completed: number;
  activeConcurrent: number;
  succeeded: number;
  failedStatus: number;
  failedTransport: number;
  requestsPerSecond: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  currentPhaseIndex: number;
  recentLog: RequestLogEntry[];
}

export interface TimeSeriesPoint {
  elapsedMs: number;
  rps: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  errorRatePct: number;
  activeConcurrent: number;
}

export interface RequestLogEntry {
  seq: number;
  status: number | null;
  latencyMs: number;
  responseBytes: number;
  error: string | null;
  phaseIndex: number;
}

export interface PhaseMarker {
  phaseIndex: number;
  startedAtMs: number;
}

export type ExportFormat = 'html' | 'csv' | 'json' | 'pdf';

// ============================================================
// Request execution
// ============================================================

export const executeRequest = (input: ExecuteRequestInput) =>
  invoke<HttpResponse>('execute_request', { input });

export const runLoadTest = (
  request: {
    method: HttpMethod;
    url: string;
    headers: Header[];
    queryParams: QueryParam[];
    body?: Body | null;
    auth: Auth;
    options: RequestOptions;
    collection?: string;
    environmentName?: string;
    requestPath?: string;
  },
  config: LoadTestConfig,
) => invoke<LoadTestResult>('run_load_test_command', { input: request, config });

export const runLoadTestV2 = (
  request: {
    method: HttpMethod;
    url: string;
    headers: Header[];
    queryParams: QueryParam[];
    body?: Body | null;
    auth: Auth;
    options: RequestOptions;
    collection?: string;
    environmentName?: string;
    requestPath?: string;
  },
  config: LoadTestConfigV2,
) => invoke<void>('run_load_test_v2_command', { input: request, config });

export const exportLoadTest = (result: LoadTestResult, format: ExportFormat) =>
  invoke<[string, string]>('export_load_test', { result, format });

// ============================================================
// History
// ============================================================

export const listHistory = (limit?: number) => invoke<HistoryEntry[]>('list_history', { limit });

export const getHistoryEntry = (id: string) => invoke<HistoryEntry>('get_history_entry', { id });

export const clearHistory = () => invoke<void>('clear_history');

export const searchHistory = (filter: HistoryFilter) =>
  invoke<HistoryEntry[]>('search_history', { filter });

// ============================================================
// Templates
// ============================================================

export const listTemplates = () => invoke<Template[]>('list_templates');

export const getTemplate = (name: string) => invoke<Template>('get_template', { name });

export const saveTemplate = (template: Template) => invoke<void>('save_template', { template });

export const deleteTemplate = (name: string) => invoke<void>('delete_template', { name });

// ============================================================
// Cookies
// ============================================================

export const getCookies = () => invoke<CookieJar[]>('get_cookies');

export const setCookies = (jar: CookieJar) => invoke<void>('set_cookies', { jar });

export const clearCookies = () => invoke<void>('clear_cookies');

// ============================================================
// App utility
// ============================================================

export const getAppDataDir = () => invoke<string>('get_app_data_dir');

export const watchCollections = () => invoke<void>('watch_collections');

export const stopWatching = () => invoke<void>('stop_watching');

// ============================================================
// Git
// ============================================================

export const gitIsRepo = (collectionPath: string) =>
  invoke<boolean>('git_is_repo', { collectionPath });

export const gitInit = (collectionPath: string) => invoke<void>('git_init', { collectionPath });

export const gitClone = (url: string, destPath: string, creds: GitCredentials) =>
  invoke<void>('git_clone', { url, destPath, creds });

export const gitStatus = (collectionPath: string) =>
  invoke<RepoStatus>('git_status', { collectionPath });

export const gitDiff = (collectionPath: string, file: string) =>
  invoke<FileDiff>('git_diff', { collectionPath, file });

export const gitDiffStaged = (collectionPath: string, file: string) =>
  invoke<FileDiff>('git_diff_staged', { collectionPath, file });

export const gitDiffCommit = (collectionPath: string, oid: string) =>
  invoke<FileDiff[]>('git_diff_commit', { collectionPath, oid });

export const gitStage = (collectionPath: string, files: string[]) =>
  invoke<void>('git_stage', { collectionPath, files });

export const gitUnstage = (collectionPath: string, files: string[]) =>
  invoke<void>('git_unstage', { collectionPath, files });

export const gitDiscard = (collectionPath: string, files: string[]) =>
  invoke<void>('git_discard', { collectionPath, files });

export const gitCommit = (collectionPath: string, message: string) =>
  invoke<CommitInfo>('git_commit', { collectionPath, message });

export const gitLog = (collectionPath: string, limit: number) =>
  invoke<CommitInfo[]>('git_log', { collectionPath, limit });

export const gitPush = (collectionPath: string, remote: string, creds: GitCredentials) =>
  invoke<void>('git_push', { collectionPath, remote, creds });

export const gitPull = (collectionPath: string, remote: string, creds: GitCredentials) =>
  invoke<void>('git_pull', { collectionPath, remote, creds });

export const gitFetch = (collectionPath: string, remote: string, creds: GitCredentials) =>
  invoke<FetchResult>('git_fetch', { collectionPath, remote, creds });

export const gitBranches = (collectionPath: string) =>
  invoke<BranchList>('git_branches', { collectionPath });

export const gitSwitchBranch = (collectionPath: string, name: string) =>
  invoke<void>('git_switch_branch', { collectionPath, name });

export const gitCheckoutRemoteBranch = (collectionPath: string, name: string) =>
  invoke<void>('git_checkout_remote_branch', { collectionPath, name });

export const gitCreateBranch = (collectionPath: string, name: string) =>
  invoke<void>('git_create_branch', { collectionPath, name });

export const gitDeleteBranch = (collectionPath: string, name: string) =>
  invoke<void>('git_delete_branch', { collectionPath, name });

export const gitMergeBranch = (collectionPath: string, name: string) =>
  invoke<void>('git_merge_branch', { collectionPath, name });

export const gitStashList = (collectionPath: string) =>
  invoke<StashEntry[]>('git_stash_list', { collectionPath });

export const gitStashSave = (collectionPath: string, message: string) =>
  invoke<void>('git_stash_save', { collectionPath, message });

export const gitStashPop = (collectionPath: string, index: number) =>
  invoke<void>('git_stash_pop', { collectionPath, index });

export const gitStashApply = (collectionPath: string, index: number) =>
  invoke<void>('git_stash_apply', { collectionPath, index });

export const gitStashDrop = (collectionPath: string, index: number) =>
  invoke<void>('git_stash_drop', { collectionPath, index });

export const gitConflicts = (collectionPath: string) =>
  invoke<ConflictFile[]>('git_conflicts', { collectionPath });

export const gitResolveConflict = (
  collectionPath: string,
  file: string,
  resolution: ConflictResolution,
) => invoke<void>('git_resolve_conflict', { collectionPath, file, resolution });

export const gitAbortMerge = (collectionPath: string) =>
  invoke<void>('git_abort_merge', { collectionPath });

export const gitListRemotes = (collectionPath: string) =>
  invoke<RemoteInfo[]>('git_list_remotes', { collectionPath });

export const gitAddRemote = (collectionPath: string, name: string, url: string) =>
  invoke<void>('git_add_remote', { collectionPath, name, url });

export const gitRemoveRemote = (collectionPath: string, name: string) =>
  invoke<void>('git_remove_remote', { collectionPath, name });

export const gitSetRemoteUrl = (collectionPath: string, name: string, url: string) =>
  invoke<void>('git_set_remote_url', { collectionPath, name, url });

export const scanCollectionsInPath = (path: string) =>
  invoke<CollectionScanResult[]>('scan_collections_in_path', { path });

export const detectClonedStructure = (path: string) =>
  invoke<ClonedRepoStructure>('detect_cloned_structure', { path });

export const getDefaultSshKeyPath = (): Promise<string | null> =>
  invoke<string | null>('get_default_ssh_key_path');

export const saveGitCredentials = (creds: GitCredentials): Promise<void> =>
  invoke<void>('save_git_credentials', { creds });

export const loadGitCredentials = (): Promise<GitCredentials | null> =>
  invoke<GitCredentials | null>('load_git_credentials');

// ============================================================
// Realtime events
// ============================================================

export const onFileChange = (handler: (event: FileChangedEvent) => void): Promise<UnlistenFn> =>
  listen<FileChangedEvent>('collection-changed', (e) => handler(e.payload));

export interface CollectionChangedEvent {
  type: string;
  collection?: string;
  name?: string;
  oldName?: string;
  newName?: string;
  path?: string;
  eventType?: string;
}

export const onCollectionChanged = (
  handler: (event: CollectionChangedEvent) => void,
): Promise<UnlistenFn> =>
  listen<CollectionChangedEvent>('collection-changed', (e) => handler(e.payload));

export const onRequestExecuted = (handler: () => void): Promise<UnlistenFn> =>
  listen('request-executed', () => handler());

export const onGitChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen('git-changed', () => handler());

// ============================================================
// OAuth2
// ============================================================

export interface OAuth2TokenResponse {
  access_token: string;
  token_type: string;
  expires_in?: number;
  refresh_token?: string;
  scope?: string;
}

export const oauth2AuthCodeFlow = (
  authorizationUrl: string,
  tokenUrl: string,
  clientId: string,
  clientSecret: string,
  scope?: string,
  callbackUrl?: string,
  verifySsl?: boolean,
) =>
  invoke<OAuth2TokenResponse>('oauth2_auth_code_flow', {
    authorizationUrl,
    tokenUrl,
    clientId,
    clientSecret,
    scope,
    callbackUrl,
    verifySsl,
  });

// ============================================================
// OAuth2 — unified commands (Phase 2)
// ============================================================

export interface OAuth2GetTokenRequest {
  grantType: string;
  authorizationUrl?: string;
  tokenUrl?: string;
  callbackUrl?: string;
  clientId: string;
  clientSecret?: string;
  scope?: string;
  state?: string;
  username?: string;
  password?: string;
  clientAuthentication?: string;
  usePkce?: boolean;
  useSystemBrowser?: boolean;
  verifySsl?: boolean;
  authParams?: OAuth2AdditionalParam[];
  tokenParams?: OAuth2AdditionalParam[];
  refreshParams?: OAuth2AdditionalParam[];
  collection?: string;
  environmentName?: string;
  requestPath?: string;
  forceReauth?: boolean;
}

export interface OAuth2RefreshRequest {
  refreshToken: string;
  tokenUrl: string;
  refreshTokenUrl?: string;
  clientId: string;
  clientSecret?: string;
  scope?: string;
  clientAuthentication?: string;
  verifySsl?: boolean;
  refreshParams?: OAuth2AdditionalParam[];
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}

// Matches the Rust `OAuthToken` struct — snake_case on the wire (no serde rename).
export interface OAuth2TokenResult {
  access_token: string;
  token_type: string;
  expires_in?: number;
  refresh_token?: string;
  scope?: string;
  id_token?: string;
}

export const oauth2GetToken = (request: OAuth2GetTokenRequest) =>
  invoke<OAuth2TokenResult>('oauth2_get_token', { request });

export const oauth2RefreshToken = (request: OAuth2RefreshRequest) =>
  invoke<OAuth2TokenResult>('oauth2_refresh_token', { request });

export const oauth2DecodeJwt = (token: string) =>
  invoke<OAuth2JwtClaims>('oauth2_decode_jwt', { token });

// ============================================================
// Workspace commands
// ============================================================

export const listWorkspaces = () => invoke<Workspace[]>('list_workspaces');

export const getActiveWorkspace = () => invoke<Workspace>('get_active_workspace');

export const createWorkspace = (name: string, path: string) =>
  invoke<Workspace>('create_workspace', { name, path });

export const switchWorkspace = (id: string) => invoke<Workspace>('switch_workspace', { id });

export const renameWorkspace = (id: string, newName: string) =>
  invoke<void>('rename_workspace', { id, newName });

export const closeWorkspace = (id: string) => invoke<void>('close_workspace', { id });

export const deleteWorkspace = (id: string) => invoke<void>('delete_workspace', { id });

export const openFolderPicker = () => invoke<string | null>('open_folder_picker');

export const pinWorkspace = (id: string) => invoke<void>('pin_workspace', { id });

export const unpinWorkspace = (id: string) => invoke<void>('unpin_workspace', { id });

export const updateWorkspaceDescription = (id: string, description: string | null) =>
  invoke<void>('update_workspace_description', { id, description });

export const openWorkspaceFromDisk = (path: string) =>
  invoke<Workspace>('open_workspace', { path });

export const getWorkspaceConfig = (workspaceId: string) =>
  invoke<WorkspaceConfig>('get_workspace_config', { workspaceId });

export const getMultiWorkspaceMode = () => invoke<boolean>('get_multi_workspace_mode');

export const setMultiWorkspaceMode = (enabled: boolean) =>
  invoke<void>('set_multi_workspace_mode', { enabled });

export const linkExternalCollection = (workspaceId: string, collectionPath: string) =>
  invoke<void>('link_external_collection', { workspaceId, collectionPath });

// ============================================================
// Variables
// ============================================================

// Global env (selection pointer in workspace.yml)
export const getGlobalEnvironmentName = () => invoke<string | null>('get_global_environment_name');
export const setGlobalEnvironment = (name: string | null) =>
  invoke<void>('set_global_environment', { name });

// Workspace-level global environment CRUD
export const listGlobalEnvironments = () => invoke<Environment[]>('list_global_environments');
export const getGlobalEnvironment = (name: string) =>
  invoke<Environment>('get_global_environment', { name });
export const saveGlobalEnvironment = (env: Environment) =>
  invoke<void>('save_global_environment', { env });
export const deleteGlobalEnvironment = (name: string) =>
  invoke<void>('delete_global_environment', { name });

// Process env (read-only OS vars)
export const getProcessEnvVars = () => invoke<Record<string, string>>('get_process_env_vars');

// Folder variables — server walks full parent chain
export const getFolderChainVariables = (collection: string, requestPath: string) =>
  invoke<CollectionVariable[]>('get_folder_chain_variables', { collection, requestPath });

// Folder variables — reads only this folder's own folder.yml (no chain walk)
export const getFolderVariables = (collection: string, folderPath: string) =>
  invoke<CollectionVariable[]>('get_folder_variables', { collection, folderPath });
export const saveFolderVariables = (
  collection: string,
  folderPath: string,
  variables: CollectionVariable[],
) => invoke<void>('save_folder_variables', { collection, folderPath, vars: variables });

// Request variables
export const getRequestVariables = (collection: string, requestPath: string) =>
  invoke<CollectionVariable[]>('get_request_variables', { collection, requestPath });
export const saveRequestVariables = (
  collection: string,
  requestPath: string,
  variables: CollectionVariable[],
) => invoke<void>('save_request_variables', { collection, requestPath, vars: variables });

// Request docs
export const updateRequestDocs = (
  collection: string,
  path: string,
  docs: string | null,
): Promise<void> => invoke<void>('update_request_docs', { collection, path, docs });

// ============================================================
// Collection import
// ============================================================

export type SkipReason =
  | { type: 'unsupportedRequestType'; detail: string }
  | { type: 'unsupportedAuthType'; detail: string }
  | { type: 'parseError'; detail: string };

export interface SkippedItem {
  path: string;
  reason: SkipReason;
}

export interface ImportReport {
  totalFiles: number;
  imported: number;
  skipped: SkippedItem[];
  createdWorkspace: string | null;
  createdCollections: string[];
  detectedType: 'collection' | 'workspace';
}

export const importBruno = (
  path: string,
  targetWorkspaceId: string,
  createNewWorkspace?: boolean,
) => invoke<ImportReport>('import_bruno', { path, targetWorkspaceId, createNewWorkspace });

export const importBrunoZip = (
  zipPath: string,
  targetWorkspaceId: string,
  createNewWorkspace?: boolean,
) => invoke<ImportReport>('import_bruno_zip', { zipPath, targetWorkspaceId, createNewWorkspace });

export const importPostmanCollection = (path: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_postman_collection', { path, targetWorkspaceId });

export const importPostmanEnvironment = (
  jsonPath: string,
  collectionName: string,
  targetWorkspaceId: string,
) =>
  invoke<ImportReport>('import_postman_environment', {
    jsonPath,
    collectionName,
    targetWorkspaceId,
  });

// ============================================================
// UI state persistence
// ============================================================

export interface UiStateWorkspaceTabs {
  workspaceId: string;
}

export interface UiStateCollectionTab {
  id: string;
  title: string;
  collectionName: string;
  activeSection?: string;
}

export interface UiState {
  activeMode: 'workspace' | 'collection';
  workspaceTabs?: UiStateWorkspaceTabs;
  layoutDirection?: 'stacked' | 'side-by-side';
  activeCollection?: string;
  collectionTabs?: UiStateCollectionTab[];
  sidebarWidth?: number;
  isConsoleOpen?: boolean;
  consoleHeight?: number;
}

export const loadUiState = () => invoke<UiState | null>('load_ui_state');

export const saveUiState = (state: UiState) => invoke<void>('save_ui_state', { state });

// ============================================================
// Contract Lock
// ============================================================

/**
 * Scope of a contract. Serialised from the Rust enum
 * `ContractScope` with `#[serde(tag = "type", rename_all = "snake_case")]`,
 * so the discriminant field is `type` and the `rel_path` field keeps
 * snake_case (not camelCase) on the wire.
 */
export type ContractScope =
  | { type: 'collection' }
  | { type: 'folder'; rel_path: string }
  | { type: 'request'; rel_path: string };

/** Supported attachment file extensions. Must mirror ALLOWED_EXTENSIONS in contract_service.rs. */
export const ATTACHMENT_ALLOWED_EXTENSIONS = [
  'pdf',
  'doc',
  'docx',
  'txt',
  'md',
  'png',
  'jpg',
  'jpeg',
] as const;
/** Maximum attachment size in bytes (2 MB). Must mirror MAX_ATTACHMENT_BYTES in contract_service.rs. */
export const ATTACHMENT_MAX_BYTES = 2 * 1024 * 1024;

export interface Contract {
  id: string;
  title: string;
  provider: string;
  consumer: string;
  project: string;
  version: string;
  effectiveDate: string;
  expiryDate: string | null;
  /** Relative paths to attachments stored inside the collection folder. */
  documentPaths: string[];
  enforcementMode: 'informational' | 'warn' | 'block';
  scope: ContractScope;
}

export interface ChangelogEntry {
  timestamp: string;
  requestPath: string;
  field: string;
  changeType: 'changed' | 'added' | 'removed';
  oldValue: string | null;
  newValue: string | null;
}

export interface ContractChangelog {
  contractId: string;
  entries: ChangelogEntry[];
}

export interface SnapshotKeyValue {
  key: string;
  value: string;
}

export interface RequestSignatureSnapshot {
  requestPath: string;
  method: string;
  urlPattern: string;
  /** Enabled headers with key and value. */
  headers: SnapshotKeyValue[];
  /** Enabled query params with key and value. */
  queryParams: SnapshotKeyValue[];
  /** Raw body string (JSON/XML/Text/Sparql/Binary). Absent for form bodies. */
  bodyContent?: string;
  /** Enabled form fields (FormData/FormUrlEncoded) with key and value. */
  formFields: SnapshotKeyValue[];
  authType: string;
  /** Summarised auth credentials for change detection. */
  authDetail: string;
  capturedAt: string;
}

export interface AttachContractInput {
  title: string;
  provider: string;
  consumer: string;
  project: string;
  version: string;
  effectiveDate: string;
  expiryDate: string | null;
  /** Absolute paths from the OS file picker. Copied into the collection by the service. */
  documentPaths: string[];
  scope: ContractScope;
  /**
   * Initial signature snapshots captured for covered requests at the
   * moment the contract is signed. Frontend sends an empty array
   * today — collection-scoped contracts start with no baseline and
   * accumulate entries as requests are saved.
   */
  initialSnapshots: RequestSignatureSnapshot[];
}

export interface UpdateContractInput {
  contractId: string;
  title: string;
  provider: string;
  consumer: string;
  project: string;
  version: string;
  effectiveDate: string;
  expiryDate: string | null;
  /** Absolute paths for newly added attachments (not yet copied). */
  newDocumentPaths: string[];
  /** Relative paths of existing attachments the user wants to keep. */
  keptDocumentPaths: string[];
}

export const attachContract = (collectionRoot: string, input: AttachContractInput) =>
  invoke<Contract>('attach_contract', { collectionRoot, input });

export const updateContract = (collectionRoot: string, input: UpdateContractInput) =>
  invoke<Contract>('update_contract', { collectionRoot, input });

export const listContracts = (collectionRoot: string) =>
  invoke<Contract[]>('list_contracts', { collectionRoot });

export const getContract = (collectionRoot: string, contractId: string) =>
  invoke<Contract>('get_contract', { collectionRoot, contractId });

export const deleteContract = (collectionRoot: string, contractId: string) =>
  invoke<void>('delete_contract', { collectionRoot, contractId });

export const getContractChangelog = (collectionRoot: string, contractId: string) =>
  invoke<ContractChangelog>('get_contract_changelog', { collectionRoot, contractId });

// ============================================================
// Security audit / compliance
// ============================================================

export type Framework = 'soc2' | 'iso27001' | 'iso42001' | 'csa_star';
export type EnforcementLevel = 'record' | 'warn' | 'block';

export interface ControlId {
  framework: Framework;
  code: string;
  title: string;
}

export interface ComplianceProfile {
  activeFrameworks: Framework[];
  enforcement: EnforcementLevel;
  mutedKinds: string[];
}

export type AuditEventKind =
  | { kind: 'contract_attached'; contractId: string; collection: string; scope: string }
  | { kind: 'contract_deleted'; contractId: string; collection: string }
  | { kind: 'contract_violation'; contractId: string; requestPath: string; field: string }
  | { kind: 'collection_deleted'; collection: string }
  | { kind: 'collection_exported'; collection: string; destination: string }
  | { kind: 'secret_variable_written'; environment: string; variableKey: string }
  | { kind: 'sensitive_auth_used'; authType: string; collection: string; requestPath: string }
  | { kind: 'audit_evidence_exported'; rangeStart: string; rangeEnd: string; count: number }
  | { kind: 'audit_chain_broken'; atEventId: string; expectedHash: string; actualHash: string };

export interface SecurityAuditEvent {
  id: string;
  occurredAt: string;
  actor: string;
  workspaceId: string | null;
  event: AuditEventKind;
  controls: ControlId[];
  prevHash: string;
  hash: string;
  metadata?: Record<string, string>;
}

export interface EvidenceExport {
  exportedAt: string;
  rangeStart: string;
  rangeEnd: string;
  events: SecurityAuditEvent[];
  chainVerified: boolean;
}

export const listAuditEvents = () => invoke<SecurityAuditEvent[]>('list_audit_events');

export const listAuditEventsRange = (start: string, end: string) =>
  invoke<SecurityAuditEvent[]>('list_audit_events_range', { input: { start, end } });

export const getComplianceProfile = () => invoke<ComplianceProfile>('get_compliance_profile');

export const setComplianceProfile = (profile: ComplianceProfile) =>
  invoke<void>('set_compliance_profile', { profile });

export const exportAuditEvidence = (start: string, end: string) =>
  invoke<EvidenceExport>('export_audit_evidence', { input: { start, end } });

export const saveAuditEvidenceFile = (path: string, content: string) =>
  invoke<void>('save_audit_evidence_file', { input: { path, content } });
