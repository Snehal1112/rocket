import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { makeStatus } from '@/components/collections/openapi-sync/__tests__/fixtures';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import type { CollectionTab } from '@/types/pane-types';
import { BreadcrumbBar } from '../BreadcrumbBar';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    listCollections: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('@/lib/queries/workspace-queries', () => ({
  useWorkspaces: () => ({ data: [] }),
  useSwitchWorkspace: () => ({ mutate: vi.fn() }),
}));

const tab: CollectionTab = {
  id: 't1',
  title: 'demo',
  isDirty: false,
  tabType: 'collection',
  collectionName: 'demo',
  activeSection: 'overview',
};

async function openSectionPicker() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: /Overview/ }));
}

describe('BreadcrumbBar section picker', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // jsdom has no scrollIntoView.
    Element.prototype.scrollIntoView = vi.fn();
  });

  it('the_section_picker_has_no_spec_sync_entry', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    renderWithQuery(<BreadcrumbBar tab={tab} />);
    await openSectionPicker();
    expect(await screen.findByText('Documentation')).toBeInTheDocument();
    expect(screen.queryByText('OpenAPI sync')).toBeNull();
    expect(screen.queryByText('OpenAPI sync')).toBeNull();
  });
});
