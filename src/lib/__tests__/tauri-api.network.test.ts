import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  addNetworkAddress,
  cancelNetworkPairing,
  cancelNetworkSync,
  cancelNetworkTransfer,
  confirmNetworkSync,
  getNetworkSyncConflictFiles,
  getNetworkStatus,
  listNetworkAddresses,
  listNetworkBlocked,
  listNetworkLinks,
  listNetworkNodes,
  markNetworkIntroSeen,
  onNetworkChanged,
  onNetworkPairing,
  onNetworkSync,
  onNetworkTransfer,
  removeNetworkAddress,
  removeNetworkPaired,
  requestNetworkPairing,
  respondNetworkPairing,
  respondNetworkTransfer,
  reviewNetworkSync,
  sendNetworkCollection,
  setNetworkProfile,
  setNetworkVisibility,
  startNetworkSync,
  unblockNetworkDevice,
  unlinkNetworkLink,
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

describe('send a collection', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('sends camelCase arguments and returns the transfer id and the notes', async () => {
    const started = {
      transferId: '0123456789abcdef0123456789abcdef',
      notes: ['Flows: skipped', 'Auth token: blanked'],
    };
    vi.mocked(invoke).mockResolvedValue(started);
    await expect(sendNetworkCollection('DEV1', 'Orders "v2" café', true, false)).resolves.toEqual(started);
    expect(invoke).toHaveBeenLastCalledWith('network_send_collection', {
      deviceId: 'DEV1',
      collection: 'Orders "v2" café',
      includeEnvironments: true,
      keepInSync: false,
    });
  });

  it('answers and cancels a transfer with camelCase arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await respondNetworkTransfer('t1', false);
    expect(invoke).toHaveBeenLastCalledWith('network_respond_transfer', {
      transferId: 't1',
      accept: false,
    });
    await cancelNetworkTransfer('t1');
    expect(invoke).toHaveBeenLastCalledWith('network_cancel_transfer', { transferId: 't1' });
  });

  it('passes a rejection through unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('Invalid input: That node is offline.');
    await expect(sendNetworkCollection('DEV1', 'Orders', false, false)).rejects.toBe(
      'Invalid input: That node is offline.',
    );
  });

  it('listens on network-transfer and hands over the payload', async () => {
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const handler = vi.fn();
    await onNetworkTransfer(handler);
    expect(listen).toHaveBeenCalledWith('network-transfer', expect.any(Function));
    const callback = vi.mocked(listen).mock.calls[0]?.[1];
    if (!callback) throw new Error('listener was not registered');
    (callback as (event: { payload: unknown }) => void)({
      payload: { type: 'progress', transfer_id: 't1' },
    });
    expect(handler).toHaveBeenCalledWith({ type: 'progress', transfer_id: 't1' });
  });
});

describe('team network sync commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('sends keepInSync with the collection', async () => {
    vi.mocked(invoke).mockResolvedValue({ transferId: 't1', notes: [] });
    await sendNetworkCollection('DEV1', 'Orders', false, true);
    expect(invoke).toHaveBeenLastCalledWith('network_send_collection', {
      deviceId: 'DEV1',
      collection: 'Orders',
      includeEnvironments: false,
      keepInSync: true,
    });
  });

  it('starts a sync and returns its id', async () => {
    vi.mocked(invoke).mockResolvedValue({ syncId: 's1' });
    await expect(startNetworkSync('L1')).resolves.toEqual({ syncId: 's1' });
    expect(invoke).toHaveBeenLastCalledWith('network_sync_start', { linkId: 'L1' });
  });

  it('sends the decisions of a review unchanged', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const decisions = [
      { path: 'Orders "v2"/a.yml', resolution: 'keep_both' as const },
      { path: 'b.yml', resolution: 'keep_mine' as const },
    ];
    await reviewNetworkSync('s1', true, decisions);
    expect(invoke).toHaveBeenLastCalledWith('network_sync_review', {
      syncId: 's1',
      accept: true,
      decisions,
    });
    await reviewNetworkSync('s1', false, []);
    expect(invoke).toHaveBeenLastCalledWith('network_sync_review', {
      syncId: 's1',
      accept: false,
      decisions: [],
    });
  });

  it('confirms, cancels, lists and unlinks', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await confirmNetworkSync('s1', true);
    expect(invoke).toHaveBeenLastCalledWith('network_sync_confirm', { syncId: 's1', accept: true });
    await cancelNetworkSync('s1');
    expect(invoke).toHaveBeenLastCalledWith('network_sync_cancel', { syncId: 's1' });
    await listNetworkLinks();
    expect(invoke).toHaveBeenLastCalledWith('network_list_links');
    await unlinkNetworkLink('L1');
    expect(invoke).toHaveBeenLastCalledWith('network_unlink', { linkId: 'L1' });
  });

  it('reads the two versions of a conflict', async () => {
    vi.mocked(invoke).mockResolvedValue({ mine: 'a', theirs: null });
    await expect(getNetworkSyncConflictFiles('s1', 'a.yml')).resolves.toEqual({
      mine: 'a',
      theirs: null,
    });
    expect(invoke).toHaveBeenLastCalledWith('network_sync_conflict_files', {
      syncId: 's1',
      path: 'a.yml',
    });
  });

  it('passes a rejected command through unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('Another sync is already running for this link.');
    await expect(startNetworkSync('L1')).rejects.toBe(
      'Another sync is already running for this link.',
    );
  });

  it('listens on network-sync and passes the payload', async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockResolvedValue(unlisten);
    const handler = vi.fn();
    await expect(onNetworkSync(handler)).resolves.toBe(unlisten);
    expect(listen).toHaveBeenCalledWith('network-sync', expect.any(Function));
    const callback = vi.mocked(listen).mock.calls[0][1] as (e: { payload: unknown }) => void;
    callback({ payload: { type: 'linkChanged', link_id: 'L1' } });
    expect(handler).toHaveBeenCalledWith({ type: 'linkChanged', link_id: 'L1' });
  });
});
