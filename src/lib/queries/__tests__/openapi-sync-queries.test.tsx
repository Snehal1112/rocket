import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { collectionKeys } from '../collection-queries';
import {
  openApiSyncKeys,
  useApplySync,
  useCheckNow,
  useClearToken,
  useGrantConsent,
  useIsApplying,
  useOpenApiSyncEvents,
  useOpenApiSyncPlan,
  useOpenApiSyncStatus,
  useRevokeConsent,
  useSetInterval,
  useSetToken,
} from '../openapi-sync-queries';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncPlan: vi.fn(),
    onOpenApiSyncChecked: vi.fn(),
    onOpenApiSyncApplied: vi.fn(),
    checkOpenApiSyncNow: vi.fn(),
    setOpenApiSyncInterval: vi.fn(),
    setOpenApiSyncToken: vi.fn(),
    clearOpenApiSyncToken: vi.fn(),
    grantOpenApiSyncConsent: vi.fn(),
    revokeOpenApiSyncConsent: vi.fn(),
    applyOpenApiSync: vi.fn(),
  };
});

describe('openapi sync queries', () => {
  let client: QueryClient;
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );

  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockResolvedValue({
      pending: false,
      items: [],
      variableChanges: [],
      unchanged: [],
      skippedHidden: 0,
      deletedByUser: [],
      cautious: false,
      warnings: [],
    });
    vi.mocked(tauriApi.onOpenApiSyncChecked).mockResolvedValue(() => undefined);
    vi.mocked(tauriApi.onOpenApiSyncApplied).mockResolvedValue(() => undefined);
  });

  it('the status hook returns null for a collection without a source', async () => {
    const { result } = renderHook(() => useOpenApiSyncStatus('A'), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeNull();
  });

  it('the plan hook does not call the API while disabled', () => {
    renderHook(() => useOpenApiSyncPlan('A', false), { wrapper });
    expect(tauriApi.getOpenApiSyncPlan).not.toHaveBeenCalled();
  });

  it('the plan hook calls the API when enabled', async () => {
    renderHook(() => useOpenApiSyncPlan('A', true), { wrapper });
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledWith('A', []));
  });

  it('an event refreshes only the named collection', async () => {
    renderHook(
      () => {
        useOpenApiSyncEvents();
        useOpenApiSyncStatus('A');
        useOpenApiSyncPlan('A', true);
        useOpenApiSyncStatus('B');
        useOpenApiSyncPlan('B', true);
      },
      { wrapper },
    );
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(tauriApi.onOpenApiSyncChecked).toHaveBeenCalled());

    vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0]({
      collection: 'A',
      status: 'changed',
    });
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledTimes(3));
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledTimes(3));
    expect(vi.mocked(tauriApi.getOpenApiSyncStatus).mock.calls.map((c) => c[0])).toEqual([
      'A',
      'B',
      'A',
    ]);
    expect(vi.mocked(tauriApi.getOpenApiSyncPlan).mock.calls.map((c) => c[0])).toEqual([
      'A',
      'B',
      'A',
    ]);
  });

  it('unlistens on unmount', async () => {
    const unlisten = vi.fn();
    vi.mocked(tauriApi.onOpenApiSyncChecked).mockResolvedValue(unlisten);
    const { unmount } = renderHook(() => useOpenApiSyncEvents(), { wrapper });
    await waitFor(() => expect(tauriApi.onOpenApiSyncChecked).toHaveBeenCalled());
    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it('the status key includes includeToken and the plan key includes the links', () => {
    expect(openApiSyncKeys.status('A', true)).not.toEqual(openApiSyncKeys.status('A'));
    expect(openApiSyncKeys.plan('A', [['x', 'y']])).not.toEqual(openApiSyncKeys.plan('A', []));
  });

  it('the status hook passes includeToken to the API', async () => {
    renderHook(() => useOpenApiSyncStatus('A', true), { wrapper });
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('A', true));
  });

  it('a new link set refetches the plan', async () => {
    const { rerender } = renderHook(
      ({ links }: { links: Array<[string, string]> }) => useOpenApiSyncPlan('A', true, links),
      { wrapper, initialProps: { links: [] as Array<[string, string]> } },
    );
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledTimes(1));
    rerender({ links: [['a', 'b']] });
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledTimes(2));
    expect(tauriApi.getOpenApiSyncPlan).toHaveBeenLastCalledWith('A', [['a', 'b']]);
  });

  it('keeps the previous plan as placeholder data while a new link set loads', async () => {
    const { result, rerender } = renderHook(
      ({ links }: { links: Array<[string, string]> }) => useOpenApiSyncPlan('A', true, links),
      { wrapper, initialProps: { links: [] as Array<[string, string]> } },
    );
    await waitFor(() => expect(result.current.data).toBeDefined());
    const first = result.current.data;
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockReturnValue(new Promise(() => undefined));
    rerender({ links: [['a', 'b']] });
    expect(result.current.data).toBe(first);
    expect(result.current.isPlaceholderData).toBe(true);
  });

  it('does not show the plan of another collection as placeholder data', async () => {
    const { result, rerender } = renderHook(
      ({ c }: { c: string }) => useOpenApiSyncPlan(c, true),
      { wrapper, initialProps: { c: 'A' } },
    );
    await waitFor(() => expect(result.current.data).toBeDefined());
    vi.mocked(tauriApi.getOpenApiSyncPlan).mockReturnValue(new Promise(() => undefined));
    rerender({ c: 'B' });
    expect(result.current.data).toBeUndefined();
  });

  it('useIsApplying follows an apply running in any component', async () => {
    let finish: (v: unknown) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }) as never,
    );
    const watcher = renderHook(() => useIsApplying('W'), { wrapper });
    const other = renderHook(() => useIsApplying('X'), { wrapper });
    const runner = renderHook(() => useApplySync('W'), { wrapper });
    expect(watcher.result.current).toBe(false);
    runner.result.current.mutate({ items: [], skips: [], variables: [], renameLinks: [] });
    await waitFor(() => expect(watcher.result.current).toBe(true));
    expect(other.result.current).toBe(false);
    finish({});
    await waitFor(() => expect(watcher.result.current).toBe(false));
  });

  describe('mutations', () => {
    type InvalidateSpy = MockInstance<QueryClient['invalidateQueries']>;
    const invalidatedKeys = (spy: InvalidateSpy) =>
      spy.mock.calls.map((c) => (c[0] as { queryKey: unknown }).queryKey);

    const expectSyncKeysOnly = (spy: InvalidateSpy) => {
      const keys = invalidatedKeys(spy);
      expect(keys).toContainEqual(openApiSyncKeys.statusAll('A'));
      expect(keys).toContainEqual(openApiSyncKeys.planAll('A'));
      expect(keys).not.toContainEqual(collectionKeys.all);
    };

    it('useCheckNow invalidates status and plan', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useCheckNow('A'), { wrapper });
      await result.current.mutateAsync();
      expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('A');
      expectSyncKeysOnly(spy);
    });

    it('useSetInterval invalidates status and plan', async () => {
      vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useSetInterval('A'), { wrapper });
      await result.current.mutateAsync('daily');
      expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('A', 'daily');
      expectSyncKeysOnly(spy);
    });

    it('useSetToken invalidates status and plan', async () => {
      vi.mocked(tauriApi.setOpenApiSyncToken).mockResolvedValue(undefined);
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useSetToken('A'), { wrapper });
      await result.current.mutateAsync('tok');
      expect(tauriApi.setOpenApiSyncToken).toHaveBeenCalledWith('A', 'tok');
      expectSyncKeysOnly(spy);
    });

    it('useClearToken invalidates status and plan', async () => {
      vi.mocked(tauriApi.clearOpenApiSyncToken).mockResolvedValue(undefined);
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useClearToken('A'), { wrapper });
      await result.current.mutateAsync();
      expect(tauriApi.clearOpenApiSyncToken).toHaveBeenCalledWith('A');
      expectSyncKeysOnly(spy);
    });

    it('useGrantConsent invalidates status and plan', async () => {
      vi.mocked(tauriApi.grantOpenApiSyncConsent).mockResolvedValue(undefined);
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useGrantConsent('A'), { wrapper });
      await result.current.mutateAsync('https://x/y');
      expect(tauriApi.grantOpenApiSyncConsent).toHaveBeenCalledWith('A', 'https://x/y');
      expectSyncKeysOnly(spy);
    });

    it('useRevokeConsent invalidates status and plan', async () => {
      vi.mocked(tauriApi.revokeOpenApiSyncConsent).mockResolvedValue(undefined);
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useRevokeConsent('A'), { wrapper });
      await result.current.mutateAsync();
      expect(tauriApi.revokeOpenApiSyncConsent).toHaveBeenCalledWith('A');
      expectSyncKeysOnly(spy);
    });

    it('useApplySync returns the report and also invalidates the collection tree', async () => {
      const report = {
        applied: [],
        stale: [],
        notRewritable: [],
        deleted: [],
        detached: [],
        skipped: 0,
        remaining: 0,
      };
      vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useApplySync('A'), { wrapper });
      const selection = { items: [], skips: [], variables: [], renameLinks: [] };
      await expect(result.current.mutateAsync(selection)).resolves.toEqual(report);
      expect(tauriApi.applyOpenApiSync).toHaveBeenCalledWith('A', selection);
      const keys = invalidatedKeys(spy);
      expect(keys).toContainEqual(openApiSyncKeys.statusAll('A'));
      expect(keys).toContainEqual(openApiSyncKeys.planAll('A'));
      expect(keys).toContainEqual(collectionKeys.all);
    });
  });

  it('two apply calls from separate hook instances run the command once', async () => {
    let finish: (v: unknown) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }) as never,
    );
    const first = renderHook(() => useApplySync('G'), { wrapper });
    const second = renderHook(() => useApplySync('G'), { wrapper });
    const selection = { items: [], skips: [], variables: [], renameLinks: [] };
    const onSettled = vi.fn();
    first.result.current.mutate(selection);
    second.result.current.mutate(selection, { onSettled });
    await waitFor(() => expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1));
    expect(second.result.current.isPending).toBe(false);
    finish({
      applied: [],
      stale: [],
      notRewritable: [],
      deleted: [],
      detached: [],
      skipped: 0,
      remaining: 0,
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    expect(onSettled).not.toHaveBeenCalled();
    expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(1);
    // The guard is released afterwards.
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue({} as never);
    await second.result.current.mutateAsync(selection);
    expect(tauriApi.applyOpenApiSync).toHaveBeenCalledTimes(2);
  });

  it('the applied event refreshes status, plan and the tree for the named collection only', async () => {
    const spy = vi.spyOn(client, 'invalidateQueries');
    renderHook(() => useOpenApiSyncEvents(), { wrapper });
    await waitFor(() => expect(tauriApi.onOpenApiSyncApplied).toHaveBeenCalled());
    vi.mocked(tauriApi.onOpenApiSyncApplied).mock.calls[0]?.[0]({
      collection: 'A',
      applied: 1,
      stale: 0,
    });
    const keys = spy.mock.calls.map((c) => (c[0] as { queryKey: unknown }).queryKey);
    expect(keys).toContainEqual(openApiSyncKeys.statusAll('A'));
    expect(keys).toContainEqual(openApiSyncKeys.planAll('A'));
    expect(keys).toContainEqual(collectionKeys.all);
    expect(JSON.stringify(keys)).not.toContain('"B"');
  });
});
