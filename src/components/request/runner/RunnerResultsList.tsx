import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CircleDashed,
  SkipForward,
  XCircle,
} from 'lucide-react';
import { useState } from 'react';
import { TestsPanel } from '@/components/response/TestsPanel';
import { METHOD_TEXT_COLOR, statusTextColor } from '@/lib/colors';
import { cn } from '@/lib/utils';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';

function StatusIcon({ status }: { status: RunnerRequestEntry['status'] }) {
  switch (status) {
    case 'passed':
      return <CheckCircle2 className='h-4 w-4 text-green-500 shrink-0' />;
    case 'failed':
      return <XCircle className='h-4 w-4 text-red-500 shrink-0' />;
    case 'skipped':
      return <SkipForward className='h-4 w-4 text-muted-foreground shrink-0' />;
    default:
      return <CircleDashed className='h-4 w-4 text-muted-foreground shrink-0 animate-pulse' />;
  }
}

function ResultRow({ entry }: { entry: RunnerRequestEntry }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetail = Boolean(entry.result);

  return (
    <div className='border-b last:border-b-0'>
      <button
        type='button'
        className='flex w-full items-center gap-3 px-3 py-2 text-sm text-left'
        onClick={() => hasDetail && setExpanded((v) => !v)}
      >
        {hasDetail ? (
          expanded ? (
            <ChevronDown className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          ) : (
            <ChevronRight className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          )
        ) : (
          <span className='w-3.5 shrink-0' />
        )}
        <StatusIcon status={entry.status} />
        <span
          className={cn(
            'w-14 shrink-0 font-mono text-xs font-medium',
            METHOD_TEXT_COLOR[entry.request.method] ?? 'text-muted-foreground',
          )}
        >
          {entry.request.method}
        </span>
        <span className='truncate text-foreground'>{entry.request.name}</span>
        {entry.status === 'skipped' && (
          <span className='ml-2 text-xs text-muted-foreground'>skipped</span>
        )}
        {entry.error && (
          <span className='ml-auto shrink-0 text-xs text-red-500 font-mono break-all'>
            {entry.error}
          </span>
        )}
        {entry.result && (
          <span
            className={cn(
              'ml-auto shrink-0 text-xs font-mono',
              statusTextColor(entry.result.status),
            )}
          >
            {entry.result.status} · {entry.result.durationMs}ms
          </span>
        )}
      </button>
      {expanded && (
        <div className='px-3 pb-3'>
          <div className='h-48 border rounded'>
            <TestsPanel results={entry.result?.testResults ?? []} />
          </div>
        </div>
      )}
    </div>
  );
}

export function RunnerResultsList({ tab }: { tab: RunnerTab }) {
  if (tab.requests.length === 0) {
    return (
      <div className='flex items-center justify-center h-full text-sm text-muted-foreground'>
        No requests found in this collection/folder.
      </div>
    );
  }

  return (
    <div className='flex-1 overflow-auto'>
      {tab.requests.map((entry) => (
        <ResultRow key={entry.requestPath} entry={entry} />
      ))}
    </div>
  );
}
