import { Play, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { useLoadTestStore } from '@/stores/load-test-store';
import type { RequestState } from '@/types/pane-types';
import { ExportMenu } from './ExportMenu';
import { LiveDashboard } from './LiveDashboard';
import { PhaseBuilder } from './PhaseBuilder';

interface Props {
  request: RequestState;
  tabId: string;
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

  return (
    <div className='flex h-full min-h-0 overflow-hidden'>
      <aside className='flex w-52 shrink-0 flex-col gap-4 overflow-y-auto border-r border-border/40 bg-background p-3'>
        <div className='flex flex-col gap-2'>
          <h3 className='text-[10px] font-medium uppercase tracking-wider text-muted-foreground'>
            Configuration
          </h3>
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
        </div>

        <Separator />

        <div className='flex flex-col gap-2'>
          <h3 className='text-[10px] font-medium uppercase tracking-wider text-muted-foreground'>
            Ramp-up phases
          </h3>
          <PhaseBuilder phases={phases} onChange={setPhases} />
        </div>

        {error && (
          <>
            <Separator />
            <p className='text-[11px] text-destructive'>{error}</p>
          </>
        )}

        <div className='mt-auto flex flex-col gap-2'>
          {isRunning ? (
            <Button variant='outline' className='w-full' onClick={() => stopTest()}>
              <Square className='mr-2 h-3.5 w-3.5' />
              Stop
            </Button>
          ) : (
            <Button
              className='w-full bg-[#533AB7] text-[#EEEDFE] hover:bg-[#7F77DD]'
              onClick={() => startTest(request, tabId)}
            >
              <Play className='mr-2 h-3.5 w-3.5' />
              Run load test
            </Button>
          )}
          <ExportMenu />
        </div>
      </aside>

      <div className='flex min-w-0 flex-1 flex-col overflow-hidden'>
        <LiveDashboard />
      </div>
    </div>
  );
}
