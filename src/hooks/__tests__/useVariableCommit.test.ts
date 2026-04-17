import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useVariableCommit } from '../useVariableCommit';

// Mock the env store
const mockUpdateEnvironment = vi.fn();
const mockUpdateGlobalEnvironment = vi.fn();

vi.mock('@/stores/env-store', () => ({
  useEnvStore: (selector: (s: unknown) => unknown) =>
    selector({
      activeEnvId: 'staging',
      environments: [
        {
          name: 'staging',
          variables: [
            { key: 'baseUrl', value: 'https://staging.api.com', enabled: true, secret: false },
          ],
        },
      ],
      updateEnvironment: mockUpdateEnvironment,
      globalEnv: {
        name: 'global',
        variables: [{ key: 'apiKey', value: 'old-key', enabled: true, secret: false }],
      },
      updateGlobalEnvironment: mockUpdateGlobalEnvironment,
    }),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('useVariableCommit', () => {
  it('updates existing env variable', async () => {
    const { result } = renderHook(() => useVariableCommit());
    await act(async () => {
      await result.current('baseUrl', 'https://prod.api.com', 'environment');
    });
    expect(mockUpdateEnvironment).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'staging',
        variables: expect.arrayContaining([
          expect.objectContaining({ key: 'baseUrl', value: 'https://prod.api.com' }),
        ]),
      }),
    );
  });

  it('adds new env variable when key does not exist', async () => {
    const { result } = renderHook(() => useVariableCommit());
    await act(async () => {
      await result.current('newVar', 'newVal', 'environment');
    });
    expect(mockUpdateEnvironment).toHaveBeenCalledWith(
      expect.objectContaining({
        variables: expect.arrayContaining([
          expect.objectContaining({ key: 'newVar', value: 'newVal', enabled: true }),
        ]),
      }),
    );
  });

  it('updates global variable', async () => {
    const { result } = renderHook(() => useVariableCommit());
    await act(async () => {
      await result.current('apiKey', 'new-key', 'global');
    });
    expect(mockUpdateGlobalEnvironment).toHaveBeenCalledWith(
      expect.objectContaining({
        variables: expect.arrayContaining([
          expect.objectContaining({ key: 'apiKey', value: 'new-key' }),
        ]),
      }),
    );
  });

  it('routes null scope to active environment', async () => {
    const { result } = renderHook(() => useVariableCommit());
    await act(async () => {
      await result.current('unknownVar', 'someValue', null);
    });
    expect(mockUpdateEnvironment).toHaveBeenCalled();
  });

  it('does nothing for read-only scopes', async () => {
    const { result } = renderHook(() => useVariableCommit());
    for (const scope of ['collection', 'folder', 'request', 'process', 'runtime'] as const) {
      await act(async () => {
        await result.current('someVar', 'someVal', scope);
      });
    }
    expect(mockUpdateEnvironment).not.toHaveBeenCalled();
    expect(mockUpdateGlobalEnvironment).not.toHaveBeenCalled();
  });
});
