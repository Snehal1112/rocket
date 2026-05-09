import type { Contract } from '@/types/contracts';

interface StatusSublineProps {
  contract: Pick<Contract, 'status' | 'breachCount'>;
}

/**
 * Short explanatory text shown below the contract title.
 * Returns null for active/expiring statuses (no subline needed).
 */
export function StatusSubline({ contract }: StatusSublineProps) {
  const { status, breachCount } = contract;

  if (status === 'drift') {
    const breakingText = breachCount > 0 ? ` — ${breachCount} breaking` : '';
    return <span className='text-[hsl(var(--warning))]'>Drift detected{breakingText}</span>;
  }
  if (status === 'breach') {
    return (
      <span className='text-[hsl(var(--destructive))]'>
        {breachCount} breaking change{breachCount !== 1 ? 's' : ''}
      </span>
    );
  }
  if (status === 'paused') return <span className='text-muted-foreground'>Monitoring paused</span>;
  if (status === 'draft') return <span className='text-muted-foreground'>Not yet published</span>;
  if (status === 'expired') return <span className='text-muted-foreground'>Contract expired</span>;
  if (status === 'in_review') return <span className='text-primary'>Awaiting consumer review</span>;
  return null;
}
