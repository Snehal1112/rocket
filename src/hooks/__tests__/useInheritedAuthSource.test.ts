import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const resolve = vi.hoisted(() => vi.fn());
vi.mock('@/lib/inherited-auth', () => ({ resolveInheritedAuthSource: resolve }));

import { useInheritedAuthSource } from '../useInheritedAuthSource';

describe('useInheritedAuthSource', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resolve.mockResolvedValue({ kind: 'none' });
  });

  it('resolves the source when enabled', async () => {
    const { result } = renderHook(() => useInheritedAuthSource('demo', 'a/req.yml', true));
    await waitFor(() => expect(result.current).toEqual({ kind: 'none' }));
    expect(resolve).toHaveBeenCalledWith('demo', 'a/req.yml');
  });

  it('does nothing when disabled or when the request has no source', () => {
    const off = renderHook(() => useInheritedAuthSource('demo', 'a/req.yml', false));
    const none = renderHook(() => useInheritedAuthSource(undefined, undefined, true));
    expect(off.result.current).toBeUndefined();
    expect(none.result.current).toBeUndefined();
    expect(resolve).not.toHaveBeenCalled();
  });

  it('drops the source again when it becomes disabled', async () => {
    const { result, rerender } = renderHook(
      ({ enabled }) => useInheritedAuthSource('demo', 'a/req.yml', enabled),
      { initialProps: { enabled: true } },
    );
    await waitFor(() => expect(result.current).toBeDefined());
    rerender({ enabled: false });
    await waitFor(() => expect(result.current).toBeUndefined());
  });
});
