# Flow Plan 10: Wiring UI + Run Controls — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Flow canvas actually usable end-to-end: drag a saved request from the collection sidebar onto the canvas, wire node outputs into specific target fields with an inline expression editor, and run/stop/save the flow with live per-node status.

**Architecture:** Three independent interaction layers bolted onto the `FlowPane`/React Flow canvas from Plan 09: (1) HTML5 drag-and-drop from the existing sidebar tree, (2) React Flow's `onConnect` callback plus a small popover for the wiring expression, (3) a toolbar driving the Plan 07 Tauri commands and subscribing to the streamed run events. None of these add new persistence or execution logic — they only produce `FlowNode`/`FlowEdge` values (Plan 08 types) and call the existing `saveFlow`/`runFlow`/`cancelFlowRun` bindings.

**Tech Stack:** React, TypeScript, `@xyflow/react`, Zustand, shadcn/ui (`Popover`, `Button`, `Select`), `SingleLineEditor` (CodeMirror 6), Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§6 Wiring semantics, §9 Frontend, §10 Error handling). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md` (locked
interface contract every plan in this series depends on).

**This is the final plan in the Flow Phase 1 MVP series.**

## Global Constraints

- shadcn/ui primitives and `lucide-react` icons only — no raw `<button>`/`<input>`/`<select>`, per `.claude/rules/frontend-component-guardrails.md`.
- The wiring-expression input is a `{{variable}}`/JS-expression-aware single-line field, so per this repo's hard rule it **must** use `SingleLineEditor` (`@/components/editor`, CodeMirror 6) — never a raw `<input>` and never Monaco. Mirror the exact usage in `src/components/request/KeyValueEditor.tsx:77-84` (`value`/`onChange`/`placeholder`/`className` props; `variableContext`/`onNavigateToSource` are optional and not needed here since the expression evaluates against a captured node output, not the app's variable scopes).
- Zustand: read pane-store state via narrow selectors (`usePaneStore((s) => ...)`), never destructure the whole store at a component's top level — matches every existing pane-store consumer (e.g. `src/components/collections/RequestNode.tsx:88-91`).
- Commit messages use conventional commits format (`feat:`, `fix:`, etc.), per this repo's hard rule.
- **Reconciled against Plan 08/09's actual output (verified on disk, not an assumption):** `FlowTab`'s discriminant field is `tabType` (not `type`) and its collection-scoping field is `collectionName: string | null` (not `collectionRoot`) — confirmed in `2026-09-27-flow-visual-workflow-builder-plan-08-frontend-types-and-tab.md`. Plan 08 provides three bespoke pane-store actions — `updateFlowNodes(tabId, nodes)`, `updateFlowEdges(tabId, edges)`, `patchFlowNodeStatus(tabId, nodeId, status)` — following this repo's one-setter-per-concern convention (`updateRequest`/`updateTabTitle`/`toggleRunnerEntry`). There is **no generic `updateFlowTab(tabId, patch)` action** — this repo doesn't use that pattern anywhere, so this plan does not invent one either. Where this plan needs state Plan 08 doesn't provide (run id/run state, and per-node status detail for the mockup's "200 OK · 184ms" line), it adds two small, additive extensions of its own in Task 3 (see that task's new pane-store step): a new `setFlowRunState` action, and a widened `patchFlowNodeStatus` signature with an optional 4th `detail` argument. Both follow Plan 08's exact `updateTabInTree`-based implementation shape.
- **`RequestNode` naming — confirmed no collision, just distinct modules:** the existing sidebar row component is `src/components/collections/RequestNode.tsx`. Plan 09's React Flow node type is a *different* component at `src/components/flow/nodes/RequestNode.tsx`, also exported as `RequestNode`. Different files/import paths, so no compile-time collision — this plan's Task 1 imports only the sidebar one (as `RequestNodeProps`, for the drag payload) and never imports both in the same file, so no aliasing is needed here.
- **`@xyflow/react` coordinate API — confirmed, not conditional:** Plan 09 installs `@xyflow/react` unpinned (`yarn add @xyflow/react`), i.e. whatever is current at implementation time (v12+, the `@xyflow/react` package name itself only exists from v12 onward — the pre-fork package was `reactflow`). `screenToFlowPosition` is the correct, current API name; there is no v11/`project` fallback to hedge for.
- **Handle ids — confirmed against Plan 09's actual `<Handle>` markup:** `RequestNode`'s per-field target handles are `id='url'`, `id='headers'`, `id='body'`; its (and `InputNode`'s) single source handle is `id='result'` (not `'output'`); `OutputNode`'s single target handle is `id='value'`. This plan's code and tests below use these exact ids.

## Review Focus

- Dropping a request that already has a node elsewhere on the canvas must create a **second, independent** node — this is not a duplicate-prevention error; multiple nodes referencing the same saved request is valid (e.g. calling the same "refresh token" request from two branches).
- Creating a connection that would form a cycle is still allowed at the canvas/UI level — live cycle prevention is not required. Cycle rejection happens only at Save (Plan 07's `save_flow`). A test must confirm the UI does **not** block the connection itself.
- The run-event listener (`onFlowStepCompleted`/`onFlowRunFinished`) must be unsubscribed on tab close/unmount and when a new run starts — no leaked listeners accumulating across multiple runs of the same tab.
- Clicking Stop after a run has already finished is a harmless no-op (matches Plan 07's `cancel_flow_run` semantics for an unknown/finished run id) — it must not throw or show an error toast.
- A `save_flow` rejection whose error indicates a cycle must visually flag the specific node ids named in the error, not just show a generic toast — a user with a 10-node canvas needs to find the bad connection without hunting.

---

## Task 1: Drag a saved request from the sidebar onto the canvas

**Files:**
- Create: `src/lib/flow-drag.ts`
- Create: `src/lib/__tests__/flow-drag.test.ts`
- Modify: `src/components/collections/RequestNode.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (from Plan 09)
- Test: `src/components/flow/__tests__/FlowPane.dragdrop.test.tsx`

**Interfaces:**
- Consumes: `FlowNode`, `FlowNodeKind` (Plan 08 types); `updateFlowNodes` pane-store action (Plan 08); `RequestNodeProps` (existing, `src/components/collections/RequestNode.tsx:56-73` — `collectionName`, `path`, `name`, `method`).
- Produces: `FLOW_REQUEST_DRAG_MIME`, `encodeFlowRequestDragPayload`, `decodeFlowRequestDragPayload` (this task) — consumed by Task 1 only; no later task depends on them.

There is **no existing drag-and-drop mechanism anywhere in
`src/components/collections/`** (confirmed by grep — no `draggable`,
`onDragStart`, or `onDrop` usage in that directory before this task). This is
new plumbing, not a duplicate of anything existing.

- [ ] **Step 1: Write the failing test for the drag payload codec**

```typescript
// src/lib/__tests__/flow-drag.test.ts
import { describe, expect, it } from 'vitest';
import {
  decodeFlowRequestDragPayload,
  encodeFlowRequestDragPayload,
  FLOW_REQUEST_DRAG_MIME,
} from '../flow-drag';

function fakeDataTransfer(data: Record<string, string>): DataTransfer {
  return {
    getData: (type: string) => data[type] ?? '',
    setData: () => {},
  } as unknown as DataTransfer;
}

describe('flow-drag payload codec', () => {
  it('round-trips a payload through encode/decode', () => {
    const payload = { collection: 'my-collection', path: 'auth/login.yml', name: 'Login', method: 'POST' };
    const encoded = encodeFlowRequestDragPayload(payload);
    const dt = fakeDataTransfer({ [FLOW_REQUEST_DRAG_MIME]: encoded });
    expect(decodeFlowRequestDragPayload(dt)).toEqual(payload);
  });

  it('returns null when the drag payload MIME type is absent', () => {
    const dt = fakeDataTransfer({ 'text/plain': 'not a flow drag' });
    expect(decodeFlowRequestDragPayload(dt)).toBeNull();
  });

  it('returns null when the payload is present but not valid JSON', () => {
    const dt = fakeDataTransfer({ [FLOW_REQUEST_DRAG_MIME]: '{not json' });
    expect(decodeFlowRequestDragPayload(dt)).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/flow-drag.test.ts`
Expected: FAIL — `flow-drag` module does not exist yet.

- [ ] **Step 3: Implement the codec**

```typescript
// src/lib/flow-drag.ts
export const FLOW_REQUEST_DRAG_MIME = 'application/x-rocket-flow-request';

export interface FlowRequestDragPayload {
  collection: string;
  path: string;
  name: string;
  method: string;
}

export function encodeFlowRequestDragPayload(payload: FlowRequestDragPayload): string {
  return JSON.stringify(payload);
}

export function decodeFlowRequestDragPayload(dataTransfer: DataTransfer): FlowRequestDragPayload | null {
  const raw = dataTransfer.getData(FLOW_REQUEST_DRAG_MIME);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<FlowRequestDragPayload>;
    if (
      typeof parsed.collection === 'string' &&
      typeof parsed.path === 'string' &&
      typeof parsed.name === 'string' &&
      typeof parsed.method === 'string'
    ) {
      return parsed as FlowRequestDragPayload;
    }
    return null;
  } catch {
    return null;
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/lib/__tests__/flow-drag.test.ts`
Expected: PASS — 3 tests.

- [ ] **Step 5: Make the sidebar request row draggable**

In `src/components/collections/RequestNode.tsx`, add a `draggable` attribute
and `onDragStart` handler to the row's root `TreeItem` element (the element
already receiving `onClick`/context-menu handlers around line 75's returned
JSX), using the props already in scope:

```tsx
import { encodeFlowRequestDragPayload, FLOW_REQUEST_DRAG_MIME } from '@/lib/flow-drag';

// on the TreeItem (or its outermost wrapping element):
<TreeItem
  draggable
  onDragStart={(e) => {
    e.dataTransfer.setData(
      FLOW_REQUEST_DRAG_MIME,
      encodeFlowRequestDragPayload({ collection: collectionName, path, name, method }),
    );
    e.dataTransfer.effectAllowed = 'copy';
  }}
  // ...existing props unchanged
>
```

This does not change any existing click/context-menu behavior — `draggable`
and a native `dragstart` do not interfere with `onClick`.

- [ ] **Step 6: Handle the drop on the Flow canvas**

In `src/components/flow/FlowPane.tsx` (Plan 09), add drop handling to the
`ReactFlow` wrapper element. If Plan 09's `<ReactFlow>` element doesn't
already carry `data-testid='flow-canvas'`, add it here too — Step 7's
component test below needs it to locate the canvas:

```tsx
import { decodeFlowRequestDragPayload } from '@/lib/flow-drag';
import { usePaneStore } from '@/stores/pane-store';

// inside the FlowPane component, alongside the existing reactFlowInstance ref/hook from Plan 09:
const updateFlowNodes = usePaneStore((s) => s.updateFlowNodes);

const handleDragOver = (e: React.DragEvent) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
};

const handleDrop = (e: React.DragEvent) => {
  e.preventDefault();
  const payload = decodeFlowRequestDragPayload(e.dataTransfer);
  if (!payload) return;

  const position = reactFlowInstance.screenToFlowPosition({ x: e.clientX, y: e.clientY });
  const newNode: FlowNode = {
    id: crypto.randomUUID(),
    kind: { kind: 'Request', label: payload.name, source: { type: 'Saved', requestPath: payload.path } },
    position,
  };
  updateFlowNodes(tab.id, [...tab.nodes, newNode]);
};

// on the <ReactFlow ...> element from Plan 09:
<ReactFlow
  data-testid='flow-canvas'
  onDragOver={handleDragOver}
  onDrop={handleDrop}
  /* ...existing Plan 09 props */
>
```

`reactFlowInstance.screenToFlowPosition` is the current `@xyflow/react` API
name for converting a screen-space drop point into canvas coordinates —
confirmed against Plan 09's unpinned `yarn add @xyflow/react` (installs
v12+, where this is the only name; the older `project` name doesn't apply).

- [ ] **Step 7: Write the drop-handling component test**

```tsx
// src/components/flow/__tests__/FlowPane.dragdrop.test.tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FlowPane } from '../FlowPane';
import { encodeFlowRequestDragPayload, FLOW_REQUEST_DRAG_MIME } from '@/lib/flow-drag';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';

const baseTab: FlowTab = {
  id: 'flow-1',
  tabType: 'flow',
  title: 'Untitled Flow',
  collectionName: 'my-collection',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

describe('FlowPane drag-and-drop', () => {
  beforeEach(() => {
    usePaneStore.setState({ /* seed whatever minimal store shape FlowPane reads, per Plan 08/09's actual store slice */ });
  });

  it('adds a Saved Request node at the drop position when a sidebar request is dropped', () => {
    render(<FlowPane tab={baseTab} />);
    const canvas = screen.getByTestId('flow-canvas'); // Plan 09 must expose this test id on the ReactFlow wrapper

    const dataTransfer = {
      getData: (type: string) =>
        type === FLOW_REQUEST_DRAG_MIME
          ? encodeFlowRequestDragPayload({
              collection: 'my-collection',
              path: 'auth/login.yml',
              name: 'Login',
              method: 'POST',
            })
          : '',
    };

    fireEvent.dragOver(canvas, { dataTransfer });
    fireEvent.drop(canvas, { dataTransfer, clientX: 200, clientY: 150 });

    const updatedTab = usePaneStore.getState().root /* find baseTab.id */ as unknown as FlowTab;
    expect(updatedTab.nodes).toHaveLength(1);
    expect(updatedTab.nodes[0].kind).toMatchObject({
      kind: 'Request',
      label: 'Login',
      source: { type: 'Saved', requestPath: 'auth/login.yml' },
    });
  });

  it('ignores a drop whose dataTransfer carries no flow-request payload', () => {
    render(<FlowPane tab={baseTab} />);
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.drop(canvas, { dataTransfer: { getData: () => '' } });
    const updatedTab = usePaneStore.getState().root as unknown as FlowTab;
    expect(updatedTab.nodes).toHaveLength(0);
  });
});
```

This test's exact store-lookup lines (`usePaneStore.getState().root /* find
baseTab.id */`) depend on Plan 08's actual pane-store tree shape — adapt them
to however Plan 08 exposes tab lookup (e.g. a `findTab(id)` selector already
used elsewhere in `src/lib/pane-utils.ts`), keeping the same two assertions
(a matching node was added; a payload-less drop is a no-op).

- [ ] **Step 8: Run tests to verify they pass**

Run: `yarn vitest run src/lib/__tests__/flow-drag.test.ts src/components/flow/__tests__/FlowPane.dragdrop.test.tsx`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/lib/flow-drag.ts src/lib/__tests__/flow-drag.test.ts src/components/collections/RequestNode.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.dragdrop.test.tsx
git commit -m "feat(flow): drag a saved request from the sidebar onto the canvas"
```

---

## Task 2: Wiring UI — connection creates a `FlowEdge` with an expression editor

**Files:**
- Create: `src/lib/flow-wiring.ts`
- Create: `src/lib/__tests__/flow-wiring.test.ts`
- Modify: `src/components/flow/FlowPane.tsx`
- Create: `src/components/flow/WireExpressionPopover.tsx`
- Test: `src/components/flow/__tests__/WireExpressionPopover.test.tsx`

**Interfaces:**
- Consumes: `FlowEdge`, `FlowNode`, `FlowNodeKind` (Plan 08); `updateFlowEdges` (Plan 08); `SingleLineEditor` (existing, `@/components/editor`).
- Produces: `buildEdgeFromConnection` (this task) — a pure function, and `WireExpressionPopover` component — both used only within this task's `FlowPane` wiring; no later task consumes them.

- [ ] **Step 1: Write the failing test for the pure connection-to-edge mapping**

```typescript
// src/lib/__tests__/flow-wiring.test.ts
import { describe, expect, it } from 'vitest';
import { buildEdgeFromConnection, defaultExpressionFor } from '../flow-wiring';
import type { FlowNode } from '@/types/flow-types';

const requestSource: FlowNode = {
  id: 'node-a',
  kind: { kind: 'Request', label: 'Login', source: { type: 'Saved', requestPath: 'auth/login.yml' } },
  position: { x: 0, y: 0 },
};

const inputSource: FlowNode = {
  id: 'node-b',
  kind: { kind: 'Input', label: 'Username', value: 'alice' },
  position: { x: 0, y: 0 },
};

describe('defaultExpressionFor', () => {
  it('defaults to "response.body" for a Request source node', () => {
    expect(defaultExpressionFor(requestSource)).toBe('response.body');
  });

  it('defaults to "value" for an Input source node', () => {
    expect(defaultExpressionFor(inputSource)).toBe('value');
  });
});

describe('buildEdgeFromConnection', () => {
  it('maps a React Flow connection into a FlowEdge with a generated id and default expression', () => {
    const edge = buildEdgeFromConnection(
      { source: 'node-a', sourceHandle: 'result', target: 'node-c', targetHandle: 'url' },
      requestSource,
    );
    expect(edge).toMatchObject({
      sourceNodeId: 'node-a',
      targetNodeId: 'node-c',
      targetField: 'url',
      expression: 'response.body',
    });
    expect(edge.id).toBeTruthy();
  });

  it('returns null when the connection is missing a target handle', () => {
    const edge = buildEdgeFromConnection(
      { source: 'node-a', sourceHandle: 'result', target: 'node-c', targetHandle: null },
      requestSource,
    );
    expect(edge).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/flow-wiring.test.ts`
Expected: FAIL — `flow-wiring` module does not exist yet.

- [ ] **Step 3: Implement the pure mapping**

```typescript
// src/lib/flow-wiring.ts
import type { Connection } from '@xyflow/react';
import type { FlowEdge, FlowNode } from '@/types/flow-types';

export function defaultExpressionFor(sourceNode: FlowNode): string {
  return sourceNode.kind.kind === 'Input' ? 'value' : 'response.body';
}

export function buildEdgeFromConnection(connection: Connection, sourceNode: FlowNode): FlowEdge | null {
  if (!connection.source || !connection.target || !connection.targetHandle) return null;
  return {
    id: crypto.randomUUID(),
    sourceNodeId: connection.source,
    targetNodeId: connection.target,
    targetField: connection.targetHandle,
    expression: defaultExpressionFor(sourceNode),
  };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/lib/__tests__/flow-wiring.test.ts`
Expected: PASS — 4 tests.

- [ ] **Step 5: Build the expression popover**

Create `src/components/flow/WireExpressionPopover.tsx`. A `headers`-targeted
connection (Plan 09's single generic `headers` input handle, standing in for
per-index header ports) needs a header name/index picker before the
expression field — resolving which existing header on the target request the
wire fills, or appending a new one if no header with that name exists yet:

```tsx
import { useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { FlowEdge, FlowNode } from '@/types/flow-types';

interface WireExpressionPopoverProps {
  edge: FlowEdge;
  targetNode: FlowNode;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCommit: (edge: FlowEdge) => void;
  children: React.ReactNode;
}

/** Existing header names on the target Request node, or [] for non-Request nodes. */
function targetHeaderNames(targetNode: FlowNode): string[] {
  if (targetNode.kind.kind !== 'Request') return [];
  const { source } = targetNode.kind;
  return source.type === 'Inline' ? source.request.headers.map((h) => h.name) : [];
}

export function WireExpressionPopover({
  edge,
  targetNode,
  open,
  onOpenChange,
  onCommit,
  children,
}: WireExpressionPopoverProps) {
  const [expression, setExpression] = useState(edge.expression);
  const [headerName, setHeaderName] = useState('');
  const isHeadersTarget = edge.targetField === 'headers';
  const existingHeaders = targetHeaderNames(targetNode);

  const handleCommit = () => {
    let targetField = edge.targetField;
    if (isHeadersTarget) {
      const trimmed = headerName.trim();
      if (!trimmed) return;
      const existingIndex = existingHeaders.indexOf(trimmed);
      // Match by name if the header already exists; otherwise this wire
      // targets a new header appended at the next index — the execution
      // side (Plan 05's apply_wired_overrides) creates it if absent.
      targetField = `headers[${existingIndex === -1 ? existingHeaders.length : existingIndex}].value`;
    }
    onCommit({ ...edge, targetField, expression });
    onOpenChange(false);
  };

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent className='w-72 space-y-2'>
        {isHeadersTarget && (
          <div>
            <label className='text-xs font-medium'>Header name</label>
            <Input
              value={headerName}
              onChange={(e) => setHeaderName(e.target.value)}
              placeholder='e.g. Authorization'
              className='h-8 text-sm'
            />
          </div>
        )}
        <div>
          <label className='text-xs font-medium'>Value from source</label>
          <SingleLineEditor value={expression} onChange={setExpression} placeholder='response.body' className='text-xs' />
        </div>
        <Button size='sm' onClick={handleCommit}>
          Save
        </Button>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 6: Wire `onConnect` in `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

```tsx
import { buildEdgeFromConnection } from '@/lib/flow-wiring';
import { WireExpressionPopover } from './WireExpressionPopover';
import type { Connection } from '@xyflow/react';

// inside FlowPane, alongside the updateFlowNodes selector from Task 1:
const updateFlowEdges = usePaneStore((s) => s.updateFlowEdges);
const [pendingEdge, setPendingEdge] = useState<FlowEdge | null>(null);

const handleConnect = (connection: Connection) => {
  const sourceNode = tab.nodes.find((n) => n.id === connection.source);
  if (!sourceNode) return;
  const edge = buildEdgeFromConnection(connection, sourceNode);
  if (!edge) return;
  updateFlowEdges(tab.id, [...tab.edges, edge]);
  setPendingEdge(edge); // opens the popover immediately, per spec §6
};

// on <ReactFlow ...>:
<ReactFlow onConnect={handleConnect} /* ...existing props */ />

{pendingEdge && (
  <WireExpressionPopover
    edge={pendingEdge}
    targetNode={tab.nodes.find((n) => n.id === pendingEdge.targetNodeId)!}
    open={pendingEdge !== null}
    onOpenChange={(open) => !open && setPendingEdge(null)}
    onCommit={(updated) =>
      updateFlowEdges(tab.id, tab.edges.map((e) => (e.id === updated.id ? updated : e)))
    }
  >
    {/* Plan 09's edge/handle DOM node the popover anchors to */}
    <span />
  </WireExpressionPopover>
)}
```

- [ ] **Step 7: Write the popover component test**

```tsx
// src/components/flow/__tests__/WireExpressionPopover.test.tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { WireExpressionPopover } from '../WireExpressionPopover';
import type { FlowEdge, FlowNode } from '@/types/flow-types';

const edge: FlowEdge = {
  id: 'edge-1',
  sourceNodeId: 'node-a',
  targetNodeId: 'node-c',
  targetField: 'url',
  expression: 'response.body',
};

const targetNode: FlowNode = {
  id: 'node-c',
  kind: { kind: 'Request', label: 'Target', source: { type: 'Inline', request: { method: 'GET', url: '', headers: [], body: undefined } } },
  position: { x: 0, y: 0 },
};

describe('WireExpressionPopover', () => {
  it('pre-fills the expression editor with the edge\'s current expression and commits an edit', () => {
    const onCommit = vi.fn();
    render(
      <WireExpressionPopover edge={edge} targetNode={targetNode} open onOpenChange={() => {}} onCommit={onCommit}>
        <button type='button'>anchor</button>
      </WireExpressionPopover>,
    );
    expect(screen.getByText('response.body')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(onCommit).toHaveBeenCalledWith(expect.objectContaining({ id: 'edge-1', expression: 'response.body' }));
  });
});
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `yarn vitest run src/lib/__tests__/flow-wiring.test.ts src/components/flow/__tests__/WireExpressionPopover.test.tsx`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/lib/flow-wiring.ts src/lib/__tests__/flow-wiring.test.ts src/components/flow/FlowPane.tsx src/components/flow/WireExpressionPopover.tsx src/components/flow/__tests__/WireExpressionPopover.test.tsx
git commit -m "feat(flow): add connection wiring with expression popover"
```

---

## Task 3: Run/Stop/Save toolbar with live status streaming

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Create: `src/components/flow/FlowToolbar.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Modify: `src/types/pane-types.ts` (add `runId?: string` and `nodeDetail?: Record<string, { statusCode?: number; durationMs?: number; error?: string }>` to `FlowTab` — neither exists in Plan 08's `FlowTab`, both are needed here to correlate streamed events to the active run and to show status/timing on each node)
- Modify: `src/stores/pane-store.ts` (add `setFlowRunState`; widen `patchFlowNodeStatus` with an optional `detail` parameter)
- Modify: `src/stores/__tests__/pane-store.test.ts` (add the two new tests in Step 4, in the existing `'Flow tab actions'` block)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: `runFlow`, `cancelFlowRun`, `saveFlow` (Plan 08 bindings over Plan 07's Tauri commands); `updateFlowNodes`/`updateFlowEdges` (Plan 08, for reference — not called directly in this task).
- Produces: `onFlowStepCompleted`, `onFlowRunFinished` (added to `tauri-api.ts`); `setFlowRunState` and the widened `patchFlowNodeStatus` (added to `pane-store.ts`) — terminal deliverables of this plan; no later plan consumes them.

The closest existing precedent for "subscribe to a streamed Tauri event and
clean up on unmount" is `onCollectionChanged`/`onFileChange`
(`src/lib/tauri-api.ts:1019-1035`): a thin wrapper over `listen<T>(eventName,
handler)` returning `Promise<UnlistenFn>`, called inside a component's
`useEffect` that awaits the promise and returns the `UnlistenFn` for cleanup.
There is **no existing per-run-id-scoped variant** — the sequential Collection
Runner's frontend (`RunnerPane.tsx`) drives its loop by directly `await`ing
`executeRequest` calls rather than listening to streamed backend events, so
it is not a precedent to follow here. This task follows the
`onCollectionChanged` shape and adds `runId` filtering inside the handler,
since a Tauri event payload — not the event name — carries the run id.

- [ ] **Step 1: Write the failing test for the toolbar's run lifecycle**

```tsx
// src/components/flow/__tests__/FlowToolbar.test.tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FlowToolbar } from '../FlowToolbar';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    runFlow: vi.fn(),
    cancelFlowRun: vi.fn(),
    saveFlow: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowRunFinished: vi.fn(),
  };
});

const onPatchStatus = vi.fn();
const onRunStateChange = vi.fn();

describe('FlowToolbar', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.runFlow).mockResolvedValue('run-123');
    vi.mocked(tauriApi.onFlowStepCompleted).mockResolvedValue(() => {});
    vi.mocked(tauriApi.onFlowRunFinished).mockResolvedValue(() => {});
    vi.mocked(tauriApi.saveFlow).mockResolvedValue(undefined);
    vi.mocked(tauriApi.cancelFlowRun).mockResolvedValue(undefined);
    onPatchStatus.mockClear();
    onRunStateChange.mockClear();
  });

  it('starts a run, subscribes to step/finish events, and reports the run id', async () => {
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null));
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');
  });

  it('a step-completed event for a different run id is ignored', async () => {
    let capturedHandler: ((e: { runId: string; nodeId: string; status: string }) => void) | undefined;
    vi.mocked(tauriApi.onFlowStepCompleted).mockImplementation(async (handler) => {
      capturedHandler = handler;
      return () => {};
    });
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(capturedHandler).toBeDefined());
    capturedHandler?.({ runId: 'some-other-run', nodeId: 'node-a', status: 'success' });
    expect(onPatchStatus).not.toHaveBeenCalled();
  });

  it('Stop calls cancelFlowRun with the active run id', async () => {
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123'));
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-123');
  });

  it('Stop is a no-op when no run is active', async () => {
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: FAIL — `FlowToolbar`, `onFlowStepCompleted`, `onFlowRunFinished` do
not exist yet.

- [ ] **Step 3: Add the streamed-event bindings**

In `src/lib/tauri-api.ts`, alongside the existing "Realtime events" section
(`src/lib/tauri-api.ts:1015-1041`):

```typescript
export interface FlowStepCompletedEvent {
  runId: string;
  nodeId: string;
  status: 'running' | 'success' | 'failed' | 'skipped';
  statusCode?: number;
  durationMs?: number;
  error?: string;
}

export interface FlowRunFinishedEvent {
  runId: string;
  stoppedReason: string;
  nodeCount: number;
  failedCount: number;
  skippedCount: number;
}

export const onFlowStepCompleted = (
  handler: (event: FlowStepCompletedEvent) => void,
): Promise<UnlistenFn> => listen<FlowStepCompletedEvent>('flow-step-completed', (e) => handler(e.payload));

export const onFlowRunFinished = (
  handler: (event: FlowRunFinishedEvent) => void,
): Promise<UnlistenFn> => listen<FlowRunFinishedEvent>('flow-run-finished', (e) => handler(e.payload));
```

(`runFlow`, `cancelFlowRun`, `saveFlow` are assumed already added by Plan 08
— this task only adds the event-listener bindings, which are this plan's own
responsibility per the index.)

- [ ] **Step 4: Extend pane-store with `setFlowRunState` and a widened `patchFlowNodeStatus`**

Plan 08's `patchFlowNodeStatus(tabId, nodeId, status)` has no way to carry a
status code/timing/error, and there is no action at all for persisting
`runState`/`runId` on the tab (Plan 08 doesn't need one; this plan does).
Both additions follow Plan 08's exact `updateTabInTree`-based shape.

Add to the existing `describe('Flow tab actions', ...)` block in
`src/stores/__tests__/pane-store.test.ts`:

```typescript
it('setFlowRunState stores the run id and state on the tab', () => {
  const tabId = usePaneStore.getState().openFlowTab('my-collection', 'my-flow');
  usePaneStore.getState().setFlowRunState(tabId as unknown as string, 'running', 'run-123');
  const tab = findFlowTab(usePaneStore.getState().root, tabId as unknown as string);
  expect(tab?.runState).toBe('running');
  expect(tab?.runId).toBe('run-123');
});

it('patchFlowNodeStatus records optional detail alongside the status', () => {
  const tabId = 'flow-1';
  seedFlowTab(tabId, { nodes: [{ id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }] });
  usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', {
    statusCode: 200,
    durationMs: 184,
  });
  const tab = findFlowTab(usePaneStore.getState().root, tabId);
  expect(tab?.nodeStatus.n1).toBe('success');
  expect(tab?.nodeDetail?.n1).toEqual({ statusCode: 200, durationMs: 184 });
});
```

(`findFlowTab`/`seedFlowTab` are whatever tab-lookup/seed test helpers Plan
08's own `'Flow tab actions'` tests already use — reuse them, don't add a
second helper.)

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Flow tab actions"`
Expected: FAIL — `setFlowRunState` doesn't exist; `patchFlowNodeStatus` doesn't accept a 4th argument yet (TypeScript error).

In `src/types/pane-types.ts`, widen `FlowTab`:

```typescript
export interface FlowTab extends BaseTab {
  tabType: 'flow';
  collectionName: string | null;
  flowName: string | null;
  nodes: import('@/lib/tauri-api').FlowNode[];
  edges: import('@/lib/tauri-api').FlowEdge[];
  nodeStatus: Record<string, import('@/lib/tauri-api').FlowNodeStatus>;
  nodeDetail?: Record<string, { statusCode?: number; durationMs?: number; error?: string }>;
  runState: 'idle' | 'running' | 'done';
  runId?: string;
}
```

In `src/stores/pane-store.ts`, add the action type declarations alongside
Plan 08's `updateFlowNodes`/`updateFlowEdges`/`patchFlowNodeStatus`:

```typescript
setFlowRunState: (tabId: string, runState: 'idle' | 'running' | 'done', runId?: string) => void;
```

and replace Plan 08's `patchFlowNodeStatus` type declaration with:

```typescript
patchFlowNodeStatus: (
  tabId: string,
  nodeId: string,
  status: FlowNodeStatus,
  detail?: { statusCode?: number; durationMs?: number; error?: string },
) => void;
```

Implement both, replacing Plan 08's `patchFlowNodeStatus` body:

```typescript
setFlowRunState(tabId, runState, runId) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) =>
      isFlowTab(tab) ? { ...tab, runState, runId } : tab,
    ),
  });
},

patchFlowNodeStatus(tabId, nodeId, status, detail) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) => {
      if (!isFlowTab(tab)) return tab;
      if (!tab.nodes.some((n) => n.id === nodeId)) return tab;
      return {
        ...tab,
        nodeStatus: { ...tab.nodeStatus, [nodeId]: status },
        nodeDetail: detail ? { ...tab.nodeDetail, [nodeId]: detail } : tab.nodeDetail,
      };
    }),
  });
},
```

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Flow tab actions"`
Expected: PASS.

- [ ] **Step 5: Build `FlowToolbar`**

```tsx
// src/components/flow/FlowToolbar.tsx
import { useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { cancelFlowRun, onFlowRunFinished, onFlowStepCompleted, runFlow } from '@/lib/tauri-api';
import type { UnlistenFn } from '@tauri-apps/api/event';

interface FlowToolbarProps {
  collection: string;
  flowName: string;
  environmentName: string | null;
  onPatchStatus: (nodeId: string, status: string, detail?: { statusCode?: number; durationMs?: number; error?: string }) => void;
  onRunStateChange: (state: 'running' | 'done', runId?: string) => void;
}

export function FlowToolbar({
  collection,
  flowName,
  environmentName,
  onPatchStatus,
  onRunStateChange,
}: FlowToolbarProps) {
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const unlistenRefs = useRef<UnlistenFn[]>([]);

  const cleanupListeners = () => {
    for (const unlisten of unlistenRefs.current) unlisten();
    unlistenRefs.current = [];
  };

  const handleRun = async () => {
    cleanupListeners();
    const runId = await runFlow(collection, flowName, environmentName ?? undefined);
    setActiveRunId(runId);
    onRunStateChange('running', runId);

    const unlistenStep = await onFlowStepCompleted((event) => {
      if (event.runId !== runId) return;
      onPatchStatus(event.nodeId, event.status, {
        statusCode: event.statusCode,
        durationMs: event.durationMs,
        error: event.error,
      });
    });
    const unlistenFinish = await onFlowRunFinished((event) => {
      if (event.runId !== runId) return;
      onRunStateChange('done', runId);
      cleanupListeners();
    });
    unlistenRefs.current = [unlistenStep, unlistenFinish];
  };

  const handleStop = () => {
    if (!activeRunId) return;
    void cancelFlowRun(activeRunId);
  };

  return (
    <div className='flex items-center gap-2'>
      <Button size='sm' onClick={() => void handleRun()}>
        Run
      </Button>
      <Button size='sm' variant='outline' onClick={handleStop}>
        Stop
      </Button>
    </div>
  );
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `yarn vitest run src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS — 4 tests.

- [ ] **Step 7: Wire `FlowToolbar` into `FlowPane`, plus Save and cycle-error flagging**

In `src/components/flow/FlowPane.tsx`:

```tsx
import { FlowToolbar } from './FlowToolbar';
import { saveFlow } from '@/lib/tauri-api';
import { toast } from 'sonner';

// inside FlowPane, alongside existing state:
const [cycleNodeIds, setCycleNodeIds] = useState<string[]>([]);

const handleSave = async () => {
  try {
    await saveFlow(tab.collectionName!, { name: tab.flowName!, nodes: tab.nodes, edges: tab.edges });
    setCycleNodeIds([]);
    toast.success('Flow saved.');
  } catch (err) {
    // Plan 07's save_flow surfaces a cycle error as a message containing the
    // offending node ids — parse and flag them rather than showing only a
    // generic toast, per this plan's Review Focus.
    const message = String(err);
    const match = message.match(/cycle detected through node\(s\): \[(.*?)\]/);
    if (match) {
      setCycleNodeIds(match[1].split(',').map((s) => s.trim().replace(/['"]/g, '')));
    }
    toast.error(`Could not save flow: ${message}`);
  }
};

// alongside the updateFlowNodes/updateFlowEdges selectors from earlier tasks:
const patchFlowNodeStatus = usePaneStore((s) => s.patchFlowNodeStatus);
const setFlowRunState = usePaneStore((s) => s.setFlowRunState);

// in the toolbar row:
<FlowToolbar
  collection={tab.collectionName!}
  flowName={tab.flowName!}
  environmentName={activeEnvironmentName}
  onPatchStatus={(nodeId, status, detail) => patchFlowNodeStatus(tab.id, nodeId, status as FlowNodeStatus, detail)}
  onRunStateChange={(state, runId) => setFlowRunState(tab.id, state, runId)}
/>
<Button size='sm' variant='outline' onClick={() => void handleSave()}>
  Save
</Button>
```

Pass `cycleNodeIds` down to each `RequestNode`/`InputNode`/`OutputNode`
render (Plan 09) as a boolean `hasCycleError` prop (`cycleNodeIds.includes(node.id)`)
so Plan 09's node components can render the temporary red outline — this
plan does not itself style the node border since that visual lives in Plan
09's node components; it only supplies which node ids are implicated.

- [ ] **Step 8: Verify the app builds**

Run: `yarn tsc --noEmit`
Expected: succeeds — no type errors across `FlowToolbar.tsx`, the
`tauri-api.ts` additions, `pane-store.ts`'s widened action, and
`FlowPane.tsx`'s new wiring.

- [ ] **Step 9: Commit**

```bash
git add src/lib/tauri-api.ts src/components/flow/FlowToolbar.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowToolbar.test.tsx src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat(flow): add run/stop/save toolbar with live status streaming"
```

---

## Next Plan

None — this is the final plan in the Flow Phase 1 MVP series. After this
plan's review passes, Phase 1 is feature-complete per the spec's §12
Acceptance Criteria; verify those criteria end-to-end (manually or via an
integration test) before considering Flow Phase 1 done.

## Post-Implementation Review

Before considering the Flow Phase 1 MVP series complete, dispatch a subagent
(Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this
brief:

> Review every file this plan created or modified:
> `src/lib/flow-drag.ts`, `src/lib/__tests__/flow-drag.test.ts`,
> `src/components/collections/RequestNode.tsx`,
> `src/components/flow/FlowPane.tsx`, `src/lib/flow-wiring.ts`,
> `src/lib/__tests__/flow-wiring.test.ts`,
> `src/components/flow/WireExpressionPopover.tsx`,
> `src/components/flow/__tests__/WireExpressionPopover.test.tsx`,
> `src/lib/tauri-api.ts`, `src/components/flow/FlowToolbar.tsx`,
> `src/components/flow/__tests__/FlowToolbar.test.tsx`,
> `src/types/pane-types.ts`, `src/stores/pane-store.ts`,
> `src/stores/__tests__/pane-store.test.ts`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — this plan was already
>    reconciled against Plans 08/09's actual shipped code (`tabType`/
>    `collectionName`, `updateFlowNodes`/`updateFlowEdges`/`patchFlowNodeStatus`,
>    the `result`/`url`/`headers`/`body`/`value` handle ids, no
>    `RequestNode` naming collision), so treat those as settled — but still
>    verify each one against whatever actually landed in `src/` by the time
>    you run this review, since a task's real implementer may have deviated
>    further. Specifically re-confirm: the new `setFlowRunState` action and
>    the widened `patchFlowNodeStatus(tabId, nodeId, status, detail?)`
>    signature compile against every call site across Plans 08-10, and the
>    `flow-canvas` test id actually exists on the rendered `<ReactFlow>`
>    element.
> 2. Code quality versus this plan's Review Focus section (independent nodes
>    from repeated drags, no live cycle prevention in the UI, listener
>    cleanup on unmount/new-run, Stop-after-finish no-op, cycle-error node
>    flagging).
> 3. Frontend guardrail conformance per
>    `.claude/rules/frontend-component-guardrails.md` — shadcn/ui and
>    lucide-react only, `SingleLineEditor` (never raw input/Monaco) for the
>    wiring expression field, narrow Zustand selectors only.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `yarn vitest run src/lib/__tests__/flow-drag.test.ts
> src/lib/__tests__/flow-wiring.test.ts
> src/components/flow/__tests__/FlowPane.dragdrop.test.tsx
> src/components/flow/__tests__/WireExpressionPopover.test.tsx
> src/components/flow/__tests__/FlowToolbar.test.tsx
> src/stores/__tests__/pane-store.test.ts -t "Flow tab actions"` and
> `yarn tsc --noEmit`, and confirm they still pass.
>
> Additionally, since this is the final plan in the series: skim the spec's
> §12 Acceptance Criteria list
> (`docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`)
> and flag (not necessarily fix, since some criteria span multiple crates)
> any criterion that no plan across the whole 10-plan series appears to
> cover. Report what you found and fixed, and any uncovered acceptance
> criteria.

Once this review comes back clean (or its fixes are applied and
re-verified), Flow Phase 1 (MVP) is complete.
