import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { renderWithQuery } from '@/test/trust-fixtures';
import { EditorGroup } from '../EditorGroup';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/components/collections/openapi-sync/OpenApiSyncTab', () => ({
  OpenApiSyncTab: () => <div data-testid='openapi-sync-tab' />,
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
});
