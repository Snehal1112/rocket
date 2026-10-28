import { ChevronDown, ChevronRight } from 'lucide-react';
import { memo, type ReactNode, useMemo, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { CollapsibleSection } from '@/components/ui/collapsible-section';
import { SegmentedControl } from '@/components/ui/segmented-control';
import type { OpenApiSyncItem, OpenApiSyncPlan } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { OpenApiSkippedList } from './OpenApiSkippedList';
import { OpenApiSyncDetail, type OpenApiSyncDetailProps } from './OpenApiSyncDetail';
import {
  displayPath,
  filterItems,
  filterVariables,
  groupItems,
  itemId,
  type SyncGroupKind,
  variableId,
  variableLabel,
} from './openApiSyncView';
import { MethodPill, SyncBadge, type SyncTone } from './SyncBadge';

/** A group shows at most this many rows. Rows past the cap are not shown yet. */
export const MAX_ROWS_PER_GROUP = 500;

type Choice = 'keep' | 'accept';
type SectionKind = SyncGroupKind | 'variables';

interface SectionCopy {
  title: string;
  summary: string;
  tone: SyncTone;
  keep: string;
  accept: string;
}

const SECTION: Record<SectionKind, SectionCopy> = {
  changed: {
    title: 'Updated in spec',
    summary: 'The spec changed these requests.',
    tone: 'info',
    keep: 'Keep current',
    accept: 'Update',
  },
  new: {
    title: 'New in spec',
    summary: 'The spec added these requests.',
    tone: 'success',
    keep: "Don't add",
    accept: 'Add',
  },
  removed: {
    title: 'Removed from spec',
    summary: 'Your collection has these, the spec no longer does.',
    tone: 'danger',
    keep: 'Keep',
    accept: 'Delete',
  },
  variables: {
    title: 'Variables',
    summary: 'Collection variables the spec changes or adds.',
    tone: 'neutral',
    keep: 'Keep current',
    accept: 'Update',
  },
};

/** Updated first, as in the review summary, then new, then removed. */
const SECTION_ORDER: SyncGroupKind[] = ['changed', 'new', 'removed'];

interface RowData {
  id: string;
  toggleKey: string;
  isVariable: boolean;
  kind: SectionKind;
  method: string | null;
  path: string;
  name: string | null;
  conflict: boolean;
}

function rowFromItem(item: OpenApiSyncItem): RowData {
  const base = {
    id: itemId(item),
    toggleKey: item.key,
    isVariable: false,
    kind: item.kind,
    conflict: false,
  };
  if (item.kind === 'new') {
    return {
      ...base,
      method: item.preview.method,
      path: displayPath(item.preview.url),
      name: item.name,
    };
  }
  if (item.kind === 'changed') {
    return {
      ...base,
      method: item.method || null,
      path: displayPath(item.url) || item.key,
      name: item.name,
      conflict: item.changes.some((c) => c.outcome === 'conflict'),
    };
  }
  return { ...base, method: null, path: item.requestPath || item.key, name: item.name };
}

interface RowProps {
  row: RowData;
  checked: boolean;
  skipped: boolean;
  open: boolean;
  onOpenChange: (id: string, open: boolean) => void;
  onToggle: (key: string) => void;
  onToggleVariable: (name: string) => void;
  detail: Omit<OpenApiSyncDetailProps, 'selectedId' | 'inline'>;
}

const Row = memo(function Row({
  row,
  checked,
  skipped,
  open,
  onOpenChange,
  onToggle,
  onToggleVariable,
  detail,
}: RowProps) {
  const copy = SECTION[row.kind];
  const label = row.name ?? row.path;
  const choice: Choice = checked ? 'accept' : 'keep';
  const toggle = () => (row.isVariable ? onToggleVariable(row.toggleKey) : onToggle(row.toggleKey));
  return (
    <li className='border-b border-border last:border-b-0'>
      <div
        className={cn(
          'flex items-center gap-2 px-2 py-1.5 hover:bg-muted/30',
          open && 'bg-muted/30',
        )}
      >
        <Button
          variant='ghost'
          aria-expanded={open}
          aria-label={`${open ? 'Hide' : 'Show'} details of ${label}`}
          onClick={() => onOpenChange(row.id, !open)}
          className='h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-1 text-left font-normal hover:bg-transparent'
        >
          {open ? (
            <ChevronDown aria-hidden='true' className='size-4 shrink-0 text-muted-foreground' />
          ) : (
            <ChevronRight aria-hidden='true' className='size-4 shrink-0 text-muted-foreground' />
          )}
          {row.method ? (
            <MethodPill method={row.method} />
          ) : (
            <SyncBadge tone={copy.tone} className='w-16 shrink-0 justify-center'>
              {row.kind === 'removed' ? 'Removed' : 'Variable'}
            </SyncBadge>
          )}
          <span title={row.path} className='min-w-0 truncate font-mono text-xs font-medium'>
            {row.path}
          </span>
          {row.name && row.name !== row.path && (
            <span className='min-w-0 shrink-[2] truncate text-xs text-muted-foreground'>
              {row.name}
            </span>
          )}
          {row.conflict && (
            <SyncBadge tone='warning' className='shrink-0'>
              Conflict
            </SyncBadge>
          )}
          {skipped && (
            <SyncBadge tone='neutral' className='shrink-0'>
              Skipped
            </SyncBadge>
          )}
        </Button>
        <SegmentedControl<Choice>
          aria-label={`Choice for ${label}`}
          value={choice}
          onValueChange={(next) => next !== choice && toggle()}
          options={[
            { value: 'keep', label: copy.keep },
            { value: 'accept', label: copy.accept },
          ]}
        />
      </div>
      {open && (
        <div className='border-t border-border/60 bg-muted/10 px-4 py-3 md:pl-12'>
          <OpenApiSyncDetail {...detail} selectedId={row.id} inline />
        </div>
      )}
    </li>
  );
});

function Section({
  kind,
  count,
  shown,
  children,
  footer,
}: {
  kind: SectionKind;
  count: number;
  /** The rows on screen. A search can make this fewer than `count`. */
  shown: number;
  children: ReactNode;
  footer?: ReactNode;
}) {
  const copy = SECTION[kind];
  const [open, setOpen] = useState(true);
  return (
    <Collapsible open={open} onOpenChange={setOpen} asChild>
      <section className='flex flex-col'>
        <div className='sticky top-0 z-10 mb-1 rounded-lg border border-border/70 bg-card shadow-sm'>
          <CollapsibleTrigger asChild>
            <Button
              variant='ghost'
              className='group h-auto min-h-11 w-full justify-start gap-3 px-3 py-2 text-left'
            >
              <ChevronDown
                aria-hidden='true'
                className='size-4 shrink-0 text-muted-foreground transition-transform motion-reduce:transition-none group-data-[state=closed]:-rotate-90'
              />
              <h3 className='shrink-0 text-sm font-semibold'>{copy.title}</h3>
              <SyncBadge tone={copy.tone}>
                {shown === count ? count : `${shown} of ${count}`}
              </SyncBadge>
              <span className='min-w-0 truncate text-xs font-normal text-muted-foreground'>
                {copy.summary}
              </span>
            </Button>
          </CollapsibleTrigger>
        </div>
        <CollapsibleContent>
          <ul>{children}</ul>
          {footer}
        </CollapsibleContent>
      </section>
    </Collapsible>
  );
}

export interface OpenApiSyncListProps {
  collection: string;
  plan: OpenApiSyncPlan;
  /** The search text. A non-empty query lifts the row cap for the matches. */
  query: string;
  approved: Set<string>;
  skips: Set<string>;
  variables: Set<string>;
  onToggle: (key: string) => void;
  onToggleVariable: (name: string) => void;
  /** What an open row needs to show its detail and take field choices. */
  detail: Omit<OpenApiSyncDetailProps, 'selectedId' | 'inline'>;
}

/** The pending changes as stacked sections. A row opens in place to show its detail. */
export function OpenApiSyncList({
  collection,
  plan,
  query,
  approved,
  skips,
  variables,
  onToggle,
  onToggleVariable,
  detail,
}: OpenApiSyncListProps) {
  const [showSkipped, setShowSkipped] = useState(false);
  const [openIds, setOpenIds] = useState<Set<string>>(() => new Set());
  // Extra pages of rows per group, added by Show more. A query shows every match instead.
  const [pages, setPages] = useState<Record<string, number>>({});
  const searching = query.trim().length > 0;

  const setOpen = (id: string, open: boolean) =>
    setOpenIds((prev) => {
      const next = new Set(prev);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });

  // The rows depend on the plan, the query and the pages only, so a toggle does not rebuild them.
  const sections = useMemo(() => {
    const byKind = new Map(groupItems(filterItems(plan.items, query)).map((g) => [g.kind, g]));
    return SECTION_ORDER.map((kind) => {
      const items = byKind.get(kind)?.items ?? [];
      const cap = searching ? items.length : MAX_ROWS_PER_GROUP * (1 + (pages[kind] ?? 0));
      return { kind, total: items.length, rows: items.slice(0, cap).map(rowFromItem) };
    }).filter((s) => s.total > 0);
  }, [plan.items, query, searching, pages]);
  const variableRows = useMemo(
    () =>
      filterVariables(plan.variableChanges, query).map(
        (c): RowData => ({
          id: variableId(c),
          toggleKey: c.name,
          isVariable: true,
          kind: 'variables',
          method: null,
          path: variableLabel(c),
          name: null,
          conflict: false,
        }),
      ),
    [plan.variableChanges, query],
  );
  const unchanged = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? plan.unchanged.filter((u) => u.name.toLowerCase().includes(q)) : plan.unchanged;
  }, [plan.unchanged, query]);

  const renderRow = (r: RowData) => (
    <Row
      key={r.id}
      row={r}
      checked={r.isVariable ? variables.has(r.toggleKey) : approved.has(r.toggleKey)}
      skipped={!r.isVariable && skips.has(r.toggleKey)}
      open={openIds.has(r.id)}
      onOpenChange={setOpen}
      onToggle={onToggle}
      onToggleVariable={onToggleVariable}
      detail={detail}
    />
  );

  return (
    <div className='flex flex-col gap-4 p-4'>
      {sections.map((s) => (
        <Section
          key={s.kind}
          kind={s.kind}
          count={s.total}
          shown={s.rows.length}
          footer={
            s.total > s.rows.length && (
              <div className='flex items-center gap-2 px-3 py-2'>
                <p className='text-xs text-muted-foreground'>
                  {s.total - s.rows.length} more not shown
                </p>
                <Button
                  variant='outline'
                  size='sm'
                  className='h-6 px-2 text-xs'
                  onClick={() => setPages((p) => ({ ...p, [s.kind]: (p[s.kind] ?? 0) + 1 }))}
                >
                  Show more
                </Button>
              </div>
            )
          }
        >
          {s.rows.map(renderRow)}
        </Section>
      ))}
      {variableRows.length > 0 && (
        <Section kind='variables' count={variableRows.length} shown={variableRows.length}>
          {variableRows.map(renderRow)}
        </Section>
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
        <div className='flex flex-col gap-1'>
          <Button
            variant='link'
            size='sm'
            className='h-auto justify-start px-1 py-0 text-xs text-muted-foreground'
            aria-expanded={showSkipped}
            onClick={() => setShowSkipped((v) => !v)}
          >
            {`${plan.skippedHidden} skipped, hidden from this list.`}
          </Button>
          {showSkipped && <OpenApiSkippedList collection={collection} />}
        </div>
      )}
      {plan.deletedByUser.length > 0 && (
        <p className='px-1 text-xs text-muted-foreground'>
          You deleted {plan.deletedByUser.length} imported{' '}
          {plan.deletedByUser.length === 1 ? 'request' : 'requests'}:{' '}
          {plan.deletedByUser.join(', ')}
        </p>
      )}
    </div>
  );
}
