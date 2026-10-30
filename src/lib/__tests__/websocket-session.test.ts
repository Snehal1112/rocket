import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'sonner';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { createDefaultWebSocketRequestState } from '@/lib/websocket-mapper';
import {
  buildConnectInput,
  connectTab,
  disconnectTab,
  releaseWebSocketTab,
  sendSelectedMessage,
} from '@/lib/websocket-session';
import { useEnvStore } from '@/stores/env-store';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    wsConnect: vi.fn(),
    wsSend: vi.fn(),
    wsDisconnect: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), warning: vi.fn() } }));

function wsTab(): RequestTab {
  const request = createDefaultWebSocketRequestState('wss://echo.example.com/ws');
  request.headers = [
    { id: 'h1', key: 'X-Token', value: '{{token}}', enabled: true },
    { id: 'h2', key: 'X-Off', value: '1', enabled: false },
    { id: 'h3', key: '', value: 'draft', enabled: true },
  ];
  request.auth = { authType: 'bearer', bearer: { token: 'abc' } };
  if (request.websocket) {
    request.websocket.timeoutMs = 5000;
    request.websocket.keepAliveMs = 'inherit';
    request.websocket.messages[0].kind = 'json';
    request.websocket.messages[0].data = '{"a":"{{token}}"}';
  }
  return {
    id: 'tab-1',
    title: 'Chat',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
    source: { collection: 'my-api', path: 'chat.yml' },
  };
}

function historyTab(historyCollection: string | null): RequestTab {
  return {
    ...wsTab(),
    id: 'history-h1',
    tabType: 'history',
    source: undefined,
    historyCollection,
  };
}

beforeEach(() => {
  setQueryClient(new QueryClient());
  vi.mocked(toast.warning).mockClear();
  useEnvStore.setState({ activeCollection: null, activeEnvId: null });
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
  vi.mocked(tauriApi.wsConnect).mockReset().mockResolvedValue(undefined);
  vi.mocked(tauriApi.wsSend).mockReset().mockResolvedValue(undefined);
  vi.mocked(tauriApi.wsDisconnect).mockReset().mockResolvedValue(undefined);
});

describe('buildConnectInput', () => {
  it('sends the url, saved headers, auth, scope and settings', () => {
    const input = buildConnectInput(wsTab());
    expect(input.url).toBe('wss://echo.example.com/ws');
    expect(input.headers).toEqual([
      { key: 'X-Token', value: '{{token}}', enabled: true },
      { key: 'X-Off', value: '1', enabled: false },
    ]);
    expect(input.auth).toEqual({ authType: 'bearer', token: 'abc' });
    expect(input.collection).toBe('my-api');
    expect(input.requestPath).toBe('chat.yml');
    expect(input.timeoutMs).toBe(5000);
    expect(input.keepAliveMs).toBeUndefined();
    expect(input.verifySsl).toBe(true);
  });

  it('buildConnectInput sends the saved collection scope', () => {
    useEnvStore.setState({ activeCollection: 'other', activeEnvId: 'dev' });
    expect(buildConnectInput(wsTab()).processEnv).toEqual({ kind: 'collection', name: 'my-api' });
  });

  it('buildConnectInput sends a closed scope for a History tab without collection', () => {
    const input = buildConnectInput(historyTab(null));
    expect(input.processEnv).toEqual({ kind: 'closed' });
    expect(input.collection).toBeUndefined();
  });

  it('buildConnectInput sends the History collection scope', () => {
    const input = buildConnectInput(historyTab('clone'));
    expect(input.processEnv).toEqual({ kind: 'collection', name: 'clone' });
    expect(input.collection).toBeUndefined();
  });

  it('buildConnectInput sends a scratch scope for a scratch tab', () => {
    const input = buildConnectInput({ ...wsTab(), source: undefined });
    expect(input.processEnv).toEqual({ kind: 'scratch' });
  });
});

describe('connectTab', () => {
  it('connectTab warns with the History scope', async () => {
    const tab = historyTab(null);
    tab.request.headers = [{ id: 'h1', key: 'X', value: '{{process.env.HOME}}', enabled: true }];

    await connectTab(tab);

    expect(toast.warning).toHaveBeenCalledTimes(1);
    expect(vi.mocked(toast.warning).mock.calls[0][0]).toContain('reopened from History');
    expect(tauriApi.wsConnect).toHaveBeenCalledTimes(1);
  });

  it('registers the session before the connect call, so early events are routed', async () => {
    let registeredWhenInvoked: string | null = null;
    vi.mocked(tauriApi.wsConnect).mockImplementation(async (sessionId) => {
      registeredWhenInvoked = useWebSocketStore.getState().tabBySession[sessionId] ?? null;
    });

    await connectTab(wsTab());

    expect(registeredWhenInvoked).toBe('tab-1');
  });

  it('marks the tab failed when the connect call rejects', async () => {
    vi.mocked(tauriApi.wsConnect).mockRejectedValue('handshake rejected with HTTP 401');
    await connectTab(wsTab());

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('failed');
    expect(session.error).toBe('handshake rejected with HTTP 401');
  });

  it('does not open a second session while one is connecting or open', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'existing');
    await connectTab(wsTab());
    expect(tauriApi.wsConnect).not.toHaveBeenCalled();
  });
});

describe('sendSelectedMessage', () => {
  it('sends the selected message with its kind and scope', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.setState((s) => ({
      byTab: { ...s.byTab, 'tab-1': { ...s.byTab['tab-1'], status: 'open' } },
    }));

    await sendSelectedMessage(wsTab());

    expect(tauriApi.wsSend).toHaveBeenCalledWith('sess-1', {
      kind: 'json',
      data: '{"a":"{{token}}"}',
      collection: 'my-api',
      environmentName: undefined,
      globalEnvName: undefined,
      requestPath: 'chat.yml',
      processEnv: { kind: 'collection', name: 'my-api' },
    });
  });

  it('sendSelectedMessage sends the History scope', async () => {
    const tab = historyTab(null);
    useWebSocketStore.getState().beginSession(tab.id, 'sess-1');
    useWebSocketStore.setState((s) => ({
      byTab: { ...s.byTab, [tab.id]: { ...s.byTab[tab.id], status: 'open' } },
    }));

    await sendSelectedMessage(tab);

    const input = vi.mocked(tauriApi.wsSend).mock.calls[0][1];
    expect(input.processEnv).toEqual({ kind: 'closed' });
  });

  it('does nothing when the tab is not connected', async () => {
    await sendSelectedMessage(wsTab());
    expect(tauriApi.wsSend).not.toHaveBeenCalled();
  });
});

describe('disconnect and release', () => {
  it('disconnectTab closes the live session', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    await disconnectTab('tab-1');
    expect(tauriApi.wsDisconnect).toHaveBeenCalledWith('sess-1');
  });

  it('releasing a websocket tab disconnects it and forgets its state', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');

    releaseWebSocketTab(wsTab());

    expect(tauriApi.wsDisconnect).toHaveBeenCalledWith('sess-1');
    expect(useWebSocketStore.getState().byTab['tab-1']).toBeUndefined();
  });

  it('releasing an http tab does nothing', () => {
    const http: RequestTab = { ...wsTab(), request: { ...wsTab().request, requestType: 'http' } };
    releaseWebSocketTab(http);
    expect(tauriApi.wsDisconnect).not.toHaveBeenCalled();
  });
});
