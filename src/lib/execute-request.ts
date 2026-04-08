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
        addTo: auth.apiKey?.addTo ?? 'header',
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

// Executes a request and writes the response into the pane store.
// This is a plain async function so it can be called from both React
// components and non-React contexts (e.g. keyboard shortcut handlers).
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  // Build the full 7-scope variable resolution context.
  const envStore = useEnvStore.getState();
  const envVars = envStore.getActiveVariables();
  const globalVars = envStore.getGlobalVariables();
  const processEnvVars = envStore.processEnvVars;

  const { root } = usePaneStore.getState();
  const found = findTabInTree(root, tabId);
  const collection = found?.tab.source?.collection;
  const requestPath = found?.tab.source?.path;

  // Fetch collection-level variables and default headers from settings.
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

  // Fetch folder-chain variables (server walks full parent chain).
  let folderVars: CollectionVariable[] = [];
  if (collection && requestPath) {
    try {
      folderVars = await getFolderChainVariables(collection, requestPath);
    } catch {
      // Non-critical: fall back to empty vars if chain lookup fails.
    }
  }

  // Fetch request-level variables.
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

  // Resolve environment variables in all request fields.
  let resolvedUrl = resolve(request.url);

  // Substitute :pathParam placeholders with values from pathParams.
  for (const p of request.pathParams) {
    if (p.enabled && p.key && p.value) {
      resolvedUrl = resolvedUrl.replace(`:${p.key}`, encodeURIComponent(p.value));
    }
  }
  const resolvedHeaders: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: h.enabled }));

  const resolvedBody = toApiBody(request.body, resolve);
  const resolvedAuth = toApiAuth(request.auth, resolve);

  // Merge collection default headers with request headers. Request headers
  // with the same key override collection defaults; collection headers fill in
  // any keys not present in the request.
  const requestHeaderKeys = new Set(resolvedHeaders.map((h) => h.key.toLowerCase()));
  const effectiveHeaders: Header[] = [
    ...collectionHeaders
      .filter((h) => !requestHeaderKeys.has(h.key.toLowerCase()))
      .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: true })),
    ...resolvedHeaders,
  ];

  try {
    const resolvedQueryParams = request.queryParams
      .filter((p) => p.enabled)
      .map((p) => ({ key: resolve(p.key), value: resolve(p.value), enabled: p.enabled }));

    const result = await executeRequest({
      method: request.method,
      url: resolvedUrl,
      headers: effectiveHeaders,
      queryParams: resolvedQueryParams,
      body: resolvedBody,
      auth: resolvedAuth,
      options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      collection: collection ?? undefined,
      environmentName: envStore.activeEnvId ?? undefined,
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
