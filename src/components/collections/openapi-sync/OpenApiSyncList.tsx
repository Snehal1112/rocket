import { memo, useMemo, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { OpenApiSyncPlan } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import {
  filterItems,
  filterVariables,
  groupItems,
  itemId,
  renameHint,
  variableId,
  variableLabel,
} from './openApiSyncView';

/** A group shows at most this many rows. Rows past the cap are not shown yet. */
export const MAX_ROWS_PER_GROUP = 500;

interface OpenApiSyncListProps {
  plan: OpenApiSyncPlan;
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** The search text. A non-empty query lifts the row cap for the matches. */
  query: string;
  approved: Set<string>;
  skips: Set<string>;
  variables: Set<string>;
  onToggle: (key: string) => void;
  onToggleVariable: (name: string) => void;
}

interface RowProps {
  id: string;
  toggleKey: string;
  isVariable: boolean;
  label: string;
  hint: string | null;
  selected: boolean;
  checked: boolean;
  skipped: boolean;
  onSelect: (id: string) => void;
  onToggle: (key: string) => void;
  onToggleVariable: (name: string) => void;
}

const Row = memo(function Row({
  id,
  toggleKey,
  isVariable,
  label,
  hint,
  selected,
  checked,
  skipped,
  onSelect,
  onToggle,
  onToggleVariable,
}: RowProps) {
  return (
    <div className={cn('flex items-start gap-1 rounded-md pl-2', selected && 'bg-accent')}>
      <Checkbox
        aria-label={`Include ${label} in apply`}
        checked={checked}
        onCheckedChange={() => (isVariable ? onToggleVariable(toggleKey) : onToggle(toggleKey))}
        className='mt-2.5'
      />
      <Button
        variant='ghost'
        aria-pressed={selected}
        onClick={() => onSelect(id)}
        className='h-auto min-w-0 flex-1 flex-col items-start gap-0 whitespace-normal px-2 py-1.5 text-left'
      >
        <span className='break-all text-sm'>{label}</span>
        {hint && <span className='text-xs font-normal text-muted-foreground'>{hint}</span>}
        {skipped && <span className='text-xs font-normal text-muted-foreground'>Skipped</span>}
      </Button>
    </div>
  );
});

interface RowData {
  id: string;
  toggleKey: string;
  label: string;
  hint: string | null;
}

/** The grouped list of pending changes. Each group collapses. */
export function OpenApiSyncList({
  plan,
  selectedId,
  onSelect,
  query,
  approved,
  skips,
  variables,
  onToggle,
  onToggleVariable,
}: OpenApiSyncListProps) {
  // Extra pages of rows per group, added by Show more. A query shows every match instead.
  const [pages, setPages] = useState<Record<string, number>>({});
  const searching = query.trim().length > 0;

  // The rows depend on the plan, the query and the pages only, so a checkbox toggle does not
  // rebuild them.
  const groups = useMemo(
    () =>
      groupItems(filterItems(plan.items, query)).map((g) => {
        const cap = searching ? g.items.length : MAX_ROWS_PER_GROUP * (1 + (pages[g.kind] ?? 0));
        return {
          kind: g.kind,
          label: g.label,
          total: g.items.length,
          rows: g.items.slice(0, cap).map(
            (item): RowData => ({
              id: itemId(item),
              toggleKey: item.key,
              label: item.name,
              hint: renameHint(item),
            }),
          ),
        };
      }),
    [plan.items, query, searching, pages],
  );
  const variableRows = useMemo(
    () =>
      filterVariables(plan.variableChanges, query).map(
        (c): RowData => ({
          id: variableId(c),
          toggleKey: c.name,
          label: variableLabel(c),
          hint: null,
        }),
      ),
    [plan.variableChanges, query],
  );
  const unchanged = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? plan.unchanged.filter((u) => u.name.toLowerCase().includes(q)) : plan.unchanged;
  }, [plan.unchanged, query]);

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
                    toggleKey={r.toggleKey}
                    isVariable={false}
                    label={r.label}
                    hint={r.hint}
                    selected={selectedId === r.id}
                    checked={approved.has(r.toggleKey)}
                    skipped={skips.has(r.toggleKey)}
                    onSelect={onSelect}
                    onToggle={onToggle}
                    onToggleVariable={onToggleVariable}
                  />
                ))}
                {group.total > group.rows.length && (
                  <div className='flex items-center gap-2 px-2 py-1'>
                    <p className='text-xs text-muted-foreground'>
                      {group.total - group.rows.length} more not shown
                    </p>
                    <Button
                      variant='outline'
                      size='sm'
                      className='h-6 px-2 text-xs'
                      onClick={() =>
                        setPages((p) => ({ ...p, [group.kind]: (p[group.kind] ?? 0) + 1 }))
                      }
                    >
                      Show more
                    </Button>
                  </div>
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
                  toggleKey={r.toggleKey}
                  isVariable
                  label={r.label}
                  hint={r.hint}
                  selected={selectedId === r.id}
                  checked={variables.has(r.toggleKey)}
                  skipped={false}
                  onSelect={onSelect}
                  onToggle={onToggle}
                  onToggleVariable={onToggleVariable}
                />
              ))}
            </div>
          </CollapsibleSection>
        )}
        {unchanged.length > 0 && (
          <CollapsibleSection
            title={`Stays as you have it (${unchanged.length})`}
            summary='The spec did not change these.'
          >
            <ul className='flex flex-col gap-0.5 text-sm'>
              {unchanged.slice(0, MAX_ROWS_PER_GROUP).map((u) => (
                <li key={u.key} className='break-all'>
                  {u.name}
                </li>
              ))}
            </ul>
            {unchanged.length > MAX_ROWS_PER_GROUP && (
              <p className='text-xs text-muted-foreground'>
                {unchanged.length - MAX_ROWS_PER_GROUP} more not shown
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
