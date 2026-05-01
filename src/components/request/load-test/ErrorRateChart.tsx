import { useMemo } from 'react';
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function ErrorRateChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const data = useMemo(
    () =>
      timeSeries.map((p) => ({
        t: (p.elapsedMs / 1000).toFixed(1),
        err: +p.errorRatePct.toFixed(2),
      })),
    [timeSeries],
  );

  return (
    <div className='flex h-full flex-col'>
      <p className='text-[11px] text-muted-foreground'>Error rate (%)</p>
      <div className='min-h-0 flex-1'>
        <ResponsiveContainer width='100%' height='100%'>
          <LineChart data={data}>
            <XAxis dataKey='t' tick={{ fontSize: 10 }} interval='preserveStartEnd' />
            <YAxis tick={{ fontSize: 10 }} width={36} domain={[0, 100]} />
            <Tooltip contentStyle={{ fontSize: 11 }} />
            <Line
              type='monotone'
              dataKey='err'
              stroke='hsl(var(--destructive))'
              dot={false}
              strokeWidth={1.5}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
