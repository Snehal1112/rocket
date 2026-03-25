/**
 * TypeScript bridge — wraps every Tauri command with a typed invoke() call.
 * All types mirror the Rust structs (camelCase via serde rename_all).
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ============================================================
// Domain types (mirror Rust structs)
// ============================================================

export type HttpMethod =
  | "GET"
  | "POST"
  | "PUT"
  | "PATCH"
  | "DELETE"
  | "OPTIONS"
  | "HEAD";

export interface Header {
  key: string;
  value: string;
  enabled: boolean;
}

export type BodyMode = "none" | "json" | "xml" | "text" | "formdata" | "binary";

export interface FormDataEntry {
  key: string;
  value: string;
  entryType: "text" | "file";
  enabled: boolean;
}

export interface Body {
  mode: BodyMode;
  content?: string;
  formData?: FormDataEntry[];
}

export type Auth =
  | { authType: "none" }
  | { authType: "basic"; username: string; password: string }
  | { authType: "bearer"; token: string }
  | { authType: "api-key"; key: string; value: string; addTo: "header" | "query" }
  | { authType: "oauth2"; [key: string]: unknown }
  | { authType: "aws-sig-v4"; [key: string]: unknown };

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
  description?: string;
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
}

export interface Folder {
  uid: string;
  name: string;
  items: CollectionItem[];
}

export type CollectionItem =
  | ({ type: "request" } & Request)
  | ({ type: "folder" } & Folder);

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
}

export interface FileChangedEvent {
  path: string;
  eventType: "create" | "modify" | "remove";
  collection?: string;
}

// ============================================================
// Collections
// ============================================================

export const listCollections = () =>
  invoke<CollectionSummary[]>("list_collections");

export const getCollection = (name: string) =>
  invoke<Collection>("get_collection", { name });

export const createCollection = (name: string) =>
  invoke<Collection>("create_collection", { name });

export const deleteCollection = (name: string) =>
  invoke<void>("delete_collection", { name });

export const renameCollection = (oldName: string, newName: string) =>
  invoke<void>("rename_collection", { oldName, newName });

export const saveRequest = (
  collection: string,
  path: string,
  request: Request,
) => invoke<Request>("save_request", { collection, path, request });

export const renameRequest = (collection: string, oldPath: string, newName: string) =>
  invoke<void>("rename_request", { collection, oldPath, newName });

export const deleteRequest = (collection: string, path: string) =>
  invoke<void>("delete_request", { collection, path });

export const createFolder = (collection: string, path: string) =>
  invoke<void>("create_folder", { collection, path });

export const deleteFolder = (collection: string, path: string) =>
  invoke<void>("delete_folder", { collection, path });

export const moveItem = (
  srcCollection: string,
  srcPath: string,
  dstCollection: string,
  dstPath: string,
) =>
  invoke<void>("move_item", {
    srcCollection,
    srcPath,
    dstCollection,
    dstPath,
  });

export async function reorderItems(collection: string, folderPath: string, orderedNames: string[]): Promise<void> {
  return invoke('reorder_items', { collection, folderPath, orderedNames });
}

export const getCollectionSettings = (name: string) =>
  invoke<CollectionSettings>("get_collection_settings", { name });

export const saveCollectionSettings = (
  collection: string,
  settings: Partial<CollectionSettings>,
) => invoke<void>("save_collection_settings", { collection, settings });

// ============================================================
// Environments
// ============================================================

export const listEnvironments = () =>
  invoke<Environment[]>("list_environments");

export const getEnvironment = (name: string) =>
  invoke<Environment>("get_environment", { name });

export const saveEnvironment = (env: Environment) =>
  invoke<void>("save_environment", { env });

export const deleteEnvironment = (name: string) =>
  invoke<void>("delete_environment", { name });

// ============================================================
// Request execution
// ============================================================

export const executeRequest = (input: ExecuteRequestInput) =>
  invoke<HttpResponse>("execute_request", { input });

// ============================================================
// History
// ============================================================

export const listHistory = (limit?: number) =>
  invoke<HistoryEntry[]>("list_history", { limit });

export const getHistoryEntry = (id: string) =>
  invoke<HistoryEntry>("get_history_entry", { id });

export const clearHistory = () => invoke<void>("clear_history");

export const searchHistory = (filter: HistoryFilter) =>
  invoke<HistoryEntry[]>("search_history", { filter });

// ============================================================
// Templates
// ============================================================

export const listTemplates = () => invoke<Template[]>("list_templates");

export const getTemplate = (name: string) =>
  invoke<Template>("get_template", { name });

export const saveTemplate = (template: Template) =>
  invoke<void>("save_template", { template });

export const deleteTemplate = (name: string) =>
  invoke<void>("delete_template", { name });

// ============================================================
// Cookies
// ============================================================

export const getCookies = () => invoke<CookieJar[]>("get_cookies");

export const setCookies = (jar: CookieJar) =>
  invoke<void>("set_cookies", { jar });

export const clearCookies = () => invoke<void>("clear_cookies");

// ============================================================
// App utility
// ============================================================

export const getAppDataDir = () => invoke<string>("get_app_data_dir");

export const watchCollections = () => invoke<void>("watch_collections");

export const stopWatching = () => invoke<void>("stop_watching");

// ============================================================
// Realtime events
// ============================================================

export const onFileChange = (
  handler: (event: FileChangedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FileChangedEvent>("collection-changed", (e) => handler(e.payload));

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
  listen<CollectionChangedEvent>("collection-changed", (e) => handler(e.payload));

export const onRequestExecuted = (
  handler: () => void,
): Promise<UnlistenFn> =>
  listen("request-executed", () => handler());

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
) => invoke<OAuth2TokenResponse>("oauth2_auth_code_flow", {
  authorizationUrl, tokenUrl, clientId, clientSecret, scope, callbackUrl, verifySsl,
});
