import { saveTabRequest } from '@/lib/save-tab-request';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState, RequestTab } from '@/types/pane-types';

const timers = new Map<string, ReturnType<typeof setTimeout>>();

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
    // The shared save path builds the payload, so autosave and Save write the same fields.
    const tab: RequestTab = {
      id: tabId,
      title,
      tabType: 'request',
      request,
      response: null,
      isDirty: true,
      source: { collection, path },
    };
    try {
      await saveTabRequest(collection, path, tab);
      // Clean only if no newer edit landed while the save was in flight.
      usePaneStore.getState().markRequestSaved(tabId, request);
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
