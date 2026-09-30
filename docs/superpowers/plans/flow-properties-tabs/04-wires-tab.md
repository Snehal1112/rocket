# Flow Properties Panel — Wires tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** List every wire into and out of the selected node, let a wire's row open its script dialog, let a node name select that node, and fade wires the last run did not take.

**Architecture:** One new presentational component, `src/components/flow/properties/WiresTab.tsx`, plus a pure helper module `src/components/flow/properties/wireRows.ts` that turns `(node, nodes, edges)` into display rows (easy to unit-test without React). The tab reuses `exitLabel` and `edgeRunState` from `src/components/flow/flowExits.ts`, so the panel and the canvas agree on exit names and on "not taken". It calls `onEditWire` and `onSelectNode`, which plan 02 wires in `FlowPane`.

**Assumptions about plan 02 (checked in reconciliation):**
1. Reconciled with plans 02/03: `NodePropertiesPanel` renders shadcn `Tabs` with the Settings and Last run triggers and contents; Task 3 ADDS the Wires trigger and content after Last run.
2. `NodePropertiesPanel` already receives `nodes`, `edges`, `onEditWire` and `onSelectNode`, and `FlowPane` implements them: `onEditWire(edgeId)` opens the same `WireScriptDialog` as the canvas's `onEdgeEdit`; `onSelectNode(nodeId)` sets the canvas selection to that node and shows it in the panel (same tab).
3. The locked contract gives the panel the selected node's `status`/`detail` only. The Wires tab needs every node's status and detail to decide "not taken" for incoming wires, so Task 3 adds two optional props, `nodeStatus?: Record<string, FlowNodeStatus>` and `nodeDetail?: Record<string, FlowNodeDetail>`, and passes `tab.nodeStatus` / `tab.nodeDetail` from `FlowPane`. Task 3 also updates `00-index.md` with these two props.

**Tech Stack:** React 18 + TypeScript, shadcn/ui (`Button`), lucide-react, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-properties-panel-tabs-design.md` (§6). Contract: `docs/superpowers/plans/flow-properties-tabs/00-index.md`.

## Global Constraints

- shadcn/ui primitives only (rows and node links are `Button`s, no raw `<button>`); lucide-react icons only.
- Field names as the node shows them: `url` → `URL`, `body` → `Body`, `headers[Name].value` → `Name`, `headers` → `Headers`, `trigger` → `Run when`, `value` → `Value`, `input` → `Input`; any other field is shown as stored.
- Exit names: `result` is omitted; others use `exitLabel(source.kind, handle)` (so `true`, `false`, a case label, `default`), falling back to the raw handle.
- Script preview is one line: the first line of the expression, trimmed, cut to 60 characters with `…`; `(no script)` for Run when wires and blank expressions.
- Run when rows are not clickable. Rows whose source node is missing show `(missing node)` and are not clickable.
- Empty text: `No wires. Drag from a dot on the canvas to connect nodes.`
- "not taken" uses `edgeRunState(edge, source, sourceStatus, sourceBranch) === 'not-taken'`.
- Frontend checks: `yarn test src/components/flow`, `yarn tsc --noEmit`, `yarn check`.
- Commit every task with the `dev-workflow-skills:1-git-commit` skill. Never `git stash`.
- Never write the literal panicking-unwrap call text.

## Review Focus

1. **A header wire** (`headers[Authorization].value`) must show the header name `Authorization`, not the raw field path. → Task 1 test `names header, URL, body and Run when fields`.
2. **A multi-line wire script** must preview only its first line, cut at 60 characters. → Task 1 test `previews only the first line of a script`.
3. **A Run when wire** must not open the script dialog (it has no script). → Task 2 test `a Run when row is not clickable`.
4. **An If with both exits wired** must group outgoing wires under `true` and `false`, and after a run fade the exit not taken. → Task 2 tests `groups outgoing wires by exit` and `fades wires the last run did not take`.
5. **Clicking a node name inside a row** must select that node and not also open the wire dialog. → Task 2 test `a node link selects the node without opening the wire`.

---

### Task 1: Wire rows helper

**Files:**
- Create: `src/components/flow/properties/wireRows.ts`
- Test: `src/components/flow/properties/__tests__/wireRows.test.ts`

**Interfaces:**
- Consumes: `FlowEdge`, `FlowNode`, `FlowNodeStatus` (`src/lib/tauri-api.ts`); `FlowNodeDetail` (`src/types/pane-types.ts`); `exitLabel`, `edgeRunState` (`src/components/flow/flowExits.ts`); `RESULT_HANDLE`, `TRIGGER_HANDLE` (`src/lib/flow-handles.ts`).
- Produces:

```ts
export interface WireRow {
  edgeId: string;
  /** Field on the target node, as the node names it. */
  field: string;
  /** The other end of the wire: the source for incoming rows, the target for outgoing rows. */
  otherNodeId: string;
  /** Label of the other node, or null when it no longer exists. */
  otherLabel: string | null;
  /** Exit the wire leaves through, or null for the default `result` exit. */
  exit: string | null;
  /** One-line script preview, or null for a wire without a script. */
  preview: string | null;
  /** True when the row can open the script dialog. */
  editable: boolean;
  /** True when the last run did not take this wire. */
  notTaken: boolean;
}
export interface OutgoingGroup { exit: string | null; rows: WireRow[] }
export function fieldLabel(targetField: string): string;
export function scriptPreview(expression: string): string | null;
export function incomingRows(node: FlowNode, nodes: FlowNode[], edges: FlowEdge[], nodeStatus?: Record<string, FlowNodeStatus>, nodeDetail?: Record<string, FlowNodeDetail>): WireRow[];
export function outgoingGroups(node: FlowNode, nodes: FlowNode[], edges: FlowEdge[], nodeStatus?: Record<string, FlowNodeStatus>, nodeDetail?: Record<string, FlowNodeDetail>): OutgoingGroup[];
```

- [ ] **Step 1: Write the failing tests**

```ts
// src/components/flow/properties/__tests__/wireRows.test.ts
import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { fieldLabel, incomingRows, outgoingGroups, scriptPreview } from '../wireRows';

const n = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });
const login = n('login', {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});
const check = n('check', { kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
const users = n('users', {
  kind: 'Request',
  label: 'List Users',
  source: { type: 'Saved', requestPath: 'users/list.yml' },
});
const out = n('out', { kind: 'Output', label: 'Result' });
const edge = (e: Partial<FlowEdge> & Pick<FlowEdge, 'id' | 'sourceNodeId' | 'targetNodeId' | 'targetField'>): FlowEdge => ({
  expression: '',
  ...e,
});

describe('fieldLabel', () => {
  it('names header, URL, body and Run when fields', () => {
    expect(fieldLabel('headers[Authorization].value')).toBe('Authorization');
    expect(fieldLabel('url')).toBe('URL');
    expect(fieldLabel('body')).toBe('Body');
    expect(fieldLabel('trigger')).toBe('Run when');
    expect(fieldLabel('value')).toBe('Value');
    expect(fieldLabel('input')).toBe('Input');
    expect(fieldLabel('headers')).toBe('Headers');
    expect(fieldLabel('custom')).toBe('custom');
  });
});

describe('scriptPreview', () => {
  it('previews only the first line of a script', () => {
    expect(scriptPreview('const t = response.body.token;\nreturn t;')).toBe(
      'const t = response.body.token;',
    );
    expect(scriptPreview(`  ${'a'.repeat(80)}  `)).toBe(`${'a'.repeat(60)}…`);
    expect(scriptPreview('   ')).toBeNull();
    expect(scriptPreview('')).toBeNull();
  });
});

describe('incomingRows', () => {
  it('lists incoming wires with source label and exit', () => {
    const edges = [
      edge({ id: 'e1', sourceNodeId: 'login', targetNodeId: 'users', targetField: 'headers[Authorization].value', expression: "'Bearer ' + response.body.token" }),
      edge({ id: 'e2', sourceNodeId: 'check', targetNodeId: 'users', targetField: 'trigger', sourceHandle: 'true' }),
    ];
    const rows = incomingRows(users, [login, check, users], edges);
    expect(rows).toEqual([
      expect.objectContaining({ edgeId: 'e1', field: 'Authorization', otherLabel: 'Login', exit: null, preview: "'Bearer ' + response.body.token", editable: true }),
      expect.objectContaining({ edgeId: 'e2', field: 'Run when', otherLabel: 'Ok?', exit: 'true', preview: null, editable: false }),
    ]);
  });

  it('marks a missing source node', () => {
    const edges = [edge({ id: 'e1', sourceNodeId: 'gone', targetNodeId: 'users', targetField: 'url', expression: 'response.body' })];
    const [row] = incomingRows(users, [users], edges);
    expect(row.otherLabel).toBeNull();
    expect(row.editable).toBe(false);
  });

  it('marks a wire the last run did not take', () => {
    const edges = [edge({ id: 'e2', sourceNodeId: 'check', targetNodeId: 'users', targetField: 'trigger', sourceHandle: 'true' })];
    const [row] = incomingRows(users, [check, users], edges, { check: 'success' }, { check: { branch: 'false' } });
    expect(row.notTaken).toBe(true);
  });
});

describe('outgoingGroups', () => {
  it('groups outgoing wires by exit in handle order', () => {
    const edges = [
      edge({ id: 'e3', sourceNodeId: 'check', targetNodeId: 'out', targetField: 'value', sourceHandle: 'false', expression: 'response.body' }),
      edge({ id: 'e2', sourceNodeId: 'check', targetNodeId: 'users', targetField: 'trigger', sourceHandle: 'true' }),
    ];
    const groups = outgoingGroups(check, [check, users, out], edges);
    expect(groups.map((g) => g.exit)).toEqual(['true', 'false']);
    expect(groups[0].rows[0]).toEqual(expect.objectContaining({ otherLabel: 'List Users', field: 'Run when' }));
    expect(groups[1].rows[0]).toEqual(expect.objectContaining({ otherLabel: 'Result', field: 'Value', editable: true }));
  });

  it('puts plain result wires in one group with no exit name', () => {
    const edges = [edge({ id: 'e1', sourceNodeId: 'login', targetNodeId: 'users', targetField: 'url', expression: 'response.body.next' })];
    const groups = outgoingGroups(login, [login, users], edges);
    expect(groups).toEqual([{ exit: null, rows: [expect.objectContaining({ edgeId: 'e1', otherLabel: 'List Users', field: 'URL' })] }]);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/wireRows.test.ts`
Expected: FAIL — `Failed to resolve import "../wireRows"`.

- [ ] **Step 3: Implement the helper**

```ts
// src/components/flow/properties/wireRows.ts
import { RESULT_HANDLE, TRIGGER_HANDLE } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { edgeRunState, exitLabel } from '../flowExits';

export interface WireRow {
  edgeId: string;
  field: string;
  otherNodeId: string;
  otherLabel: string | null;
  exit: string | null;
  preview: string | null;
  editable: boolean;
  notTaken: boolean;
}

export interface OutgoingGroup {
  exit: string | null;
  rows: WireRow[];
}

const FIELD_LABELS: Record<string, string> = {
  url: 'URL',
  body: 'Body',
  headers: 'Headers',
  [TRIGGER_HANDLE]: 'Run when',
  value: 'Value',
  input: 'Input',
};

const PREVIEW_MAX = 60;

// A header wire names its header; other fields use the label the node shows.
export function fieldLabel(targetField: string): string {
  const header = /^headers\[(.+)\]\.value$/.exec(targetField);
  if (header) return header[1];
  return FIELD_LABELS[targetField] ?? targetField;
}

// The first non-empty line of a script, cut to fit one row.
export function scriptPreview(expression: string): string | null {
  const firstLine = expression
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line !== '');
  if (!firstLine) return null;
  return firstLine.length > PREVIEW_MAX ? `${firstLine.slice(0, PREVIEW_MAX)}…` : firstLine;
}

// The exit name of an edge's source, or null for the default exit.
function exitName(edge: FlowEdge, source: FlowNode | undefined): string | null {
  const handle = edge.sourceHandle ?? RESULT_HANDLE;
  if (handle === RESULT_HANDLE) return null;
  return (source && exitLabel(source.kind, handle)) ?? handle;
}

function isNotTaken(
  edge: FlowEdge,
  source: FlowNode | undefined,
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): boolean {
  return (
    edgeRunState(
      edge,
      source,
      nodeStatus?.[edge.sourceNodeId],
      nodeDetail?.[edge.sourceNodeId]?.branch,
    ) === 'not-taken'
  );
}

function row(
  edge: FlowEdge,
  other: FlowNode | undefined,
  source: FlowNode | undefined,
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): WireRow {
  const isTrigger = edge.targetField === TRIGGER_HANDLE;
  const preview = isTrigger ? null : scriptPreview(edge.expression);
  return {
    edgeId: edge.id,
    field: fieldLabel(edge.targetField),
    otherNodeId: other?.id ?? '',
    otherLabel: other?.kind.label ?? null,
    exit: exitName(edge, source),
    preview,
    // A Run when wire has no script, and a wire to a missing node cannot be edited.
    editable: !isTrigger && other !== undefined,
    notTaken: isNotTaken(edge, source, nodeStatus, nodeDetail),
  };
}

export function incomingRows(
  node: FlowNode,
  nodes: FlowNode[],
  edges: FlowEdge[],
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): WireRow[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  return edges
    .filter((e) => e.targetNodeId === node.id)
    .map((e) => {
      const source = byId.get(e.sourceNodeId);
      return row(e, source, source, nodeStatus, nodeDetail);
    });
}

export function outgoingGroups(
  node: FlowNode,
  nodes: FlowNode[],
  edges: FlowEdge[],
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): OutgoingGroup[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const groups: OutgoingGroup[] = [];
  for (const e of edges.filter((edge) => edge.sourceNodeId === node.id)) {
    const r = row(e, byId.get(e.targetNodeId), node, nodeStatus, nodeDetail);
    const group = groups.find((g) => g.exit === r.exit);
    if (group) group.rows.push(r);
    else groups.push({ exit: r.exit, rows: [r] });
  }
  return groups.sort((a, b) => exitOrder(node, a.exit) - exitOrder(node, b.exit));
}

// Orders exits as the node draws them: result, true, false, cases in order, default.
function exitOrder(node: FlowNode, exit: string | null): number {
  if (exit === null) return 0;
  const kind = node.kind;
  if (kind.kind === 'If') return exit === 'true' ? 1 : 2;
  if (kind.kind === 'Switch') {
    const index = kind.cases.findIndex((c, i) => exitLabel(kind, `case:${c.id}`) === exit || `Case ${i + 1}` === exit);
    return index < 0 ? kind.cases.length + 1 : index + 1;
  }
  return 1;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow/properties/__tests__/wireRows.test.ts`
Expected: PASS.

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: clean. Format with `yarn biome format --write <files>` if only formatting is reported.

- [ ] **Step 6: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): build wire rows for the Wires tab`.

---

### Task 2: WiresTab component

**Files:**
- Create: `src/components/flow/properties/WiresTab.tsx`
- Test: `src/components/flow/properties/__tests__/WiresTab.test.tsx`

**Interfaces:**
- Consumes: Task 1's `incomingRows`, `outgoingGroups`, `WireRow`.
- Produces:

```ts
export function WiresTab(props: {
  node: FlowNode;
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus?: Record<string, FlowNodeStatus>;
  nodeDetail?: Record<string, FlowNodeDetail>;
  onEditWire: (edgeId: string) => void;
  onSelectNode: (nodeId: string) => void;
}): JSX.Element;
```

- [ ] **Step 1: Write the failing tests**

```tsx
// src/components/flow/properties/__tests__/WiresTab.test.tsx
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { WiresTab } from '../WiresTab';

const n = (id: string, kind: FlowNodeKind): FlowNode => ({ id, kind, position: { x: 0, y: 0 } });
const login = n('login', { kind: 'Request', label: 'Login', source: { type: 'Saved', requestPath: 'a.yml' } });
const check = n('check', { kind: 'If', label: 'Ok?', condition: 'true' });
const users = n('users', { kind: 'Request', label: 'List Users', source: { type: 'Saved', requestPath: 'b.yml' } });
const out = n('out', { kind: 'Output', label: 'Result' });
const nodes = [login, check, users, out];
const edges: FlowEdge[] = [
  { id: 'e1', sourceNodeId: 'login', targetNodeId: 'check', targetField: 'input', expression: '' },
  { id: 'e2', sourceNodeId: 'check', targetNodeId: 'users', targetField: 'trigger', expression: '', sourceHandle: 'true' },
  { id: 'e3', sourceNodeId: 'check', targetNodeId: 'out', targetField: 'value', expression: 'response.body', sourceHandle: 'false' },
];

function renderTab(node: FlowNode, extra: Partial<Parameters<typeof WiresTab>[0]> = {}) {
  const onEditWire = vi.fn();
  const onSelectNode = vi.fn();
  render(
    <WiresTab node={node} nodes={nodes} edges={edges} onEditWire={onEditWire} onSelectNode={onSelectNode} {...extra} />,
  );
  return { onEditWire, onSelectNode };
}

describe('WiresTab', () => {
  it('shows the empty state for a node without wires', () => {
    const lonely = n('x', { kind: 'Output', label: 'X' });
    render(<WiresTab node={lonely} nodes={[lonely]} edges={[]} onEditWire={vi.fn()} onSelectNode={vi.fn()} />);
    expect(screen.getByText('No wires. Drag from a dot on the canvas to connect nodes.')).toBeInTheDocument();
  });

  it('lists incoming and outgoing wires', () => {
    renderTab(check);
    expect(within(screen.getByTestId('wires-incoming')).getByText('Login')).toBeInTheDocument();
    const outgoing = screen.getByTestId('wires-outgoing');
    expect(within(outgoing).getByText('true')).toBeInTheDocument();
    expect(within(outgoing).getByText('false')).toBeInTheDocument();
  });

  it('groups outgoing wires by exit', () => {
    renderTab(check);
    const groups = screen.getAllByTestId('wires-exit-group');
    expect(groups.map((g) => g.getAttribute('data-exit'))).toEqual(['true', 'false']);
    expect(within(groups[0]).getByText('List Users')).toBeInTheDocument();
    expect(within(groups[1]).getByText('Result')).toBeInTheDocument();
  });

  it('opens the wire dialog from a row with a script', async () => {
    const { onEditWire } = renderTab(out);
    await userEvent.click(screen.getByRole('button', { name: /Edit wire into Value/ }));
    expect(onEditWire).toHaveBeenCalledWith('e3');
  });

  it('a Run when row is not clickable', () => {
    renderTab(users);
    expect(screen.queryByRole('button', { name: /Edit wire into Run when/ })).not.toBeInTheDocument();
    expect(screen.getByText('(no script)')).toBeInTheDocument();
  });

  it('a node link selects the node without opening the wire', async () => {
    const { onEditWire, onSelectNode } = renderTab(out);
    await userEvent.click(screen.getByRole('button', { name: 'Select node Ok?' }));
    expect(onSelectNode).toHaveBeenCalledWith('check');
    expect(onEditWire).not.toHaveBeenCalled();
  });

  it('fades wires the last run did not take', () => {
    renderTab(check, { nodeStatus: { check: 'success' }, nodeDetail: { check: { branch: 'true' } } });
    const groups = screen.getAllByTestId('wires-exit-group');
    const falseRow = within(groups[1]).getByTestId('wire-row');
    expect(falseRow.className).toContain('opacity-50');
    expect(falseRow).toHaveTextContent('not taken');
    expect(within(groups[0]).getByTestId('wire-row').className).not.toContain('opacity-50');
  });

  it('shows a missing source node', () => {
    const orphan: FlowEdge[] = [{ id: 'e9', sourceNodeId: 'gone', targetNodeId: 'out', targetField: 'value', expression: 'response.body' }];
    render(<WiresTab node={out} nodes={[out]} edges={orphan} onEditWire={vi.fn()} onSelectNode={vi.fn()} />);
    expect(screen.getByText('(missing node)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Edit wire/ })).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/WiresTab.test.tsx`
Expected: FAIL — `Failed to resolve import "../WiresTab"`.

- [ ] **Step 3: Implement the component**

```tsx
// src/components/flow/properties/WiresTab.tsx
import { ArrowLeft, ArrowRight, Pencil } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import type { FlowNodeDetail } from '@/types/pane-types';
import { incomingRows, outgoingGroups, type WireRow } from './wireRows';

interface WiresTabProps {
  node: FlowNode;
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus?: Record<string, FlowNodeStatus>;
  nodeDetail?: Record<string, FlowNodeDetail>;
  onEditWire: (edgeId: string) => void;
  onSelectNode: (nodeId: string) => void;
}

function NodeLink({ row, onSelectNode }: { row: WireRow; onSelectNode: (id: string) => void }) {
  if (row.otherLabel === null) {
    return <span className='italic text-muted-foreground'>(missing node)</span>;
  }
  return (
    <Button
      type='button'
      variant='link'
      className='h-auto p-0 text-xs'
      aria-label={`Select node ${row.otherLabel}`}
      // The row itself may open the wire, so the link must not bubble up.
      onClick={(e) => {
        e.stopPropagation();
        onSelectNode(row.otherNodeId);
      }}
    >
      {row.otherLabel}
    </Button>
  );
}

function Row({
  row,
  direction,
  onEditWire,
  onSelectNode,
}: {
  row: WireRow;
  direction: 'in' | 'out';
  onEditWire: (id: string) => void;
  onSelectNode: (id: string) => void;
}) {
  return (
    <div
      data-testid='wire-row'
      className={cn('flex items-start gap-1.5 rounded-md border px-2 py-1.5', row.notTaken && 'opacity-50')}
    >
      <div className='min-w-0 flex-1 space-y-0.5'>
        <div className='flex flex-wrap items-center gap-1'>
          {direction === 'in' ? (
            <>
              <span className='font-medium'>{row.field}</span>
              <ArrowLeft className='h-3 w-3 text-muted-foreground' aria-hidden='true' />
              <NodeLink row={row} onSelectNode={onSelectNode} />
              {row.exit && <span className='text-muted-foreground'>· {row.exit}</span>}
            </>
          ) : (
            <>
              <ArrowRight className='h-3 w-3 text-muted-foreground' aria-hidden='true' />
              <NodeLink row={row} onSelectNode={onSelectNode} />
              <span className='text-muted-foreground'>· {row.field}</span>
            </>
          )}
          {row.notTaken && <span className='italic text-muted-foreground'>not taken</span>}
        </div>
        <p className='truncate font-mono text-[11px] text-muted-foreground'>
          {row.preview ?? '(no script)'}
        </p>
      </div>
      {row.editable && (
        <Button
          type='button'
          variant='ghost'
          size='icon'
          className='h-5 w-5 shrink-0'
          aria-label={`Edit wire into ${row.field}`}
          title='Edit script'
          onClick={() => onEditWire(row.edgeId)}
        >
          <Pencil className='h-3 w-3' aria-hidden='true' />
        </Button>
      )}
    </div>
  );
}

export function WiresTab({ node, nodes, edges, nodeStatus, nodeDetail, onEditWire, onSelectNode }: WiresTabProps) {
  const incoming = incomingRows(node, nodes, edges, nodeStatus, nodeDetail);
  const outgoing = outgoingGroups(node, nodes, edges, nodeStatus, nodeDetail);

  if (incoming.length === 0 && outgoing.length === 0) {
    return (
      <p className='text-xs text-muted-foreground'>
        No wires. Drag from a dot on the canvas to connect nodes.
      </p>
    );
  }

  return (
    <div className='space-y-3 text-xs'>
      {incoming.length > 0 && (
        <section data-testid='wires-incoming' className='space-y-1.5'>
          <h4 className='font-medium'>Incoming</h4>
          {incoming.map((row) => (
            <Row key={row.edgeId} row={row} direction='in' onEditWire={onEditWire} onSelectNode={onSelectNode} />
          ))}
        </section>
      )}
      {outgoing.length > 0 && (
        <section data-testid='wires-outgoing' className='space-y-2'>
          <h4 className='font-medium'>Outgoing</h4>
          {outgoing.map((group) => (
            <div
              key={group.exit ?? 'result'}
              data-testid='wires-exit-group'
              data-exit={group.exit ?? 'result'}
              className='space-y-1.5'
            >
              {group.exit && <p className='text-muted-foreground'>{group.exit}</p>}
              {group.rows.map((row) => (
                <Row key={row.edgeId} row={row} direction='out' onEditWire={onEditWire} onSelectNode={onSelectNode} />
              ))}
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
```

The edit action is a separate icon button on the row, not the whole row, so the node link and the edit action never compete for one click.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow/properties/__tests__/WiresTab.test.tsx`
Expected: PASS.

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: clean.

- [ ] **Step 6: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): add the Wires tab`.

---

### Task 3: Mount the Wires tab and pass run state from FlowPane

**Files:**
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (add the Wires trigger and content; two new optional props)
- Modify: `src/components/flow/FlowPane.tsx` (pass `nodeStatus` and `nodeDetail`)
- Modify: `docs/superpowers/plans/flow-properties-tabs/00-index.md` (record the two props)
- Test: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx` (extend), `src/components/flow/__tests__/FlowPane.properties.test.tsx` (extend)

**Interfaces:**
- Consumes: `WiresTab` (Task 2); plan 02's panel props `nodes`, `edges`, `onEditWire`, `onSelectNode`, `activeTab`, `onTabChange`.
- Produces: `NodePropertiesPanel` gains `nodeStatus?: Record<string, FlowNodeStatus>` and `nodeDetail?: Record<string, FlowNodeDetail>`.

- [ ] **Step 1: Write the failing tests**

Append to `NodePropertiesPanel.test.tsx` a case that renders the panel on the Wires tab. Use the props plan 02 introduced for every other required prop (reuse the file's `renderPanel` helper after plan 02 has extended it; add `activeTab='wires'`, `nodes`, `edges`, `onEditWire`, `onSelectNode`):

```tsx
describe('Wires tab in the panel', () => {
  it('lists the wires of the selected node and opens one', async () => {
    const outNode = node('out', { kind: 'Output', label: 'Result' });
    const src = node('src', { kind: 'Input', label: 'Token', value: 'x' });
    const onEditWire = vi.fn();
    render(
      <NodePropertiesPanel
        node={outNode}
        nodes={[src, outNode]}
        edges={[{ id: 'w1', sourceNodeId: 'src', targetNodeId: 'out', targetField: 'value', expression: 'response.body' }]}
        collection='demo'
        status='idle'
        activeTab='wires'
        onTabChange={vi.fn()}
        onEditWire={onEditWire}
        onSelectNode={vi.fn()}
        onChange={vi.fn()}
        onClose={vi.fn()}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByText('Token')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Edit wire into Value/ }));
    expect(onEditWire).toHaveBeenCalledWith('w1');
  });
});
```

In `src/components/flow/__tests__/FlowPane.properties.test.tsx`, add a test that runs through `FlowPane`: select an If node whose tab `nodeStatus` is `success` and `nodeDetail` has `branch: 'true'`, switch to the Wires tab (click the `Wires` tab trigger), and assert the row wired from the `false` exit has `opacity-50` and the text `not taken`. Build the flow tab fixture the same way the file's existing tests do (read the file first and reuse its helpers and store setup).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx src/components/flow/__tests__/FlowPane.properties.test.tsx`
Expected: FAIL — there is no Wires tab yet (`Unable to find an accessible element with the role "tab" and name "Wires"`, or the row text `Token` is not found), and the FlowPane test finds no `not taken` row.

- [ ] **Step 3: Mount the tab and pass run state**

In `NodePropertiesPanel.tsx`:

1. Import `WiresTab` and the `FlowNodeStatus` / `FlowNodeDetail` types.
2. Add to the props type and destructuring:

```tsx
  /** Every node's last-run status, so the Wires tab can fade wires not taken. */
  nodeStatus?: Record<string, FlowNodeStatus>;
  /** Every node's last-run detail; the Wires tab reads each routing node's branch. */
  nodeDetail?: Record<string, FlowNodeDetail>;
```

3. Add a Wires trigger after the Last run trigger (`<TabsTrigger value='wires' className='text-xs'>Wires</TabsTrigger>`) and a content block after the Last run content, `<TabsContent value='wires' className='min-h-0 flex-1 overflow-y-auto p-3'>`, whose children are exactly:

```tsx
<WiresTab
  node={node}
  nodes={nodes}
  edges={edges}
  nodeStatus={nodeStatus}
  nodeDetail={nodeDetail}
  onEditWire={onEditWire}
  onSelectNode={onSelectNode}
/>
```

In `FlowPane.tsx`, where `<NodePropertiesPanel` is rendered, add:

```tsx
              nodeStatus={tab.nodeStatus}
              nodeDetail={tab.nodeDetail}
```

`00-index.md` already records these two props as optional, added by plan 04 (reconciliation).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow`
Expected: PASS — the new tests and the rest of the flow suite.

- [ ] **Step 5: Check types and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: clean.

- [ ] **Step 6: Manual check (skip if you cannot run the desktop app, and say so in your report)**

Run `yarn tauri dev`, open a flow with an If node wired to two nodes, run it, select the If, open the Wires tab: the exit not taken is faded with "not taken"; clicking a node name selects that node on the canvas and the panel stays on Wires; clicking the pencil on a wire opens "Value from source".

- [ ] **Step 7: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): show the Wires tab in the panel`.
