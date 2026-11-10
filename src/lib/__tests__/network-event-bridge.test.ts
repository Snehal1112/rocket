import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useNetworkEventBridge } from '@/lib/network-event-bridge';
import { PAIRING_FAILED_TOAST_ID } from '@/lib/network-utils';
import { useNetworkStore } from '@/stores/network-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { createDeferred } from '@/test/deferred';
import { makeIncoming, makeIncomingTransfer, makeNode, makeStatus } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => {
  const handlers: Record<string, (payload: unknown) => void> = {};
  const unlisten = vi.fn();
  const listener = (name: string) =>
    vi.fn((handler: (payload: unknown) => void) => {
      handlers[name] = handler;
      return Promise.resolve<() => void>(unlisten);
    });
  return {
    handlers,
    unlisten,
    toast: { info: vi.fn(), success: vi.fn(), error: vi.fn(), dismiss: vi.fn() },
    api: {
      onNetworkChanged: listener('changed'),
      onNetworkPairing: listener('pairing'),
      onNetworkTransfer: listener('transfer'),
      onNetworkSync: listener('sync'),
      onNetworkContractCheck: listener('contractCheck'),
      listNetworkLinks: vi.fn(),
      getNetworkStatus: vi.fn(),
      listNetworkNodes: vi.fn(),
      requestNetworkPairing: vi.fn(),
    },
  };
});

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

async function mountBridge() {
  const view = renderHook(() => useNetworkEventBridge());
  await waitFor(() => expect(store().status).not.toBeNull());
  return view;
}

function emit(name: 'changed' | 'pairing' | 'transfer' | 'sync', payload: unknown): void {
  const handler = mocks.handlers[name];
  if (!handler) throw new Error(`no ${name} listener`);
  handler(payload);
}

const incomingEvent = (id = 'p1') => ({
  type: 'pairingIncoming',
  pairing_id: id,
  device_id: 'DEV9',
  short_id: 'ZZZZ-9999',
  display_name: 'Dana Roy',
  code: '482913',
});

const finished = (outcome: string, id = 'p1') => ({
  type: 'pairingFinished',
  pairing_id: id,
  outcome,
  display_name: 'Dana Roy',
});

describe('useNetworkEventBridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.handlers)) delete mocks.handlers[key];
    store().reset();
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkNodes.mockResolvedValue([makeNode()]);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
  });

  it('loads the status and the nodes once on mount', async () => {
    await mountBridge();
    expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1);
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkChanged).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkPairing).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkTransfer).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkSync).toHaveBeenCalledTimes(1);
    expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
  });

  it('reloads the links when the active workspace changes', async () => {
    await mountBridge();
    expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
    act(() => useWorkspaceStore.getState().setActiveWorkspaceId('ws-other'));
    await waitFor(() => expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(2));
    useWorkspaceStore.getState().setActiveWorkspaceId('');
  });

  it('nodesChanged reloads the nodes', async () => {
    await mountBridge();
    emit('changed', { type: 'nodesChanged' });
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
    expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1);
  });

  it('statusChanged reloads the status', async () => {
    await mountBridge();
    emit('changed', { type: 'statusChanged' });
    await waitFor(() => expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(2));
  });

  it('pairedRemoved reloads the nodes and tells the user when the peer removed it', async () => {
    await mountBridge();
    emit('changed', {
      type: 'pairedRemoved',
      device_id: 'DEV9',
      display_name: 'Dana Roy',
      by_peer: true,
    });
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy removed the pairing.');
  });

  it('pairedRemoved from this node reloads quietly', async () => {
    await mountBridge();
    emit('changed', {
      type: 'pairedRemoved',
      device_id: 'DEV9',
      display_name: 'Dana Roy',
      by_peer: false,
    });
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('pairingIncoming adds one request, even when the event repeats', async () => {
    await mountBridge();
    emit('pairing', incomingEvent());
    emit('pairing', incomingEvent());
    // The default of makeIncoming matches the fields of incomingEvent.
    expect(store().incoming).toEqual([makeIncoming()]);
  });

  it('shows a failed pairing under the shared id, so a second report replaces it', async () => {
    await mountBridge();
    emit('pairing', finished('failed'));
    expect(mocks.toast.error).toHaveBeenCalledWith('Pairing failed. Try again.', {
      id: PAIRING_FAILED_TOAST_ID,
    });
  });

  it('tells a responder who accepted that the other side backed out', async () => {
    await mountBridge();
    emit('pairing', incomingEvent());
    store().markIncomingAccepted('p1');
    emit('pairing', finished('declined'));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy did not pair.');
  });

  it('tells a responder who accepted that the request was cancelled', async () => {
    await mountBridge();
    emit('pairing', incomingEvent());
    store().markIncomingAccepted('p1');
    emit('pairing', finished('cancelled'));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy did not pair.');
  });

  it.each([
    ['accepted', 'success', 'Paired with Dana Roy.'],
    ['expired', 'info', 'The request expired.'],
  ] as const)('announces the %s outcome', async (outcome, method, message) => {
    await mountBridge();
    emit('pairing', finished(outcome));
    expect(mocks.toast[method]).toHaveBeenCalledWith(message);
  });

  it('accepted also reloads the nodes', async () => {
    await mountBridge();
    emit('pairing', finished('accepted'));
    await waitFor(() => expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(2));
  });

  it('says nothing for a cancelled or unknown outcome', async () => {
    await mountBridge();
    emit('pairing', finished('cancelled'));
    emit('pairing', finished('something-new'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(mocks.toast.success).not.toHaveBeenCalled();
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('tells the starter of a pairing that the peer declined', async () => {
    await mountBridge();
    useNetworkStore.setState({
      activePairing: {
        pairingId: 'p1',
        deviceId: 'DEV9',
        shortId: 'ZZZZ-9999',
        displayName: 'Dana Roy',
        code: '482913',
        phase: 'done',
      },
    });
    emit('pairing', finished('declined'));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy declined.');
    expect(store().activePairing).toBeNull();
  });

  it('does not tell a user about their own decline', async () => {
    await mountBridge();
    emit('pairing', incomingEvent());
    emit('pairing', finished('declined'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(store().incoming).toEqual([]);
  });

  it('a finished pairing closes its dialog but not another one', async () => {
    await mountBridge();
    useNetworkStore.setState({ dialogPairingId: 'p1' });
    emit('pairing', finished('expired', 'other'));
    expect(store().dialogPairingId).toBe('p1');
    emit('pairing', finished('expired', 'p1'));
    expect(store().dialogPairingId).toBeNull();
  });

  it('ignores events after unmount and stops listening', async () => {
    const view = await mountBridge();
    view.unmount();
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(5));
    emit('changed', { type: 'nodesChanged' });
    emit('pairing', incomingEvent());
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
    expect(store().incoming).toEqual([]);
  });

  it('stops a listener that finishes registering after unmount', async () => {
    const late = createDeferred<() => void>();
    const lateUnlisten = vi.fn();
    mocks.api.onNetworkChanged.mockImplementationOnce(() => late.promise);
    const view = renderHook(() => useNetworkEventBridge());
    view.unmount();
    late.resolve(lateUnlisten);
    await waitFor(() => expect(lateUnlisten).toHaveBeenCalledTimes(1));
  });
});

describe('useNetworkEventBridge transfers', () => {
  const offerEvent = (id = 't1', patch: Record<string, unknown> = {}) => ({
    type: 'offerIncoming',
    transfer_id: id,
    device_id: 'DEV9',
    short_id: 'ZZZZ-9999',
    display_name: 'Dana Roy',
    collection_name: 'Orders',
    size_bytes: 2048,
    ...patch,
  });
  const progressEvent = (id = 't1', done = 1024, direction = 'receive') => ({
    type: 'progress',
    transfer_id: id,
    direction,
    done_bytes: done,
    total_bytes: 2048,
  });
  const finishedEvent = (
    outcome: string,
    direction = 'receive',
    patch: Record<string, unknown> = {},
  ) => ({
    type: 'finished',
    transfer_id: 't1',
    direction,
    outcome,
    display_name: 'Dana Roy',
    collection_name: 'Orders',
    message: null,
    ...patch,
  });
  const resultId = { id: 'network-transfer-result-t1' };

  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.handlers)) delete mocks.handlers[key];
    store().reset();
    mocks.api.getNetworkStatus.mockResolvedValue(makeStatus());
    mocks.api.listNetworkNodes.mockResolvedValue([makeNode()]);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
  });

  it('adds one row for a repeated offer', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    emit('transfer', offerEvent());
    expect(store().incomingTransfers).toEqual([makeIncomingTransfer()]);
  });

  it('adds two offers at once as two rows', async () => {
    await mountBridge();
    emit('transfer', offerEvent('a'));
    emit('transfer', offerEvent('b', { collection_name: 'Billing' }));
    expect(store().incomingTransfers.map((t) => [t.transferId, t.collectionName])).toEqual([
      ['a', 'Orders'],
      ['b', 'Billing'],
    ]);
  });

  it('maps progress into the active transfers', async () => {
    await mountBridge();
    emit('transfer', progressEvent('t1', 1024, 'send'));
    emit('transfer', progressEvent('t1', 2048, 'send'));
    expect(store().activeTransfers).toEqual([
      { transferId: 't1', direction: 'send', doneBytes: 2048, totalBytes: 2048 },
    ]);
  });

  it.each([
    ['receive', 'completed', 'success', 'Received "Orders" from Dana Roy.'],
    ['send', 'completed', 'success', 'Sent "Orders" to Dana Roy.'],
    ['send', 'unconfirmed', 'info', 'Sent, but the other node did not confirm.'],
    ['send', 'declined', 'info', 'Dana Roy declined.'],
    ['receive', 'expired', 'info', 'The request expired.'],
    ['send', 'expired', 'info', 'The request expired.'],
    ['send', 'busy', 'error', 'Dana Roy is busy. Try again in a minute.'],
    ['send', 'too_big', 'error', 'This collection is too big to send (limit 10 MiB).'],
    ['send', 'failed', 'error', 'Sending failed. Try again.'],
    ['receive', 'failed', 'error', 'Receiving failed.'],
    ['send', 'damaged', 'error', 'Sending failed. Try again.'],
    ['receive', 'damaged', 'error', 'The collection arrived damaged and was not imported.'],
    ['send', 'rejected', 'error', 'Sending failed. Try again.'],
    [
      'receive',
      'rejected',
      'error',
      'The collection was rejected because it is not safe to import.',
    ],
    ['receive', 'cancelled', 'info', 'Dana Roy cancelled the transfer.'],
    ['send', 'cancelled', 'info', 'Dana Roy cancelled the transfer.'],
  ] as const)('announces %s %s', async (direction, outcome, method, message) => {
    await mountBridge();
    emit('transfer', finishedEvent(outcome, direction));
    expect(mocks.toast[method]).toHaveBeenCalledWith(message, resultId);
    expect(mocks.toast[method]).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['receive', 'declined'],
    ['receive', 'busy'],
    // The backend never emits these two: a refused offer emits nothing, and only a sender is unconfirmed.
    ['receive', 'too_big'],
    ['receive', 'unconfirmed'],
    ['send', 'something-new'],
  ] as const)('says nothing for %s %s', async (direction, outcome) => {
    await mountBridge();
    emit('transfer', finishedEvent(outcome, direction));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(mocks.toast.success).not.toHaveBeenCalled();
    expect(mocks.toast.error).not.toHaveBeenCalled();
  });

  it('tells the sender that the peer declined', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('declined', 'send'));
    expect(mocks.toast.info).toHaveBeenCalledWith('Dana Roy declined.', resultId);
  });

  it('says nothing to the receiver who declined', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    emit('transfer', finishedEvent('declined', 'receive'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
    expect(store().incomingTransfers).toEqual([]);
  });

  it('shows the importer message when a receive fails with one', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'receive', { message: 'The ZIP has no collection.' }));
    expect(mocks.toast.error).toHaveBeenCalledWith('The ZIP has no collection.', resultId);
  });

  it('falls back when the message is null', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'receive', { message: null }));
    expect(mocks.toast.error).toHaveBeenCalledWith('Receiving failed.', resultId);
  });

  it('falls back when the message is blank', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'receive', { message: '  ' }));
    expect(mocks.toast.error).toHaveBeenCalledWith('Receiving failed.', resultId);
  });

  it('ignores the message on a send', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('failed', 'send', { message: 'Internal detail' }));
    expect(mocks.toast.error).toHaveBeenCalledWith('Sending failed. Try again.', resultId);
  });

  it('dismisses the waiting toast when the transfer finishes', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('declined', 'send'));
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-waiting-t1');
  });

  it('dismisses the waiting toast also when the outcome shows nothing', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('declined', 'receive'));
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-waiting-t1');
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it.each(['send', 'receive'] as const)(
    'says nothing when the local user cancelled a %s',
    async (direction) => {
      await mountBridge();
      store().markCancelledHere('t1');
      emit('transfer', finishedEvent('cancelled', direction));
      expect(mocks.toast.info).not.toHaveBeenCalled();
      expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-waiting-t1');
      expect(store().wasCancelledHere('t1')).toBe(false);
    },
  );

  it('switches an open pending offer to the offline text when the sender goes away', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('failed', 'receive', { message: 'The other node went offline.' }));
    expect(store().dialogTransferId).toBe('t1');
    expect(store().dialogEndedText).toBe('The other node went offline.');
    expect(store().incomingTransfers).toEqual([]);
    expect(mocks.toast.error).toHaveBeenCalledWith('The other node went offline.', resultId);
  });

  it('closes the dialog of an accepted transfer that fails with a message', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().markTransferAccepted('t1');
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('failed', 'receive', { message: 'The ZIP has no collection.' }));
    expect(store().dialogTransferId).toBeNull();
  });

  it('switches the waiting toast to sending on the first progress of a send', async () => {
    await mountBridge();
    store().trackOutgoing('t1', 'Dana Roy');
    emit('transfer', progressEvent('t1', 1024, 'send'));
    emit('transfer', progressEvent('t1', 1536, 'send'));
    expect(mocks.toast.info).toHaveBeenCalledTimes(1);
    expect(mocks.toast.info).toHaveBeenCalledWith('Sending to Dana Roy...', {
      id: 'network-transfer-waiting-t1',
      duration: Number.POSITIVE_INFINITY,
    });
  });

  it('drops the Cancel action once the last chunk is out', async () => {
    await mountBridge();
    store().trackOutgoing('t1', 'Dana Roy');
    emit('transfer', progressEvent('t1', 1024, 'send'));
    emit('transfer', progressEvent('t1', 2048, 'send'));
    expect(mocks.toast.info).toHaveBeenCalledTimes(2);
    // Sonner merges options for a reused id, so the action must be overwritten explicitly.
    expect(mocks.toast.info).toHaveBeenLastCalledWith('Sending to Dana Roy...', {
      id: 'network-transfer-waiting-t1',
      duration: Number.POSITIVE_INFINITY,
      action: undefined,
    });
    const last = mocks.toast.info.mock.lastCall?.[1] as Record<string, unknown>;
    expect(Object.prototype.hasOwnProperty.call(last, 'action')).toBe(true);
  });

  it('leaves the toasts alone for a send it did not start or a receive', async () => {
    await mountBridge();
    store().trackOutgoing('t1', 'Dana Roy');
    emit('transfer', progressEvent('t9', 1024, 'send'));
    emit('transfer', progressEvent('t1', 1024, 'receive'));
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('shows the result once, also when the finish event repeats', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed', 'send'));
    emit('transfer', finishedEvent('completed', 'send'));
    expect(mocks.toast.success).toHaveBeenCalledTimes(1);
    expect(mocks.toast.dismiss).toHaveBeenCalledTimes(1);
  });

  it('keeps a double quote in the collection name', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed', 'receive', { collection_name: 'My "Quoted" API' }));
    expect(mocks.toast.success).toHaveBeenCalledWith(
      'Received "My "Quoted" API" from Dana Roy.',
      resultId,
    );
  });

  it('names a sender without a display name', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed', 'receive', { display_name: '  ' }));
    expect(mocks.toast.success).toHaveBeenCalledWith(
      'Received "Orders" from Unnamed node.',
      resultId,
    );
  });

  it('a finish removes the offer and the progress of that transfer only', async () => {
    await mountBridge();
    emit('transfer', offerEvent('t1'));
    emit('transfer', offerEvent('t2'));
    emit('transfer', progressEvent('t1'));
    emit('transfer', finishedEvent('completed'));
    expect(store().incomingTransfers.map((t) => t.transferId)).toEqual(['t2']);
    expect(store().activeTransfers).toEqual([]);
  });

  it('closes the dialog of a transfer that failed', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('failed'));
    expect(store().dialogTransferId).toBeNull();
    expect(mocks.toast.error).toHaveBeenCalledWith('Receiving failed.', resultId);
  });

  it('keeps an open dialog on expired, so the dialog can say so', async () => {
    await mountBridge();
    emit('transfer', offerEvent());
    store().openTransferDialog('t1');
    emit('transfer', finishedEvent('expired'));
    expect(store().dialogTransferId).toBe('t1');
    expect(store().incomingTransfers).toEqual([]);
  });

  it('a finish does not close the dialog of another transfer', async () => {
    await mountBridge();
    store().openTransferDialog('other');
    emit('transfer', finishedEvent('completed'));
    expect(store().dialogTransferId).toBe('other');
  });

  it('ignores progress that arrives after the finish', async () => {
    await mountBridge();
    emit('transfer', finishedEvent('completed'));
    emit('transfer', progressEvent());
    expect(store().activeTransfers).toEqual([]);
  });

  it('ignores transfer events after unmount', async () => {
    const view = await mountBridge();
    view.unmount();
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(5));
    emit('transfer', offerEvent());
    emit('transfer', finishedEvent('completed'));
    expect(store().incomingTransfers).toEqual([]);
    expect(mocks.toast.success).not.toHaveBeenCalled();
  });

  it('stops a transfer listener that finishes registering after unmount', async () => {
    const late = createDeferred<() => void>();
    const lateUnlisten = vi.fn();
    mocks.api.onNetworkTransfer.mockImplementationOnce(() => late.promise);
    const view = renderHook(() => useNetworkEventBridge());
    view.unmount();
    late.resolve(lateUnlisten);
    await waitFor(() => expect(lateUnlisten).toHaveBeenCalledTimes(1));
  });

  it('ignores sync events after unmount', async () => {
    const view = await mountBridge();
    view.unmount();
    emit('sync', { type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 2 });
    expect(store().syncProgress).toHaveLength(0);
  });

  it('stops a sync listener that finishes registering after unmount', async () => {
    const late = createDeferred<() => void>();
    const lateUnlisten = vi.fn();
    mocks.api.onNetworkSync.mockImplementationOnce(() => late.promise);
    const view = renderHook(() => useNetworkEventBridge());
    view.unmount();
    late.resolve(lateUnlisten);
    await waitFor(() => expect(lateUnlisten).toHaveBeenCalledTimes(1));
  });
});
