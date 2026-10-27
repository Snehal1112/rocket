import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { makeStatus } from '../openapi-sync/__tests__/fixtures';

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
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
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

  it('the_menu_entry_opens_the_openapi_sync_tab', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    const openTab = vi.spyOn(usePaneStore.getState(), 'openOpenApiSyncTab');
    const openSection = vi.spyOn(usePaneStore.getState(), 'openCollectionTab');
    const user = userEvent.setup();
    renderNode();
    await user.click(await screen.findByLabelText('Actions for my-collection'));
    await user.click(await screen.findByRole('menuitem', { name: /OpenAPI sync/ }));
    await waitFor(() => expect(openTab).toHaveBeenCalledWith('my-collection'));
    expect(openSection).not.toHaveBeenCalled();
  });

  it('the_entry_is_hidden_without_a_source', async () => {
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

  describe('waiting count badge', () => {
    const badge = (n: number) =>
      screen.findByRole('button', {
        name: `Open OpenAPI sync, ${n} ${n === 1 ? 'update' : 'updates'}`,
      });

    it('uses_the_singular_for_one_update', async () => {
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
        { collection: 'my-collection', waiting: 1 },
      ]);
      const user = userEvent.setup();
      renderNode();
      const el = await badge(1);
      expect(el).toHaveAccessibleName('Open OpenAPI sync, 1 update');
      await user.hover(el);
      expect(await screen.findAllByText('1 OpenAPI spec update')).not.toHaveLength(0);
    });

    it('double_clicking_the_badge_does_not_open_the_collection_tab', async () => {
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
        { collection: 'my-collection', waiting: 2 },
      ]);
      const openTab = vi.spyOn(usePaneStore.getState(), 'openTab');
      const user = userEvent.setup();
      renderNode();
      await user.dblClick(await badge(2));
      expect(openTab).not.toHaveBeenCalledWith(expect.objectContaining({ tabType: 'collection' }));
    });

    it('shows_the_waiting_count_badge', async () => {
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
        { collection: 'my-collection', waiting: 5 },
      ]);
      renderNode();
      expect(await badge(5)).toHaveTextContent('5');
    });

    it('hides_the_badge_at_zero_and_for_other_collections', async () => {
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
        { collection: 'my-collection', waiting: 0 },
        { collection: 'Orders', waiting: 4 },
      ]);
      renderNode();
      await waitFor(() => expect(tauriApi.getOpenApiSyncBadges).toHaveBeenCalled());
      await act(async () => {
        await Promise.resolve();
      });
      expect(screen.queryByRole('button', { name: /Open OpenAPI sync/ })).toBeNull();
    });

    it('clicking_the_badge_opens_the_tab_and_keeps_the_row_closed', async () => {
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
        { collection: 'my-collection', waiting: 2 },
      ]);
      const openTab = vi.spyOn(usePaneStore.getState(), 'openOpenApiSyncTab');
      const user = userEvent.setup();
      renderNode();
      const el = await badge(2);
      const row = el.closest('button');
      const before = row?.getAttribute('aria-expanded');
      await user.click(el);
      expect(openTab).toHaveBeenCalledWith('my-collection');
      expect(row?.getAttribute('aria-expanded')).toBe(before);
    });

    it('the_badge_is_keyboard_reachable', async () => {
      vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([
        { collection: 'my-collection', waiting: 3 },
      ]);
      const openTab = vi.spyOn(usePaneStore.getState(), 'openOpenApiSyncTab');
      const user = userEvent.setup();
      renderNode();
      (await badge(3)).focus();
      await user.keyboard('{Enter}');
      expect(openTab).toHaveBeenCalledWith('my-collection');
    });
  });
});
