import { act, screen, waitFor } from '@testing-library/react';
import { useEffect } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { renderWithQuery } from '@/test/trust-fixtures';
import { EditorGroup } from '../EditorGroup';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
const mounts = vi.hoisted(() => ({ count: 0 }));
vi.mock('@/components/collections/openapi-sync/OpenApiSyncTab', () => ({
  OpenApiSyncTab: ({ collection }: { collection: string }) => {
    useEffect(() => {
      mounts.count += 1;
    }, []);
    return <div data-testid='openapi-sync-tab'>{collection}</div>;
  },
}));
vi.mock('@/components/collections/CollectionOverviewTab', () => ({
  CollectionOverviewTab: () => <div data-testid='collection-overview' />,
}));

function Group() {
  const root = usePaneStore((s) => s.root);
  if (root.type !== 'leaf') return null;
  return <EditorGroup node={root} />;
}

describe('EditorGroup OpenAPI sync tab', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('renders_the_openapi_sync_tab_instead_of_the_collection_overview', async () => {
    usePaneStore.setState({ switchCollection: vi.fn() });
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    renderWithQuery(<Group />);
    expect(await screen.findByTestId('openapi-sync-tab')).toBeInTheDocument();
    expect(screen.queryByTestId('collection-overview')).toBeNull();
  });

  it('remounts_the_tab_when_another_collection_sync_tab_becomes_active', async () => {
    mounts.count = 0;
    usePaneStore.setState({ switchCollection: vi.fn() });
    usePaneStore.getState().openOpenApiSyncTab('Orders');
    renderWithQuery(<Group />);
    expect(await screen.findByTestId('openapi-sync-tab')).toHaveTextContent('Orders');
    act(() => usePaneStore.getState().openOpenApiSyncTab('Billing'));
    await waitFor(() => expect(screen.getByTestId('openapi-sync-tab')).toHaveTextContent('Billing'));
    expect(mounts.count).toBe(2);
  });
});
