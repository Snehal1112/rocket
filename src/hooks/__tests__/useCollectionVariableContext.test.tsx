import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionVariable } from '@/lib/tauri-api';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
}));
const queries = vi.hoisted(() => ({
  environments: [] as unknown[],
  globalEnv: null as unknown,
  globalName: null as string | null,
  processEnv: {} as Record<string, string>,
  seenCollection: null as string | null,
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: (collection: string | null) => {
    queries.seenCollection = collection;
    return { data: queries.environments };
  },
  useGlobalEnvironmentName: () => ({ data: queries.globalName }),
  useGlobalEnvironment: () => ({ data: queries.globalEnv }),
  useProcessEnvVars: () => ({ data: queries.processEnv }),
}));

import { useEnvStore } from '@/stores/env-store';
import { useCollectionVariableContext } from '../useCollectionVariableContext';

const cv = (key: string, value: string): CollectionVariable => ({
  key,
  value,
  initialValue: '',
  enabled: true,
  secret: false,
});

describe('useCollectionVariableContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getCollectionSettings.mockResolvedValue({ variables: [cv('tokenUrl', 'https://idp/token')] });
    queries.environments = [
      {
        name: 'dev',
        variables: [
          { key: 'clientId', value: 'dev-client', enabled: true, secret: false },
          { key: 'clientSecret', value: 'real-secret', enabled: true, secret: true },
          { key: 'off', value: 'x', enabled: false, secret: false },
        ],
        externalSecrets: [
          { alias: 'vault', connectionId: 'c', vaultName: 'v', secretNames: [{ name: 'k' }] },
        ],
      },
    ];
    queries.globalName = 'global';
    queries.globalEnv = {
      name: 'global',
      variables: [{ key: 'tenant', value: 'acme', enabled: true, secret: false }],
    };
    queries.processEnv = { HOME: '/home/u' };
    useEnvStore.setState({ activeEnvId: 'dev', activeCollection: 'other' });
  });

  it('layers process, global, collection and the active environment of the given collection', async () => {
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(result.current.variableContext.get('tokenUrl')).toBeDefined());
    const ctx = result.current.variableContext;
    expect(ctx.get('process.env.HOME')?.value).toBe('/home/u');
    expect(ctx.get('tenant')).toEqual(expect.objectContaining({ source: 'global', value: 'acme' }));
    expect(ctx.get('tokenUrl')).toEqual(expect.objectContaining({ source: 'collection' }));
    expect(ctx.get('clientId')).toEqual(
      expect.objectContaining({ source: 'environment', value: 'dev-client' }),
    );
    expect(ctx.has('off')).toBe(false);
    expect(ctx.get('vault.k')?.source).toBe('vault');
  });

  it('looks environments up in the collection argument, not the env store collection', () => {
    renderHook(() => useCollectionVariableContext('api'));
    expect(queries.seenCollection).toBe('api');
  });

  it('marks a secret environment variable as secret but keeps its value for resolution', async () => {
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(result.current.variableContext.get('tokenUrl')).toBeDefined());
    expect(result.current.variableContext.get('clientSecret')).toEqual(
      expect.objectContaining({ secret: true, value: 'real-secret' }),
    );
    expect(result.current.envVars.clientSecret).toBe('real-secret');
  });

  it('returns the plain maps and names the editors need for token keys', async () => {
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(result.current.collectionVars).toHaveLength(1));
    expect(result.current.envVars).toEqual({
      clientId: 'dev-client',
      clientSecret: 'real-secret',
    });
    expect(result.current.globalVars).toEqual({ tenant: 'acme' });
    expect(result.current.processEnvVars).toEqual({ HOME: '/home/u' });
    expect(result.current.activeEnvId).toBe('dev');
    expect(result.current.globalEnvName).toBe('global');
  });

  it('falls back to no collection variables when settings fail to load', async () => {
    api.getCollectionSettings.mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useCollectionVariableContext('api'));
    await waitFor(() => expect(api.getCollectionSettings).toHaveBeenCalled());
    expect(result.current.collectionVars).toEqual([]);
    expect(result.current.variableContext.get('clientId')).toBeDefined();
  });
});
