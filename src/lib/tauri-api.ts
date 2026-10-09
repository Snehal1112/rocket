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

// Standard methods or any custom method token, as the backend serializes it.
export type HttpMethod = string;

export interface Header {
  key: string;
  value: string;
  enabled: boolean;
}

export type BodyMode =
  | 'none'
  | 'json'
  | 'xml'
  | 'text'
  | 'sparql'
  | 'formdata'
  | 'formurlencoded'
  | 'binary';

export interface FormDataEntry {
  key: string;
  value: string;
  entryType: 'text' | 'file';
  enabled: boolean;
  contentType?: string;
}

export interface Body {
  mode: BodyMode;
  content?: string;
  formData?: FormDataEntry[];
  filePath?: string;
}

export type Auth =
  | { authType: 'none' }
  | { authType: 'inherit' }
  | { authType: 'basic'; username: string; password: string }
  | { authType: 'bearer'; token: string }
  | { authType: 'api-key'; key: string; value: string; placement: 'header' | 'query' }
  | { authType: 'o-auth2'; [key: string]: unknown }
  | { authType: 'aws-sig-v4'; [key: string]: unknown }
  | { authType: 'digest'; username: string; password: string }
  | { authType: 'wsse'; username: string; password: string }
  | { authType: 'ntlm'; username: string; password: string; domain: string }
  | { authType: 'o-auth1'; [key: string]: unknown };

export interface RequestOptions {
  followRedirects: boolean;
  timeoutMs: number;
  verifySsl: boolean;
  /** Send and keep cookies. On by default; the backend treats a missing value as true. */
  useCookieJar?: boolean;
  /** Redirects to follow before giving up. The backend default (10) applies when missing. */
  maxRedirects?: number;
  /** Percent-encode the params table. On by default; the backend treats a missing value as true. */
  encodeUrl?: boolean;
}

export interface CollectionVariable {
  key: string;
  value: string;
  initialValue: string;
  enabled: boolean;
  secret: boolean;
}

export type SandboxMode = 'safe' | 'developer';

/** Script order across collection, folders and request. Absent means sandwich. */
export type ScriptFlow = 'sandwich' | 'sequential';

export interface CollectionSettings {
  docs?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
  sandboxMode: SandboxMode;
  /** Extra directories scripts may require() from, in Developer mode only. */
  scriptContextRoots?: string[];
  /** Persisted at extensions.bruno.scripts.flow in opencollection.yml. */
  scriptFlow?: ScriptFlow;
  /**
   * Lets the AI Assistant send this collection's requests. Off by default.
   * Persisted at extensions.rocketapi.agentAutonomyEnabled in opencollection.yml.
   */
  agentAutonomyEnabled?: boolean;
}

/** One folder's own settings from its folder.yml. Mirrors `FolderSettingsDto` in Rust. */
export interface FolderSettings {
  headers: Header[];
  /** Absent or `inherit` both mean the folder sets no auth. */
  auth?: Auth;
  /** Pre-request folder variables. */
  variables: CollectionVariable[];
  preRequestScript?: string;
  postResponseScript?: string;
  testsScript?: string;
  /** Markdown docs content. */
  docs?: string;
}

export interface CollectionSummary {
  uid: string;
  repositoryId: string;
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

export interface AssertionEntry {
  expression: string;
  operator: string;
  value?: string;
  disabled?: boolean;
}

/** Selector for extracting a value from a request/response (`Action`, OC spec). */
export interface ActionSelector {
  expression: string;
  /** Only "jsonq" is valid per the OpenCollection spec. */
  method: 'jsonq';
}

export interface ActionVariable {
  name: string;
  scope: 'runtime' | 'request' | 'folder' | 'collection' | 'environment';
}

/** Declarative `set-variable` action (OC spec `Action`), edited via the Vars tab. */
export interface ActionEntry {
  phase: 'before-request' | 'after-response';
  selector: ActionSelector;
  variable: ActionVariable;
  disabled?: boolean;
}

export interface Request {
  uid: string;
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  queryParams?: QueryParam[];
  pathParams?: PathParam[];
  body?: Body;
  auth: Auth;
  fileName?: string;
  tags?: string[];
  docs?: string | null;
  settings?: ApiRequestSettings;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
  tests?: string | null;
  assertions?: AssertionEntry[];
  actions?: ActionEntry[];
}

export type RequestKind = 'http' | 'graphql' | 'grpc' | 'websocket';

export interface GraphQlBody {
  query: string;
  /** JSON text, not a parsed object. */
  variables?: string | null;
}

export interface GraphQlBodyVariant {
  title: string;
  selected: boolean;
  body: GraphQlBody;
}

export interface GraphQlRequest {
  uid: string;
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  body: GraphQlBody;
  /** Every stored variant. Send it back unchanged so a save keeps the unselected ones. */
  bodyVariants?: GraphQlBodyVariant[];
  auth: Auth;
  fileName?: string;
  tags?: string[];
  docs?: string | null;
  settings?: ApiRequestSettings;
  preRequestScript?: string | null;
  postResponseScript?: string | null;
  tests?: string | null;
  assertions?: AssertionEntry[];
  actions?: ActionEntry[];
}

export type GrpcMethodType = 'unary' | 'client-streaming' | 'server-streaming' | 'bidi-streaming';

export interface GrpcMessage {
  /** Empty for the single untitled message of a simple request. */
  title: string;
  /** The message the editor shows first and a unary call sends. */
  selected: boolean;
  /** Protobuf JSON text. */
  content: string;
}

export interface GrpcScript {
  type: string;
  code: string;
}

/** A saved gRPC request. Empty lists are absent, like the Rust side skips them. */
export interface GrpcRequest {
  uid: string;
  name: string;
  fileName?: string;
  seq?: number;
  tags?: string[];
  /** Polymorphic on the Rust side (string or typed). Passed back unchanged. */
  description?: unknown;
  url: string;
  /** `package.Service/Method`. */
  method?: string;
  methodType: GrpcMethodType;
  protoFilePath?: string;
  metadata?: Header[];
  messages?: GrpcMessage[];
  auth: Auth;
  variables?: CollectionVariable[];
  scripts?: GrpcScript[];
  assertions?: AssertionEntry[];
  docs?: string | null;
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
  /** Which protocol the file holds. Absent means HTTP. */
  kind?: RequestKind;
}

/** Non-HTTP item (GraphQL, gRPC, WebSocket) kept as raw YAML. Not shown or editable in the UI yet. */
export type WebSocketMessageKind = 'text' | 'json' | 'xml' | 'binary';

export interface WebSocketMessage {
  title: string;
  selected: boolean;
  kind: WebSocketMessageKind;
  /** Text as is. For `binary` this is base64. */
  data: string;
}

export interface WebSocketSettings {
  /** Connect timeout in ms, or 'inherit' for the 30 s default. 0 waits forever. */
  timeout?: number | 'inherit';
  /** Ms between client pings, or 'inherit' for none. */
  keepAliveInterval?: number | 'inherit';
}

/** Mirrors the Rust `WebSocketRequest` (rocket-collection). */
export interface WebSocketRequest {
  uid: string;
  name: string;
  /** String, `{ content, type }` or null. Kept verbatim, never edited here. */
  description?: unknown;
  seq?: number;
  tags?: string[];
  url: string;
  headers: Header[];
  messages: WebSocketMessage[];
  auth: Auth;
  runtimeAuth?: Auth;
  /** Edited through the request-variables commands, so a save from the tab never sends it. */
  variables?: CollectionVariable[];
  scripts?: { scriptType: string; code: string }[];
  settings?: WebSocketSettings;
  docs?: string | null;
  fileName?: string;
}

export interface OpaqueProtocolItem {
  protocol: 'graphql' | 'grpc' | 'websocket';
  name: string;
  raw: unknown;
}

export interface ScriptFileItem {
  fileName: string;
  name: string;
}

export type CollectionItem =
  | ({ type: 'request' } & Request)
  | ({ type: 'folder' } & Folder)
  | ({ type: 'summary' } & RequestSummary)
  | ({ type: 'graphql' } & GraphQlRequest)
  | ({ type: 'websocket' } & WebSocketRequest)
  | ({ type: 'grpc' } & GrpcRequest)
  | ({ type: 'opaque' } & OpaqueProtocolItem)
  | ({ type: 'scriptFile' } & ScriptFileItem);

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

export interface ExternalSecretRef {
  name: string;
  secretId: string;
}

export interface ExternalSecretBinding {
  alias: string;
  connectionId: string;
  vaultName: string;
  secretNames: ExternalSecretRef[];
}

export type SecretProviderKind = 'rocketvault' | 'azure' | 'aws' | 'hashicorp' | 'gcp';

// Azure AD service principal settings. The vault URL is baseUrl and the app
// registration id is clientId. authorityHost exists for tests only.
export interface AzureConfig {
  kind: 'azure';
  tenantId: string;
  authorityHost?: string;
}

// Provider-specific, non-secret settings. Matches the Rust ProviderConfigDto.
export type ProviderConfig = AzureConfig;

export interface SecretManagerConnection {
  id: string;
  label: string;
  baseUrl: string;
  clientId: string;
  verifySsl: boolean;
  allowInsecureHttp: boolean;
  // Absent means RocketVault, for payloads written before providers existed.
  provider?: SecretProviderKind;
  config?: ProviderConfig | null;
}

// A certificate in a RocketVault vault, for the Certificates tab picker. Never key material.
export interface VaultCertificateSummary {
  id: string;
  name: string;
  exportable: boolean;
  enabled: boolean;
  keyAlgorithm: string;
  expiresAt?: string | null;
}

// How a RocketVault certificate is exported.
export type VaultCertificateFormat = 'pem' | 'pkcs12';

// Persisted client certificate. For PEM and PKCS12, a piece has one source: a
// file path or a vault reference (`alias.secretName`). A reference is never a
// value. A `vault` entry names a RocketVault certificate that is exported when
// a request needs it; it stores names only. A missing format means PEM.
export type ClientCertificate =
  | {
      type: 'pem';
      domain: string;
      certificateFilePath?: string;
      privateKeyFilePath?: string;
      certificateSecret?: string;
      privateKeySecret?: string;
      passphrase?: string;
    }
  | {
      type: 'pkcs12';
      domain: string;
      pkcs12FilePath?: string;
      pkcs12Secret?: string;
      passphrase?: string;
    }
  | {
      type: 'vault';
      domain: string;
      // External Secrets alias of this environment; the connection and vault come from it.
      binding: string;
      // Certificate name in that vault.
      certificate: string;
      format?: VaultCertificateFormat;
    };

export interface Environment {
  name: string;
  variables: Variable[];
  externalSecrets?: ExternalSecretBinding[];
  clientCertificates?: ClientCertificate[];
  extends?: string;
  dotEnvFilePath?: string;
  color?: string;
  description?: unknown;
}

export interface AgentConfig {
  id: string;
  label: string;
  command: string;
  args: string[];
  workingDir?: string;
  credentialEnvVar: string;
  vaultConnectionId: string;
  vaultName: string;
  vaultSecretId: string;
  vaultSecretName: string;
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
  /** Set when the body is not text: `body` is empty and the bytes are in `bodyBase64`. */
  isBinary?: boolean;
  /** Raw bytes of a binary body, base64. Absent when the body was too large to carry. */
  bodyBase64?: string;
}

export interface TestResult {
  name: string;
  status: 'passed' | 'failed';
  error: string | null;
}

export interface ConsoleEntry {
  level: 'log' | 'warn' | 'error';
  message: string;
}

export interface ExecuteRequestResponse extends HttpResponse {
  testResults: TestResult[];
  consoleEntries: ConsoleEntry[];
  scriptError: string | null;
}

export interface QueryParam {
  key: string;
  value: string;
  enabled: boolean;
}

export interface PathParam {
  name: string;
  value: string;
  description?: string;
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
  preRequestScript?: string;
  postResponseScript?: string;
  testsScript?: string;
  assertions?: AssertionEntry[];
  globalEnvName?: string;
  /** Tags on the request, exposed to scripts via `req.getTags()`. */
  tags?: string[];
  /** Path parameters on the request, exposed to scripts via `req.getPathParams()`. */
  pathParams?: PathParam[];
  /** Declarative `set-variable` actions, evaluated at the phase they declare. */
  actions?: ActionEntry[];
  /** Opt-in per-workspace policy checked against BeforeRequest req.setUrl() redirects. */
  requestGuardPolicy?: RequestGuardPolicy;
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

export interface GitIdentity {
  name: string;
  email: string;
}

export interface CloneDestinationGrant {
  capability: string;
  displayPath: string;
  expiresInSeconds: number;
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

/**
 * Structured Git network error returned by `git_clone`, `git_push_v2`,
 * `git_pull_v2`, and `git_fetch_v2`. These commands reject with this typed
 * object (not a plain string like other Git commands), so callers must use
 * `parseGitNetworkError` rather than `String(error)`.
 */
export type GitNetworkError =
  | {
      code: 'sshUnknownHost' | 'sshHostKeyChanged' | 'sshHostVerificationUnavailable';
      message: string;
      host: string;
      port: number;
      algorithm: string;
      fingerprint: string;
    }
  | {
      code: 'tlsCertificateInvalid';
      message: string;
      host: string;
      port: number;
      fingerprint: string;
    }
  | { code: 'generic'; message: string };

/** The three SSH host-trust failure variants of {@link GitNetworkError}. */
export type GitSshTrustFailure = Extract<
  GitNetworkError,
  { code: 'sshUnknownHost' | 'sshHostKeyChanged' | 'sshHostVerificationUnavailable' }
>;

const GIT_SSH_TRUST_FAILURE_CODES = new Set<GitSshTrustFailure['code']>([
  'sshUnknownHost',
  'sshHostKeyChanged',
  'sshHostVerificationUnavailable',
]);

export function isGitSshTrustFailure(error: GitNetworkError): error is GitSshTrustFailure {
  return GIT_SSH_TRUST_FAILURE_CODES.has(error.code as GitSshTrustFailure['code']);
}

/**
 * Normalize a rejected `git_clone`/`git_push_v2`/`git_pull_v2`/`git_fetch_v2`
 * promise into a {@link GitNetworkError}. Tauri rejects with the deserialized
 * JSON error object for these commands, but this defensively falls back to a
 * generic error for any unexpected shape (e.g. a transport-level rejection).
 */
export function parseGitNetworkError(error: unknown): GitNetworkError {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    typeof (error as { code: unknown }).code === 'string' &&
    typeof (error as { message: unknown }).message === 'string'
  ) {
    const candidate = error as { code: string; message: string };
    if (GIT_SSH_TRUST_FAILURE_CODES.has(candidate.code as GitSshTrustFailure['code'])) {
      const trust = error as {
        code: string;
        message: string;
        host?: unknown;
        port?: unknown;
        algorithm?: unknown;
        fingerprint?: unknown;
      };
      if (
        typeof trust.host === 'string' &&
        typeof trust.port === 'number' &&
        typeof trust.algorithm === 'string' &&
        typeof trust.fingerprint === 'string'
      ) {
        return {
          code: trust.code as GitSshTrustFailure['code'],
          message: trust.message,
          host: trust.host,
          port: trust.port,
          algorithm: trust.algorithm,
          fingerprint: trust.fingerprint,
        };
      }
    }
    if (candidate.code === 'tlsCertificateInvalid') {
      const tls = error as {
        code: string;
        message: string;
        host?: unknown;
        port?: unknown;
        fingerprint?: unknown;
      };
      if (
        typeof tls.host === 'string' &&
        typeof tls.port === 'number' &&
        typeof tls.fingerprint === 'string'
      ) {
        return {
          code: 'tlsCertificateInvalid',
          message: tls.message,
          host: tls.host,
          port: tls.port,
          fingerprint: tls.fingerprint,
        };
      }
    }
    if (candidate.code === 'generic') {
      return { code: 'generic', message: candidate.message };
    }
    // A recognized error shape (has code + message) that failed its
    // specific field validation above — prefer the original message over
    // stringifying the whole object.
    return { code: 'generic', message: candidate.message };
  }
  return { code: 'generic', message: String(error) };
}

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
  repositoryId: string;
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

export interface RequestGuardPolicy {
  blockScriptRedirectsToInternalHosts: boolean;
  alsoBlockPrivateRanges: boolean;
}

export interface WorkspaceConfig {
  name: string;
  description?: string | null;
  collections: CollectionReference[];
  environments: WorkspaceEnvironmentsConfig;
  /** Absent (not just default-valued) for a workspace that has never opted in — the Rust side skips serializing a fully-permissive policy. */
  requestGuardPolicy?: RequestGuardPolicy;
}

// ============================================================
// Collections
// ============================================================

export const listCollections = () => invoke<CollectionSummary[]>('list_collections');

export const getCollection = (name: string) => invoke<Collection>('get_collection', { name });

export const getCollectionSummaries = (name: string) =>
  invoke<Collection>('get_collection_summaries', { name });

export const getRequest = (collection: string, path: string) =>
  invoke<Request>('get_request', { collection, path });

export const getGraphQlRequest = (collection: string, path: string) =>
  invoke<GraphQlRequest>('get_graphql_request', { collection, path });

export const createCollection = (name: string) => invoke<Collection>('create_collection', { name });

export const deleteCollection = (name: string) => invoke<void>('delete_collection', { name });

export const renameCollection = (oldName: string, newName: string) =>
  invoke<void>('rename_collection', { oldName, newName });

export const saveRequest = (collection: string, path: string, request: Request) =>
  invoke<Request>('save_request', { collection, path, request });

export const saveGraphQlRequest = (collection: string, path: string, request: GraphQlRequest) =>
  invoke<GraphQlRequest>('save_graphql_request', { collection, path, request });

export const getGrpcRequest = (collection: string, path: string) =>
  invoke<GrpcRequest>('get_grpc_request', { collection, path });

export const saveGrpcRequest = (collection: string, path: string, request: GrpcRequest) =>
  invoke<GrpcRequest>('save_grpc_request', { collection, path, request });

export const getWebSocketRequest = (collection: string, path: string) =>
  invoke<WebSocketRequest>('get_websocket_request', { collection, path });

export const saveWebSocketRequest = (collection: string, path: string, request: WebSocketRequest) =>
  invoke<WebSocketRequest>('save_websocket_request', { collection, path, request });

export const renameRequest = (collection: string, oldPath: string, newName: string) =>
  invoke<void>('rename_request', { collection, oldPath, newName });

export const deleteRequest = (collection: string, path: string) =>
  invoke<void>('delete_request', { collection, path });

export const createFolder = (collection: string, path: string) =>
  invoke<void>('create_folder', { collection, path });

export const deleteFolder = (collection: string, path: string) =>
  invoke<void>('delete_folder', { collection, path });

export const createScriptFile = (collection: string, folderPath: string, name: string) =>
  invoke<string>('create_script_file', { collection, folderPath, name });

export const readScriptFile = (collection: string, path: string) =>
  invoke<string>('read_script_file', { collection, path });

export const saveScriptFile = (collection: string, path: string, content: string) =>
  invoke<void>('save_script_file', { collection, path, content });

export const renameScriptFile = (collection: string, path: string, newName: string) =>
  invoke<string>('rename_script_file', { collection, path, newName });

export const deleteScriptFile = (collection: string, path: string) =>
  invoke<void>('delete_script_file', { collection, path });

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
  /** Hard cap on total requests; backend stops spawning once this is reached. */
  maxRequests?: number;
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
  invoke<ExecuteRequestResponse>('execute_request', { input });

/** A GraphQL send: the HTTP side plus the GraphQL payload. `request.body` is ignored. */
export interface ExecuteGraphQlInput {
  request: ExecuteRequestInput;
  query: string;
  variables?: string;
  /** Required when the document defines several operations. */
  operationName?: string;
  /** Run the first operation instead of failing when none is named. For the runner. */
  fallbackFirst?: boolean;
}

export interface GraphQlOperation {
  /** `null` for an anonymous operation. */
  name: string | null;
  kind: 'query' | 'mutation' | 'subscription';
}

export const executeGraphQlRequest = (input: ExecuteGraphQlInput) =>
  invoke<ExecuteRequestResponse>('execute_graphql_request', { input });

export const listGraphQlOperations = (document: string) =>
  invoke<GraphQlOperation[]>('list_graphql_operations', { document });

/** A schema as `buildClientSchema` takes it: `{ __schema: ... }`. */
export interface GraphQlSchemaResult {
  key: string;
  /** RFC 3339 time of the fetch. */
  fetchedAt: string;
  introspection: unknown;
}

export interface FetchGraphQlSchemaInput {
  /** The tab's endpoint, headers, auth and options. Its scripts and body are ignored. */
  request: ExecuteRequestInput;
  /** Bypass the cache and fetch again. */
  refresh?: boolean;
}

export const fetchGraphQlSchema = (input: FetchGraphQlSchemaInput) =>
  invoke<GraphQlSchemaResult>('fetch_graphql_schema', { input });

/** Reads the cache with the same request a fetch would send, so auth and headers count. */
export const getCachedGraphQlSchema = (request: ExecuteRequestInput) =>
  invoke<GraphQlSchemaResult | null>('get_cached_graphql_schema', { request });

export const clearGraphQlSchema = (request: ExecuteRequestInput) =>
  invoke<void>('clear_graphql_schema', { request });

export const evaluateVarExpression = (
  collectionRoot: string,
  expression: string,
  responseJson: string,
): Promise<unknown> =>
  invoke('evaluate_var_expression', { collectionRoot, expression, responseJson });

export const runLoadTest = (
  request: {
    method: HttpMethod;
    url: string;
    headers: Header[];
    queryParams: QueryParam[];
    pathParams?: PathParam[];
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
    pathParams?: PathParam[];
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

export const gitIsRepo = (repositoryId: string) =>
  invoke<boolean>('git_is_repo_v2', { repositoryId });

export const gitInit = (repositoryId: string) => invoke<void>('git_init_v2', { repositoryId });

export const selectCloneDestination = () =>
  invoke<CloneDestinationGrant | null>('select_clone_destination');

export const gitClone = (url: string, capability: string, creds: GitCredentials) =>
  invoke<void>('git_clone', { url, capability, creds });

export const gitStatus = (repositoryId: string) =>
  invoke<RepoStatus>('git_status_v2', { repositoryId });

export const gitDiff = (repositoryId: string, file: string) =>
  invoke<FileDiff>('git_diff_v2', { repositoryId, file });

export const gitDiffStaged = (repositoryId: string, file: string) =>
  invoke<FileDiff>('git_diff_staged_v2', { repositoryId, file });

export const gitDiffCommit = (repositoryId: string, oid: string) =>
  invoke<FileDiff[]>('git_diff_commit_v2', { repositoryId, oid });

export const gitStage = (repositoryId: string, files: string[]) =>
  invoke<void>('git_stage_v2', { repositoryId, files });

export const gitUnstage = (repositoryId: string, files: string[]) =>
  invoke<void>('git_unstage_v2', { repositoryId, files });

export const gitDiscard = (repositoryId: string, files: string[]) =>
  invoke<void>('git_discard_v2', { repositoryId, files });

export const gitCommit = (repositoryId: string, message: string) =>
  invoke<CommitInfo>('git_commit_v2', { repositoryId, message });

export const gitLog = (repositoryId: string, limit: number) =>
  invoke<CommitInfo[]>('git_log_v2', { repositoryId, limit });

export const gitPush = (
  repositoryId: string,
  remote: string,
  creds: GitCredentials,
  force = false,
) => invoke<void>('git_push_v2', { repositoryId, remote, creds, force });

export const gitPull = (repositoryId: string, remote: string, creds: GitCredentials) =>
  invoke<void>('git_pull_v2', { repositoryId, remote, creds });

export const gitFetch = (repositoryId: string, remote: string, creds: GitCredentials) =>
  invoke<FetchResult>('git_fetch_v2', { repositoryId, remote, creds });

export const gitBranches = (repositoryId: string) =>
  invoke<BranchList>('git_branches_v2', { repositoryId });

export const gitSwitchBranch = (repositoryId: string, name: string) =>
  invoke<void>('git_switch_branch_v2', { repositoryId, name });

export const gitCheckoutRemoteBranch = (
  repositoryId: string,
  name: string,
  force = false,
  asName?: string,
) => invoke<void>('git_checkout_remote_branch_v2', { repositoryId, name, force, asName });

export const gitCreateBranch = (repositoryId: string, name: string) =>
  invoke<void>('git_create_branch_v2', { repositoryId, name });

export const gitDeleteBranch = (repositoryId: string, name: string) =>
  invoke<void>('git_delete_branch_v2', { repositoryId, name });

export const gitMergeBranch = (repositoryId: string, name: string) =>
  invoke<void>('git_merge_branch_v2', { repositoryId, name });

export const gitStashList = (repositoryId: string) =>
  invoke<StashEntry[]>('git_stash_list_v2', { repositoryId });

export const gitStashSave = (repositoryId: string, message: string) =>
  invoke<void>('git_stash_save_v2', { repositoryId, message });

export const gitStashPop = (repositoryId: string, index: number) =>
  invoke<void>('git_stash_pop_v2', { repositoryId, index });

export const gitStashApply = (repositoryId: string, index: number) =>
  invoke<void>('git_stash_apply_v2', { repositoryId, index });

export const gitStashDrop = (repositoryId: string, index: number) =>
  invoke<void>('git_stash_drop_v2', { repositoryId, index });

export const gitStashDiff = (repositoryId: string, index: number) =>
  invoke<FileDiff[]>('git_stash_diff_v2', { repositoryId, index });

export const gitConflicts = (repositoryId: string) =>
  invoke<ConflictFile[]>('git_conflicts_v2', { repositoryId });

export const gitResolveConflict = (
  repositoryId: string,
  file: string,
  resolution: ConflictResolution,
) => invoke<void>('git_resolve_conflict_v2', { repositoryId, file, resolution });

export const gitAbortMerge = (repositoryId: string) =>
  invoke<void>('git_abort_merge_v2', { repositoryId });

export const gitListRemotes = (repositoryId: string) =>
  invoke<RemoteInfo[]>('git_list_remotes_v2', { repositoryId });

export const gitAddRemote = (repositoryId: string, name: string, url: string) =>
  invoke<void>('git_add_remote_v2', { repositoryId, name, url });

export const gitRemoveRemote = (repositoryId: string, name: string) =>
  invoke<void>('git_remove_remote_v2', { repositoryId, name });

export const gitSetRemoteUrl = (repositoryId: string, name: string, url: string) =>
  invoke<void>('git_set_remote_url_v2', { repositoryId, name, url });

export const gitGetIdentity = (repositoryId: string) =>
  invoke<GitIdentity>('git_get_identity_v2', { repositoryId });

export const gitSetIdentity = (repositoryId: string, name: string, email: string) =>
  invoke<void>('git_set_identity_v2', { repositoryId, name, email });

export const scanCollectionsInPath = (path: string) =>
  invoke<CollectionScanResult[]>('scan_collections_in_path', { path });

export const detectClonedStructure = (path: string) =>
  invoke<ClonedRepoStructure>('detect_cloned_structure', { path });

export const getDefaultSshKeyPath = (): Promise<string | null> =>
  invoke<string | null>('get_default_ssh_key_path');

export const listSshKeyPaths = (): Promise<string[]> => invoke<string[]>('list_ssh_key_paths');

export const saveGitCredentials = (repositoryId: string, creds: GitCredentials): Promise<void> =>
  invoke<void>('save_git_credentials_v2', { repositoryId, creds });

export const loadGitCredentials = (repositoryId: string): Promise<GitCredentials | null> =>
  invoke<GitCredentials | null>('load_git_credentials_v2', { repositoryId });

// ============================================================
// Realtime events
// ============================================================

export const onFileChange = (handler: (event: FileChangedEvent) => void): Promise<UnlistenFn> =>
  listen<FileChangedEvent>('collection-changed', (e) => handler(e.payload));

/** `type` of the collection-changed payload sent after a folder settings save. */
export const FOLDER_SETTINGS_SAVED_EVENT = 'folderSettingsSaved';

export interface CollectionChangedEvent {
  type: string;
  /** Null when a watched file is outside any collection. */
  collection?: string | null;
  name?: string;
  oldName?: string;
  newName?: string;
  path?: string;
  /** Set by folder events. Snake case, because Rust event fields are sent as is. */
  folder_path?: string;
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

export const updateRequestGuardPolicy = (workspaceId: string, policy: RequestGuardPolicy) =>
  invoke<void>('update_request_guard_policy', { workspaceId, policy });

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

// Folder settings: headers, auth, vars, scripts and docs of one folder.yml (no chain walk).
export const getFolderSettings = (collection: string, folderPath: string) =>
  invoke<FolderSettings>('get_folder_settings', { collection, folderPath });
export const saveFolderSettings = (
  collection: string,
  folderPath: string,
  settings: FolderSettings,
) => invoke<void>('save_folder_settings', { collection, folderPath, settings });

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

export const importWsdl = (path: string, targetWorkspaceId: string) =>
  invoke<ImportReport>('import_wsdl', { path, targetWorkspaceId });

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

// Contract status — mirrors Rust ContractStatus enum (serialised as snake_case strings)
export type ContractStatus =
  | 'draft'
  | 'active'
  | 'drift'
  | 'breach'
  | 'in_review'
  | 'paused'
  | 'expiring_in_30_days'
  | 'expired'
  | 'archived';

export type PartyKind = 'team' | 'company' | 'service' | 'legacy';

export type BreakingChangePolicy = 'strict' | 'lenient' | 'additive_ok';

export interface ContractParty {
  id: string;
  name: string;
  kind: PartyKind;
  avatarSeed?: string;
  avatarColor?: string;
}

export interface ContractPolicy {
  breakingChangePolicy: BreakingChangePolicy;
  noticeDays: number;
  uptimeSla?: number;
}

export interface Contract {
  id: string;
  title: string;
  provider: ContractParty;
  consumers: ContractParty[];
  project: string;
  version: string;
  status: ContractStatus;
  effectiveDate: string;
  expiryDate: string | null;
  /** Relative paths to attachments stored inside the collection folder. */
  documentPaths: string[];
  enforcementMode: 'informational' | 'warn' | 'block';
  scope: ContractScope;
  policy: ContractPolicy;
  driftCount: number;
  breachCount: number;
  endpointCount: number;
  createdBy: string | null;
  createdAt: string | null;
  updatedAt: string | null;
}

export interface ChangelogEntry {
  timestamp: string;
  requestPath: string;
  field: string;
  changeType: 'changed' | 'added' | 'removed';
  oldValue: string | null;
  newValue: string | null;
  isBreaking: boolean;
  requestMethod?: string;
  httpPath?: string;
  author?: string;
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
  /** Legacy: key-only lists for old snapshot format. Absent when empty. */
  queryParamKeys?: string[];
  headerKeys?: string[];
  bodyFieldKeys?: string[];
}

export interface AttachContractInput {
  title: string;
  provider: ContractParty;
  consumers: ContractParty[];
  version: string;
  effectiveDate: string;
  expiryDate: string | null;
  /** Absolute paths from the OS file picker. */
  documentPaths: string[];
  scope: ContractScope;
  policy: ContractPolicy;
  /**
   * Initial signature snapshots captured for covered requests at the
   * moment the contract is signed.
   */
  initialSnapshots: RequestSignatureSnapshot[];
  /** If true, status is set to Active and snapshot taken on creation. */
  publishImmediately: boolean;
}

export interface UpdateContractInput {
  contractId: string;
  title: string;
  provider: ContractParty;
  consumers: ContractParty[];
  version: string;
  effectiveDate: string;
  expiryDate: string | null;
  policy: ContractPolicy;
  /** Absolute paths for newly added attachments (not yet copied). */
  newDocumentPaths: string[];
  /** Relative paths of existing attachments the user wants to keep. */
  keptDocumentPaths: string[];
}

export const attachContract = (collectionRoot: string, input: AttachContractInput) =>
  invoke<Contract>('attach_contract', { collectionRoot, input });

export const updateContract = (collectionRoot: string, input: UpdateContractInput) =>
  invoke<Contract>('update_contract', { collectionRoot, input });

const inFlightContractsRequests = new Map<string, Promise<Contract[]>>();

export const listContracts = (collectionRoot: string): Promise<Contract[]> => {
  const cached = inFlightContractsRequests.get(collectionRoot);
  if (cached) return cached;

  const request = invoke<Contract[]>('list_contracts', { collectionRoot }).finally(() => {
    inFlightContractsRequests.delete(collectionRoot);
  });
  inFlightContractsRequests.set(collectionRoot, request);
  return request;
};

export const getContract = (collectionRoot: string, contractId: string) =>
  invoke<Contract>('get_contract', { collectionRoot, contractId });

export const deleteContract = (collectionRoot: string, contractId: string) =>
  invoke<void>('delete_contract', { collectionRoot, contractId });

export const getContractChangelog = (collectionRoot: string, contractId: string) =>
  invoke<ContractChangelog>('get_contract_changelog', { collectionRoot, contractId });

export interface ContractDriftSummary {
  contractId: string;
  status: ContractStatus;
  driftCount: number;
  breachCount: number;
}

export interface ContractSummary {
  id: string;
  title: string;
  status: ContractStatus;
  driftCount: number;
  breachCount: number;
  endpointCount: number;
}

// ─── Contract lifecycle commands ─────────────────────────────

export async function publishContract(
  collectionRoot: string,
  contractId: string,
  snapshots: RequestSignatureSnapshot[],
): Promise<Contract> {
  return invoke('publish_contract', { collectionRoot, contractId, snapshots });
}

export async function acceptDrift(
  collectionRoot: string,
  contractId: string,
  newVersion: string,
): Promise<Contract> {
  return invoke('accept_drift', { collectionRoot, contractId, newVersion });
}

export async function pauseContract(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('pause_contract', { collectionRoot, contractId });
}

export async function resumeContract(
  collectionRoot: string,
  contractId: string,
): Promise<Contract> {
  return invoke('resume_contract', { collectionRoot, contractId });
}

export async function renewContract(
  collectionRoot: string,
  contractId: string,
  newExpiresAt: string | null,
): Promise<Contract> {
  return invoke('renew_contract', { collectionRoot, contractId, newExpiresAt });
}

export async function sendForReview(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('send_for_review', { collectionRoot, contractId });
}

export async function approveContract(
  collectionRoot: string,
  contractId: string,
): Promise<Contract> {
  return invoke('approve_contract', { collectionRoot, contractId });
}

export async function rejectContract(
  collectionRoot: string,
  contractId: string,
): Promise<Contract> {
  return invoke('reject_contract', { collectionRoot, contractId });
}

export async function archiveContract(
  collectionRoot: string,
  contractId: string,
): Promise<Contract> {
  return invoke('archive_contract', { collectionRoot, contractId });
}

export async function unarchiveContract(
  collectionRoot: string,
  contractId: string,
): Promise<Contract> {
  return invoke('unarchive_contract', { collectionRoot, contractId });
}

export async function duplicateContract(
  collectionRoot: string,
  contractId: string,
): Promise<Contract> {
  return invoke('duplicate_contract', { collectionRoot, contractId });
}

export async function recomputeDrift(collectionRoot: string): Promise<ContractDriftSummary[]> {
  return invoke('recompute_drift', { collectionRoot });
}

export async function getContractSummary(collectionRoot: string): Promise<ContractSummary[]> {
  return invoke('get_contract_summary', { collectionRoot });
}

/** Returns an OpenAPI 3.0 YAML stub for a contract as a string.
 *  The caller is responsible for triggering the native save dialog. */
export async function exportContractOpenapi(
  collectionRoot: string,
  contractId: string,
): Promise<string> {
  return invoke('export_contract_openapi', { collectionRoot, contractId });
}

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

// ============================================================
// Secret Manager connections (RocketVault external secrets)
// ============================================================

export const listSecretManagerConnections = () =>
  invoke<SecretManagerConnection[]>('list_secret_manager_connections');

export const saveSecretManagerConnection = (
  connection: SecretManagerConnection,
  clientSecret?: string,
) =>
  invoke<void>('save_secret_manager_connection', {
    connection,
    clientSecret,
  });

export const deleteSecretManagerConnection = (id: string) =>
  invoke<void>('delete_secret_manager_connection', { id });

export const testSecretManagerConnection = (id: string, vaultName: string) =>
  invoke<void>('test_secret_manager_connection', { id, vaultName });

export const fetchExternalSecretNames = (id: string, vaultName: string) =>
  invoke<ExternalSecretRef[]>('fetch_external_secret_names', { id, vaultName });

export const listVaultCertificates = (connectionId: string, vaultName: string) =>
  invoke<VaultCertificateSummary[]>('list_vault_certificates', { connectionId, vaultName });

// ============================================================
// Agent configs (ACP AI assist)
// ============================================================

export const listAgentConfigs = () => invoke<AgentConfig[]>('list_agent_configs');

export const saveAgentConfig = (config: AgentConfig) =>
  invoke<void>('save_agent_config', { config });

export const deleteAgentConfig = (id: string) => invoke<void>('delete_agent_config', { id });

export const testAgentConfig = (id: string) => invoke<void>('test_agent_config', { id });

// ============================================================
// Flow (visual workflow builder)
// ============================================================

export interface NodePosition {
  x: number;
  y: number;
}

export interface InlineHeader {
  name: string;
  value: string;
}

export interface InlineRequestData {
  method: string;
  url: string;
  headers: InlineHeader[];
  // The backend sends null for "no body"; it accepts null or absent.
  body?: string | null;
}

export type RequestSource =
  | { type: 'Saved'; requestPath: string }
  | { type: 'Inline'; request: InlineRequestData };

/** One Switch case. Edges leave a case through the handle `case:<id>`. */
export interface SwitchCase {
  id: string;
  label: string;
  matches: string;
}

/** Polling settings of a Request node. Mirrors the Rust `RepeatUntilDto`. */
export interface RepeatUntil {
  condition: string;
  intervalMs: number;
  maxAttempts: number;
  timeoutMs: number;
}

export type FlowNodeKind =
  | {
      kind: 'Request';
      label: string;
      source: RequestSource;
      debug?: boolean;
      repeatUntil?: RepeatUntil | null;
    }
  | { kind: 'Input'; label: string; value: unknown }
  | { kind: 'Output'; label: string }
  | { kind: 'If'; label: string; condition: string }
  | { kind: 'Switch'; label: string; value: string; cases: SwitchCase[] }
  | {
      kind: 'WaitForCallback';
      label: string;
      /** Letters, digits and `_`; unique in the flow. Used as `{{callback.<name>}}`. */
      name: string;
      timeoutMs: number;
      /** Optional condition over `request`. Null or absent accepts the first call. */
      acceptWhen?: string | null;
    }
  | {
      kind: 'Transform';
      label: string;
      /** One expression or a function body that returns a value. It reads `response`. */
      script: string;
    }
  | {
      kind: 'Auth';
      label: string;
      /** The auth configuration. A token is never stored here. */
      auth: Auth;
      /** When true, every Request in the flow whose auth is inherit uses this credential. */
      applyToInherit: boolean;
    };

export interface FlowNode {
  id: string;
  kind: FlowNodeKind;
  position: NodePosition;
}

export interface FlowEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  targetField: string;
  expression: string;
  /** Exit of the source node the edge leaves from. Absent or `result` means the default exit. */
  sourceHandle?: string;
}

export interface Flow {
  name: string;
  nodes: FlowNode[];
  edges: FlowEdge[];
  /** Host used in callback URLs. Absent or null means this machine's LAN IP. */
  callbackHost?: string | null;
}

export type FlowNodeStatus = 'idle' | 'running' | 'success' | 'failed' | 'skipped';

export const listFlows = (collection: string) => invoke<string[]>('list_flows', { collection });

export const getFlow = (collection: string, name: string) =>
  invoke<Flow>('get_flow', { collection, name });

export const saveFlow = (collection: string, flow: Flow) =>
  invoke<void>('save_flow', { collection, flow });

export type FlowLintSeverity = 'error' | 'warning';

/** One finding of the backend lint tier. Optional keys are omitted, never null. */
export interface FlowLint {
  code: string;
  severity: FlowLintSeverity;
  nodeId?: string;
  edgeId?: string;
  message: string;
  hint?: string;
}

/** Lints the graph as the canvas holds it now, saved or not. */
export const lintFlow = (collection: string, flow: Flow) =>
  invoke<FlowLint[]>('lint_flow', { collection, flow });

export const deleteFlow = (collection: string, name: string) =>
  invoke<void>('delete_flow', { collection, name });

export const renameFlow = (collection: string, oldName: string, newName: string) =>
  invoke<void>('rename_flow', { collection, oldName, newName });

/** Backend-reported node status. `'idle'` is frontend-only. */
export type FlowRunNodeStatus = Exclude<FlowNodeStatus, 'idle'>;

/** Why a skipped node did not run. Only set on skipped steps. */
export type FlowSkipReason = 'upstream_failed' | 'branch_not_taken';

/** One line of script console output captured during a flow step. */
export interface FlowLogEntry {
  level: 'log' | 'warn' | 'error';
  message: string;
}

/** `run_flow`'s return value. Camel-cased by the Rust IPC DTO. */
export interface FlowStepResult {
  nodeId: string;
  status: FlowRunNodeStatus;
  statusCode: number | null;
  durationMs: number | null;
  error: string | null;
  value: string | null;
  skipReason?: FlowSkipReason;
  /** Exit a completed If/Switch node took: `true`, `false`, `case:<id>` or `default`. */
  branch?: string;
  /** Script console output from this step. Omitted when empty. */
  logs?: FlowLogEntry[];
  /** Sent request of a debug node. */
  debugRequest?: FlowDebugRequest;
  /** Masked request and response of a step that sent. */
  exchange?: FlowDebugRequest;
  /** How many times a repeat-until Request node sent its request. */
  attempts?: number;
  /** Wire values and routing decision of the step, masked and capped. */
  trace?: FlowStepTrace;
}

/** One header line in a debug record, already masked by the backend. */
export interface FlowDebugHeader {
  key: string;
  value: string;
}

/** The response half of a debug record. */
export interface FlowDebugResponse {
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  headers: FlowDebugHeader[];
  body: string;
  /** True when the body was cut at 256 KB. */
  truncated?: boolean;
}

/** The request a debug node sent and what came back. */
export interface FlowDebugRequest {
  method: string;
  url: string;
  headers: FlowDebugHeader[];
  body?: string;
  /** True when the request body was cut at 256 KB. */
  bodyTruncated?: boolean;
  response?: FlowDebugResponse;
  error?: string;
}

/** The value one wire delivered to a step. Masked and size-capped by the backend. */
export interface FlowWireValue {
  edgeId: string;
  sourceNodeId: string;
  targetField: string;
  /** Absent for a credential wire and for a wire that failed before it had a value. */
  value?: string;
  /** True when `value` was cut at 16 KB or by the 64 KB per-step budget. */
  truncated?: boolean;
  /** True for an `auth` wire. Its credential is never sent to the UI. */
  credential?: boolean;
  error?: string;
}

/** How an If or Switch node decided. `value` is masked and cut at 1 KB. */
export interface FlowRouteEval {
  kind: 'if' | 'switch';
  /** `true` or `false` for an If, the evaluated value for a Switch. */
  value: string;
  /** The Switch case id that matched. Absent for If and for the default exit. */
  matchedCase?: string;
}

/** What one step saw and decided. Every key is optional. */
export interface FlowStepTrace {
  wires?: FlowWireValue[];
  route?: FlowRouteEval;
  poll?: FlowPollDetail;
  wait?: FlowWaitDetail;
  /** The wire whose failure failed the step. */
  failedEdgeId?: string;
  /** True when the step's `value` was cut at 256 KB. */
  valueTruncated?: boolean;
}

/** A call a Wait for callback node turned down. Masked by the backend. */
export interface FlowRejectedCall {
  method: string;
  /** Path and query. The token path is shown as `/cb/…`. */
  url: string;
  headers: FlowDebugHeader[];
  body: string;
  /** True when the body was cut: at 2 KB in a live event, at 256 KB in the trace. */
  bodyTruncated?: boolean;
  reason: string;
}

/** How a repeat-until poll went. */
export interface FlowPollDetail {
  attempts: number;
  maxAttempts: number;
  lastStatusCode?: number;
  /** Absent when no verdict was reached, as after a condition script error. */
  conditionMet?: boolean;
  elapsedMs: number;
  timeoutMs: number;
}

/** How a callback wait went. */
export interface FlowWaitDetail {
  ignored: number;
  timeoutMs: number;
  lastRejected?: FlowRejectedCall;
}

/** Structured progress of a node that is still running. Every key is optional. */
export interface FlowLiveProgress {
  lastStatusCode?: number;
  conditionMet?: boolean;
  elapsedMs?: number;
  /** Time left before the node gives up. The UI counts down from it. */
  remainingMs?: number;
  ignored?: number;
  lastRejected?: FlowRejectedCall;
}

/** The callback URL of one Wait for callback node. Valid only while its run is active. */
export interface FlowCallbackInfo {
  nodeId: string;
  name: string;
  url: string;
}

export interface FlowRunSummary {
  runId: string;
  steps: FlowStepResult[];
  stoppedReason: 'completed' | 'cancelled' | string;
  /** Set for a partial run. */
  partial?: FlowPartialRunInfo;
}

/** A token the UI obtained for an Auth node. Held in memory only, never persisted. */
export interface FlowAuthToken {
  accessToken: string;
}

/** Which nodes a partial run executes: one node, or a node and everything below it. */
export type FlowPartialMode = 'node' | 'fromHere';

/** Asks run_flow to re-run part of the flow on top of the run `baseRunId`. */
export interface FlowPartialRunRequest {
  baseRunId: string;
  startNodeId: string;
  mode: FlowPartialMode;
}

/** Describes a partial run on flow-run-started and on the summary. Keys are camelCase in both. */
export interface FlowPartialRunInfo extends FlowPartialRunRequest {
  /** Every node the run executes, in order. */
  nodeIds: string[];
}

/** Extra settings for one run_flow call. */
export interface RunFlowOptions {
  /** Re-runs part of the flow on top of an earlier run. A full run omits it. */
  partial?: FlowPartialRunRequest;
  /**
   * Run id chosen by the client, a UUID. The backend uses it in every
   * flow-run-* event and for Stop, so a tab matches only its own run. The
   * backend picks one when absent and refuses a malformed or used id.
   */
  runId?: string;
}

/**
 * Runs a flow. The promise resolves only when the run ENDS. Subscribe to
 * the flow-run-* events before calling this, and match them by
 * `options.runId`.
 */
export const runFlow = (
  collection: string,
  flowName: string,
  environmentName?: string | null,
  globalEnvName?: string | null,
  authTokens?: Record<string, FlowAuthToken>,
  options?: RunFlowOptions,
) =>
  invoke<FlowRunSummary>('run_flow', {
    input: {
      collection,
      flowName,
      environmentName: environmentName ?? null,
      globalEnvName: globalEnvName ?? null,
      // Sent only when there is something to send, so a flow without Auth nodes
      // calls the command exactly as before.
      ...(authTokens && Object.keys(authTokens).length > 0 ? { authTokens } : {}),
      // Sent only when chosen, so other callers keep the old payload.
      ...(options?.runId ? { runId: options.runId } : {}),
      // A full run sends no partial key.
      ...(options?.partial ? { partial: options.partial } : {}),
    },
  });

export const cancelFlowRun = (runId: string) => invoke<void>('cancel_flow_run', { runId });

// Event payloads are DomainEvent JSON. Their fields are snake_case, like
// every other DomainEvent. Do not camelCase them here.
export interface FlowRunStartedEvent {
  type: 'flowRunStarted';
  run_id: string;
  flow_name: string;
  collection: string;
  total_nodes: number;
  /** Every Wait for callback node's URL. Omitted when the flow has none. Keys are camelCase. */
  callbacks?: FlowCallbackInfo[];
  /** Set for a partial run. `total_nodes` then counts only its nodes. */
  partial?: FlowPartialRunInfo;
}

export const onFlowRunStarted = (
  handler: (event: FlowRunStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowRunStartedEvent>('flow-run-started', (e) => handler(e.payload));

export interface FlowStepStartedEvent {
  type: 'flowStepStarted';
  run_id: string;
  node_id: string;
}

export const onFlowStepStarted = (
  handler: (event: FlowStepStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowStepStartedEvent>('flow-step-started', (e) => handler(e.payload));

export interface FlowStepCompletedEvent {
  type: 'flowStepCompleted';
  run_id: string;
  node_id: string;
  status: FlowRunNodeStatus;
  status_code: number | null;
  duration_ms: number | null;
  error: string | null;
  value: string | null;
  skip_reason?: FlowSkipReason;
  branch?: string;
  /** Script console output from this step. Omitted when empty. */
  logs?: FlowLogEntry[];
  /** Sent request of a debug node. */
  debug_request?: FlowDebugRequest;
  /** Masked request and response of a step that sent. */
  exchange?: FlowDebugRequest;
  /** How many times a repeat-until Request node sent its request. */
  attempts?: number;
  /** Wire values and routing decision of the step. Its keys are camelCase. */
  trace?: FlowStepTrace;
}

export const onFlowStepCompleted = (
  handler: (event: FlowStepCompletedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowStepCompletedEvent>('flow-step-completed', (e) => handler(e.payload));

export interface FlowStepProgressEvent {
  type: 'flowStepProgress';
  run_id: string;
  node_id: string;
  /** 1-based attempt number, or null when attempts do not apply. */
  attempt: number | null;
  max_attempts: number | null;
  /** Short text shown on the node, such as "attempt 3/30". */
  message: string;
  /** Structured progress. Omitted by older backends. Keys are camelCase. */
  live?: FlowLiveProgress;
}

export const onFlowStepProgress = (
  handler: (event: FlowStepProgressEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowStepProgressEvent>('flow-step-progress', (e) => handler(e.payload));

export interface FlowRunFinishedEvent {
  type: 'flowRunFinished';
  run_id: string;
  stopped_reason: string;
  node_count: number;
  failed_count: number;
  skipped_count: number;
  /** How many of `skipped_count` were skipped because their branch was not taken. */
  not_taken_count?: number;
}

export const onFlowRunFinished = (
  handler: (event: FlowRunFinishedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowRunFinishedEvent>('flow-run-finished', (e) => handler(e.payload));

// ==== AI Assist (ACP chat sessions) ====

/** One choice of a session option. */
export interface ConfigChoice {
  value: string;
  name: string;
  description: string | null;
}

/** A session option the agent reports, such as the model or the effort level. */
export interface ConfigOption {
  id: string;
  name: string;
  /** `model`, `thought_level`, `mode`, `model_config`, or another agent value. */
  category: string | null;
  currentValue: string;
  choices: ConfigChoice[];
}

export interface AgentSessionStarted {
  sessionId: string;
  configOptions: ConfigOption[];
}

/** A text resource sent with a prompt, such as a request definition. */
export interface PromptResourceDto {
  uri: string;
  mimeType: string | null;
  text: string;
}

/** The chip kinds whose text the backend builds from its masked views. */
export type AssistantChipKind = 'request' | 'folder' | 'collection' | 'environment';

/** The last response of a request tab, sent to the backend to be masked. */
export interface AssistantResponseChip {
  method: string;
  url: string;
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  headers: { key: string; value: string }[];
  body: string;
  isBinary: boolean;
  tests: { name: string; passed: boolean; error?: string | null }[];
  /** The tab's request as it is on screen, so unsaved credentials are masked too. */
  request?: {
    headers: Header[];
    queryParams: QueryParam[];
    body?: Body;
    auth: Auth;
  };
}

/**
 * The masked, size-capped text resource of a request, folder, collection or environment
 * chip. `path` is the request or folder path, or the environment name.
 */
export const buildAssistantChipResource = (
  kind: AssistantChipKind,
  collection: string,
  path?: string,
) =>
  invoke<PromptResourceDto>('build_assistant_chip_resource', {
    kind,
    collection,
    path: path ?? null,
  });

/**
 * Masks the last response of a request tab and returns it as a text resource.
 * `environmentName` is the tab's active environment, whose vault secrets are masked too.
 */
export const maskAssistantResponse = (
  collection: string,
  requestPath: string,
  response: AssistantResponseChip,
  environmentName?: string,
) =>
  invoke<PromptResourceDto>('mask_assistant_response', {
    collection,
    requestPath,
    environmentName: environmentName ?? null,
    response,
  });

/** Resolves with the stop reason. A stopped turn resolves with `cancelled`. */
export const sendAgentPrompt = (
  sessionId: string,
  prompt: string,
  resources?: PromptResourceDto[],
) => invoke<string>('send_agent_prompt', { sessionId, prompt, resources: resources ?? null });

/** Asks the agent to stop the running turn. The session stays open. */
export const cancelAgentPrompt = (sessionId: string) =>
  invoke<void>('cancel_agent_prompt', { sessionId });

/** Changes one session option and resolves with the agent's new option list. */
export const setAgentConfigOption = (sessionId: string, configId: string, value: string) =>
  invoke<ConfigOption[]>('set_agent_config_option', { sessionId, configId, value });

export const endAgentSession = (sessionId: string) =>
  invoke<void>('end_agent_session', { sessionId });

/** The workspace assistant's Rocket mode. Matches the backend `AssistantMode`. */
export type AssistantMode = 'ask' | 'edit' | 'agent';

export const setAssistantMode = (sessionId: string, mode: AssistantMode) =>
  invoke<void>('set_assistant_mode', { sessionId, mode });

export const startWorkspaceAssistant = (
  agentConfigId: string,
  mode: AssistantMode,
  model?: string,
) =>
  invoke<AgentSessionStarted>('start_workspace_assistant', {
    agentConfigId,
    mode,
    model: model ?? null,
  });

/**
 * Ends every agent session the backend still tracks and resolves to how many
 * it ended. Call once per webview load, before starting a session (Plan 05's
 * assistant event bridge does this, and every start waits for it). Sessions
 * started by this webview would be ended too.
 */
export const endStaleAssistantSessions = () => invoke<number>('end_stale_assistant_sessions');

// Event payloads are DomainEvent JSON. Their fields are snake_case, like
// every other DomainEvent. Do not camelCase them here.
export interface AgentSessionStartedEvent {
  type: 'acpSessionStarted';
  session_id: string;
}

export const onAgentSessionStarted = (
  handler: (event: AgentSessionStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionStartedEvent>('agent-session-started', (e) => handler(e.payload));

export interface AgentSessionChunkEvent {
  type: 'acpSessionChunk';
  session_id: string;
  text: string;
}

export const onAgentSessionChunk = (
  handler: (event: AgentSessionChunkEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionChunkEvent>('agent-session-chunk', (e) => handler(e.payload));

export interface AgentSessionFinishedEvent {
  type: 'acpSessionFinished';
  session_id: string;
  stop_reason: string;
}

export const onAgentSessionFinished = (
  handler: (event: AgentSessionFinishedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionFinishedEvent>('agent-session-finished', (e) => handler(e.payload));

// ============================================================
// WebSocket sessions
// ============================================================

/** Where `{{variables}}` come from for a connect or send. */
export interface WebSocketScopeInput {
  collection?: string;
  environmentName?: string;
  globalEnvName?: string;
  requestPath?: string;
}

export interface WebSocketConnectInput extends WebSocketScopeInput {
  url: string;
  headers: Header[];
  auth?: Auth;
  subprotocols?: string[];
  timeoutMs?: number;
  keepAliveMs?: number;
  verifySsl?: boolean;
}

export interface WebSocketSendInput extends WebSocketScopeInput {
  kind: WebSocketMessageKind;
  data: string;
}

/** `ws_connect` only reports whether the connect worked. Frames arrive as events. */
export const wsConnect = (sessionId: string, input: WebSocketConnectInput) =>
  invoke<void>('ws_connect', { sessionId, input });

export const wsSend = (sessionId: string, input: WebSocketSendInput) =>
  invoke<void>('ws_send', { sessionId, input });

export const wsDisconnect = (sessionId: string) => invoke<void>('ws_disconnect', { sessionId });

/** Payload of the `ws:message` event. Fields are snake_case, like every `DomainEvent`. */
export interface WebSocketMessageEvent {
  type: 'webSocketMessage';
  session_id: string;
  direction: 'in' | 'out';
  kind: 'text' | 'binary';
  /** Text as is, or base64 for binary frames. */
  data: string;
  size: number;
  timestamp_ms: number;
}

export interface WebSocketStatusEvent {
  type: 'webSocketStatus';
  session_id: string;
  state: 'connecting' | 'open' | 'closed' | 'failed';
  subprotocol: string | null;
  code: number | null;
  reason: string | null;
}

export const onWebSocketMessage = (
  handler: (event: WebSocketMessageEvent) => void,
): Promise<UnlistenFn> => listen<WebSocketMessageEvent>('ws:message', (e) => handler(e.payload));

export const onWebSocketStatus = (
  handler: (event: WebSocketStatusEvent) => void,
): Promise<UnlistenFn> => listen<WebSocketStatusEvent>('ws:status', (e) => handler(e.payload));

export interface AgentSessionFailedEvent {
  type: 'acpSessionFailed';
  session_id: string;
  error: string;
}

export const onAgentSessionFailed = (
  handler: (event: AgentSessionFailedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionFailedEvent>('agent-session-failed', (e) => handler(e.payload));

// ==== AI Assistant proposals ====
// Proposals are DTOs, so their fields are camelCase. The two events below
// are DomainEvent JSON, so their fields stay snake_case.

export type AgentProposalStatus = 'pending' | 'accepted' | 'rejected' | 'stale' | 'failed';

export interface AgentProposedRequest {
  name: string;
  method: HttpMethod;
  url: string;
  headers: Header[];
  queryParams: QueryParam[];
  body?: Body;
  docs?: string;
  preRequestScript?: string;
  postResponseScript?: string;
  tests?: string;
}

/** Only the fields the patch sets are present. */
export interface AgentRequestPatch {
  method?: HttpMethod;
  url?: string;
  headers?: Header[];
  queryParams?: QueryParam[];
  body?: Body;
  docs?: string;
}

export type AgentProposedChange =
  | { op: 'createFolder'; collection: string; parentPath: string; name: string }
  | { op: 'createRequest'; collection: string; folderPath: string; request: AgentProposedRequest }
  | { op: 'updateRequest'; collection: string; requestPath: string; patch: AgentRequestPatch }
  | {
      op: 'editScript';
      collection: string;
      requestPath: string;
      phase: 'preRequest' | 'postResponse' | 'tests';
      body: string;
    }
  | { op: 'moveItem'; collection: string; fromPath: string; toFolder: string }
  | { op: 'renameItem'; collection: string; path: string; newName: string }
  | { op: 'setEnvVar'; collection: string; environment: string; key: string; value: string };

export interface AgentProposal {
  id: string;
  sessionId: string;
  change: AgentProposedChange;
  summary: string;
  status: AgentProposalStatus;
  /** Why a proposal failed. Present only when `status` is `failed`. */
  statusMessage?: string;
  createdAtMs: number;
}

export const listAgentProposals = (sessionId: string) =>
  invoke<AgentProposal[]>('list_agent_proposals', { sessionId });

export const acceptAgentProposal = (sessionId: string, proposalId: string) =>
  invoke<AgentProposal>('accept_agent_proposal', { sessionId, proposalId });

export const rejectAgentProposal = (sessionId: string, proposalId: string) =>
  invoke<AgentProposal>('reject_agent_proposal', { sessionId, proposalId });

export interface AgentProposalCreatedEvent {
  type: 'acpProposalCreated';
  session_id: string;
  proposal_id: string;
  summary: string;
}

export interface AgentProposalResolvedEvent {
  type: 'acpProposalResolved';
  session_id: string;
  proposal_id: string;
  status: Exclude<AgentProposalStatus, 'pending'>;
}

export const onAgentProposalCreated = (
  handler: (event: AgentProposalCreatedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentProposalCreatedEvent>('agent-proposal-created', (e) => handler(e.payload));

export const onAgentProposalResolved = (
  handler: (event: AgentProposalResolvedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentProposalResolvedEvent>('agent-proposal-resolved', (e) => handler(e.payload));

export type AgentToolCallStatus = 'pending' | 'in_progress' | 'completed' | 'failed';

export interface AgentToolActivityEvent {
  type: 'acpToolActivity';
  session_id: string;
  call_id: string;
  title: string;
  status: AgentToolCallStatus;
}

export const onAgentToolActivity = (
  handler: (event: AgentToolActivityEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentToolActivityEvent>('agent-session-tool-activity', (e) => handler(e.payload));

/** A config option as the event carries it. Keys are snake_case, like every event field. */
export interface AgentConfigOptionPayload {
  id: string;
  name: string;
  category: string | null;
  current_value: string;
  choices: ConfigChoice[];
}

export interface AgentConfigOptionsEvent {
  type: 'acpConfigOptionsChanged';
  session_id: string;
  options: AgentConfigOptionPayload[];
}

export const onAgentConfigOptions = (
  handler: (event: AgentConfigOptionsEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentConfigOptionsEvent>('agent-session-config-options', (e) => handler(e.payload));

export interface AgentUsageEvent {
  type: 'acpUsage';
  session_id: string;
  used: number;
  size: number;
  /** Cumulative session cost in US dollars, or null when not reported in dollars. */
  cost_usd: number | null;
}

export const onAgentUsage = (handler: (event: AgentUsageEvent) => void): Promise<UnlistenFn> =>
  listen<AgentUsageEvent>('agent-session-usage', (e) => handler(e.payload));

/** Converts the event's options to the camelCase shape the commands return. */
export function configOptionsFromEvent(options: AgentConfigOptionPayload[]): ConfigOption[] {
  return options.map((option) => ({
    id: option.id,
    name: option.name,
    category: option.category,
    currentValue: option.current_value,
    choices: option.choices,
  }));
}

// ============================================================
// Proxy
// ============================================================

export type ProxyMode = 'system' | 'none' | 'custom';

export interface ProxySettings {
  mode: ProxyMode;
  httpProxy?: string;
  httpsProxy?: string;
  noProxy?: string;
  username?: string;
}

/** The saved setting. The password itself is never returned, only whether one is stored. */
export interface ProxySettingsView extends ProxySettings {
  hasPassword: boolean;
}

export type ProxyPasswordChange =
  | { action: 'keep' }
  | { action: 'clear' }
  | { action: 'set'; value: string };

export const getProxySettings = () => invoke<ProxySettingsView>('get_proxy_settings');

export const saveProxySettings = (settings: ProxySettings, password: ProxyPasswordChange) =>
  invoke<void>('save_proxy_settings', { settings, password });

// ============================================================
// GraphQL subscriptions
// ============================================================

export interface GraphQlSubscribeInput extends WebSocketScopeInput {
  /** The request URL. `http` and `https` are turned into `ws` and `wss`. */
  url: string;
  /** Where subscriptions are served when that differs from `url`. */
  subscriptionUrl?: string;
  query: string;
  /** JSON object text. */
  variables?: string;
  operationName?: string;
  /** JSON object text sent with `connection_init`. */
  connectionParams?: string;
  headers: Header[];
  auth?: Auth;
  verifySsl?: boolean;
  timeoutMs?: number;
}

/** `graphql_subscribe` only reports whether the socket opened. Results arrive as events. */
export const graphqlSubscribe = (sessionId: string, input: GraphQlSubscribeInput) =>
  invoke<void>('graphql_subscribe', { sessionId, input });

export const graphqlUnsubscribe = (sessionId: string) =>
  invoke<void>('graphql_unsubscribe', { sessionId });

/** Payload of `graphql:subscription-message`. Fields are snake_case, like every `DomainEvent`. */
export interface GraphQlSubscriptionMessageEvent {
  type: 'graphQlSubscriptionMessage';
  session_id: string;
  event: 'next' | 'error' | 'complete';
  /** Pretty-printed JSON. Empty for `complete`. */
  data: string;
  timestamp_ms: number;
}

export interface GraphQlSubscriptionStatusEvent {
  type: 'graphQlSubscriptionStatus';
  session_id: string;
  state: 'connecting' | 'open' | 'closed' | 'failed';
  /** The subprotocol the server selected. */
  dialect: string | null;
  reason: string | null;
}

export const onGraphQlSubscriptionMessage = (
  handler: (event: GraphQlSubscriptionMessageEvent) => void,
): Promise<UnlistenFn> =>
  listen<GraphQlSubscriptionMessageEvent>('graphql:subscription-message', (e) =>
    handler(e.payload),
  );

export const onGraphQlSubscriptionStatus = (
  handler: (event: GraphQlSubscriptionStatusEvent) => void,
): Promise<UnlistenFn> =>
  listen<GraphQlSubscriptionStatusEvent>('graphql:subscription-status', (e) => handler(e.payload));

// ============================================================
// gRPC calls
// ============================================================

/** One metadata (header or trailer) line. Binary values are base64 text. */
export interface GrpcPair {
  name: string;
  value: string;
}

export interface GrpcStatus {
  /** Canonical gRPC code, 0 is OK. */
  code: number;
  codeName: string;
  message: string;
}

export interface GrpcUnaryResponse {
  headers: GrpcPair[];
  trailers: GrpcPair[];
  /** Protobuf JSON of the reply. Absent when the call failed. */
  messageJson?: string | null;
  status: GrpcStatus;
  durationMs: number;
}

export interface GrpcMethodInfo {
  name: string;
  /** `package.Service/Method`, the value stored in a request. */
  fullName: string;
  methodType: GrpcMethodType;
  inputType: string;
  outputType: string;
}

export interface GrpcServiceInfo {
  name: string;
  methods: GrpcMethodInfo[];
}

/** What the gRPC tab sends. `request` is the editor state, which may be unsaved. */
export interface GrpcExecuteInput {
  collection?: string;
  request: GrpcRequest;
  /** The message to send. Omitted means the selected saved message. */
  message?: string;
  environmentName?: string;
  globalEnvName?: string;
  requestPath?: string;
  /** Deadline in milliseconds. 0 or absent means none. */
  timeoutMs?: number;
}

export const grpcUnaryCall = (input: GrpcExecuteInput) =>
  invoke<GrpcUnaryResponse>('grpc_unary_call', { input });

/**
 * Opens a streaming call under `sessionId`, which the caller chooses so it can listen and
 * cancel before the connection is up. Messages arrive as events. Resolves to the same id.
 */
export const grpcStartSession = (input: GrpcExecuteInput, sessionId: string) =>
  invoke<string>('grpc_start_session', { input, sessionId });

export const grpcSendMessage = (sessionId: string, message: string) =>
  invoke<void>('grpc_send_message', { sessionId, message });

/** Ends the request side of a streaming call (half-close). */
export const grpcEndRequests = (sessionId: string) =>
  invoke<void>('grpc_end_requests', { sessionId });

export const grpcCancelSession = (sessionId: string) =>
  invoke<void>('grpc_cancel_session', { sessionId });

/** Lists services from the request's .proto file, or from server reflection when it has none. */
export const grpcListServices = (input: GrpcExecuteInput, refresh: boolean) =>
  invoke<GrpcServiceInfo[]>('grpc_list_services', { input, refresh });

// Event fields stay snake_case on the wire, like the agent session events.
export interface GrpcSessionStartedEvent {
  type: 'grpcSessionStarted';
  session_id: string;
  method_type: string;
}

export interface GrpcSessionHeadersEvent {
  type: 'grpcSessionHeaders';
  session_id: string;
  headers: GrpcPair[];
}

export interface GrpcSessionMessageEvent {
  type: 'grpcSessionMessage';
  session_id: string;
  index: number;
  json: string;
}

export interface GrpcSessionFinishedEvent {
  type: 'grpcSessionFinished';
  session_id: string;
  code: number;
  code_name: string;
  message: string;
  trailers: GrpcPair[];
  duration_ms: number;
}

export const onGrpcSessionStarted = (
  handler: (event: GrpcSessionStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionStartedEvent>('grpc-session-started', (e) => handler(e.payload));

export const onGrpcSessionHeaders = (
  handler: (event: GrpcSessionHeadersEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionHeadersEvent>('grpc-session-headers', (e) => handler(e.payload));

export const onGrpcSessionMessage = (
  handler: (event: GrpcSessionMessageEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionMessageEvent>('grpc-session-message', (e) => handler(e.payload));

export const onGrpcSessionFinished = (
  handler: (event: GrpcSessionFinishedEvent) => void,
): Promise<UnlistenFn> =>
  listen<GrpcSessionFinishedEvent>('grpc-session-finished', (e) => handler(e.payload));
