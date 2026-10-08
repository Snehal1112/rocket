import { CheckCircle2, Loader2, MinusCircle, XCircle } from 'lucide-react';
import type { FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

// Shows the run status as a shape as well as a colour. It is hidden from
// assistive technology because the node's accessible name already says the status.
export function NodeStatusIcon({ status }: { status: FlowNodeStatus }) {
  if (status === 'idle') return null;
  const icon =
    status === 'running' ? (
      <Loader2
        className={cn('h-3.5 w-3.5 text-blue-500', 'animate-spin motion-reduce:animate-none')}
      />
    ) : status === 'success' ? (
      <CheckCircle2 className='h-3.5 w-3.5 text-green-600' />
    ) : status === 'failed' ? (
      <XCircle className='h-3.5 w-3.5 text-red-600' />
    ) : (
      <MinusCircle className='h-3.5 w-3.5 text-muted-foreground' />
    );
  return (
    <span
      data-testid='node-status-icon'
      data-status={status}
      aria-hidden='true'
      className='inline-flex shrink-0'
    >
      {icon}
    </span>
  );
}
