import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useNetworkEventBridge } from '@/lib/network-event-bridge';
import { PAIRING_FAILED_TOAST_ID } from '@/lib/network-utils';
import { useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeIncoming, makeNode, makeStatus } from '@/test/network-fixtures';

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
    toast: { info: vi.fn(), success: vi.fn(), error: vi.fn() },
    api: {
      onNetworkChanged: listener('changed'),
      onNetworkPairing: listener('pairing'),
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

function emit(name: 'changed' | 'pairing', payload: unknown): void {
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
  });

  it('loads the status and the nodes once on mount', async () => {
    await mountBridge();
    expect(mocks.api.getNetworkStatus).toHaveBeenCalledTimes(1);
    expect(mocks.api.listNetworkNodes).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkChanged).toHaveBeenCalledTimes(1);
    expect(mocks.api.onNetworkPairing).toHaveBeenCalledTimes(1);
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
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(2));
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
