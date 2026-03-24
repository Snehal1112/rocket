import { usePaneStore } from '@/stores/pane-store';
import {
  executeRequest,
  type Auth,
  type Body,
  type Header,
} from '@/lib/tauri-api';
import type {
  AuthState,
  BodyState,
  RequestState,
  ResponseState,
} from '@/types/pane-types';

function toApiAuth(auth: AuthState): Auth {
  switch (auth.authType) {
    case 'basic':
      return {
        authType: 'basic',
        username: auth.basic?.username ?? '',
        password: auth.basic?.password ?? '',
      };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return {
        authType: 'api-key',
        key: auth.apiKey?.key ?? '',
        value: auth.apiKey?.value ?? '',
        addTo: auth.apiKey?.addTo ?? 'header',
      };
    case 'oauth2':
      // Send the stored access token as a bearer token.
      return {
        authType: 'bearer',
        token: auth.oauth2?.accessToken ?? '',
      };
    default:
      return { authType: 'none' };
  }
}

function toApiBody(body: BodyState): Body | undefined {
  if (body.mode === 'none') return undefined;
  if (body.mode === 'formdata') {
    return {
      mode: 'formdata',
      formData: body.formData
        .filter((e) => e.enabled)
        .map((e) => ({
          key: e.key,
          value: e.value,
          entryType: 'text' as const,
          enabled: e.enabled,
        })),
    };
  }
  return { mode: body.mode as Body['mode'], content: body.content };
}

// Executes a request and writes the response into the pane store.
// This is a plain async function so it can be called from both React
// components and non-React contexts (e.g. keyboard shortcut handlers).
export async function sendRequest(tabId: string, request: RequestState): Promise<void> {
  const headers: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled }));

  try {
    const result = await executeRequest({
      method: request.method,
      url: request.url,
      headers,
      body: toApiBody(request.body),
      auth: toApiAuth(request.auth),
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
  }
}
