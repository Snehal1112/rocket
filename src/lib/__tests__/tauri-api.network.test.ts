import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  addNetworkAddress,
  cancelNetworkPairing,
  getNetworkStatus,
  listNetworkAddresses,
  listNetworkBlocked,
  listNetworkNodes,
  markNetworkIntroSeen,
  onNetworkChanged,
  onNetworkPairing,
  removeNetworkAddress,
  removeNetworkPaired,
  requestNetworkPairing,
  respondNetworkPairing,
  setNetworkProfile,
  setNetworkVisibility,
  unblockNetworkDevice,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('team network commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('calls the commands that take no arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await getNetworkStatus();
    expect(invoke).toHaveBeenLastCalledWith('network_status');
    await listNetworkNodes();
    expect(invoke).toHaveBeenLastCalledWith('network_list_nodes');
    await markNetworkIntroSeen();
    expect(invoke).toHaveBeenLastCalledWith('network_mark_intro_seen');
    await listNetworkAddresses();
    expect(invoke).toHaveBeenLastCalledWith('network_list_addresses');
    await listNetworkBlocked();
    expect(invoke).toHaveBeenLastCalledWith('network_list_blocked');
  });

  it('sends camelCase arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await setNetworkVisibility(true);
    expect(invoke).toHaveBeenLastCalledWith('network_set_visibility', { hidden: true });
    await setNetworkProfile('Ana Lima');
    expect(invoke).toHaveBeenLastCalledWith('network_set_profile', { displayName: 'Ana Lima' });
    await addNetworkAddress('10.0.0.5', 47821);
    expect(invoke).toHaveBeenLastCalledWith('network_add_address', {
      host: '10.0.0.5',
      port: 47821,
    });
    await removeNetworkAddress('10.0.0.5', 47821);
    expect(invoke).toHaveBeenLastCalledWith('network_remove_address', {
      host: '10.0.0.5',
      port: 47821,
    });
    await requestNetworkPairing('DEV1');
    expect(invoke).toHaveBeenLastCalledWith('network_request_pairing', { deviceId: 'DEV1' });
    await respondNetworkPairing('p1', 'block');
    expect(invoke).toHaveBeenLastCalledWith('network_respond_pairing', {
      pairingId: 'p1',
      decision: 'block',
    });
    await cancelNetworkPairing('p1');
    expect(invoke).toHaveBeenLastCalledWith('network_cancel_pairing', { pairingId: 'p1' });
    await removeNetworkPaired('DEV1');
    expect(invoke).toHaveBeenLastCalledWith('network_remove_paired', { deviceId: 'DEV1' });
    await unblockNetworkDevice('DEV1');
    expect(invoke).toHaveBeenLastCalledWith('network_unblock', { deviceId: 'DEV1' });
  });

  it('returns what the command returns', async () => {
    vi.mocked(invoke).mockResolvedValue({ pairingId: 'p1', code: '482913' });
    await expect(requestNetworkPairing('DEV1')).resolves.toEqual({
      pairingId: 'p1',
      code: '482913',
    });
  });

  it('passes a rejection through unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('Invalid input: Port is taken');
    await expect(addNetworkAddress('h', 1)).rejects.toBe('Invalid input: Port is taken');
  });

  it('listens on the two channels and hands over the payload', async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const onChanged = vi.fn();
    const onPairing = vi.fn();
    await onNetworkChanged(onChanged);
    await onNetworkPairing(onPairing);
    expect(listen).toHaveBeenNthCalledWith(1, 'network-changed', expect.any(Function));
    expect(listen).toHaveBeenNthCalledWith(2, 'network-pairing', expect.any(Function));

    const changedCallback = vi.mocked(listen).mock.calls[0]?.[1];
    const pairingCallback = vi.mocked(listen).mock.calls[1]?.[1];
    if (!changedCallback || !pairingCallback) throw new Error('listeners were not registered');
    const deliver = (cb: unknown, payload: unknown) =>
      (cb as (event: { payload: unknown }) => void)({ payload });
    deliver(changedCallback, { type: 'nodesChanged' });
    deliver(pairingCallback, { type: 'pairingFinished', pairing_id: 'p1' });
    expect(onChanged).toHaveBeenCalledWith({ type: 'nodesChanged' });
    expect(onPairing).toHaveBeenCalledWith({ type: 'pairingFinished', pairing_id: 'p1' });
  });
});
