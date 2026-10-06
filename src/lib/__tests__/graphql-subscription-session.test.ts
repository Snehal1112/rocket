import { QueryClient } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  buildSubscribeInput,
  graphqlSendMode,
  releaseGraphQlSubscriptionTab,
  startSubscription,
  stopSubscription,
} from '@/lib/graphql-subscription-session';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import { setQueryClient } from '@/lib/query-client';
import * as tauriApi from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    graphqlSubscribe: vi.fn(),
    graphqlUnsubscribe: vi.fn(),
  };
});

function gqlTab(): RequestTab {
  const request = createDefaultRequestFor('graphql');
  request.url = 'https://api.example.com/graphql';
  request.headers = [
    { id: 'h1', key: 'X-Token', value: '{{token}}', enabled: true },
    { id: 'h2', key: '', value: 'draft', enabled: true },
  ];
  request.auth = { authType: 'bearer', bearer: { token: 'abc' } };
  request.graphql = {
    query: 'subscription OnN { n }',
    variables: '{"room":"general"}',
    operationName: 'OnN',
    connectionParams: '{"token":"abc"}',
  };
  return {
    id: 'tab-1',
    title: 'Updates',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
    source: { collection: 'my-api', path: 'updates.yml' },
  };
}

beforeEach(() => {
  setQueryClient(new QueryClient());
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
  vi.mocked(tauriApi.graphqlSubscribe).mockReset().mockResolvedValue(undefined);
  vi.mocked(tauriApi.graphqlUnsubscribe).mockReset().mockResolvedValue(undefined);
});

describe('buildSubscribeInput', () => {
  it('sends the query, variables, operation, params, headers, auth and scope', () => {
    const input = buildSubscribeInput(gqlTab());
    expect(input.url).toBe('https://api.example.com/graphql');
    expect(input.query).toBe('subscription OnN { n }');
    expect(input.variables).toBe('{"room":"general"}');
    expect(input.operationName).toBe('OnN');
    expect(input.connectionParams).toBe('{"token":"abc"}');
    expect(input.headers).toEqual([{ key: 'X-Token', value: '{{token}}', enabled: true }]);
    expect(input.auth).toEqual({ authType: 'bearer', token: 'abc' });
    expect(input.collection).toBe('my-api');
    expect(input.requestPath).toBe('updates.yml');
    expect(input.verifySsl).toBe(true);
  });

  it('omits blank variables and params', () => {
    const tab = gqlTab();
    tab.request.graphql = { query: 'subscription { n }', variables: '  ', connectionParams: ' ' };
    const input = buildSubscribeInput(tab);
    expect(input.variables).toBeUndefined();
    expect(input.connectionParams).toBeUndefined();
  });
});

describe('startSubscription', () => {
  it('registers the session before the call, so early events are routed', async () => {
    let registeredWhenInvoked: string | null = null;
    vi.mocked(tauriApi.graphqlSubscribe).mockImplementation(async (sessionId) => {
      registeredWhenInvoked = useWebSocketStore.getState().tabBySession[sessionId] ?? null;
    });

    await startSubscription(gqlTab());

    expect(registeredWhenInvoked).toBe('tab-1');
  });

  it('shows a rejected start through the store instead of throwing', async () => {
    vi.mocked(tauriApi.graphqlSubscribe).mockRejectedValue('connection failed: refused');
    await startSubscription(gqlTab());

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('failed');
    expect(session.error).toBe('connection failed: refused');
  });

  it('does not start a second subscription while one is connecting or open', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'existing');
    await startSubscription(gqlTab());
    expect(tauriApi.graphqlSubscribe).not.toHaveBeenCalled();
  });
});

describe('stop and release', () => {
  it('stopSubscription asks the backend to end the live session', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    await stopSubscription('tab-1');
    expect(tauriApi.graphqlUnsubscribe).toHaveBeenCalledWith('sess-1');
  });

  it('releasing a graphql tab ends its subscription and forgets its state', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');

    releaseGraphQlSubscriptionTab(gqlTab());

    expect(tauriApi.graphqlUnsubscribe).toHaveBeenCalledWith('sess-1');
    expect(useWebSocketStore.getState().byTab['tab-1']).toBeUndefined();
  });

  it('releasing an http tab does nothing', () => {
    const http: RequestTab = { ...gqlTab(), request: createDefaultRequestFor('http') };
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    releaseGraphQlSubscriptionTab(http);
    expect(tauriApi.graphqlUnsubscribe).not.toHaveBeenCalled();
  });
});

describe('graphqlSendMode', () => {
  it('subscribes for a subscription operation and sends for anything else', () => {
    expect(graphqlSendMode(true, 'subscription', 'idle')).toBe('subscribe');
    expect(graphqlSendMode(true, 'query', 'idle')).toBe('send');
    expect(graphqlSendMode(true, null, 'closed')).toBe('send');
    expect(graphqlSendMode(false, 'subscription', 'idle')).toBe('send');
  });

  it('offers Stop while a stream is live, even after the operation was edited to a query', () => {
    expect(graphqlSendMode(true, 'subscription', 'open')).toBe('stop');
    expect(graphqlSendMode(true, 'query', 'open')).toBe('stop');
    expect(graphqlSendMode(true, null, 'connecting')).toBe('stop');
    expect(graphqlSendMode(false, null, 'open')).toBe('send');
  });
});
