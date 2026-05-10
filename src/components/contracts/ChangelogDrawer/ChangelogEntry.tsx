import { AlertTriangle, ChevronRight, Clock, PenLine } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { avatarColorForName, initialsForName } from '@/lib/contracts/avatarColor';
import { cn } from '@/lib/utils';
import type { ChangelogEntry as ChangelogEntryType } from '@/types/contracts';
import { ChangelogDiffBlock } from './ChangelogDiffBlock';

interface ChangelogEntryProps {
  entry: ChangelogEntryType;
  onOpenRequest?: (requestId: string) => void;
  onMarkBreaking?: (entryId: string) => void;
  onMarkNonBreaking?: (entryId: string) => void;
  onNotifyConsumer?: (entryId: string) => void;
  onSnapshotToContract?: (entryId: string) => void;
}

function timeAgo(iso: string): string {
  try {
    const diffMs = Date.now() - new Date(iso).getTime()
    const mins = Math.floor(diffMs / 60_000)
    if (mins < 1) return 'now'
    if (mins < 60) return `${mins}m`
    const hours = Math.floor(mins / 60)
    if (hours < 24) return `${hours}h`
    const days = Math.floor(hours / 24)
    if (days < 30) return `${days}d`
    const months = Math.floor(days / 30)
    if (months < 12) return `${months}mo`
    return `${Math.floor(months / 12)}y`
  } catch {
    return '—'
  }
}

function KindChip({ kind }: { kind: ChangelogEntryType['kind'] }) {
  if (kind === 'add') {
    return (
      <span className='text-[10px] font-bold uppercase px-1.5 py-0.5 rounded bg-[hsl(var(--success)/0.12)] text-[hsl(var(--success))]'>
        ADD
      </span>
    );
  }
  if (kind === 'remove') {
    return (
      <span className='text-[10px] font-bold uppercase px-1.5 py-0.5 rounded bg-destructive/15 text-destructive'>
        REM
      </span>
    );
  }
  return (
    <span className='text-[10px] font-bold uppercase px-1.5 py-0.5 rounded bg-[hsl(var(--warning)/0.12)] text-[hsl(var(--warning))]'>
      MOD
    </span>
  );
}

function Dot({ kind, isSign }: { kind: ChangelogEntryType['kind']; isSign?: boolean }) {
  if (isSign) {
    return (
      <span className='absolute -left-7 top-3.5 w-3 h-3 rounded-full border-2 border-background bg-muted-foreground flex items-center justify-center'>
        <Clock className='w-1.5 h-1.5 text-background' aria-hidden='true' />
      </span>
    );
  }
  return (
    <span
      className={cn(
        'absolute -left-7 top-3.5 w-3 h-3 rounded-full border-2 border-background',
        kind === 'add' && 'bg-[hsl(var(--success))]',
        kind === 'remove' && 'bg-destructive',
        kind === 'modify' && 'bg-[hsl(var(--warning))]',
      )}
    />
  );
}

export function ChangelogEntry({
  entry,
  onOpenRequest,
  onMarkBreaking,
  onMarkNonBreaking,
  onNotifyConsumer,
  onSnapshotToContract,
}: ChangelogEntryProps) {
  const isSign = entry.isSignEvent === true;
  const hasTarget = !isSign && (entry.requestMethod || entry.requestPath);
  const hasDiff = !isSign && entry.diffLines && entry.diffLines.length > 0;
  const hasAuthor = Boolean(entry.authorName);

  const avatarBg = entry.authorName
    ? avatarColorForName(entry.authorAvatarSeed ?? entry.authorName)
    : '#6b7280';
  const initials = entry.authorName ? initialsForName(entry.authorName) : '?';

  return (
    <div
      className={cn(
        'relative px-3 py-2 rounded-md hover:bg-accent group',
        entry.isBreaking && !isSign && 'bg-destructive/10 hover:bg-destructive/15',
      )}
    >
      <Dot kind={entry.kind} isSign={isSign} />

      {/* Line 1: chip + breaking tag + summary + time */}
      <div className='flex items-center gap-1.5'>
        {!isSign && <KindChip kind={entry.kind} />}
        {isSign && (
          <PenLine className='w-3 h-3 text-muted-foreground shrink-0' aria-hidden='true' />
        )}
        {entry.isBreaking && !isSign && (
          <span className='bg-destructive/20 text-destructive text-[10px] font-bold uppercase px-1.5 py-0.5 rounded flex items-center gap-0.5'>
            <AlertTriangle className='w-2.5 h-2.5' aria-hidden='true' />
            BREAKING
          </span>
        )}
        <span className='text-[12px] text-foreground flex-1'>
          {isSign ? entry.signEventLabel : entry.summary}
        </span>
        <span className='text-[11px] text-muted-foreground/70 tabular-nums shrink-0'>
          {timeAgo(entry.at)}
        </span>
      </div>

      {/* Target pill */}
      {hasTarget && (
        <div className='mt-1'>
          <span className='font-mono text-[10px] bg-background border border-border px-1.5 py-0.5 rounded mt-1 inline-flex gap-1 items-center'>
            {entry.requestMethod && (
              <span className='text-primary font-bold'>{entry.requestMethod}</span>
            )}
            {entry.requestPath && <span>{entry.requestPath}</span>}
          </span>
        </div>
      )}

      {/* Diff block */}
      {hasDiff && <ChangelogDiffBlock diffLines={entry.diffLines!} />}

      {/* Author row */}
      {hasAuthor && (
        <div className='flex items-center gap-1.5 mt-1'>
          <span
            className='w-4 h-4 rounded-full text-[9px] font-bold flex items-center justify-center text-primary-foreground shrink-0'
            style={{ backgroundColor: avatarBg }}
            aria-hidden='true'
          >
            {initials}
          </span>
          <span className='text-[11px] text-muted-foreground'>{entry.authorName}</span>
        </div>
      )}

      {/* Actions row (hover-revealed) */}
      {!isSign && (
        <div className='flex items-center gap-1.5 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity'>
          {entry.requestId && onOpenRequest && (
            <Button
              variant='outline'
              size='sm'
              className='text-[11px] px-2 h-6 rounded border border-border hover:bg-accent'
              onClick={() => onOpenRequest(entry.requestId!)}
            >
              Open request
            </Button>
          )}
          {(entry.kind === 'remove' || entry.kind === 'modify') && entry.isBreaking && onMarkNonBreaking && (
            <Button
              variant='outline'
              size='sm'
              className='text-[11px] px-2 h-6 rounded border border-border hover:bg-accent'
              onClick={() => onMarkNonBreaking(entry.id)}
            >
              Mark non-breaking
            </Button>
          )}
          {(entry.kind === 'remove' || entry.kind === 'modify') && entry.isBreaking && onNotifyConsumer && (
            <Button
              variant='outline'
              size='sm'
              className='text-[11px] px-2 h-6 rounded border border-border hover:bg-accent'
              onClick={() => onNotifyConsumer(entry.id)}
            >
              Notify consumer <ChevronRight className="w-3 h-3" aria-hidden="true" />
            </Button>
          )}
          {(entry.kind === 'remove' || entry.kind === 'modify') && !entry.isBreaking && onMarkBreaking && (
            <Button
              variant='outline'
              size='sm'
              className='text-[11px] px-2 h-6 rounded border border-border hover:bg-accent'
              onClick={() => onMarkBreaking(entry.id)}
            >
              Mark breaking
            </Button>
          )}
          {entry.kind === 'add' && onSnapshotToContract && (
            <Button
              variant='outline'
              size='sm'
              className='text-[11px] px-2 h-6 rounded border border-border hover:bg-accent'
              onClick={() => onSnapshotToContract(entry.id)}
            >
              Snapshot to contract
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
