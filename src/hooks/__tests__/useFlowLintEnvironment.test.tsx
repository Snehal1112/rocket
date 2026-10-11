import { QueryClient } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { setQueryClient } from '@/lib/query-client';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { useFlowLintEnvironment } from '../useFlowLintEnvironment';

describe('useFlowLintEnvironment', () => {
  let client: QueryClient;
  beforeEach(() => {
    client = new QueryClient();
    setQueryClient(client);
  });

  it('keeps the global name unknown, and so names no environment, until it has loaded', () => {
    const { result } = renderHook(() => useFlowLintEnvironment('dev'));
    // An unloaded global name could hide a variable, so the lint must not claim to know.
    expect(result.current).toEqual({ environmentName: null, globalEnvName: null });
  });

  it('lints again with the names once the global name loads', () => {
    const { result } = renderHook(() => useFlowLintEnvironment('dev'));
    act(() => client.setQueryData(environmentKeys.globalName, 'shared'));
    expect(result.current).toEqual({ environmentName: 'dev', globalEnvName: 'shared' });
  });

  it('treats a loaded "no global environment" as a real answer', () => {
    client.setQueryData(environmentKeys.globalName, null);
    const { result } = renderHook(() => useFlowLintEnvironment('dev'));
    expect(result.current).toEqual({ environmentName: 'dev', globalEnvName: null });
  });

  it('follows a switch of the global environment', () => {
    client.setQueryData(environmentKeys.globalName, 'a');
    const { result } = renderHook(() => useFlowLintEnvironment(null));
    act(() => client.setQueryData(environmentKeys.globalName, 'b'));
    expect(result.current).toEqual({ environmentName: null, globalEnvName: 'b' });
  });
});
