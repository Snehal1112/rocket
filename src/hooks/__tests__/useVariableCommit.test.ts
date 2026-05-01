import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { useVariableCommit } from '../useVariableCommit';

const mockSaveEnvironment = vi.fn().mockResolvedValue(undefined);
const mockSaveGlobalEnvironment = vi.fn().mockResolvedValue(undefined);

vi.mock('@/lib/tauri-api', () => ({
  listEnvironments: vi.fn().mockResolvedValue([]),
  saveEnvironment: mockSaveEnvironment,
  saveGlobalEnvironment: mockSaveGlobalEnvironment,
  getGlobalEnvironmentName: vi.fn().mockResolvedValue('global'),
  getGlobalEnvironment: vi.fn().mockResolvedValue({
    name: 'global',
    variables: [{ key: 'apiKey', value: 'old-key', enabled: true, secret: false }],
  }),
  listGlobalEnvironments: vi.fn().mockResolvedValue([]),
  getProcessEnvVars: vi.fn().mockResolvedValue({}),
}));

const stagingEnv = {
  name: 'staging',
  variables: [{ key: 'baseUrl', value: 'https://staging.api.com', enabled: true, secret: false }],
};

const globalEnv = {
  name: 'global',
  variables: [{ key: 'apiKey', value: 'old-key', enabled: true, secret: false }],
};

function makeWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // Seed cache with test data.
  qc.setQueryData(environmentKeys.collection('col'), [stagingEnv]);
  qc.setQueryData(environmentKeys.globalName, 'global');
  qc.setQueryData(environmentKeys.global('global'), globalEnv);

  return ({ children }: { children: ReactNode }) =>
    React.createElement(QueryClientProvider, { client: qc }, children);
}

beforeEach(() => {
  vi.clearAllMocks();
  // Set up env store state.
  const { useEnvStore } = require('@/stores/env-store');
  useEnvStore.setState({ activeEnvId: 'staging', activeCollection: 'col' });
});

describe('useVariableCommit', () => {
  it('updates existing env variable', async () => {
    const { result } = renderHook(() => useVariableCommit(), { wrapper: makeWrapper() });
    await act(async () => {
      await result.current('baseUrl', 'https://prod.api.com', 'environment');
    });
    expect(mockSaveEnvironment).toHaveBeenCalledWith(
      'col',
      expect.objectContaining({
        name: 'staging',
        variables: expect.arrayContaining([
          expect.objectContaining({ key: 'baseUrl', value: 'https://prod.api.com' }),
        ]),
      }),
    );
  });

  it('adds new env variable when key does not exist', async () => {
    const { result } = renderHook(() => useVariableCommit(), { wrapper: makeWrapper() });
    await act(async () => {
      await result.current('newVar', 'newVal', 'environment');
    });
    expect(mockSaveEnvironment).toHaveBeenCalledWith(
      'col',
      expect.objectContaining({
        variables: expect.arrayContaining([
          expect.objectContaining({ key: 'newVar', value: 'newVal', enabled: true }),
        ]),
      }),
    );
  });

  it('updates global variable', async () => {
    const { result } = renderHook(() => useVariableCommit(), { wrapper: makeWrapper() });
    await act(async () => {
      await result.current('apiKey', 'new-key', 'global');
    });
    expect(mockSaveGlobalEnvironment).toHaveBeenCalledWith(
      expect.objectContaining({
        variables: expect.arrayContaining([
          expect.objectContaining({ key: 'apiKey', value: 'new-key' }),
        ]),
      }),
    );
  });

  it('routes null scope to active environment', async () => {
    const { result } = renderHook(() => useVariableCommit(), { wrapper: makeWrapper() });
    await act(async () => {
      await result.current('unknownVar', 'someValue', null);
    });
    expect(mockSaveEnvironment).toHaveBeenCalled();
  });

  it('does nothing for read-only scopes', async () => {
    const { result } = renderHook(() => useVariableCommit(), { wrapper: makeWrapper() });
    for (const scope of ['collection', 'folder', 'request', 'process', 'runtime'] as const) {
      await act(async () => {
        await result.current('someVar', 'someVal', scope);
      });
    }
    expect(mockSaveEnvironment).not.toHaveBeenCalled();
    expect(mockSaveGlobalEnvironment).not.toHaveBeenCalled();
  });
});
