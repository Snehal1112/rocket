import { Play, RotateCcw, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { getRunnerSummary } from '@/lib/runner-summary';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';

export function RunnerSummaryHeader({ tab }: { tab: RunnerTab }) {
  const startRun = usePaneStore((s) => s.startRun);
  const stopRun = usePaneStore((s) => s.stopRun);
  const rerunAll = usePaneStore((s) => s.rerunAll);

  const summary = getRunnerSummary(tab);
  const isRunning = tab.runState === 'running';
  const isFinished = tab.runState === 'done' || tab.runState === 'stopped';

  return (
    <div className='flex items-center gap-3 px-3 py-2 border-b shrink-0 text-sm'>
      <span className='text-muted-foreground'>
        {summary.total === 0
          ? 'No requests'
          : `${summary.passed + summary.failed + summary.skipped} / ${summary.included} complete`}
      </span>
      {summary.passed > 0 && (
        <span className='text-green-600 dark:text-green-400'>{summary.passed} passed</span>
      )}
      {summary.failed > 0 && (
        <span className='text-red-600 dark:text-red-400'>{summary.failed} failed</span>
      )}
      {summary.skipped > 0 && (
        <span className='text-muted-foreground'>{summary.skipped} skipped</span>
      )}

      <div className='ml-auto flex items-center gap-2'>
        {isRunning ? (
          <Button size='sm' variant='outline' onClick={() => stopRun(tab.id)}>
            <Square className='h-3.5 w-3.5 mr-1.5' /> Stop
          </Button>
        ) : isFinished ? (
          <Button size='sm' onClick={() => void rerunAll(tab.id)}>
            <RotateCcw className='h-3.5 w-3.5 mr-1.5' /> Re-run
          </Button>
        ) : (
          <Button size='sm' disabled={summary.included === 0} onClick={() => void startRun(tab.id)}>
            <Play className='h-3.5 w-3.5 mr-1.5' /> Start
          </Button>
        )}
      </div>
    </div>
  );
}
