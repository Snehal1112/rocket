import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { NetworkNode } from '@/lib/tauri-api';
import {
  INCOMING_TTL_MS,
  onlinePairedNodes,
  selectOnlinePairedCount,
  startNetworkPairing,
  useNetworkStore,
} from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeIncoming, makeIncomingTransfer, makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  api: {
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
    requestNetworkPairing: vi.fn(),
    removeNetworkPaired: vi.fn(),
    addNetworkAddress: vi.fn(),
    removeNetworkAddress: vi.fn(),
    unblockNetworkDevice: vi.fn(),
    setNetworkVisibility: vi.fn(),
    setNetworkProfile: vi.fn(),
    markNetworkIntroSeen: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);

const store = () => useNetworkStore.getState();

const incoming = (pairingId = 'p1') => makeIncoming({ pairingId });

describe('network-store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('refresh stores the status and clears an old error', async () => {
    useNetworkStore.setState({ error: 'old' });
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus({ displayName: 'Ana' }));
    await store().refresh();
    expect(store().status?.displayName).toBe('Ana');
    expect(store().error).toBeNull();
  });

  it('a failed refresh keeps the last status and records the error text', async () => {
    useNetworkStore.setState({ status: makeStatus({ displayName: 'Kept' }) });
    mocks.api.getNetworkStatus.mockRejectedValue('Internal error: backend down');
    await store().refresh();
    expect(store().status?.displayName).toBe('Kept');
    expect(store().error).toBe('backend down');
  });

  it('refreshNodes stores the node list', async () => {
    mocks.api.listNetworkNodes.mockResolvedValue([makeNode({ deviceId: 'A' })]);
    await store().refreshNodes();
    expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
  });

  it('a successful node load keeps a status error', async () => {
    useNetworkStore.setState({ error: 'status failed' });
    mocks.api.listNetworkNodes.mockResolvedValue([]);
    await store().refreshNodes();
    expect(store().error).toBe('status failed');
  });

  it('keeps the newest node list when responses arrive out of order', async () => {
    const first = createDeferred<NetworkNode[]>();
    const second = createDeferred<NetworkNode[]>();
    mocks.api.listNetworkNodes
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const a = store().refreshNodes();
    const b = store().refreshNodes();
    second.resolve([makeNode({ deviceId: 'NEW' })]);
    await b;
    first.resolve([makeNode({ deviceId: 'OLD' })]);
    await a;
    expect(store().nodes.map((n) => n.deviceId)).toEqual(['NEW']);
  });

  it('addIncoming dedupes by pairing id', () => {
    store().addIncoming(incoming('p1'));
    store().addIncoming(incoming('p1'));
    store().addIncoming(incoming('p2'));
    expect(store().incoming.map((i) => i.pairingId)).toEqual(['p1', 'p2']);
  });

  it('removes an incoming request after the expiry time', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    vi.advanceTimersByTime(INCOMING_TTL_MS - 1);
    expect(store().incoming).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(store().incoming).toHaveLength(0);
  });

  it('a duplicate event does not extend the expiry', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    vi.advanceTimersByTime(60_000);
    store().addIncoming(incoming());
    expect(store().incoming).toHaveLength(1);
    vi.advanceTimersByTime(INCOMING_TTL_MS - 60_000);
    expect(store().incoming).toHaveLength(0);
  });

  it('removeIncoming cancels the expiry timer', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    expect(vi.getTimerCount()).toBe(1);
    store().removeIncoming('p1');
    expect(vi.getTimerCount()).toBe(0);
    expect(store().incoming).toHaveLength(0);
  });

  it('markIncomingAccepted flags one request and ignores an unknown id', () => {
    store().addIncoming(incoming('p1'));
    store().addIncoming(incoming('p2'));
    store().markIncomingAccepted('p2');
    store().markIncomingAccepted('missing');
    expect(store().incoming.map((i) => i.accepted)).toEqual([false, true]);
  });

  it('reset clears everything, including pending timers', () => {
    vi.useFakeTimers();
    store().addIncoming(incoming());
    useNetworkStore.setState({
      status: makeStatus(),
      nodes: [makeNode()],
      panelOpen: true,
      dialogPairingId: 'p1',
    });
    store().reset();
    expect(store().status).toBeNull();
    expect(store().nodes).toEqual([]);
    expect(store().incoming).toEqual([]);
    expect(store().activePairing).toBeNull();
    expect(store().panelOpen).toBe(false);
    expect(store().dialogPairingId).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('selectOnlinePairedCount counts only online paired nodes', () => {
    useNetworkStore.setState({
      nodes: [
        makeNode({ deviceId: '1', group: 'paired', status: 'online' }),
        makeNode({ deviceId: '2', group: 'paired', status: 'offline' }),
        makeNode({ deviceId: '3', group: 'nearby', status: 'available' }),
        makeNode({ deviceId: '4', group: 'paired', status: 'online' }),
      ],
    });
    expect(selectOnlinePairedCount(store())).toBe(2);
  });

  describe('mutation actions', () => {
    beforeEach(() => {
      mocks.api.listNetworkNodes.mockResolvedValue([makeNode({ deviceId: 'A' })]);
      mocks.api.getNetworkStatus.mockResolvedValue(makeStatus({ displayName: 'Fresh' }));
    });

    it('removePaired calls the backend and reloads the nodes', async () => {
      mocks.api.removeNetworkPaired.mockResolvedValue(undefined);
      await store().removePaired('DEV1');
      expect(mocks.api.removeNetworkPaired).toHaveBeenCalledWith('DEV1');
      expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
    });

    it('removePaired reloads the nodes after a failure and rethrows', async () => {
      mocks.api.removeNetworkPaired.mockRejectedValue('Internal error: disk full');
      await expect(store().removePaired('DEV1')).rejects.toBe('Internal error: disk full');
      expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
      expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
    });

    it('addAddress sends the address and reloads the nodes', async () => {
      mocks.api.addNetworkAddress.mockResolvedValue(undefined);
      await store().addAddress('10.0.0.5', 47821);
      expect(mocks.api.addNetworkAddress).toHaveBeenCalledWith('10.0.0.5', 47821);
      expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
    });

    it('addAddress rejects with the backend error and does not reload', async () => {
      mocks.api.addNetworkAddress.mockRejectedValue('Invalid input: Port is taken');
      await expect(store().addAddress('h', 1)).rejects.toBe('Invalid input: Port is taken');
      expect(mocks.api.listNetworkNodes).not.toHaveBeenCalled();
    });

    it('removeAddress sends the address and reloads the nodes', async () => {
      mocks.api.removeNetworkAddress.mockResolvedValue(undefined);
      await store().removeAddress('10.0.0.5', 47821);
      expect(mocks.api.removeNetworkAddress).toHaveBeenCalledWith('10.0.0.5', 47821);
      expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
    });

    it('unblock sends the device id and reloads the nodes', async () => {
      mocks.api.unblockNetworkDevice.mockResolvedValue(undefined);
      await store().unblock('B1');
      expect(mocks.api.unblockNetworkDevice).toHaveBeenCalledWith('B1');
      expect(store().nodes.map((n) => n.deviceId)).toEqual(['A']);
    });

    it('setHidden saves the flag and reloads the status', async () => {
      mocks.api.setNetworkVisibility.mockResolvedValue(undefined);
      await store().setHidden(true);
      expect(mocks.api.setNetworkVisibility).toHaveBeenCalledWith(true);
      expect(store().status?.displayName).toBe('Fresh');
    });

    it('setHidden rejects and leaves the status alone when the call fails', async () => {
      useNetworkStore.setState({ status: makeStatus({ displayName: 'Kept' }) });
      mocks.api.setNetworkVisibility.mockRejectedValue('Internal error: settings locked');
      await expect(store().setHidden(true)).rejects.toBe('Internal error: settings locked');
      expect(mocks.api.getNetworkStatus).not.toHaveBeenCalled();
      expect(store().status?.displayName).toBe('Kept');
    });

    it('saveName saves the name and reloads the status', async () => {
      mocks.api.setNetworkProfile.mockResolvedValue(undefined);
      await store().saveName('Ana L.');
      expect(mocks.api.setNetworkProfile).toHaveBeenCalledWith('Ana L.');
      expect(store().status?.displayName).toBe('Fresh');
    });

    it('markIntroSeen saves the flag and reloads the status', async () => {
      mocks.api.markNetworkIntroSeen.mockResolvedValue(undefined);
      await store().markIntroSeen();
      expect(mocks.api.markNetworkIntroSeen).toHaveBeenCalledTimes(1);
      expect(store().status?.displayName).toBe('Fresh');
    });
  });

  describe('startNetworkPairing', () => {
    it('keeps the code, opens the dialog and closes the panel', async () => {
      useNetworkStore.setState({ panelOpen: true });
      mocks.api.requestNetworkPairing.mockResolvedValue({ pairingId: 'p1', code: '482913' });
      await startNetworkPairing(makeNode({ deviceId: 'DEV1', shortId: 'AAAA-1111' }));
      expect(mocks.api.requestNetworkPairing).toHaveBeenCalledWith('DEV1');
      expect(store().activePairing).toEqual({
        pairingId: 'p1',
        deviceId: 'DEV1',
        shortId: 'AAAA-1111',
        displayName: 'Ben Ortiz',
        code: '482913',
        phase: 'waiting',
      });
      expect(store().dialogPairingId).toBe('p1');
      expect(store().panelOpen).toBe(false);
    });

    it('leaves the state alone and rethrows when the request fails', async () => {
      useNetworkStore.setState({ panelOpen: true });
      mocks.api.requestNetworkPairing.mockRejectedValue('Invalid input: busy');
      await expect(startNetworkPairing(makeNode())).rejects.toBe('Invalid input: busy');
      expect(store().activePairing).toBeNull();
      expect(store().dialogPairingId).toBeNull();
      expect(store().panelOpen).toBe(true);
    });
  });
});

describe('network-store transfers', () => {
  const offer = (transferId = 't1') => makeIncomingTransfer({ transferId });
  const progress = (transferId = 't1', doneBytes = 1024) => ({
    transferId,
    direction: 'receive' as const,
    doneBytes,
    totalBytes: 2048,
  });

  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('dedupes by transfer id and does not extend the expiry', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer('t1'));
    vi.advanceTimersByTime(60_000);
    store().addIncomingTransfer(offer('t1'));
    expect(store().incomingTransfers).toHaveLength(1);
    vi.advanceTimersByTime(INCOMING_TTL_MS - 60_000);
    expect(store().incomingTransfers).toHaveLength(0);
  });

  it('keeps an offer until the expiry time', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer());
    vi.advanceTimersByTime(INCOMING_TTL_MS - 1);
    expect(store().incomingTransfers).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(store().incomingTransfers).toHaveLength(0);
  });

  it('keeps two offers, each with its own timer', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer('a'));
    store().addIncomingTransfer(offer('b'));
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['a', 'b']);
    expect(vi.getTimerCount()).toBe(2);
    store().removeIncomingTransfer('a');
    expect(vi.getTimerCount()).toBe(1);
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['b']);
  });

  it('stores a zero byte offer as given', () => {
    store().addIncomingTransfer(makeIncomingTransfer({ sizeBytes: 0 }));
    expect(store().incomingTransfers[0]?.sizeBytes).toBe(0);
  });

  it('an accepted offer no longer expires', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer());
    store().markTransferAccepted('t1');
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(INCOMING_TTL_MS * 2);
    expect(store().incomingTransfers[0]?.accepted).toBe(true);
  });

  it('markTransferAccepted does not bring back an offer that is gone', () => {
    store().markTransferAccepted('missing');
    expect(store().incomingTransfers).toEqual([]);
  });

  it('keeps one progress row for each transfer and updates it in place', () => {
    store().setTransferProgress(progress('a', 100));
    store().setTransferProgress(progress('b', 5));
    store().setTransferProgress(progress('a', 900));
    expect(store().activeTransfers).toEqual([progress('a', 900), progress('b', 5)]);
  });

  it('keeps the direction of a progress row', () => {
    store().setTransferProgress({ ...progress('a'), direction: 'send' });
    expect(store().activeTransfers[0]?.direction).toBe('send');
  });

  it('finishTransfer removes the offer, the progress and the timer, and reports the first finish', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer());
    store().setTransferProgress(progress());
    expect(store().finishTransfer('t1')).toBe(true);
    expect(store().incomingTransfers).toEqual([]);
    expect(store().activeTransfers).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
    expect(store().finishTransfer('t1')).toBe(false);
  });

  it('ignores progress and offers that arrive after the finish', () => {
    store().finishTransfer('t1');
    store().setTransferProgress(progress());
    store().addIncomingTransfer(offer());
    expect(store().activeTransfers).toEqual([]);
    expect(store().incomingTransfers).toEqual([]);
  });

  it('remembers only the last 100 finished transfers', () => {
    for (let i = 0; i < 101; i++) store().finishTransfer(`f${i}`);
    // The oldest id is forgotten, the newest is still remembered.
    expect(store().finishTransfer('f0')).toBe(true);
    expect(store().finishTransfer('f100')).toBe(false);
  });

  it('opens and closes the transfer dialog without touching the pairing dialog', () => {
    store().openPairingDialog('p1');
    store().openTransferDialog('t1');
    expect(store().dialogTransferId).toBe('t1');
    store().closeTransferDialog();
    expect(store().dialogTransferId).toBeNull();
    expect(store().dialogPairingId).toBe('p1');
  });

  it('reset clears the transfers, the timers and the finished memory', () => {
    vi.useFakeTimers();
    store().addIncomingTransfer(offer('a'));
    store().setTransferProgress(progress('b'));
    store().openTransferDialog('a');
    store().endTransferDialog('The other node went offline.');
    store().finishTransfer('c');
    store().trackOutgoing('d', 'Dana Roy');
    store().markCancelledHere('e');
    store().reset();
    expect(store().incomingTransfers).toEqual([]);
    expect(store().activeTransfers).toEqual([]);
    expect(store().dialogTransferId).toBeNull();
    expect(store().dialogEndedText).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
    expect(store().advanceOutgoing('d', 'sending')).toBeNull();
    expect(store().wasCancelledHere('e')).toBe(false);
    // The id may be used again after a reset.
    expect(store().finishTransfer('c')).toBe(true);
  });

  it('tracks a send and moves it forward once per phase', () => {
    store().trackOutgoing('t1', 'Dana Roy');
    expect(store().advanceOutgoing('t1', 'sending')).toBe('Dana Roy');
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
    expect(store().advanceOutgoing('t1', 'sent')).toBe('Dana Roy');
    // A phase never goes back.
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
    expect(store().advanceOutgoing('other', 'sending')).toBeNull();
  });

  it('trackOutgoing reports false for a transfer that already finished', () => {
    store().finishTransfer('t1');
    expect(store().trackOutgoing('t1', 'Dana Roy')).toBe(false);
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
    expect(store().trackOutgoing('t2', 'Dana Roy')).toBe(true);
  });

  it('unmarkCancelledHere undoes a local cancel mark', () => {
    store().markCancelledHere('t1');
    store().unmarkCancelledHere('t1');
    expect(store().wasCancelledHere('t1')).toBe(false);
  });

  it('can skip the sending phase of a one chunk send', () => {
    store().trackOutgoing('t1', 'Dana Roy');
    expect(store().advanceOutgoing('t1', 'sent')).toBe('Dana Roy');
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
  });

  it('finishTransfer forgets the send and the local cancel', () => {
    store().trackOutgoing('t1', 'Dana Roy');
    store().markCancelledHere('t1');
    expect(store().wasCancelledHere('t1')).toBe(true);
    store().finishTransfer('t1');
    expect(store().wasCancelledHere('t1')).toBe(false);
    expect(store().advanceOutgoing('t1', 'sending')).toBeNull();
  });

  it('shows an end reason in the open dialog until it closes or another opens', () => {
    store().openTransferDialog('t1');
    expect(store().dialogEndedText).toBeNull();
    store().endTransferDialog('The other node went offline.');
    expect(store().dialogEndedText).toBe('The other node went offline.');
    expect(store().dialogTransferId).toBe('t1');
    store().openTransferDialog('t2');
    expect(store().dialogEndedText).toBeNull();
    store().endTransferDialog('The other node went offline.');
    store().closeTransferDialog();
    expect(store().dialogEndedText).toBeNull();
  });

  it('a pairing id and a transfer id never share a timer', () => {
    vi.useFakeTimers();
    store().addIncoming(makeIncoming({ pairingId: 'same' }));
    store().addIncomingTransfer(offer('same'));
    expect(vi.getTimerCount()).toBe(2);
    store().removeIncomingTransfer('same');
    expect(vi.getTimerCount()).toBe(1);
    expect(store().incoming).toHaveLength(1);
  });
});

describe('onlinePairedNodes', () => {
  it('keeps only paired nodes that are online', () => {
    const nodes = [
      makeNode({ deviceId: 'A', group: 'paired', status: 'online' }),
      makeNode({ deviceId: 'B', group: 'paired', status: 'offline' }),
      makeNode({ deviceId: 'C', group: 'nearby', status: 'available' }),
      makeNode({ deviceId: 'D', group: 'paired', status: 'needs_update' }),
    ];
    expect(onlinePairedNodes(nodes).map((n) => n.deviceId)).toEqual(['A']);
  });

  it('agrees with the count selector', () => {
    const nodes = [makeNode({ group: 'paired', status: 'online' })];
    useNetworkStore.setState({ nodes });
    expect(selectOnlinePairedCount(useNetworkStore.getState())).toBe(onlinePairedNodes(nodes).length);
  });
});
