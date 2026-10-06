import { beforeEach, describe, expect, it } from 'vitest';
import type { WebSocketMessageEvent, WebSocketStatusEvent } from '@/lib/tauri-api';
import { appendCapped, MAX_LOG_ENTRIES, useWebSocketStore } from '../websocket-store';

const message = (
  session: string,
  data: string,
  direction: 'in' | 'out' = 'in',
): WebSocketMessageEvent => ({
  type: 'webSocketMessage',
  session_id: session,
  direction,
  kind: 'text',
  data,
  size: data.length,
  timestamp_ms: 1000,
});

const status = (
  session: string,
  state: WebSocketStatusEvent['state'],
  extra: Partial<WebSocketStatusEvent> = {},
): WebSocketStatusEvent => ({
  type: 'webSocketStatus',
  session_id: session,
  state,
  subprotocol: null,
  code: null,
  reason: null,
  ...extra,
});

const tab = () => useWebSocketStore.getState().byTab['tab-1'];

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('websocket-store', () => {
  it('routes events that arrive before the connect call returns', () => {
    const s = useWebSocketStore.getState();
    s.beginSession('tab-1', 'sess-1');
    // The backend publishes Open and the first frame before `ws_connect` resolves.
    useWebSocketStore
      .getState()
      .applyStatus(status('sess-1', 'open', { subprotocol: 'graphql-ws' }));
    useWebSocketStore.getState().applyMessage(message('sess-1', 'hello'));

    expect(tab().status).toBe('open');
    expect(tab().subprotocol).toBe('graphql-ws');
    expect(tab().log.map((e) => [e.direction, e.data])).toEqual([
      ['system', 'Connected (graphql-ws)'],
      ['in', 'hello'],
    ]);
  });

  it('ignores events for unknown or finished sessions', () => {
    useWebSocketStore.getState().applyMessage(message('nobody', 'x'));
    expect(useWebSocketStore.getState().byTab).toEqual({});

    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'closed', { code: 1000 }));
    const before = tab().log.length;
    useWebSocketStore.getState().applyMessage(message('sess-1', 'late'));
    expect(tab().log).toHaveLength(before);
    expect(tab().sessionId).toBeNull();
  });

  it('a terminal status records the reason and frees the session id', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore
      .getState()
      .applyStatus(status('sess-1', 'failed', { reason: 'handshake rejected with HTTP 401' }));

    expect(tab().status).toBe('failed');
    expect(tab().error).toBe('handshake rejected with HTTP 401');
    expect(tab().sessionId).toBeNull();
    expect(tab().log[tab().log.length - 1]?.data).toBe('Failed: handshake rejected with HTTP 401');
    expect(useWebSocketStore.getState().tabBySession['sess-1']).toBeUndefined();
  });

  it('a clean close logs the code and reason', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore
      .getState()
      .applyStatus(status('sess-1', 'closed', { code: 4001, reason: 'bye' }));
    expect(tab().status).toBe('closed');
    expect(tab().log[tab().log.length - 1]?.data).toBe('Closed 4001: bye');
  });

  it('failSession reports a rejected connect once, even if the failed event also arrives', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().failSession('tab-1', 'sess-1', 'boom');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'failed', { reason: 'boom' }));

    expect(tab().status).toBe('failed');
    expect(tab().log.filter((e) => e.data.startsWith('Failed')).length).toBe(1);
  });

  it('failSession does nothing once the event already marked the session failed', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyStatus(status('sess-1', 'failed', { reason: 'boom' }));
    useWebSocketStore.getState().failSession('tab-1', 'sess-1', 'boom');
    expect(tab().log.filter((e) => e.data.startsWith('Failed')).length).toBe(1);
  });

  it('a new session replaces the old mapping but keeps the log', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyMessage(message('sess-1', 'one'));
    useWebSocketStore.getState().beginSession('tab-1', 'sess-2');

    expect(tab().sessionId).toBe('sess-2');
    expect(tab().status).toBe('connecting');
    expect(tab().log).toHaveLength(1);
    useWebSocketStore.getState().applyMessage(message('sess-1', 'stale'));
    expect(tab().log).toHaveLength(1);
  });

  it('caps the log and drops the oldest entries', () => {
    expect(appendCapped([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);

    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    for (let i = 0; i < MAX_LOG_ENTRIES + 5; i++) {
      useWebSocketStore.getState().applyMessage(message('sess-1', `m${i}`));
    }
    expect(tab().log).toHaveLength(MAX_LOG_ENTRIES);
    expect(tab().log[0].data).toBe('m5');
  });

  it('clearLog empties only the log and forgetTab removes everything for the tab', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyMessage(message('sess-1', 'one'));
    useWebSocketStore.getState().clearLog('tab-1');
    expect(tab().log).toEqual([]);
    expect(tab().sessionId).toBe('sess-1');

    useWebSocketStore.getState().forgetTab('tab-1');
    expect(useWebSocketStore.getState().byTab['tab-1']).toBeUndefined();
    expect(useWebSocketStore.getState().tabBySession['sess-1']).toBeUndefined();
  });

  it('keeps outgoing messages as out entries with their size', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().applyMessage(message('sess-1', 'ping', 'out'));
    expect(tab().log[0]).toMatchObject({ direction: 'out', size: 4, timestampMs: 1000 });
  });

  it("a late rejection from an old connect never fails the tab's newer session", () => {
    useWebSocketStore.getState().beginSession('tab-1', 'old');
    useWebSocketStore.getState().beginSession('tab-1', 'new');
    useWebSocketStore.getState().failSession('tab-1', 'old', 'cancelled');

    expect(tab().status).toBe('connecting');
    expect(tab().sessionId).toBe('new');
    expect(useWebSocketStore.getState().tabBySession.new).toBe('tab-1');
  });
});
