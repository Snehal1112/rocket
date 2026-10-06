import { useEffect, useMemo, useState } from 'react';
import { type GraphQlOperation, listGraphQlOperations } from '@/lib/tauri-api';

export type OperationKind = GraphQlOperation['kind'];

/** The kind of the operation a send would run, or null when it cannot be told yet. */
export function pickOperationKind(
  operations: GraphQlOperation[],
  operationName: string | undefined,
): OperationKind | null {
  if (operationName) return operations.find((o) => o.name === operationName)?.kind ?? null;
  return operations.length === 1 ? operations[0].kind : null;
}

// Asks the backend scanner (the same one the editor's operation picker uses), debounced while
// the user types, and reports the kind of the selected operation.
export function useSelectedOperationKind(
  query: string,
  operationName: string | undefined,
): OperationKind | null {
  const [operations, setOperations] = useState<GraphQlOperation[]>([]);

  useEffect(() => {
    if (query.trim() === '') {
      setOperations([]);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      listGraphQlOperations(query)
        .then((ops) => {
          if (!cancelled) setOperations(ops);
        })
        .catch(() => {
          if (!cancelled) setOperations([]);
        });
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  return useMemo(() => pickOperationKind(operations, operationName), [operations, operationName]);
}
