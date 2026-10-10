import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/tauri-api', () => ({
  saveRequest: vi.fn().mockResolvedValue({}),
  saveGraphQlRequest: vi.fn().mockResolvedValue({}),
  saveGrpcRequest: vi.fn().mockResolvedValue({}),
  saveWebSocketRequest: vi.fn().mockResolvedValue({}),
}));

import {
  saveGraphQlRequest,
  saveGrpcRequest,
  saveRequest,
  saveWebSocketRequest,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';
import { scheduleAutoSave } from '../auto-save';
import { createDefaultRequestFor } from '../pane-utils';
import { saveTabRequest } from '../save-tab-request';

const SAVERS = {
  http: saveRequest,
  graphql: saveGraphQlRequest,
  grpc: saveGrpcRequest,
  websocket: saveWebSocketRequest,
} as const;

function makeTab(type: keyof typeof SAVERS): RequestTab {
  const request = createDefaultRequestFor(type);
  request.url = 'http://localhost/x';
  request.docs = 'some docs';
  request.actions = [{ id: 'a1', type: 'set-variable' } as never];
  return {
    id: 'tab1',
    title: 'Req',
    tabType: 'request',
    request,
    response: null,
    isDirty: true,
    source: { collection: 'c', path: 'r.yml' },
  };
}

describe('autosave and explicit save payloads', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  for (const type of ['http', 'graphql', 'grpc', 'websocket'] as const) {
    it(`write identical payloads for ${type}`, async () => {
      const tab = makeTab(type);
      await saveTabRequest('c', 'r.yml', tab);
      scheduleAutoSave(tab.id, 'c', 'r.yml', tab.title, tab.request);
      await vi.advanceTimersByTimeAsync(500);
      const saver = vi.mocked(SAVERS[type]);
      expect(saver).toHaveBeenCalledTimes(2);
      expect(saver.mock.calls[1][2]).toEqual(saver.mock.calls[0][2]);
    });
  }

  it('keeps both docs and actions for http', async () => {
    const tab = makeTab('http');
    await saveTabRequest('c', 'r.yml', tab);
    const payload = vi.mocked(saveRequest).mock.calls[0][2];
    expect(payload.docs).toBe('some docs');
    expect(payload.actions).toHaveLength(1);
  });
});

describe('dirty flag after save', () => {
  function openDirtyTab(): RequestTab {
    usePaneStore.getState().reset();
    const tab = makeTab('http');
    usePaneStore.getState().openTab(tab);
    usePaneStore.getState().updateRequest(tab.id, { url: 'http://localhost/first' });
    return tab;
  }
  function current(): RequestTab {
    const root = usePaneStore.getState().root;
    const find = (n: typeof root): RequestTab | undefined => {
      if ('tabs' in n) return n.tabs.find((t) => t.id === 'tab1') as RequestTab | undefined;
      return n.children.map(find).find(Boolean);
    };
    const t = find(root);
    if (!t) throw new Error('tab missing');
    return t;
  }

  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('autosave clears the flag when nothing changed', async () => {
    openDirtyTab();
    const t = current();
    scheduleAutoSave(t.id, 'c', 'r.yml', t.title, t.request);
    await vi.advanceTimersByTimeAsync(500);
    expect(current().isDirty).toBe(false);
  });

  it('a newer edit during an in-flight autosave keeps the tab dirty', async () => {
    openDirtyTab();
    const t = current();
    let release: () => void = () => {};
    vi.mocked(saveRequest).mockImplementationOnce(
      () => new Promise((resolve) => (release = () => resolve({ uid: 'tab1' } as never))),
    );
    scheduleAutoSave(t.id, 'c', 'r.yml', t.title, t.request);
    await vi.advanceTimersByTimeAsync(500);
    usePaneStore.getState().updateRequest(t.id, { url: 'http://localhost/second' });
    release();
    await vi.advanceTimersByTimeAsync(0);
    expect(current().isDirty).toBe(true);
  });

  it('a failed autosave keeps the tab dirty', async () => {
    openDirtyTab();
    const t = current();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(saveRequest).mockRejectedValueOnce(new Error('boom'));
    scheduleAutoSave(t.id, 'c', 'r.yml', t.title, t.request);
    await vi.advanceTimersByTimeAsync(500);
    expect(current().isDirty).toBe(true);
  });

  it('markRequestSaved ignores a stale snapshot', () => {
    openDirtyTab();
    const stale = current().request;
    usePaneStore.getState().updateRequest('tab1', { url: 'http://localhost/newer' });
    usePaneStore.getState().markRequestSaved('tab1', stale);
    expect(current().isDirty).toBe(true);
    usePaneStore.getState().markRequestSaved('tab1', current().request);
    expect(current().isDirty).toBe(false);
  });
});
