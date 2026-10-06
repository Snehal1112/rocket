import { beforeEach, describe, expect, it } from 'vitest';
import { MAX_LOG_ENTRIES, useWebSocketStore } from '../websocket-store';

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('websocket-store appendEntry', () => {
  it('appends a labelled entry to the tab that owns the session', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().appendEntry('sess-1', {
      direction: 'in',
      label: 'next',
      kind: 'text',
      data: '{ "n": 1 }',
      size: 10,
      timestampMs: 5,
    });

    const log = useWebSocketStore.getState().byTab['tab-1'].log;
    expect(log).toHaveLength(1);
    expect(log[0]).toMatchObject({ direction: 'in', label: 'next', data: '{ "n": 1 }', size: 10 });
    expect(log[0].id).toBeTruthy();
  });

  it('ignores a session that is unknown or already finished', () => {
    useWebSocketStore.getState().appendEntry('nobody', {
      direction: 'in',
      kind: 'text',
      data: 'x',
      size: 1,
      timestampMs: 1,
    });
    expect(useWebSocketStore.getState().byTab).toEqual({});
  });

  it('keeps the log bounded like the other entries do', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    for (let i = 0; i < MAX_LOG_ENTRIES + 3; i++) {
      useWebSocketStore.getState().appendEntry('sess-1', {
        direction: 'in',
        kind: 'text',
        data: `m${i}`,
        size: 2,
        timestampMs: i,
      });
    }
    const log = useWebSocketStore.getState().byTab['tab-1'].log;
    expect(log).toHaveLength(MAX_LOG_ENTRIES);
    expect(log[0].data).toBe('m3');
  });
});
