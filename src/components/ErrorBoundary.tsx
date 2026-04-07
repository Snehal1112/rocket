import { RefreshCw } from 'lucide-react';
import type { ReactNode } from 'react';
import { type FallbackProps, ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';

// Fallback UI — matches the original compact embedded design.
export function ErrorFallback({ error, resetErrorBoundary }: FallbackProps) {
  return (
    <div className='flex flex-col items-center justify-center h-full p-4 text-center'>
      <p className='text-sm font-medium text-destructive'>Something went wrong.</p>
      <p className='text-xs text-muted-foreground mt-1'>
        {error instanceof Error ? error.message : String(error)}
      </p>
      <button
        type='button'
        className='mt-3 flex items-center gap-1 text-xs text-primary hover:underline'
        onClick={resetErrorBoundary}
      >
        <RefreshCw className='h-3 w-3' />
        Try again
      </button>
    </div>
  );
}

interface ErrorBoundaryProps {
  children: ReactNode;
  /** When any of these keys change the boundary resets automatically. */
  resetKeys?: unknown[];
}

export function ErrorBoundary({ children, resetKeys }: ErrorBoundaryProps) {
  function handleError(error: unknown, info: { componentStack?: string | null }) {
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  return (
    <ReactErrorBoundary
      FallbackComponent={ErrorFallback}
      onError={handleError}
      resetKeys={resetKeys}
    >
      {children}
    </ReactErrorBoundary>
  );
}
