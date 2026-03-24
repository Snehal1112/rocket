import { usePaneStore } from '@/stores/pane-store';
import { useEnvStore } from '@/stores/env-store';
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

export function toApiAuth(auth: AuthState, resolve = (s: string) => s): Auth {
  switch (auth.authType) {
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
  const resolve = useEnvStore.getState().resolveVariables;

  // Resolve environment variables in all request fields.
  const resolvedUrl = resolve(request.url);
  const resolvedHeaders: Header[] = request.headers
    .filter((h) => h.enabled)
    .map((h) => ({ key: resolve(h.key), value: resolve(h.value), enabled: h.enabled }));

  const resolvedBody = toApiBody(request.body, resolve);
  const resolvedAuth = toApiAuth(request.auth, resolve);

  try {
    const result = await executeRequest({
      method: request.method,
      url: resolvedUrl,
      headers: resolvedHeaders,
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
