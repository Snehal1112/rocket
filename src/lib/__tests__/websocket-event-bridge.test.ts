import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WebSocketMessageEvent, WebSocketStatusEvent } from '@/lib/tauri-api';
import { useWebSocketEventBridge } from '@/lib/websocket-event-bridge';
import { useWebSocketStore } from '@/stores/websocket-store';

let messageHandler: ((e: WebSocketMessageEvent) => void) | undefined;
let statusHandler: ((e: WebSocketStatusEvent) => void) | undefined;
const unlisten = vi.fn();

vi.mock('@/lib/tauri-api', () => ({
  onWebSocketMessage: vi.fn((h: (e: WebSocketMessageEvent) => void) => {
    messageHandler = h;
    return Promise.resolve(unlisten);
  }),
  onWebSocketStatus: vi.fn((h: (e: WebSocketStatusEvent) => void) => {
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

describe('useWebSocketEventBridge', () => {
  it('routes message and status events into the store by session id', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    renderHook(() => useWebSocketEventBridge());

    statusHandler?.({
      type: 'webSocketStatus',
      session_id: 'sess-1',
      state: 'open',
      subprotocol: null,
      code: null,
      reason: null,
    });
    messageHandler?.({
      type: 'webSocketMessage',
      session_id: 'sess-1',
      direction: 'in',
      kind: 'text',
      data: 'hi',
      size: 2,
      timestamp_ms: 5,
    });

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('open');
    expect(session.log.map((e) => e.data)).toEqual(['Connected', 'hi']);
  });

  it('unsubscribes both listeners on unmount', async () => {
    const { unmount } = renderHook(() => useWebSocketEventBridge());
    unmount();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });
});
