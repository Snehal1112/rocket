import { cn } from '@/lib/utils';
import { useLoadTestStore } from '@/stores/load-test-store';

interface StatProps {
  label: string;
  value: string;
  className?: string;
}

function Stat({ label, value, className }: StatProps) {
  return (
    <div className='rounded-md bg-muted/50 px-3 py-2'>
      <p className='text-[10px] uppercase tracking-wider text-muted-foreground'>{label}</p>
      <p className={cn('mt-0.5 text-sm font-medium', className)}>{value}</p>
    </div>
  );
}

export function StatBar() {
  const latestSnapshot = useLoadTestStore((s) => s.latestSnapshot);
  const status = useLoadTestStore((s) => s.status);
  const result = useLoadTestStore((s) => s.result);
  const targetUnit = useLoadTestStore((s) => s.targetUnit);
  const phases = useLoadTestStore((s) => s.phases);

  // Prefer final result when complete; fall back to live snapshot during advanced run.
  const src = status === 'complete' && result ? result : latestSnapshot;

  const phaseIndex = latestSnapshot?.currentPhaseIndex ?? 0;
  const targetRps =
    targetUnit === 'rps' && phases[phaseIndex]?.target.kind === 'rps'
      ? phases[phaseIndex].target.value
      : null;

  if (!src) {
    return (
      <div className='grid grid-cols-6 gap-2 border-b border-border/40 bg-background px-3 py-2'>
        <Stat label='Completed' value='—' />
        <Stat label='Succeeded' value='—' />
        <Stat label='Failed' value='—' />
        <Stat label='Req / sec' value='—' />
        <Stat label='p95 latency' value='—' />
        <Stat label='Elapsed' value='—' />
      </div>
    );
  }

  const isSnapshot = 'elapsedMs' in src;

  const completed = isSnapshot ? src.completed : src.totalRequests;
  const succeeded = src.succeeded;
  const failed = isSnapshot ? src.failedStatus + src.failedTransport : src.failed;
  const rps = isSnapshot ? src.requestsPerSecond : src.requestsPerSecond;
  const p95 = isSnapshot ? src.p95Ms : src.p95LatencyMs;
  const elapsedMs = isSnapshot ? src.elapsedMs : src.totalDurationMs;

  return (
    <div className='grid grid-cols-6 gap-2 border-b border-border/40 bg-background px-3 py-2'>
      <Stat label='Completed' value={String(completed)} />
      <Stat label='Succeeded' value={String(succeeded)} className='text-chart-2' />
      <Stat
        label='Failed'
        value={String(failed)}
        className={failed > 0 ? 'text-destructive' : ''}
      />
      <Stat
        label='Req / sec'
        value={targetRps !== null ? `${rps.toFixed(1)} / ${targetRps}` : rps.toFixed(1)}
        className='text-chart-4'
      />
      <Stat label='p95 latency' value={`${p95.toFixed(0)}ms`} />
      <Stat label='Elapsed' value={`${(elapsedMs / 1000).toFixed(1)}s`} />
    </div>
  );
}
