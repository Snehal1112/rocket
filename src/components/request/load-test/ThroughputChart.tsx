import { Area, AreaChart, Line, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { useLoadTestStore } from '@/stores/load-test-store';

/** Returns the target rate (rps) at a given elapsed_ms timestamp. */
function targetRateAt(
  elapsedMs: number,
  phases: ReturnType<typeof useLoadTestStore.getState>['phases'],
): number | null {
  let boundaryMs = 0;
  let prevValue = 0;
  for (const phase of phases) {
    const phaseEndMs = boundaryMs + phase.durationSecs * 1000;
    if (phase.target.kind !== 'rps') return null;
    if (elapsedMs <= phaseEndMs) {
      // Linear interp for ramp phases; flat for hold.
      if (phase.kind === 'Hold') return phase.target.value;
      const progress = (elapsedMs - boundaryMs) / (phaseEndMs - boundaryMs);
      return Math.round(prevValue + (phase.target.value - prevValue) * progress);
    }
    boundaryMs = phaseEndMs;
    prevValue = phase.target.value;
  }
  return prevValue;
}

export function ThroughputChart() {
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const targetUnit = useLoadTestStore((s) => s.targetUnit);
  const phases = useLoadTestStore((s) => s.phases);
  const showTarget = targetUnit === 'rps' && phases.every((p) => p.target.kind === 'rps');

  const data = timeSeries.map((p) => ({
    t: (p.elapsedMs / 1000).toFixed(1),
    rps: +p.rps.toFixed(1),
    target: showTarget ? targetRateAt(p.elapsedMs, phases) : null,
  }));

  return (
    <div className='flex h-full flex-col'>
      <p className='mb-1 text-[11px] font-medium text-muted-foreground'>Throughput (req / sec)</p>
      <ResponsiveContainer width='100%' height='100%'>
        <AreaChart data={data} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
          <XAxis dataKey='t' tick={{ fontSize: 10 }} interval='preserveStartEnd' />
          <YAxis tick={{ fontSize: 10 }} width={36} />
          <Tooltip contentStyle={{ fontSize: 11 }} />
          <Area
            type='monotone'
            dataKey='rps'
            stroke='hsl(var(--chart-4))'
            fill='hsl(var(--chart-4) / 0.12)'
            strokeWidth={1.5}
            dot={false}
            name='actual'
          />
          {showTarget && (
            <Line
              type='monotone'
              dataKey='target'
              stroke='hsl(var(--chart-2))'
              strokeDasharray='4 3'
              strokeWidth={1.2}
              dot={false}
              name='target'
              isAnimationActive={false}
            />
          )}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
