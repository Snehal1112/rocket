import { createContext, useContext } from 'react';
import type { FlowNodeKind, FlowPartialMode } from '@/lib/tauri-api';

export interface FlowNodeActions {
  updateNodeKind: (nodeId: string, kind: FlowNodeKind) => void;
  removeSwitchCase: (nodeId: string, caseId: string) => void;
  /** Selects exactly this node, which opens its properties panel. */
  openProperties: (nodeId: string) => void;
  /** Duplicates just this node. Absent when the canvas cannot duplicate. */
  duplicateNode?: (nodeId: string) => void;
  /** Starts a partial run from this node. Absent until the tab has a run to build on. */
  runNode?: (nodeId: string, mode: FlowPartialMode) => void;
  /** True while a run is starting or in progress. Disables the run items. */
  runBusy?: boolean;
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
