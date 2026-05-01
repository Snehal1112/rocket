import { useEffect, useRef } from 'react';
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

const LOG_HEIGHT_KEY = 'rocket:load-test:log-height';
const DEFAULT_LOG_HEIGHT = 144;
const MIN_LOG_HEIGHT = 72;
const MAX_LOG_HEIGHT = 400;

export function LiveDashboard() {
  const mode = useLoadTestStore((s) => s.mode);
  const status = useLoadTestStore((s) => s.status);
  const timeSeries = useLoadTestStore((s) => s.timeSeries);
  const latestSnapshot = useLoadTestStore((s) => s.latestSnapshot);
  const phases = useLoadTestStore((s) => s.phases);
  const targetUnit = useLoadTestStore((s) => s.targetUnit);

  const idle = status === 'idle' && timeSeries.length === 0;

  const currentPhase = latestSnapshot ? phases[latestSnapshot.currentPhaseIndex] : null;
  const phaseName = currentPhase ? (PHASE_NAMES[currentPhase.kind] ?? currentPhase.kind) : null;

  const totalDurationMs = phases.reduce((sum, p) => sum + p.durationSecs * 1000, 0);
  const completed = latestSnapshot?.completed ?? 0;
  const elapsedMs = latestSnapshot?.elapsedMs ?? 0;
  const progressPct =
    status === 'complete'
      ? 100
      : targetUnit === 'rps'
        ? (() => {
            const totalExpected = phases.reduce((sum, p) => sum + p.target.value * p.durationSecs, 0);
            return totalExpected > 0 ? Math.min(100, (completed / totalExpected) * 100) : 0;
          })()
        : totalDurationMs > 0
          ? Math.min(100, (elapsedMs / totalDurationMs) * 100)
          : 0;

  const logRef = useRef<HTMLDivElement>(null);
  const logHeight = useRef<number>(DEFAULT_LOG_HEIGHT);
  const onMoveRef = useRef<((ev: MouseEvent) => void) | null>(null);
  const onUpRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const saved = localStorage.getItem(LOG_HEIGHT_KEY);
    if (saved) {
      const h = Number(saved);
      if (h >= MIN_LOG_HEIGHT && h <= MAX_LOG_HEIGHT) {
        logHeight.current = h;
        if (logRef.current) logRef.current.style.height = `${h}px`;
      }
    }
  }, []);

  useEffect(() => {
    return () => {
      if (onMoveRef.current) window.removeEventListener('mousemove', onMoveRef.current);
      if (onUpRef.current) window.removeEventListener('mouseup', onUpRef.current);
    };
  }, []);

  const handleResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = logHeight.current;

    const onMove = (ev: MouseEvent) => {
      const next = Math.min(
        MAX_LOG_HEIGHT,
        Math.max(MIN_LOG_HEIGHT, startH - (ev.clientY - startY)),
      );
      logHeight.current = next;
      if (logRef.current) logRef.current.style.height = `${next}px`;
    };

    const onUp = () => {
      localStorage.setItem(LOG_HEIGHT_KEY, String(logHeight.current));
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      onMoveRef.current = null;
      onUpRef.current = null;
    };

    onMoveRef.current = onMove;
    onUpRef.current = onUp;
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  const isSimpleIdle = mode === 'simple' && status === 'idle';
  const isAdvancedIdle = mode === 'advanced' && idle;

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <StatBar />

      {isSimpleIdle || isAdvancedIdle ? (
        <div className='flex flex-1 items-center justify-center text-sm text-muted-foreground'>
          Configure and click Run load test to start.
        </div>
      ) : mode === 'simple' ? (
        <div className='min-h-0 flex-1 overflow-hidden'>
          <RequestLogTable />
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

          {/* 5-chart grid */}
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

          {/* Drag handle + request log */}
          <div
            aria-hidden='true'
            className='group h-[5px] shrink-0 cursor-row-resize border-t border-border/40 bg-transparent transition-colors hover:bg-border/40'
            onMouseDown={handleResizeStart}
          />
          <div
            ref={logRef}
            className='shrink-0 overflow-hidden border-t border-border/40'
            style={{ height: logHeight.current }}
          >
            <RequestLogTable />
          </div>
        </>
      )}
    </div>
  );
}
