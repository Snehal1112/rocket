import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({
  getCollectionSettings: vi.fn(),
  getFolderVariables: vi.fn(),
  getRequestVariables: vi.fn(),
}));
const queries = vi.hoisted(() => ({
  resolved: {} as Record<string, unknown>,
  processEnv: vi.fn((_collection: string | null, enabled = true) => ({
    data: enabled ? { HOME: '/home/me' } : {},
  })),
}));
vi.mock('@/lib/tauri-api', () => api);
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: () => ({ data: [] }),
  useGlobalEnvironmentName: () => ({ data: null }),
  useProcessEnvVars: queries.processEnv,
  useResolvedEnvironment: (collection: string | null, name: string | null) => ({
    data: name ? queries.resolved[`${collection}/${name}`] : undefined,
  }),
}));

import { CLOSED_PROCESS_ENV } from '@/lib/process-env-gate';
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
    const { result } = renderHook(() =>
      useGrpcVariableContext({ collection: 'api', path: 'a.yml' }, 'api'),
    );
    await waitFor(() => expect(api.getCollectionSettings).toHaveBeenCalled());
    const ctx = result.current.variableContext;
    expect(ctx.get('baseVar')).toEqual(
      expect.objectContaining({ source: 'environment', value: 'from-base' }),
    );
    expect(ctx.get('pw')?.secret).toBe(true);
  });

  it('closed_scope_queries_no_process_env', () => {
    const { result } = renderHook(() => useGrpcVariableContext(undefined, CLOSED_PROCESS_ENV));
    expect(queries.processEnv).toHaveBeenCalledWith(null, false);
    expect(result.current.variableContext.has('process.env.HOME')).toBe(false);
  });

  it('history_collection_scope_queries_that_collection', () => {
    renderHook(() => useGrpcVariableContext(undefined, 'clone'));
    expect(queries.processEnv).toHaveBeenCalledWith('clone', true);
  });

  it('scratch_scope_queries_the_full_map', () => {
    const { result } = renderHook(() => useGrpcVariableContext(undefined, undefined));
    expect(queries.processEnv).toHaveBeenCalledWith(null, true);
    expect(result.current.variableContext.has('process.env.HOME')).toBe(true);
  });
});
