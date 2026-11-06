import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { createDeferred } from '@/test/deferred';

const mocks = vi.hoisted(() => ({ toast: { error: vi.fn() } }));

vi.mock('sonner', () => ({ toast: mocks.toast }));

describe('useAsyncAction', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('is busy while the action runs and resolves true when it succeeds', async () => {
    const pending = createDeferred<void>();
    const { result } = renderHook(() => useAsyncAction());
    let outcome: Promise<boolean> = Promise.resolve(false);
    act(() => {
      outcome = result.current.run(() => pending.promise);
    });
    expect(result.current.busy).toBe(true);
    await act(async () => {
      pending.resolve();
    });
    await expect(outcome).resolves.toBe(true);
    expect(result.current.busy).toBe(false);
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('toasts the cleaned error and resolves false by default', async () => {
    const { result } = renderHook(() => useAsyncAction());
    let ok = true;
    await act(async () => {
      ok = await result.current.run(() => Promise.reject('Internal error: disk full'));
    });
    expect(ok).toBe(false);
    expect(mocks.toast.error).toHaveBeenCalledWith('disk full');
    expect(result.current.error).toBeNull();
    expect(result.current.busy).toBe(false);
  });

  it('keeps the error inline, without a toast, in inline mode', async () => {
    const { result } = renderHook(() => useAsyncAction('inline'));
    await act(async () => {
      await result.current.run(() => Promise.reject('Invalid input: Port is taken'));
    });
    expect(result.current.error).toBe('Port is taken');
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('clears the inline error when the next run starts', async () => {
    const { result } = renderHook(() => useAsyncAction('inline'));
    await act(async () => {
      await result.current.run(() => Promise.reject('Internal error: first'));
    });
    expect(result.current.error).toBe('first');
    await act(async () => {
      await result.current.run(() => Promise.resolve());
    });
    expect(result.current.error).toBeNull();
  });

  it('lets the caller set and clear the inline error', () => {
    const { result } = renderHook(() => useAsyncAction('inline'));
    act(() => result.current.setError('Enter a name.'));
    expect(result.current.error).toBe('Enter a name.');
    act(() => result.current.setError(null));
    expect(result.current.error).toBeNull();
  });

  it('reset clears the error and the busy flag', async () => {
    const pending = createDeferred<void>();
    const { result } = renderHook(() => useAsyncAction('inline'));
    act(() => result.current.setError('old'));
    act(() => {
      void result.current.run(() => pending.promise);
    });
    expect(result.current.busy).toBe(true);
    act(() => result.current.reset());
    expect(result.current.busy).toBe(false);
    expect(result.current.error).toBeNull();
    await act(async () => {
      pending.resolve();
    });
  });

  it('keeps run and reset stable across renders', () => {
    const { result, rerender } = renderHook(() => useAsyncAction());
    const { run, reset } = result.current;
    rerender();
    expect(result.current.run).toBe(run);
    expect(result.current.reset).toBe(reset);
  });
});
