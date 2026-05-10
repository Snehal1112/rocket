import { useDrawerStore } from '@/stores/contracts/drawerSlice'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { DayGroup } from '@/hooks/useChangelogEntries'
import { ChangelogEmptyState } from './ChangelogEmptyState'
import { ChangelogEntry } from './ChangelogEntry'

interface ChangelogTimelineProps {
  groups: DayGroup[]
}

export function ChangelogTimeline({ groups }: ChangelogTimelineProps) {
  const resetFilters = useDrawerStore(s => s.resetFilters)

  return (
    <ScrollArea className='flex-1'>
      <div className='py-3'>
        {groups.length === 0 ? (
          <ChangelogEmptyState onReset={resetFilters} />
        ) : (
          <div
            role='feed'
            aria-label='Changelog timeline'
            className='relative pl-14 pr-5 before:absolute before:left-[28px] before:top-0 before:bottom-0 before:w-px before:bg-border'
          >
            {groups.map((group) => (
              <div key={group.day}>
                <div className='text-[10.5px] font-semibold uppercase tracking-[0.06em] text-muted-foreground flex items-center gap-2 mb-2 mt-3 first:mt-0'>
                  {group.label}
                  <span className='flex-1 h-px bg-border' />
                </div>
                {group.items.map((entry) => (
                  <ChangelogEntry
                    key={entry.id}
                    entry={entry}
                    onMarkBreaking={() => {}}
                    onMarkNonBreaking={() => {}}
                    onNotifyConsumer={() => {}}
                    onSnapshotToContract={() => {}}
                  />
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
    </ScrollArea>
  )
}
