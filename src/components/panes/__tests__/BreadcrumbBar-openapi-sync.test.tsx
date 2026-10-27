import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import type { OpenApiSyncTab } from '@/types/pane-types';
import { BreadcrumbBar } from '../BreadcrumbBar';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn().mockResolvedValue(null),
    listCollections: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('@/lib/queries/workspace-queries', () => ({
  useWorkspaces: () => ({ data: [] }),
  useSwitchWorkspace: () => ({ mutate: vi.fn() }),
}));

const tab: OpenApiSyncTab = {
  id: 'openapi-sync:Orders',
  title: 'OpenAPI sync — Orders',
  isDirty: false,
  tabType: 'openapi-sync',
  collectionName: 'Orders',
};

describe('BreadcrumbBar OpenAPI sync tab', () => {
  it('shows_collection_then_openapi_sync', async () => {
    renderWithQuery(<BreadcrumbBar tab={tab} />);
    expect(await screen.findByText('Orders')).toBeInTheDocument();
    expect(screen.getByText('OpenAPI sync')).toBeInTheDocument();
  });
});
