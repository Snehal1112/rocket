import { releaseGraphQlSubscriptionTab } from '@/lib/graphql-subscription-session';
import { releaseWebSocketTab } from '@/lib/websocket-session';
import { useGrpcStore } from '@/stores/grpc-store';
import type { Tab } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

/** Ends whatever stream a tab owns (WebSocket, GraphQL subscription or gRPC call) when the tab goes away. */
export function releaseStreamingTab(tab: Tab): void {
  releaseWebSocketTab(tab);
  releaseGraphQlSubscriptionTab(tab);
  // A running gRPC stream would otherwise keep a connection open with no tab to show it.
  if (isRequestTab(tab) && tab.request.requestType === 'grpc') {
    void useGrpcStore.getState().dropTab(tab.id);
  }
}
