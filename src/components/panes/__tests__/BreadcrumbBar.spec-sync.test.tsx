import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { makeStatus } from '@/components/collections/spec-sync/__tests__/fixtures';
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

/** Waits for the status call, then lets its result reach the component. */
async function settled() {
  await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('demo'));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 20));
  });
}

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

  it('omits Spec sync when the collection has no spec source', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    renderWithQuery(<BreadcrumbBar tab={tab} />);
    await settled();
    await openSectionPicker();
    expect(await screen.findByText('Documentation')).toBeInTheDocument();
    expect(screen.queryByText('Spec sync')).toBeNull();
  });

  it('lists Spec sync when the collection has a spec source', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    renderWithQuery(<BreadcrumbBar tab={tab} />);
    await settled();
    await openSectionPicker();
    expect(await screen.findByText('Spec sync')).toBeInTheDocument();
  });

  it('lists Spec sync when the status errored', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockRejectedValue('bad yaml');
    renderWithQuery(<BreadcrumbBar tab={tab} />);
    await settled();
    await openSectionPicker();
    expect(await screen.findByText('Spec sync')).toBeInTheDocument();
  });
});
