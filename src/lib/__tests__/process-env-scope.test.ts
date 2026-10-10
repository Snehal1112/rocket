import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { buildOAuth2VarContext } from '@/lib/execute-request';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getProcessEnvVars: vi.fn(),
    getCollectionSettings: vi.fn().mockResolvedValue({ headers: [], variables: [] }),
  };
});

describe('process env scope', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setQueryClient(new QueryClient({ defaultOptions: { queries: { retry: false } } }));
  });

  it('asks for the host environment of the collection and gets none when it is withheld', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue({});
    const ctx = await buildOAuth2VarContext('clone');
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith('clone');
    expect(Object.keys(ctx).some((k) => k.startsWith('process.env.'))).toBe(false);
  });

  it('adds the host environment when the backend returns it', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue({ HOME: '/home/u' });
    const ctx = await buildOAuth2VarContext('mine');
    expect(ctx['process.env.HOME']).toBe('/home/u');
  });

  it('asks the backend on every send, so a withdrawn permission applies at once', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValueOnce({ HOME: '/home/u' });
    expect((await buildOAuth2VarContext('mine'))['process.env.HOME']).toBe('/home/u');

    // The backend now returns nothing for the collection.
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValue({});
    const ctx = await buildOAuth2VarContext('mine');
    expect(ctx['process.env.HOME']).toBeUndefined();
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledTimes(2);
  });

  it('keeps full access for a scratch request and never throws on a failed read', async () => {
    vi.mocked(tauriApi.getProcessEnvVars).mockResolvedValueOnce({ HOME: '/h' });
    expect((await buildOAuth2VarContext(undefined))['process.env.HOME']).toBe('/h');
    expect(tauriApi.getProcessEnvVars).toHaveBeenCalledWith(null);

    vi.mocked(tauriApi.getProcessEnvVars).mockRejectedValue(new Error('boom'));
    const ctx = await buildOAuth2VarContext('other');
    expect(Object.keys(ctx).some((k) => k.startsWith('process.env.'))).toBe(false);
  });
});
