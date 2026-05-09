import { formatDistanceToNow } from 'date-fns';
import type { ChangelogEntry, ContractStatus } from '@/types/contracts';
import { ChangeChip } from './ChangeChip';

interface MiniChangelogProps {
  entries: ChangelogEntry[];
  status: ContractStatus;
  onViewAll?: () => void;
}

function railLabel(status: ContractStatus): string {
  if (status === 'paused') return 'Paused state';
  if (status === 'draft') return 'Proposed shape';
  return 'Recent changes';
}

function timeAgo(iso: string): string {
  try {
    const dist = formatDistanceToNow(new Date(iso), { addSuffix: false });
    return dist
      .replace('about ', '')
      .replace(' minutes', 'm')
      .replace(' minute', 'm')
      .replace(' hours', 'h')
      .replace(' hour', 'h')
      .replace(' days', 'd')
      .replace(' day', 'd');
  } catch {
    return '—';
  }
}

export function MiniChangelog({ entries, status, onViewAll }: MiniChangelogProps) {
  const visible = entries.slice(0, 4);

  return (
    <div className='bg-card border border-border rounded-[calc(var(--radius)-2px)] p-3 flex flex-col gap-1 h-full'>
      <div className='flex justify-between items-center mb-1'>
        <span className='text-[10px] font-semibold text-muted-foreground uppercase tracking-wider'>
          {railLabel(status)}
        </span>
        {entries.length > 0 && onViewAll && (
          <button
            type='button'
            onClick={onViewAll}
            className='text-[11px] text-primary hover:underline cursor-pointer'
          >
            View all →
          </button>
        )}
      </div>

      {visible.map((entry) => (
        <div key={entry.id} className='flex items-center gap-2 py-0.5'>
          <span className='text-[11px] text-muted-foreground/70 w-11 shrink-0 tabular-nums'>
            {timeAgo(entry.at)}
          </span>
          <ChangeChip kind={entry.kind} />
          <span className='text-[11px] text-muted-foreground truncate flex-1'>
            <code className='font-mono text-[10px] bg-background px-1 rounded text-foreground'>
              {entry.summary}
            </code>
          </span>
        </div>
      ))}

      {entries.length === 0 && (
        <p className='text-[11px] text-muted-foreground/60 italic py-1'>No changes recorded</p>
      )}
    </div>
  );
}
