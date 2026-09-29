# Flow Async P2 — Plan 09: Wait for Callback Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users add, wire, edit and watch Wait for callback nodes on the canvas, copy their `{{callback.<name>}}` variable, and set the flow's callback host.

**Architecture:** The TS types mirror the plan 07 DTOs. A small `src/lib/flow-callback.ts` holds naming and default helpers. `flow-wiring.ts` learns the node's single `trigger` input and `result` exit. A new `WaitForCallbackNode` card and `WaitForCallbackEditor` panel follow the existing Request/If patterns. `FlowTab` carries `callbackHost`, loaded by `openFlowTab`, edited by a small popover in the canvas toolbar area, and sent by `handleSave` only when set.

**Tech Stack:** React + TypeScript, `@xyflow/react`, shadcn/ui (`Button`, `Input`, `Label`, `Popover`, `DropdownMenu`), `lucide-react`, `SingleLineEditor` (CodeMirror), Zustand, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §7.5. Interfaces are locked in `docs/superpowers/plans/flow-async/00-index.md` ("P2 — IPC and TS").

## Global Constraints

- shadcn/ui primitives only (no raw `<button>`, `<input>`, `<select>`, `<form>`); icons from `lucide-react` only.
- Single-line, variable-aware fields (the `accept_when` condition) use `SingleLineEditor`; never Monaco.
- Zustand: narrow selectors; never destructure the whole store at a component's top level.
- New node names are `callback`, `callback_2`, `callback_3`, … (first free one). Names match `^[A-Za-z0-9_]+$`.
- Default timeout `60000` ms; the editor shows and edits seconds (1–3600).
- The card shows `Wait for callback · <name> · <timeout>s`, the variable `{{callback.<name>}}` with a copy button, and after a run `✓ received <METHOD> · <secs>s` or the failure.
- The editor notes that the URL is reachable from the local network while a run is active.
- Callback host field placeholder: `auto (LAN IP)`, hint mentions `host.docker.internal`.
- A flow without a callback host saves exactly as before (no `callbackHost` key sent).
- Checks: `yarn test src/components/flow src/lib src/stores`, `yarn tsc --noEmit`, `yarn check`.

## Review Focus

1. Adding a second Wait node must not reuse `callback`; it must become `callback_2` (and a third `callback_3`), even after the first is renamed back and forth. → Task 1 test `nextCallbackName picks the first free name`.
2. A user typing `pay ment` as the name must see a hint before Save rejects it. → Task 2 test `shows a hint for an invalid name`.
3. Clearing the accept_when field must send `acceptWhen: null`, not `''` (the backend rejects a blank condition). → Task 2 test `clearing accept_when stores null`.
4. Dragging a data wire (URL/body) into a Wait node must be refused on the canvas, not only at Save. → Task 1 test `a Wait node accepts only a Run when wire`.
5. An existing flow without `callbackHost` must save with exactly `{ name, nodes, edges }`. → Task 3 test `saves exactly name, nodes and edges when no callback host is set`.

---

### Task 1: Types, helpers, wiring rules and palette entry

**Files:**
- Modify: `src/lib/tauri-api.ts` (`FlowNodeKind` union, `Flow`)
- Create: `src/lib/flow-callback.ts`
- Create: `src/lib/__tests__/flow-callback.test.ts`
- Modify: `src/lib/flow-wiring.ts` (`sourceHandleExists`, `targetAccepts`)
- Modify: `src/lib/__tests__/flow-wiring.test.ts`
- Modify: `src/components/flow/NodePalette.tsx`
- Modify: `src/components/flow/__tests__/NodePalette.test.tsx`
- Modify: `src/components/flow/FlowPane.tsx:396` (pass `nodes` to the palette)
- Modify: `src/components/flow/__tests__/flowExits.test.ts`

**Interfaces:**
- Consumes: DTO JSON from plan 07 (`timeoutMs`, `acceptWhen`, `callbackHost`).
- Produces:

```ts
// src/lib/tauri-api.ts
| { kind: 'WaitForCallback'; label: string; name: string; timeoutMs: number; acceptWhen?: string | null }
// Flow gains: callbackHost?: string | null

// src/lib/flow-callback.ts
export const DEFAULT_CALLBACK_TIMEOUT_MS = 60_000;
export const MIN_CALLBACK_TIMEOUT_MS = 1_000;
export const MAX_CALLBACK_TIMEOUT_MS = 3_600_000;
export function isValidCallbackName(name: string): boolean;
export function callbackVariable(name: string): string; // `{{callback.<name>}}`
export function nextCallbackName(nodes: FlowNode[]): string;

// NodePalette gains an optional prop: nodes?: FlowNode[]
```

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-callback.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { callbackVariable, isValidCallbackName, nextCallbackName } from '../flow-callback';

const waitNode = (id: string, name: string): FlowNode => ({
  id,
  kind: { kind: 'WaitForCallback', label: id, name, timeoutMs: 60000 },
  position: { x: 0, y: 0 },
});

describe('flow-callback helpers', () => {
  it('accepts letters, digits and underscores only', () => {
    expect(isValidCallbackName('payment_2')).toBe(true);
    for (const bad of ['', 'pay ment', 'pay-ment', 'a.b', 'päy']) {
      expect(isValidCallbackName(bad)).toBe(false);
    }
  });

  it('builds the run-scoped variable', () => {
    expect(callbackVariable('payment')).toBe('{{callback.payment}}');
  });

  it('nextCallbackName picks the first free name', () => {
    expect(nextCallbackName([])).toBe('callback');
    expect(nextCallbackName([waitNode('a', 'callback')])).toBe('callback_2');
    expect(nextCallbackName([waitNode('a', 'callback'), waitNode('b', 'callback_2')])).toBe(
      'callback_3',
    );
    expect(nextCallbackName([waitNode('a', 'callback_2')])).toBe('callback');
  });
});
```

Append to `src/lib/__tests__/flow-wiring.test.ts`:

```ts
describe('Wait for callback wiring', () => {
  const wait: FlowNode = {
    id: 'wait',
    kind: { kind: 'WaitForCallback', label: 'Hook', name: 'payment', timeoutMs: 60000 },
    position: { x: 0, y: 0 },
  };
  const nodes = [requestSource, wait];

  it('a Wait node accepts only a Run when wire', () => {
    const into = (targetHandle: string) =>
      isValidFlowConnection(
        { source: 'node-a', target: 'wait', sourceHandle: 'result', targetHandle },
        nodes,
        [],
      );
    expect(into('trigger')).toBe(true);
    expect(into('url')).toBe(false);
    expect(into('body')).toBe(false);
    expect(into('value')).toBe(false);
    expect(into('input')).toBe(false);
  });

  it('a Wait node exits through result only', () => {
    const from = (sourceHandle: string) =>
      isValidFlowConnection(
        { source: 'wait', target: 'node-a', sourceHandle, targetHandle: 'url' },
        nodes,
        [],
      );
    expect(from('result')).toBe(true);
    expect(from('true')).toBe(false);
  });
});
```

Append to `src/components/flow/__tests__/NodePalette.test.tsx`:

```ts
describe('NodePalette Wait for callback entry', () => {
  it('adds a Wait for callback node with the first free name', async () => {
    const onAddNode = vi.fn();
    render(
      <NodePalette
        onAddNode={onAddNode}
        nodes={[
          {
            id: 'w1',
            kind: { kind: 'WaitForCallback', label: 'Hook', name: 'callback', timeoutMs: 60000 },
            position: { x: 0, y: 0 },
          },
        ]}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Wait for callback' }));
    expect(onAddNode).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^wait-/),
        position: { x: 100, y: 100 },
        kind: {
          kind: 'WaitForCallback',
          label: 'Wait for callback',
          name: 'callback_2',
          timeoutMs: 60000,
        },
      }),
    );
  });
});
```

Append to `src/components/flow/__tests__/flowExits.test.ts` (it already imports `exitLabel` and `edgeRunState`; add any missing import):

```ts
describe('Wait for callback exits', () => {
  const wait = {
    id: 'w',
    kind: { kind: 'WaitForCallback' as const, label: 'Hook', name: 'payment', timeoutMs: 60000 },
    position: { x: 0, y: 0 },
  };

  it('has no exit label and neutral edges, like a Request node', () => {
    expect(exitLabel(wait.kind, 'result')).toBeUndefined();
    expect(
      edgeRunState(
        { id: 'e', sourceNodeId: 'w', targetNodeId: 'x', targetField: 'url', expression: '' },
        wait,
        'success',
        undefined,
      ),
    ).toBe('neutral');
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/lib/__tests__/flow-callback.test.ts src/lib/__tests__/flow-wiring.test.ts src/components/flow/__tests__/NodePalette.test.tsx src/components/flow/__tests__/flowExits.test.ts`
Expected: FAIL — `Failed to resolve import "../flow-callback"`, TS errors on the unknown `'WaitForCallback'` kind, and no "Wait for callback" menu item.

- [ ] **Step 3: Implement the types and helpers**

In `src/lib/tauri-api.ts`, extend the `FlowNodeKind` union (keep plan 03's `repeatUntil` on `Request`):

```ts
  | { kind: 'Switch'; label: string; value: string; cases: SwitchCase[] }
  | {
      kind: 'WaitForCallback';
      label: string;
      /** Letters, digits and `_`; unique in the flow. Used as `{{callback.<name>}}`. */
      name: string;
      timeoutMs: number;
      /** Optional condition over `request`. Null or absent accepts the first call. */
      acceptWhen?: string | null;
    };
```

and `Flow`:

```ts
export interface Flow {
  name: string;
  nodes: FlowNode[];
  edges: FlowEdge[];
  /** Host used in callback URLs. Absent or null means this machine's LAN IP. */
  callbackHost?: string | null;
}
```

Create `src/lib/flow-callback.ts`:

```ts
import type { FlowNode } from '@/lib/tauri-api';

// These values must match rocket_flow's CALLBACK_* constants.
export const DEFAULT_CALLBACK_TIMEOUT_MS = 60_000;
export const MIN_CALLBACK_TIMEOUT_MS = 1_000;
export const MAX_CALLBACK_TIMEOUT_MS = 3_600_000;

const NAME_PATTERN = /^[A-Za-z0-9_]+$/;

export function isValidCallbackName(name: string): boolean {
  return NAME_PATTERN.test(name);
}

/** The run-scoped variable a request uses to send this node's URL. */
export function callbackVariable(name: string): string {
  return `{{callback.${name}}}`;
}

/** `callback`, then `callback_2`, `callback_3`, … whichever is free first. */
export function nextCallbackName(nodes: FlowNode[]): string {
  const taken = new Set(
    nodes.flatMap((n) => (n.kind.kind === 'WaitForCallback' ? [n.kind.name] : [])),
  );
  if (!taken.has('callback')) return 'callback';
  let i = 2;
  while (taken.has(`callback_${i}`)) i += 1;
  return `callback_${i}`;
}
```

- [ ] **Step 4: Implement wiring and the palette entry**

In `src/lib/flow-wiring.ts`, `sourceHandleExists`:

```ts
    case 'Request':
    case 'Input':
    case 'WaitForCallback':
      return handle === RESULT_HANDLE;
```

and `targetAccepts`:

```ts
    case 'WaitForCallback':
      return handle === TRIGGER_HANDLE;
```

Update the comment above `isValidFlowConnection` from "rules V1–V5" to "rules V1–V5 and V10".

In `src/components/flow/NodePalette.tsx`, add `Hourglass` to the `lucide-react` import and import the helpers:

```ts
import { DEFAULT_CALLBACK_TIMEOUT_MS, nextCallbackName } from '@/lib/flow-callback';
```

Change the signature:

```tsx
export function NodePalette({
  onAddNode,
  nodes = [],
}: {
  onAddNode: (node: FlowNode) => void;
  // Existing nodes, so a new Wait for callback node gets a free name.
  nodes?: FlowNode[];
}) {
```

Add after the "Switch" item:

```tsx
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('wait'),
                kind: {
                  kind: 'WaitForCallback',
                  label: 'Wait for callback',
                  name: nextCallbackName(nodes),
                  timeoutMs: DEFAULT_CALLBACK_TIMEOUT_MS,
                },
                position: defaultPosition,
              })
            }
          >
            <Hourglass className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Wait for callback
          </DropdownMenuItem>
```

In `src/components/flow/FlowPane.tsx`, pass the nodes:

```tsx
          <NodePalette onAddNode={handleAddNode} nodes={tab.nodes} />
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test src/lib/__tests__/flow-callback.test.ts src/lib/__tests__/flow-wiring.test.ts src/components/flow/__tests__/NodePalette.test.tsx src/components/flow/__tests__/flowExits.test.ts`
Expected: PASS.

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add Wait for callback types and palette entry`.

---

### Task 2: `WaitForCallbackNode` card and `WaitForCallbackEditor`

**Files:**
- Create: `src/components/flow/nodes/WaitForCallbackNode.tsx`
- Create: `src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx:32-38` (`nodeTypes`)
- Create: `src/components/flow/properties/WaitForCallbackEditor.tsx`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (`editorFor`)
- Modify: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`

**Interfaces:**
- Consumes: Task 1 types and helpers; `NodeStatusCaption`'s `progress` prop and `FlowNodeDetail.progress` (plan 02); `FlowNodeDetail.value` / `durationMs` (existing) — for a succeeded Wait node `value` is the received method (plan 08).
- Produces: `WaitForCallbackNode` registered as `nodeTypes.WaitForCallback`; `WaitForCallbackEditor({ kind, onChange })`.

- [ ] **Step 1: Write the failing card tests**

Create `src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { WaitForCallbackNode, type WaitForCallbackNodeData } from '../WaitForCallbackNode';

const props = {
  selected: false,
  type: 'WaitForCallback',
  dragging: false,
  zIndex: 0,
  isConnectable: true,
  draggable: true,
  selectable: true,
  deletable: true,
  positionAbsoluteX: 0,
  positionAbsoluteY: 0,
};

function renderNode(data: Partial<WaitForCallbackNodeData> = {}) {
  return render(
    <ReactFlowProvider>
      <WaitForCallbackNode
        {...props}
        id='w1'
        data={{
          kind: { kind: 'WaitForCallback', label: 'Payment done', name: 'payment', timeoutMs: 60000 },
          status: 'idle',
          ...data,
        }}
      />
    </ReactFlowProvider>,
  );
}

describe('WaitForCallbackNode', () => {
  it('has a Run when input and a result exit, and shows name, timeout and variable', () => {
    renderNode();
    const card = screen.getByTestId('wait-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    const sources = [...card.querySelectorAll('.react-flow__handle.source')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['trigger']);
    expect(sources).toEqual(['result']);
    expect(screen.getByText('Payment done')).toBeInTheDocument();
    expect(card).toHaveTextContent('Wait for callback · payment · 60s');
    expect(screen.getByTestId('wait-node-variable')).toHaveTextContent('{{callback.payment}}');
  });

  it('copies the variable', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    renderNode();
    fireEvent.click(screen.getByRole('button', { name: 'Copy variable' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('{{callback.payment}}'));
  });

  it('shows the received method and time after a success', () => {
    renderNode({ status: 'success', value: 'POST', durationMs: 3100 });
    expect(screen.getByTestId('wait-node-result')).toHaveTextContent('✓ received POST · 3.1s');
  });

  it('shows progress while waiting', () => {
    renderNode({ status: 'running', progress: 'waiting… 42s left · 1 ignored call(s)' });
    expect(screen.getByTestId('node-progress')).toHaveTextContent('waiting… 42s left');
  });

  it('shows the error of a failed wait', () => {
    renderNode({ status: 'failed', error: 'Invalid input: no matching callback within 60s (0 ignored)' });
    expect(screen.getByTestId('node-error')).toHaveTextContent('no matching callback within 60s');
  });
});
```

- [ ] **Step 2: Write the failing editor tests**

Append to `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`:

```tsx
describe('Wait for callback editor', () => {
  const waitKind = {
    kind: 'WaitForCallback' as const,
    label: 'Hook',
    name: 'payment',
    timeoutMs: 60000,
    acceptWhen: "request.body.event === 'done'",
  };

  it('edits the name', async () => {
    const { onChange } = renderPanel(node('w', waitKind));
    await userEvent.type(screen.getByLabelText('Name'), '_2');
    expect(onChange).toHaveBeenLastCalledWith({ ...waitKind, name: 'payment2' });
  });

  it('shows a hint for an invalid name', () => {
    renderPanel(node('w', { ...waitKind, name: 'pay ment' }));
    expect(screen.getByTestId('callback-name-hint')).toHaveTextContent(
      'Use letters, digits and _ only.',
    );
  });

  it('edits the timeout in seconds', async () => {
    const { onChange } = renderPanel(node('w', waitKind));
    // The field shows 60; typing 0 makes it 600 seconds.
    await userEvent.type(screen.getByLabelText('Timeout (seconds)'), '0');
    expect(onChange).toHaveBeenLastCalledWith({ ...waitKind, timeoutMs: 600000 });
  });

  it('clearing accept_when stores null', async () => {
    const { onChange } = renderPanel(node('w', waitKind));
    await userEvent.clear(screen.getByLabelText('Accept when'));
    expect(onChange).toHaveBeenLastCalledWith({ ...waitKind, acceptWhen: null });
  });

  it('shows the variable and the local network note', () => {
    renderPanel(node('w', waitKind));
    expect(screen.getByText('{{callback.payment}}')).toBeInTheDocument();
    expect(screen.getByText(/reachable from your local network/)).toBeInTheDocument();
  });
});
```

`renderPanel` already returns `{ onChange, onClose, onDelete }`. The panel is not re-rendered with the new kind in these tests, so each keystroke is applied to the original value (the existing label test relies on the same behaviour).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `yarn test src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
Expected: FAIL — `Failed to resolve import "../WaitForCallbackNode"` and no "Name" field in the panel.

- [ ] **Step 4: Implement the card**

Create `src/components/flow/nodes/WaitForCallbackNode.tsx`:

```tsx
import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Check, Copy, Hourglass } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { callbackVariable } from '@/lib/flow-callback';
import { RESULT_HANDLE, TRIGGER_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export interface WaitForCallbackNodeData {
  kind: Extract<FlowNodeKind, { kind: 'WaitForCallback' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  error?: string;
  /** The received method (e.g. `POST`) after a success. */
  value?: string;
  durationMs?: number;
  /** Live text while waiting, such as "waiting… 42s left". */
  progress?: string;
  /** Set when this node is named in a save validation error, such as a cycle. */
  hasCycleError?: boolean;
}

const COPIED_MS = 1500;

export function WaitForCallbackNode({
  id,
  data,
  isConnectable,
}: NodeProps & { data: WaitForCallbackNodeData }) {
  const { kind, status } = data;
  const variable = callbackVariable(kind.name);
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Clear the pending reset so it cannot fire after unmount.
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = () => {
    navigator.clipboard.writeText(variable).then(
      () => {
        setCopied(true);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => setCopied(false), COPIED_MS);
      },
      (err) => console.warn('Copy failed', err),
    );
  };

  const seconds = (ms: number) => `${Math.round(ms / 100) / 10}s`;

  return (
    <div
      data-testid='wait-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <Hourglass className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      {status === 'success' && (
        <div data-testid='wait-node-result' className='px-2 pt-1 text-green-600'>
          ✓ received {data.value ?? 'call'}
          {data.durationMs !== undefined ? ` · ${seconds(data.durationMs)}` : ''}
        </div>
      )}
      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
      />

      <div className='relative space-y-1 px-2 py-1.5'>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id={TRIGGER_HANDLE}
            title='Run when'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Run when</span>
        </div>
        <div className='text-muted-foreground'>
          Wait for callback · {kind.name} · {Math.round(kind.timeoutMs / 1000)}s
        </div>
        <div className='nodrag nokey flex items-center gap-1'>
          <code data-testid='wait-node-variable' className='truncate font-mono text-[11px]'>
            {variable}
          </code>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            className='h-5 w-5 shrink-0'
            aria-label='Copy variable'
            title='Copy variable'
            onClick={copy}
          >
            {copied ? <Check className='h-3 w-3' /> : <Copy className='h-3 w-3' />}
          </Button>
        </div>
      </div>

      <Handle
        type='source'
        id={RESULT_HANDLE}
        position={Position.Right}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
    </div>
  );
}
```

`3100` ms renders as `3.1s` (`Math.round(31) / 10`).

Register it in `src/components/flow/FlowCanvas.tsx`:

```ts
import { WaitForCallbackNode } from './nodes/WaitForCallbackNode';

const nodeTypes = {
  Request: RequestNode,
  Input: InputNode,
  Output: OutputNode,
  If: IfNode,
  Switch: SwitchNode,
  WaitForCallback: WaitForCallbackNode,
};
```

- [ ] **Step 5: Implement the editor**

Create `src/components/flow/properties/WaitForCallbackEditor.tsx`:

```tsx
import { SingleLineEditor } from '@/components/editor';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { callbackVariable, isValidCallbackName } from '@/lib/flow-callback';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { LabelField } from './LabelField';

type WaitKind = Extract<FlowNodeKind, { kind: 'WaitForCallback' }>;

export function WaitForCallbackEditor({
  kind,
  onChange,
}: {
  kind: WaitKind;
  onChange: (kind: FlowNodeKind) => void;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />

      <div className='space-y-1'>
        <Label htmlFor='flow-callback-name' className='text-xs'>
          Name
        </Label>
        <Input
          id='flow-callback-name'
          value={kind.name}
          onChange={(e) => onChange({ ...kind, name: e.target.value })}
          className='h-8 font-mono text-xs'
        />
        {!isValidCallbackName(kind.name) && (
          <p data-testid='callback-name-hint' className='text-xs text-red-600'>
            Use letters, digits and _ only.
          </p>
        )}
        <p className='text-xs text-muted-foreground'>
          Send this URL in an earlier request as{' '}
          <code className='font-mono'>{callbackVariable(kind.name)}</code>
        </p>
      </div>

      <div className='space-y-1'>
        <Label htmlFor='flow-callback-timeout' className='text-xs'>
          Timeout (seconds)
        </Label>
        <Input
          id='flow-callback-timeout'
          type='number'
          min={1}
          max={3600}
          value={Math.round(kind.timeoutMs / 1000)}
          onChange={(e) => {
            const seconds = Number(e.target.value);
            if (Number.isFinite(seconds) && seconds > 0) {
              onChange({ ...kind, timeoutMs: Math.round(seconds * 1000) });
            }
          }}
          className='h-8 text-xs'
        />
      </div>

      <div className='space-y-1'>
        <span className='text-xs font-medium'>Accept when</span>
        <SingleLineEditor
          aria-label='Accept when'
          value={kind.acceptWhen ?? ''}
          onChange={(value) => onChange({ ...kind, acceptWhen: value.trim() ? value : null })}
          placeholder="request.body.event === 'payment.completed'"
        />
        <p className='text-xs text-muted-foreground'>
          Optional. <code className='font-mono'>request</code> has method, path, query, headers and
          body. Calls that do not match are answered and ignored. Empty accepts the first call.
        </p>
      </div>

      <p className='text-xs text-muted-foreground'>
        While a run is active, the callback URL is reachable from your local network.
      </p>
    </div>
  );
}
```

In `src/components/flow/properties/NodePropertiesPanel.tsx`, import it and add a case to `editorFor`:

```tsx
import { WaitForCallbackEditor } from './WaitForCallbackEditor';
```

```tsx
    case 'WaitForCallback':
      return <WaitForCallbackEditor kind={kind} onChange={onChange} />;
```

An emptied field gives `Number('') === 0`, which the `seconds > 0` guard ignores, so clearing the field never sends a zero timeout.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `yarn test src/components/flow/nodes/__tests__/WaitForCallbackNode.test.tsx src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 7: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add Wait for callback node card and editor`.

---

### Task 3: Callback host setting

**Files:**
- Modify: `src/types/pane-types.ts` (`FlowTab`)
- Modify: `src/stores/pane-store.ts` (`openFlowTab`, new `setFlowCallbackHost`)
- Modify: `src/stores/__tests__/pane-store.test.ts`
- Create: `src/components/flow/CallbackHostSetting.tsx`
- Create: `src/components/flow/__tests__/CallbackHostSetting.test.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (render the setting, `handleSave`)
- Modify: `src/components/flow/__tests__/FlowPane.test.tsx`

**Interfaces:**
- Consumes: `Flow.callbackHost` (Task 1).
- Produces: `FlowTab.callbackHost?: string | null`; store action `setFlowCallbackHost(tabId: string, host: string | null) => void`; component `CallbackHostSetting({ value, onChange })`.

- [ ] **Step 1: Write the failing tests**

Append to the `Flow tab actions` describe in `src/stores/__tests__/pane-store.test.ts`:

```ts
  it('openFlowTab loads the callback host', async () => {
    vi.mocked(getFlow).mockResolvedValue({
      name: 'My Flow',
      nodes: [],
      edges: [],
      callbackHost: 'host.docker.internal',
    });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    expect(findFirstFlowTab()?.callbackHost).toBe('host.docker.internal');
  });

  it('setFlowCallbackHost stores the host and marks the tab dirty', async () => {
    vi.mocked(getFlow).mockResolvedValue({ name: 'My Flow', nodes: [], edges: [] });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    const tabId = findFirstFlowTab()?.id ?? '';

    usePaneStore.getState().setFlowCallbackHost(tabId, '10.0.0.5');

    const tab = findFirstFlowTab();
    expect(tab?.callbackHost).toBe('10.0.0.5');
    expect(tab?.isDirty).toBe(true);
  });
```

Create `src/components/flow/__tests__/CallbackHostSetting.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { CallbackHostSetting } from '../CallbackHostSetting';

// The setting is controlled, so the test holds the value like FlowPane does.
function Harness({ onChange }: { onChange: (v: string | null) => void }) {
  const [value, setValue] = useState<string | null>('10.0.0.5');
  return (
    <CallbackHostSetting
      value={value}
      onChange={(v) => {
        setValue(v);
        onChange(v);
      }}
    />
  );
}

describe('CallbackHostSetting', () => {
  it('edits the host and clears it to null', async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onChange={onChange} />);

    await user.click(screen.getByRole('button', { name: 'Callback host' }));
    const field = screen.getByRole('textbox', { name: 'Callback host' });
    expect(field).toHaveValue('10.0.0.5');
    expect(field).toHaveAttribute('placeholder', 'auto (LAN IP)');
    expect(screen.getByText(/host\.docker\.internal/)).toBeInTheDocument();

    await user.clear(field);
    expect(onChange).toHaveBeenLastCalledWith(null);
    await user.type(field, 'host.docker.internal');
    expect(onChange).toHaveBeenLastCalledWith('host.docker.internal');
  });
});
```

Append to the `FlowPane save` describe in `src/components/flow/__tests__/FlowPane.test.tsx`:

```tsx
  it('saves callbackHost when it is set', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const withHost: FlowTab = { ...flowTab, id: 'flow-host', callbackHost: 'host.docker.internal' };
    usePaneStore.getState().openTab(withHost);
    render(<FlowPane tab={withHost} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith(
        'demo',
        expect.objectContaining({ callbackHost: 'host.docker.internal' }),
      ),
    );
  });

  it('saves exactly name, nodes and edges when no callback host is set', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(vi.mocked(saveFlow).mock.calls[0][1]).toEqual({
      name: 'my-flow',
      nodes: flowTab.nodes,
      edges: [],
    });
  });

  it('shows the callback host setting only when the flow has a Wait node', () => {
    const { unmount } = render(
      <FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />,
    );
    expect(screen.queryByRole('button', { name: 'Callback host' })).toBeNull();
    unmount();

    const withWait: FlowTab = {
      ...flowTab,
      id: 'flow-wait',
      nodes: [
        ...flowTab.nodes,
        {
          id: 'w',
          kind: { kind: 'WaitForCallback', label: 'Hook', name: 'payment', timeoutMs: 60000 },
          position: { x: 0, y: 0 },
        },
      ],
    };
    usePaneStore.getState().openTab(withWait);
    render(<FlowPane tab={withWait} groupId={usePaneStore.getState().activeGroupId} />);
    expect(screen.getByRole('button', { name: 'Callback host' })).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/stores/__tests__/pane-store.test.ts src/components/flow/__tests__/CallbackHostSetting.test.tsx src/components/flow/__tests__/FlowPane.test.tsx`
Expected: FAIL — `setFlowCallbackHost is not a function`, `Failed to resolve import "../CallbackHostSetting"`, `callbackHost` missing from the `saveFlow` payload.

- [ ] **Step 3: Implement**

In `src/types/pane-types.ts`, add to `FlowTab` after `edges`:

```ts
  /** Host used in callback URLs. Null or absent means this machine's LAN IP. */
  callbackHost?: string | null;
```

In `src/stores/pane-store.ts`, declare the action next to `updateFlowGraph` in `PaneState`:

```ts
  setFlowCallbackHost: (tabId: string, host: string | null) => void;
```

In `openFlowTab`, keep the loaded host:

```ts
    let callbackHost: string | null = null;
    // …inside the successful getFlow branch, next to nodes/edges:
        callbackHost = flow.callbackHost ?? null;
```

and add `callbackHost,` to the `tab: FlowTab` literal. Add the action after `updateFlowGraph`:

```ts
  setFlowCallbackHost(tabId, host) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, callbackHost: host, isDirty: true } : tab,
      ),
    });
  },
```

Create `src/components/flow/CallbackHostSetting.tsx`:

```tsx
import { Network } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

// The host a flow's callback URLs use. Empty means auto-detect the LAN IP.
export function CallbackHostSetting({
  value,
  onChange,
}: {
  value: string | null | undefined;
  onChange: (host: string | null) => void;
}) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size='sm' variant='outline' aria-label='Callback host' title='Callback host'>
          <Network className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      </PopoverTrigger>
      <PopoverContent align='end' className='nokey w-72 space-y-2'>
        <Label htmlFor='flow-callback-host' className='text-xs'>
          Callback host
        </Label>
        <Input
          id='flow-callback-host'
          value={value ?? ''}
          placeholder='auto (LAN IP)'
          onChange={(e) => {
            const host = e.target.value.trim();
            onChange(host ? host : null);
          }}
          className='h-8 text-xs'
        />
        <p className='text-xs text-muted-foreground'>
          Used in <code className='font-mono'>{'{{callback.*}}'}</code> URLs. Use{' '}
          <code className='font-mono'>host.docker.internal</code> when the caller runs in Docker.
        </p>
      </PopoverContent>
    </Popover>
  );
}
```

In `src/components/flow/FlowPane.tsx`, add a narrow selector next to `updateFlowGraph`:

```ts
  const setFlowCallbackHost = usePaneStore((s) => s.setFlowCallbackHost);
```

import the component:

```ts
import { CallbackHostSetting } from './CallbackHostSetting';
```

render it before `<FlowToolbar` inside the top-right toolbar `div`, only when the flow has a Wait node:

```tsx
            {tab.nodes.some((n) => n.kind.kind === 'WaitForCallback') && (
              <CallbackHostSetting
                value={tab.callbackHost}
                onChange={(host) => setFlowCallbackHost(tab.id, host)}
              />
            )}
```

and send the host from `handleSave` only when set, so existing flows keep the exact old payload:

```ts
      await saveFlow(collectionName, {
        name: flowName,
        nodes: tab.nodes,
        edges: tab.edges,
        ...(tab.callbackHost ? { callbackHost: tab.callbackHost } : {}),
      });
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/stores/__tests__/pane-store.test.ts src/components/flow/__tests__/CallbackHostSetting.test.tsx src/components/flow/__tests__/FlowPane.test.tsx`
Expected: PASS.

Run: `yarn test src/components/flow src/lib src/stores`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 5: Manual check (P2 acceptance)**

1. `yarn tauri dev`. Open a flow, add "Wait for callback" from the palette; the card shows `{{callback.callback}}`.
2. Add a Request whose body contains `{"callbackUrl":"{{callback.callback}}"}`, wire its result into the Wait node's Run when, and the Wait node's result into an Output (`response.body`).
3. In a terminal, run a tiny local server that reads `callbackUrl` from the request body and POSTs `{"orderId":42}` to it after 2 seconds (any language; for example a 10-line Python `http.server` handler).
4. Run the flow. The Wait card shows `waiting… Ns left`, then `✓ received POST · ~2s`, and the Output shows `{"orderId":42}`.
5. Run again and press Stop while it waits: the run stops within a second.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add callback host setting`.
