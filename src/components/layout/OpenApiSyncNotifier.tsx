import { useQueryClient } from '@tanstack/react-query';
import { useCallback } from 'react';
import { toast } from 'sonner';
import { findActiveLeaf } from '@/lib/pane-utils';
import {
  type OpenApiCheckedEvent,
  openApiSyncKeys,
  useOpenApiSyncGlobalEvents,
} from '@/lib/queries/openapi-sync-queries';
import { getOpenApiSyncBadges, getOpenApiSyncStatus } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { forgetNotified, notifyStore, shouldNotify } from './notifyUpdates';

function syncTabIsActive(collection: string): boolean {
  const { root, activeGroupId } = usePaneStore.getState();
  return findActiveLeaf(root, activeGroupId).activeTabId === `openapi-sync:${collection}`;
}

/**
 * Mounts the app-wide OpenAPI sync listeners once. It shows one toast per new spec version of
 * a collection, and renders nothing.
 */
export function OpenApiSyncNotifier() {
  const queryClient = useQueryClient();
  const onChecked = useCallback(
    async ({ collection, status }: OpenApiCheckedEvent) => {
      if (status !== 'changed') return;
      try {
        const info = await queryClient.fetchQuery({
          networkMode: 'always',
          queryKey: openApiSyncKeys.status(collection),
          queryFn: () => getOpenApiSyncStatus(collection, false),
          staleTime: 0,
        });
        if (!info || info.consent !== 'granted' || !info.hasPending || !info.pendingId) return;
        // The badge count comes first, so a failed lookup does not consume the id.
        const badges = await queryClient.fetchQuery({
          networkMode: 'always',
          queryKey: openApiSyncKeys.badges,
          queryFn: getOpenApiSyncBadges,
          staleTime: 0,
        });
        const waiting = badges.find((b) => b.collection === collection)?.waiting ?? 0;
        if (waiting < 1) return;
        // Record the id only now. A viewed version is recorded too, but gets no toast.
        if (!shouldNotify(notifyStore, collection, info.pendingId)) return;
        if (syncTabIsActive(collection)) return;
        try {
          toast.info(`${collection} has ${waiting} spec ${waiting === 1 ? 'update' : 'updates'}`, {
            action: {
              label: 'Review',
              onClick: () => usePaneStore.getState().openOpenApiSyncTab(collection),
            },
          });
        } catch (error) {
          // The toast never showed, so free the id and let the next event retry.
          forgetNotified(notifyStore, collection, info.pendingId);
          throw error;
        }
      } catch {
        // A failed lookup only skips the toast. The id is not recorded, so the next event retries.
      }
    },
    [queryClient],
  );
  useOpenApiSyncGlobalEvents((event) => void onChecked(event));
  return null;
}
