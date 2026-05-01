import { useMemo } from 'react';
import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function ConcurrencyChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const data = useMemo(
    () =>
      timeSeries.map((p) => ({
        t: (p.elapsedMs / 1000).toFixed(1),
        conc: p.activeConcurrent,
      })),
    [timeSeries],
  );

  return (
    <div className='flex h-full flex-col'>
      <p className='text-[11px] text-muted-foreground'>Active concurrent</p>
      <div className='min-h-0 flex-1'>
        <ResponsiveContainer width='100%' height='100%'>
          <AreaChart data={data}>
            <XAxis dataKey='t' tick={{ fontSize: 10 }} interval='preserveStartEnd' />
            <YAxis tick={{ fontSize: 10 }} width={36} />
            <Tooltip contentStyle={{ fontSize: 11 }} />
            <Area
              type='monotone'
              dataKey='conc'
              stroke='#1D9E75'
              fill='rgba(29,158,117,0.12)'
              strokeWidth={1.5}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
