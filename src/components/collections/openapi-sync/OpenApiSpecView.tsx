import { ArrowLeft, ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { StateBlock } from '@/components/ui/state-block';
import { useOpenApiSyncOverview, useOpenApiSyncSpecView } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiSpecGroup, OpenApiSpecOperation } from '@/lib/tauri-api';
import { syncErrorMessage } from './syncErrors';

/** Large specs list this many operations per tag, then offer more. */
const PAGE_SIZE = 500;

function matches(op: OpenApiSpecOperation, needle: string): boolean {
  if (!needle) return true;
  return [op.method, op.path, op.key, op.summary ?? ''].some((t) =>
    t.toLowerCase().includes(needle),
  );
}

function OperationDetail({ op }: { op: OpenApiSpecOperation }) {
  return (
    <div className='flex flex-col gap-2 border-t border-border/60 bg-muted/30 px-3 py-2 text-xs'>
      <div className='flex flex-col gap-1'>
        <h4 className='font-medium'>Parameters</h4>
        {op.parameters.length === 0 ? (
          <p className='text-muted-foreground'>None.</p>
        ) : (
          <ul aria-label='Parameters' className='flex flex-col gap-0.5'>
            {op.parameters.map((p) => (
              <li key={`${p.location}:${p.name}`} className='flex items-center gap-2 font-mono'>
                <span>{`${p.name} (${p.location})`}</span>
                {p.required && <Badge variant='secondary'>required</Badge>}
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className='flex flex-col gap-1'>
        <h4 className='font-medium'>Responses</h4>
        {op.responses.length === 0 ? (
          <p className='text-muted-foreground'>None documented.</p>
        ) : (
          <ul aria-label='Responses' className='flex flex-col gap-0.5'>
            {op.responses.map((r) => (
              <li key={r.status} className='flex items-center gap-2'>
                <span className='font-mono font-semibold'>{r.status}</span>
                <span>{r.description}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function OperationRow({ op }: { op: OpenApiSpecOperation }) {
  const [open, setOpen] = useState(false);
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <li className='border-b border-border/60 last:border-b-0'>
      <Button
        variant='ghost'
        className='h-auto w-full justify-start gap-2 rounded-none px-3 py-2 text-left font-normal'
        aria-expanded={open}
        aria-label={[op.method, op.path, op.summary].filter(Boolean).join(' ')}
        onClick={() => setOpen((v) => !v)}
      >
        <Chevron aria-hidden='true' className='h-3 w-3 shrink-0' />
        <span className='w-14 shrink-0 font-mono text-xs font-semibold'>{op.method}</span>
        <span className='min-w-0 break-all font-mono text-xs'>{op.path}</span>
        {op.summary && (
          <span className='min-w-0 flex-1 truncate text-xs text-muted-foreground'>
            {op.summary}
          </span>
        )}
        {op.isNew && <Badge variant='warning'>New</Badge>}
      </Button>
      {open && <OperationDetail op={op} />}
    </li>
  );
}

function TagGroup({ group, ops }: { group: OpenApiSpecGroup; ops: OpenApiSpecOperation[] }) {
  const [shown, setShown] = useState(PAGE_SIZE);
  const visible = ops.slice(0, shown);
  const remaining = ops.length - visible.length;
  return (
    <section className='flex flex-col gap-1'>
      <h3 className='text-sm font-medium'>{`${group.tag} (${ops.length})`}</h3>
      <ul className='rounded-md border border-border/70'>
        {visible.map((op) => (
          <OperationRow key={op.key} op={op} />
        ))}
      </ul>
      {remaining > 0 && (
        <div>
          <Button variant='outline' size='sm' onClick={() => setShown((n) => n + PAGE_SIZE)}>
            {`Show more (${remaining})`}
          </Button>
        </div>
      )}
    </section>
  );
}

/** A read-only page of the latest stored spec, grouped by tag. It never contacts the network. */
export function OpenApiSpecView({
  collection,
  onBack,
}: {
  collection: string;
  onBack: () => void;
}) {
  const spec = useOpenApiSyncSpecView(collection);
  const overview = useOpenApiSyncOverview(collection);
  const [query, setQuery] = useState('');
  const needle = query.trim().toLowerCase();
  const groups = useMemo(
    () =>
      (spec.data?.groups ?? [])
        .map((group) => ({ group, ops: group.operations.filter((o) => matches(o, needle)) }))
        .filter((g) => g.ops.length > 0),
    [spec.data, needle],
  );

  const back = (
    <Button variant='ghost' size='sm' className='self-start' onClick={onBack}>
      <ArrowLeft aria-hidden='true' className='h-4 w-4' />
      Back
    </Button>
  );
  if (spec.isError) {
    return (
      <div className='flex flex-col gap-3 p-4'>
        {back}
        <StateBlock tone='error'>{syncErrorMessage(spec.error)}</StateBlock>
      </div>
    );
  }
  const data = spec.data;
  if (!data) {
    return (
      <div className='flex flex-col gap-3 p-4'>
        {back}
        <StateBlock tone='loading'>Loading the spec...</StateBlock>
      </div>
    );
  }
  if (data.state === 'no_copy') {
    return (
      <div className='flex flex-col gap-3 p-4'>
        {back}
        <StateBlock tone='empty'>No saved copy yet. The next check stores one.</StateBlock>
      </div>
    );
  }
  const waiting = overview.data?.updatesWaiting ?? 0;
  const waitingText =
    waiting === 1 ? '1 update is waiting.' : waiting > 1 ? `${waiting} updates are waiting.` : null;
  return (
    <div className='flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4'>
      {back}
      <div className='flex flex-col gap-1'>
        <div className='flex items-center gap-2'>
          <h2 className='text-sm font-semibold'>{data.title}</h2>
          {data.version && <Badge variant='outline'>{data.version}</Badge>}
        </div>
        <p className='text-xs text-muted-foreground'>Showing the latest spec Rocket fetched.</p>
        <p className='text-xs text-muted-foreground'>
          {data.state === 'updates_waiting'
            ? (waitingText ?? 'Updates are waiting.')
            : 'Your collection is in sync with it.'}
        </p>
      </div>
      <Input
        aria-label='Search operations'
        placeholder='Search by method, path or summary'
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {groups.length === 0 ? (
        <StateBlock tone='empty'>No operations match.</StateBlock>
      ) : (
        groups.map(({ group, ops }) => <TagGroup key={group.tag} group={group} ops={ops} />)
      )}
    </div>
  );
}
