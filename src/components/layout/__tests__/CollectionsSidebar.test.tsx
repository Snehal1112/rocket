import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { findScriptTab } from '@/lib/pane-utils';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listWorkspaces: vi.fn(),
    getCollectionSummaries: vi.fn(),
    deleteScriptFile: vi.fn(),
    readScriptFile: vi.fn(),
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

describe('CollectionsSidebar script delete', () => {
  const summary: tauriApi.CollectionSummary = {
    uid: 'c1',
    repositoryId: 'r1',
    name: 'col',
    path: '/w/col',
    requestCount: 0,
  };

  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(tauriApi.listCollections).mockResolvedValue([summary]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([
      { id: 'ws1', repositoryId: 'r1', name: 'WS', path: '/w', pinned: false },
    ]);
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'col',
      root: {
        uid: 'root',
        name: 'col',
        items: [{ type: 'scriptFile', fileName: 'utils.js', name: 'utils.js' }],
      },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
    vi.mocked(tauriApi.readScriptFile).mockResolvedValue('x');
    vi.mocked(tauriApi.deleteScriptFile).mockReset().mockResolvedValue(undefined);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws1' });
    usePaneStore.setState({ activeCollection: 'col' });
  });

  it('warns about unsaved edits and closes the script tab on confirm', async () => {
    await usePaneStore.getState().openScriptTab('col', 'utils.js');
    const tabId = findScriptTab(usePaneStore.getState().root, 'col', 'utils.js')?.tab.id ?? '';
    usePaneStore.getState().updateScriptContent(tabId, 'edited');

    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for utils.js' }));
    await userEvent.click(await screen.findByText('Delete'));

    expect(await screen.findByText(/unsaved changes that will be lost/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(tauriApi.deleteScriptFile).toHaveBeenCalledWith('col', 'utils.js'));
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'utils.js')).toBeNull(),
    );
  });
});
