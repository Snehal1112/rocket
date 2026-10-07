import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CollectionVariable } from '@/lib/tauri-api';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderChainVariables: vi.fn(),
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: () => ({ data: [] }),
  useGlobalEnvironmentName: () => ({ data: null }),
  useGlobalEnvironment: () => ({ data: null }),
  useProcessEnvVars: () => ({ data: {} }),
}));

import { useFolderVariableContext } from '../useFolderVariableContext';

const v = (key: string, value: string): CollectionVariable => ({
  key,
  value,
  initialValue: '',
  enabled: true,
  secret: false,
});

describe('useFolderVariableContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getCollectionSettings.mockResolvedValue({ variables: [v('host', 'c.example')] });
    api.getFolderChainVariables.mockResolvedValue([v('host', 'outer.example'), v('team', 'a')]);
  });

  it('layers collection, saved folder chain and unsaved folder variables', async () => {
    const { result } = renderHook(() =>
      useFolderVariableContext('demo', 'api/users', [v('team', 'b')]),
    );
    await waitFor(() => expect(result.current.variableContext.get('host')?.source).toBe('folder'));
    expect(result.current.variableContext.get('host')?.value).toBe('outer.example');
    expect(result.current.variableContext.get('team')?.value).toBe('b');
    expect(api.getFolderChainVariables).toHaveBeenCalledWith('demo', 'api/users/folder.yml');
  });

  it('skips the chain lookup for the root folder', async () => {
    const { result } = renderHook(() => useFolderVariableContext('demo', '', []));
    await waitFor(() =>
      expect(result.current.variableContext.get('host')?.source).toBe('collection'),
    );
    expect(api.getFolderChainVariables).not.toHaveBeenCalled();
  });
});
