import { useEffect, useRef } from 'react';
import type { FlowNode } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';

const authIds = (nodes: FlowNode[]) =>
  new Set(nodes.filter((n) => n.kind.kind === 'Auth').map((n) => n.id));

/**
 * Drops the in-memory token state of every Auth node that leaves `nodes`
 * (deleted, undone, or replaced), so a fetched token never outlives its node.
 * Does nothing without a collection and flow.
 */
export function useClearRemovedAuthTokens(
  collection: string | null | undefined,
  flowName: string | null | undefined,
  nodes: FlowNode[],
) {
  const previous = useRef<{ scope: string; ids: Set<string> } | null>(null);
  useEffect(() => {
    if (!collection || !flowName) {
      previous.current = null;
      return;
    }
    const scope = `${collection}::${flowName}`;
    const ids = authIds(nodes);
    const before = previous.current;
    previous.current = { scope, ids };
    if (!before || before.scope !== scope) return;
    for (const id of before.ids) {
      if (!ids.has(id)) useFlowAuthStore.getState().clearNode(collection, flowName, id);
    }
  }, [collection, flowName, nodes]);
}
