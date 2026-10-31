import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionsSidebar } from '@/components/layout/CollectionsSidebar';
import { WorkspaceSection } from '@/components/layout/WorkspaceSection';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { useSettingsStore } from '@/stores/settings-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listWorkspaces: vi.fn().mockResolvedValue([]),
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});
vi.mock('@/components/history/HistoryPanel', () => ({
  HistoryPanel: () => <div data-testid='history-panel-marker' />,
}));

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  setQueryClient(queryClient);
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

const workspace = { id: 'ws1', repositoryId: 'r1', name: 'WS', path: '/w', pinned: false };

describe('workspace clicks close Settings', () => {
  beforeEach(() => {
    useSettingsStore.getState().reset();
    useSettingsStore.getState().openSettings();
    vi.restoreAllMocks();
  });

  it('closes Settings on the sidebar workspace home click', async () => {
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([workspace]);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws1' });
    render(<CollectionsSidebar />, { wrapper });
    fireEvent.click(await screen.findByRole('button', { name: 'WS' }));
    await waitFor(() => expect(useSettingsStore.getState().open).toBe(false));
  });

  it('keeps a locked Settings open on the workspace home click', async () => {
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([workspace]);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws1' });
    useSettingsStore.getState().setLocked(true);
    render(<CollectionsSidebar />, { wrapper });
    fireEvent.click(await screen.findByRole('button', { name: 'WS' }));
    expect(useSettingsStore.getState().open).toBe(true);
  });

  it('closes Settings on a workspace name click in the section', () => {
    render(
      <WorkspaceSection workspace={workspace} collectionCount={0}>
        <div />
      </WorkspaceSection>,
      { wrapper },
    );
    fireEvent.click(screen.getByRole('button', { name: 'WS' }));
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('closes Settings on the audit log click in the section', () => {
    render(
      <WorkspaceSection workspace={workspace} collectionCount={0}>
        <div />
      </WorkspaceSection>,
      { wrapper },
    );
    fireEvent.click(screen.getByRole('button', { name: 'Open audit log' }));
    expect(useSettingsStore.getState().open).toBe(false);
  });
});
