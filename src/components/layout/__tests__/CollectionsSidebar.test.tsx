import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listWorkspaces: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

// Marker component so the test can assert on mount/unmount without pulling in
// HistoryPanel's own dependencies (tauri-api's listHistory/searchHistory).
vi.mock('@/components/history/HistoryPanel', () => ({
  HistoryPanel: () => <div data-testid='history-panel-marker' />,
}));

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  setQueryClient(queryClient);
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('CollectionsSidebar History panel deferred mount', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listCollections).mockResolvedValue([]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([]);
  });

  it('does not mount HistoryPanel on initial render (Collections tab active)', async () => {
    render(<CollectionsSidebar />, { wrapper });

    // Positive control: wait for the sidebar to finish its initial data load
    // before asserting on absence, so the check below isn't trivially true.
    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalled());

    expect(screen.queryByTestId('history-panel-marker')).not.toBeInTheDocument();
  });

  it('mounts HistoryPanel after the History tab is activated for the first time', async () => {
    render(<CollectionsSidebar />, { wrapper });

    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalled());

    fireEvent.click(screen.getByRole('tab', { name: 'History' }));

    expect(screen.getByTestId('history-panel-marker')).toBeInTheDocument();
  });

  it('keeps HistoryPanel mounted after switching back to Collections (only the first mount is deferred)', async () => {
    render(<CollectionsSidebar />, { wrapper });

    await waitFor(() => expect(tauriApi.listCollections).toHaveBeenCalled());

    fireEvent.click(screen.getByRole('tab', { name: 'History' }));
    expect(screen.getByTestId('history-panel-marker')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: 'Collections' }));
    expect(screen.getByTestId('history-panel-marker')).toBeInTheDocument();
  });
});
