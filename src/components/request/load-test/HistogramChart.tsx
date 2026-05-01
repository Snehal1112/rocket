import { useMemo } from 'react';
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

const BUCKETS = [0, 50, 100, 200, 500, 1000, 2000, 5000];

export function HistogramChart() {
  const requestLog = useLoadTestStore((s) => s.requestLog);

  const data = useMemo(
    () =>
      BUCKETS.map((lo, i) => {
        const hi = BUCKETS[i + 1] ?? Infinity;
        const label = hi === Infinity ? `>${lo}` : `${lo}–${hi}`;
        const count = requestLog.filter((e) => e.latencyMs >= lo && e.latencyMs < hi).length;
        return { label, count };
      }),
    [requestLog],
  );

  return (
    <div className='flex h-full flex-col'>
      <p className='text-[11px] text-muted-foreground'>Response time distribution (ms)</p>
      <div className='min-h-0 flex-1'>
        <ResponsiveContainer width='100%' height='100%'>
          <BarChart data={data}>
            <XAxis dataKey='label' tick={{ fontSize: 9 }} interval='preserveStartEnd' />
            <YAxis tick={{ fontSize: 10 }} width={36} />
            <Tooltip contentStyle={{ fontSize: 11 }} />
            <Bar dataKey='count' fill='#7F77DD' radius={[2, 2, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
