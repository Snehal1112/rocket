import { listen } from '@tauri-apps/api/event';
import { useEffect } from 'react';
import { type BackendLogPayload, useConsoleStore } from '@/stores/console-store';

export const BACKEND_LOG_EVENT = 'backend-log';

// Forwards backend tracing events into the console store. Mount once.
// Events emitted before the listener is registered are not replayed.
export function useBackendLogs(): void {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    listen<BackendLogPayload>(BACKEND_LOG_EVENT, (event) => {
      useConsoleStore.getState().addLogEntry(event.payload);
    })
      .then((fn) => {
        // The component may unmount before the promise resolves.
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
    };
  }, []);
}
