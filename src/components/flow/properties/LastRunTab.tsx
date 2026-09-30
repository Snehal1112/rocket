import { Check, ChevronRight, Copy } from 'lucide-react';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table';
import { formatOutputValue } from '@/lib/flow-output';
import { msToSecondsLabel } from '@/lib/flow-repeat';
import type {
  FlowDebugHeader,
  FlowDebugRequest,
  FlowLogEntry,
  FlowNode,
  FlowNodeStatus,
} from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { exitLabel } from '../flowExits';

interface LastRunTabProps {
  node: FlowNode;
  status: FlowNodeStatus;
  detail?: FlowNodeDetail;
}

// The badge text for a status. A branch that was not taken is a skip too,
// but people read it as its own outcome.
function statusLabel(status: FlowNodeStatus, detail?: FlowNodeDetail): string {
  switch (status) {
    case 'success':
      return 'Success';
    case 'failed':
      return 'Failed';
    case 'running':
      return 'Running';
    case 'skipped':
      return detail?.skipReason === 'branch_not_taken' ? 'Not taken' : 'Skipped';
    case 'idle':
      return 'Not run';
  }
}

const badgeClass: Record<FlowNodeStatus, string> = {
  idle: '',
  running: 'bg-blue-500/15 text-blue-600',
  success: 'bg-green-500/15 text-green-600',
  failed: 'bg-red-500/15 text-red-600',
  skipped: 'bg-muted text-muted-foreground',
};

function attemptsLabel(n: number): string {
  return n === 1 ? '1 attempt' : `${n} attempts`;
}

// Duration reads as milliseconds for a single send and seconds for a poll,
// matching the Request card.
function timingParts(detail?: FlowNodeDetail): string[] {
  if (!detail) return [];
  const parts: string[] = [];
  if (detail.statusCode !== undefined) parts.push(String(detail.statusCode));
  if (detail.attempts !== undefined) {
    parts.push(attemptsLabel(detail.attempts));
    if (detail.durationMs !== undefined) parts.push(msToSecondsLabel(detail.durationMs));
  } else if (detail.durationMs !== undefined) {
    parts.push(`${detail.durationMs}ms`);
  }
  return parts;
}

function skipText(detail?: FlowNodeDetail): string {
  return detail?.skipReason === 'branch_not_taken'
    ? 'Its branch was not taken.'
    : 'An earlier node failed.';
}

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

const COPIED_MS = 1500;

// Copies text and shows a check for a moment, like the Output card.
function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = () => {
    navigator.clipboard?.writeText(text).then(
      () => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_MS);
      },
      (err) => console.warn('Copy failed', err),
    );
  };
  return (
    <Button
      type='button'
      variant='ghost'
      size='icon'
      className='h-5 w-5'
      aria-label={label}
      title={label}
      onClick={copy}
    >
      {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
    </Button>
  );
}

function HeadersTable({ headers }: { headers: FlowDebugHeader[] }) {
  if (headers.length === 0) return <p className='text-muted-foreground'>No headers.</p>;
  return (
    <Table>
      <TableBody>
        {headers.map((h, i) => (
          // Headers can repeat, so the index keeps keys unique.
          // biome-ignore lint/suspicious/noArrayIndexKey: header order is stable within a record.
          <TableRow key={`${h.key}-${i}`}>
            <TableCell className='w-1/3 py-1 font-mono text-[11px]'>{h.key}</TableCell>
            <TableCell className='select-text py-1 font-mono text-[11px] [overflow-wrap:anywhere]'>
              {h.value}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function isJson(body: string): boolean {
  try {
    JSON.parse(body);
    return true;
  } catch {
    return false;
  }
}

function BodyViewer({ body }: { body: string }) {
  if (body === '') return <p className='text-muted-foreground'>No body.</p>;
  return (
    <div className='h-48 overflow-hidden rounded-md border'>
      <Suspense fallback={<div className='p-2 text-muted-foreground'>Loading…</div>}>
        <MonacoWrapper
          value={formatOutputValue(body)}
          readOnly
          height='100%'
          language={isJson(body) ? 'json' : 'plaintext'}
        />
      </Suspense>
    </div>
  );
}

function ExchangeSections({
  exchange,
  shownError,
}: {
  exchange: FlowDebugRequest;
  shownError?: string;
}) {
  const [sentOpen, setSentOpen] = useState(false);
  const response = exchange.response;
  return (
    <div className='space-y-3'>
      {exchange.error && exchange.error !== shownError && (
        <div
          data-testid='last-run-send-error'
          className='select-text whitespace-pre-wrap break-words rounded-md border border-red-500/40 bg-red-500/5 p-2 text-red-600'
        >
          {exchange.error}
        </div>
      )}
      {response && (
        <section data-testid='last-run-response' className='space-y-1.5'>
          <div className='flex items-center justify-between'>
            <h4 className='font-medium'>
              Response · {response.status} {response.statusText}
            </h4>
            <CopyButton text={response.body} label='Copy response body' />
          </div>
          <HeadersTable headers={response.headers} />
          <BodyViewer body={response.body} />
          {response.truncated && <p className='text-muted-foreground'>Truncated at 256 KB</p>}
        </section>
      )}
      <Collapsible open={sentOpen} onOpenChange={setSentOpen}>
        <CollapsibleTrigger asChild>
          <Button type='button' variant='ghost' size='sm' className='h-6 px-1 text-xs'>
            <ChevronRight
              className={
                sentOpen ? 'h-3 w-3 rotate-90 transition-transform' : 'h-3 w-3 transition-transform'
              }
            />
            Request as sent
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <section data-testid='last-run-request' className='mt-1.5 space-y-1.5'>
            <p className='select-text font-mono text-[11px] [overflow-wrap:anywhere]'>
              {exchange.method} {exchange.url}
            </p>
            <HeadersTable headers={exchange.headers} />
            {exchange.body !== undefined && exchange.body !== '' && (
              <pre className='max-h-48 select-text overflow-auto whitespace-pre-wrap rounded-md border p-2 font-mono text-[11px]'>
                {exchange.body}
              </pre>
            )}
          </section>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

const logClass: Record<FlowLogEntry['level'], string> = {
  log: '',
  warn: 'text-amber-600',
  error: 'text-red-600',
};

function ValueSection({ value }: { value: string }) {
  return (
    <section className='space-y-1'>
      <div className='flex items-center justify-between'>
        <h4 className='font-medium'>Value</h4>
        {value !== '' && <CopyButton text={value} label='Copy value' />}
      </div>
      <pre
        data-testid='last-run-value'
        className='max-h-80 select-text overflow-auto whitespace-pre-wrap rounded-md border p-2 font-mono text-[11px] [overflow-wrap:anywhere]'
      >
        {value === '' ? <span className='italic'>(empty)</span> : formatOutputValue(value)}
      </pre>
    </section>
  );
}

function LogsSection({ logs }: { logs: FlowLogEntry[] }) {
  return (
    <section data-testid='last-run-logs' className='space-y-1'>
      <h4 className='font-medium'>Logs</h4>
      <div className='max-h-48 space-y-0.5 overflow-auto rounded-md border p-2 font-mono text-[11px]'>
        {logs.map((entry, i) => (
          // Log lines have no id and can repeat.
          // biome-ignore lint/suspicious/noArrayIndexKey: log order is stable within a run.
          <p key={i} className={`select-text whitespace-pre-wrap ${logClass[entry.level]}`}>
            {entry.message}
          </p>
        ))}
      </div>
    </section>
  );
}

export function LastRunTab({ node, status, detail }: LastRunTabProps) {
  if (status === 'idle') {
    return (
      <p className='text-xs text-muted-foreground'>
        Not run yet. Run the flow to see results here.
      </p>
    );
  }

  const parts = status === 'running' ? [] : timingParts(detail);

  return (
    <div data-node-id={node.id} className='space-y-3 text-xs'>
      <div data-testid='last-run-status' className='flex flex-wrap items-center gap-1.5'>
        <Badge variant='secondary' className={badgeClass[status]}>
          {statusLabel(status, detail)}
        </Badge>
        {status === 'running' && detail?.progress && (
          <span className='text-muted-foreground'>{detail.progress}</span>
        )}
        {parts.length > 0 && <span className='text-muted-foreground'>{parts.join(' · ')}</span>}
      </div>
      {status === 'failed' && detail?.error && (
        <div
          data-testid='last-run-error'
          className='select-text whitespace-pre-wrap break-words rounded-md border border-red-500/40 bg-red-500/5 p-2 text-red-600'
        >
          {detail.error}
        </div>
      )}
      {status === 'skipped' && <p className='text-muted-foreground'>{skipText(detail)}</p>}
      {(node.kind.kind === 'Request' || node.kind.kind === 'WaitForCallback') &&
        detail?.exchange && (
          <ExchangeSections
            exchange={detail.exchange}
            shownError={status === 'failed' ? detail.error : undefined}
          />
        )}
      {(node.kind.kind === 'If' || node.kind.kind === 'Switch') && detail?.branch && (
        <p data-testid='last-run-branch'>
          Took:{' '}
          <span className='font-mono'>{exitLabel(node.kind, detail.branch) ?? detail.branch}</span>
        </p>
      )}
      {(node.kind.kind === 'Output' || node.kind.kind === 'Input') &&
        detail?.value !== undefined && <ValueSection value={detail.value} />}
      {detail?.logs && detail.logs.length > 0 && <LogsSection logs={detail.logs} />}
    </div>
  );
}
