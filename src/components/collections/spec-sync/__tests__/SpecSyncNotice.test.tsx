import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncNotice } from '../SpecSyncNotice';
import { makeStatus } from './fixtures';

const openCollectionTab = vi.fn();

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: (sel: (s: { openCollectionTab: typeof openCollectionTab }) => unknown) =>
    sel({ openCollectionTab }),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    onOpenApiSyncChecked: vi.fn(() => Promise.resolve(() => undefined)),
  };
});

describe('SpecSyncNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders nothing for a collection without a spec source', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    const { container } = renderWithQuery(<SpecSyncNotice collection='c' />);
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('c'));
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when the last check found no change', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(
      makeStatus({ lastStatus: { state: 'unchanged' }, hasPending: false }),
    );
    const { container } = renderWithQuery(<SpecSyncNotice collection='c' />);
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when changed but nothing is pending', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasPending: false }));
    const { container } = renderWithQuery(<SpecSyncNotice collection='c' />);
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('opens the Spec sync tab on click', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus());
    renderWithQuery(<SpecSyncNotice collection='c' />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Review changes' }));
    expect(openCollectionTab).toHaveBeenCalledWith('c', 'spec-sync');
  });
});
