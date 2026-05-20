import { toApiAuth } from '@/lib/execute-request';
import { oauth2StateToApiAuth } from '@/lib/oauth2-mapping';
import { type Request, saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState } from '@/types/pane-types';

const timers = new Map<string, ReturnType<typeof setTimeout>>();

function toApiRequest(uid: string, name: string, request: RequestState): Request {
  const auth =
    request.auth.authType === 'oauth2' && request.auth.oauth2
      ? oauth2StateToApiAuth(request.auth.oauth2)
      : toApiAuth(request.auth);

  const s = request.settings;

  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: request.headers
      .filter((h) => h.enabled)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body:
      request.body.mode !== 'none'
        ? { mode: request.body.mode, content: request.body.content }
        : undefined,
    auth,
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
    preRequestScript: request.preRequestScript ?? null,
    postResponseScript: request.postResponseScript ?? null,
    tests: request.testsScript ?? null,
    settings: {
      timeout: s.timeoutMs,
      followRedirects: s.followRedirects,
      verifySsl: s.verifySsl,
      maxRedirects: s.maxRedirects,
      encodeUrl: s.encodeUrl,
    },
  };
}

export function scheduleAutoSave(
  tabId: string,
  collection: string,
  path: string,
  title: string,
  request: RequestState,
) {
  cancelAutoSave(tabId);
  const timer = setTimeout(async () => {
    timers.delete(tabId);
    try {
      await saveRequest(
        collection,
        path,
        toApiRequest(tabId || crypto.randomUUID(), title, request),
      );
      // Mark tab clean after successful save.
      usePaneStore.getState().markClean(tabId);
    } catch (err) {
      console.error('[AutoSave] Failed:', err);
    }
  }, 500);
  timers.set(tabId, timer);
}

export function cancelAutoSave(tabId: string) {
  const existing = timers.get(tabId);
  if (existing) {
    clearTimeout(existing);
    timers.delete(tabId);
  }
}
