import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderVariables: vi.fn(),
  getRequestVariables: vi.fn(),
}));
const queries = vi.hoisted(() => ({
  resolved: {} as Record<string, unknown>,
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: () => ({ data: [] }),
  useGlobalEnvironmentName: () => ({ data: null }),
  useProcessEnvVars: () => ({ data: {} }),
  useResolvedEnvironment: (collection: string | null, name: string | null) => ({
    data: name ? queries.resolved[`${collection}/${name}`] : undefined,
  }),
}));

import { useEnvStore } from '@/stores/env-store';
import { useGrpcVariableContext } from '../useGrpcVariableContext';

describe('useGrpcVariableContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getCollectionSettings.mockResolvedValue({ variables: [] });
    api.getFolderVariables.mockResolvedValue([]);
    api.getRequestVariables.mockResolvedValue([]);
    queries.resolved = {
      'api/child': {
        name: 'child',
        warnings: [],
        variables: [
          { key: 'baseVar', value: 'from-base', enabled: true, secret: false, source: 'base' },
          { key: 'pw', value: 'hunter22', enabled: true, secret: true, source: 'base' },
        ],
      },
    };
    useEnvStore.setState({ activeEnvId: 'child', activeCollection: 'api' });
  });

  it('includes_inherited_variables', async () => {
    const { result } = renderHook(() => useGrpcVariableContext({ collection: 'api', path: 'a.yml' }));
    await waitFor(() => expect(api.getCollectionSettings).toHaveBeenCalled());
    const ctx = result.current.variableContext;
    expect(ctx.get('baseVar')).toEqual(
      expect.objectContaining({ source: 'environment', value: 'from-base' }),
    );
    expect(ctx.get('pw')?.secret).toBe(true);
  });
});
