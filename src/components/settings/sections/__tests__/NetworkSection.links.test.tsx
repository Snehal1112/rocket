import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkSection } from '@/components/settings/sections/NetworkSection';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
    setNetworkVisibility: vi.fn(),
    setNetworkProfile: vi.fn(),
    listNetworkBlocked: vi.fn(),
    listNetworkAddresses: vi.fn(),
    unblockNetworkDevice: vi.fn(),
    removeNetworkAddress: vi.fn(),
    listNetworkLinks: vi.fn(),
    unlinkNetworkLink: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

describe('NetworkSection links', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkBlocked.mockResolvedValue([]);
    mocks.api.listNetworkAddresses.mockResolvedValue([]);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
  });

  it('lists the linked collections and unlinks after a confirmation', async () => {
    mocks.api.unlinkNetworkLink.mockResolvedValue(undefined);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    useNetworkStore.setState({ status: makeStatus(), links: [makeLink()] });
    render(<NetworkSection />);
    expect(screen.getByRole('heading', { name: 'Linked collections' })).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Unlink Orders from Ben Ortiz (AAAA-1111)' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
  });

  it('says there are no linked collections', () => {
    useNetworkStore.setState({ status: makeStatus(), links: [] });
    render(<NetworkSection />);
    expect(screen.getByText('No linked collections.')).toBeInTheDocument();
  });
});
