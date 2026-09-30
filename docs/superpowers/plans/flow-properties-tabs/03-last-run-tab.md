# Flow Properties Panel — Last run tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show what a node did in the last run — status, full error, skip reason, the Request/Wait exchange (response and request as sent), the branch an If/Switch took, an Output/Input value, and the node's script logs — in the panel's Last run tab.

**Architecture:** One new presentational component, `src/components/flow/properties/LastRunTab.tsx`, with small private sections. It reads only its props (`node`, `status`, `detail`), which plan 02 already passes to `NodePropertiesPanel`. No store access, no IPC. Bodies are shown in a read-only lazy `MonacoWrapper`, like `WireScriptDialog`.

**Reconciled with plan 02:** plan 02 renders shadcn `Tabs` with only the Settings trigger and content. Task 1 Step 3 adds the Last run trigger and content (after Settings).

**Tech Stack:** React 18 + TypeScript, shadcn/ui (`Badge`, `Button`, `Collapsible`, `Table`), lucide-react, Monaco via `@/components/editor/MonacoWrapper`, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-properties-panel-tabs-design.md` (§5.3). Contract: `docs/superpowers/plans/flow-properties-tabs/00-index.md`.

## Global Constraints

- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<table>` outside `@/components/ui/table`); lucide-react icons only.
- Multi-line bodies use Monaco, read-only. No new editor library.
- Content order (spec §5.3): status line → error → per-kind section → logs → never-ran state.
- Skip text: `An earlier node failed.` for `upstream_failed` (and a skip with no reason); `Its branch was not taken.` for `branch_not_taken`.
- Never-ran text: `Not run yet. Run the flow to see results here.`
- Truncation note text: `Truncated at 256 KB`.
- Status line reuses `msToSecondsLabel` (`src/lib/flow-repeat.ts`) for polled requests and `formatOutputValue` (`src/lib/flow-output.ts`) for JSON pretty-printing.
- Frontend checks: `yarn test src/components/flow`, `yarn tsc --noEmit`, `yarn check`.
- Commit every task with the `dev-workflow-skills:1-git-commit` skill. Never `git stash`.
- Never write the literal panicking-unwrap call text.

## Review Focus

1. **A failed node's long error** must be shown in full and selectable, not clamped to 3 lines like the card. → Task 1 test `shows the full error text`.
2. **A node that never ran** (status `idle`, no detail) must show the never-ran text and nothing else. → Task 1 test `shows the never-ran state`.
3. **A Request that failed before sending** (no `exchange`, only `error`) must not render an empty Response section. → Task 2 test `shows no exchange sections when nothing was sent`.
4. **A non-JSON response body** (HTML, plain text) must be shown as is, not break pretty-printing. → Task 2 test `shows a text body unchanged`.
5. **A running node** must show its progress text in the status line instead of stale results. → Task 1 test `shows progress while running`.

---

### Task 1: Tab shell — status line, error, skip reason, never-ran

**Files:**
- Create: `src/components/flow/properties/LastRunTab.tsx`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (the `TabsContent value='last-run'` block from plan 02)
- Test: `src/components/flow/properties/__tests__/LastRunTab.test.tsx`

**Interfaces:**
- Consumes: `FlowNodeDetail` (`src/types/pane-types.ts`, incl. `exchange`, `logs` from plan 01), `FlowNodeStatus`, `FlowNode` (`src/lib/tauri-api.ts`), `msToSecondsLabel`.
- Produces: `export function LastRunTab(props: { node: FlowNode; status: FlowNodeStatus; detail?: FlowNodeDetail }): JSX.Element`.

- [ ] **Step 1: Write the failing tests**

```tsx
// src/components/flow/properties/__tests__/LastRunTab.test.tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { LastRunTab } from '../LastRunTab';

// Monaco cannot run in jsdom. A read-only textarea stands in for it.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: { value: string }) => (
    <textarea aria-label='Body viewer' readOnly value={props.value} />
  ),
}));

const node = (kind: FlowNodeKind): FlowNode => ({ id: 'n1', kind, position: { x: 0, y: 0 } });
const request = node({
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});

describe('LastRunTab status', () => {
  it('shows the never-ran state', () => {
    render(<LastRunTab node={request} status='idle' />);
    expect(screen.getByText('Not run yet. Run the flow to see results here.')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-status')).not.toBeInTheDocument();
  });

  it('shows the status line of a success', () => {
    render(
      <LastRunTab node={request} status='success' detail={{ statusCode: 200, durationMs: 184 }} />,
    );
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('Success');
    expect(line).toHaveTextContent('200');
    expect(line).toHaveTextContent('184ms');
  });

  it('shows attempts and total time for a polled request', () => {
    render(
      <LastRunTab
        node={request}
        status='success'
        detail={{ statusCode: 200, durationMs: 14200, attempts: 7 }}
      />,
    );
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('7 attempts');
    expect(line).toHaveTextContent('14.2s');
  });

  it('shows progress while running', () => {
    render(<LastRunTab node={request} status='running' detail={{ progress: 'attempt 3/30' }} />);
    const line = screen.getByTestId('last-run-status');
    expect(line).toHaveTextContent('Running');
    expect(line).toHaveTextContent('attempt 3/30');
  });

  it('shows the full error text', () => {
    const error = `condition not met after 30 attempts (60.0s) ${'x'.repeat(400)}`;
    render(<LastRunTab node={request} status='failed' detail={{ statusCode: 404, error }} />);
    const box = screen.getByTestId('last-run-error');
    expect(box).toHaveTextContent(error);
    expect(box.className).not.toContain('line-clamp');
    expect(box.className).toContain('select-text');
  });

  it('explains an upstream skip', () => {
    render(<LastRunTab node={request} status='skipped' detail={{ skipReason: 'upstream_failed' }} />);
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('Skipped');
    expect(screen.getByText('An earlier node failed.')).toBeInTheDocument();
  });

  it('explains a branch that was not taken', () => {
    render(
      <LastRunTab node={request} status='skipped' detail={{ skipReason: 'branch_not_taken' }} />,
    );
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('Not taken');
    expect(screen.getByText('Its branch was not taken.')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: FAIL — `Failed to resolve import "../LastRunTab"`.

- [ ] **Step 3: Write the minimal implementation and mount it**

```tsx
// src/components/flow/properties/LastRunTab.tsx
import { Badge } from '@/components/ui/badge';
import { msToSecondsLabel } from '@/lib/flow-repeat';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';

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

export function LastRunTab({ node, status, detail }: LastRunTabProps) {
  if (status === 'idle') {
    return (
      <p className='text-xs text-muted-foreground'>Not run yet. Run the flow to see results here.</p>
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
    </div>
  );
}
```

In `src/components/flow/properties/NodePropertiesPanel.tsx`, import `LastRunTab`. Plan 02 adds only the Settings tab, so ADD the Last run tab to the same `Tabs`: a trigger after the Settings trigger, and a content block after the Settings content, copying the Settings trigger's and content's class names:

```tsx
<TabsTrigger value='last-run' className='text-xs'>
  Last run
</TabsTrigger>
```

```tsx
<TabsContent value='last-run' className='min-h-0 flex-1 overflow-y-auto p-3'>
  <LastRunTab node={node} status={status} detail={detail} />
</TabsContent>
```

Add a panel test that clicking the "Last run" trigger calls `onTabChange('last-run')`, and that with `activeTab='last-run'` the tab content renders.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow/properties`
Expected: PASS (the new `LastRunTab` tests and the existing panel tests).

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: both clean. If Biome only reports formatting in the two files, run `yarn biome format --write src/components/flow/properties/LastRunTab.tsx src/components/flow/properties/__tests__/LastRunTab.test.tsx src/components/flow/properties/NodePropertiesPanel.tsx` and re-run.

- [ ] **Step 6: Commit**

Commit with the dev-workflow-skills:1-git-commit skill (inline fallback mode for subagents). Stage only the three files. Suggested message: `feat(flow): add the Last run tab status section`.

---

### Task 2: Request and Wait exchange — response and request as sent

**Files:**
- Modify: `src/components/flow/properties/LastRunTab.tsx`
- Test: `src/components/flow/properties/__tests__/LastRunTab.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowDebugRequest`, `FlowDebugResponse` (with plan 01's `truncated?: boolean`), `FlowDebugHeader` from `src/lib/tauri-api.ts`; `formatOutputValue` from `src/lib/flow-output.ts`.
- Produces: nothing new outside the file.

- [ ] **Step 1: Write the failing tests**

Append to `LastRunTab.test.tsx`:

```tsx
import userEvent from '@testing-library/user-event';
import type { FlowDebugRequest } from '@/lib/tauri-api';

const exchange: FlowDebugRequest = {
  method: 'POST',
  url: 'https://api.example.com/login',
  headers: [
    { key: 'Content-Type', value: 'application/json' },
    { key: 'Authorization', value: '[REDACTED]' },
  ],
  body: '{"user":"ada"}',
  response: {
    status: 200,
    statusText: 'OK',
    durationMs: 184,
    sizeBytes: 15,
    headers: [{ key: 'content-type', value: 'application/json' }],
    body: '{"token":"abc"}',
  },
};

describe('LastRunTab exchange', () => {
  it('shows the response headers and a pretty body', () => {
    render(
      <LastRunTab node={request} status='success' detail={{ statusCode: 200, exchange }} />,
    );
    const response = screen.getByTestId('last-run-response');
    expect(response).toHaveTextContent('content-type');
    expect(response).toHaveTextContent('application/json');
    expect(screen.getByLabelText('Body viewer')).toHaveValue('{\n  "token": "abc"\n}');
  });

  it('shows a text body unchanged', () => {
    const html = { ...exchange, response: { ...exchange.response!, body: '<h1>Hi</h1>' } };
    render(<LastRunTab node={request} status='success' detail={{ exchange: html }} />);
    expect(screen.getByLabelText('Body viewer')).toHaveValue('<h1>Hi</h1>');
  });

  it('notes a truncated body', () => {
    const cut = { ...exchange, response: { ...exchange.response!, truncated: true } };
    render(<LastRunTab node={request} status='success' detail={{ exchange: cut }} />);
    expect(screen.getByText('Truncated at 256 KB')).toBeInTheDocument();
  });

  it('shows the request as sent when expanded', async () => {
    render(<LastRunTab node={request} status='success' detail={{ exchange }} />);
    expect(screen.queryByTestId('last-run-request')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Request as sent/ }));
    const sent = screen.getByTestId('last-run-request');
    expect(sent).toHaveTextContent('POST');
    expect(sent).toHaveTextContent('https://api.example.com/login');
    expect(sent).toHaveTextContent('[REDACTED]');
    expect(sent).toHaveTextContent('{"user":"ada"}');
  });

  it('shows no exchange sections when nothing was sent', () => {
    render(
      <LastRunTab node={request} status='failed' detail={{ error: 'could not resolve host' }} />,
    );
    expect(screen.queryByTestId('last-run-response')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Request as sent/ })).not.toBeInTheDocument();
  });

  it('shows the send error of an exchange without a response', () => {
    const noResponse = { ...exchange, response: undefined, error: 'connection refused' };
    render(<LastRunTab node={request} status='failed' detail={{ exchange: noResponse }} />);
    expect(screen.queryByTestId('last-run-response')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Request as sent/ })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: FAIL — `Unable to find an element by: [data-testid="last-run-response"]`.

- [ ] **Step 3: Implement the exchange sections**

Add to `LastRunTab.tsx` (keep Task 1's code; extend imports):

```tsx
import { Check, ChevronRight, Copy } from 'lucide-react';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table';
import { formatOutputValue } from '@/lib/flow-output';
import type { FlowDebugHeader, FlowDebugRequest } from '@/lib/tauri-api';

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

function BodyViewer({ body }: { body: string }) {
  return (
    <div className='h-48 overflow-hidden rounded-md border'>
      <Suspense fallback={<div className='p-2 text-muted-foreground'>Loading…</div>}>
        <MonacoWrapper value={formatOutputValue(body)} readOnly height='100%' language='json' />
      </Suspense>
    </div>
  );
}

function ExchangeSections({ exchange }: { exchange: FlowDebugRequest }) {
  const [sentOpen, setSentOpen] = useState(false);
  const response = exchange.response;
  return (
    <div className='space-y-3'>
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
              className={sentOpen ? 'h-3 w-3 rotate-90 transition-transform' : 'h-3 w-3 transition-transform'}
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
```

In `LastRunTab`, after the skip paragraph, add:

```tsx
{(node.kind.kind === 'Request' || node.kind.kind === 'WaitForCallback') && detail?.exchange && (
  <ExchangeSections exchange={detail.exchange} />
)}
```

The request body is shown in a `<pre>` rather than Monaco, because it is what the app sent and usually short; the response body can be large, so it gets Monaco.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: PASS, all Task 1 and Task 2 tests.

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: clean. Format the touched files with `yarn biome format --write` if Biome reports formatting only.

- [ ] **Step 6: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): show the last response in the panel`.

---

### Task 3: Branch, value and logs

**Files:**
- Modify: `src/components/flow/properties/LastRunTab.tsx`
- Test: `src/components/flow/properties/__tests__/LastRunTab.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowNodeDetail.branch`, `.value`, `.logs` (`FlowLogEntry { level: 'log' | 'warn' | 'error'; message: string }`); `exitLabel` from `src/components/flow/flowExits.ts`.
- Produces: nothing new outside the file.

- [ ] **Step 1: Write the failing tests**

Append to `LastRunTab.test.tsx`:

```tsx
describe('LastRunTab per kind', () => {
  it('shows the branch an If took', () => {
    const ifNode = node({ kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
    render(<LastRunTab node={ifNode} status='success' detail={{ branch: 'true' }} />);
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Took: true');
  });

  it('shows the case label a Switch took', () => {
    const sw = node({
      kind: 'Switch',
      label: 'Type',
      value: 'response.body.type',
      cases: [{ id: 'c1', label: 'Admin', matches: 'admin' }],
    });
    render(<LastRunTab node={sw} status='success' detail={{ branch: 'case:c1' }} />);
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Took: Admin');
  });

  it('shows an Output value pretty-printed with a copy button', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(<LastRunTab node={out} status='success' detail={{ value: '{"a":1}' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('"a": 1');
    expect(screen.getByRole('button', { name: 'Copy value' })).toBeInTheDocument();
  });

  it('shows an Input value', () => {
    const input = node({ kind: 'Input', label: 'User', value: '{{user}}' });
    render(<LastRunTab node={input} status='success' detail={{ value: 'ada' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('ada');
  });

  it('shows (empty) for an empty value', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(<LastRunTab node={out} status='success' detail={{ value: '' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('(empty)');
  });

  it('lists the node logs with their level', () => {
    render(
      <LastRunTab
        node={request}
        status='success'
        detail={{
          logs: [
            { level: 'log', message: 'wire data: {}' },
            { level: 'error', message: 'boom' },
          ],
        }}
      />,
    );
    const logs = screen.getByTestId('last-run-logs');
    expect(logs).toHaveTextContent('wire data: {}');
    expect(logs).toHaveTextContent('boom');
    expect(screen.getByText('boom').className).toContain('text-red-600');
  });

  it('shows no logs section without logs', () => {
    render(<LastRunTab node={request} status='success' detail={{ statusCode: 200 }} />);
    expect(screen.queryByTestId('last-run-logs')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: FAIL — `Unable to find an element by: [data-testid="last-run-branch"]`.

- [ ] **Step 3: Implement the sections**

Add to `LastRunTab.tsx`:

```tsx
import { exitLabel } from '../flowExits';
import type { FlowLogEntry } from '@/lib/tauri-api';

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
```

In `LastRunTab`, after the exchange block, add:

```tsx
{(node.kind.kind === 'If' || node.kind.kind === 'Switch') && detail?.branch && (
  <p data-testid='last-run-branch'>
    Took: <span className='font-mono'>{exitLabel(node.kind, detail.branch) ?? detail.branch}</span>
  </p>
)}
{(node.kind.kind === 'Output' || node.kind.kind === 'Input') && detail?.value !== undefined && (
  <ValueSection value={detail.value} />
)}
{detail?.logs && detail.logs.length > 0 && <LogsSection logs={detail.logs} />}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow`
Expected: PASS — all `LastRunTab` tests and the rest of the flow suite.

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: clean.

- [ ] **Step 6: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): show branch, value and logs in Last run`.
