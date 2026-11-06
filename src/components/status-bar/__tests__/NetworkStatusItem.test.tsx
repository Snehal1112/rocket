import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkStatusItem } from '@/components/status-bar/NetworkStatusItem';
import { useNetworkStore } from '@/stores/network-store';
import { makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
    removeNetworkPaired: vi.fn(),
    addNetworkAddress: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const dana = makeNode({
  deviceId: 'P2',
  displayName: 'Dana Roy',
  shortId: 'DDDD-4444',
  group: 'paired',
  status: 'online',
});
const cleo = makeNode({
  deviceId: 'P1',
  displayName: 'Cleo Park',
  shortId: 'CCCC-3333',
  group: 'paired',
  status: 'online',
});

describe('NetworkStatusItem', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    useNetworkStore.setState({ status: makeStatus(), nodes: [dana, cleo] });
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkNodes.mockResolvedValue([dana, cleo]);
    mocks.api.removeNetworkPaired.mockResolvedValue(undefined);
  });

  it('shows the number of online paired nodes', () => {
    render(<NetworkStatusItem />);
    expect(screen.getByRole('button', { name: 'Team network, 2 online' })).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('shows no number when nobody is online', () => {
    useNetworkStore.setState({ nodes: [{ ...dana, status: 'offline' }] });
    render(<NetworkStatusItem />);
    expect(screen.getByRole('button', { name: 'Team network, 0 online' })).toBeInTheDocument();
    expect(screen.queryByText('0')).not.toBeInTheDocument();
  });

  it('says so when networking is off', () => {
    useNetworkStore.setState({ status: makeStatus({ state: 'off', offReason: 'No keychain.' }) });
    render(<NetworkStatusItem />);
    expect(screen.getByRole('button', { name: 'Team network, off' })).toBeInTheDocument();
  });

  it('opens the panel and reloads the status and the nodes', async () => {
    render(<NetworkStatusItem />);
    await userEvent.click(screen.getByRole('button', { name: 'Team network, 2 online' }));
    expect(await screen.findByRole('searchbox', { name: 'Search nodes' })).toBeInTheDocument();
    expect(useNetworkStore.getState().panelOpen).toBe(true);
    await waitFor(() => expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1));
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
  });

  it('opens when the store asks for it', async () => {
    useNetworkStore.setState({ panelOpen: true });
    render(<NetworkStatusItem />);
    expect(await screen.findByRole('searchbox', { name: 'Search nodes' })).toBeInTheDocument();
  });

  it('opens Add by address after the panel closes', async () => {
    render(<NetworkStatusItem />);
    await userEvent.click(screen.getByRole('button', { name: 'Team network, 2 online' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Add by address' }));
    expect(await screen.findByRole('dialog', { name: 'Add by address' })).toBeInTheDocument();
    expect(useNetworkStore.getState().panelOpen).toBe(false);
  });

  it('removes a paired node through the menu and the confirmation', async () => {
    render(<NetworkStatusItem />);
    await userEvent.click(screen.getByRole('button', { name: 'Team network, 2 online' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Actions for Dana Roy (DDDD-4444)' }),
    );
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove' }));
    const dialog = await screen.findByRole('alertdialog', { name: 'Remove Dana Roy?' });
    expect(useNetworkStore.getState().panelOpen).toBe(false);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(mocks.api.removeNetworkPaired).toHaveBeenCalledWith('P2'));
  });
});
