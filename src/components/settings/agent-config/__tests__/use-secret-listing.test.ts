import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { SCOPE_DEBOUNCE_MS, useSecretListing } from '../use-secret-listing';

vi.mock('@/lib/tauri-api', () => ({ fetchExternalSecretNames: vi.fn() }));

type Refs = api.ExternalSecretRef[];

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const fetchMock = () => vi.mocked(api.fetchExternalSecretNames);

type Props = { connectionId: string; scope: string; enabled: boolean };
const start = (props: Props) => renderHook((p: Props) => useSecretListing(p), { initialProps: props });

describe('useSecretListing', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('stays idle and does not fetch while disabled', () => {
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: false });
    expect(result.current.status).toBe('idle');
    expect(result.current.secrets).toEqual([]);
    expect(fetchMock()).not.toHaveBeenCalled();
  });

  it('loads and maps the secrets', async () => {
    fetchMock().mockResolvedValue([{ name: 'anthropic', secretId: 's1' }]);
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    expect(result.current.status).toBe('loading');
    await vi.waitFor(() => expect(result.current.status).toBe('ready'));
    expect(fetchMock()).toHaveBeenCalledWith('c1', 'prod');
    expect(result.current.secrets).toEqual([{ id: 's1', name: 'anthropic' }]);
    expect(result.current.error).toBeNull();
  });

  it('becomes ready with an empty list', async () => {
    fetchMock().mockResolvedValue([] as Refs);
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    await vi.waitFor(() => expect(result.current.status).toBe('ready'));
    expect(result.current.secrets).toEqual([]);
  });

  it('reports an error and clears the list', async () => {
    fetchMock().mockRejectedValue('boom');
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    await vi.waitFor(() => expect(result.current.status).toBe('error'));
    expect(result.current.error).toBe('boom');
    expect(result.current.secrets).toEqual([]);
  });

  it('ignores a stale response after the connection changes', async () => {
    const a = deferred<Refs>();
    const b = deferred<Refs>();
    fetchMock().mockImplementation((id) => (id === 'a' ? a.promise : b.promise));
    const { result, rerender } = start({ connectionId: 'a', scope: 's', enabled: true });
    rerender({ connectionId: 'b', scope: 's', enabled: true });
    await act(async () => {
      b.resolve([{ name: 'B', secretId: 'b1' }]);
      await b.promise;
    });
    await act(async () => {
      a.resolve([{ name: 'A', secretId: 'a1' }]);
      await a.promise;
    });
    expect(result.current.status).toBe('ready');
    expect(result.current.secrets).toEqual([{ id: 'b1', name: 'B' }]);
  });

  it('drops a response that arrives after the hook was disabled', async () => {
    const a = deferred<Refs>();
    fetchMock().mockReturnValue(a.promise);
    const { result, rerender } = start({ connectionId: 'a', scope: 's', enabled: true });
    rerender({ connectionId: 'a', scope: 's', enabled: false });
    await act(async () => {
      a.resolve([{ name: 'A', secretId: 'a1' }]);
      await a.promise;
    });
    expect(result.current.status).toBe('idle');
    expect(result.current.secrets).toEqual([]);
  });

  it('debounces rapid scope typing into one fetch', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { rerender } = start({ connectionId: 'c1', scope: 'p', enabled: true });
    expect(fetchMock()).toHaveBeenCalledTimes(1);
    rerender({ connectionId: 'c1', scope: 'pr', enabled: true });
    rerender({ connectionId: 'c1', scope: 'pro', enabled: true });
    rerender({ connectionId: 'c1', scope: 'prod', enabled: true });
    expect(fetchMock()).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCOPE_DEBOUNCE_MS);
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('c1', 'prod');
  });

  it('restarts the wait on every keystroke', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { rerender } = start({ connectionId: 'c1', scope: 'p', enabled: true });
    rerender({ connectionId: 'c1', scope: 'pr', enabled: true });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    rerender({ connectionId: 'c1', scope: 'pro', enabled: true });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(fetchMock()).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('c1', 'pro');
    expect(fetchMock()).not.toHaveBeenCalledWith('c1', 'pr');
  });

  it('loads at once when the connection changes', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { rerender } = start({ connectionId: 'a', scope: 'x', enabled: true });
    rerender({ connectionId: 'b', scope: 'y', enabled: true });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('b', 'y');
  });

  it('refreshes at once with the latest scope', async () => {
    vi.useFakeTimers();
    fetchMock().mockResolvedValue([]);
    const { result, rerender } = start({ connectionId: 'c1', scope: 'p', enabled: true });
    rerender({ connectionId: 'c1', scope: 'pr', enabled: true });
    act(() => {
      result.current.refresh();
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
    expect(fetchMock()).toHaveBeenLastCalledWith('c1', 'pr');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCOPE_DEBOUNCE_MS);
    });
    expect(fetchMock()).toHaveBeenCalledTimes(2);
  });

  it('refreshes the current list when the scope did not change', async () => {
    fetchMock().mockResolvedValue([{ name: 'one', secretId: 's1' }]);
    const { result } = start({ connectionId: 'c1', scope: 'prod', enabled: true });
    await vi.waitFor(() => expect(result.current.status).toBe('ready'));
    fetchMock().mockResolvedValue([
      { name: 'one', secretId: 's1' },
      { name: 'two', secretId: 's2' },
    ]);
    act(() => {
      result.current.refresh();
    });
    await vi.waitFor(() => expect(result.current.secrets).toHaveLength(2));
    expect(fetchMock()).toHaveBeenCalledTimes(2);
  });

  it('does nothing on refresh while disabled', () => {
    const { result } = start({ connectionId: '', scope: '', enabled: false });
    act(() => {
      result.current.refresh();
    });
    expect(fetchMock()).not.toHaveBeenCalled();
  });
});
