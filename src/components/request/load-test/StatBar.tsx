import { useLoadTestStore } from '@/stores/load-test-store';

interface StatProps {
  label: string;
  value: string;
  className?: string;
}

function Stat({ label, value, className = '' }: StatProps) {
  return (
    <div className='rounded-md bg-muted/50 px-3 py-2'>
      <p className='text-[10px] uppercase tracking-wider text-muted-foreground'>{label}</p>
      <p className={'mt-0.5 text-sm font-medium ' + className}>{value}</p>
    </div>
  );
}

export function StatBar() {
  const latestSnapshot = useLoadTestStore((s) => s.latestSnapshot);
  const status = useLoadTestStore((s) => s.status);
  const result = useLoadTestStore((s) => s.result);

  const src = status === 'complete' && result ? result : latestSnapshot;

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

  let completed: number;
  let succeeded: number;
  let failed: number;
  let rps: number;
  let p95: number;
  let elapsedMs: number;

  if (isSnapshot) {
    completed = src.completed;
    succeeded = src.succeeded;
    failed = src.failedStatus + src.failedTransport;
    rps = src.requestsPerSecond;
    p95 = src.p95Ms;
    elapsedMs = src.elapsedMs;
  } else {
    completed = src.totalRequests;
    succeeded = src.succeeded;
    failed = src.failed;
    rps = src.requestsPerSecond;
    p95 = src.p95LatencyMs;
    elapsedMs = src.totalDurationMs;
  }

  return (
    <div className='grid grid-cols-6 gap-2 border-b border-border/40 bg-background px-3 py-2'>
      <Stat label='Completed' value={String(completed)} />
      <Stat label='Succeeded' value={String(succeeded)} className='text-[#1D9E75]' />
      <Stat
        label='Failed'
        value={String(failed)}
        className={failed > 0 ? 'text-destructive' : ''}
      />
      <Stat label='Req / sec' value={rps.toFixed(1)} className='text-[#7F77DD]' />
      <Stat label='p95 latency' value={p95.toFixed(0) + 'ms'} />
      <Stat label='Elapsed' value={(elapsedMs / 1000).toFixed(1) + 's'} />
    </div>
  );
}
