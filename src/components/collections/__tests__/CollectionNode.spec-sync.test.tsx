import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import { collectAllTabs } from '@/lib/pane-utils';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { isCollectionTab } from '@/types/pane-types';
import { makeStatus } from '../spec-sync/__tests__/fixtures';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    getOpenApiSyncStatus: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

const summary: CollectionSummary = {
  uid: 'col-1',
  repositoryId: 'repo-1',
  name: 'my-collection',
  path: '/workspace/collections/my-collection',
  requestCount: 0,
};

function renderNode() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <CollectionNode
        summary={summary}
        filter=''
        summaries={[summary]}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe('CollectionNode OpenAPI sync entry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
  });

  it('asks for the sync status only once the row is hovered', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    const user = userEvent.setup();
    renderNode();
    const actions = await screen.findByLabelText('Actions for my-collection');
    expect(tauriApi.getOpenApiSyncStatus).not.toHaveBeenCalled();
    await user.hover(actions);
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
  });

  it('shows the entry when the status errored, like the section does', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue('state file unreadable');
    const user = userEvent.setup();
    renderNode();
    await user.hover(await screen.findByLabelText('Actions for my-collection'));
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    await user.click(screen.getByLabelText('Actions for my-collection'));
    expect(await screen.findByRole('menuitem', { name: /OpenAPI sync/ })).toBeInTheDocument();
  });

  it('shows OpenAPI sync for a collection with a source and opens the section', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    const user = userEvent.setup();
    renderNode();
    await user.click(await screen.findByLabelText('Actions for my-collection'));
    await user.click(await screen.findByRole('menuitem', { name: /OpenAPI sync/ }));
    await waitFor(() => {
      const tab = collectAllTabs(usePaneStore.getState().root).find(isCollectionTab);
      expect(tab?.activeSection).toBe('spec-sync');
    });
  });

  it('does not show OpenAPI sync without a source', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    const user = userEvent.setup();
    renderNode();
    await user.hover(await screen.findByLabelText('Actions for my-collection'));
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    // Let the settled null result reach the component before asserting absence.
    await act(async () => {
      await Promise.resolve();
    });
    await user.click(screen.getByLabelText('Actions for my-collection'));
    await screen.findByRole('menuitem', { name: /Overview/ });
    expect(screen.queryByRole('menuitem', { name: /OpenAPI sync/ })).toBeNull();
  });
});
