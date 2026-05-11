import { useMemo } from 'react';
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

const X_TICK_STYLE = { fontSize: 9 };
const Y_TICK_STYLE = { fontSize: 10 };
const TOOLTIP_STYLE = { fontSize: 11 };

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
            <XAxis dataKey='label' tick={X_TICK_STYLE} interval='preserveStartEnd' />
            <YAxis tick={Y_TICK_STYLE} width={36} />
            <Tooltip contentStyle={TOOLTIP_STYLE} />
            <Bar dataKey='count' fill='hsl(var(--chart-4))' radius={[2, 2, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
