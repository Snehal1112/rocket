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
  oauth2GetToken,
  oauth2RefreshToken,
} from '@/lib/tauri-api';
import { buildVariableContext, resolveWithContext } from '@/lib/variable-context';
import { useCollectionAuthStore } from '@/stores/collection-auth-store';
import { useConsoleStore } from '@/stores/console-store';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { AuthState, BodyState, RequestState, ResponseState } from '@/types/pane-types';

// Returns the synthetic request header(s) that the Rust executor adds for a
// given Auth value, so the console can show the full set of sent headers.
// Only covers auth schemes that produce a deterministic header on the frontend;
// AWS SigV4 signing happens in Rust so its headers are not included here.
export function authToConsoleHeaders(auth: Auth): { key: string; value: string }[] {
  switch (auth.authType) {
    case 'bearer':
      return [{ key: 'Authorization', value: `Bearer ${auth.token}` }];
    case 'basic': {
      const encoded = btoa(`${auth.username}:${auth.password}`);
      return [{ key: 'Authorization', value: `Basic ${encoded}` }];
    }
    case 'api-key':
      if (auth.placement === 'header') {
        return [{ key: auth.key, value: auth.value }];
      }
      return [];
    default:
      return [];
  }
}

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
      // For execution, send the stored access token as a bearer token.
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

// Non-interactive grants — safe to silently fetch on send. Authorization Code
// and Implicit pop a browser window, which would be surprising as a side effect
// of hitting Send, so they're excluded from auto-fetch.
const NON_INTERACTIVE_GRANTS = new Set(['client_credentials', 'password']);

// Runs auto-refresh and auto-fetch for an OAuth2 auth state, persisting any new
// token into the pane store before the request is built. Returns the (possibly
// updated) request. Failures are logged but non-fatal — the original auth flows
// through and the user sees the auth error in the response.
async function maybeAutoRefreshOrFetchToken(
  tabId: string,
  request: RequestState,
  collection: string | undefined,
  environmentName: string | undefined,
  requestPath: string | undefined,
  varCtx: Record<string, string>,
): Promise<RequestState> {
  if (request.auth.authType !== 'oauth2' || !request.auth.oauth2) return request;
  const oauth = request.auth.oauth2;
  const now = Math.floor(Date.now() / 1000);
  const tokenExpired =
    oauth.expiresIn && oauth.tokenAcquiredAt
      ? now >= oauth.tokenAcquiredAt + oauth.expiresIn
      : false;

  const applyToken = (patch: Partial<NonNullable<AuthState['oauth2']>>) => {
    const { updateRequest } = usePaneStore.getState();
    updateRequest(tabId, {
      auth: {
        ...request.auth,
        oauth2: { ...oauth, ...patch } as NonNullable<AuthState['oauth2']>,
      },
    });
  };

  const rv = (s: string) => resolveWithContext(s, varCtx);

  // Auto-refresh takes precedence when a usable refresh token exists.
  if (
    oauth.autoRefreshToken &&
    tokenExpired &&
    oauth.refreshToken &&
    (oauth.refreshTokenUrl || oauth.tokenUrl)
  ) {
    try {
      const resolvedRefreshParams = oauth.refreshParams.length
        ? oauth.refreshParams.map((p) => ({ ...p, key: rv(p.key), value: rv(p.value) }))
        : undefined;
      const result = await oauth2RefreshToken({
        refreshToken: rv(oauth.refreshToken),
        tokenUrl: rv(oauth.tokenUrl),
        refreshTokenUrl: oauth.refreshTokenUrl ? rv(oauth.refreshTokenUrl) : undefined,
        clientId: rv(oauth.clientId),
        clientSecret: oauth.clientSecret ? rv(oauth.clientSecret) : undefined,
        scope: oauth.scope ? rv(oauth.scope) : undefined,
        clientAuthentication: oauth.clientAuthentication,
        verifySsl: oauth.verifySsl,
        refreshParams: resolvedRefreshParams,
        collection,
        environmentName,
        requestPath,
      });
      applyToken({
        accessToken: result.access_token,
        refreshToken: result.refresh_token || oauth.refreshToken,
        expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.id_token || oauth.idToken,
        tokenType: result.token_type || oauth.tokenType,
        responseScope: result.scope || oauth.responseScope,
      });
    } catch (err) {
      console.warn('[OAuth2] Auto-refresh failed:', err);
    }
  } else if (
    oauth.autoFetchToken &&
    !oauth.accessToken &&
    NON_INTERACTIVE_GRANTS.has(oauth.grantType)
  ) {
    // Auto-fetch is restricted to non-interactive grants. Opening the system
    // browser or a webview as a side effect of Send would surprise the user.
    try {
      const resolvedAuthParams = oauth.authParams.length
        ? oauth.authParams.map((p) => ({ ...p, key: rv(p.key), value: rv(p.value) }))
        : undefined;
      const resolvedTokenParams = oauth.tokenParams.length
        ? oauth.tokenParams.map((p) => ({ ...p, key: rv(p.key), value: rv(p.value) }))
        : undefined;
      const resolvedRefreshParams = oauth.refreshParams.length
        ? oauth.refreshParams.map((p) => ({ ...p, key: rv(p.key), value: rv(p.value) }))
        : undefined;
      const result = await oauth2GetToken({
        grantType: oauth.grantType,
        authorizationUrl: rv(oauth.authorizationUrl) || undefined,
        tokenUrl: rv(oauth.tokenUrl) || undefined,
        callbackUrl: rv(oauth.callbackUrl) || undefined,
        clientId: rv(oauth.clientId),
        clientSecret: oauth.clientSecret ? rv(oauth.clientSecret) : undefined,
        scope: oauth.scope ? rv(oauth.scope) : undefined,
        state: oauth.state ? rv(oauth.state) : undefined,
        username: oauth.username ? rv(oauth.username) : undefined,
        password: oauth.password ? rv(oauth.password) : undefined,
        clientAuthentication: oauth.clientAuthentication,
        usePkce: oauth.usePkce,
        useSystemBrowser: oauth.useSystemBrowser,
        verifySsl: oauth.verifySsl,
        authParams: resolvedAuthParams,
        tokenParams: resolvedTokenParams,
        refreshParams: resolvedRefreshParams,
        collection,
        environmentName,
        requestPath,
      });
      applyToken({
        accessToken: result.access_token,
        refreshToken: result.refresh_token || '',
        expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.id_token || '',
        tokenType: result.token_type || '',
        responseScope: result.scope || '',
      });
    } catch (err) {
      console.warn('[OAuth2] Auto-fetch failed:', err);
    }
  }

  // Re-read the tab's request in case we just patched it; otherwise return as-is.
  const { root } = usePaneStore.getState();
  const refreshed = findTabInTree(root, tabId);
  if (refreshed && (refreshed.tab.tabType === 'request' || refreshed.tab.tabType === 'history')) {
    return refreshed.tab.request;
  }
  return request;
}

// Executes a request and writes the response into the pane store.
// This is a plain async function so it can be called from both React
// components and non-React contexts (e.g. keyboard shortcut handlers).
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  // Resolve context once up-front so auto-fetch can pass it to the Tauri command
  // without repeating the tab/env lookup.
  const { root } = usePaneStore.getState();
  const found = findTabInTree(root, tabId);
  const preCollection = found?.tab.source?.collection;
  const preRequestPath = found?.tab.source?.path;
  const preEnvName = useEnvStore.getState().activeEnvId ?? undefined;

  // Build a variable context for pre-resolving OAuth2 fields before they reach
  // the Tauri command. The backend env_repo only covers the workspace-level
  // (global) environment directory, not collection-scoped environments, so
  // variable resolution for OAuth2 must happen here on the frontend.
  const preEnvStore = useEnvStore.getState();
  let preCollectionVars: CollectionVariable[] = [];
  if (preCollection) {
    try {
      const settings = await getCollectionSettings(preCollection);
      preCollectionVars = settings.variables;
    } catch {
      // Non-critical.
    }
  }
  let preFolderVars: CollectionVariable[] = [];
  let preRequestVars: CollectionVariable[] = [];
  if (preCollection && preRequestPath) {
    try {
      preFolderVars = await getFolderChainVariables(preCollection, preRequestPath);
    } catch {
      // Non-critical.
    }
    try {
      preRequestVars = await getRequestVariables(preCollection, preRequestPath);
    } catch {
      // Non-critical.
    }
  }
  const preVarCtx = buildVariableContext({
    processEnvVars: preEnvStore.processEnvVars,
    globalVars: preEnvStore.getGlobalVariables(),
    envVars: preEnvStore.getActiveVariables(),
    collectionVars: preCollectionVars,
    folderVars: preFolderVars,
    requestVars: preRequestVars,
  });

  const effectiveRequest = await maybeAutoRefreshOrFetchToken(
    tabId,
    request,
    preCollection,
    preEnvName,
    preRequestPath,
    preVarCtx,
  );

  const {
    url: resolvedUrl,
    headers: effectiveHeaders,
    queryParams: resolvedQueryParams,
    body: resolvedBody,
    auth: resolvedAuth,
    collection,
    environmentName,
    requestPath,
  } = await resolveRequestFields(tabId, effectiveRequest);

  try {
    const result = await executeRequest({
      method: effectiveRequest.method,
      url: resolvedUrl,
      headers: effectiveHeaders,
      queryParams: resolvedQueryParams,
      body: resolvedBody,
      auth: resolvedAuth,
      options: {
        followRedirects: effectiveRequest.settings?.followRedirects ?? true,
        timeoutMs: effectiveRequest.settings?.timeoutMs ?? 30000,
        verifySsl: effectiveRequest.settings?.verifySsl ?? true,
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
      ttfbMs: result.ttfbMs,
      sizeBytes: result.sizeBytes,
      activeView: 'pretty',
    };
    usePaneStore.getState().setResponse(tabId, responseState);
    // Merge auth-synthesized headers with explicit headers for the console.
    // Auth headers (Bearer, Basic, API Key) are injected by reqwest at the Rust
    // level and never appear in effectiveHeaders — add them here so the console
    // shows the full set of headers actually sent on the wire.
    const authHeaders = authToConsoleHeaders(resolvedAuth);
    const explicitKeys = new Set(effectiveHeaders.map((h) => h.key.toLowerCase()));
    const consoleRequestHeaders = [
      ...authHeaders.filter((h) => !explicitKeys.has(h.key.toLowerCase())),
      ...effectiveHeaders.map((h) => ({ key: h.key, value: h.value })),
    ];
    useConsoleStore.getState().addEntry({
      method: effectiveRequest.method,
      url: resolvedUrl,
      status: result.status,
      statusText: result.statusText,
      durationMs: result.durationMs,
      sizeBytes: result.sizeBytes,
      requestHeaders: consoleRequestHeaders,
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
      ttfbMs: 0,
      sizeBytes: msg.length,
      activeView: 'raw',
    });
    useConsoleStore.getState().addEntry({
      method: effectiveRequest.method,
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
