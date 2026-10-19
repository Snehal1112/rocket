import { useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { toast } from 'sonner';
import { flowKeys } from '@/lib/queries/flow-queries';
import { onFlowRequestPathsRewritten } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

/**
 * Applies a flow request path rewrite to open flow tabs and the flow list cache, and warns
 * about flows that could not be updated. Mount once.
 */
export function useFlowRequestPathEvents(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unsub = onFlowRequestPathsRewritten((event) => {
      if (event.flows.length > 0) {
        usePaneStore.getState().rewriteFlowRequestPaths(event.collection, event.from, event.to);
        void queryClient.invalidateQueries({ queryKey: flowKeys.collection(event.collection) });
      }
      if (event.failed.length > 0) {
        const list = event.failed.map((f) => `${f.flow} (${f.message})`).join('; ');
        toast.warning(`Flows not updated: ${list}`);
      }
    });
    return () => {
      void unsub.then((fn) => fn()).catch(() => undefined);
    };
  }, [queryClient]);
}
