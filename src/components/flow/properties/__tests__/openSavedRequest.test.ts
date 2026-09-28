import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getRequest, type Request } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
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

function leafOf() {
  const root = usePaneStore.getState().root;
  if (root.type !== 'leaf') throw new Error('Expected a leaf');
  return root;
}

describe('openSavedRequestTab', () => {
  beforeEach(() => {
    vi.mocked(getRequest).mockReset();
    usePaneStore.getState().reset();
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
