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
- **Corrections from the Plan 08 post-implementation review (checked against the as-built code, commits 8b3ee94/395011e plus the review's own fix commit):**
  - `openFlowTab(collectionName, flowName?)` returns `Promise<void>`, not a tab id. Tests `await` it and then look the tab up.
  - The pane-store test file's Flow helpers are `findFirstFlowTab()` (no arguments; first `FlowTab` anywhere in the tree) and `updateTabInTreeForTest(root, tabId, updater)` (seeds a tab's fields). There is no `findFlowTab(root, id)` or `seedFlowTab`. Step 4's tests below use the real helpers.
  - `FlowPane`'s props are `{ tab: FlowTab; groupId: string }`. `groupId` is required, so every `render(<FlowPane ... />)` must pass it.
  - `ReactFlowProvider` and `<ReactFlow>` both live inside Plan 09's `FlowCanvas`, not `FlowPane`, so `FlowPane` cannot call `useReactFlow()` (it is outside the provider and would throw). Drop handling therefore goes in `FlowCanvas` — see Step 6.
  - `FlowTab` has no `nodeDetail` until this plan's Task 3 adds it, and Plan 09's `toRfNodes` passes only `{ kind, status }`. Task 3 must also extend `FlowCanvas` so each node's `data` includes `...nodeDetail[id]` (and `hasCycleError`), otherwise the "200 · 184ms" line never shows on the real canvas.
  - A "Flow" entry in the tab bar's "+" context menu (`openFlowTab(null)`) and a create-new-flow control in `FlowPane`'s picker (saves an empty flow, then opens it) already exist from the Plan 08 review. Do not add second copies.
- **Corrections from the Plan 09 post-implementation review (checked against the as-built code):**
  - `FlowCanvas` is now a thin wrapper: `FlowCanvas` renders `<ReactFlowProvider><FlowCanvasInner {...props} /></ReactFlowProvider>`, and `FlowCanvasInner` (same file, not exported) owns `<ReactFlow>` and the `data-testid='flow-canvas'` wrapper `<div>`. So `useReactFlow()` **can** be called synchronously inside `FlowCanvasInner`. Use that for drop handling (Task 1 Step 6), not `onInit` — `onInit` fires on a `setTimeout` after the viewport initializes, so a test that renders and drops at once would see a `null` instance.
  - `FlowCanvasInner` keeps node/edge **selection** and each node's **measured size** in canvas-local state (neither is persisted). Selection is what makes the Backspace delete work; measured size keeps nodes visible across status patches. `toRfNodes(nodes, nodeStatus, selectedIds, measured)` and `toRfEdges(edges, selectedIds)` take those extra arguments — when Task 3 extends `toRfNodes`, keep them. `toRfEdges` also sets `sourceHandle: 'result'`.
  - All three node `data` types already accept an optional `hasCycleError?: boolean` and render a red ring for it. `RequestNodeData` also accepts an optional `method?: string`; a Saved node with no `method` shows a neutral `SAVED` badge (never a guessed `GET`). `FlowNode` has nowhere to persist a Saved request's method, so Task 1 does not need to pass it.
  - The wire expression is evaluated against a response-shaped object for **every** source kind: an Input node's value is exposed as `response.body` (`resolve_flow_wire_expression` in `crates/rocket-app/src/flow_execution_service.rs`). There is no bare `value` binding, so an expression of `value` fails the run. The default expression is `response.body` for all sources (Task 2).
  - Output nodes: `OutputNodeData.result` exists but nothing fills it. Neither `FlowStepResult` nor `flow-step-completed` carries an Output node's resolved value, so spec §6's "captured output shown read-only in the node body" is not reachable in Phase 1 without a backend change. Out of scope for this plan; do not fake it.
- **Handle ids — confirmed against Plan 09's actual `<Handle>` markup:** `RequestNode`'s per-field target handles are `id='url'`, `id='headers'`, `id='body'`; its (and `InputNode`'s) single source handle is `id='result'` (not `'output'`); `OutputNode`'s single target handle is `id='value'`. This plan's code and tests below use these exact ids.

## Review Focus

- Dropping a request that already has a node elsewhere on the canvas must create a **second, independent** node — this is not a duplicate-prevention error; multiple nodes referencing the same saved request is valid (e.g. calling the same "refresh token" request from two branches).
- Creating a connection that would form a cycle is still allowed at the canvas/UI level — live cycle prevention is not required. Cycle rejection happens only at Save (Plan 07's `save_flow`). A test must confirm the UI does **not** block the connection itself.
- The run-event listeners (`onFlowRunStarted`/`onFlowStepCompleted`, subscribed *before* `runFlow` is called — see Task 3's correction note) must be unsubscribed on tab close/unmount and when a new run starts — no leaked listeners accumulating across multiple runs of the same tab.
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

**Corrected in the Plan 08 and Plan 09 reviews:** `<ReactFlow>` and its
`ReactFlowProvider` live in `src/components/flow/FlowCanvas.tsx` (Plan 09).
As built, `FlowCanvas` only renders the provider around a same-file
`FlowCanvasInner`, which owns `<ReactFlow>` and the wrapper
`<div data-testid='flow-canvas'>`. Put the drag/drop handlers on that
wrapper `<div>` inside `FlowCanvasInner`, not in `FlowPane`, and get
`screenToFlowPosition` from `useReactFlow()` there (it is inside the
provider). Do not use `onInit`: it fires on a timer, so a drop right after
render would find no instance. Add an optional
`onAddNode?: (node: FlowNode) => void` prop to `FlowCanvasProps`; `FlowPane`
passes its existing `handleAddNode` (the same one `NodePalette` uses). Add
`src/components/flow/FlowCanvas.tsx` to Step 9's `git add` list:

```tsx
// src/components/flow/FlowCanvas.tsx
import { useReactFlow } from '@xyflow/react';
import { decodeFlowRequestDragPayload } from '@/lib/flow-drag';

// inside FlowCanvasInner, whose props now also include onAddNode:
const { screenToFlowPosition } = useReactFlow();

const handleDragOver = (e: React.DragEvent) => {
  e.preventDefault();
  e.dataTransfer.dropEffect = 'copy';
};

const handleDrop = (e: React.DragEvent) => {
  e.preventDefault();
  const payload = decodeFlowRequestDragPayload(e.dataTransfer);
  if (!payload) return;

  const position = screenToFlowPosition({ x: e.clientX, y: e.clientY });
  onAddNode?.({
    id: crypto.randomUUID(),
    kind: { kind: 'Request', label: payload.name, source: { type: 'Saved', requestPath: payload.path } },
    position,
  });
};

// on the existing wrapper div (not on <ReactFlow>):
<div
  data-testid='flow-canvas'
  className='h-full w-full'
  onDragOver={handleDragOver}
  onDrop={handleDrop}
>
  <ReactFlow /* ...existing Plan 09 props, unchanged */ />
</div>
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
    // The store only updates tabs that are in its tree, so seed baseTab.
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('adds a Saved Request node at the drop position when a sidebar request is dropped', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
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
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.drop(canvas, { dataTransfer: { getData: () => '' } });
    const updatedTab = usePaneStore.getState().root as unknown as FlowTab;
    expect(updatedTab.nodes).toHaveLength(0);
  });
});
```

This test's exact store-lookup lines (`usePaneStore.getState().root /* find
baseTab.id */`) depend on Plan 08's actual pane-store tree shape — adapt them
to however Plan 08 exposes tab lookup (as built: `findTabInTree` is private
to `pane-store.ts`; read the seeded tab back with
`usePaneStore.getState().root` — a single leaf after `reset()` — and
`tabs.find((t) => t.id === baseTab.id)`), keeping the same two assertions
(a matching node was added; a payload-less drop is a no-op).

- [ ] **Step 8: Run tests to verify they pass**

Run: `yarn vitest run src/lib/__tests__/flow-drag.test.ts src/components/flow/__tests__/FlowPane.dragdrop.test.tsx`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/lib/flow-drag.ts src/lib/__tests__/flow-drag.test.ts src/components/collections/RequestNode.tsx src/components/flow/FlowPane.tsx src/components/flow/FlowCanvas.tsx src/components/flow/__tests__/FlowPane.dragdrop.test.tsx
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
import type { FlowNode } from '@/lib/tauri-api';

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

  it('defaults to "response.body" for an Input source node too', () => {
    // The backend exposes an Input node's value as response.body. A bare
    // `value` is not bound and would fail the run.
    expect(defaultExpressionFor(inputSource)).toBe('response.body');
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
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

// Every source kind is evaluated as a response-shaped object. An Input
// node's value is its `response.body` (see resolve_flow_wire_expression).
// The node argument is kept so a later per-kind default is a local change.
export function defaultExpressionFor(_sourceNode: FlowNode): string {
  return 'response.body';
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
import { Label } from '@/components/ui/label';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

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
      // Address the header by name, never by index: the frontend does not
      // know a Saved request's header order. Plan 05's apply_wired_overrides
      // treats a non-numeric selector as a case-insensitive name, updating
      // the matching header or appending it if absent. Reuse an existing
      // inline header's spelling when one matches.
      const existing = existingHeaders.find((h) => h.toLowerCase() === trimmed.toLowerCase());
      targetField = `headers[${existing ?? trimmed}].value`;
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
            <Label className='text-xs font-medium'>Header name</Label>
            <Input
              value={headerName}
              onChange={(e) => setHeaderName(e.target.value)}
              placeholder='e.g. Authorization'
              className='h-8 text-sm'
            />
          </div>
        )}
        <div>
          <Label className='text-xs font-medium'>Value from source</Label>
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
import { useRef, useState } from 'react';
import { buildEdgeFromConnection } from '@/lib/flow-wiring';
import { WireExpressionPopover } from './WireExpressionPopover';
import type { Connection } from '@xyflow/react';

// inside FlowPane, alongside the updateFlowNodes selector from Task 1:
const updateFlowEdges = usePaneStore((s) => s.updateFlowEdges);
const [pendingEdge, setPendingEdge] = useState<FlowEdge | null>(null);
// Set by onCommit, so closing the popover can tell a commit from a cancel.
const committedEdgeIdRef = useRef<string | null>(null);

const handleConnect = (connection: Connection) => {
  const sourceNode = tab.nodes.find((n) => n.id === connection.source);
  if (!sourceNode) return;
  const edge = buildEdgeFromConnection(connection, sourceNode);
  if (!edge) return;
  updateFlowEdges(tab.id, [...tab.edges, edge]);
  setPendingEdge(edge); // opens the popover immediately, per spec §6
};

// on the existing <FlowCanvas ...> element (FlowPane never renders <ReactFlow> itself):
<FlowCanvas onConnect={handleConnect} /* ...existing props */ />

{pendingEdge && (
  <WireExpressionPopover
    edge={pendingEdge}
    targetNode={tab.nodes.find((n) => n.id === pendingEdge.targetNodeId)!}
    open={pendingEdge !== null}
    onOpenChange={(open) => {
      if (open) return;
      // A bare `headers` target is not a valid target_field (the backend
      // rejects it). If the popover closes without a commit, drop that edge
      // instead of leaving it to fail the run.
      if (committedEdgeIdRef.current !== pendingEdge.id && pendingEdge.targetField === 'headers') {
        updateFlowEdges(tab.id, tab.edges.filter((e) => e.id !== pendingEdge.id));
      }
      setPendingEdge(null);
    }}
    onCommit={(updated) => {
      committedEdgeIdRef.current = updated.id;
      updateFlowEdges(tab.id, tab.edges.map((e) => (e.id === updated.id ? updated : e)));
    }}
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
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

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
- Create: `src/components/flow/FlowToolbar.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Modify: `src/types/pane-types.ts` (add `runId?: string` and `nodeDetail?: Record<string, { statusCode?: number; durationMs?: number; error?: string }>` to `FlowTab` — neither exists in Plan 08's `FlowTab`, both are needed here to correlate streamed events to the active run and to show status/timing on each node)
- Modify: `src/stores/pane-store.ts` (add `setFlowRunState`; widen `patchFlowNodeStatus` with an optional `detail` parameter)
- Modify: `src/components/flow/FlowCanvas.tsx` (accept `nodeDetail` and `cycleNodeIds` props and spread `...nodeDetail?.[n.id]` plus `hasCycleError` into each node's `data` in `toRfNodes` — added by the Plan 08 review; without it the status-code/timing line never reaches the node components)
- Modify: `src/stores/__tests__/pane-store.test.ts` (add the two new tests in Step 4, in the existing `'Flow tab actions'` block)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: `runFlow`, `cancelFlowRun`, `saveFlow`, `onFlowRunStarted`, `onFlowStepCompleted` and the `FlowRunSummary`/`FlowStepCompletedEvent` types (all Plan 08 bindings over Plan 07's Tauri commands); `updateFlowNodes`/`updateFlowEdges` (Plan 08, for reference — not called directly in this task).
- Produces: `FlowToolbar`; `setFlowRunState` and the widened `patchFlowNodeStatus` (added to `pane-store.ts`) — terminal deliverables of this plan; no later plan consumes them.

**Corrected in the Plan 07 post-implementation review.** This task was first
written against the plan index's original sketch, where `run_flow` returned
a `run_id` at once. The as-built `run_flow` instead stays pending until the
run **ends** and then resolves with a `FlowRunSummary`. Every
`flow-step-completed` event is emitted *before* that promise resolves. So a
toolbar that awaits `runFlow` first and subscribes afterwards never sees a
single event. The corrected lifecycle is:

1. Subscribe to `flow-run-started` and `flow-step-completed` **before**
   calling `runFlow`.
2. Take the `run_id` from the first `flow-run-started` event whose
   `collection` and `flow_name` match. Only then is Stop enabled.
3. Filter step events by that `run_id`. Event payloads are snake_case
   (`run_id`, `node_id`, `status_code`, `duration_ms`) — see Plan 08.
4. When `runFlow` resolves, apply `summary.steps` once more as the
   authoritative final state. Tauri does not promise that event delivery
   finishes before the command response arrives. Then mark the run done
   and unsubscribe.
5. If `runFlow` rejects (flow not found, stored cycle, secret fetch
   failure), no event was emitted. Show the error and mark the run done.

The event bindings already exist from Plan 08 Task 1. This task does not
add or redeclare any `tauri-api.ts` binding.

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
    onFlowRunStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
  };
});

const onPatchStatus = vi.fn();
const onRunStateChange = vi.fn();

type StartedHandler = Parameters<typeof tauriApi.onFlowRunStarted>[0];
type StepHandler = Parameters<typeof tauriApi.onFlowStepCompleted>[0];

let startedHandler: StartedHandler | undefined;
let stepHandler: StepHandler | undefined;
let resolveRun: (summary: tauriApi.FlowRunSummary) => void = () => {};

const started = (runId: string, flowName = 'my-flow') =>
  startedHandler?.({
    type: 'flowRunStarted',
    run_id: runId,
    flow_name: flowName,
    collection: 'my-collection',
    total_nodes: 1,
  });

const renderToolbar = () =>
  render(
    <FlowToolbar
      collection='my-collection'
      flowName='my-flow'
      environmentName={null}
      onPatchStatus={onPatchStatus}
      onRunStateChange={onRunStateChange}
    />,
  );

describe('FlowToolbar', () => {
  beforeEach(() => {
    startedHandler = undefined;
    stepHandler = undefined;
    vi.mocked(tauriApi.onFlowRunStarted).mockImplementation(async (h) => {
      startedHandler = h;
      return () => {};
    });
    vi.mocked(tauriApi.onFlowStepCompleted).mockImplementation(async (h) => {
      stepHandler = h;
      return () => {};
    });
    // run_flow stays pending until the test resolves it, like the real backend.
    vi.mocked(tauriApi.runFlow).mockImplementation(
      () => new Promise((resolve) => {
        resolveRun = resolve;
      }),
    );
    vi.mocked(tauriApi.saveFlow).mockResolvedValue(undefined);
    vi.mocked(tauriApi.cancelFlowRun).mockResolvedValue(undefined);
    onPatchStatus.mockClear();
    onRunStateChange.mockClear();
    vi.mocked(tauriApi.cancelFlowRun).mockClear();
  });

  it('subscribes before running, takes the run id from flow-run-started, and finishes on resolve', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null));
    expect(startedHandler).toBeDefined();
    expect(stepHandler).toBeDefined();

    started('run-123');
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');

    resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
    await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-123'));
  });

  it('ignores a flow-run-started event for a different flow', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-other', 'some-other-flow');
    expect(onRunStateChange).not.toHaveBeenCalled();
  });

  it('a step-completed event for a different run id is ignored', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'some-other-run',
      node_id: 'node-a',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
    });
    expect(onPatchStatus).not.toHaveBeenCalled();
  });

  it('applies the returned summary steps as the final state', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    resolveRun({
      runId: 'run-123',
      steps: [{ nodeId: 'n1', status: 'success', statusCode: 200, durationMs: 184, error: null }],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenCalledWith('n1', 'success', {
        statusCode: 200,
        durationMs: 184,
        error: undefined,
      }),
    );
  });

  it('Stop calls cancelFlowRun with the active run id', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-123');
  });

  it('Stop is a no-op when no run is active', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: FAIL — `FlowToolbar` does not exist yet.

- [ ] **Step 3: Confirm the Plan 08 bindings are present**

`onFlowRunStarted`, `onFlowStepCompleted`, `onFlowRunFinished`, `runFlow`,
`cancelFlowRun`, `saveFlow` and the `FlowRunSummary`/event types were all
added by Plan 08 Task 1. Check they exist in `src/lib/tauri-api.ts`. Do not
add a second copy of any of them.

- [ ] **Step 4: Extend pane-store with `setFlowRunState` and a widened `patchFlowNodeStatus`**

Plan 08's `patchFlowNodeStatus(tabId, nodeId, status)` has no way to carry a
status code/timing/error, and there is no action at all for persisting
`runState`/`runId` on the tab (Plan 08 doesn't need one; this plan does).
Both additions follow Plan 08's exact `updateTabInTree`-based shape.

Add to the existing `describe('Flow tab actions', ...)` block in
`src/stores/__tests__/pane-store.test.ts`:

```typescript
it('setFlowRunState stores the run id and state on the tab', async () => {
  await usePaneStore.getState().openFlowTab('my-collection');
  const tabId = findFirstFlowTab()?.id;
  if (!tabId) throw new Error('Expected a flow tab');
  usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-123');
  const tab = findFirstFlowTab();
  expect(tab?.runState).toBe('running');
  expect(tab?.runId).toBe('run-123');
});

it('patchFlowNodeStatus records optional detail alongside the status', async () => {
  await usePaneStore.getState().openFlowTab('my-collection');
  const tabId = findFirstFlowTab()?.id;
  if (!tabId) throw new Error('Expected a flow tab');
  usePaneStore.setState({
    root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
      tab.tabType === 'flow'
        ? {
            ...tab,
            nodes: [{ id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
          }
        : tab,
    ),
  });
  usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', {
    statusCode: 200,
    durationMs: 184,
  });
  const tab = findFirstFlowTab();
  expect(tab?.nodeStatus.n1).toBe('success');
  expect(tab?.nodeDetail?.n1).toEqual({ statusCode: 200, durationMs: 184 });
});
```

(`findFirstFlowTab`/`updateTabInTreeForTest` are the helpers Plan 08 already
added at the top of this test file. Reuse them; don't add a second helper.)

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
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  cancelFlowRun,
  onFlowRunStarted,
  onFlowStepCompleted,
  runFlow,
} from '@/lib/tauri-api';
import type { UnlistenFn } from '@tauri-apps/api/event';

type NodeDetail = { statusCode?: number; durationMs?: number; error?: string };

interface FlowToolbarProps {
  collection: string;
  flowName: string;
  environmentName: string | null;
  onPatchStatus: (nodeId: string, status: string, detail?: NodeDetail) => void;
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

  // Unsubscribe when the tab closes mid-run.
  useEffect(() => cleanupListeners, []);

  const handleRun = async () => {
    cleanupListeners();
    // Held in a local, not state, so the event handlers see it at once.
    let runId: string | null = null;

    // Subscribe first. run_flow only resolves when the run ends, so every
    // event is emitted while its promise is still pending.
    const unlistenStarted = await onFlowRunStarted((event) => {
      if (runId !== null) return;
      if (event.collection !== collection || event.flow_name !== flowName) return;
      runId = event.run_id;
      setActiveRunId(event.run_id);
      onRunStateChange('running', event.run_id);
    });
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, event.status, {
        statusCode: event.status_code ?? undefined,
        durationMs: event.duration_ms ?? undefined,
        error: event.error ?? undefined,
      });
    });
    unlistenRefs.current = [unlistenStarted, unlistenStep];

    try {
      const summary = await runFlow(collection, flowName, environmentName);
      // The summary is the authoritative final state. Event delivery is not
      // guaranteed to finish before the command response arrives.
      for (const step of summary.steps) {
        onPatchStatus(step.nodeId, step.status, {
          statusCode: step.statusCode ?? undefined,
          durationMs: step.durationMs ?? undefined,
          error: step.error ?? undefined,
        });
      }
      onRunStateChange('done', summary.runId);
    } catch (err) {
      // A run that cannot start rejects before any event is emitted.
      toast.error(`Could not run flow: ${String(err)}`);
      onRunStateChange('done');
    } finally {
      setActiveRunId(null);
      cleanupListeners();
    }
  };

  const handleStop = () => {
    if (!activeRunId) return;
    void cancelFlowRun(activeRunId);
  };

  return (
    <div className='flex items-center gap-2'>
      <Button size='sm' onClick={() => void handleRun()} disabled={activeRunId !== null}>
        Run
      </Button>
      <Button size='sm' variant='outline' onClick={handleStop}>
        Stop
      </Button>
    </div>
  );
}
```

Known limit: if the same flow is started from two tabs at once, the
`flow-run-started` match by collection and flow name can pick up the other
tab's run id. Phase 1 accepts this. The returned summary still carries the
correct final state for each tab.

- [ ] **Step 6: Run tests to verify they pass**

Run: `yarn vitest run src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS — 6 tests.

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
    // Plan 07's save_flow rejects with the plain string
    // "Invalid input: flow contains a cycle through node(s): a, b"
    // (ids joined by ", ", no brackets or quotes — verified in the Plan 07
    // review). Parse and flag them rather than showing only a generic
    // toast, per this plan's Review Focus.
    const message = String(err);
    const match = message.match(/flow contains a cycle through node\(s\): (.*)$/);
    if (match) {
      setCycleNodeIds(match[1].split(', ').map((s) => s.trim()));
    }
    toast.error(`Could not save flow: ${message}`);
  }
};

// alongside the updateFlowNodes/updateFlowEdges selectors from earlier tasks:
const patchFlowNodeStatus = usePaneStore((s) => s.patchFlowNodeStatus);
const setFlowRunState = usePaneStore((s) => s.setFlowRunState);
// There is no `activeEnvironmentName` anywhere. The active environment's
// name is env-store's `activeEnvId` (it holds the name; see
// src/lib/execute-request.ts, which passes it as environmentName).
const activeEnvironmentName = useEnvStore((s) => s.activeEnvId);

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

(Import `useEnvStore` from `@/stores/env-store`.)

Pass `cycleNodeIds` to `FlowCanvas` as a new optional `cycleNodeIds?: string[]`
prop. In `toRfNodes`, set `hasCycleError: cycleNodeIds.includes(n.id)` in each
node's `data`, next to the `...nodeDetail?.[n.id]` spread. As built after the
Plan 09 review, all three node components already accept
`data.hasCycleError` and draw a red ring for it, so this plan only supplies
which node ids are implicated. Keep `toRfNodes`'s existing `selectedIds` and
`measured` arguments when adding these; see Global Constraints.

- [ ] **Step 8: Verify the app builds**

Run: `yarn tsc --noEmit`
Expected: succeeds — no type errors across `FlowToolbar.tsx`,
`pane-store.ts`'s widened action, and
`FlowPane.tsx`'s new wiring.

- [ ] **Step 9: Commit**

```bash
git add src/components/flow/FlowToolbar.tsx src/components/flow/FlowPane.tsx src/components/flow/FlowCanvas.tsx src/components/flow/__tests__/FlowToolbar.test.tsx src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
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
> `src/components/flow/FlowToolbar.tsx`,
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
>    `flow-canvas` test id actually exists on `FlowCanvasInner`'s wrapper
>    `<div>` (which also carries the drop handlers).
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
