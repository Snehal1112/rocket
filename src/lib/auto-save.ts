import { toApiBody } from '@/lib/execute-request';
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import { type Request, saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState } from '@/types/pane-types';

const timers = new Map<string, ReturnType<typeof setTimeout>>();

function toApiRequest(uid: string, name: string, request: RequestState): Request {
  const s = request.settings;

  return {
    uid,
    name,
    method: request.method,
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    body: toApiBody(request.body),
    auth: toPersistedAuth(request.auth),
    tags: request.tags && request.tags.length > 0 ? request.tags : undefined,
    preRequestScript: request.preRequestScript ?? null,
    postResponseScript: request.postResponseScript ?? null,
    tests: request.testsScript ?? null,
    assertions: request.assertions ?? [],
    actions: request.actions ?? [],
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
