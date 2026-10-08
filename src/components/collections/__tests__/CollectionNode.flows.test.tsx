import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    listFlows: vi.fn(),
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

function renderNode(filter = '') {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <CollectionNode
        summary={summary}
        filter={filter}
        summaries={[summary]}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe('CollectionNode flows group', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
    vi.mocked(tauriApi.listFlows).mockReset().mockResolvedValue(['Login', 'Sync']);
    // Opens the node through the pane store's active collection, as the sibling test does.
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('lists the collection flows under a Flows heading once the node is open', async () => {
    renderNode();
    expect(await screen.findByLabelText('Open flow Login')).toBeInTheDocument();
    expect(screen.getByLabelText('Open flow Sync')).toBeInTheDocument();
    expect(screen.getByText('Flows')).toBeInTheDocument();
    expect(tauriApi.listFlows).toHaveBeenCalledWith('my-collection');
  });

  it('applies the sidebar filter to flow names', async () => {
    renderNode('syn');
    expect(await screen.findByLabelText('Open flow Sync')).toBeInTheDocument();
    expect(screen.queryByLabelText('Open flow Login')).not.toBeInTheDocument();
  });

  it('shows no Flows heading when the collection has no flows', async () => {
    vi.mocked(tauriApi.listFlows).mockResolvedValue([]);
    renderNode();
    await vi.waitFor(() => expect(tauriApi.listFlows).toHaveBeenCalled());
    expect(screen.queryByText('Flows')).not.toBeInTheDocument();
  });
});
