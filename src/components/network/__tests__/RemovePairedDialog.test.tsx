import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RemovePairedDialog } from '@/components/network/RemovePairedDialog';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    removeNetworkPaired: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const dana = makeNode({ deviceId: 'P2', displayName: 'Dana Roy', shortId: 'DDDD-4444' });

describe('RemovePairedDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.removeNetworkPaired.mockResolvedValue(undefined);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
  });

  it('renders nothing while there is no node', () => {
    render(<RemovePairedDialog node={null} onClose={vi.fn()} />);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('names the node and removes it on confirm', async () => {
    const onClose = vi.fn();
    render(<RemovePairedDialog node={dana} onClose={onClose} />);
    const dialog = await screen.findByRole('alertdialog', { name: 'Remove Dana Roy?' });
    expect(dialog).toHaveTextContent('DDDD-4444');
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(mocks.api.removeNetworkPaired).toHaveBeenCalledWith('P2'));
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalled());
    expect(onClose).toHaveBeenCalled();
  });

  it('does not remove on Cancel', async () => {
    const onClose = vi.fn();
    render(<RemovePairedDialog node={dana} onClose={onClose} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(mocks.api.removeNetworkPaired).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('shows the backend error in a toast', async () => {
    mocks.api.removeNetworkPaired.mockRejectedValue('Internal error: disk full');
    render(<RemovePairedDialog node={dana} onClose={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('disk full'));
  });
});
