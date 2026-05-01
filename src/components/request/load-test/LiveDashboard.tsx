import { useLoadTestStore } from '@/stores/load-test-store';
import { ErrorRateChart } from './ErrorRateChart';
import { HistogramChart } from './HistogramChart';
import { LatencyChart } from './LatencyChart';
import { RequestLogTable } from './RequestLogTable';
import { StatBar } from './StatBar';
import { ThroughputChart } from './ThroughputChart';

export function LiveDashboard() {
  const status = useLoadTestStore((s) => s.status);
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const idle = status === 'idle' && timeSeries.length === 0;

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <StatBar />
      {idle ? (
        <div className='flex flex-1 items-center justify-center text-sm text-muted-foreground'>
          Configure phases and click Run load test to start.
        </div>
      ) : (
        <>
          <div className='grid min-h-0 flex-1 grid-cols-2 grid-rows-2 gap-2 p-2 pb-0'>
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
          </div>
          <div className='h-36 shrink-0 overflow-hidden border-t border-border/40'>
            <RequestLogTable />
          </div>
        </>
      )}
    </div>
  );
}
