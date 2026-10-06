import { useEffect } from 'react';
import { onWebSocketMessage, onWebSocketStatus } from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';

// Subscribes once, for the app's lifetime, to the WebSocket events and routes each one into the
// store by session id. It lives outside the panel so frames are never dropped while the panel
// of the owning tab is unmounted (only the active tab of a pane is mounted).
export function useWebSocketEventBridge(): void {
  useEffect(() => {
    const unsubs = Promise.all([
      onWebSocketMessage((e) => useWebSocketStore.getState().applyMessage(e)),
      onWebSocketStatus((e) => useWebSocketStore.getState().applyStatus(e)),
    ]);
    return () => {
      unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
