import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export type StateBlockTone = 'loading' | 'empty' | 'error';

export interface StateBlockProps {
  tone: StateBlockTone;
  children: ReactNode;
  action?: { label: string; onClick: () => void };
}

const DOT_CLASS: Record<StateBlockTone, string> = {
  loading: 'bg-muted-foreground animate-pulse motion-reduce:animate-none',
  empty: 'bg-muted-foreground/50',
  error: 'bg-destructive',
};

/** Inline dashed block for a loading, empty or error state, with an optional action. */
export function StateBlock({ tone, children, action }: StateBlockProps) {
  return (
    <div
      role={tone === 'error' ? 'alert' : 'status'}
      data-slot='state-block'
      data-tone={tone}
      className={cn(
        'flex items-center gap-2 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground',
        tone === 'error' && 'border-destructive/50',
      )}
    >
      <span
        data-slot='state-dot'
        aria-hidden='true'
        className={cn('h-2 w-2 shrink-0 rounded-full', DOT_CLASS[tone])}
      />
      <span className='min-w-0 flex-1 break-words'>{children}</span>
      {action && (
        <Button variant='outline' size='sm' className='h-7 shrink-0' onClick={action.onClick}>
          {action.label}
        </Button>
      )}
    </div>
  );
}
