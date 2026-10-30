import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const queries = vi.hoisted(() => ({
  processEnv: vi.fn((_collection: string | null | undefined, enabled = true) => ({
    data: enabled ? { HOME: '/home/me' } : {},
  })),
}));
vi.mock('@/lib/tauri-api', () => ({
  getCollectionSettings: vi.fn().mockResolvedValue({ variables: [] }),
}));
vi.mock('@/lib/queries/environment-queries', () => ({
  useEnvironments: () => ({ data: [] }),
  useGlobalEnvironmentName: () => ({ data: null }),
  useGlobalEnvironment: () => ({ data: null }),
  useProcessEnvVars: queries.processEnv,
}));

import { CLOSED_PROCESS_ENV } from '@/lib/process-env-gate';
import { useWebSocketVariableContext } from '../useWebSocketVariableContext';

describe('useWebSocketVariableContext', () => {
  beforeEach(() => {
    queries.processEnv.mockClear();
  });

  it('closed scope queries no process env', () => {
    const { result } = renderHook(() => useWebSocketVariableContext(undefined, CLOSED_PROCESS_ENV));
    expect(queries.processEnv).toHaveBeenCalledWith(null, false);
    expect(result.current.has('process.env.HOME')).toBe(false);
  });

  it('history collection scope queries that collection', () => {
    renderHook(() => useWebSocketVariableContext(undefined, 'clone'));
    expect(queries.processEnv).toHaveBeenCalledWith('clone', true);
  });

  it('saved request queries its collection', () => {
    renderHook(() => useWebSocketVariableContext('api', 'api'));
    expect(queries.processEnv).toHaveBeenCalledWith('api', true);
  });
});
