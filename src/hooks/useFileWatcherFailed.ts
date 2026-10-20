import { useEffect } from 'react';
import { toast } from 'sonner';
import { onFileWatcherFailed } from '@/lib/tauri-api';

// Tells the user when the file watcher could not start. Mount once.
export function useFileWatcherFailed(): void {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    onFileWatcherFailed((message) => {
      toast.error('Automatic refresh is off', {
        description: `The file watcher could not start (${message}). Reload the sidebar manually.`,
      });
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
