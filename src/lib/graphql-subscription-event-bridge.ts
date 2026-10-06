import { useEffect } from 'react';
import { onGraphQlSubscriptionMessage, onGraphQlSubscriptionStatus } from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';

const encoder = new TextEncoder();

// Subscribes once, for the app's lifetime, to the GraphQL subscription events and feeds them to
// the same store the WebSocket tab uses: results become labelled log lines, status changes map
// onto the WebSocket status shape (the dialect plays the part of the subprotocol).
export function useGraphQlSubscriptionEventBridge(): void {
  useEffect(() => {
    const unsubs = Promise.all([
      onGraphQlSubscriptionMessage((e) =>
        useWebSocketStore.getState().appendEntry(e.session_id, {
          direction: 'in',
          label: e.event,
          kind: 'text',
          data: e.data,
          size: encoder.encode(e.data).length,
          timestampMs: e.timestamp_ms,
        }),
      ),
      onGraphQlSubscriptionStatus((e) =>
        useWebSocketStore.getState().applyStatus({
          type: 'webSocketStatus',
          session_id: e.session_id,
          state: e.state,
          subprotocol: e.dialect,
          code: null,
          reason: e.reason,
        }),
      ),
    ]);
    return () => {
      unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
