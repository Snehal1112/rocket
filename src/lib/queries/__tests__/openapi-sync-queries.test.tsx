import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import {
  useOpenApiSyncEvents,
  useOpenApiSyncPlan,
  useOpenApiSyncStatus,
} from '../openapi-sync-queries';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncPlan: vi.fn(),
    onOpenApiSyncChecked: vi.fn(),
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
    await waitFor(() => expect(tauriApi.getOpenApiSyncPlan).toHaveBeenCalledWith('A'));
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
});
