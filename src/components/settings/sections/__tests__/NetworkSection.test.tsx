import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkSection } from '@/components/settings/sections/NetworkSection';
import type { NetworkStatus } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { makeStatus } from '@/test/network-fixtures';

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
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const mallory = {
  deviceId: 'B1',
  shortId: 'BBBB-0001',
  name: 'Mallory',
  blockedAt: '2026-11-06T10:00:00Z',
};
const saved = { host: '10.0.0.5', port: 47821, addedAt: '2026-11-06T10:00:00Z' };

function setup(status: NetworkStatus = makeStatus()) {
  mocks.api.getNetworkStatus.mockResolvedValue(status);
  useNetworkStore.setState({ status });
  render(<NetworkSection />);
}

describe('NetworkSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.listNetworkBlocked.mockResolvedValue([mallory]);
    mocks.api.listNetworkAddresses.mockResolvedValue([saved]);
    mocks.api.listNetworkNodes.mockResolvedValue([]);
    mocks.api.setNetworkVisibility.mockResolvedValue(undefined);
    mocks.api.setNetworkProfile.mockResolvedValue(undefined);
    mocks.api.unblockNetworkDevice.mockResolvedValue(undefined);
    mocks.api.removeNetworkAddress.mockResolvedValue(undefined);
  });

  it('shows the load error instead of loading forever when the status fails', () => {
    useNetworkStore.setState({ status: null, error: 'backend down' });
    render(<NetworkSection />);
    expect(screen.getByRole('alert')).toHaveTextContent('backend down');
    expect(screen.queryByText('Loading...')).not.toBeInTheDocument();
  });

  it('shows the port in use and the short device id as read-only text', async () => {
    setup();
    expect(screen.getByRole('heading', { name: 'Team network' })).toBeInTheDocument();
    expect(screen.getByText('47821')).toBeInTheDocument();
    expect(screen.getByText('K7QF-2M9X')).toBeInTheDocument();
  });

  it('says there is no port when the node is not listening', () => {
    setup(makeStatus({ port: null, deviceShortId: null }));
    expect(screen.getByText('Not listening')).toBeInTheDocument();
  });

  it('shows why networking is off', () => {
    setup(makeStatus({ state: 'off', offReason: 'The keychain is not available.' }));
    expect(screen.getByRole('alert')).toHaveTextContent('The keychain is not available.');
  });

  it('lists the warnings', () => {
    setup(makeStatus({ warnings: ['blocked.yml was damaged and was reset.'] }));
    expect(screen.getByText('blocked.yml was damaged and was reset.')).toBeInTheDocument();
  });

  it('hides the node with the switch and reloads the status', async () => {
    mocks.api.getNetworkStatus
      .mockResolvedValueOnce(makeStatus())
      .mockResolvedValueOnce(makeStatus({ hidden: true }));
    useNetworkStore.setState({ status: makeStatus() });
    render(<NetworkSection />);
    const toggle = screen.getByRole('switch', { name: 'Hide my node' });
    expect(toggle).not.toBeChecked();
    await userEvent.click(toggle);
    await waitFor(() => expect(mocks.api.setNetworkVisibility).toHaveBeenCalledWith(true));
    await waitFor(() => expect(screen.getByRole('switch', { name: 'Hide my node' })).toBeChecked());
  });

  it('shows the error and keeps the switch when hiding fails', async () => {
    mocks.api.setNetworkVisibility.mockRejectedValue('Internal error: settings locked');
    setup();
    await userEvent.click(screen.getByRole('switch', { name: 'Hide my node' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('settings locked'));
    expect(screen.getByRole('switch', { name: 'Hide my node' })).not.toBeChecked();
  });

  it('saves a new display name', async () => {
    setup();
    const field = screen.getByLabelText('Display name');
    await userEvent.clear(field);
    await userEvent.type(field, '  Ana L.  ');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(mocks.api.setNetworkProfile).toHaveBeenCalledWith('Ana L.'));
    await waitFor(() => expect(mocks.toast.success).toHaveBeenCalledWith('Name saved.'));
  });

  it('keeps Save off while the name is unchanged', () => {
    setup();
    expect(screen.getByLabelText('Display name')).toHaveValue('Ana Lima');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('refuses an empty name and a name over 40 characters', async () => {
    setup();
    const field = screen.getByLabelText('Display name');
    await userEvent.clear(field);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByText('Enter a name.')).toBeInTheDocument();
    await userEvent.type(field, 'a'.repeat(41));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByText('Use 40 characters or fewer.')).toBeInTheDocument();
    expect(mocks.api.setNetworkProfile).not.toHaveBeenCalled();
  });

  it('lists blocked devices and unblocks one', async () => {
    setup();
    const row = (await screen.findByText('Mallory')).closest('li');
    if (!row) throw new Error('no blocked row');
    expect(within(row).getByText('BBBB-0001')).toBeInTheDocument();
    await userEvent.click(within(row).getByRole('button', { name: 'Unblock Mallory' }));
    await waitFor(() => expect(mocks.api.unblockNetworkDevice).toHaveBeenCalledWith('B1'));
    await waitFor(() => expect(screen.queryByText('Mallory')).not.toBeInTheDocument());
    expect(screen.getByText('No blocked devices.')).toBeInTheDocument();
  });

  it('lists saved addresses and removes one', async () => {
    setup();
    expect(await screen.findByText('10.0.0.5:47821')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove 10.0.0.5:47821' }));
    await waitFor(() =>
      expect(mocks.api.removeNetworkAddress).toHaveBeenCalledWith('10.0.0.5', 47821),
    );
    await waitFor(() => expect(screen.queryByText('10.0.0.5:47821')).not.toBeInTheDocument());
    expect(screen.getByText('No saved addresses.')).toBeInTheDocument();
    expect(mocks.api.listNetworkNodes).toHaveBeenCalled();
  });

  it('shows the error when the lists cannot load', async () => {
    mocks.api.listNetworkBlocked.mockRejectedValue('Internal error: repo down');
    setup();
    expect(await screen.findByText('repo down')).toBeInTheDocument();
  });
});
