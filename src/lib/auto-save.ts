import { saveTabRequest } from '@/lib/save-tab-request';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { RequestState, RequestTab } from '@/types/pane-types';

interface PendingSave {
  timer: ReturnType<typeof setTimeout>;
  run: () => Promise<boolean>;
}

const pending = new Map<string, PendingSave>();
const inFlight = new Set<Promise<boolean>>();
// Above zero while a workspace change is in progress. A save then could land in either
// workspace. A counter, so one change ending never re-enables saves for another.
let suspended = 0;

const activeWorkspaceId = () => useWorkspaceStore.getState().activeWorkspaceId;

function track(save: Promise<boolean>): Promise<boolean> {
  inFlight.add(save);
  void save.finally(() => inFlight.delete(save));
  return save;
}

/**
 * Saves one request tab. The save is dropped when the active workspace is no longer the one
 * the edit was made in, because the backend writes to whatever workspace is active.
 * Resolves to false when the save failed or was dropped.
 */
async function runSave(
  tabId: string,
  collection: string,
  path: string,
  title: string,
  request: RequestState,
  workspaceId: string,
): Promise<boolean> {
  if (suspended > 0 || activeWorkspaceId() !== workspaceId) {
    console.warn('[AutoSave] Dropped: the workspace changed since the edit.');
    return false;
  }
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
    return true;
  } catch (err) {
    console.error('[AutoSave] Failed:', err);
    return false;
  }
}

export function scheduleAutoSave(
  tabId: string,
  collection: string,
  path: string,
  title: string,
  request: RequestState,
) {
  cancelAutoSave(tabId);
  // Stamped now, so a save that fires after a workspace switch is dropped.
  const workspaceId = activeWorkspaceId();
  const run = () => runSave(tabId, collection, path, title, request, workspaceId);
  const timer = setTimeout(() => {
    pending.delete(tabId);
    void track(run());
  }, 500);
  pending.set(tabId, { timer, run });
}

export function cancelAutoSave(tabId: string) {
  const existing = pending.get(tabId);
  if (existing) {
    clearTimeout(existing.timer);
    pending.delete(tabId);
  }
}

/**
 * Saves the given dirty tabs and every pending autosave now, and waits for all saves,
 * including ones already in flight. Resolves to the number of saves that did not succeed.
 */
export async function flushAutoSaves(dirtyTabs: RequestTab[] = []): Promise<number> {
  const workspaceId = activeWorkspaceId();
  for (const tab of dirtyTabs) {
    if (!tab.source) continue;
    cancelAutoSave(tab.id);
    const { collection, path } = tab.source;
    void track(runSave(tab.id, collection, path, tab.title, tab.request, workspaceId));
  }
  for (const [tabId, save] of pending) {
    clearTimeout(save.timer);
    pending.delete(tabId);
    void track(save.run());
  }
  // Every save above is tracked, so this also waits for saves that were already running.
  const results = await Promise.all([...inFlight]);
  return results.filter((ok) => !ok).length;
}

/** Waits for the saves that are already running, without starting pending ones. */
export async function waitForAutoSaves(): Promise<void> {
  await Promise.all([...inFlight]);
}

/** Drops every autosave that fires until each call has its `resumeAutoSaves`. */
export function suspendAutoSaves() {
  suspended += 1;
}

export function resumeAutoSaves() {
  suspended = Math.max(0, suspended - 1);
}
