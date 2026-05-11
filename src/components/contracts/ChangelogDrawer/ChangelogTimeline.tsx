import { Clock } from 'lucide-react';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { DayGroup } from '@/hooks/useChangelogEntries';
import { cn } from '@/lib/utils';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import type { ChangelogEntry } from '@/types/contracts';
import { ChangelogEmptyState } from './ChangelogEmptyState';
import { ChangelogEntry as ChangelogEntryComponent } from './ChangelogEntry';

interface ChangelogTimelineProps {
  groups: DayGroup[];
}

function noop(_id: string): void {
  /* read-only timeline — actions not wired */
}

// Layout constants:
//   container pl-14 = 56px, spine at left-[34px]
//   entry wrapper left edge = 56px from container
//   dot w-5 h-5 (20px): center must be at 34px → left edge at 26px → -left-[30px] from wrapper
//   connector: dot right = 26+20=46px, card left = 56px → left-[-10px] w-[10px]

function TimelineDot({ kind, isSign }: { kind: ChangelogEntry['kind']; isSign: boolean }) {
  if (isSign) {
    return (
      <span
        aria-hidden='true'
        className='absolute -left-[30px] top-3 w-5 h-5 rounded-full bg-background border-[2.5px] border-muted-foreground flex items-center justify-center z-10'
      >
        <Clock className='w-2.5 h-2.5 text-muted-foreground' />
      </span>
    );
  }
  return (
    <span
      aria-hidden='true'
      className={cn(
        'absolute -left-[30px] top-3 w-5 h-5 rounded-full bg-background border-[2.5px] z-10',
        kind === 'add' && 'border-[hsl(var(--success))] shadow-[0_0_10px_rgba(74,222,128,0.3)]',
        kind === 'remove' && 'border-destructive shadow-[0_0_10px_rgba(248,113,113,0.3)]',
        kind === 'modify' && 'border-[hsl(var(--warning))] shadow-[0_0_10px_rgba(251,191,36,0.25)]',
      )}
    />
  );
}

export function ChangelogTimeline({ groups }: ChangelogTimelineProps) {
  const resetFilters = useDrawerStore((s) => s.resetFilters);

  return (
    <ScrollArea className='flex-1'>
      <div className='py-3'>
        {groups.length === 0 ? (
          <ChangelogEmptyState onReset={resetFilters} />
        ) : (
          <div role='feed' aria-label='Changelog timeline' className='relative pl-14 pr-5'>
            {/* Spine with fade at top and bottom */}
            <span
              aria-hidden='true'
              className='absolute left-[34px] top-0 bottom-0 w-px pointer-events-none'
              style={{
                background:
                  'linear-gradient(to bottom, transparent, hsl(var(--border)) 6%, hsl(var(--border)) 94%, transparent)',
              }}
            />

            {groups.map((group, i) => (
              <div key={group.day}>
                {/* Day header */}
                <div
                  className={cn(
                    'text-[12px] font-semibold uppercase tracking-[0.07em] text-muted-foreground flex items-center gap-2 mb-3 relative',
                    i === 0 ? 'mt-1' : 'mt-7',
                  )}
                >
                  <span
                    aria-hidden='true'
                    className='absolute left-[-22px] top-1/2 -translate-y-1/2 -translate-x-1/2 w-3 h-3 rounded-full bg-background border-[1.5px] border-border z-10'
                  />
                  {group.label}
                  <span className='flex-1 h-px bg-border/40' />
                </div>

                {group.items.map((entry) => (
                  <div key={entry.id} className='relative mb-3'>
                    <TimelineDot kind={entry.kind} isSign={entry.isSignEvent === true} />
                    {/* Connector: from dot right edge to card left */}
                    <span
                      aria-hidden='true'
                      className='absolute -left-[10px] top-[23px] w-[10px] h-px bg-border/50'
                    />
                    <ChangelogEntryComponent
                      entry={entry}
                      onMarkBreaking={noop}
                      onMarkNonBreaking={noop}
                      onNotifyConsumer={noop}
                      onSnapshotToContract={noop}
                    />
                  </div>
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
    </ScrollArea>
  );
}
