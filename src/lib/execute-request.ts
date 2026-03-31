import { usePaneStore } from '@/stores/pane-store';
import { useEnvStore } from '@/stores/env-store';
import { useConsoleStore } from '@/stores/console-store';
import {
  executeRequest,
  getCollectionSettings,
  type Auth,
  type Body,
  type Header,
} from '@/lib/tauri-api';
import { findTabInTree } from '@/lib/pane-utils';
import { buildResolver } from '@/lib/url-variables';
import type {
  AuthState,
  BodyState,
  RequestState,
  ResponseState,
} from '@/types/pane-types';

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

function toApiBody(body: BodyState, resolve = (s: string) => s): Body | undefined {
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
  // Build merged variable resolution: env vars (high priority) + collection vars (fallback).
  const envVars = useEnvStore.getState().getActiveVariables();

  const collectionVars: Record<string, string> = {};
  const { root } = usePaneStore.getState();
  const found = findTabInTree(root, tabId);
  if (found?.tab.source?.collection) {
    try {
      const settings = await getCollectionSettings(found.tab.source.collection);
      for (const v of settings.variables) {
        if (v.enabled) collectionVars[v.key] = v.value || v.initialValue;
      }
    } catch {
      // Collection settings unavailable — proceed with env vars only.
    }
  }

  const resolve = buildResolver(envVars, collectionVars);

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

  try {
    const resolvedQueryParams = request.queryParams
      .filter((p) => p.enabled)
      .map((p) => ({ key: resolve(p.key), value: resolve(p.value), enabled: p.enabled }));

    const result = await executeRequest({
      method: request.method,
      url: resolvedUrl,
      headers: resolvedHeaders,
      queryParams: resolvedQueryParams,
      body: resolvedBody,
      auth: resolvedAuth,
      options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
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
      requestHeaders: resolvedHeaders.map((h) => ({ key: h.key, value: h.value })),
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
      requestHeaders: resolvedHeaders.map((h) => ({ key: h.key, value: h.value })),
      requestBody: resolvedBody?.content ?? '',
      responseHeaders: [],
      responseBody: msg,
    });
  }
}
