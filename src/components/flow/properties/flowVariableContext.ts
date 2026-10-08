import { createContext, useContext } from 'react';
import type { VariableScopeEntry } from '@/lib/url-variables';

// The variables a flow's collection offers, for the editors in the properties panel.
// Undefined outside a provider, which leaves an editor without highlighting.
const FlowVariableContext = createContext<Map<string, VariableScopeEntry> | undefined>(undefined);

export const FlowVariableContextProvider = FlowVariableContext.Provider;

export function useFlowVariableContext(): Map<string, VariableScopeEntry> | undefined {
  return useContext(FlowVariableContext);
}
