import { useMemo } from 'react';
import { Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

const TICK_STYLE = { fontSize: 10 };
const TOOLTIP_STYLE = { fontSize: 11 };
const LEGEND_STYLE = { fontSize: 10 };

export function LatencyChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const data = useMemo(
    () =>
      timeSeries.map((p) => ({
        t: (p.elapsedMs / 1000).toFixed(1),
        p50: +p.p50Ms.toFixed(1),
        p95: +p.p95Ms.toFixed(1),
        p99: +p.p99Ms.toFixed(1),
      })),
    [timeSeries],
  );

  return (
    <div className='flex h-full flex-col'>
      <p className='text-[11px] text-muted-foreground'>Latency over time (ms)</p>
      <div className='min-h-0 flex-1'>
        <ResponsiveContainer width='100%' height='100%'>
          <LineChart data={data}>
            <XAxis dataKey='t' tick={TICK_STYLE} interval='preserveStartEnd' />
            <YAxis tick={TICK_STYLE} width={36} />
            <Tooltip contentStyle={TOOLTIP_STYLE} />
            <Legend iconSize={8} wrapperStyle={LEGEND_STYLE} />
            <Line
              type='monotone'
              dataKey='p50'
              stroke='hsl(var(--chart-4))'
              dot={false}
              strokeWidth={1.5}
            />
            <Line
              type='monotone'
              dataKey='p95'
              stroke='hsl(var(--chart-2))'
              dot={false}
              strokeWidth={1.5}
            />
            <Line
              type='monotone'
              dataKey='p99'
              stroke='hsl(var(--destructive))'
              strokeDasharray='3 2'
              dot={false}
              strokeWidth={1.5}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
