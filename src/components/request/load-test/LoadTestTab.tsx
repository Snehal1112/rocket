import { Activity, GripVertical, Play, ShieldCheck, Square } from 'lucide-react';
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
    <div className='flex items-center gap-2 mb-1'>
      <Icon className='h-3.5 w-3.5 text-muted-foreground' />
      <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
        {label}
      </span>
    </div>
  );
}

export function LoadTestTab({ request, tabId }: Props) {
  const phases = useLoadTestStore((s) => s.phases);
  const successStatusBelow = useLoadTestStore((s) => s.successStatusBelow);
  const status = useLoadTestStore((s) => s.status);
  const error = useLoadTestStore((s) => s.error);
  const setPhases = useLoadTestStore((s) => s.setPhases);
  const setSuccessStatusBelow = useLoadTestStore((s) => s.setSuccessStatusBelow);
  const startTest = useLoadTestStore((s) => s.startTest);
  const stopTest = useLoadTestStore((s) => s.stopTest);

  const isRunning = status === 'running';

  const asideRef = useRef<HTMLDivElement>(null);
  const sidebarWidth = useRef<number>(DEFAULT_WIDTH);
  const isDragging = useRef(false);
  const onMoveRef = useRef<((ev: MouseEvent) => void) | null>(null);
  const onUpRef = useRef<(() => void) | null>(null);

  // Apply persisted width on mount.
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

  // Remove any dangling drag listeners when the component unmounts.
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
        <Card>
          <CardContent className='p-3 space-y-2'>
            <SectionHeader icon={ShieldCheck} label='Success rule' />
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
          </CardContent>
        </Card>

        <Card>
          <CardContent className='p-3 space-y-2'>
            <SectionHeader icon={Activity} label='Ramp-up phases' />
            <PhaseBuilder phases={phases} onChange={setPhases} disabled={isRunning} />
          </CardContent>
        </Card>

        {error && <p className='text-[11px] text-destructive px-1'>{error}</p>}

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
        className='w-[5px] shrink-0 cursor-col-resize border-l border-r border-border/40 bg-transparent hover:bg-border/40 transition-colors flex items-center justify-center'
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
