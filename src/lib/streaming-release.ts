import { releaseGraphQlSubscriptionTab } from '@/lib/graphql-subscription-session';
import { releaseWebSocketTab } from '@/lib/websocket-session';
import type { Tab } from '@/types/pane-types';

/** Ends whatever stream a tab owns (WebSocket or GraphQL subscription) when the tab goes away. */
export function releaseStreamingTab(tab: Tab): void {
  releaseWebSocketTab(tab);
  releaseGraphQlSubscriptionTab(tab);
}
