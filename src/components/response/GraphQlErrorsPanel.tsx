import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import type { GraphQlError } from '@/lib/graphql-response';

interface GraphQlErrorsPanelProps {
  errors: GraphQlError[];
}

// Lists the entries of a GraphQL `errors` array with their path and location.
export function GraphQlErrorsPanel({ errors }: GraphQlErrorsPanelProps) {
  if (errors.length === 0) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-2 text-muted-foreground'>
        <CheckCircle2 className='h-8 w-8 opacity-20' />
        <span className='text-xs'>No errors in the response</span>
      </div>
    );
  }
  return (
    <ul className='h-full overflow-auto p-3 space-y-2'>
      {errors.map((err, i) => (
        // The array has no stable id, and entries never reorder.
        // biome-ignore lint/suspicious/noArrayIndexKey: errors are rendered once per response
        <li key={i} className='rounded-md border border-destructive/30 bg-destructive/5 p-2.5'>
          <div className='flex items-start gap-2'>
            <AlertTriangle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-destructive' />
            <span className='text-xs font-medium text-foreground break-words'>{err.message}</span>
          </div>
          {err.path && err.path.length > 0 && (
            <div className='mt-1 pl-5 font-mono text-2xs text-muted-foreground'>
              {err.path.join('.')}
            </div>
          )}
          {err.locations && err.locations.length > 0 && (
            <div className='pl-5 font-mono text-2xs text-muted-foreground'>
              {err.locations.map((l) => `line ${l.line}, column ${l.column}`).join('; ')}
            </div>
          )}
          {err.extensions && (
            <pre className='mt-1 pl-5 font-mono text-2xs text-muted-foreground whitespace-pre-wrap'>
              {JSON.stringify(err.extensions, null, 2)}
            </pre>
          )}
        </li>
      ))}
    </ul>
  );
}
