import { useMemo } from 'react';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { cn } from '@/lib/utils';
import { useLoadTestStore } from '@/stores/load-test-store';

export function RequestLogTable() {
  const requestLog = useLoadTestStore((s) => s.requestLog);
  const recent = useMemo(() => requestLog.slice(-50), [requestLog]);

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <div className='border-b border-border/40 bg-muted/30 px-3 py-1.5'>
        <span className='text-[11px] font-medium text-muted-foreground'>Per-request log</span>
        <span className='ml-2 text-[10px] font-normal text-muted-foreground'>
          showing last 50 of {requestLog.length}
        </span>
      </div>
      <div className='overflow-y-auto'>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className='h-7 text-xs'>#</TableHead>
              <TableHead className='h-7 text-xs'>Status</TableHead>
              <TableHead className='h-7 text-xs'>Latency</TableHead>
              <TableHead className='h-7 text-xs'>Size</TableHead>
              <TableHead className='h-7 text-xs'>Phase</TableHead>
              <TableHead className='h-7 text-xs'>Error</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {recent.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6} className='py-4 text-center text-xs text-muted-foreground'>
                  No requests yet
                </TableCell>
              </TableRow>
            ) : (
              recent.map((entry) => {
                const ok = entry.status !== null && entry.status < 400;
                return (
                  <TableRow key={entry.seq}>
                    <TableCell className='px-3 py-1 text-xs text-muted-foreground'>
                      {entry.seq}
                    </TableCell>
                    <TableCell
                      className={cn(
                        'px-3 py-1 text-xs font-medium',
                        ok ? 'text-chart-2' : 'text-destructive',
                      )}
                    >
                      {entry.status ?? 'ERR'}
                    </TableCell>
                    <TableCell className='px-3 py-1 text-xs'>
                      {entry.latencyMs.toFixed(1)}ms
                    </TableCell>
                    <TableCell className='px-3 py-1 text-xs'>{entry.responseBytes}b</TableCell>
                    <TableCell className='px-3 py-1 text-xs'>{entry.phaseIndex}</TableCell>
                    <TableCell className='max-w-[200px] truncate px-3 py-1 text-xs text-muted-foreground'>
                      {entry.error ?? '—'}
                    </TableCell>
                  </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
