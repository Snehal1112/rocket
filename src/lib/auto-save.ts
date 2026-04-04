import { toApiAuth } from '@/lib/execute-request';
import { type Request, saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState } from '@/types/pane-types';

const timers = new Map<string, ReturnType<typeof setTimeout>>();

function toApiRequest(uid: string, name: string, request: RequestState): Request {
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
    auth: toApiAuth(request.auth),
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
      await saveRequest(collection, path, toApiRequest(tabId, title, request));
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
