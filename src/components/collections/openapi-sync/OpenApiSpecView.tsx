import { ArrowLeft, ChevronDown, ChevronRight } from 'lucide-react';
import { lazy, type ReactNode, Suspense, useCallback, useMemo, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable';
import { StateBlock } from '@/components/ui/state-block';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  useOpenApiSyncOverview,
  useOpenApiSyncSpecSource,
  useOpenApiSyncSpecView,
} from '@/lib/queries/openapi-sync-queries';
import type { OpenApiBodyView, OpenApiSpecGroup, OpenApiSpecOperation } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { SchemaTree } from './SchemaTree';
import type { RevealRequest } from './SpecSourcePane';
import { MethodPill, methodBarClass, SyncBadge, type SyncTone } from './SyncBadge';
import { findOperationLine, splitLines } from './specSourceLines';
import { syncErrorMessage } from './syncErrors';

// Monaco loads only when the spec view opens.
const SpecSourcePane = lazy(() =>
  import('./SpecSourcePane').then((m) => ({ default: m.SpecSourcePane })),
);

/** Large specs list this many operations in total, then offer more. */
const PAGE_SIZE = 500;

function matches(op: OpenApiSpecOperation, needle: string): boolean {
  if (!needle) return true;
  return [op.method, op.path, op.key, op.summary ?? ''].some((t) =>
    t.toLowerCase().includes(needle),
  );
}

/** One media type of a body. The schema opens on demand when `collapsed`. */
function BodyBlock({ body, collapsed }: { body: OpenApiBodyView; collapsed: boolean }) {
  const [open, setOpen] = useState(!collapsed);
  return (
    <div className='flex flex-col gap-1'>
      <div className='flex items-center gap-2'>
        <span className='font-mono font-medium'>{body.contentType}</span>
        {body.required && <SyncBadge tone='warning'>required</SyncBadge>}
        {collapsed && body.schema && (
          <Button
            variant='link'
            size='sm'
            className='h-auto p-0 text-xs'
            aria-expanded={open}
            onClick={() => setOpen((v) => !v)}
          >
            {open ? 'Hide schema' : 'Show schema'}
          </Button>
        )}
      </div>
      {open && body.schema && (
        <div className='rounded-md border border-border/70 bg-background/60 p-3'>
          <SchemaTree schema={body.schema} />
        </div>
      )}
    </div>
  );
}

/** A titled block of the operation detail. */
function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className='flex flex-col gap-2'>
      <h4 className='border-b border-border/70 pb-1 text-sm font-semibold'>{title}</h4>
      {children}
    </section>
  );
}

const LOCATION_TONE: Record<string, SyncTone> = {
  path: 'info',
  query: 'neutral',
  header: 'neutral',
  cookie: 'neutral',
};

function ParametersTable({ parameters }: { parameters: OpenApiSpecOperation['parameters'] }) {
  return (
    <Table aria-label='Parameters'>
      <TableHeader>
        <TableRow className='hover:bg-transparent'>
          <TableHead className='h-8 text-xs'>Name</TableHead>
          <TableHead className='h-8 text-xs'>In</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {parameters.map((p) => (
          <TableRow key={`${p.location}:${p.name}`}>
            <TableCell className='py-1.5 text-xs'>
              <span className='font-mono font-semibold'>{p.name}</span>
              {p.required && (
                <span className='ml-1 text-destructive' title='required'>
                  *
                </span>
              )}
              {p.required && <span className='sr-only'>required</span>}
            </TableCell>
            <TableCell className='py-1.5 text-xs'>
              <SyncBadge tone={LOCATION_TONE[p.location] ?? 'neutral'}>{p.location}</SyncBadge>
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function ResponsesTable({ responses }: { responses: OpenApiSpecOperation['responses'] }) {
  return (
    <Table aria-label='Responses'>
      <TableHeader>
        <TableRow className='hover:bg-transparent'>
          <TableHead className='h-8 w-16 text-xs'>Code</TableHead>
          <TableHead className='h-8 text-xs'>Description</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {responses.map((r) => (
          <TableRow key={r.status} className='align-top hover:bg-transparent'>
            <TableCell className='py-1.5 align-top'>
              <SyncBadge tone={statusTone(r.status)}>{r.status}</SyncBadge>
            </TableCell>
            <TableCell className='flex flex-col gap-1.5 py-1.5 text-xs'>
              <span>{r.description}</span>
              {r.bodies.map((b) => (
                <BodyBlock key={b.contentType} body={b} collapsed />
              ))}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function statusTone(status: string): SyncTone {
  if (status.startsWith('2')) return 'success';
  if (status.startsWith('3')) return 'info';
  if (status.startsWith('4')) return 'warning';
  if (status.startsWith('5')) return 'danger';
  return 'neutral';
}

function OperationDetail({ op }: { op: OpenApiSpecOperation }) {
  return (
    <div className='mt-1 flex flex-col gap-4 rounded-md border border-border/70 bg-card px-4 py-3 text-xs'>
      <Section title='Parameters'>
        {op.parameters.length === 0 ? (
          <p className='text-muted-foreground'>None.</p>
        ) : (
          <ParametersTable parameters={op.parameters} />
        )}
      </Section>
      {op.requestBody.length > 0 && (
        <Section title='Request body'>
          {op.requestBody.map((b) => (
            <BodyBlock key={b.contentType} body={b} collapsed={false} />
          ))}
        </Section>
      )}
      <Section title='Responses'>
        {op.responses.length === 0 ? (
          <p className='text-muted-foreground'>None documented.</p>
        ) : (
          <ResponsesTable responses={op.responses} />
        )}
      </Section>
      <Section title='Security'>
        {op.security.length === 0 ? (
          <p className='text-muted-foreground'>None.</p>
        ) : (
          <p className='font-mono'>{op.security.join(', ')}</p>
        )}
      </Section>
    </div>
  );
}

function OperationRow({
  op,
  onOpen,
}: {
  op: OpenApiSpecOperation;
  onOpen: (op: OpenApiSpecOperation) => void;
}) {
  const [open, setOpen] = useState(false);
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <li className='flex flex-col'>
      <Button
        variant='ghost'
        className={cn(
          'h-auto w-full justify-start gap-3 rounded-md border px-2 py-1.5 text-left font-normal',
          methodBarClass(op.method),
        )}
        aria-expanded={open}
        aria-label={`${[op.method, op.path, op.summary].filter(Boolean).join(' ')}${op.isNew ? ', new' : ''}`}
        onClick={() => {
          if (!open) onOpen(op);
          setOpen((v) => !v);
        }}
      >
        <MethodPill method={op.method} />
        <span className='min-w-0 break-all font-mono text-xs font-medium'>{op.path}</span>
        {op.summary && (
          <span className='min-w-0 flex-1 truncate text-xs text-muted-foreground'>
            {op.summary}
          </span>
        )}
        {!op.summary && <span className='flex-1' />}
        {op.isNew && <SyncBadge tone='warning'>New</SyncBadge>}
        <Chevron aria-hidden='true' className='h-4 w-4 shrink-0 text-muted-foreground' />
      </Button>
      {open && <OperationDetail op={op} />}
    </li>
  );
}

function TagGroup({
  group,
  total,
  shown,
  onOpen,
}: {
  group: OpenApiSpecGroup;
  total: number;
  shown: OpenApiSpecOperation[];
  onOpen: (op: OpenApiSpecOperation) => void;
}) {
  return (
    <section className='flex flex-col gap-1'>
      <h3 className='border-b border-border/70 pb-1 text-sm font-medium'>{`${group.tag} (${total})`}</h3>
      <ul className='flex flex-col gap-1.5'>
        {shown.map((op) => (
          <OperationRow key={op.key} op={op} onOpen={onOpen} />
        ))}
      </ul>
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
  const source = useOpenApiSyncSpecSource(collection);
  const [query, setQuery] = useState('');
  const [shown, setShown] = useState(PAGE_SIZE);
  const [reveal, setReveal] = useState<RevealRequest | null>(null);
  const needle = query.trim().toLowerCase();
  const sourceText = source.data?.text;
  const sourceFormat = source.data?.format;
  const hasEditor = typeof sourceText === 'string' && !source.data?.tooLarge;
  // The text splits once, so opening a row only scans.
  const sourceLines = useMemo(
    () => (hasEditor && typeof sourceText === 'string' ? splitLines(sourceText) : null),
    [hasEditor, sourceText],
  );
  const openOperation = useCallback(
    (o: OpenApiSpecOperation) => {
      if (!sourceLines || !sourceFormat) return;
      const line = findOperationLine(sourceLines, sourceFormat, o.path, o.method);
      if (line !== null) setReveal((prev) => ({ line, nonce: (prev?.nonce ?? 0) + 1 }));
    },
    [sourceLines, sourceFormat],
  );
  const groups = useMemo(
    () =>
      (spec.data?.groups ?? [])
        .map((group) => ({ group, ops: group.operations.filter((o) => matches(o, needle)) }))
        .filter((g) => g.ops.length > 0),
    [spec.data, needle],
  );
  // One budget covers every tag, so a spec with many small tags stays light too.
  const total = groups.reduce((n, g) => n + g.ops.length, 0);
  const remaining = Math.max(0, total - shown);
  const visible = useMemo(() => {
    let budget = shown;
    return groups
      .map(({ group, ops }) => {
        const take = ops.slice(0, budget);
        budget -= take.length;
        return { group, total: ops.length, shown: take };
      })
      .filter((g) => g.shown.length > 0);
  }, [groups, shown]);

  const back = (
    <Button variant='ghost' size='sm' onClick={onBack}>
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
    waiting === 1 ? '1 update waiting' : waiting > 1 ? `${waiting} updates waiting` : null;
  const operations = (
    <div className='flex h-full min-h-0 flex-col gap-3 overflow-y-auto p-4'>
      <Input
        aria-label='Search operations'
        placeholder='Search by method, path or summary'
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setShown(PAGE_SIZE);
        }}
      />
      {groups.length === 0 ? (
        <StateBlock tone='empty'>No operations match.</StateBlock>
      ) : (
        visible.map((g) => (
          <TagGroup
            key={g.group.tag}
            group={g.group}
            total={g.total}
            shown={g.shown}
            onOpen={openOperation}
          />
        ))
      )}
      {remaining > 0 && (
        <div>
          <Button variant='outline' size='sm' onClick={() => setShown((n) => n + PAGE_SIZE)}>
            {`Show more (${remaining})`}
          </Button>
        </div>
      )}
    </div>
  );
  const sourceMessage = source.isError ? (
    <StateBlock tone='error'>{syncErrorMessage(source.error)}</StateBlock>
  ) : source.isPending ? (
    <StateBlock tone='loading'>Loading the spec source...</StateBlock>
  ) : source.data?.tooLarge ? (
    <StateBlock tone='empty'>
      The spec is too large to show here. The operations on the right still work.
    </StateBlock>
  ) : null;
  const showLeft = hasEditor || sourceMessage !== null;
  const serverLine =
    data.servers.length === 0
      ? null
      : data.servers.length === 1
        ? `Server: ${data.servers[0]}`
        : `Server: ${data.servers[0]} +${data.servers.length - 1} more`;
  return (
    <div className='flex min-h-0 flex-1 flex-col overflow-hidden'>
      <div className='flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border/70 px-4 py-2'>
        {back}
        <h2 className='text-sm font-semibold'>{data.title}</h2>
        {data.version && <SyncBadge tone='neutral'>{data.version}</SyncBadge>}
        {data.state === 'updates_waiting' ? (
          <SyncBadge tone='warning'>{waitingText ?? 'Updates waiting'}</SyncBadge>
        ) : (
          <SyncBadge tone='success'>In sync</SyncBadge>
        )}
        {serverLine && (
          <span
            className='min-w-0 truncate font-mono text-xs text-muted-foreground'
            title={data.servers.length > 1 ? data.servers.join('\n') : undefined}
          >
            {serverLine}
          </span>
        )}
      </div>
      <ResizablePanelGroup orientation='horizontal' className='min-h-0 flex-1'>
        {showLeft && (
          <ResizablePanel id='spec-source' defaultSize={45} minSize='25%'>
            <div className='h-full overflow-hidden'>
              {hasEditor && source.data ? (
                <Suspense
                  fallback={
                    <div className='p-3'>
                      <StateBlock tone='loading'>Loading the editor...</StateBlock>
                    </div>
                  }
                >
                  <SpecSourcePane source={{ ...source.data, text: sourceText }} reveal={reveal} />
                </Suspense>
              ) : (
                <div className='p-3'>{sourceMessage}</div>
              )}
            </div>
          </ResizablePanel>
        )}
        {showLeft && <ResizableHandle withHandle />}
        <ResizablePanel id='spec-operations' defaultSize={55} minSize='30%'>
          {operations}
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
