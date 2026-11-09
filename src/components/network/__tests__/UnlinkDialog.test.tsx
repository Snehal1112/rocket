import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { UnlinkDialog } from '@/components/network/UnlinkDialog';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink, makeSyncReview } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    unlinkNetworkLink: vi.fn(),
    listNetworkLinks: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

describe('UnlinkDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.unlinkNetworkLink.mockResolvedValue(undefined);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
  });

  it('stays closed without a link', () => {
    render(<UnlinkDialog link={null} onClose={vi.fn()} />);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('says what an unlink does and does not do', () => {
    render(<UnlinkDialog link={makeLink({ collectionName: 'Orders "v2"' })} onClose={vi.fn()} />);
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Unlink Orders "v2" from Ben Ortiz?');
    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      'Your collection and its files stay as they are. Ben Ortiz (AAAA-1111) learns of it at the next sync.',
    );
  });

  it('unlinks and closes', async () => {
    const onClose = vi.fn();
    render(<UnlinkDialog link={makeLink()} onClose={onClose} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('shows a refused unlink as a toast', async () => {
    mocks.api.unlinkNetworkLink.mockRejectedValue('Could not unlink.');
    render(<UnlinkDialog link={makeLink()} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalled());
  });

  it('marks a pending review of the link as cancelled here before it unlinks', async () => {
    useNetworkStore.getState().addSyncReview(makeSyncReview({ linkId: 'L1' }));
    render(<UnlinkDialog link={makeLink()} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
    expect(useNetworkStore.getState().wasSyncCancelledHere('s1')).toBe(true);
  });

  it('forgets that mark when the unlink is refused', async () => {
    mocks.api.unlinkNetworkLink.mockRejectedValue('Could not unlink.');
    useNetworkStore.getState().addSyncReview(makeSyncReview({ linkId: 'L1' }));
    render(<UnlinkDialog link={makeLink()} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalled());
    expect(useNetworkStore.getState().wasSyncCancelledHere('s1')).toBe(false);
  });

  it('does nothing on Cancel', async () => {
    const onClose = vi.fn();
    render(<UnlinkDialog link={makeLink()} onClose={onClose} />);
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mocks.api.unlinkNetworkLink).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('marks a sync started here for the link as cancelled here', async () => {
    useNetworkStore
      .getState()
      .trackStartedSync('s9', { displayName: 'Ben', collectionName: 'Orders', linkId: 'L1' });
    render(<UnlinkDialog link={makeLink()} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
    expect(useNetworkStore.getState().wasSyncCancelledHere('s9')).toBe(true);
  });
});
