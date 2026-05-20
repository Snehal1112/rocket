import { useMemo } from 'react';
import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';
import { makeTimeLabel } from './chart-utils';

const TICK_STYLE = { fontSize: 10 };
const TOOLTIP_STYLE = { fontSize: 11 };

export function ConcurrencyChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const data = useMemo(() => {
    const fmt = makeTimeLabel(timeSeries);
    return timeSeries.map((p) => ({
      t: fmt(p.elapsedMs),
      conc: p.activeConcurrent,
    }));
  }, [timeSeries]);

  return (
    <div className='flex h-full flex-col'>
      <p className='text-[11px] text-muted-foreground'>Active concurrent</p>
      <div className='min-h-0 flex-1'>
        <ResponsiveContainer width='100%' height='100%'>
          <AreaChart data={data}>
            <XAxis dataKey='t' tick={TICK_STYLE} interval='preserveStartEnd' />
            <YAxis tick={TICK_STYLE} width={36} />
            <Tooltip contentStyle={TOOLTIP_STYLE} />
            <Area
              type='monotone'
              dataKey='conc'
              stroke='hsl(var(--chart-2))'
              fill='hsl(var(--chart-2) / 0.12)'
              strokeWidth={1.5}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
