import { toast } from 'sonner';
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import { environmentKeys } from '@/lib/queries/environment-queries';
import { getQueryClient } from '@/lib/query-client';
import {
  type WebSocketConnectInput,
  type WebSocketScopeInput,
  wsConnect,
  wsDisconnect,
  wsSend,
} from '@/lib/tauri-api';
import { selectedMessage } from '@/lib/websocket-messages';
import { useEnvStore } from '@/stores/env-store';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab, Tab } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// Where {{variables}} come from. Read from the stores at call time, like the HTTP send path.
function scopeFor(tab: RequestTab): WebSocketScopeInput {
  return {
    collection: tab.source?.collection,
    environmentName: useEnvStore.getState().activeEnvId ?? undefined,
    globalEnvName:
      getQueryClient().getQueryData<string | null>(environmentKeys.globalName) ?? undefined,
    requestPath: tab.source?.path,
  };
}

/** The `ws_connect` input for a tab: raw values, the backend resolves variables. */
export function buildConnectInput(tab: RequestTab): WebSocketConnectInput {
  const { request } = tab;
  const draft = request.websocket;
  return {
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    auth: toPersistedAuth(request.auth),
    timeoutMs: typeof draft?.timeoutMs === 'number' ? draft.timeoutMs : undefined,
    keepAliveMs: typeof draft?.keepAliveMs === 'number' ? draft.keepAliveMs : undefined,
    verifySsl: request.settings.verifySsl,
    ...scopeFor(tab),
  };
}

/**
 * Opens a session for the tab. The session id is registered in the store BEFORE the invoke,
 * because the backend publishes its first events before `ws_connect` resolves. A rejected
 * connect is shown through the store, not thrown.
 */
export async function connectTab(tab: RequestTab): Promise<void> {
  const current = useWebSocketStore.getState().byTab[tab.id];
  if (current && (current.status === 'connecting' || current.status === 'open')) return;

  const sessionId = crypto.randomUUID();
  useWebSocketStore.getState().beginSession(tab.id, sessionId);
  try {
    await wsConnect(sessionId, buildConnectInput(tab));
  } catch (err) {
    useWebSocketStore.getState().failSession(tab.id, errorText(err));
  }
}

/** Sends the tab's selected message on its open session. */
export async function sendSelectedMessage(tab: RequestTab): Promise<void> {
  const session = useWebSocketStore.getState().byTab[tab.id];
  if (!session?.sessionId || session.status !== 'open') return;
  const message = selectedMessage(tab.request.websocket?.messages ?? []);
  if (!message) return;
  try {
    await wsSend(session.sessionId, { kind: message.kind, data: message.data, ...scopeFor(tab) });
  } catch (err) {
    toast.error(`Could not send: ${errorText(err)}`);
  }
}

/** Asks the backend to close the tab's live session. The final status arrives as an event. */
export async function disconnectTab(tabId: string): Promise<void> {
  const sessionId = useWebSocketStore.getState().byTab[tabId]?.sessionId;
  if (!sessionId) return;
  try {
    await wsDisconnect(sessionId);
  } catch (err) {
    console.error('[websocket] disconnect failed:', err);
  }
}

/** Called when a tab is about to be discarded: closes its socket and forgets its state. */
export function releaseWebSocketTab(tab: Tab): void {
  if (!isRequestTab(tab) || tab.request.requestType !== 'websocket') return;
  void disconnectTab(tab.id);
  useWebSocketStore.getState().forgetTab(tab.id);
}
