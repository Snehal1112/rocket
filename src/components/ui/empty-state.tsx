import type { ReactNode } from 'react';
import { RocketError, RocketIdle, RocketPlug, RocketSearch } from '@/components/illustrations';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

// Why there is nothing to show. The cause decides the wording, so a user is never told
// to create something that exists, or that nothing exists when a filter hides it.
export type EmptyCause = 'none-yet' | 'filtered' | 'needs-setup' | 'unavailable';

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
}

export interface EmptyStateProps {
  cause: EmptyCause;
  /** What is empty, in a few words. For a filter, say that nothing matches. */
  title: string;
  /** One sentence: what to do next, or where to go. */
  reason?: ReactNode;
  action?: EmptyStateAction;
  /** Replaces the illustration for the cause. `false` hides it, for a short panel. */
  illustration?: ReactNode | false;
  /** `compact` fits a sidebar or dialog. `full` fits a whole panel. */
  size?: 'compact' | 'full';
  className?: string;
}

const ILLUSTRATION_CLASS: Record<NonNullable<EmptyStateProps['size']>, string> = {
  compact: 'h-16 w-16',
  full: 'h-24 w-24',
};

function defaultIllustration(cause: EmptyCause, className: string): ReactNode {
  switch (cause) {
    case 'none-yet':
      return <RocketIdle className={className} />;
    case 'filtered':
      return <RocketSearch className={className} />;
    case 'needs-setup':
      return <RocketPlug className={className} />;
    case 'unavailable':
      return <RocketError className={className} />;
  }
}

/** A centered empty state: an illustration, what is empty, why, and the next step. */
export function EmptyState({
  cause,
  title,
  reason,
  action,
  illustration,
  size = 'full',
  className,
}: EmptyStateProps) {
  const art = ILLUSTRATION_CLASS[size];
  return (
    <div
      role={cause === 'unavailable' ? 'alert' : 'status'}
      data-slot='empty-state'
      data-cause={cause}
      className={cn(
        'flex h-full flex-col items-center justify-center text-center',
        size === 'compact' ? 'gap-2 px-4 py-6' : 'gap-4 px-6 py-8',
        className,
      )}
    >
      {illustration !== false && (
        <span aria-hidden='true' className='opacity-80'>
          {illustration ?? defaultIllustration(cause, art)}
        </span>
      )}
      <div className='space-y-1'>
        <p className='text-sm font-medium text-foreground'>{title}</p>
        {reason && (
          <p className='mx-auto max-w-[280px] break-words text-xs leading-relaxed text-muted-foreground'>
            {reason}
          </p>
        )}
      </div>
      {action && (
        <Button variant='outline' size='sm' className='h-7 text-xs' onClick={action.onClick}>
          {action.label}
        </Button>
      )}
    </div>
  );
}
