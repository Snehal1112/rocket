import { createDefaultRequestFor } from '@/lib/pane-utils';
import { fromPersistedAuth, toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import type { WebSocketRequest } from '@/lib/tauri-api';
import { createDefaultWebSocketDraft, normalizeSelection } from '@/lib/websocket-messages';
import type { RequestState, RequestTab } from '@/types/pane-types';

/** A blank WebSocket tab state, for new and unsaved tabs. */
export function createDefaultWebSocketRequestState(url = ''): RequestState {
  return { ...createDefaultRequestFor('websocket'), url };
}

/** Maps a saved WebSocket item to the frontend request state. */
export function mapWebSocketToState(ws: WebSocketRequest): RequestState {
  return {
    ...createDefaultRequestFor('websocket'),
    requestType: 'websocket',
    url: ws.url,
    headers: ws.headers.map((h) => ({
      id: crypto.randomUUID(),
      key: h.key,
      value: h.value,
      enabled: h.enabled,
    })),
    auth: fromPersistedAuth(ws.auth, 'inherit'),
    tags: ws.tags ?? [],
    docs: ws.docs ?? null,
    websocket: {
      messages: normalizeSelection(
        ws.messages.map((m) => ({
          id: crypto.randomUUID(),
          title: m.title,
          selected: m.selected,
          kind: m.kind,
          data: m.data,
        })),
      ),
      timeoutMs: ws.settings?.timeout ?? 'inherit',
      keepAliveMs: ws.settings?.keepAliveInterval ?? 'inherit',
      passthrough: {
        description: ws.description,
        seq: ws.seq,
        runtimeAuth: ws.runtimeAuth,
        scripts: ws.scripts,
      },
    },
  };
}

/** Maps request state to the payload of `save_websocket_request`. */
export function toApiWebSocketRequest(
  uid: string,
  name: string,
  request: RequestState,
  fileName?: string,
): WebSocketRequest {
  const draft = request.websocket ?? createDefaultWebSocketDraft();
  const extra = draft.passthrough;
  const inheritsAll = draft.timeoutMs === 'inherit' && draft.keepAliveMs === 'inherit';
  return {
    uid,
    name,
    url: request.url,
    headers: toPersistedHeaders(request.headers),
    messages: draft.messages.map((m) => ({
      title: m.title,
      selected: m.selected,
      kind: m.kind,
      data: m.data,
    })),
    auth: toPersistedAuth(request.auth),
    tags: request.tags.length > 0 ? request.tags : undefined,
    docs: request.docs ?? null,
    description: extra.description,
    seq: extra.seq,
    runtimeAuth: extra.runtimeAuth,
    scripts: extra.scripts,
    settings: inheritsAll
      ? undefined
      : { timeout: draft.timeoutMs, keepAliveInterval: draft.keepAliveMs },
    ...(fileName !== undefined ? { fileName } : {}),
  };
}

/** The save payload for a WebSocket tab. Same override shape as Plan 05's GraphQL builder. */
export function buildWebSocketSavePayload(
  tab: RequestTab,
  overrides?: { name?: string; fileName?: string },
): WebSocketRequest {
  return toApiWebSocketRequest(
    tab.id || crypto.randomUUID(),
    overrides?.name ?? tab.title,
    tab.request,
    overrides?.fileName,
  );
}

/** A request tab for a saved WebSocket item. The tab id is the item uid. */
export function webSocketToTab(ws: WebSocketRequest, collection: string, path: string): RequestTab {
  return {
    id: ws.uid,
    title: ws.name,
    tabType: 'request',
    request: mapWebSocketToState(ws),
    response: null,
    isDirty: false,
    source: { collection, path },
  };
}

/** Blank or invalid input means "inherit"; otherwise a non-negative number of milliseconds. */
export function parseOptionalMs(text: string): number | 'inherit' {
  const trimmed = text.trim();
  if (trimmed === '') return 'inherit';
  const value = Number(trimmed);
  return Number.isFinite(value) && value >= 0 ? value : 'inherit';
}
