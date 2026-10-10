import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Environment } from '@/lib/tauri-api';

const store: { envs: Record<string, Environment>; active: string | null } = {
  envs: {},
  active: null,
};

vi.mock('@/lib/tauri-api', () => ({
  getGlobalEnvironment: vi.fn(async (name: string) => store.envs[name] ?? null),
  getGlobalEnvironmentName: vi.fn(async () => store.active),
  listGlobalEnvironments: vi.fn(async () => Object.values(store.envs)),
  saveGlobalEnvironment: vi.fn(async (env: Environment) => {
    store.envs[env.name] = env;
  }),
  setGlobalEnvironment: vi.fn(async (name: string | null) => {
    store.active = name;
  }),
  deleteGlobalEnvironment: vi.fn(async (name: string) => {
    delete store.envs[name];
    if (store.active === name) store.active = null;
  }),
}));

let client: QueryClient;
vi.mock('@/lib/query-client', () => ({ getQueryClient: () => client }));
vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null, activeCollection: null }) },
}));
vi.mock('@/stores/collection-auth-store', () => ({
  useCollectionAuthStore: { getState: () => ({ getCollectionAuth: () => undefined }) },
}));

import { getGlobalVariables } from '@/lib/execute-request';
import {
  environmentKeys,
  reloadGlobalEnvironments,
  useDeleteGlobalEnvironment,
  useSaveGlobalEnvironment,
  useSetGlobalEnvironment,
} from '@/lib/queries/environment-queries';

function env(name: string, value: string): Environment {
  return {
    name,
    variables: [{ key: 'host', value, enabled: true, secret: false }],
  };
}

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  store.envs = { prod: env('prod', 'old'), dev: env('dev', 'dev-value') };
  store.active = 'prod';
  client.setQueryData(environmentKeys.globalName, 'prod');
  client.setQueryData(environmentKeys.global('prod'), env('prod', 'old'));
});

describe('global environment cache', () => {
  it('serves the saved value to execute-request right after a save', async () => {
    expect(getGlobalVariables()).toEqual({ host: 'old' });
    const { result } = renderHook(() => useSaveGlobalEnvironment(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync(env('prod', 'new'));
    });
    expect(getGlobalVariables()).toEqual({ host: 'new' });
  });

  it('keeps an environment named "name" off the active-name key', () => {
    expect(environmentKeys.global('name')).not.toEqual(environmentKeys.globalName);
    expect(environmentKeys.global('list')).not.toEqual(environmentKeys.globalList);
  });

  it('reads the newly active environment right after a switch', async () => {
    const { result } = renderHook(() => useSetGlobalEnvironment(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync('dev');
    });
    expect(getGlobalVariables()).toEqual({ host: 'dev-value' });
  });

  it('reads nothing right after the active environment is deleted', async () => {
    const { result } = renderHook(() => useDeleteGlobalEnvironment(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync('prod');
    });
    expect(getGlobalVariables()).toEqual({});
    expect(client.getQueryData(environmentKeys.global('prod'))).toBeUndefined();
  });

  it('keeps the active environment after deleting another one', async () => {
    const { result } = renderHook(() => useDeleteGlobalEnvironment(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync('dev');
    });
    expect(getGlobalVariables()).toEqual({ host: 'old' });
  });

  it('does not fail a saved change when the refetch fails', async () => {
    const api = await import('@/lib/tauri-api');
    vi.mocked(api.getGlobalEnvironment).mockRejectedValueOnce(new Error('offline'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { result } = renderHook(() => useSaveGlobalEnvironment(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync(env('prod', 'new'));
    });
    expect(result.current.isError).toBe(false);
    expect(getGlobalVariables()).toEqual({ host: 'new' });
    warn.mockRestore();
  });

  it('drops the previous workspace values when the workspace switches', async () => {
    // The other workspace has an environment with the same name and other values.
    store.envs = { prod: env('prod', 'other-workspace') };
    expect(getGlobalVariables()).toEqual({ host: 'old' });
    await reloadGlobalEnvironments(client);
    expect(getGlobalVariables()).toEqual({ host: 'other-workspace' });
  });
});
