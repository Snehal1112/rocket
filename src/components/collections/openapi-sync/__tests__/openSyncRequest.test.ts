import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree, mapApiRequestToState } from '@/lib/pane-utils';
import { getRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { openSyncRequest } from '../openSyncRequest';

vi.mock('@/lib/tauri-api', () => ({ getRequest: vi.fn() }));
vi.mock('@/lib/pane-utils', () => ({
  findTabInTree: vi.fn(),
  mapApiRequestToState: vi.fn(),
}));

const openTab = vi.fn();
vi.mock('@/stores/pane-store', () => ({
  usePaneStore: { getState: vi.fn() },
}));

const ARGS = { collection: 'Orders', uid: 'u1', requestPath: 'pets/list-pets' };
const FULL = { uid: 'u1', name: 'List pets' };

describe('openSyncRequest', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(usePaneStore.getState).mockReturnValue({ root: 'tree', openTab } as never);
    vi.mocked(findTabInTree).mockReturnValue(null as never);
    vi.mocked(mapApiRequestToState).mockReturnValue({ mapped: true } as never);
    vi.mocked(getRequest).mockResolvedValue(FULL as never);
  });

  it('opens_a_new_tab_for_the_request', async () => {
    await expect(openSyncRequest(ARGS)).resolves.toBe('opened');
    expect(getRequest).toHaveBeenCalledWith('Orders', 'pets/list-pets');
    expect(openTab).toHaveBeenCalledWith({
      id: 'u1',
      title: 'List pets',
      tabType: 'request',
      request: { mapped: true },
      response: null,
      isDirty: false,
      source: { collection: 'Orders', path: 'pets/list-pets' },
    });
  });

  it('focuses_an_open_tab_instead_of_duplicating', async () => {
    const tab = { id: 'u1' };
    vi.mocked(findTabInTree).mockReturnValue({ tab } as never);
    await expect(openSyncRequest(ARGS)).resolves.toBe('opened');
    expect(findTabInTree).toHaveBeenCalledWith('tree', 'u1');
    expect(getRequest).not.toHaveBeenCalled();
    expect(openTab).toHaveBeenCalledWith(tab);
  });

  it('open_request_reports_a_missing_file', async () => {
    vi.mocked(getRequest).mockRejectedValue('no such file /home/me/x');
    await expect(openSyncRequest(ARGS)).resolves.toBe('missing');
    expect(openTab).not.toHaveBeenCalled();
  });

  it('a_yaml_file_is_found_with_its_extension', async () => {
    vi.mocked(getRequest).mockRejectedValueOnce('missing').mockResolvedValueOnce(FULL as never);
    await expect(openSyncRequest(ARGS)).resolves.toBe('opened');
    expect(getRequest).toHaveBeenLastCalledWith('Orders', 'pets/list-pets.yaml');
    expect(openTab).toHaveBeenCalledWith(
      expect.objectContaining({ source: { collection: 'Orders', path: 'pets/list-pets.yaml' } }),
    );
  });

  it('a_request_without_a_uid_uses_the_loaded_one_as_the_tab_id', async () => {
    vi.mocked(getRequest).mockResolvedValue({ uid: 'loaded', name: 'X' } as never);
    await openSyncRequest({ ...ARGS, uid: '' });
    expect(openTab).toHaveBeenCalledWith(expect.objectContaining({ id: 'loaded' }));
  });
});
