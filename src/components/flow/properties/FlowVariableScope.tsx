import type { ReactNode } from 'react';
import { useCollectionVariableContext } from '@/hooks/useCollectionVariableContext';
import { FlowVariableContextProvider } from './flowVariableContext';

// Provides the flow collection's variable scope to the editors below it.
export function FlowVariableScope({
  collection,
  children,
}: {
  collection: string;
  children: ReactNode;
}) {
  const { variableContext } = useCollectionVariableContext(collection);
  return (
    <FlowVariableContextProvider value={variableContext}>{children}</FlowVariableContextProvider>
  );
}
