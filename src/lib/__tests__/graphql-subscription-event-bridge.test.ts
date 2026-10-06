import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useGraphQlSubscriptionEventBridge } from '@/lib/graphql-subscription-event-bridge';
import type {
  GraphQlSubscriptionMessageEvent,
  GraphQlSubscriptionStatusEvent,
} from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';

let messageHandler: ((e: GraphQlSubscriptionMessageEvent) => void) | undefined;
let statusHandler: ((e: GraphQlSubscriptionStatusEvent) => void) | undefined;
const unlisten = vi.fn();

vi.mock('@/lib/tauri-api', () => ({
  onGraphQlSubscriptionMessage: vi.fn((h: (e: GraphQlSubscriptionMessageEvent) => void) => {
    messageHandler = h;
    return Promise.resolve(unlisten);
  }),
  onGraphQlSubscriptionStatus: vi.fn((h: (e: GraphQlSubscriptionStatusEvent) => void) => {
    statusHandler = h;
    return Promise.resolve(unlisten);
  }),
}));

beforeEach(() => {
  messageHandler = undefined;
  statusHandler = undefined;
  unlisten.mockClear();
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('useGraphQlSubscriptionEventBridge', () => {
  it('turns subscription events into log entries and status changes of the owning tab', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    renderHook(() => useGraphQlSubscriptionEventBridge());

    statusHandler?.({
      type: 'graphQlSubscriptionStatus',
      session_id: 'sess-1',
      state: 'open',
      dialect: 'graphql-transport-ws',
      reason: null,
    });
    messageHandler?.({
      type: 'graphQlSubscriptionMessage',
      session_id: 'sess-1',
      event: 'next',
      data: '{ "data": 1 }',
      timestamp_ms: 9,
    });
    messageHandler?.({
      type: 'graphQlSubscriptionMessage',
      session_id: 'sess-1',
      event: 'complete',
      data: '',
      timestamp_ms: 10,
    });

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('open');
    expect(session.subprotocol).toBe('graphql-transport-ws');
    expect(session.log.map((e) => [e.direction, e.label, e.data])).toEqual([
      ['system', undefined, 'Connected (graphql-transport-ws)'],
      ['in', 'next', '{ "data": 1 }'],
      ['in', 'complete', ''],
    ]);
    expect(session.log[1].size).toBe(new TextEncoder().encode('{ "data": 1 }').length);
  });

  it('a failed status records the reason', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    renderHook(() => useGraphQlSubscriptionEventBridge());

    statusHandler?.({
      type: 'graphQlSubscriptionStatus',
      session_id: 'sess-1',
      state: 'failed',
      dialect: null,
      reason: 'the server reported an error',
    });

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('failed');
    expect(session.error).toBe('the server reported an error');
  });

  it('unsubscribes both listeners on unmount', async () => {
    const { unmount } = renderHook(() => useGraphQlSubscriptionEventBridge());
    unmount();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });
});
