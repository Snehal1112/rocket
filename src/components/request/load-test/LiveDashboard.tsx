import { useLoadTestStore } from '@/stores/load-test-store';
import { ConcurrencyChart } from './ConcurrencyChart';
import { ErrorRateChart } from './ErrorRateChart';
import { HistogramChart } from './HistogramChart';
import { LatencyChart } from './LatencyChart';
import { RequestLogTable } from './RequestLogTable';
import { StatBar } from './StatBar';
import { ThroughputChart } from './ThroughputChart';

const PHASE_NAMES: Record<string, string> = {
  RampUp: 'ramp-up phase',
  Hold: 'hold phase',
  RampDown: 'ramp-down phase',
};

export function LiveDashboard() {
  const status = useLoadTestStore((s) => s.status);
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const latestSnapshot = useLoadTestStore((s) => s.latestSnapshot);
  const phases = useLoadTestStore((s) => s.phases);
  const idle = status === 'idle' && timeSeries.length === 0;

  const currentPhase = latestSnapshot ? phases[latestSnapshot.currentPhaseIndex] : null;
  const phaseName = currentPhase ? (PHASE_NAMES[currentPhase.kind] ?? currentPhase.kind) : null;

  // Total target requests across all phases for progress calculation.
  const totalTarget = phases.reduce((sum, p) => sum + p.targetConcurrency * p.durationSecs, 0);
  const completed = latestSnapshot?.completed ?? 0;
  const progressPct =
    status === 'complete'
      ? 100
      : totalTarget > 0
        ? Math.min(100, (completed / totalTarget) * 100)
        : 0;

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <StatBar />

      {idle ? (
        <div className='flex flex-1 items-center justify-center text-sm text-muted-foreground'>
          Configure phases and click Run load test to start.
        </div>
      ) : (
        <>
          {/* Progress bar */}
          <div className='border-b border-border/40 px-3 py-1.5'>
            <div className='mb-1 h-1 w-full overflow-hidden rounded-full bg-muted'>
              <div
                className='h-full rounded-full bg-chart-4 transition-[width] duration-300'
                style={{ width: `${progressPct}%` }}
              />
            </div>
            {latestSnapshot && (
              <p className='text-[10px] text-muted-foreground'>
                {completed.toLocaleString()} requests
                {phaseName ? ` · ${phaseName}` : ''}
                {` · ${latestSnapshot.activeConcurrent} concurrent`}
              </p>
            )}
          </div>

          {/* 2×2 + concurrency chart grid (5 charts, 2 cols, 3 rows with last row spanning) */}
          <div className='grid min-h-0 flex-1 grid-cols-2 grid-rows-[1fr_1fr_1fr] gap-2 p-2 pb-0'>
            <div className='rounded-md border border-border/40 bg-background p-2'>
              <LatencyChart />
            </div>
            <div className='rounded-md border border-border/40 bg-background p-2'>
              <ThroughputChart />
            </div>
            <div className='rounded-md border border-border/40 bg-background p-2'>
              <ErrorRateChart />
            </div>
            <div className='rounded-md border border-border/40 bg-background p-2'>
              <HistogramChart />
            </div>
            <div className='col-span-2 rounded-md border border-border/40 bg-background p-2'>
              <ConcurrencyChart />
            </div>
          </div>

          {/* Request log */}
          <div className='h-36 shrink-0 overflow-hidden border-t border-border/40'>
            <RequestLogTable />
          </div>
        </>
      )}
    </div>
  );
}
