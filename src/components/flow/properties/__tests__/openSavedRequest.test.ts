import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getRequest, type Request } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { openSavedRequestTab } from '../openSavedRequest';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getRequest: vi.fn() };
});

const fullRequest: Request = {
  uid: 'req-login',
  name: 'Login',
  method: 'POST',
  url: '{{baseUrl}}/login',
  headers: [],
  auth: { authType: 'inherit' },
};

const flowTab: FlowTab = {
  id: 'flow-1',
  tabType: 'flow',
  title: 'Flow: f',
  isDirty: true,
  collectionName: 'demo',
  flowName: 'f',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function leafOf() {
  const root = usePaneStore.getState().root;
  if (root.type !== 'leaf') throw new Error('Expected a leaf');
  return root;
}

describe('openSavedRequestTab', () => {
  beforeEach(() => {
    vi.mocked(getRequest).mockReset();
    usePaneStore.getState().reset();
    useEnvStore.getState().setActiveCollection(null);
  });

  it('adopts the collection without dropping tabs when none is active', async () => {
    vi.mocked(getRequest).mockResolvedValue(fullRequest);
    usePaneStore.getState().openTab(flowTab);
    expect(usePaneStore.getState().activeCollection).toBeNull();

    await expect(openSavedRequestTab('demo', 'login.yml')).resolves.toBe('opened');

    expect(leafOf().tabs.map((t) => t.id)).toEqual(['flow-1', 'req-login']);
    expect(leafOf().tabs[0]).toBe(flowTab);
    expect(leafOf().activeTabId).toBe('req-login');
    expect(usePaneStore.getState().activeCollection).toBe('demo');
    expect(useEnvStore.getState().activeCollection).toBe('demo');
  });

  it('does not switch away from another active collection', async () => {
    vi.mocked(getRequest).mockResolvedValue(fullRequest);
    usePaneStore.getState().switchCollection('other');
    usePaneStore.getState().openTab(flowTab);

    await expect(openSavedRequestTab('demo', 'login.yml')).resolves.toBe('other-collection');

    expect(usePaneStore.getState().activeCollection).toBe('other');
    expect(leafOf().tabs.map((t) => t.id)).toEqual(['flow-1']);
    expect(getRequest).not.toHaveBeenCalled();
  });

  it('focuses an already-open tab instead of opening a duplicate', async () => {
    vi.mocked(getRequest).mockResolvedValue(fullRequest);
    await openSavedRequestTab('demo', 'login.yml');
    const first = leafOf().tabs[0];
    usePaneStore.getState().openTab({ ...first, id: 'other', title: 'Other' });
    expect(leafOf().activeTabId).toBe('other');

    await openSavedRequestTab('demo', 'login.yml');

    expect(getRequest).toHaveBeenCalledTimes(2);
    expect(leafOf().tabs.filter((t) => t.id === 'req-login')).toHaveLength(1);
    expect(leafOf().activeTabId).toBe('req-login');
  });
});
