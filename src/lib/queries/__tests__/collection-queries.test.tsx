import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn() };
});

// Shared across both renders within a single test so the two hooks hit the
// same cache — mirrors the single app-wide QueryClient in production, where
// CollectionsSidebar and WorkspaceOverviewTab both read through getQueryClient().
// Recreated in beforeEach so tests don't leak cache state between each other.
let queryClient: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('useCollections', () => {
  beforeEach(() => {
    // staleTime mirrors the app's production QueryClient (src/main.tsx) so this
    // test reflects real refetch-on-mount behavior rather than the library's
    // staleTime: 0 default, which would refetch on every new mount regardless
    // of cache sharing.
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
    });
    // The shared-cache test below invalidates through the real getQueryClient()
    // (as production code does), so it must resolve to this test's client.
    setQueryClient(queryClient);
    vi.mocked(tauriApi.listCollections).mockResolvedValue([
      {
        uid: 'c1',
        repositoryId: 'r1',
        name: 'my-api',
        path: '/ws/collections/my-api',
        requestCount: 3,
      },
    ]);
  });

  it('fetches collections once and serves a second mount from cache', async () => {
    const { useCollections } = await import('../collection-queries');
    const { result: first } = renderHook(() => useCollections(), { wrapper });
    await waitFor(() => expect(first.current.data).toHaveLength(1));

    const { result: second } = renderHook(() => useCollections(), { wrapper });
    await waitFor(() => expect(second.current.data).toHaveLength(1));

    expect(tauriApi.listCollections).toHaveBeenCalledTimes(1);
  });

  it('useCollections is a shared cache — invalidating once refetches for every mounted consumer', async () => {
    const { useCollections, collectionKeys } = await import('../collection-queries');
    const { getQueryClient } = await import('@/lib/query-client');
    const { result } = renderHook(() => useCollections(), { wrapper });
    await waitFor(() => expect(result.current.data).toHaveLength(1));

    vi.mocked(tauriApi.listCollections).mockResolvedValue([
      {
        uid: 'c1',
        repositoryId: 'r1',
        name: 'my-api',
        path: '/ws/collections/my-api',
        requestCount: 4,
      },
    ]);
    await act(async () => {
      await getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
    });

    await waitFor(() => expect(result.current.data?.[0].requestCount).toBe(4));
  });
});
