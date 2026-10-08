import { Check, ChevronRight, Copy } from 'lucide-react';
import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table';
import { copyTextAsync } from '@/lib/clipboard';
import { formatOutputValue } from '@/lib/flow-output';
import { msToSecondsLabel } from '@/lib/flow-repeat';
import type {
  FlowDebugHeader,
  FlowDebugRequest,
  FlowLiveProgress,
  FlowLogEntry,
  FlowNode,
  FlowNodeStatus,
  FlowRejectedCall,
  FlowRouteEval,
  FlowWireValue,
} from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import type { FlowNodeDetail } from '@/types/pane-types';
import { CallbackUrlField } from '../CallbackUrlField';
import { exitDisplayLabel, fieldLabel } from './wireRows';

interface LastRunTabProps {
  node: FlowNode;
  status: FlowNodeStatus;
  detail?: FlowNodeDetail;
  /** The flow's nodes, to name the source of each input. */
  nodes?: FlowNode[];
  /** This run's callback URL, for a Wait node. Absent when no run is active. */
  callbackUrl?: string;
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
    // Started in the click handler, so WebKit keeps the user activation.
    copyTextAsync(Promise.resolve(text)).then(
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

// Parses a body once. Any JSON gets the json language, but only an object or
// array is re-indented, as formatOutputValue does.
function parseBody(body: string): { isJson: boolean; pretty: string } {
  try {
    const parsed: unknown = JSON.parse(body);
    const pretty =
      typeof parsed === 'object' && parsed !== null ? JSON.stringify(parsed, null, 2) : body;
    return { isJson: true, pretty };
  } catch {
    return { isJson: false, pretty: body };
  }
}

function BodyViewer({ body }: { body: string }) {
  const parsed = useMemo(() => (body === '' ? null : parseBody(body)), [body]);
  if (parsed === null) return <p className='text-muted-foreground'>No body.</p>;
  return (
    <div className='h-48 overflow-hidden rounded-md border'>
      <Suspense fallback={<div className='p-2 text-muted-foreground'>Loading…</div>}>
        <MonacoWrapper
          value={parsed.pretty}
          readOnly
          height='100%'
          language={parsed.isJson ? 'json' : 'plaintext'}
        />
      </Suspense>
    </div>
  );
}

function ExchangeSections({
  exchange,
  shownError,
  received = false,
}: {
  exchange: FlowDebugRequest;
  shownError?: string;
  // A callback wait sent nothing. Its record holds the call it received.
  received?: boolean;
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
            <h4 className='min-w-0 font-medium [overflow-wrap:anywhere]'>
              {received
                ? `Received call · ${exchange.method} ${exchange.url}`
                : `Response · ${response.status} ${response.statusText}`}
            </h4>
            <CopyButton text={response.body} label='Copy response body' />
          </div>
          <HeadersTable headers={response.headers} />
          <BodyViewer body={response.body} />
          {response.truncated && <p className='text-muted-foreground'>Truncated at 256 KB</p>}
        </section>
      )}
      {!received && (
        <Collapsible open={sentOpen} onOpenChange={setSentOpen}>
          <CollapsibleTrigger asChild>
            <Button type='button' variant='ghost' size='sm' className='h-6 px-1 text-xs'>
              <ChevronRight
                className={
                  sentOpen
                    ? 'h-3 w-3 rotate-90 transition-transform'
                    : 'h-3 w-3 transition-transform'
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
              {exchange.bodyTruncated && (
                <p className='text-muted-foreground'>Truncated at 256 KB</p>
              )}
            </section>
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  );
}

const logClass: Record<FlowLogEntry['level'], string> = {
  log: '',
  warn: 'text-amber-600 dark:text-amber-400',
  error: 'text-red-600',
};

function ValueSection({ value, truncated = false }: { value: string; truncated?: boolean }) {
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
      {truncated && <p className='text-muted-foreground'>Cut at 256 KB.</p>}
    </section>
  );
}

// A wire's value as the step received it. A credential is never shown.
function WireValueText({ wire }: { wire: FlowWireValue }) {
  if (wire.credential) {
    return <p className='italic text-muted-foreground'>Credential (hidden)</p>;
  }
  if (wire.error) {
    return (
      <p
        data-testid='last-run-input-error'
        className='select-text whitespace-pre-wrap break-words text-red-600'
      >
        {wire.error}
      </p>
    );
  }
  if (wire.value === undefined) return null;
  return (
    <>
      <pre className='max-h-32 select-text overflow-auto whitespace-pre-wrap rounded-md border p-1.5 font-mono text-[11px] [overflow-wrap:anywhere]'>
        {wire.value === '' ? <span className='italic'>(empty)</span> : wire.value}
      </pre>
      {wire.truncated && <p className='text-muted-foreground'>Cut at 16 KB.</p>}
    </>
  );
}

function InputsSection({
  wires,
  nodes,
  failedEdgeId,
}: {
  wires: FlowWireValue[];
  nodes?: FlowNode[];
  failedEdgeId?: string;
}) {
  // A source deleted after the run is named by its id.
  const sourceLabel = (id: string) => nodes?.find((n) => n.id === id)?.kind.label || id;
  return (
    <section data-testid='last-run-inputs' className='space-y-1.5'>
      <h4 className='font-medium'>Inputs</h4>
      {wires.map((wire) => {
        const failed = wire.edgeId === failedEdgeId;
        return (
          <div
            key={wire.edgeId}
            data-testid='last-run-input'
            data-failed={failed ? 'true' : undefined}
            className={cn(
              'space-y-1 rounded-md border px-2 py-1.5',
              failed && 'border-red-500/60 bg-red-500/5',
            )}
          >
            <p className='[overflow-wrap:anywhere]'>
              <span className='font-medium'>{fieldLabel(wire.targetField)}</span>
              <span className='text-muted-foreground'> ← {sourceLabel(wire.sourceNodeId)}</span>
            </p>
            <WireValueText wire={wire} />
          </div>
        );
      })}
    </section>
  );
}

// How a routing node decided. A run from before routes were recorded only
// knows the exit it took.
function RouteLine({
  node,
  branch,
  route,
}: {
  node: FlowNode;
  branch: string;
  route?: FlowRouteEval;
}) {
  const exit = exitDisplayLabel(node, branch);
  if (!route) {
    return (
      <p data-testid='last-run-branch'>
        Took: <span className='font-mono'>{exit}</span>
      </p>
    );
  }
  if (route.kind === 'if') {
    return (
      <p data-testid='last-run-branch'>
        Condition → <span className='font-mono'>{route.value}</span>
      </p>
    );
  }
  return (
    <p data-testid='last-run-branch' className='[overflow-wrap:anywhere]'>
      Value <span className='font-mono'>{route.value}</span> →{' '}
      {route.matchedCase ? `case ${exit}` : 'default'}
    </p>
  );
}

const LOG_LINE = 'select-text whitespace-pre-wrap [overflow-wrap:anywhere]';

function LogsSection({ logs }: { logs: FlowLogEntry[] }) {
  return (
    <section data-testid='last-run-logs' className='space-y-1'>
      <h4 className='font-medium'>Logs</h4>
      <div className='max-h-48 space-y-0.5 overflow-auto rounded-md border p-2 font-mono text-[11px]'>
        {logs.map((entry, i) => (
          // Log lines have no id and can repeat.
          // biome-ignore lint/suspicious/noArrayIndexKey: log order is stable within a run.
          <p key={i} className={`${LOG_LINE} ${logClass[entry.level]}`}>
            {entry.message}
          </p>
        ))}
      </div>
    </section>
  );
}

// Whole seconds left before a running node gives up. Each live event restarts
// the count from the backend's `remainingMs`, and the hook ticks once a second.
function useSecondsLeft(live?: FlowLiveProgress): number | undefined {
  const [deadline, setDeadline] = useState<number | undefined>(undefined);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const remaining = live?.remainingMs;
    if (remaining === undefined) {
      setDeadline(undefined);
      return;
    }
    const start = Date.now();
    setNow(start);
    setDeadline(start + remaining);
  }, [live]);
  useEffect(() => {
    if (deadline === undefined) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [deadline]);
  return deadline === undefined ? undefined : Math.max(0, Math.ceil((deadline - now) / 1000));
}

function verdictText(conditionMet?: boolean): string {
  if (conditionMet === undefined) return 'no verdict';
  return conditionMet ? 'condition true' : 'condition false';
}

// While a poll runs: its last status, verdict and time left.
function PollLiveLine({ live }: { live: FlowLiveProgress }) {
  const seconds = useSecondsLeft(live);
  const parts: string[] = [];
  if (live.lastStatusCode !== undefined) parts.push(`Last status ${live.lastStatusCode}`);
  if (live.conditionMet !== undefined) parts.push(verdictText(live.conditionMet));
  if (seconds !== undefined) parts.push(`${seconds}s left`);
  if (parts.length === 0) return null;
  return (
    <p data-testid='last-run-poll-live' className='text-muted-foreground'>
      {parts.join(' · ')}
    </p>
  );
}

// While a callback wait runs: ignored calls and time left.
function WaitLiveLine({ live }: { live: FlowLiveProgress }) {
  const seconds = useSecondsLeft(live);
  const parts: string[] = [];
  if (live.ignored !== undefined) parts.push(`${live.ignored} ignored`);
  if (seconds !== undefined) parts.push(`${seconds}s left`);
  if (parts.length === 0) return null;
  return (
    <p data-testid='last-run-wait-live' className='text-muted-foreground'>
      {parts.join(' · ')}
    </p>
  );
}

// A call the wait turned down, collapsed because its body can be large.
function RejectedCallSection({ call }: { call: FlowRejectedCall }) {
  const [open, setOpen] = useState(false);
  return (
    <section data-testid='last-run-rejected'>
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger asChild>
          <Button type='button' variant='ghost' size='sm' className='h-6 px-1 text-xs'>
            <ChevronRight
              className={
                open ? 'h-3 w-3 rotate-90 transition-transform' : 'h-3 w-3 transition-transform'
              }
            />
            Last rejected call
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className='mt-1.5 space-y-1.5'>
            <p className='select-text font-mono text-[11px] [overflow-wrap:anywhere]'>
              {call.method} {call.url}
            </p>
            <p className='text-muted-foreground'>{call.reason}</p>
            <HeadersTable headers={call.headers} />
            <BodyViewer body={call.body} />
            {call.bodyTruncated && <p className='text-muted-foreground'>Body truncated.</p>}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </section>
  );
}

export function LastRunTab({ node, status, detail, nodes, callbackUrl }: LastRunTabProps) {
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
      {node.kind.kind === 'Request' && status === 'running' && detail?.live && (
        <PollLiveLine live={detail.live} />
      )}
      {node.kind.kind === 'Request' && status !== 'running' && detail?.trace?.poll && (
        <p data-testid='last-run-poll' className='text-muted-foreground'>
          {[
            detail.trace.poll.lastStatusCode !== undefined
              ? `Last status ${detail.trace.poll.lastStatusCode}`
              : null,
            verdictText(detail.trace.poll.conditionMet),
            `${detail.trace.poll.attempts} of ${detail.trace.poll.maxAttempts} attempts`,
          ]
            .filter((part) => part !== null)
            .join(' · ')}
        </p>
      )}
      {node.kind.kind === 'WaitForCallback' && status === 'running' && detail?.live && (
        <>
          <WaitLiveLine live={detail.live} />
          {detail.live.lastRejected && <RejectedCallSection call={detail.live.lastRejected} />}
        </>
      )}
      {node.kind.kind === 'WaitForCallback' && callbackUrl && (
        <section className='space-y-1'>
          <h4 className='font-medium'>Callback URL</h4>
          <CallbackUrlField url={callbackUrl} />
        </section>
      )}
      {node.kind.kind === 'WaitForCallback' && status !== 'running' && detail?.trace?.wait && (
        <>
          <p data-testid='last-run-wait' className='text-muted-foreground'>
            {detail.trace.wait.ignored} ignored
          </p>
          {detail.trace.wait.lastRejected && (
            <RejectedCallSection call={detail.trace.wait.lastRejected} />
          )}
        </>
      )}
      {status === 'skipped' && <p className='text-muted-foreground'>{skipText(detail)}</p>}
      {detail?.trace?.wires && detail.trace.wires.length > 0 && (
        <InputsSection
          wires={detail.trace.wires}
          nodes={nodes}
          failedEdgeId={detail.trace.failedEdgeId}
        />
      )}
      {(node.kind.kind === 'Request' || node.kind.kind === 'WaitForCallback') &&
        detail?.exchange && (
          <ExchangeSections
            exchange={detail.exchange}
            shownError={status === 'failed' ? detail.error : undefined}
            received={node.kind.kind === 'WaitForCallback'}
          />
        )}
      {(node.kind.kind === 'If' || node.kind.kind === 'Switch') && detail?.branch && (
        <RouteLine node={node} branch={detail.branch} route={detail.trace?.route} />
      )}
      {(node.kind.kind === 'If' || node.kind.kind === 'Switch') && status === 'success' && (
        <p className='text-muted-foreground'>Passes its input through.</p>
      )}
      {node.kind.kind === 'Auth' && status === 'success' && (
        <p className='text-muted-foreground'>The credential is hidden.</p>
      )}
      {(node.kind.kind === 'Output' ||
        node.kind.kind === 'Input' ||
        node.kind.kind === 'Transform') &&
        detail?.value !== undefined && (
          <ValueSection value={detail.value} truncated={detail.trace?.valueTruncated} />
        )}
      {detail?.logs && detail.logs.length > 0 && <LogsSection logs={detail.logs} />}
    </div>
  );
}
