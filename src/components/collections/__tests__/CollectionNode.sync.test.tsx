import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink, makeNode } from '@/test/network-fixtures';

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
    startNetworkSync: vi.fn(),
    cancelNetworkSync: vi.fn(),
    unlinkNetworkLink: vi.fn(),
    listNetworkLinks: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

vi.mock('sonner', async (original) => {
  const actual = await original<typeof import('sonner')>();
  return {
    ...actual,
    toast: Object.assign(vi.fn(), actual.toast, {
      info: vi.fn(),
      error: vi.fn(),
      success: vi.fn(),
      dismiss: vi.fn(),
    }),
  };
});

const makeSummary = (name: string): CollectionSummary => ({
  uid: 'col-1',
  repositoryId: 'repo-1',
  name,
  path: `/workspace/collections/${name}`,
  requestCount: 0,
});

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

const ben = makeNode({ deviceId: 'DEV1', displayName: 'Ben Ortiz', group: 'paired', status: 'online' });
const caraOffline = makeNode({
  deviceId: 'DEV2',
  shortId: 'BBBB-2222',
  displayName: 'Cara Lund',
  group: 'paired',
  status: 'offline',
});

describe('CollectionNode sync entries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useNetworkStore.getState().reset();
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    vi.mocked(tauriApi.startNetworkSync).mockResolvedValue({ syncId: 's1' });
    vi.mocked(tauriApi.unlinkNetworkLink).mockResolvedValue(undefined);
    // The list is the truth, so a reload returns the links the test put in the store.
    vi.mocked(tauriApi.listNetworkLinks).mockImplementation(async () => useNetworkStore.getState().links);
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
  });

  describe.each<Menu>(['dropdown', 'context'])('in the %s menu', (menu) => {
    it('shows nothing for a collection without a link', async () => {
      useNetworkStore.setState({ nodes: [ben], links: [makeLink({ collectionName: 'other' })] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Sync with/ })).toBeNull();
      expect(screen.queryByRole('menuitem', { name: /Unlink/ })).toBeNull();
    });

    it('shows the status line, Sync with and Unlink for a linked, online peer', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection', state: 'changes_waiting' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.getByText('Changes waiting to sync with Ben Ortiz')).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink' })).toBeInTheDocument();
    });

    it('hides Sync with ... when the peer is offline', async () => {
      useNetworkStore.setState({
        nodes: [caraOffline],
        links: [
          makeLink({
            collectionName: 'my-collection',
            deviceId: 'DEV2',
            displayName: 'Cara Lund',
            shortId: 'BBBB-2222',
          }),
        ],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Sync with/ })).toBeNull();
    });

    it('shows Unlink and the status line when the peer is offline', async () => {
      useNetworkStore.setState({
        nodes: [caraOffline],
        links: [
          makeLink({
            collectionName: 'my-collection',
            deviceId: 'DEV2',
            displayName: 'Cara Lund',
            state: 'broken',
          }),
        ],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(
        screen.getByText('Link to Cara Lund is broken. Unlink and send the collection again.'),
      ).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink' })).toBeInTheDocument();
    });

    it('shows one group per linked peer', async () => {
      useNetworkStore.setState({
        nodes: [ben, { ...caraOffline, status: 'online' }],
        links: [
          makeLink({ collectionName: 'my-collection' }),
          makeLink({
            linkId: 'L2',
            deviceId: 'DEV2',
            displayName: 'Cara Lund',
            shortId: 'BBBB-2222',
            collectionName: 'my-collection',
          }),
        ],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Sync with Cara Lund' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink from Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink from Cara Lund' })).toBeInTheDocument();
    });

    it('starts the sync for that link', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' }));
      await waitFor(() => expect(tauriApi.startNetworkSync).toHaveBeenCalledWith('L1'));
    });

    it('reloads the links when the menu opens', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await waitFor(() => expect(tauriApi.listNetworkLinks).toHaveBeenCalled());
    });

    it('shows the refusal of a start as an error toast', async () => {
      vi.mocked(tauriApi.startNetworkSync).mockRejectedValue('Another sync is already running for this link.');
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' }));
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
    });

    it('asks before it unlinks', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: 'Unlink' }));
      expect(tauriApi.unlinkNetworkLink).not.toHaveBeenCalled();
      await user.click(await screen.findByRole('button', { name: 'Unlink' }));
      await waitFor(() => expect(tauriApi.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
    });
  });
});
