import { describe, expect, it } from 'vitest';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import type { WebSocketRequest } from '@/lib/tauri-api';
import {
  buildWebSocketSavePayload,
  createDefaultWebSocketRequestState,
  mapWebSocketToState,
  parseOptionalMs,
  toApiWebSocketRequest,
  webSocketToTab,
} from '@/lib/websocket-mapper';

const saved: WebSocketRequest = {
  uid: 'ws-1',
  name: 'Chat',
  description: { content: '# Chat', type: 'text/markdown' },
  seq: 3,
  tags: ['realtime'],
  url: 'wss://chat.example.com/ws',
  headers: [
    { key: 'Origin', value: 'https://example.com', enabled: true },
    { key: 'X-Off', value: '1', enabled: false },
  ],
  messages: [
    { title: 'hello', selected: false, kind: 'json', data: '{}' },
    { title: 'raw', selected: true, kind: 'binary', data: 'AQID' },
  ],
  auth: { authType: 'bearer', token: 't' },
  runtimeAuth: { authType: 'basic', username: 'u', password: 'p' },
  variables: [{ key: 'room', value: 'general', initialValue: '', enabled: true, secret: false }],
  scripts: [{ scriptType: 'before-request', code: '// pre' }],
  settings: { timeout: 5000, keepAliveInterval: 'inherit' },
  docs: '# Docs',
  fileName: 'chat.yml',
};

describe('websocket mapper', () => {
  it('maps a saved item to request state with the websocket discriminator', () => {
    const state = mapWebSocketToState(saved);
    expect(state.requestType).toBe('websocket');
    expect(state.url).toBe('wss://chat.example.com/ws');
    expect(state.headers.map((h) => [h.key, h.enabled])).toEqual([
      ['Origin', true],
      ['X-Off', false],
    ]);
    expect(state.auth.authType).toBe('bearer');
    expect(state.websocket?.messages.map((m) => [m.title, m.selected])).toEqual([
      ['hello', false],
      ['raw', true],
    ]);
    expect(state.websocket?.timeoutMs).toBe(5000);
    expect(state.websocket?.keepAliveMs).toBe('inherit');
  });

  it('writes back every field the UI does not edit, unchanged', () => {
    const payload = toApiWebSocketRequest('ws-1', 'Chat', mapWebSocketToState(saved));
    expect(payload.description).toEqual(saved.description);
    expect(payload.seq).toBe(3);
    expect(payload.runtimeAuth).toEqual(saved.runtimeAuth);
    expect(payload.scripts).toEqual(saved.scripts);
    expect(payload.docs).toBe('# Docs');
    expect(payload.tags).toEqual(['realtime']);
  });

  it('never sends runtime variables, so a stale copy cannot overwrite edited ones', () => {
    const payload = toApiWebSocketRequest('ws-1', 'Chat', mapWebSocketToState(saved));
    expect(payload.variables).toBeUndefined();
  });

  it('round-trips headers, messages, auth and settings', () => {
    const payload = toApiWebSocketRequest('ws-1', 'Chat', mapWebSocketToState(saved));
    expect(payload.headers).toEqual(saved.headers);
    expect(payload.messages).toEqual(saved.messages);
    expect(payload.auth).toEqual(saved.auth);
    expect(payload.settings).toEqual({ timeout: 5000, keepAliveInterval: 'inherit' });
  });

  it('omits settings when both values inherit', () => {
    const state = mapWebSocketToState({ ...saved, settings: undefined });
    expect(toApiWebSocketRequest('ws-1', 'Chat', state).settings).toBeUndefined();
  });

  it('normalizes a saved item with no selected message', () => {
    const state = mapWebSocketToState({
      ...saved,
      messages: saved.messages.map((m) => ({ ...m, selected: false })),
    });
    expect(state.websocket?.messages.map((m) => m.selected)).toEqual([true, false]);
  });

  it('a new default state is a websocket request with one selected message', () => {
    const state = createDefaultWebSocketRequestState('wss://x');
    expect(state.requestType).toBe('websocket');
    expect(state.url).toBe('wss://x');
    expect(state.websocket?.messages).toHaveLength(1);
    expect(state.websocket?.messages[0].selected).toBe(true);
    expect(state.websocket?.timeoutMs).toBe('inherit');
  });

  it('createDefaultRequestFor(websocket) carries a draft (new ephemeral tabs use it)', () => {
    const state = createDefaultRequestFor('websocket');
    expect(state.requestType).toBe('websocket');
    expect(state.websocket?.messages).toHaveLength(1);
  });

  it('builds a request tab keyed by uid with its source', () => {
    const tab = webSocketToTab(saved, 'my-api', 'chat.yml');
    expect(tab).toMatchObject({
      id: 'ws-1',
      title: 'Chat',
      tabType: 'request',
      isDirty: false,
      source: { collection: 'my-api', path: 'chat.yml' },
    });
    expect(tab.request.requestType).toBe('websocket');
  });

  it('buildWebSocketSavePayload honours the name and file name overrides', () => {
    const tab = webSocketToTab(saved, 'my-api', 'chat.yml');
    const payload = buildWebSocketSavePayload(tab, { name: 'Renamed', fileName: 'chat2' });
    expect(payload).toMatchObject({ uid: 'ws-1', name: 'Renamed', fileName: 'chat2' });
    expect(buildWebSocketSavePayload(tab).name).toBe('Chat');
  });

  it('parseOptionalMs maps blank to inherit and rejects negatives', () => {
    expect(parseOptionalMs('')).toBe('inherit');
    expect(parseOptionalMs('  ')).toBe('inherit');
    expect(parseOptionalMs('2500')).toBe(2500);
    expect(parseOptionalMs('0')).toBe(0);
    expect(parseOptionalMs('-5')).toBe('inherit');
    expect(parseOptionalMs('abc')).toBe('inherit');
  });
});
