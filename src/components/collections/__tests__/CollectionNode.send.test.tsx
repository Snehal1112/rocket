import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode } from '@/test/network-fixtures';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncBadges: vi.fn(),
    sendNetworkCollection: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

const makeSummary = (name: string): CollectionSummary => ({
  uid: 'col-1',
  repositoryId: 'repo-1',
  name,
  path: `/workspace/collections/${name}`,
  requestCount: 0,
});

const online = makeNode({
  deviceId: 'DEV1',
  displayName: 'Ben Ortiz',
  group: 'paired',
  status: 'online',
});
const offline = makeNode({ deviceId: 'DEV2', group: 'paired', status: 'offline' });

function renderNode(summary: CollectionSummary) {
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

type Menu = 'dropdown' | 'context';

// Opens the `...` menu with the pointer, or the right-click menu on the row.
async function openMenu(menu: Menu, name: string, user: ReturnType<typeof userEvent.setup>) {
  if (menu === 'dropdown') {
    await user.hover(await screen.findByLabelText(`Actions for ${name}`));
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    await act(async () => {
      await Promise.resolve();
    });
    await user.click(screen.getByLabelText(`Actions for ${name}`));
  } else {
    fireEvent.contextMenu(await screen.findByText(name));
  }
  await screen.findByRole('menuitem', { name: /Overview/ });
}

describe('CollectionNode Send to... entry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useNetworkStore.getState().reset();
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    vi.mocked(tauriApi.sendNetworkCollection).mockResolvedValue({ transferId: 't1', notes: [] });
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
  });

  describe.each<Menu>(['dropdown', 'context'])('in the %s menu', (menu) => {
    it('is hidden with zero nodes', async () => {
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Send to/ })).toBeNull();
    });

    it('is hidden when the only paired node is offline', async () => {
      useNetworkStore.setState({ nodes: [offline] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Send to/ })).toBeNull();
    });

    it('is visible with one online paired node', async () => {
      useNetworkStore.setState({ nodes: [online, offline] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.getByRole('menuitem', { name: /Send to/ })).toBeInTheDocument();
    });

    it('opens the dialog and sends the collection', async () => {
      useNetworkStore.setState({ nodes: [online] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: /Send to/ }));
      expect(await screen.findByRole('dialog', { name: 'Send my-collection' })).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Send' }));
      await waitFor(() =>
        expect(tauriApi.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'my-collection', false),
      );
    });

    it('passes a collection name with spaces and unicode unchanged', async () => {
      const name = 'Orders v2 – café "x"';
      useNetworkStore.setState({ nodes: [online] });
      const user = userEvent.setup();
      renderNode(makeSummary(name));
      await openMenu(menu, name, user);
      await user.click(screen.getByRole('menuitem', { name: /Send to/ }));
      await user.click(await screen.findByRole('button', { name: 'Send' }));
      await waitFor(() =>
        expect(tauriApi.sendNetworkCollection).toHaveBeenCalledWith('DEV1', name, false),
      );
    });
  });

  it('keeps the dialog mounted when the last node goes offline', async () => {
    useNetworkStore.setState({ nodes: [online] });
    const user = userEvent.setup();
    renderNode(makeSummary('my-collection'));
    await openMenu('dropdown', 'my-collection', user);
    await user.click(screen.getByRole('menuitem', { name: /Send to/ }));
    await screen.findByRole('dialog', { name: 'Send my-collection' });
    act(() => useNetworkStore.setState({ nodes: [{ ...online, status: 'offline' }] }));
    expect(await screen.findByText('No paired node is online')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });
});
