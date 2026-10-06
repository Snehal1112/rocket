import { beforeEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  handlers: {} as Record<string, (event: unknown) => void>,
  listen: vi.fn(),
}));

vi.mock('@/lib/tauri-api', () => {
  const on = (name: string) =>
    vi.fn(async (handler: (event: unknown) => void) => {
      hoisted.listen(name);
      hoisted.handlers[name] = handler;
      return () => undefined;
    });
  return {
    onGrpcSessionStarted: on('started'),
    onGrpcSessionHeaders: on('headers'),
    onGrpcSessionMessage: on('message'),
    onGrpcSessionFinished: on('finished'),
    grpcCancelSession: vi.fn().mockResolvedValue(undefined),
  };
});

import { grpcCancelSession } from '@/lib/tauri-api';
import { ensureGrpcListeners, MAX_GRPC_LOG_ENTRIES, useGrpcStore } from '../grpc-store';

function emit(name: string, payload: object) {
  hoisted.handlers[name]?.(payload);
}

beforeEach(async () => {
  useGrpcStore.setState({ sessions: {}, sessionByTab: {}, unaryByTab: {} });
  vi.mocked(grpcCancelSession).mockClear();
  await ensureGrpcListeners();
});

describe('ensureGrpcListeners', () => {
  it('subscribes to the four session events exactly once', async () => {
    await ensureGrpcListeners();
    await ensureGrpcListeners();
    expect(hoisted.listen.mock.calls.map(([name]) => name).sort()).toEqual([
      'finished',
      'headers',
      'message',
      'started',
    ]);
  });
});

describe('session events', () => {
  it('builds the view of a session from its events in order', () => {
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 's1',
      method_type: 'bidi-streaming',
    });
    emit('headers', {
      type: 'grpcSessionHeaders',
      session_id: 's1',
      headers: [{ name: 'content-type', value: 'application/grpc' }],
    });
    emit('message', { type: 'grpcSessionMessage', session_id: 's1', index: 0, json: '{"a":1}' });
    emit('message', { type: 'grpcSessionMessage', session_id: 's1', index: 1, json: '{"a":2}' });
    emit('finished', {
      type: 'grpcSessionFinished',
      session_id: 's1',
      code: 0,
      code_name: 'OK',
      message: '',
      trailers: [{ name: 'x-end', value: '1' }],
      duration_ms: 42,
    });

    const s = useGrpcStore.getState().sessions.s1;
    expect(s.methodType).toBe('bidi-streaming');
    expect(s.headers).toEqual([{ name: 'content-type', value: 'application/grpc' }]);
    expect(s.log.map((l) => [l.direction, l.json])).toEqual([
      ['in', '{"a":1}'],
      ['in', '{"a":2}'],
    ]);
    expect(s.status).toBe('finished');
    expect(s.finished).toEqual({
      status: { code: 0, codeName: 'OK', message: '' },
      trailers: [{ name: 'x-end', value: '1' }],
      durationMs: 42,
    });
  });

  it('creates the session from whichever event arrives first', () => {
    emit('message', { type: 'grpcSessionMessage', session_id: 'early', index: 0, json: '{}' });
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 'early',
      method_type: 'server-streaming',
    });
    const s = useGrpcStore.getState().sessions.early;
    expect(s.status).toBe('running');
    expect(s.methodType).toBe('server-streaming');
    expect(s.log).toHaveLength(1);
  });

  it('keeps only the newest entries when a stream runs long', () => {
    for (let i = 0; i < MAX_GRPC_LOG_ENTRIES + 5; i++) {
      emit('message', { type: 'grpcSessionMessage', session_id: 'long', index: i, json: `${i}` });
    }
    const log = useGrpcStore.getState().sessions.long.log;
    expect(log).toHaveLength(MAX_GRPC_LOG_ENTRIES);
    expect(log[log.length - 1].json).toBe(`${MAX_GRPC_LOG_ENTRIES + 4}`);
    expect(log[0].json).toBe('5');
  });
});

describe('store actions', () => {
  it('records a sent message as an outbound entry', () => {
    useGrpcStore.getState().recordOutbound('s2', '{"x":1}');
    const log = useGrpcStore.getState().sessions.s2.log;
    expect(log).toHaveLength(1);
    expect(log[0]).toMatchObject({ direction: 'out', json: '{"x":1}' });
  });

  it('drops the previous finished session of a tab when a new one is attached', () => {
    emit('finished', {
      type: 'grpcSessionFinished',
      session_id: 'old',
      code: 0,
      code_name: 'OK',
      message: '',
      trailers: [],
      duration_ms: 1,
    });
    useGrpcStore.getState().attachSession('tab', 'old');
    useGrpcStore.getState().attachSession('tab', 'new');
    expect(useGrpcStore.getState().sessions.old).toBeUndefined();
    expect(useGrpcStore.getState().sessionByTab.tab).toBe('new');
  });

  it('shows an attached session as running before any event reaches the store', () => {
    useGrpcStore.getState().attachSession('tab', 'fresh');
    expect(useGrpcStore.getState().sessions.fresh.status).toBe('running');
  });

  it('keeps what an early event already recorded when the session is attached', () => {
    emit('message', { type: 'grpcSessionMessage', session_id: 'early', index: 0, json: '{}' });
    useGrpcStore.getState().attachSession('tab', 'early');
    expect(useGrpcStore.getState().sessions.early.log).toHaveLength(1);
  });

  it('cancels only a running session and survives a failed cancel', async () => {
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 'run',
      method_type: 'bidi-streaming',
    });
    useGrpcStore.getState().attachSession('tab', 'run');
    await useGrpcStore.getState().cancelTabSession('tab');
    expect(grpcCancelSession).toHaveBeenCalledWith('run');

    vi.mocked(grpcCancelSession).mockRejectedValueOnce(new Error('gone'));
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    await expect(useGrpcStore.getState().cancelTabSession('tab')).resolves.toBeUndefined();

    vi.mocked(grpcCancelSession).mockClear();
    emit('finished', {
      type: 'grpcSessionFinished',
      session_id: 'run',
      code: 1,
      code_name: 'CANCELLED',
      message: '',
      trailers: [],
      duration_ms: 1,
    });
    await useGrpcStore.getState().cancelTabSession('tab');
    expect(grpcCancelSession).not.toHaveBeenCalled();
  });

  it('forgets a closed tab and cancels its running call first', async () => {
    emit('started', {
      type: 'grpcSessionStarted',
      session_id: 'run',
      method_type: 'bidi-streaming',
    });
    useGrpcStore.getState().attachSession('tab', 'run');
    useGrpcStore.getState().setUnary('tab', { status: 'sending' });
    await useGrpcStore.getState().dropTab('tab');
    expect(grpcCancelSession).toHaveBeenCalledWith('run');
    const s = useGrpcStore.getState();
    expect(s.sessions.run).toBeUndefined();
    expect(s.sessionByTab.tab).toBeUndefined();
    expect(s.unaryByTab.tab).toBeUndefined();
  });

  it("forgets a session that never opened, but only while it is still the tab's session", () => {
    useGrpcStore.getState().attachSession('tab', 'never-opened');
    useGrpcStore.getState().detachSession('tab', 'never-opened');
    expect(useGrpcStore.getState().sessions['never-opened']).toBeUndefined();
    expect(useGrpcStore.getState().sessionByTab.tab).toBeUndefined();

    useGrpcStore.getState().attachSession('tab', 'first');
    useGrpcStore.getState().attachSession('tab', 'second');
    useGrpcStore.getState().detachSession('tab', 'first');
    expect(useGrpcStore.getState().sessionByTab.tab).toBe('second');
    expect(useGrpcStore.getState().sessions.second).toBeDefined();
  });

  it('clears a unary result when given undefined', () => {
    useGrpcStore.getState().setUnary('tab', { status: 'error', error: 'boom' });
    useGrpcStore.getState().setUnary('tab', undefined);
    expect(useGrpcStore.getState().unaryByTab.tab).toBeUndefined();
  });
});
