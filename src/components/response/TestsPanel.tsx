import { CheckCircle2, XCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import type { TestResult } from '@/lib/tauri-api';

interface TestsPanelProps {
  results: TestResult[];
}

export function TestsPanel({ results }: TestsPanelProps) {
  if (results.length === 0) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        No tests ran for this request.
      </div>
    );
  }

  const passed = results.filter((r) => r.status === 'passed').length;
  const failed = results.filter((r) => r.status === 'failed').length;

  return (
    <div className='flex flex-col h-full'>
      <div className='flex items-center gap-2 px-3 py-2 border-b shrink-0'>
        {passed > 0 && (
          <Badge
            variant='outline'
            className='gap-1 text-green-600 border-green-200 bg-green-50 dark:bg-green-950 dark:border-green-800 dark:text-green-400'
          >
            <CheckCircle2 className='h-3 w-3' />
            {passed} passed
          </Badge>
        )}
        {failed > 0 && (
          <Badge
            variant='outline'
            className='gap-1 text-red-600 border-red-200 bg-red-50 dark:bg-red-950 dark:border-red-800 dark:text-red-400'
          >
            <XCircle className='h-3 w-3' />
            {failed} failed
          </Badge>
        )}
      </div>

      <div className='flex-1 overflow-auto'>
        {results.map((result) => (
          <div
            key={result.name}
            className='flex items-start gap-2 px-3 py-2 border-b last:border-b-0 text-sm'
          >
            {result.status === 'passed' ? (
              <CheckCircle2 className='h-4 w-4 text-green-500 mt-0.5 shrink-0' />
            ) : (
              <XCircle className='h-4 w-4 text-red-500 mt-0.5 shrink-0' />
            )}
            <div className='flex flex-col gap-0.5 min-w-0'>
              <span
                className={result.status === 'failed' ? 'text-foreground' : 'text-muted-foreground'}
              >
                {result.name}
              </span>
              {result.error && (
                <span className='text-xs text-red-500 font-mono break-all'>{result.error}</span>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
