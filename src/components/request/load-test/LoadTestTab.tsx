import { Activity, GripVertical, Play, Settings, Square } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useLoadTestStore } from '@/stores/load-test-store';
import type { RequestState } from '@/types/pane-types';
import { ExportMenu } from './ExportMenu';
import { LiveDashboard } from './LiveDashboard';
import { PhaseBuilder } from './PhaseBuilder';

const SIDEBAR_WIDTH_KEY = 'load-test-sidebar-width';
const DEFAULT_WIDTH = 260;
const MIN_WIDTH = 180;
const MAX_WIDTH = 480;

interface Props {
  request: RequestState;
  tabId: string;
}

function SectionHeader({ icon: Icon, label }: { icon: React.ElementType; label: string }) {
  return (
    <div className='mb-1 flex items-center gap-2'>
      <Icon className='h-3.5 w-3.5 text-muted-foreground' />
      <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
        {label}
      </span>
    </div>
  );
}

function IntegerField({
  id,
  label,
  value,
  min,
  disabled,
  optional,
  onChange,
}: {
  id: string;
  label: string;
  value: number | undefined;
  min?: number;
  disabled: boolean;
  optional?: boolean;
  onChange: (v: number | undefined) => void;
}) {
  return (
    <div className='flex flex-col gap-1.5'>
      <Label htmlFor={id} className='text-xs'>
        {label}
      </Label>
      <Input
        id={id}
        type='number'
        step={1}
        min={min}
        value={value ?? ''}
        placeholder={optional ? 'None' : undefined}
        onChange={(e) => {
          const raw = e.target.value;
          if (optional && raw === '') {
            onChange(undefined);
          } else {
            const n = Math.floor(Number(raw));
            if (!Number.isNaN(n)) onChange(n);
          }
        }}
        disabled={disabled}
        className='h-7 text-xs'
      />
    </div>
  );
}

export function LoadTestTab({ request, tabId }: Props) {
  const mode = useLoadTestStore((s) => s.mode);
  const simpleConfig = useLoadTestStore((s) => s.simpleConfig);
  const phases = useLoadTestStore((s) => s.phases);
  const targetUnit = useLoadTestStore((s) => s.targetUnit);
  const successStatusBelow = useLoadTestStore((s) => s.successStatusBelow);
  const ringBufferSize = useLoadTestStore((s) => s.ringBufferSize);
  const status = useLoadTestStore((s) => s.status);
  const error = useLoadTestStore((s) => s.error);

  const setMode = useLoadTestStore((s) => s.setMode);
  const setSimpleConfig = useLoadTestStore((s) => s.setSimpleConfig);
  const setPhases = useLoadTestStore((s) => s.setPhases);
  const setTargetUnit = useLoadTestStore((s) => s.setTargetUnit);
  const setSuccessStatusBelow = useLoadTestStore((s) => s.setSuccessStatusBelow);
  const setRingBufferSize = useLoadTestStore((s) => s.setRingBufferSize);
  const startTest = useLoadTestStore((s) => s.startTest);
  const stopTest = useLoadTestStore((s) => s.stopTest);

  const isRunning = status === 'running';

  const asideRef = useRef<HTMLDivElement>(null);
  const sidebarWidth = useRef<number>(DEFAULT_WIDTH);
  const isDragging = useRef(false);
  const onMoveRef = useRef<((ev: MouseEvent) => void) | null>(null);
  const onUpRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const saved = localStorage.getItem(SIDEBAR_WIDTH_KEY);
    if (saved) {
      const w = Number(saved);
      if (w >= MIN_WIDTH && w <= MAX_WIDTH) {
        sidebarWidth.current = w;
        if (asideRef.current) asideRef.current.style.width = `${w}px`;
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
    isDragging.current = true;
    const startX = e.clientX;
    const startW = sidebarWidth.current;

    const onMove = (ev: MouseEvent) => {
      if (!isDragging.current) return;
      const next = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startW + ev.clientX - startX));
      sidebarWidth.current = next;
      if (asideRef.current) asideRef.current.style.width = `${next}px`;
    };

    const onUp = () => {
      isDragging.current = false;
      localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth.current));
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

  return (
    <div className='flex h-full min-h-0 overflow-hidden'>
      <aside
        ref={asideRef}
        style={{ width: DEFAULT_WIDTH }}
        className='flex shrink-0 flex-col gap-3 overflow-y-auto bg-background p-3'
      >
        {/* Mode toggle */}
        <div className='flex rounded-md border border-border/60 p-0.5'>
          <button
            type='button'
            className={`flex-1 rounded py-1 text-xs font-medium transition-colors ${
              mode === 'simple'
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground'
            }`}
            onClick={() => setMode('simple')}
            disabled={isRunning}
          >
            Simple
          </button>
          <button
            type='button'
            className={`flex-1 rounded py-1 text-xs font-medium transition-colors ${
              mode === 'advanced'
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground'
            }`}
            onClick={() => setMode('advanced')}
            disabled={isRunning}
          >
            Advanced
          </button>
        </div>

        {/* Mode-specific config */}
        {mode === 'simple' ? (
          <Card>
            <CardContent className='space-y-3 p-3'>
              <SectionHeader icon={Settings} label='Configuration' />
              <IntegerField
                id='concurrency'
                label='Concurrency'
                value={simpleConfig.concurrency}
                min={1}
                disabled={isRunning}
                onChange={(v) => setSimpleConfig({ concurrency: v ?? 1 })}
              />
              <IntegerField
                id='total-requests'
                label='Total requests'
                value={simpleConfig.totalRequests}
                min={1}
                disabled={isRunning}
                onChange={(v) => setSimpleConfig({ totalRequests: v ?? 1 })}
              />
              <IntegerField
                id='interval-ms'
                label='Delay between (ms)'
                value={simpleConfig.intervalMs}
                min={0}
                disabled={isRunning}
                onChange={(v) => setSimpleConfig({ intervalMs: v ?? 0 })}
              />
              <IntegerField
                id='duration-cap'
                label='Duration cap (s)'
                value={simpleConfig.durationCapSecs}
                min={1}
                disabled={isRunning}
                optional
                onChange={(v) => setSimpleConfig({ durationCapSecs: v })}
              />
            </CardContent>
          </Card>
        ) : (
          <Card>
            <CardContent className='space-y-2 p-3'>
              <SectionHeader icon={Activity} label='Ramp-up phases' />

              {/* Workload type — chooses concurrency or rps for all phases */}
              <div className='flex rounded-md border border-border/60 p-0.5'>
                <button
                  type='button'
                  className={`flex-1 rounded py-1 text-[11px] font-medium transition-colors ${
                    targetUnit === 'concurrency'
                      ? 'bg-muted text-foreground'
                      : 'text-muted-foreground hover:text-foreground'
                  }`}
                  onClick={() => setTargetUnit('concurrency')}
                  disabled={isRunning}
                >
                  Concurrent users
                </button>
                <button
                  type='button'
                  className={`flex-1 rounded py-1 text-[11px] font-medium transition-colors ${
                    targetUnit === 'rps'
                      ? 'bg-muted text-foreground'
                      : 'text-muted-foreground hover:text-foreground'
                  }`}
                  onClick={() => setTargetUnit('rps')}
                  disabled={isRunning}
                >
                  Throughput
                </button>
              </div>

              <PhaseBuilder
                phases={phases}
                onChange={setPhases}
                disabled={isRunning}
                unit={targetUnit}
              />
            </CardContent>
          </Card>
        )}

        {/* Configuration — always visible */}
        <Card>
          <CardContent className='space-y-3 p-3'>
            <SectionHeader icon={Settings} label='Configuration' />
            <div className='flex flex-col gap-1.5'>
              <Label htmlFor='success-status-below' className='text-xs'>
                Success if status &lt;
              </Label>
              <Input
                id='success-status-below'
                type='number'
                min={100}
                max={600}
                value={successStatusBelow}
                onChange={(e) => setSuccessStatusBelow(Number(e.target.value))}
                disabled={isRunning}
                className='h-7 text-xs'
              />
            </div>
            {mode === 'advanced' && (
              <div className='flex flex-col gap-1.5'>
                <Label htmlFor='ring-buffer-size' className='text-xs'>
                  Request log size
                </Label>
                <Input
                  id='ring-buffer-size'
                  type='number'
                  min={100}
                  max={100000}
                  step={100}
                  value={ringBufferSize}
                  onChange={(e) => {
                    const n = Math.floor(Number(e.target.value));
                    if (!Number.isNaN(n) && n > 0) setRingBufferSize(n);
                  }}
                  disabled={isRunning}
                  className='h-7 text-xs'
                />
              </div>
            )}
          </CardContent>
        </Card>

        {error && <p className='px-1 text-[11px] text-destructive'>{error}</p>}

        <div className='mt-auto flex flex-col gap-2'>
          {isRunning ? (
            <Button variant='outline' className='w-full' onClick={() => stopTest()}>
              <Square className='mr-2 h-3.5 w-3.5' />
              Stop
            </Button>
          ) : (
            <Button
              className='w-full bg-chart-4 text-primary-foreground hover:bg-chart-4/80'
              onClick={() => startTest(request, tabId)}
            >
              <Play className='mr-2 h-3.5 w-3.5' />
              Run load test
            </Button>
          )}
          <ExportMenu />
        </div>
      </aside>

      {/* Drag handle */}
      <div
        className='flex w-[5px] shrink-0 cursor-col-resize items-center justify-center border-l border-r border-border/40 bg-transparent transition-colors hover:bg-border/40'
        onMouseDown={handleResizeStart}
        aria-hidden='true'
      >
        <GripVertical className='h-4 w-4 text-border' />
      </div>

      <div className='flex min-w-0 flex-1 flex-col overflow-hidden'>
        <LiveDashboard />
      </div>
    </div>
  );
}
