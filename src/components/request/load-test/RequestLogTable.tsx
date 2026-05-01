import { useLoadTestStore } from '@/stores/load-test-store';

export function RequestLogTable() {
  const requestLog = useLoadTestStore((s) => s.requestLog);
  const recent = requestLog.slice(-50);

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <div className='border-b border-border/40 bg-muted/30 px-3 py-1.5'>
        <span className='text-[11px] font-medium text-muted-foreground'>Per-request log</span>
        <span className='ml-2 text-[10px] font-normal'>showing last 50 of {requestLog.length}</span>
      </div>
      <div className='overflow-y-auto'>
        <table className='w-full text-[11px]'>
          <thead className='sticky top-0 bg-muted/50'>
            <tr>
              <th className='px-3 py-1 text-left font-medium'>#</th>
              <th className='px-3 py-1 text-left font-medium'>Status</th>
              <th className='px-3 py-1 text-left font-medium'>Latency</th>
              <th className='px-3 py-1 text-left font-medium'>Size</th>
              <th className='px-3 py-1 text-left font-medium'>Phase</th>
              <th className='px-3 py-1 text-left font-medium'>Error</th>
            </tr>
          </thead>
          <tbody>
            {recent.length === 0 ? (
              <tr>
                <td colSpan={6} className='px-3 py-4 text-center text-muted-foreground'>
                  No requests yet
                </td>
              </tr>
            ) : (
              recent.map((entry) => {
                const ok = entry.status !== null && entry.status < 400;
                return (
                  <tr key={entry.seq} className='border-b border-border/20'>
                    <td className='px-3 py-1'>{entry.seq}</td>
                    <td className='px-3 py-1' style={ok ? { color: '#1D9E75' } : undefined}>
                      <span className={ok ? '' : 'text-destructive'}>{entry.status ?? 'ERR'}</span>
                    </td>
                    <td className='px-3 py-1'>{entry.latencyMs.toFixed(1)}ms</td>
                    <td className='px-3 py-1'>{entry.responseBytes}b</td>
                    <td className='px-3 py-1'>{entry.phaseIndex}</td>
                    <td className='max-w-[200px] truncate px-3 py-1 text-muted-foreground'>
                      {entry.error ?? '—'}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
