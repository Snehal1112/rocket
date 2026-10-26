import { memo, useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { OpenApiSyncPlan } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { groupItems, itemId, renameHint, variableId, variableLabel } from './specSyncView';

/** A group shows at most this many rows. The detail pane still reaches every entry. */
export const MAX_ROWS_PER_GROUP = 500;

interface SpecSyncListProps {
  plan: OpenApiSyncPlan;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

interface RowProps {
  id: string;
  label: string;
  hint: string | null;
  selected: boolean;
  onSelect: (id: string) => void;
}

const Row = memo(function Row({ id, label, hint, selected, onSelect }: RowProps) {
  return (
    <Button
      variant='ghost'
      aria-pressed={selected}
      onClick={() => onSelect(id)}
      className={cn(
        'h-auto w-full flex-col items-start gap-0 whitespace-normal px-2 py-1.5 text-left',
        selected && 'bg-accent',
      )}
    >
      <span className='break-all text-sm'>{label}</span>
      {hint && <span className='text-xs font-normal text-muted-foreground'>{hint}</span>}
    </Button>
  );
});

interface RowData {
  id: string;
  label: string;
  hint: string | null;
}

/** The grouped list of pending changes. Each group collapses. */
export function SpecSyncList({ plan, selectedId, onSelect }: SpecSyncListProps) {
  // The rows depend on the plan only, so a selection change does not rebuild them.
  const groups = useMemo(
    () =>
      groupItems(plan.items).map((g) => ({
        kind: g.kind,
        label: g.label,
        total: g.items.length,
        rows: g.items.slice(0, MAX_ROWS_PER_GROUP).map(
          (item): RowData => ({
            id: itemId(item),
            label: item.name,
            hint: renameHint(item),
          }),
        ),
      })),
    [plan.items],
  );
  const variableRows = useMemo(
    () =>
      plan.variableChanges.map(
        (c): RowData => ({ id: variableId(c), label: variableLabel(c), hint: null }),
      ),
    [plan.variableChanges],
  );

  return (
    <ScrollArea className='h-full'>
      <div className='flex flex-col gap-2 p-3'>
        {groups.map((group) => (
          <CollapsibleSection
            key={group.kind}
            title={`${group.label} (${group.total})`}
            defaultOpen
          >
            {group.total === 0 ? (
              <p className='text-xs text-muted-foreground'>Nothing here.</p>
            ) : (
              <div className='flex flex-col gap-0.5'>
                {group.rows.map((r) => (
                  <Row
                    key={r.id}
                    id={r.id}
                    label={r.label}
                    hint={r.hint}
                    selected={selectedId === r.id}
                    onSelect={onSelect}
                  />
                ))}
                {group.total > group.rows.length && (
                  <p className='px-2 py-1 text-xs text-muted-foreground'>
                    {group.total - group.rows.length} more not shown
                  </p>
                )}
              </div>
            )}
          </CollapsibleSection>
        ))}
        {variableRows.length > 0 && (
          <CollapsibleSection title={`Variables (${variableRows.length})`} defaultOpen>
            <div className='flex flex-col gap-0.5'>
              {variableRows.map((r) => (
                <Row
                  key={r.id}
                  id={r.id}
                  label={r.label}
                  hint={r.hint}
                  selected={selectedId === r.id}
                  onSelect={onSelect}
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
              {plan.unchanged.slice(0, MAX_ROWS_PER_GROUP).map((u) => (
                <li key={u.key} className='break-all'>
                  {u.name}
                </li>
              ))}
            </ul>
            {plan.unchanged.length > MAX_ROWS_PER_GROUP && (
              <p className='text-xs text-muted-foreground'>
                {plan.unchanged.length - MAX_ROWS_PER_GROUP} more not shown
              </p>
            )}
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
