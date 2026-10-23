import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushAutoSaves, scheduleAutoSave } from '@/lib/auto-save';
import { collectAllTabs } from '@/lib/pane-utils';
import { saveTabRequest } from '@/lib/save-tab-request';
import type { Request } from '@/lib/tauri-api';
import { getRequest, saveRequest } from '@/lib/tauri-api';
import type { RequestTab } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    endAgentSession: vi.fn(),
    getRequest: vi.fn(),
    saveRequest: vi.fn().mockResolvedValue({}),
  };
});

const parent: Request = {
  uid: 'u1',
  name: 'Users',
  method: 'GET',
  url: 'https://api.test/users',
  headers: [],
  auth: { authType: 'none' } as Request['auth'],
  preRequestScript: 'rok.log("pre");',
};
const example = { name: '200 OK', request: { url: 'https://api.test/x', method: 'POST' } };

function tryTabs(): RequestTab[] {
  return collectAllTabs(usePaneStore.getState().root).filter(
    (t): t is RequestTab => isRequestTab(t) && t.transient === true,
  );
}

async function openTry(): Promise<RequestTab> {
  await usePaneStore.getState().openTryTab('col', 'users.yml', example);
  const tabs = tryTabs();
  return tabs[tabs.length - 1];
}

describe('pane-store try tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(saveRequest).mockClear();
    vi.mocked(getRequest).mockResolvedValue(parent);
  });

  it('try_opens_a_transient_tab_with_the_parent_source', async () => {
    const tab = await openTry();
    expect(tab).toMatchObject({
      tabType: 'request',
      title: 'Try: 200 OK',
      source: { collection: 'col', path: 'users.yml' },
      transient: true,
      isDirty: false,
      response: null,
    });
    // The example's request replaces the parent's, whose scripts stay.
    expect(tab.request).toMatchObject({
      method: 'POST',
      url: 'https://api.test/x',
      preRequestScript: 'rok.log("pre");',
    });
  });

  it('try_twice_opens_two_tabs', async () => {
    await openTry();
    await openTry();
    const tabs = tryTabs();
    expect(tabs).toHaveLength(2);
    expect(tabs[0].id).not.toBe(tabs[1].id);
  });

  it('a_transient_tab_never_saves_to_the_parent', async () => {
    const tab = await openTry();
    const store = usePaneStore.getState();
    store.updateRequest(tab.id, { url: 'https://edited.test' });
    store.markDirty(tab.id);
    const edited = tryTabs()[0];
    // Edits stay in memory and never turn the tab dirty.
    expect(edited.request.url).toBe('https://edited.test');
    expect(edited.isDirty).toBe(false);
    expect(store.dirtyRequestTabs()).toHaveLength(0);

    // Even a forced flush of the tab writes nothing.
    await flushAutoSaves([{ ...edited, isDirty: true }]);
    store.setActiveTab(tab.id, usePaneStore.getState().activeGroupId);
    store.closeAll();
    await flushAutoSaves();
    expect(saveRequest).not.toHaveBeenCalled();
  });

  it('closing_a_transient_tab_does_not_prompt', async () => {
    const tab = await openTry();
    usePaneStore.getState().updateRequest(tab.id, { url: 'https://edited.test' });
    // The close prompt shows only for a dirty tab.
    expect(tryTabs()[0].isDirty).toBe(false);
    usePaneStore.getState().closeTab(tab.id, usePaneStore.getState().activeGroupId);
    await flushAutoSaves();
    expect(tryTabs()).toHaveLength(0);
    expect(saveRequest).not.toHaveBeenCalled();
  });

  it('the_save_helpers_refuse_a_transient_tab', async () => {
    const tab = await openTry();
    await expect(saveTabRequest('col', 'users.yml', tab)).rejects.toThrow();
    vi.useFakeTimers();
    try {
      scheduleAutoSave(tab.id, 'col', 'users.yml', tab.title, tab.request);
      await vi.advanceTimersByTimeAsync(1000);
    } finally {
      vi.useRealTimers();
    }
    expect(saveRequest).not.toHaveBeenCalled();
  });
});
