import { beforeEach, describe, expect, it, vi } from 'vitest';

const listEnvironments = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: (collection: string) => listEnvironments(collection),
}));

import { restoreActiveEnv, useEnvStore } from '@/stores/env-store';

const key = 'rocket-api:active-env:api';

describe('restoreActiveEnv', () => {
  beforeEach(() => {
    localStorage.clear();
    listEnvironments.mockReset();
    useEnvStore.setState({ activeCollection: 'api', activeEnvId: null });
  });

  it('keeps a stored environment that exists in this workspace', async () => {
    localStorage.setItem(key, 'dev');
    listEnvironments.mockResolvedValue([{ name: 'dev' }]);
    restoreActiveEnv('api');
    await vi.waitFor(() => expect(listEnvironments).toHaveBeenCalled());
    await Promise.resolve();
    expect(useEnvStore.getState().activeEnvId).toBe('dev');
  });

  it('drops a stored name that only another workspace has, and keeps it stored', async () => {
    localStorage.setItem(key, 'dev');
    listEnvironments.mockResolvedValue([{ name: 'prod' }]);
    restoreActiveEnv('api');
    await vi.waitFor(() => expect(useEnvStore.getState().activeEnvId).toBeNull());
    expect(localStorage.getItem(key)).toBe('dev');
  });
});
