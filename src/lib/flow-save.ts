import type { Flow } from '@/lib/tauri-api';
import type { FlowTab } from '@/types/pane-types';

// Builds the `save_flow` arguments from a flow tab. Null while the tab is still a picker.
export function flowPayloadFromTab(tab: FlowTab): { collection: string; flow: Flow } | null {
  if (!tab.collectionName || !tab.flowName) return null;
  return {
    collection: tab.collectionName,
    flow: {
      name: tab.flowName,
      nodes: tab.nodes,
      edges: tab.edges,
      ...(tab.callbackHost ? { callbackHost: tab.callbackHost } : {}),
    },
  };
}
