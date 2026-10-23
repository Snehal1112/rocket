import { Checkbox } from '@/components/ui/checkbox';
import { EmptyState } from '@/components/ui/empty-state';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { runnerScopeLabel } from './runner-scope';

export function RunnerRequestList({ tab }: { tab: RunnerTab }) {
  const toggleRunnerEntry = usePaneStore((s) => s.toggleRunnerEntry);

  if (tab.requests.length === 0) {
    return (
      <EmptyState
        size='compact'
        cause='none-yet'
        title='No requests to run'
        reason={`${runnerScopeLabel(tab)} has no requests. Add one, then open the runner again.`}
      />
    );
  }

  return (
    <div className='flex-1 overflow-auto'>
      {tab.requests.map((entry) => (
        <div
          key={entry.requestPath}
          className='flex items-center gap-3 px-3 py-2 border-b last:border-b-0 text-sm'
        >
          <Checkbox
            checked={entry.included}
            disabled={tab.runState === 'running'}
            onCheckedChange={() => toggleRunnerEntry(tab.id, entry.requestPath)}
            aria-label={`${entry.included ? 'Exclude' : 'Include'} ${entry.request.name}`}
          />
          <span
            className={cn(
              'w-14 shrink-0 font-mono text-xs font-medium',
              METHOD_TEXT_COLOR[entry.graphql ? 'POST' : entry.request.method] ??
                'text-muted-foreground',
            )}
          >
            {entry.graphql ? 'GQL' : entry.request.method}
          </span>
          <span className='truncate text-foreground'>{entry.request.name}</span>
          <span className='ml-auto shrink-0 truncate text-xs text-muted-foreground'>
            {entry.requestPath}
          </span>
        </div>
      ))}
    </div>
  );
}
