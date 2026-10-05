import { AlertTriangle } from 'lucide-react';
import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { validateVariablesText } from '@/lib/graphql-variables';
import { type GraphQlOperation, listGraphQlOperations } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import type { GraphQlState } from '@/types/pane-types';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

interface GraphQlEditorProps {
  state: GraphQlState;
  onChange: (patch: Partial<GraphQlState>) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

// Query on top, variables below, operation picker in the toolbar when the
// document defines several operations.
export function GraphQlEditor({ state, onChange, variableContext }: GraphQlEditorProps) {
  const [operations, setOperations] = useState<GraphQlOperation[] | null>(null);

  // Ask the backend scanner for the operation list, debounced while typing.
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      listGraphQlOperations(state.query)
        .then((ops) => {
          if (!cancelled) setOperations(ops);
        })
        .catch(() => {
          if (!cancelled) setOperations(null);
        });
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [state.query]);

  const names = useMemo(
    () => (operations ?? []).flatMap((o) => (o.name ? [o.name] : [])),
    [operations],
  );
  const hasSeveral = (operations?.length ?? 0) > 1;

  // The parent's handler changes identity on every state patch, so keep the latest in a ref.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // Keep the chosen operation valid as the document changes. Nothing happens until the
  // first scan answers, so a remount cannot clobber the saved choice, and the patch is
  // only sent when the value would actually change.
  useEffect(() => {
    if (operations === null) return;
    const current = state.operationName;
    const next = hasSeveral ? (current && names.includes(current) ? current : names[0]) : undefined;
    if (next !== current) onChangeRef.current({ operationName: next });
  }, [operations, hasSeveral, names, state.operationName]);

  const variablesError = validateVariablesText(state.variables);

  return (
    <div className='flex h-full min-h-0 flex-col'>
      {hasSeveral && (
        <div className='flex items-center gap-2 border-b border-border px-3 py-1.5 shrink-0'>
          <span className='text-xs text-muted-foreground'>Operation</span>
          <Select
            value={state.operationName ?? ''}
            onValueChange={(v) => onChange({ operationName: v })}
          >
            <SelectTrigger className='h-7 w-56 text-xs' aria-label='Operation'>
              <SelectValue placeholder='Choose an operation' />
            </SelectTrigger>
            <SelectContent>
              {(operations ?? []).map((op) =>
                op.name ? (
                  <SelectItem key={op.name} value={op.name} className='text-xs'>
                    {op.kind} {op.name}
                  </SelectItem>
                ) : null,
              )}
            </SelectContent>
          </Select>
        </div>
      )}

      <div className='flex-1 min-h-0'>
        <Suspense fallback={<EditorSkeleton />}>
          <MonacoWrapper
            value={state.query}
            onChange={(query) => onChange({ query })}
            language='graphql'
            height='100%'
            variableContext={variableContext}
          />
        </Suspense>
      </div>

      <div className='flex h-44 shrink-0 flex-col border-t border-border'>
        <div className='flex items-center justify-between px-3 py-1 shrink-0'>
          <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
            Variables (JSON)
          </span>
          {variablesError && (
            <span role='alert' className='flex items-center gap-1 text-xs text-destructive'>
              <AlertTriangle className='h-3 w-3' aria-hidden='true' />
              {variablesError}
            </span>
          )}
        </div>
        <div className='flex-1 min-h-0'>
          <Suspense fallback={<EditorSkeleton />}>
            <MonacoWrapper
              value={state.variables}
              onChange={(variables) => onChange({ variables })}
              language='json'
              height='100%'
              variableContext={variableContext}
            />
          </Suspense>
        </div>
      </div>
    </div>
  );
}
