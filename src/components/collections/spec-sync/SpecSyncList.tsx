import { Button } from '@/components/ui/button';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { OpenApiSyncPlan } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { groupItems, itemId, renameHint, variableId, variableLabel } from './specSyncView';

interface SpecSyncListProps {
  plan: OpenApiSyncPlan;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

interface RowProps {
  label: string;
  hint?: string | null;
  selected: boolean;
  onClick: () => void;
}

function Row({ label, hint, selected, onClick }: RowProps) {
  return (
    <Button
      variant='ghost'
      aria-pressed={selected}
      onClick={onClick}
      className={cn(
        'h-auto w-full flex-col items-start gap-0 whitespace-normal px-2 py-1.5 text-left',
        selected && 'bg-accent',
      )}
    >
      <span className='break-all text-sm'>{label}</span>
      {hint && <span className='text-xs font-normal text-muted-foreground'>{hint}</span>}
    </Button>
  );
}

/** The grouped list of pending changes. Each group collapses. */
export function SpecSyncList({ plan, selectedId, onSelect }: SpecSyncListProps) {
  const groups = groupItems(plan.items);
  return (
    <ScrollArea className='h-full'>
      <div className='flex flex-col gap-2 p-3'>
        {groups.map((group) => (
          <CollapsibleSection
            key={group.kind}
            title={`${group.label} (${group.items.length})`}
            defaultOpen
          >
            {group.items.length === 0 ? (
              <p className='text-xs text-muted-foreground'>Nothing here.</p>
            ) : (
              <div className='flex flex-col gap-0.5'>
                {group.items.map((item) => (
                  <Row
                    key={itemId(item)}
                    label={item.name}
                    hint={renameHint(item)}
                    selected={selectedId === itemId(item)}
                    onClick={() => onSelect(itemId(item))}
                  />
                ))}
              </div>
            )}
          </CollapsibleSection>
        ))}
        {plan.variableChanges.length > 0 && (
          <CollapsibleSection title={`Variables (${plan.variableChanges.length})`} defaultOpen>
            <div className='flex flex-col gap-0.5'>
              {plan.variableChanges.map((change) => (
                <Row
                  key={variableId(change)}
                  label={variableLabel(change)}
                  selected={selectedId === variableId(change)}
                  onClick={() => onSelect(variableId(change))}
                />
              ))}
            </div>
          </CollapsibleSection>
        )}
        {plan.unchanged.length > 0 && (
          <CollapsibleSection
            title={`Stays as you have it (${plan.unchanged.length})`}
            summary='The spec did not change these.'
          >
            <ul className='flex flex-col gap-0.5 text-sm'>
              {plan.unchanged.map((u) => (
                <li key={u.key} className='break-all'>
                  {u.name}
                </li>
              ))}
            </ul>
          </CollapsibleSection>
        )}
        {plan.skippedHidden > 0 && (
          <p className='px-1 text-xs text-muted-foreground'>
            {plan.skippedHidden} skipped, hidden from this list.
          </p>
        )}
        {plan.deletedByUser.length > 0 && (
          <p className='px-1 text-xs text-muted-foreground'>
            You deleted {plan.deletedByUser.length} imported{' '}
            {plan.deletedByUser.length === 1 ? 'request' : 'requests'}:{' '}
            {plan.deletedByUser.join(', ')}
          </p>
        )}
      </div>
    </ScrollArea>
  );
}
