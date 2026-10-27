import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { collectionKeys } from '../collection-queries';
import {
  clearApplyState,
  openApiSyncKeys,
  useApplySync,
  useCheckNow,
  useClearToken,
  useDisconnectSync,
  useGrantConsent,
  useIsApplying,
  useLastApply,
  useOpenApiSyncBadges,
  useOpenApiSyncChanges,
  useOpenApiSyncEvents,
  useOpenApiSyncGlobalEvents,
  useOpenApiSyncOverview,
  useOpenApiSyncSpecView,
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
    getOpenApiSyncOverview: vi.fn(),
    getOpenApiSyncChanges: vi.fn(),
    getOpenApiSyncSpecView: vi.fn(),
    getOpenApiSyncBadges: vi.fn(),
    onOpenApiSyncChecked: vi.fn(),
    onOpenApiSyncApplied: vi.fn(),
    checkOpenApiSyncNow: vi.fn(),
    setOpenApiSyncInterval: vi.fn(),
    setOpenApiSyncToken: vi.fn(),
    clearOpenApiSyncToken: vi.fn(),
    grantOpenApiSyncConsent: vi.fn(),
    revokeOpenApiSyncConsent: vi.fn(),
    disconnectOpenApiSync: vi.fn(),
    applyOpenApiSync: vi.fn(),
  };
});

describe('openapi sync queries', () => {
  let client: QueryClient;
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );

  afterEach(() => clearApplyState());

  beforeEach(() => {
    clearApplyState();
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

  it('global_events_refresh_the_badge_list_for_any_collection', async () => {
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
    renderHook(
      () => {
        useOpenApiSyncGlobalEvents();
        useOpenApiSyncBadges();
      },
      { wrapper },
    );
    await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(tauriApi.onOpenApiSyncChecked).toHaveBeenCalled());
    await waitFor(() => expect(tauriApi.onOpenApiSyncApplied).toHaveBeenCalled());
    vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0]({
      collection: 'Zed',
      status: 'changed',
    });
    await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(2));
    vi.mocked(tauriApi.onOpenApiSyncApplied).mock.calls[0]?.[0]({ collection: 'Other' } as never);
    await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(3));
  });

  it('global_events_refresh_status_and_plan_only_for_the_named_collection', async () => {
    renderHook(
      () => {
        useOpenApiSyncGlobalEvents();
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

  it('the_listeners_are_removed_on_unmount', async () => {
    const unlistenChecked = vi.fn();
    const unlistenApplied = vi.fn();
    vi.mocked(tauriApi.onOpenApiSyncChecked).mockResolvedValue(unlistenChecked);
    vi.mocked(tauriApi.onOpenApiSyncApplied).mockResolvedValue(unlistenApplied);
    const { unmount } = renderHook(() => useOpenApiSyncGlobalEvents(), { wrapper });
    unmount();
    await waitFor(() => expect(unlistenChecked).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(unlistenApplied).toHaveBeenCalledTimes(1));
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

  it('the apply hook rejects a duplicate mutateAsync with the already-running error', async () => {
    let finish: (v: unknown) => void = () => undefined;
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
    vi.mocked(tauriApi.applyOpenApiSync).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }) as never,
    );
    const { result } = renderHook(() => useApplySync('D'), { wrapper });
    const selection = { items: [], skips: [], variables: [], renameLinks: [] };
    const first = result.current.mutateAsync(selection);
    await expect(result.current.mutateAsync(selection)).rejects.toThrow(
      'An apply is already running.',
    );
    finish({});
    await first;
  });

  it('keeps the last apply outcome per collection until it is dismissed', async () => {
    const report = {
      applied: [],
      stale: [],
      notRewritable: [],
      deleted: [],
      detached: [],
      skipped: 0,
      remaining: 0,
    };
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
    vi.mocked(tauriApi.applyOpenApiSync).mockResolvedValue(report);
    const runner = renderHook(() => useApplySync('L'), { wrapper });
    const first = renderHook(() => useLastApply('L'), { wrapper });
    expect(first.result.current.last).toBeNull();
    await runner.result.current.mutateAsync({
      items: [],
      skips: [],
      variables: [{ name: 'v', digest: 'd' }],
      renameLinks: [],
    });
    await waitFor(() => expect(first.result.current.last?.report).toEqual(report));
    expect(first.result.current.last?.variableCount).toBe(1);
    // A hook that mounts later still sees it.
    const later = renderHook(() => useLastApply('L'), { wrapper });
    expect(later.result.current.last?.report).toEqual(report);
    const other = renderHook(() => useLastApply('M'), { wrapper });
    expect(other.result.current.last).toBeNull();
    act(() => later.result.current.dismiss());
    expect(first.result.current.last).toBeNull();
  });

  it('keeps the error of a failed apply', async () => {
    vi.mocked(tauriApi.applyOpenApiSync).mockReset();
    vi.mocked(tauriApi.applyOpenApiSync).mockRejectedValue('boom');
    const runner = renderHook(() => useApplySync('E'), { wrapper });
    const watch = renderHook(() => useLastApply('E'), { wrapper });
    await expect(
      runner.result.current.mutateAsync({ items: [], skips: [], variables: [], renameLinks: [] }),
    ).rejects.toBe('boom');
    await waitFor(() => expect(watch.result.current.last?.error).toBe('boom'));
    expect(watch.result.current.last?.report).toBeUndefined();
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

    it('useDisconnectSync invalidates sync keys and the tree, and clears the last apply', async () => {
      vi.mocked(tauriApi.applyOpenApiSync).mockRejectedValue(new Error('x'));
      vi.mocked(tauriApi.disconnectOpenApiSync).mockResolvedValue(undefined);
      const selection = { items: [], skips: [], variables: [], renameLinks: [] };
      const apply = renderHook(() => ({ a: useApplySync('A'), l: useLastApply('A') }), {
        wrapper,
      });
      await act(async () => {
        await apply.result.current.a.mutateAsync(selection).catch(() => undefined);
      });
      expect(apply.result.current.l.last).not.toBeNull();
      const spy = vi.spyOn(client, 'invalidateQueries');
      const { result } = renderHook(() => useDisconnectSync('A'), { wrapper });
      await act(async () => {
        await result.current.mutateAsync(true);
      });
      expect(tauriApi.disconnectOpenApiSync).toHaveBeenCalledWith('A', true);
      const keys = invalidatedKeys(spy);
      expect(keys).toContainEqual(openApiSyncKeys.statusAll('A'));
      expect(keys).toContainEqual(openApiSyncKeys.planAll('A'));
      expect(keys).toContainEqual(collectionKeys.all);
      expect(apply.result.current.l.last).toBeNull();
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

  describe('overview, changes and badges', () => {
    const overview = {
      endpoints: 3,
      inSync: 2,
      changedByYou: 1,
      updatesWaiting: 0,
      specVersion: '1.0',
      lastSyncedAt: null,
      lastSyncedUpdated: 0,
    };
    const changes = { baselineTrusted: true, modified: [], added: [], deleted: [] };

    beforeEach(() => {
      vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue(overview);
      vi.mocked(tauriApi.getOpenApiSyncChanges).mockResolvedValue(changes);
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
    });

    it('the_overview_hook_returns_the_rows_unchanged', async () => {
      const { result } = renderHook(() => useOpenApiSyncOverview('A'), { wrapper });
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toBe(overview);
    });

    it('an_applied_event_refreshes_overview_changes_and_badges_for_that_collection', async () => {
      renderHook(
        () => {
          useOpenApiSyncEvents();
          useOpenApiSyncOverview('A');
          useOpenApiSyncChanges('A');
          useOpenApiSyncBadges();
        },
        { wrapper },
      );
      await waitFor(() => expect(tauriApi.getOpenApiSyncOverview).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(tauriApi.getOpenApiSyncChanges).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(tauriApi.onOpenApiSyncApplied).toHaveBeenCalled());
      vi.mocked(tauriApi.onOpenApiSyncApplied).mock.calls[0]?.[0]({
        collection: 'A',
        applied: 1,
        stale: 0,
      });
      await waitFor(() => expect(tauriApi.getOpenApiSyncOverview).toHaveBeenCalledTimes(2));
      await waitFor(() => expect(tauriApi.getOpenApiSyncChanges).toHaveBeenCalledTimes(2));
      await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(2));
    });

    it('an_applied_event_refreshes_the_spec_view_of_that_collection', async () => {
      renderHook(
        () => {
          useOpenApiSyncEvents();
          useOpenApiSyncSpecView('A');
          useOpenApiSyncSpecView('B');
        },
        { wrapper },
      );
      await waitFor(() => expect(tauriApi.getOpenApiSyncSpecView).toHaveBeenCalledTimes(2));
      await waitFor(() => expect(tauriApi.onOpenApiSyncApplied).toHaveBeenCalled());
      vi.mocked(tauriApi.onOpenApiSyncApplied).mock.calls[0]?.[0]({
        collection: 'A',
        applied: 1,
        stale: 0,
      });
      await waitFor(() => expect(tauriApi.getOpenApiSyncSpecView).toHaveBeenCalledTimes(3));
      expect(vi.mocked(tauriApi.getOpenApiSyncSpecView).mock.calls.map((c) => c[0])).toEqual([
        'A',
        'B',
        'A',
      ]);
    });

    it('a_checked_event_refreshes_overview_and_badges_but_not_other_collections', async () => {
      renderHook(
        () => {
          useOpenApiSyncEvents();
          useOpenApiSyncOverview('A');
          useOpenApiSyncOverview('B');
          useOpenApiSyncBadges();
        },
        { wrapper },
      );
      await waitFor(() => expect(tauriApi.getOpenApiSyncOverview).toHaveBeenCalledTimes(2));
      await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(tauriApi.onOpenApiSyncChecked).toHaveBeenCalled());
      vi.mocked(tauriApi.onOpenApiSyncChecked).mock.calls[0]?.[0]({
        collection: 'A',
        status: 'changed',
      });
      await waitFor(() => expect(tauriApi.getOpenApiSyncOverview).toHaveBeenCalledTimes(3));
      await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalledTimes(2));
      expect(vi.mocked(tauriApi.getOpenApiSyncOverview).mock.calls.map((c) => c[0])).toEqual([
        'A',
        'B',
        'A',
      ]);
    });
  });
});
