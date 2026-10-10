import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import { warnIfProcessEnvWithheld } from '@/lib/process-env-gate';
import { type GraphQlSubscribeInput, graphqlSubscribe, graphqlUnsubscribe } from '@/lib/tauri-api';
import { scopeFor } from '@/lib/websocket-session';
import { type ConnectionStatus, useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab, Tab } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The `graphql_subscribe` input for a tab: raw values, the backend resolves variables. */
export function buildSubscribeInput(tab: RequestTab): GraphQlSubscribeInput {
  const { request } = tab;
  const gql = request.graphql ?? { query: '', variables: '' };
  return {
    url: request.url,
    query: gql.query,
    variables: gql.variables.trim() === '' ? undefined : gql.variables,
    operationName: gql.operationName,
    connectionParams: gql.connectionParams?.trim() ? gql.connectionParams : undefined,
    headers: toPersistedHeaders(request.headers),
    auth: toPersistedAuth(request.auth),
    verifySsl: request.settings.verifySsl,
    timeoutMs: request.settings.timeoutMs > 0 ? request.settings.timeoutMs : undefined,
    ...scopeFor(tab),
  };
}

/**
 * Starts a subscription for the tab. The session id is registered in the store BEFORE the call,
 * because the backend publishes its first events before `graphql_subscribe` resolves. A rejected
 * start is shown through the store, not thrown.
 */
export async function startSubscription(tab: RequestTab): Promise<void> {
  const current = useWebSocketStore.getState().byTab[tab.id];
  if (current && (current.status === 'connecting' || current.status === 'open')) return;

  const sessionId = crypto.randomUUID();
  useWebSocketStore.getState().beginSession(tab.id, sessionId);
  try {
    const input = buildSubscribeInput(tab);
    await warnIfProcessEnvWithheld(input.collection, [input], tab.title);
    await graphqlSubscribe(sessionId, input);
  } catch (err) {
    useWebSocketStore.getState().failSession(tab.id, sessionId, errorText(err));
  }
}

/** Asks the backend to end the tab's live subscription. The final status arrives as an event. */
export async function stopSubscription(tabId: string): Promise<void> {
  const sessionId = useWebSocketStore.getState().byTab[tabId]?.sessionId;
  if (!sessionId) return;
  try {
    await graphqlUnsubscribe(sessionId);
  } catch (err) {
    console.error('[graphql-subscription] stop failed:', err);
  }
}

/** Called when a tab is about to be discarded: ends its subscription and forgets its state. */
export function releaseGraphQlSubscriptionTab(tab: Tab): void {
  if (!isRequestTab(tab) || tab.request.requestType !== 'graphql') return;
  void stopSubscription(tab.id);
  useWebSocketStore.getState().forgetTab(tab.id);
}

export type GraphQlSendMode = 'send' | 'subscribe' | 'stop';

/**
 * What the Send button of a GraphQL tab does. A live stream always offers Stop, whatever the
 * document says now, so editing the operation never strands a running subscription.
 */
export function graphqlSendMode(
  isGraphQl: boolean,
  operationKind: string | null,
  status: ConnectionStatus,
): GraphQlSendMode {
  if (isGraphQl && (status === 'connecting' || status === 'open')) return 'stop';
  if (isGraphQl && operationKind === 'subscription') return 'subscribe';
  return 'send';
}
