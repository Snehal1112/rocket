import { findTabInTree } from '@/lib/pane-utils';
import {
  type Auth,
  type Body,
  type CollectionVariable,
  executeRequest,
  getCollectionSettings,
  getFolderChainVariables,
  getRequestVariables,
  type Header,
} from '@/lib/tauri-api';
import { buildVariableContext, resolveWithContext } from '@/lib/variable-context';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useConsoleStore } from '@/stores/console-store';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { AuthState, BodyState, RequestState, ResponseState } from '@/types/pane-types';

export function toApiAuth(auth: AuthState, resolve = (s: string) => s): Auth {
  switch (auth.authType) {
    case 'inherit':
    case 'none':
      return { authType: 'none' };
    case 'basic':
      return {
        authType: 'basic',
        username: resolve(auth.basic?.username ?? ''),
        password: resolve(auth.basic?.password ?? ''),
      };
    case 'bearer':
      return { authType: 'bearer', token: resolve(auth.bearer?.token ?? '') };
    case 'api-key':
      return {
        authType: 'api-key',
        key: resolve(auth.apiKey?.key ?? ''),
        value: resolve(auth.apiKey?.value ?? ''),
        placement: auth.apiKey?.addTo ?? 'header',
      };
    case 'oauth2':
      // Send the stored access token as a bearer token.
      return {
        authType: 'bearer',
        token: resolve(auth.oauth2?.accessToken ?? ''),
      };
    case 'aws-sig-v4':
      // AWS Signature V4 is not yet supported by the backend; falling back to none.
      console.warn('Auth type aws-sig-v4 is not yet supported, falling back to none.');
      return { authType: 'none' };
    default: {
      const unsupported = (auth as AuthState).authType;
      console.warn(`Unsupported auth type: ${unsupported}, falling back to none.`);
      return { authType: 'none' };
    }
  }
}

export function toApiBody(body: BodyState, resolve = (s: string) => s): Body | undefined {
  if (body.mode === 'none') return undefined;
  if (body.mode === 'formdata') {
    return {
      mode: 'formdata',
      formData: body.formData
        .filter((e) => e.enabled)
        .map((e) => ({
          key: resolve(e.key),
          value: resolve(e.value),
          entryType: 'text' as const,
          enabled: e.enabled,
        })),
    };
  }
  return { mode: body.mode as Body['mode'], content: resolve(body.content) };
}

// Resolved request fields returned by resolveRequestFields().
export interface ResolvedRequestFields {
  url: string;
  headers: Header[];
  queryParams: { key: string; value: string; enabled: boolean }[];
  body: ReturnType<typeof toApiBody>;
  auth: Auth;
  collection: string | undefined;
  environmentName: string | undefined;
  requestPath: string | undefined;
}

// Builds the fully-resolved request fields for a given tab and request state.
// Applies the same 7-scope variable resolution as sendRequest() so that callers
// (e.g. the load test dialog) get consistent env var substitution.
export async function resolveRequestFields(
  tabId: string,
  request: RequestState,
): Promise<ResolvedRequestFields> {
  const envStore = useEnvStore.getState();
  const envVars = envStore.getActiveVariables();
  const globalVars = envStore.getGlobalVariables();
  const processEnvVars = envStore.processEnvVars;

  const { root } = usePaneStore.getState();
  const found = findTabInTree(root, tabId);
  const collection = found?.tab.source?.collection;
  const requestPath = found?.tab.source?.path;

  let collectionVars: CollectionVariable[] = [];
  let collectionHeaders: { key: string; value: string; enabled: boolean }[] = [];
  if (collection) {
    try {
      const settings = await getCollectionSettings(collection);
      collectionVars = settings.variables;
      collectionHeaders = settings.headers.filter((h) => h.enabled);
    } catch {
      // Collection settings unavailable — proceed without collection vars/headers.
    }
  }

  let folderVars: CollectionVariable[] = [];
  if (collection && requestPath) {
    try {
      folderVars = await getFolderChainVariables(collection, requestPath);
    } catch {
      // Non-critical: fall back to empty vars if chain lookup fails.
    }
  }

  let requestVars: CollectionVariable[] = [];
  if (collection && requestPath) {
    try {
      requestVars = await getRequestVariables(collection, requestPath);
    } catch {
      // Non-critical: fall back to empty vars if lookup fails.
    }
  }

  const ctx = buildVariableContext({
    processEnvVars,
    globalVars,
    envVars,
    collectionVars,
    folderVars,
    requestVars,
  });
  const resolve = (text: string) => resolveWithContext(text, ctx);

  let resolvedUrl = resolve(request.url);
  for (const p of request.pathParams) {
    if (p.enabled && p.key && p.value) {
      resolvedUrl = resolvedUrl.replace(`:${p.key}`, encodeURIComponent(p.value));
    }
  }

  const resolvedHeaders: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: h.enabled }));

  const resolvedBody = toApiBody(request.body, resolve);

  let authToResolve: AuthState = request.auth;
  if (request.auth.authType === 'inherit' && collection) {
    const storedAuth = useCollectionAuthStore.getState().getCollectionAuth(collection);
    if (storedAuth && storedAuth.authType !== 'none' && storedAuth.authType !== 'inherit') {
      authToResolve = storedAuth;
    }
  }
  const resolvedAuth = toApiAuth(authToResolve, resolve);

  const requestHeaderKeys = new Set(resolvedHeaders.map((h) => h.key.toLowerCase()));
  const effectiveHeaders: Header[] = [
    ...collectionHeaders
      .filter((h) => !requestHeaderKeys.has(h.key.toLowerCase()))
      .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: true })),
    ...resolvedHeaders,
  ];

  const resolvedQueryParams = request.queryParams
    .filter((p) => p.enabled)
    .map((p) => ({ key: resolve(p.key), value: resolve(p.value), enabled: p.enabled }));

  return {
    url: resolvedUrl,
    headers: effectiveHeaders,
    queryParams: resolvedQueryParams,
    body: resolvedBody,
    auth: resolvedAuth,
    collection,
    environmentName: envStore.activeEnvId ?? undefined,
    requestPath,
  };
}

// Executes a request and writes the response into the pane store.
// This is a plain async function so it can be called from both React
// components and non-React contexts (e.g. keyboard shortcut handlers).
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  const {
    url: resolvedUrl,
    headers: effectiveHeaders,
    queryParams: resolvedQueryParams,
    body: resolvedBody,
    auth: resolvedAuth,
    collection,
    environmentName,
    requestPath,
  } = await resolveRequestFields(tabId, request);

  try {
    const result = await executeRequest({
      method: request.method,
      url: resolvedUrl,
      headers: effectiveHeaders,
      queryParams: resolvedQueryParams,
      body: resolvedBody,
      auth: resolvedAuth,
      options: {
        followRedirects: request.settings?.followRedirects ?? true,
        timeoutMs: request.settings?.timeoutMs ?? 30000,
        verifySsl: request.settings?.verifySsl ?? true,
      },
      collection: collection ?? undefined,
      environmentName,
      requestPath,
    });

    const responseState: ResponseState = {
      status: result.status,
      statusText: result.statusText,
      headers: result.headers.map((h) => ({
        id: crypto.randomUUID(),
        key: h.key,
        value: h.value,
        enabled: h.enabled,
      })),
      body: result.body,
      durationMs: result.durationMs,
      sizeBytes: result.sizeBytes,
      activeView: 'pretty',
    };
    usePaneStore.getState().setResponse(tabId, responseState);
    useConsoleStore.getState().addEntry({
      method: request.method,
      url: resolvedUrl,
      status: result.status,
      statusText: result.statusText,
      durationMs: result.durationMs,
      sizeBytes: result.sizeBytes,
      requestHeaders: effectiveHeaders.map((h) => ({ key: h.key, value: h.value })),
      requestBody: resolvedBody?.content ?? '',
      responseHeaders: result.headers.map((h) => ({ key: h.key, value: h.value })),
      responseBody: result.body,
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    usePaneStore.getState().setResponse(tabId, {
      status: 0,
      statusText: 'Error',
      headers: [],
      body: msg,
      durationMs: 0,
      sizeBytes: msg.length,
      activeView: 'raw',
    });
    useConsoleStore.getState().addEntry({
      method: request.method,
      url: resolvedUrl,
      status: 0,
      statusText: 'Error',
      durationMs: 0,
      sizeBytes: msg.length,
      requestHeaders: effectiveHeaders.map((h) => ({ key: h.key, value: h.value })),
      requestBody: resolvedBody?.content ?? '',
      responseHeaders: [],
      responseBody: msg,
    });
  }
}
