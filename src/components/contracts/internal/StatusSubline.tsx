import type { Contract } from '@/types/contracts';

interface StatusSublineProps {
  contract: Pick<
    Contract,
    'status' | 'breachCount' | 'consumers' | 'pauseReason' | 'successorName'
  >;
}

const Dot = () => (
  <span className='w-1 h-1 rounded-full bg-muted-foreground/40 shrink-0' aria-hidden='true' />
);

export function StatusSubline({ contract }: StatusSublineProps) {
  const { status, breachCount, consumers, pauseReason, successorName } = contract;
  const consumerSuffix = consumers.length > 1 ? ` · ${consumers.length} consumers` : '';

  if (status === 'active' || status === 'expiring_in_30_days') {
    return (
      <>
        <Dot />
        <span>In compliance{consumerSuffix}</span>
      </>
    );
  }
  if (status === 'drift') {
    const breakingText = breachCount > 0 ? ` — ${breachCount} breaking` : ' — non-breaking';
    return (
      <>
        <Dot />
        <span className='text-[hsl(var(--warning))]'>Drift detected{breakingText}</span>
      </>
    );
  }
  if (status === 'breach') {
    return (
      <>
        <Dot />
        <span className='text-[hsl(var(--destructive))]'>
          {breachCount} breaking change{breachCount !== 1 ? 's' : ''}
        </span>
      </>
    );
  }
  if (status === 'paused') {
    const subtext = pauseReason ?? 'Monitoring paused';
    return (
      <>
        <Dot />
        <span className='text-muted-foreground'>{subtext}</span>
      </>
    );
  }
  if (status === 'draft') {
    return (
      <>
        <Dot />
        <span className='text-muted-foreground'>Not yet published</span>
      </>
    );
  }
  if (status === 'expired') {
    const subtext = successorName ? `Succeeded by ${successorName}` : 'No successor';
    return (
      <>
        <Dot />
        <span className='text-muted-foreground'>{subtext}</span>
      </>
    );
  }
  if (status === 'in_review') {
    return (
      <>
        <Dot />
        <span className='text-primary'>Awaiting consumer sign-off{consumerSuffix}</span>
      </>
    );
  }
  return null;
}
