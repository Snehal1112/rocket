import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ContractCheckAction } from '@/components/contracts/ContractCheckAction';
import type { NetworkNode, NetworkStatus } from '@/lib/tauri-api';
import { contractCheckRunKey, useContractCheckRunsStore } from '@/stores/contract-check-runs-store';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({ api: { startContractCheck: vi.fn() } }));
vi.mock('@/lib/tauri-api', () => mocks.api);

const store = () => useNetworkStore.getState();

function node(over: Partial<NetworkNode>): NetworkNode {
  return {
    deviceId: 'd1',
    shortId: 'AAAA-1111',
    displayName: 'Ada',
    initials: 'A',
    group: 'paired',
    status: 'online',
    source: 'mdns',
    ...over,
  };
}

function setup(hasChanges = true, networkOn = true) {
  useNetworkStore.setState({
    status: { state: networkOn ? 'running' : 'off' } as NetworkStatus,
    nodes: [
      node({}),
      node({ deviceId: 'd2', displayName: 'Offline Bo', status: 'offline' }),
      node({ deviceId: 'd3', displayName: 'Nearby Cy', group: 'nearby' }),
    ],
  });
  return render(<ContractCheckAction collectionId='/ws/col' contractId='k1' hasChanges={hasChanges} />);
}

describe('ContractCheckAction', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    useContractCheckRunsStore.getState().reset();
  });

  it('is disabled with a tooltip when the contract has no changes', async () => {
    setup(false);
    expect(screen.getByRole('button', { name: 'Check with the team' })).toBeDisabled();
    await userEvent.hover(screen.getByRole('button', { name: 'Check with the team' }).parentElement as HTMLElement);
    expect((await screen.findAllByText('This contract has no changes since its snapshot.')).length).toBeGreaterThan(0);
  });

  it('is disabled when the network is off', () => {
    setup(true, false);
    expect(screen.getByRole('button', { name: 'Check with the team' })).toBeDisabled();
  });

  it('lists only online paired nodes', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    expect(screen.getByLabelText(/Ada/)).toBeInTheDocument();
    expect(screen.queryByText(/Offline Bo/)).toBeNull();
    expect(screen.queryByText(/Nearby Cy/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('shows an empty state and keeps Send disabled with no online node', async () => {
    setup();
    useNetworkStore.setState({ nodes: [] });
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    expect(screen.getByText('No paired node is online')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('starts the check, tracks each check and stores the run', async () => {
    mocks.api.startContractCheck.mockResolvedValue({
      checks: [{ deviceId: 'd1', checkId: 'c1' }],
      changes: [],
    });
    const track = vi.spyOn(store(), 'trackContractCheck');
    useNetworkStore.setState({ trackContractCheck: track });
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    await userEvent.click(screen.getByLabelText(/Ada/));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() =>
      expect(mocks.api.startContractCheck).toHaveBeenCalledWith('/ws/col', 'k1', ['d1']),
    );
    await waitFor(() => expect(track).toHaveBeenCalledWith('c1', 'd1'));
    const run = useContractCheckRunsStore.getState().runs[contractCheckRunKey('/ws/col', 'k1')];
    expect(run.targets[0]).toEqual({ checkId: 'c1', deviceId: 'd1', name: 'Ada' });
  });

  it('shows the fixed text when too many changes were offered', async () => {
    mocks.api.startContractCheck.mockRejectedValue('Check fewer changes at a time.');
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    await userEvent.click(screen.getByLabelText(/Ada/));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Check fewer changes at a time.')).toBeInTheDocument();
  });

  it('shows a generic text for any other error', async () => {
    mocks.api.startContractCheck.mockRejectedValue(new Error('/home/me/secret failed'));
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    await userEvent.click(screen.getByLabelText(/Ada/));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Could not start the check.')).toBeInTheDocument();
    expect(screen.queryByText(/secret/)).toBeNull();
  });

  it('explains a contract with nothing that can be checked', async () => {
    mocks.api.startContractCheck.mockRejectedValue('Invalid input: This contract has no changes to check.');
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    await userEvent.click(screen.getByLabelText(/Ada/));
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(
      await screen.findByText(
        'No changes can be checked. Changes to the method, the URL or a removed request are not covered.',
      ),
    ).toBeInTheDocument();
  });

  it('starts once on a double click and tracks every returned check', async () => {
    mocks.api.startContractCheck.mockResolvedValue({
      checks: [
        { deviceId: 'd1', checkId: 'c1' },
        { deviceId: 'd9', checkId: 'c9' },
      ],
      changes: [],
    });
    const track = vi.fn();
    useNetworkStore.setState({ trackContractCheck: track });
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Check with the team' }));
    await userEvent.click(screen.getByLabelText(/Ada/));
    await userEvent.dblClick(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(track).toHaveBeenCalledTimes(2));
    expect(mocks.api.startContractCheck).toHaveBeenCalledTimes(1);
    expect(track).toHaveBeenCalledWith('c1', 'd1');
    expect(track).toHaveBeenCalledWith('c9', 'd9');
  });
});
