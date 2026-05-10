import { formatDistanceToNow } from 'date-fns';
import { Card } from '@/components/ui/card';
import type { ChangelogEntry, ContractStatus } from '@/types/contracts';
import { ChangeChip } from './ChangeChip';

interface MiniChangelogProps {
  entries: ChangelogEntry[];
  status: ContractStatus;
  effectiveAt?: string;
  endpointCount?: number;
  /** Passed for paused cards — total consumer count for migration stat line */
  consumersCount?: number;
  /** Passed for paused cards — successor contract name */
  successorName?: string;
  onViewAll?: () => void;
}

function railLabel(status: ContractStatus): string {
  if (status === 'paused') return 'Paused state';
  if (status === 'expired') return 'Historical';
  if (status === 'draft' || status === 'in_review') return 'Proposed shape';
  return 'Recent changes';
}

function entryTimestamp(iso: string): string {
  try {
    const date = new Date(iso);
    const ageDays = (Date.now() - date.getTime()) / 86_400_000;
    if (ageDays < 30) {
      return formatDistanceToNow(date, { addSuffix: false })
        .replace('about ', '')
        .replace(' minutes', 'm')
        .replace(' minute', 'm')
        .replace(' hours', 'h')
        .replace(' hour', 'h')
        .replace(' days', 'd')
        .replace(' day', 'd');
    }
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  } catch {
    return '—';
  }
}

function formatSinceDate(iso: string): string {
  try {
    return new Date(`${iso}T00:00:00`).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}

function SysChip() {
  return (
    <span className='inline-flex items-center px-1.5 py-0.5 rounded-[3px] border text-[10px] font-mono font-semibold shrink-0 bg-muted text-muted-foreground border-border'>
      SYS
    </span>
  );
}

export function MiniChangelog({
  entries,
  status,
  effectiveAt,
  endpointCount,
  consumersCount,
  successorName,
  onViewAll,
}: MiniChangelogProps) {
  const visible = entries.slice(0, 4);

  return (
    <Card className='rounded-[calc(var(--radius)-2px)] p-3 flex flex-col gap-1 h-full'>
      <div className='flex justify-between items-center mb-1'>
        <span className='text-[10px] font-semibold text-muted-foreground uppercase tracking-wider'>
          {railLabel(status)}
        </span>
        {status !== 'paused' && entries.length > 0 && onViewAll && (
          <button
            type='button'
            onClick={onViewAll}
            className='text-[11px] text-primary hover:underline cursor-pointer'
          >
            View all →
          </button>
        )}
      </div>

      {/* Paused: static description + optional migration stat */}
      {status === 'paused' && (
        <div className='flex flex-col gap-1.5'>
          <p className='text-[11px] text-muted-foreground/70 leading-relaxed py-1'>
            Changes are not being tracked while paused. Consumers still reference this version until
            migration completes.
          </p>
          {successorName && consumersCount !== undefined && consumersCount > 0 && (
            <p className='text-[11px] text-muted-foreground/80'>
              <span className='font-semibold text-foreground/80'>{consumersCount}</span>
              {' '}consumer{consumersCount !== 1 ? 's' : ''} → migrating to{' '}
              <span className='font-semibold text-foreground/80'>{successorName}</span>
            </p>
          )}
        </div>
      )}

      {/* All other statuses: entry list */}
      {status !== 'paused' && (
        <>
          {visible.map((entry) => (
            <div key={entry.id} className='flex items-center gap-2 py-0.5'>
              <span className='text-[11px] text-muted-foreground/70 w-11 shrink-0 tabular-nums'>
                {entryTimestamp(entry.at)}
              </span>
              {entry.isSignEvent ? <SysChip /> : <ChangeChip kind={entry.kind} />}
              <span className='text-[11px] text-muted-foreground truncate flex-1'>
                <code className='font-mono text-[10px] bg-background px-1 rounded text-foreground'>
                  {entry.isSignEvent ? (entry.signEventLabel ?? entry.summary) : entry.summary}
                </code>
              </span>
            </div>
          ))}

          {entries.length === 0 && (
            <div className='py-1 space-y-0.5'>
              <p className='text-[11px] text-muted-foreground/60'>
                No changes recorded{effectiveAt ? ` since ${formatSinceDate(effectiveAt)}` : ''}.
              </p>
              {endpointCount !== undefined && endpointCount > 0 && (
                <p className='text-[11px] text-muted-foreground/60'>
                  Monitoring {endpointCount} endpoint{endpointCount !== 1 ? 's' : ''}.
                </p>
              )}
            </div>
          )}

          {/* Expired: frozen footer */}
          {status === 'expired' && entries.length > 0 && (
            <div className='mt-auto pt-2 border-t border-dashed border-border/50'>
              <span className='text-[10px] text-muted-foreground/50'>
                Future · {entries.length} total historical change{entries.length !== 1 ? 's' : ''}
              </span>
            </div>
          )}
        </>
      )}
    </Card>
  );
}
