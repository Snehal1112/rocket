import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NetworkPanel } from '@/components/network/NetworkPanel';
import { PAIRING_FAILED_TOAST_ID } from '@/lib/network-utils';
import { useNetworkStore } from '@/stores/network-store';
import { makeIncoming, makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
  },
}));

const workspace = vi.hoisted(() => ({ names: null as string[] | null }));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('@/hooks/useWorkspaceCollectionNames', () => ({
  useWorkspaceCollectionNames: () => workspace.names,
}));
vi.mock('sonner', () => ({ toast: mocks.toast }));

const cleo = makeNode({
  deviceId: 'P1',
  displayName: 'Cleo Park',
  initials: 'CP',
  shortId: 'CCCC-3333',
  group: 'paired',
  status: 'offline',
});
const dana = makeNode({
  deviceId: 'P2',
  displayName: 'Dana Roy',
  initials: 'DR',
  shortId: 'DDDD-4444',
  group: 'paired',
  status: 'online',
});
const ben = makeNode({ deviceId: 'N1', displayName: 'Ben Ortiz', shortId: 'AAAA-1111' });

function setup(nodes = [cleo, dana, ben], status = makeStatus()) {
  useNetworkStore.setState({ status, nodes });
  const onAddAddress = vi.fn();
  const onRemove = vi.fn();
  render(<NetworkPanel onAddAddress={onAddAddress} onRemove={onRemove} />);
  return { onAddAddress, onRemove };
}

const search = () => screen.getByRole('searchbox', { name: 'Search nodes' });

describe('NetworkPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('shows a loading line until the status arrives', () => {
    render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} />);
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('shows the load error instead of loading forever when the status fails', () => {
    useNetworkStore.setState({ status: null, error: 'backend down' });
    render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent('backend down');
    expect(screen.queryByText('Loading...')).not.toBeInTheDocument();
  });

  it('lists paired nodes with online first and offline greyed, then nearby nodes', () => {
    setup();
    const pairedItems = within(screen.getByRole('list', { name: 'Paired' })).getAllByRole(
      'listitem',
    );
    expect(pairedItems[0]).toHaveTextContent('Dana Roy');
    expect(pairedItems[0]).toHaveTextContent('Online');
    expect(pairedItems[1]).toHaveTextContent('Cleo Park');
    expect(pairedItems[1]).toHaveTextContent('Offline');
    expect(pairedItems[1]).toHaveClass('opacity-60');
    expect(pairedItems[0]).not.toHaveClass('opacity-60');
    const nearby = within(screen.getByRole('list', { name: 'Nearby' }));
    expect(nearby.getByText('Ben Ortiz')).toBeInTheDocument();
    expect(nearby.getByText('AAAA-1111')).toBeInTheDocument();
    expect(nearby.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' })).toBeEnabled();
  });

  it('shows the initials circle and the short id of each row', () => {
    setup();
    expect(screen.getByText('DR')).toBeInTheDocument();
    expect(screen.getByText('DDDD-4444')).toBeInTheDocument();
  });

  it('filters by name, initials and short id', async () => {
    setup();
    await userEvent.type(search(), 'dana');
    expect(screen.getByText('Dana Roy')).toBeInTheDocument();
    expect(screen.queryByText('Cleo Park')).not.toBeInTheDocument();
    expect(screen.queryByText('Ben Ortiz')).not.toBeInTheDocument();

    await userEvent.clear(search());
    await userEvent.type(search(), 'cccc3333');
    expect(screen.getByText('Cleo Park')).toBeInTheDocument();
    expect(screen.queryByText('Dana Roy')).not.toBeInTheDocument();

    await userEvent.clear(search());
    await userEvent.type(search(), 'dr');
    expect(screen.getByText('Dana Roy')).toBeInTheDocument();
    expect(screen.queryByText('Ben Ortiz')).not.toBeInTheDocument();
  });

  it('says nothing matches, keeps the requests and hides the empty state', async () => {
    setup();
    useNetworkStore.getState().addIncoming(makeIncoming({ displayName: 'Eli Stone' }));
    await userEvent.type(search(), 'zzzqq');
    expect(screen.getByText('No nodes match "zzzqq".')).toBeInTheDocument();
    expect(screen.queryByText('No Rocket nodes found on this network.')).not.toBeInTheDocument();
    expect(screen.getByText('Eli Stone')).toBeInTheDocument();
  });

  it('tells two nodes with the same name apart by short id', async () => {
    const twin = makeNode({ deviceId: 'N2', displayName: 'Ben Ortiz', shortId: 'BBBB-2222' });
    setup([ben, twin]);
    expect(screen.getByText('AAAA-1111')).toBeInTheDocument();
    expect(screen.getByText('BBBB-2222')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /^Pair with Ben Ortiz/ })).toHaveLength(2);
    await userEvent.type(search(), 'bbbb2222');
    expect(screen.getAllByRole('button', { name: /^Pair with Ben Ortiz/ })).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Pair with Ben Ortiz (BBBB-2222)' })).toBeEnabled();
  });

  it('disables Pair and shows a chip for a node that needs an update', () => {
    setup([makeNode({ displayName: 'Old Mac', shortId: 'OLDM-0001', status: 'needs_update' })]);
    expect(screen.getByText('Needs update')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pair with Old Mac (OLDM-0001)' })).toBeDisabled();
  });

  it('shows the Pairing chip while a pairing with the node is open', () => {
    setup([makeNode({ displayName: 'New Mac', shortId: 'NEWM-0001', status: 'pairing' })]);
    expect(screen.getByText('Pairing…')).toBeInTheDocument();
  });

  it('shows the empty state with the VPN hint and one Add by address button', async () => {
    const { onAddAddress } = setup([]);
    expect(screen.getByText('No Rocket nodes found on this network.')).toBeInTheDocument();
    expect(screen.getByText('VPNs and some Wi-Fi networks block discovery.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Add by address' }));
    expect(onAddAddress).toHaveBeenCalledTimes(1);
  });

  it('says discovery is unavailable instead of the VPN hint', () => {
    setup([], makeStatus({ discoveryAvailable: false }));
    expect(screen.getByText("Automatic discovery isn't available here.")).toBeInTheDocument();
    expect(
      screen.queryByText('VPNs and some Wi-Fi networks block discovery.'),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add by address' })).toBeInTheDocument();
  });

  it('warns when others cannot connect', () => {
    setup([dana], makeStatus({ listenerAvailable: false }));
    expect(
      screen.getByText("Others can't connect to you. Allow Rocket in your firewall settings."),
    ).toBeInTheDocument();
    expect(screen.getByText('Dana Roy')).toBeInTheDocument();
  });

  it('shows why networking is off and nothing else', () => {
    setup([dana], makeStatus({ state: 'off', offReason: 'The keychain is not available.' }));
    expect(screen.getByText('The keychain is not available.')).toBeInTheDocument();
    expect(screen.queryByRole('searchbox')).not.toBeInTheDocument();
    expect(screen.queryByText('Dana Roy')).not.toBeInTheDocument();
  });

  it('lists open requests at the top and reviews one', async () => {
    setup([dana]);
    useNetworkStore.setState({ panelOpen: true });
    useNetworkStore.getState().addIncoming(makeIncoming({ displayName: 'Eli Stone' }));
    const requests = within(await screen.findByRole('list', { name: 'Requests' }));
    expect(requests.getByText('Eli Stone')).toBeInTheDocument();
    expect(requests.getByText('ZZZZ-9999 · 482 913')).toBeInTheDocument();
    await userEvent.click(
      requests.getByRole('button', { name: 'Review request from Eli Stone (ZZZZ-9999)' }),
    );
    expect(useNetworkStore.getState().dialogPairingId).toBe('p1');
    expect(useNetworkStore.getState().panelOpen).toBe(false);
  });

  it('starts a pairing from the Pair button', async () => {
    setup([ben]);
    mocks.api.requestNetworkPairing.mockResolvedValue({ pairingId: 'p7', code: '482913' });
    await userEvent.click(screen.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' }));
    await waitFor(() => expect(useNetworkStore.getState().dialogPairingId).toBe('p7'));
    expect(mocks.api.requestNetworkPairing).toHaveBeenCalledWith('N1');
    expect(useNetworkStore.getState().activePairing?.code).toBe('482913');
  });

  it('shows the backend error when a pairing cannot start', async () => {
    setup([ben]);
    mocks.api.requestNetworkPairing.mockRejectedValue(
      "Invalid input: You've reached the limit of 200 paired devices. Remove one to pair.",
    );
    await userEvent.click(screen.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' }));
    await waitFor(() =>
      expect(mocks.toast.error).toHaveBeenCalledWith(
        "You've reached the limit of 200 paired devices. Remove one to pair.",
        { id: PAIRING_FAILED_TOAST_ID },
      ),
    );
    expect(useNetworkStore.getState().dialogPairingId).toBeNull();
    expect(screen.getByRole('button', { name: 'Pair with Ben Ortiz (AAAA-1111)' })).toBeEnabled();
  });

  it('asks the host to remove a paired node from its menu', async () => {
    const { onRemove } = setup([dana]);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Dana Roy (DDDD-4444)' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove' }));
    expect(onRemove).toHaveBeenCalledWith(expect.objectContaining({ deviceId: 'P2' }));
  });
});
