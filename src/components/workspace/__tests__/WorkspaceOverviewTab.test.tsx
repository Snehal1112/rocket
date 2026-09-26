import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { WorkspaceOverviewTab } from '@/components/workspace/WorkspaceOverviewTab';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listWorkspaces: vi.fn(),
    listGlobalEnvironments: vi.fn(),
    onCollectionChanged: vi.fn(),
  };
});

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  setQueryClient(queryClient);
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('WorkspaceOverviewTab collection freshness', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listCollections).mockResolvedValue([
      {
        uid: 'c1',
        repositoryId: 'r1',
        name: 'my-api',
        path: '/ws/collections/my-api',
        requestCount: 3,
      },
    ]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([
      { id: 'ws-1', name: 'My Workspace', repositoryId: null } as never,
    ]);
    vi.mocked(tauriApi.listGlobalEnvironments).mockResolvedValue([]);
  });

  it('does not register its own onCollectionChanged listener — CollectionsSidebar owns invalidation of the shared cache', async () => {
    render(<WorkspaceOverviewTab workspaceId='ws-1' />, { wrapper });

    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalledTimes(1));

    // The component reads useCollections() from the shared query cache but no
    // longer listens for collection-changed events itself — CollectionsSidebar
    // (always mounted in the app shell) is the sole owner of that invalidation
    // now that both components share one query key. Asserting this listener is
    // never touched is the direct proof that a burst of events routed through
    // this component can no longer cause a second, duplicate list_collections call.
    expect(tauriApi.onCollectionChanged).not.toHaveBeenCalled();
  });
});
