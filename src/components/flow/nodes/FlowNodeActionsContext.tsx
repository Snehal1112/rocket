import { createContext, useContext } from 'react';
import type { FlowNodeKind } from '@/lib/tauri-api';

export interface FlowNodeActions {
  updateNodeKind: (nodeId: string, kind: FlowNodeKind) => void;
  removeSwitchCase: (nodeId: string, caseId: string) => void;
  /** Selects exactly this node, which opens its properties panel. */
  openProperties: (nodeId: string) => void;
}

const noop = () => {
  // Intentionally empty.
};

// The default is a no-op, so a node rendered outside a canvas (for example in
// a unit test) stays inert instead of throwing.
export const FlowNodeActionsContext = createContext<FlowNodeActions>({
  updateNodeKind: noop,
  removeSwitchCase: noop,
  openProperties: noop,
});

export function useFlowNodeActions(): FlowNodeActions {
  return useContext(FlowNodeActionsContext);
}
