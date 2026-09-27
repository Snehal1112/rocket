# Flow Plan 09: Canvas + Node Components — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `FlowPane`'s loaded-state stub (Plan 08) with a real
pannable/zoomable node canvas, and build the three node types (`RequestNode`,
`InputNode`, `OutputNode`) with the field-level ports and run-status visuals
from the approved design mockup.

**Architecture:** `@xyflow/react` (React Flow) renders the canvas — its
built-in `<Background variant="dots">` gives the dotted-grid look for free,
along with pan/zoom and multi-`<Handle>`-per-node support that maps directly
onto field-level ports. `FlowPane` becomes a controlled `<ReactFlow>` whose
`nodes`/`edges` come from the `FlowTab` (Plan 08) and whose changes write
back through `updateFlowNodes`/`updateFlowEdges`.

**Tech Stack:** React, TypeScript, `@xyflow/react`, shadcn/ui, lucide-react,
Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§6 wiring semantics — node mockup ASCII art; §9 frontend). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`.
Depends on Plan 08's `FlowTab`/pane-store actions/domain types.

## Global Constraints

- `@xyflow/react` is this plan's one new dependency — it is the allowed
  "canvas/SVG" exception to this repo's shadcn-only rule (React Flow renders
  nodes as positioned `<div>`s and edges as SVG paths, not an HTML5
  `<canvas>` bitmap surface), confirmed against
  `.claude/rules/frontend-component-guardrails.md` and the OpenCollection
  spec reference's frontend rules (§9's canvas/SVG exception). All
  **non-canvas** chrome (the node palette, buttons, the picker inherited from
  Plan 08) still uses shadcn/ui primitives and `lucide-react` icons only.
- Every custom node component (`RequestNode`, `InputNode`, `OutputNode`) is
  registered via React Flow's `nodeTypes` prop on the single `<ReactFlow>`
  instance in `FlowPane` — do not create a second canvas or a second
  `nodeTypes` map anywhere else.
- Field-level input handles use React Flow's per-handle `id` prop so a
  `RequestNode` can expose more than one target `<Handle>`. Handle ids are
  `"url"`, `"headers"`, `"body"` — matching the backend `target_field`
  strings' first path segment (`"url"`, `"headers[N].value"`, `"body"`).
  **One `headers` handle represents all header slots for v1** — which
  specific header index/name a connection targets is chosen in the
  connection UI built in Plan 10, not by having one handle per header row.
  Say this explicitly in the `RequestNode` code comments so Plan 10's author
  isn't guessing.
- `ReactFlowProvider` wraps the canvas exactly once, inside `FlowCanvas`
  (as Task 1's code does) — not in `FlowPane`. A component tree with no
  provider (or more than one) breaks React Flow's internal state; this is
  the most common integration mistake with this library and must be checked
  explicitly in Step 3/Review Focus below. (Corrected in the Plan 08 review:
  this bullet used to say "at the top of `FlowPane`", which contradicted the
  code below. Because the provider is inside `FlowCanvas`, `useReactFlow()`
  cannot be called from `FlowPane` — see Plan 10 Step 6.)
- The `FlowCanvas` wrapper `<div>` carries `data-testid='flow-canvas'`.
  Plan 10's drag-and-drop test locates the canvas by this id.
- `FlowPane`'s picker branch (as built in Plan 08, extended in its review)
  now also has a "New flow name" `Input` + create `Button` that saves an
  empty flow and opens it. Step 5 replaces only the final loaded-state
  `return`; leave the picker branch as it is.
- `FlowTab` (Plan 08, as built) has no per-node status detail. Until Plan 10
  adds `nodeDetail`, `toRfNodes` passes only `{ kind, status }`, so
  `RequestNode`'s `statusCode`/`durationMs`/`error` are always undefined on
  the real canvas; they are exercised only by the node unit tests. Plan 10
  Task 3 extends `toRfNodes` to spread `tab.nodeDetail?.[id]` into `data`.
- Conventional-commit format for every Commit step.

## Review Focus

- The dotted background renders and pans/zooms without the node cards'
  `<Handle>` positions drifting out of alignment with their field rows —
  React Flow recalculates handle positions from the DOM automatically via
  `useUpdateNodeInternals`/its own resize observer, but a node component that
  conditionally renders/hides a field row without calling
  `updateNodeInternals` can leave a stale handle position; test that adding a
  header to a node's data updates its handle position, not just its text.
- A `RequestNode` with zero headers still renders its Headers row (e.g. "0
  set") rather than collapsing the row and losing its `headers` port — a
  connectable target must always be present, even for an empty field.
- Rapid idle → running → success/failure status transitions (simulating fast
  successive `patchFlowNodeStatus` calls, as a real run would produce) render
  the final state correctly and don't leave a stale intermediate visual
  (e.g. a "running" pulse that never clears).
- `InputNode`/`OutputNode` render correctly with no field rows at all — just
  their single port (`InputNode`: source only; `OutputNode`: target only) —
  and must not crash if their `kind.value`/incoming data is `undefined`.
- Deleting a node that has connected edges must not leave orphaned edges
  pointing at a nonexistent node id in the `FlowTab`'s `edges` array — React
  Flow's own `onNodesDelete`/`onEdgesChange` machinery handles this if wired
  correctly; a test should confirm the resulting `updateFlowEdges` call
  excludes edges touching the deleted node.

---

## Task 1: Canvas shell

**Files:**
- Modify: `package.json` (add `@xyflow/react`)
- Modify: `src/components/flow/FlowPane.tsx`
- Create: `src/components/flow/FlowCanvas.tsx`
- Create: `src/components/flow/__tests__/FlowCanvas.test.tsx`

**Interfaces:**
- Consumes: `FlowTab`, `updateFlowNodes`, `updateFlowEdges` (Plan 08).
- Produces: `FlowCanvas` component — consumed by Task 2/3 of this plan
  (registers `nodeTypes`) and by Plan 10 (adds `onConnect`/drag-and-drop
  handlers on top of this shell).

- [ ] **Step 1: Add the dependency**

Run: `yarn add @xyflow/react`
Expected: `package.json`/`yarn.lock` gain `@xyflow/react` (a React 18+ peer
dependency — compatible with this repo's React 19). Verify with
`yarn why @xyflow/react` that no peer-dependency warning is emitted; if one
is, note the exact warning text in the commit message body rather than
silently ignoring it.

- [ ] **Step 2: Write the failing test**

```tsx
// src/components/flow/__tests__/FlowCanvas.test.tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowCanvas } from '../FlowCanvas';
import type { FlowNode, FlowEdge } from '@/lib/tauri-api';

describe('FlowCanvas', () => {
  const nodes: FlowNode[] = [
    { id: 'n1', kind: { kind: 'Output', label: 'Result' }, position: { x: 0, y: 0 } },
  ];
  const edges: FlowEdge[] = [];

  it('renders the dotted background and the given nodes', () => {
    render(
      <FlowCanvas
        nodes={nodes}
        edges={edges}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    // React Flow renders its background as an SVG pattern container with
    // this test id in @xyflow/react — confirmed via its own testing docs.
    expect(document.querySelector('.react-flow__background')).toBeInTheDocument();
    expect(screen.getByText('Result')).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `yarn vitest run src/components/flow/__tests__/FlowCanvas.test.tsx`
Expected: FAIL — `FlowCanvas` module does not exist yet.

- [ ] **Step 4: Build `FlowCanvas`**

Create `src/components/flow/FlowCanvas.tsx`:

```tsx
import { useMemo } from 'react';
import {
  Background,
  BackgroundVariant,
  Controls,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  ReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { InputNode } from './nodes/InputNode';
import { OutputNode } from './nodes/OutputNode';
import { RequestNode } from './nodes/RequestNode';

const nodeTypes = {
  Request: RequestNode,
  Input: InputNode,
  Output: OutputNode,
};

export interface FlowCanvasProps {
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus: Record<string, FlowNodeStatus>;
  onNodesChange: (nodes: FlowNode[]) => void;
  onEdgesChange: (edges: FlowEdge[]) => void;
  onConnect: (connection: Connection) => void;
}

// Maps our backend-shaped FlowNode/FlowEdge into React Flow's own Node/Edge
// shape. `type` selects the nodeTypes entry above; everything else our
// custom node components need travels in `data`.
function toRfNodes(nodes: FlowNode[], nodeStatus: Record<string, FlowNodeStatus>): Node[] {
  return nodes.map((n) => ({
    id: n.id,
    type: n.kind.kind,
    position: n.position,
    data: { kind: n.kind, status: nodeStatus[n.id] ?? 'idle' },
  }));
}

function toRfEdges(edges: FlowEdge[]): Edge[] {
  return edges.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    target: e.targetNodeId,
    targetHandle: e.targetField.split('[')[0],
  }));
}

export function FlowCanvas({
  nodes,
  edges,
  nodeStatus,
  onNodesChange,
  onEdgesChange,
  onConnect,
}: FlowCanvasProps) {
  const rfNodes = useMemo(() => toRfNodes(nodes, nodeStatus), [nodes, nodeStatus]);
  const rfEdges = useMemo(() => toRfEdges(edges), [edges]);

  // React Flow's onNodesChange/onEdgesChange report deltas (position drags,
  // deletions, selection). We only need to persist deletions and position
  // moves back into FlowTab state for v1 — dragging a node updates its
  // `position`; deleting a node also drops any edge that referenced it, so
  // Plan 10's onConnect additions and this handler never leave a dangling
  // edge behind.
  const handleNodesChange = (changes: NodeChange[]) => {
    let next = nodes;
    for (const change of changes) {
      if (change.type === 'position' && change.position) {
        next = next.map((n) => (n.id === change.id ? { ...n, position: change.position! } : n));
      } else if (change.type === 'remove') {
        next = next.filter((n) => n.id !== change.id);
      }
    }
    if (next !== nodes) onNodesChange(next);
  };

  const handleEdgesChange = (changes: EdgeChange[]) => {
    let next = edges;
    for (const change of changes) {
      if (change.type === 'remove') {
        next = next.filter((e) => e.id !== change.id);
      }
    }
    if (next !== edges) onEdgesChange(next);
  };

  return (
    <ReactFlowProvider>
      <div data-testid='flow-canvas' className='h-full w-full'>
        <ReactFlow
          nodes={rfNodes}
          edges={rfEdges}
          nodeTypes={nodeTypes}
          onNodesChange={handleNodesChange}
          onEdgesChange={handleEdgesChange}
          onConnect={onConnect}
          fitView
        >
          <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
          <Controls />
        </ReactFlow>
      </div>
    </ReactFlowProvider>
  );
}
```

Note: `RequestNode`/`InputNode`/`OutputNode` (imported above) don't exist
yet — they're built in Task 2/3 of this plan. Create minimal placeholder
files for this step only if `yarn tsc --noEmit` fails without them:

```tsx
// src/components/flow/nodes/InputNode.tsx (placeholder — replaced in Task 3)
export function InputNode() {
  return null;
}
```

```tsx
// src/components/flow/nodes/OutputNode.tsx (placeholder — replaced in Task 3)
export function OutputNode() {
  return null;
}
```

```tsx
// src/components/flow/nodes/RequestNode.tsx (placeholder — replaced in Task 2)
export function RequestNode() {
  return null;
}
```

- [ ] **Step 5: Wire `FlowCanvas` into `FlowPane`'s loaded state**

In `src/components/flow/FlowPane.tsx`, replace the stub's final `return`
block (the "Loaded flow ... canvas rendering lands in Plan 09" placeholder
from Plan 08) with:

```tsx
import { FlowCanvas } from './FlowCanvas';
// ...
const updateFlowNodes = usePaneStore((s) => s.updateFlowNodes);
const updateFlowEdges = usePaneStore((s) => s.updateFlowEdges);
// ...
return (
  <FlowCanvas
    nodes={tab.nodes}
    edges={tab.edges}
    nodeStatus={tab.nodeStatus}
    onNodesChange={(nodes) => updateFlowNodes(tab.id, nodes)}
    onEdgesChange={(edges) => updateFlowEdges(tab.id, edges)}
    onConnect={() => {
      /* Plan 10 replaces this with real edge-creation + the expression editor popover */
    }}
  />
);
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `yarn vitest run src/components/flow/__tests__/FlowCanvas.test.tsx`
Expected: PASS — 1 test.

Run: `yarn tsc --noEmit`
Expected: succeeds.

- [ ] **Step 7: Commit**

```bash
git add package.json yarn.lock src/components/flow/FlowCanvas.tsx src/components/flow/FlowPane.tsx src/components/flow/nodes src/components/flow/__tests__/FlowCanvas.test.tsx
git commit -m "feat(frontend): add Flow canvas shell with React Flow"
```

---

## Task 2: `RequestNode` with field-level ports

**Files:**
- Modify: `src/components/flow/nodes/RequestNode.tsx`
- Create: `src/components/flow/nodes/__tests__/RequestNode.test.tsx`

**Interfaces:**
- Consumes: `FlowNodeKind` (`Request` variant), `FlowNodeStatus` (Plan 08),
  React Flow's `NodeProps`/`Handle`/`Position`.
- Produces: `RequestNode` — registered in `FlowCanvas`'s `nodeTypes` (Task 1).

- [ ] **Step 1: Write the failing tests**

```tsx
// src/components/flow/nodes/__tests__/RequestNode.test.tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ReactFlowProvider } from '@xyflow/react';
import { RequestNode } from '../RequestNode';

function renderNode(data: Parameters<typeof RequestNode>[0]['data']) {
  return render(
    <ReactFlowProvider>
      <RequestNode
        id='n1'
        data={data}
        selected={false}
        type='Request'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />
    </ReactFlowProvider>,
  );
}

const baseKind = {
  kind: 'Request' as const,
  label: 'Get Auth Token',
  source: { type: 'Saved' as const, requestPath: 'auth/login.yml' },
};

describe('RequestNode', () => {
  it('renders idle state with method badge, label, and field rows', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    expect(screen.getByText('Get Auth Token')).toBeInTheDocument();
    expect(screen.getByText(/URL/)).toBeInTheDocument();
    expect(screen.getByText(/Headers/)).toBeInTheDocument();
    expect(screen.getByText(/Body/)).toBeInTheDocument();
  });

  it('renders a headers row even with zero headers configured', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    expect(screen.getByTestId('request-node-headers-row')).toBeInTheDocument();
  });

  it('renders success state with status glow and result text', () => {
    renderNode({
      kind: baseKind,
      status: 'success',
      statusCode: 200,
      durationMs: 184,
    });
    expect(screen.getByText(/200/)).toBeInTheDocument();
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'success');
  });

  it('renders failure state with status glow and error text', () => {
    renderNode({
      kind: baseKind,
      status: 'failed',
      statusCode: 401,
      durationMs: 92,
      error: 'Unauthorized',
    });
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'failed');
  });

  it('renders running state distinctly from idle', () => {
    renderNode({ kind: baseKind, status: 'running' });
    expect(screen.getByTestId('request-node-card')).toHaveAttribute('data-status', 'running');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/components/flow/nodes/__tests__/RequestNode.test.tsx`
Expected: FAIL — the placeholder `RequestNode` renders `null`.

- [ ] **Step 3: Implement `RequestNode`**

```tsx
// src/components/flow/nodes/RequestNode.tsx
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { MoreVertical } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';

export interface RequestNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Request' }>;
  status: FlowNodeStatus;
  statusCode?: number;
  durationMs?: number;
  error?: string;
  headerCount?: number;
  bodyPreview?: string;
}

const METHOD_FROM_SOURCE = (kind: RequestNodeData['kind']) =>
  kind.source.type === 'Inline' ? kind.source.request.method : 'GET';
// Saved sources don't carry their method on the node itself (it lives in the
// referenced request file, resolved server-side at run time) — v1 shows a
// generic method badge for Saved nodes until Plan 10's sidebar-drag flow
// optionally hydrates a cached method label. Not a gap in this task: the
// spec's Saved/Inline distinction (§4) never promises client-visible method
// for Saved without an extra read, and no task in this plan claims to add one.

const statusStyles: Record<FlowNodeStatus, string> = {
  idle: 'border-border',
  running: 'border-blue-400 shadow-[0_0_0_1px_rgba(96,165,250,0.5)] animate-pulse',
  success: 'border-green-500 shadow-[0_0_0_1px_rgba(34,197,94,0.5)]',
  failed: 'border-red-500 shadow-[0_0_0_1px_rgba(239,68,68,0.5)]',
  skipped: 'border-muted-foreground/40 opacity-60',
};

export function RequestNode({ data, isConnectable }: NodeProps & { data: RequestNodeData }) {
  const { kind, status, statusCode, durationMs, error } = data;
  const method = METHOD_FROM_SOURCE(kind);
  const url = kind.source.type === 'Inline' ? kind.source.request.url : kind.source.requestPath;
  const headerCount =
    kind.source.type === 'Inline' ? kind.source.request.headers.length : (data.headerCount ?? 0);
  const bodyPreview =
    kind.source.type === 'Inline' ? (kind.source.request.body ?? '—') : (data.bodyPreview ?? '—');

  return (
    <div
      data-testid='request-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        statusStyles[status],
      )}
    >
      <div className='flex items-center justify-between gap-2 border-b px-2 py-1.5'>
        <div className='flex items-center gap-1.5 truncate'>
          <span className='rounded bg-muted px-1 py-0.5 font-mono text-[10px]'>{method}</span>
          <span className='truncate font-medium'>{kind.label}</span>
        </div>
        <MoreVertical className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
      </div>

      {status === 'success' && (
        <div className='px-2 pt-1 text-green-600'>
          ✓ {statusCode} · {durationMs}ms
        </div>
      )}
      {status === 'failed' && (
        <div className='px-2 pt-1 text-red-600'>
          ✕ {statusCode ?? 'Error'} · {error ?? `${durationMs}ms`}
        </div>
      )}

      <div className='relative space-y-1 px-2 py-1.5'>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id='url'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>URL</span>
          <span className='truncate'>{url}</span>
        </div>
        <div
          data-testid='request-node-headers-row'
          className='relative flex items-center gap-1.5 pl-2'
        >
          <Handle
            type='target'
            id='headers'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Headers</span>
          <span>{headerCount} set</span>
        </div>
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id='body'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Body</span>
          <span className='truncate'>{bodyPreview}</span>
        </div>
      </div>

      <Handle
        type='source'
        id='result'
        position={Position.Right}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/components/flow/nodes/__tests__/RequestNode.test.tsx`
Expected: PASS — 5 tests.

- [ ] **Step 5: Commit**

```bash
git add src/components/flow/nodes/RequestNode.tsx src/components/flow/nodes/__tests__/RequestNode.test.tsx
git commit -m "feat(frontend): add RequestNode with field-level ports"
```

---

## Task 3: `InputNode`, `OutputNode`, and node palette

**Files:**
- Modify: `src/components/flow/nodes/InputNode.tsx`
- Modify: `src/components/flow/nodes/OutputNode.tsx`
- Create: `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`
- Create: `src/components/flow/NodePalette.tsx`
- Modify: `src/components/flow/FlowPane.tsx`

**Interfaces:**
- Consumes: `FlowNodeKind` (`Input`/`Output` variants), `updateFlowNodes`
  (Plan 08).
- Produces: `InputNode`, `OutputNode`, `NodePalette` — `NodePalette` is
  consumed by Plan 10 (which adds the sidebar-drag-to-create Request node
  flow alongside this palette's Input/Output/inline-Request creation).

- [ ] **Step 1: Write the failing tests**

```tsx
// src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ReactFlowProvider } from '@xyflow/react';
import { InputNode } from '../InputNode';
import { OutputNode } from '../OutputNode';

function wrap(children: React.ReactNode) {
  return render(<ReactFlowProvider>{children}</ReactFlowProvider>);
}

describe('InputNode', () => {
  it('renders its label and value with only a source handle', () => {
    wrap(
      <InputNode
        id='i1'
        data={{ kind: { kind: 'Input', label: 'API Key', value: 'sk-123' }, status: 'idle' }}
        selected={false}
        type='Input'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />,
    );
    expect(screen.getByText('API Key')).toBeInTheDocument();
    expect(screen.getByTestId('input-node-card')).toBeInTheDocument();
  });

  it('renders without crashing when value is undefined', () => {
    wrap(
      <InputNode
        id='i1'
        data={{ kind: { kind: 'Input', label: 'API Key', value: undefined }, status: 'idle' }}
        selected={false}
        type='Input'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />,
    );
    expect(screen.getByTestId('input-node-card')).toBeInTheDocument();
  });
});

describe('OutputNode', () => {
  it('renders its label with only a target handle', () => {
    wrap(
      <OutputNode
        id='o1'
        data={{ kind: { kind: 'Output', label: 'Result' }, status: 'idle' }}
        selected={false}
        type='Output'
        dragging={false}
        zIndex={0}
        isConnectable
        draggable
        selectable
        deletable
        positionAbsoluteX={0}
        positionAbsoluteY={0}
      />,
    );
    expect(screen.getByText('Result')).toBeInTheDocument();
    expect(screen.getByTestId('output-node-card')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`
Expected: FAIL — both are still placeholders returning `null`.

- [ ] **Step 3: Implement `InputNode` and `OutputNode`**

```tsx
// src/components/flow/nodes/InputNode.tsx
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';

export interface InputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Input' }>;
  status: FlowNodeStatus;
}

export function InputNode({ data, isConnectable }: NodeProps & { data: InputNodeData }) {
  const value = data.kind.value;
  const display = value === undefined || value === null ? '—' : String(value);
  return (
    <div
      data-testid='input-node-card'
      className='w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm'
    >
      <div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{display}</div>
      <Handle
        type='source'
        id='result'
        position={Position.Right}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
    </div>
  );
}
```

```tsx
// src/components/flow/nodes/OutputNode.tsx
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';

export interface OutputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Output' }>;
  status: FlowNodeStatus;
  result?: string;
}

export function OutputNode({ data, isConnectable }: NodeProps & { data: OutputNodeData }) {
  return (
    <div
      data-testid='output-node-card'
      className='w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm'
    >
      <Handle
        type='target'
        id='value'
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>
      <div className='truncate px-2 py-1.5 text-muted-foreground'>{data.result ?? '—'}</div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`
Expected: PASS — 3 tests.

- [ ] **Step 5: Build the node palette**

Create `src/components/flow/NodePalette.tsx` — a small floating panel with
buttons to add an Input node, an Output node, or an inline Request node at a
default canvas position (shadcn `Button` + `lucide-react` icons only):

```tsx
import { Plus, ArrowRightToLine, ArrowLeftFromLine, Globe } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { FlowNode } from '@/lib/tauri-api';

let nextId = 0;
function newNodeId(prefix: string) {
  nextId += 1;
  return `${prefix}-${Date.now()}-${nextId}`;
}

export function NodePalette({ onAddNode }: { onAddNode: (node: FlowNode) => void }) {
  const defaultPosition = { x: 100, y: 100 };

  return (
    <div className='absolute left-3 top-3 z-10'>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant='outline' size='sm' className='gap-1.5'>
            <Plus className='h-3.5 w-3.5' aria-hidden='true' />
            Add node
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align='start'>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('input'),
                kind: { kind: 'Input', label: 'New Input', value: '' },
                position: defaultPosition,
              })
            }
          >
            <ArrowRightToLine className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Input
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('output'),
                kind: { kind: 'Output', label: 'New Output' },
                position: defaultPosition,
              })
            }
          >
            <ArrowLeftFromLine className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Output
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('request'),
                kind: {
                  kind: 'Request',
                  label: 'New Request',
                  source: {
                    type: 'Inline',
                    request: { method: 'GET', url: '', headers: [] },
                  },
                },
                position: defaultPosition,
              })
            }
          >
            <Globe className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Inline Request
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
```

If `@/components/ui/dropdown-menu` does not already exist in this repo's
shadcn component set, run the project's existing shadcn add process for it
(check `src/components/ui/` first — most shadcn primitive sets already
include it; do not hand-roll a dropdown with raw elements if it's missing —
add the primitive properly).

In `src/components/flow/FlowPane.tsx`, render `NodePalette` above
`FlowCanvas` in the loaded-state branch, wiring `onAddNode` to
`updateFlowNodes(tab.id, [...tab.nodes, node])`.

- [ ] **Step 6: Commit**

```bash
git add src/components/flow/nodes/InputNode.tsx src/components/flow/nodes/OutputNode.tsx src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx src/components/flow/NodePalette.tsx src/components/flow/FlowPane.tsx
git commit -m "feat(frontend): add Input/Output nodes and node palette"
```

---

## Next Plan

[Plan 10: Wiring UI + run controls](2026-09-27-flow-visual-workflow-builder-plan-10-frontend-wiring-and-run.md) —
adds sidebar-drag-to-create for Saved Request nodes, real edge creation with
the inline expression editor, and the Run/Stop/Save toolbar.

## Post-Implementation Review

Before starting Plan 10, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `package.json`, `yarn.lock`, `src/components/flow/FlowCanvas.tsx`,
> `src/components/flow/FlowPane.tsx`, `src/components/flow/nodes/RequestNode.tsx`,
> `src/components/flow/nodes/InputNode.tsx`, `src/components/flow/nodes/OutputNode.tsx`,
> `src/components/flow/NodePalette.tsx`, and this plan's test files.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — do the three node
>    components' `data` shapes match what `FlowCanvas.toRfNodes` actually
>    passes them, and does `NodePalette` produce `FlowNode` values that match
>    the Plan 01/08 domain-type contract exactly (discriminated unions on
>    `kind`/`type`)?
> 2. Code quality versus this plan's Review Focus section (handle-position
>    staleness on data changes, always-present Headers row, no stale visual
>    state across rapid status transitions, no orphaned edges after node
>    deletion, `InputNode`/`OutputNode` resilience to undefined data).
> 3. DDD/frontend guardrail conformance — `@xyflow/react` is used only for
>    the canvas itself; every other new element is shadcn/ui + lucide-react;
>    no raw `<button>`/`<select>` snuck into `NodePalette` or the node cards.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run every Vitest file this plan added
> (`FlowCanvas.test.tsx`, `RequestNode.test.tsx`, `InputOutputNodes.test.tsx`)
> and `yarn tsc --noEmit`, and confirm they still pass. Report what you found
> and fixed.

Only proceed to Plan 10 once this review comes back clean (or its fixes are
applied and re-verified).
