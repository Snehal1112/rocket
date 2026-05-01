import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

export function ThroughputChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const data = timeSeries.map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1),
    rps: +p.rps.toFixed(1),
  }));

  return (
    <div className='flex h-full flex-col'>
      <p className='text-[11px] text-muted-foreground'>Throughput (req / sec)</p>
      <div className='min-h-0 flex-1'>
        <ResponsiveContainer width='100%' height='100%'>
          <AreaChart data={data}>
            <XAxis dataKey='t' tick={{ fontSize: 10 }} interval='preserveStartEnd' />
            <YAxis tick={{ fontSize: 10 }} width={36} />
            <Tooltip contentStyle={{ fontSize: 11 }} />
            <Area
              type='monotone'
              dataKey='rps'
              stroke='#7F77DD'
              fill='rgba(127,119,221,0.12)'
              strokeWidth={1.5}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
