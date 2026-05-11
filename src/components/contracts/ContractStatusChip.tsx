import { cn } from '@/lib/utils';
import { statusChipLabel } from '@/lib/contracts/statusMachine';
import type { ContractStatus } from '@/types/contracts';

interface ContractStatusChipProps {
  status: ContractStatus;
  /** driftCount or breachCount — shown in label for drift/breach */
  count?: number;
  className?: string;
}

const chipVariants: Record<ContractStatus, string> = {
  active:
    'bg-[hsl(var(--success)/0.14)] text-[hsl(var(--success))] border-[hsl(var(--success)/0.30)]',
  drift:
    'bg-[hsl(var(--warning)/0.14)] text-[hsl(var(--warning))] border-[hsl(var(--warning)/0.30)]',
  breach:
    'bg-[hsl(var(--destructive)/0.16)] text-[hsl(var(--destructive))] border-[hsl(var(--destructive)/0.34)] animate-pulse-red',
  in_review: 'bg-[hsl(var(--primary)/0.14)] text-primary border-[hsl(var(--primary)/0.30)]',
  draft: 'bg-muted text-muted-foreground border-border',
  paused: 'bg-muted text-foreground border-border border-2',
  expired: 'bg-[hsl(var(--muted-foreground)/0.18)] text-muted-foreground border-border border-2',
  expiring_in_30_days:
    'bg-[hsl(var(--warning)/0.10)] text-[hsl(var(--warning))] border-[hsl(var(--warning)/0.25)]',
  archived: 'bg-muted text-muted-foreground border-border',
};

const dotVariants: Record<ContractStatus, string | null> = {
  active: 'bg-[hsl(var(--success))] animate-pulse',
  drift: 'bg-[hsl(var(--warning))]',
  breach: 'bg-[hsl(var(--destructive))]',
  in_review: 'bg-primary',
  draft: null,
  paused: 'bg-muted-foreground/50',
  expired: 'bg-muted-foreground/50',
  expiring_in_30_days: 'bg-[hsl(var(--warning))] animate-pulse',
  archived: null,
};

export function ContractStatusChip({ status, count, className }: ContractStatusChipProps) {
  const label = statusChipLabel(status, count);
  const dot = dotVariants[status];

  return (
    <div
      role='status'
      className={cn(
        'inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md border text-xs font-semibold transition-colors shrink-0',
        chipVariants[status],
        className,
      )}
    >
      {dot && <span className={cn('w-1.5 h-1.5 rounded-full shrink-0', dot)} aria-hidden='true' />}
      <span>{label}</span>
      <span className='sr-only'>Status: {label}</span>
    </div>
  );
}
