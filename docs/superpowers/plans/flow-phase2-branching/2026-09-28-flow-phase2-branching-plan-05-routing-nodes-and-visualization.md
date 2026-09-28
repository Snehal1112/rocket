# Flow Phase 2 — Plan 05: Routing Nodes and Run Visualization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users add, edit and wire If/Switch nodes on the Flow canvas, and show run results for every node kind: status styling, skip-reason captions, the chosen exit, and taken/not-taken edges.

**Architecture:**
- A pure status helper (`nodeStatus.ts`) replaces RequestNode's private `statusStyles`. It is applied to every node kind.
- Node components edit their own `kind` through a small React context (`FlowNodeActionsContext`). `FlowCanvas` provides the context from two new props. `FlowPane` implements them with pure graph-edit functions and a new single-update store action, `updateFlowGraph`.
- A pure `flowExits.ts` module is the one place that turns an exit handle into its label, and that decides whether an edge was taken in the last run. `FlowCanvas.toRfEdges`, the If/Switch badges and the edge styling all use it.

**Tech Stack:** React 19 + TypeScript, `@xyflow/react` 12, Zustand (`pane-store`), shadcn/ui (`Button`, `Input`, `Badge`), `lucide-react`, CodeMirror `SingleLineEditor`, Vitest + Testing Library + `@testing-library/user-event`.

**Spec:** `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` §9.1, §9.3, §9.4, §13. Read it together with this plan.

## Global Constraints

- UI uses **shadcn/ui primitives only**: no raw `<button>`, `<input>`, `<select>`, `<form>` or `<dialog>` in components. Raw elements are allowed only inside test mocks.
- Icons come from `lucide-react` only: `GitBranch` for If, `Split` for Switch, `X` to remove a case, `Plus` to add one. No inline SVGs.
- Single-line expression fields (If `condition`, Switch `value`) use `SingleLineEditor` from `@/components/editor`, never Monaco.
- Zustand: use narrow selectors (`usePaneStore((s) => s.x)`), and never fully destructure store state at the top of a component.
- Handle and exit names come from `src/lib/flow-handles.ts` (plan 04): `RESULT_HANDLE`, `TRUE_HANDLE`, `FALSE_HANDLE`, `DEFAULT_HANDLE`, `INPUT_HANDLE`, `TRIGGER_HANDLE`, `caseHandle(id)`, `caseIdFromHandle(handle)`. Never hard-code `'true'`, `'case:'` and similar strings in components.
- `FlowEdge.sourceHandle` is optional. If it is absent, the edge leaves from `RESULT_HANDLE` (spec §8.4).
- Skip captions must read exactly `Skipped — upstream failed` (with an em dash) and `Not taken` (spec §9.3).
- Defaults when a node is created (spec §9.1):
  - If: `condition = "response.status === 200"`.
  - Switch: `value = "response.body.type"` plus one case `{ label: "Case 1", matches: "" }`.
  - A case added later: `{ label: "Case N", matches: "" }`.
- New node placement matches the existing palette: `{ x: 100, y: 100 }` and id `${prefix}-${Date.now()}-${n}`.
- The worktree has no `node_modules`. Run `yarn install` once before the first test run.
- Commit steps use the `dev-workflow-skills:1-git-commit` skill, with conventional-commit subjects.

## Review Focus

1. **Backspace while editing a node field must not delete the node.** Typing Backspace in a Switch case `Input` (or the If condition editor) while that node is selected must edit the text, not remove the node. Covered by the `nokey`/`nodrag` wrapper test in Task 3, Step 1 (`FlowCanvas.routing.test.tsx`).
2. **A new case must be connectable at once.** React Flow only registers handles added after mount when `updateNodeInternals(id)` runs, so SwitchNode must call it whenever the case set changes. Covered in Task 3, Step 1 (`SwitchNode.test.tsx`, "refreshes node internals when cases change").
3. **Removing a wired case leaves no dangling edge.** The case and every edge on `case:<id>` must disappear in one store update, leaving no edge pointing at a missing handle. Covered in Task 2, Step 1 (`flow-graph-edits.test.ts` and the `updateFlowGraph` single-notification store test).
4. **Two fresh cases share `matches: ""`.** Backend rule V7 rejects that on save, so the node must flag the clash inline before the user hits Save. Covered in Task 3, Step 1 (`SwitchNode.test.tsx`, "flags duplicate match values").
5. **Re-running must not show stale branch highlighting.** While a new run is `running`, or before a routing node completes, its exits must render neutral, not the previous run's taken/not-taken styling. Covered in Task 4, Step 1 (`flowExits.test.ts`, "is neutral while the routing node is running or has no branch").

---

### Task 1: Shared node status styling and skip captions

**Files:**
- Create: `src/components/flow/nodes/nodeStatus.ts`
- Create: `src/components/flow/nodes/NodeStatusCaption.tsx`
- Modify: `src/components/flow/nodes/RequestNode.tsx` (remove the local `statusStyles` at lines 26-32 and use the helper; add `skipReason` to `RequestNodeData`)
- Modify: `src/components/flow/nodes/InputNode.tsx`
- Modify: `src/components/flow/nodes/OutputNode.tsx`
- Test: `src/components/flow/nodes/__tests__/nodeStatus.test.ts` (create)
- Test: `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx` (extend)
- Test: `src/components/flow/nodes/__tests__/RequestNode.test.tsx` (extend)

**Interfaces:**
- Consumes (plan 04): `FlowSkipReason` from `@/lib/tauri-api` (`'upstream_failed' | 'branch_not_taken'`). `FlowCanvas.toRfNodes` already spreads `nodeDetail[id]` (which includes `skipReason`/`branch`) into node `data`.
- Produces:
  - `nodeStatusClassName(status: FlowNodeStatus, skipReason?: FlowSkipReason): string`
  - `nodeStatusCaption(status: FlowNodeStatus, detail?: { skipReason?: FlowSkipReason }): string | null`
  - `<NodeStatusCaption status={FlowNodeStatus} skipReason?={FlowSkipReason} />`, which renders `data-testid="node-status-caption"` or nothing.
  - Every node card carries `data-status={status}`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/nodes/__tests__/nodeStatus.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { nodeStatusCaption, nodeStatusClassName } from '../nodeStatus';

describe('nodeStatusClassName', () => {
  it('keeps the existing per-status styles', () => {
    expect(nodeStatusClassName('idle')).toBe('border-border');
    expect(nodeStatusClassName('success')).toContain('border-green-500');
    expect(nodeStatusClassName('failed')).toContain('border-red-500');
    expect(nodeStatusClassName('running')).toContain('animate-pulse');
  });

  it('fades an upstream-failed skip without a dashed border', () => {
    const cls = nodeStatusClassName('skipped', 'upstream_failed');
    expect(cls).toContain('opacity-60');
    expect(cls).not.toContain('border-dashed');
  });

  it('treats a skip with no reason like an upstream-failed skip', () => {
    expect(nodeStatusClassName('skipped')).toBe(nodeStatusClassName('skipped', 'upstream_failed'));
  });

  it('fades and dashes a not-taken skip', () => {
    const cls = nodeStatusClassName('skipped', 'branch_not_taken');
    expect(cls).toContain('border-dashed');
    expect(cls).toContain('opacity-50');
  });
});

describe('nodeStatusCaption', () => {
  it('has no caption unless the node was skipped', () => {
    expect(nodeStatusCaption('idle')).toBeNull();
    expect(nodeStatusCaption('success', { skipReason: 'branch_not_taken' })).toBeNull();
  });

  it('names each skip reason', () => {
    expect(nodeStatusCaption('skipped', { skipReason: 'upstream_failed' })).toBe(
      'Skipped — upstream failed',
    );
    expect(nodeStatusCaption('skipped', { skipReason: 'branch_not_taken' })).toBe('Not taken');
    expect(nodeStatusCaption('skipped')).toBe('Skipped — upstream failed');
  });
});
```

Append to `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`:

```tsx
describe('run status on Input/Output nodes', () => {
  const props = {
    selected: false,
    dragging: false,
    zIndex: 0,
    isConnectable: true,
    draggable: true,
    selectable: true,
    deletable: true,
    positionAbsoluteX: 0,
    positionAbsoluteY: 0,
  };

  it('marks an Input node with its status', () => {
    wrap(
      <InputNode
        {...props}
        id='i1'
        type='Input'
        data={{ kind: { kind: 'Input', label: 'Key', value: 'k' }, status: 'success' }}
      />,
    );
    expect(screen.getByTestId('input-node-card')).toHaveAttribute('data-status', 'success');
  });

  it('captions a not-taken Output node', () => {
    wrap(
      <OutputNode
        {...props}
        id='o1'
        type='Output'
        data={{
          kind: { kind: 'Output', label: 'Result' },
          status: 'skipped',
          skipReason: 'branch_not_taken',
        }}
      />,
    );
    const card = screen.getByTestId('output-node-card');
    expect(card).toHaveAttribute('data-status', 'skipped');
    expect(card).toHaveClass('border-dashed');
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent('Not taken');
  });
});
```

Append inside the `describe('RequestNode', …)` block in `RequestNode.test.tsx`:

```tsx
  it('captions an upstream-failed skip and a not-taken skip differently', () => {
    const { rerender } = renderNode({
      kind: baseKind,
      status: 'skipped',
      skipReason: 'upstream_failed',
    });
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent(
      'Skipped — upstream failed',
    );
    expect(screen.getByTestId('request-node-card')).not.toHaveClass('border-dashed');

    rerender(nodeElement({ kind: baseKind, status: 'skipped', skipReason: 'branch_not_taken' }));
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent('Not taken');
    expect(screen.getByTestId('request-node-card')).toHaveClass('border-dashed');
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn install && yarn test src/components/flow/nodes`
Expected: FAIL. `nodeStatus.test.ts` fails with "Failed to resolve import '../nodeStatus'", and the new Input/Output/Request cases fail on the missing `data-status`/caption (or type errors on `skipReason`).

- [ ] **Step 3: Write the implementation**

Create `src/components/flow/nodes/nodeStatus.ts`:

```ts
import type { FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';

const statusStyles: Record<FlowNodeStatus, string> = {
  idle: 'border-border',
  running: 'border-blue-400 shadow-[0_0_0_1px_rgba(96,165,250,0.5)] animate-pulse',
  success: 'border-green-500 shadow-[0_0_0_1px_rgba(34,197,94,0.5)]',
  failed: 'border-red-500 shadow-[0_0_0_1px_rgba(239,68,68,0.5)]',
  skipped: 'border-muted-foreground/40 opacity-60',
};

const notTakenStyle = 'border-dashed border-muted-foreground/40 opacity-50';

export function nodeStatusClassName(status: FlowNodeStatus, skipReason?: FlowSkipReason): string {
  if (status === 'skipped' && skipReason === 'branch_not_taken') return notTakenStyle;
  return statusStyles[status];
}

// A skip with no reason predates Phase 2 (it can only mean an upstream failure).
export function nodeStatusCaption(
  status: FlowNodeStatus,
  detail?: { skipReason?: FlowSkipReason },
): string | null {
  if (status !== 'skipped') return null;
  return detail?.skipReason === 'branch_not_taken' ? 'Not taken' : 'Skipped — upstream failed';
}
```

Create `src/components/flow/nodes/NodeStatusCaption.tsx`:

```tsx
import type { FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { nodeStatusCaption } from './nodeStatus';

export function NodeStatusCaption({
  status,
  skipReason,
}: {
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
}) {
  const caption = nodeStatusCaption(status, { skipReason });
  if (!caption) return null;
  return (
    <div data-testid='node-status-caption' className='px-2 pt-1 italic text-muted-foreground'>
      {caption}
    </div>
  );
}
```

In `RequestNode.tsx`:
- Delete the `statusStyles` constant.
- Add imports: `import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';`, `import { NodeStatusCaption } from './NodeStatusCaption';` and `import { nodeStatusClassName } from './nodeStatus';`.
- Add `skipReason?: FlowSkipReason;` to `RequestNodeData`.
- In the card `className`, replace `statusStyles[status],` with `nodeStatusClassName(status, data.skipReason),`.
- Insert the caption directly after the failed-status block (after line 70):

```tsx
      <NodeStatusCaption status={status} skipReason={data.skipReason} />
```

Replace `InputNode.tsx` with:

```tsx
import { Handle, type NodeProps, Position } from '@xyflow/react';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export interface InputNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Input' }>;
  status: FlowNodeStatus;
  skipReason?: FlowSkipReason;
  /** Set when a save was rejected because this node is part of a cycle. */
  hasCycleError?: boolean;
}

export function InputNode({ data, isConnectable }: NodeProps & { data: InputNodeData }) {
  const value = data.kind.value;
  const display = value === undefined || value === null ? '—' : String(value);
  return (
    <div
      data-testid='input-node-card'
      data-status={data.status}
      className={cn(
        'w-48 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(data.status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <div className='border-b px-2 py-1.5 font-medium'>{data.kind.label}</div>
      <NodeStatusCaption status={data.status} skipReason={data.skipReason} />
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

In `OutputNode.tsx`, apply the same three changes and keep plan 04's handles exactly as they are (the `value` target handle plus the `trigger` "Run when" handle that plan 04 added):
- Add `skipReason?: FlowSkipReason;` to `OutputNodeData`, and import `FlowSkipReason`, `NodeStatusCaption` and `nodeStatusClassName`.
- On the card `div`, add `data-status={data.status}`, and add `nodeStatusClassName(data.status, data.skipReason),` inside `cn(…)` before the cycle ring.
- Insert `<NodeStatusCaption status={data.status} skipReason={data.skipReason} />` directly after the label `div`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow/nodes && yarn tsc --noEmit`
Expected: PASS. Every existing RequestNode/Input/Output test stays green, and `tsc` reports no errors.

- [ ] **Step 5: Commit**

Stage `src/components/flow/nodes/` and commit using the `dev-workflow-skills:1-git-commit` skill.
Suggested subject: `feat(flow): show run status and skip reason on every node`

---

### Task 2: Node edit actions: context, graph-edit helpers, single-update store action

**Files:**
- Create: `src/components/flow/nodes/FlowNodeActionsContext.tsx`
- Create: `src/lib/flow-graph-edits.ts`
- Modify: `src/stores/pane-store.ts` (interface near line 229, implementation after `updateFlowEdges` near line 772)
- Modify: `src/components/flow/FlowCanvas.tsx` (props and provider)
- Modify: `src/components/flow/FlowPane.tsx` (implements the two callbacks)
- Test: `src/lib/__tests__/flow-graph-edits.test.ts` (create)
- Test: `src/stores/__tests__/pane-store.test.ts` (extend the "Flow tab actions" suite at line 833)

**Interfaces:**
- Consumes (plan 04): `caseHandle(id: string): string` from `@/lib/flow-handles`; `SwitchCase` and `FlowNodeKind` (with `If`/`Switch` variants) from `@/lib/tauri-api`.
- Produces:
  - `interface FlowNodeActions { updateNodeKind(nodeId: string, kind: FlowNodeKind): void; removeSwitchCase(nodeId: string, caseId: string): void }`.
  - `FlowNodeActionsContext` (its default value is a no-op) and `useFlowNodeActions(): FlowNodeActions`.
  - `replaceNodeKind(nodes: FlowNode[], nodeId: string, kind: FlowNodeKind): FlowNode[]`.
  - `removeSwitchCase(nodes: FlowNode[], edges: FlowEdge[], nodeId: string, caseId: string): { nodes: FlowNode[]; edges: FlowEdge[] } | null`.
  - Store action `updateFlowGraph(tabId: string, nodes: FlowNode[], edges: FlowEdge[]): void`.
  - New optional `FlowCanvas` props `onNodeKindChange?: (nodeId: string, kind: FlowNodeKind) => void` and `onRemoveSwitchCase?: (nodeId: string, caseId: string) => void`.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-graph-edits.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { removeSwitchCase, replaceNodeKind } from '@/lib/flow-graph-edits';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

const switchNode: FlowNode = {
  id: 'sw1',
  position: { x: 0, y: 0 },
  kind: {
    kind: 'Switch',
    label: 'Plan',
    value: 'response.body.plan',
    cases: [
      { id: 'c1', label: 'Free', matches: 'free' },
      { id: 'c2', label: 'Pro', matches: 'pro' },
    ],
  },
};
const other: FlowNode = { id: 'o1', kind: { kind: 'Output', label: 'Out' }, position: { x: 1, y: 1 } };

const edges: FlowEdge[] = [
  { id: 'toFree', sourceNodeId: 'sw1', sourceHandle: 'case:c1', targetNodeId: 'o1', targetField: 'trigger', expression: '' },
  { id: 'toPro', sourceNodeId: 'sw1', sourceHandle: 'case:c2', targetNodeId: 'o1', targetField: 'trigger', expression: '' },
  { id: 'unrelated', sourceNodeId: 'o1', targetNodeId: 'sw1', targetField: 'input', expression: '' },
];

describe('replaceNodeKind', () => {
  it('swaps only the target node kind and keeps position and id', () => {
    const next = replaceNodeKind([switchNode, other], 'o1', { kind: 'Output', label: 'Renamed' });
    expect(next[0]).toBe(switchNode);
    expect(next[1]).toEqual({ ...other, kind: { kind: 'Output', label: 'Renamed' } });
  });
});

describe('removeSwitchCase', () => {
  it('drops the case and exactly the edges leaving its handle', () => {
    const result = removeSwitchCase([switchNode, other], edges, 'sw1', 'c1');
    expect(result).not.toBeNull();
    const sw = result?.nodes.find((n) => n.id === 'sw1');
    expect(sw?.kind.kind === 'Switch' && sw.kind.cases.map((c) => c.id)).toEqual(['c2']);
    expect(result?.edges.map((e) => e.id)).toEqual(['toPro', 'unrelated']);
  });

  it('returns null when the node is missing or is not a Switch', () => {
    expect(removeSwitchCase([other], edges, 'sw1', 'c1')).toBeNull();
    expect(removeSwitchCase([other], edges, 'o1', 'c1')).toBeNull();
  });
});
```

Append inside `describe('Flow tab actions', …)` in `src/stores/__tests__/pane-store.test.ts`:

```ts
  it('updateFlowGraph replaces nodes and edges in a single store update', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    const node: FlowNode = { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } };
    const listener = vi.fn();
    const unsubscribe = usePaneStore.subscribe(listener);

    usePaneStore.getState().updateFlowGraph(tabId, [node], []);
    unsubscribe();

    expect(listener).toHaveBeenCalledTimes(1);
    const tab = findFirstFlowTab();
    expect(tab?.nodes).toEqual([node]);
    expect(tab?.edges).toEqual([]);
    expect(tab?.isDirty).toBe(true);
  });
```

If `FlowNode` is not already imported in that test file, add it to the existing `import type { … }` block (the one around line 5).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/lib/__tests__/flow-graph-edits.test.ts src/stores/__tests__/pane-store.test.ts`
Expected: FAIL. "Failed to resolve import '@/lib/flow-graph-edits'", and `updateFlowGraph is not a function`.

- [ ] **Step 3: Write the implementation**

Create `src/lib/flow-graph-edits.ts`:

```ts
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';

export function replaceNodeKind(nodes: FlowNode[], nodeId: string, kind: FlowNodeKind): FlowNode[] {
  return nodes.map((n) => (n.id === nodeId ? { ...n, kind } : n));
}

// Removes the case and every wire leaving its exit, so no edge is left
// pointing at a handle that no longer exists.
export function removeSwitchCase(
  nodes: FlowNode[],
  edges: FlowEdge[],
  nodeId: string,
  caseId: string,
): { nodes: FlowNode[]; edges: FlowEdge[] } | null {
  const node = nodes.find((n) => n.id === nodeId);
  if (!node || node.kind.kind !== 'Switch') return null;
  const kind: FlowNodeKind = {
    ...node.kind,
    cases: node.kind.cases.filter((c) => c.id !== caseId),
  };
  const handle = caseHandle(caseId);
  return {
    nodes: replaceNodeKind(nodes, nodeId, kind),
    edges: edges.filter((e) => !(e.sourceNodeId === nodeId && e.sourceHandle === handle)),
  };
}
```

Create `src/components/flow/nodes/FlowNodeActionsContext.tsx`:

```tsx
import { createContext, useContext } from 'react';
import type { FlowNodeKind } from '@/lib/tauri-api';

export interface FlowNodeActions {
  updateNodeKind: (nodeId: string, kind: FlowNodeKind) => void;
  removeSwitchCase: (nodeId: string, caseId: string) => void;
}

const noop = () => {};

// The default is a no-op, so a node rendered outside a canvas (for example in
// a unit test) stays inert instead of throwing.
export const FlowNodeActionsContext = createContext<FlowNodeActions>({
  updateNodeKind: noop,
  removeSwitchCase: noop,
});

export function useFlowNodeActions(): FlowNodeActions {
  return useContext(FlowNodeActionsContext);
}
```

In `src/stores/pane-store.ts`, add to the interface after `updateFlowEdges`:

```ts
  /** Replaces nodes and edges together, so dependent edits land in one update. */
  updateFlowGraph: (tabId: string, nodes: FlowNode[], edges: FlowEdge[]) => void;
```

Then add the implementation after `updateFlowEdges(tabId, edges) { … },`:

```ts
  updateFlowGraph(tabId, nodes, edges) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, nodes, edges, isDirty: true } : tab,
      ),
    });
  },
```

In `src/components/flow/FlowCanvas.tsx`:
- Import `FlowNodeKind` (add it to the existing `@/lib/tauri-api` type import) and `FlowNodeActionsContext, type FlowNodeActions` from `./nodes/FlowNodeActionsContext`.
- Add to `FlowCanvasProps`:

```ts
  // Inline edits from routing nodes (If condition, Switch value and cases).
  onNodeKindChange?: (nodeId: string, kind: FlowNodeKind) => void;
  // Removes a Switch case together with the edges leaving its exit.
  onRemoveSwitchCase?: (nodeId: string, caseId: string) => void;
```

- Destructure `onNodeKindChange` and `onRemoveSwitchCase` in `FlowCanvasInner`. Add `useMemo` is already imported. Build the context value next to `rfEdges`:

```ts
  const nodeActions = useMemo<FlowNodeActions>(
    () => ({
      updateNodeKind: (nodeId, kind) => onNodeKindChange?.(nodeId, kind),
      removeSwitchCase: (nodeId, caseId) => onRemoveSwitchCase?.(nodeId, caseId),
    }),
    [onNodeKindChange, onRemoveSwitchCase],
  );
```

- Wrap the `<ReactFlow …>…</ReactFlow>` element in `<FlowNodeActionsContext.Provider value={nodeActions}> … </FlowNodeActionsContext.Provider>`. The outer drop-target `div` stays outermost.

In `src/components/flow/FlowPane.tsx`:
- Add `const updateFlowGraph = usePaneStore((s) => s.updateFlowGraph);` next to the other selectors.
- Import `removeSwitchCase` and `replaceNodeKind` from `@/lib/flow-graph-edits`, and add `FlowNodeKind` to the `@/lib/tauri-api` type imports.
- After `handleAddNode`, add:

```ts
  const handleNodeKindChange = (nodeId: string, kind: FlowNodeKind) => {
    updateFlowNodes(tab.id, replaceNodeKind(tab.nodes, nodeId, kind));
  };

  const handleRemoveSwitchCase = (nodeId: string, caseId: string) => {
    const next = removeSwitchCase(tab.nodes, tab.edges, nodeId, caseId);
    if (next) updateFlowGraph(tab.id, next.nodes, next.edges);
  };
```

- Pass them to `<FlowCanvas … onNodeKindChange={handleNodeKindChange} onRemoveSwitchCase={handleRemoveSwitchCase} />`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/lib/__tests__/flow-graph-edits.test.ts src/stores/__tests__/pane-store.test.ts src/components/flow && yarn tsc --noEmit`
Expected: PASS, and `tsc` reports no errors.

- [ ] **Step 5: Commit**

Stage the seven files listed under **Files** above and commit using the `dev-workflow-skills:1-git-commit` skill.
Suggested subject: `feat(flow): route inline node edits to the pane store`

---

### Task 3: IfNode and SwitchNode components

**Files:**
- Create: `src/components/flow/flowExits.ts`
- Create: `src/components/flow/nodes/IfNode.tsx`
- Create: `src/components/flow/nodes/SwitchNode.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx` (register `If` and `Switch` in `nodeTypes`, lines 23-27)
- Test: `src/components/flow/__tests__/flowExits.test.ts` (create; Task 4 extends it)
- Test: `src/components/flow/nodes/__tests__/IfNode.test.tsx` (create)
- Test: `src/components/flow/nodes/__tests__/SwitchNode.test.tsx` (create)
- Test: `src/components/flow/__tests__/FlowCanvas.routing.test.tsx` (create; Task 4 extends it)

**Interfaces:**
- Consumes:
  - From Task 1: `nodeStatusClassName` and `NodeStatusCaption`.
  - From Task 2: `useFlowNodeActions` and `FlowNodeActionsContext`.
  - From plan 04: `INPUT_HANDLE`, `TRUE_HANDLE`, `FALSE_HANDLE`, `DEFAULT_HANDLE`, `caseHandle` and `caseIdFromHandle` (returns the case id, or a falsy value when the handle is not a case handle). Also `SwitchCase`, `FlowSkipReason`, and the `If`/`Switch` variants of `FlowNodeKind`.
- Produces:
  - `exitLabel(kind: FlowNodeKind, handle: string): string | undefined`. Returns `'true'`/`'false'` for If exits, `'default'` or the case's current `label` for Switch exits, and `undefined` otherwise.
  - `IfNode` and `IfNodeData`; `SwitchNode` and `SwitchNodeData`.
  - React Flow `nodeTypes` keys `If` and `Switch`.
  - Test ids `if-node-card`, `switch-node-card` and `branch-badge`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/__tests__/flowExits.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { exitLabel } from '../flowExits';

const ifKind = { kind: 'If' as const, label: 'Ok?', condition: 'response.status === 200' };
const switchKind = {
  kind: 'Switch' as const,
  label: 'Plan',
  value: 'response.body.plan',
  cases: [{ id: 'c1', label: 'Pro plan', matches: 'pro' }],
};

describe('exitLabel', () => {
  it('labels If exits', () => {
    expect(exitLabel(ifKind, 'true')).toBe('true');
    expect(exitLabel(ifKind, 'false')).toBe('false');
    expect(exitLabel(ifKind, 'result')).toBeUndefined();
  });

  it('labels Switch exits by the current case label', () => {
    expect(exitLabel(switchKind, 'case:c1')).toBe('Pro plan');
    expect(exitLabel(switchKind, 'default')).toBe('default');
    expect(exitLabel(switchKind, 'case:gone')).toBeUndefined();
  });

  it('has no label for plain node exits', () => {
    expect(exitLabel({ kind: 'Output', label: 'Out' }, 'result')).toBeUndefined();
  });
});
```

Create `src/components/flow/nodes/__tests__/IfNode.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { IfNode, type IfNodeData } from '../IfNode';

// The real CodeMirror editor needs react-query and Tauri mocks. A plain input
// with the same value/onChange contract is enough to test the node.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

const kind = { kind: 'If' as const, label: 'Logged in?', condition: 'response.status === 200' };

function renderIf(data: IfNodeData) {
  const actions = { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn() };
  render(
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <IfNode
          id='if1'
          type='If'
          data={data}
          selected={false}
          dragging={false}
          zIndex={0}
          isConnectable
          draggable
          selectable
          deletable
          positionAbsoluteX={0}
          positionAbsoluteY={0}
        />
      </FlowNodeActionsContext.Provider>
    </ReactFlowProvider>,
  );
  return actions;
}

describe('IfNode', () => {
  it('renders one input handle and true/false exits', () => {
    renderIf({ kind, status: 'idle' });
    const card = screen.getByTestId('if-node-card');
    expect(screen.getByText('Logged in?')).toBeInTheDocument();
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(1);
    expect(card.querySelector('[data-handleid="input"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="true"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="false"]')).toBeInTheDocument();
  });

  it('edits the condition inline through the node actions', () => {
    const actions = renderIf({ kind, status: 'idle' });
    fireEvent.change(screen.getByLabelText('Condition'), {
      target: { value: 'response.status === 201' },
    });
    expect(actions.updateNodeKind).toHaveBeenCalledWith('if1', {
      ...kind,
      condition: 'response.status === 201',
    });
  });

  it('keeps the condition editor out of canvas drag, wheel and key handling', () => {
    renderIf({ kind, status: 'idle' });
    const wrapper = screen.getByLabelText('Condition').closest('.nodrag');
    expect(wrapper).toHaveClass('nowheel');
    expect(wrapper).toHaveClass('nokey');
  });

  it('shows the chosen exit after a successful run', () => {
    renderIf({ kind, status: 'success', branch: 'false' });
    expect(screen.getByTestId('branch-badge')).toHaveTextContent('→ false');
  });

  it('shows the evaluation error when the condition throws', () => {
    renderIf({ kind, status: 'failed', error: 'ReferenceError: x is not defined' });
    expect(screen.getByText(/ReferenceError/)).toBeInTheDocument();
  });

  it('captions a not-taken skip', () => {
    renderIf({ kind, status: 'skipped', skipReason: 'branch_not_taken' });
    expect(screen.getByTestId('node-status-caption')).toHaveTextContent('Not taken');
  });
});
```

Create `src/components/flow/nodes/__tests__/SwitchNode.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { SwitchNode, type SwitchNodeData } from '../SwitchNode';

const { updateNodeInternals } = vi.hoisted(() => ({ updateNodeInternals: vi.fn() }));

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>();
  return { ...actual, useUpdateNodeInternals: () => updateNodeInternals };
});

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

const kind = {
  kind: 'Switch' as const,
  label: 'Plan router',
  value: 'response.body.plan',
  cases: [
    { id: 'c1', label: 'Free', matches: 'free' },
    { id: 'c2', label: 'Pro plan', matches: 'pro' },
  ],
};

function element(data: SwitchNodeData, actions: ReturnType<typeof makeActions>) {
  return (
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <SwitchNode
          id='sw1'
          type='Switch'
          data={data}
          selected={false}
          dragging={false}
          zIndex={0}
          isConnectable
          draggable
          selectable
          deletable
          positionAbsoluteX={0}
          positionAbsoluteY={0}
        />
      </FlowNodeActionsContext.Provider>
    </ReactFlowProvider>
  );
}

function makeActions() {
  return { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn() };
}

describe('SwitchNode', () => {
  it('renders an input handle, one exit per case and a default exit', () => {
    render(element({ kind, status: 'idle' }, makeActions()));
    const card = screen.getByTestId('switch-node-card');
    expect(card.querySelector('[data-handleid="input"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="case:c1"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="case:c2"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="default"]')).toBeInTheDocument();
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(3);
  });

  it('edits the value and a case inline', () => {
    const actions = makeActions();
    render(element({ kind, status: 'idle' }, actions));
    fireEvent.change(screen.getByLabelText('Switch value'), {
      target: { value: 'response.body.tier' },
    });
    expect(actions.updateNodeKind).toHaveBeenLastCalledWith('sw1', {
      ...kind,
      value: 'response.body.tier',
    });
    fireEvent.change(screen.getByLabelText('Case 2 matches'), { target: { value: 'premium' } });
    expect(actions.updateNodeKind).toHaveBeenLastCalledWith('sw1', {
      ...kind,
      cases: [kind.cases[0], { ...kind.cases[1], matches: 'premium' }],
    });
  });

  it('adds a case with the next number and a unique placeholder match', () => {
    const actions = makeActions();
    render(element({ kind, status: 'idle' }, actions));
    fireEvent.click(screen.getByRole('button', { name: 'Add case' }));
    const next = actions.updateNodeKind.mock.calls[0][1];
    expect(next.cases).toHaveLength(3);
    expect(next.cases[2]).toMatchObject({ label: 'Case 3', matches: 'case-3' });
    expect(next.cases[2].id).toEqual(expect.any(String));
    expect(new Set(next.cases.map((c: { id: string }) => c.id)).size).toBe(3);
  });

  it('skips a placeholder match that is already taken', () => {
    const actions = makeActions();
    const taken = { ...kind, cases: [kind.cases[0], { ...kind.cases[1], matches: 'case-3' }] };
    render(element({ kind: taken, status: 'idle' }, actions));
    fireEvent.click(screen.getByRole('button', { name: 'Add case' }));
    const next = actions.updateNodeKind.mock.calls[0][1];
    expect(next.cases[2]).toMatchObject({ label: 'Case 4', matches: 'case-4' });
  });

  it('removes a case through removeSwitchCase, not a plain kind update', () => {
    const actions = makeActions();
    render(element({ kind, status: 'idle' }, actions));
    fireEvent.click(screen.getByRole('button', { name: 'Remove case Free' }));
    expect(actions.removeSwitchCase).toHaveBeenCalledWith('sw1', 'c1');
    expect(actions.updateNodeKind).not.toHaveBeenCalled();
  });

  it('flags duplicate match values before save', () => {
    const dup = {
      ...kind,
      cases: [
        { id: 'c1', label: 'Case 1', matches: '' },
        { id: 'c2', label: 'Case 2', matches: '' },
      ],
    };
    render(element({ kind: dup, status: 'idle' }, makeActions()));
    expect(screen.getByRole('alert')).toHaveTextContent('Two cases match the same value.');
    expect(screen.getByLabelText('Case 1 matches')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Case 2 matches')).toHaveAttribute('aria-invalid', 'true');
  });

  it('refreshes node internals when cases change', () => {
    updateNodeInternals.mockClear();
    const actions = makeActions();
    const { rerender } = render(element({ kind, status: 'idle' }, actions));
    const afterMount = updateNodeInternals.mock.calls.length;
    rerender(
      element(
        { kind: { ...kind, cases: [...kind.cases, { id: 'c3', label: 'Case 3', matches: '' }] }, status: 'idle' },
        actions,
      ),
    );
    expect(updateNodeInternals.mock.calls.length).toBeGreaterThan(afterMount);
    expect(updateNodeInternals).toHaveBeenLastCalledWith('sw1');
  });

  it('badges the chosen case by its label', () => {
    render(element({ kind, status: 'success', branch: 'case:c2' }, makeActions()));
    expect(screen.getByTestId('branch-badge')).toHaveTextContent('→ Pro plan');
  });
});
```

Create `src/components/flow/__tests__/FlowCanvas.routing.test.tsx`:

```tsx
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

const switchNode: FlowNode = {
  id: 'sw1',
  position: { x: 0, y: 0 },
  kind: {
    kind: 'Switch',
    label: 'Plan router',
    value: 'response.body.plan',
    cases: [{ id: 'c1', label: 'Free', matches: 'free' }],
  },
};

function Harness({ onRemoveSwitchCase }: { onRemoveSwitchCase: (n: string, c: string) => void }) {
  const [nodes, setNodes] = useState<FlowNode[]>([switchNode]);
  const [edges, setEdges] = useState<FlowEdge[]>([]);
  return (
    <FlowCanvas
      nodes={nodes}
      edges={edges}
      nodeStatus={{}}
      onNodesChange={setNodes}
      onEdgesChange={setEdges}
      onConnect={vi.fn()}
      onNodeKindChange={(id, kind) =>
        setNodes((prev) => prev.map((n) => (n.id === id ? { ...n, kind } : n)))
      }
      onRemoveSwitchCase={onRemoveSwitchCase}
    />
  );
}

describe('FlowCanvas with routing nodes', () => {
  it('renders If and Switch nodes through nodeTypes', () => {
    render(
      <FlowCanvas
        nodes={[
          switchNode,
          {
            id: 'if1',
            position: { x: 300, y: 0 },
            kind: { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' },
          },
        ]}
        edges={[]}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    expect(screen.getByTestId('switch-node-card')).toBeInTheDocument();
    expect(screen.getByTestId('if-node-card')).toBeInTheDocument();
  });

  it('wires the remove-case button to onRemoveSwitchCase', () => {
    const onRemove = vi.fn();
    render(<Harness onRemoveSwitchCase={onRemove} />);
    fireEvent.click(screen.getByRole('button', { name: 'Remove case Free' }));
    expect(onRemove).toHaveBeenCalledWith('sw1', 'c1');
  });

  it('does not delete a selected Switch node when Backspace is typed in a case field', async () => {
    render(<Harness onRemoveSwitchCase={vi.fn()} />);
    fireEvent.click(screen.getByText('Plan router'));
    await waitFor(() =>
      expect(document.querySelector('.react-flow__node[data-id="sw1"]')).toHaveClass('selected'),
    );
    const field = screen.getByLabelText('Case 1 label');
    act(() => field.focus());
    await act(async () => {
      fireEvent.keyDown(field, { key: 'Backspace' });
    });
    await act(async () => {
      fireEvent.keyUp(field, { key: 'Backspace' });
    });
    expect(screen.getByTestId('switch-node-card')).toBeInTheDocument();
    expect(field.closest('.nokey')).not.toBeNull();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow`
Expected: FAIL. The new files fail with "Failed to resolve import '../flowExits'", "'../IfNode'" and "'../SwitchNode'". The canvas routing test fails because `switch-node-card` is not found.

- [ ] **Step 3: Write the implementation**

Create `src/components/flow/flowExits.ts`:

```ts
import { caseIdFromHandle, DEFAULT_HANDLE, FALSE_HANDLE, TRUE_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind } from '@/lib/tauri-api';

// Display label of a routing node's exit. A case is looked up by id, so
// renaming a case relabels its edges and badge without rewiring anything.
export function exitLabel(kind: FlowNodeKind, handle: string): string | undefined {
  if (kind.kind === 'If') {
    if (handle === TRUE_HANDLE) return 'true';
    if (handle === FALSE_HANDLE) return 'false';
    return undefined;
  }
  if (kind.kind === 'Switch') {
    if (handle === DEFAULT_HANDLE) return 'default';
    const caseId = caseIdFromHandle(handle);
    if (!caseId) return undefined;
    return kind.cases.find((c) => c.id === caseId)?.label;
  }
  return undefined;
}
```

Create `src/components/flow/nodes/IfNode.tsx`:

```tsx
import { Handle, type NodeProps, Position } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { FALSE_HANDLE, INPUT_HANDLE, TRUE_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { exitLabel } from '../flowExits';
import { useFlowNodeActions } from './FlowNodeActionsContext';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export interface IfNodeData {
  kind: Extract<FlowNodeKind, { kind: 'If' }>;
  status: FlowNodeStatus;
  error?: string;
  skipReason?: FlowSkipReason;
  /** Exit chosen by the last run: "true" or "false". */
  branch?: string;
  /** Set when a save was rejected because of this node. */
  hasCycleError?: boolean;
}

export function IfNode({ id, data, isConnectable }: NodeProps & { data: IfNodeData }) {
  const { updateNodeKind } = useFlowNodeActions();
  const { kind, status } = data;

  return (
    <div
      data-testid='if-node-card'
      data-status={status}
      className={cn(
        'w-64 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <Handle
        type='target'
        id={INPUT_HANDLE}
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <GitBranch className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>If</span>
        <span className='truncate font-medium'>{kind.label}</span>
      </div>

      {status === 'success' && data.branch && (
        <div className='px-2 pt-1'>
          <Badge variant='secondary' data-testid='branch-badge'>
            → {exitLabel(kind, data.branch) ?? data.branch}
          </Badge>
        </div>
      )}
      {status === 'failed' && (
        <div className='px-2 pt-1 text-red-600'>✕ {data.error ?? 'Error'}</div>
      )}
      <NodeStatusCaption status={status} skipReason={data.skipReason} />

      {/* nodrag/nowheel/nokey keep typing, selecting text and scrolling in
          the editor from dragging the node or deleting it on Backspace. */}
      <div className='nodrag nowheel nokey px-2 py-1.5'>
        <span className='text-muted-foreground'>condition</span>
        <SingleLineEditor
          aria-label='Condition'
          value={kind.condition}
          onChange={(condition) => updateNodeKind(id, { ...kind, condition })}
          placeholder='response.status === 200'
          className='text-xs'
        />
      </div>

      <div className='space-y-1 px-2 pb-1.5'>
        <div className='relative flex justify-end pr-2'>
          <span className='text-green-600'>true</span>
          <Handle
            type='source'
            id={TRUE_HANDLE}
            position={Position.Right}
            isConnectable={isConnectable}
            className='!h-2 !w-2 !bg-green-500'
          />
        </div>
        <div className='relative flex justify-end pr-2'>
          <span className='text-muted-foreground'>false</span>
          <Handle
            type='source'
            id={FALSE_HANDLE}
            position={Position.Right}
            isConnectable={isConnectable}
            className='!h-2 !w-2 !bg-muted-foreground'
          />
        </div>
      </div>
    </div>
  );
}
```

Create `src/components/flow/nodes/SwitchNode.tsx`:

```tsx
import { Handle, type NodeProps, Position, useUpdateNodeInternals } from '@xyflow/react';
import { Plus, Split, X } from 'lucide-react';
import { useEffect } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { caseHandle, DEFAULT_HANDLE, INPUT_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason, SwitchCase } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { exitLabel } from '../flowExits';
import { useFlowNodeActions } from './FlowNodeActionsContext';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export interface SwitchNodeData {
  kind: Extract<FlowNodeKind, { kind: 'Switch' }>;
  status: FlowNodeStatus;
  error?: string;
  skipReason?: FlowSkipReason;
  /** Exit chosen by the last run: "case:<id>" or "default". */
  branch?: string;
  /** Set when a save was rejected because of this node. */
  hasCycleError?: boolean;
}

// Match values that appear more than once. The backend rejects them on save
// (rule V7), so flag them while the user is still editing.
function duplicateMatches(cases: SwitchCase[]): Set<string> {
  const seen = new Set<string>();
  const dupes = new Set<string>();
  for (const c of cases) {
    if (seen.has(c.matches)) dupes.add(c.matches);
    seen.add(c.matches);
  }
  return dupes;
}

// New cases get a unique placeholder match so adding several cases never
// trips rule V7 before the user has typed real values.
function nextCaseNumber(cases: SwitchCase[]): number {
  const used = new Set(cases.map((c) => c.matches));
  let n = cases.length + 1;
  while (used.has(`case-${n}`)) n += 1;
  return n;
}

export function SwitchNode({ id, data, isConnectable }: NodeProps & { data: SwitchNodeData }) {
  const { updateNodeKind, removeSwitchCase } = useFlowNodeActions();
  const { kind, status } = data;
  const dupes = duplicateMatches(kind.cases);

  // React Flow only learns about handles added or removed after mount when
  // told explicitly. Without this, a new case's exit cannot be connected.
  const updateNodeInternals = useUpdateNodeInternals();
  const caseKey = kind.cases.map((c) => c.id).join('|');
  // biome-ignore lint/correctness/useExhaustiveDependencies: caseKey changes exactly when exit handles are added or removed.
  useEffect(() => {
    updateNodeInternals(id);
  }, [id, caseKey, updateNodeInternals]);

  const setCase = (caseId: string, patch: Partial<SwitchCase>) =>
    updateNodeKind(id, {
      ...kind,
      cases: kind.cases.map((c) => (c.id === caseId ? { ...c, ...patch } : c)),
    });

  const addCase = () => {
    const n = nextCaseNumber(kind.cases);
    updateNodeKind(id, {
      ...kind,
      cases: [...kind.cases, { id: crypto.randomUUID(), label: `Case ${n}`, matches: `case-${n}` }],
    });
  };

  return (
    <div
      data-testid='switch-node-card'
      data-status={status}
      className={cn(
        'w-72 rounded-md border bg-card text-card-foreground text-xs shadow-sm',
        nodeStatusClassName(status, data.skipReason),
        data.hasCycleError && 'ring-2 ring-red-500',
      )}
    >
      <Handle
        type='target'
        id={INPUT_HANDLE}
        position={Position.Left}
        isConnectable={isConnectable}
        className='!h-2 !w-2'
      />
      <div className='flex items-center gap-1.5 border-b px-2 py-1.5'>
        <Split className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>Switch</span>
        <span className='truncate font-medium'>{kind.label}</span>
      </div>

      {status === 'success' && data.branch && (
        <div className='px-2 pt-1'>
          <Badge variant='secondary' data-testid='branch-badge'>
            → {exitLabel(kind, data.branch) ?? data.branch}
          </Badge>
        </div>
      )}
      {status === 'failed' && (
        <div className='px-2 pt-1 text-red-600'>✕ {data.error ?? 'Error'}</div>
      )}
      <NodeStatusCaption status={status} skipReason={data.skipReason} />

      <div className='nodrag nowheel nokey space-y-1 px-2 py-1.5'>
        <span className='text-muted-foreground'>value</span>
        <SingleLineEditor
          aria-label='Switch value'
          value={kind.value}
          onChange={(value) => updateNodeKind(id, { ...kind, value })}
          placeholder='response.body.type'
          className='text-xs'
        />

        {kind.cases.map((c, i) => (
          <div key={c.id} className='relative flex items-center gap-1 pr-3'>
            <Input
              aria-label={`Case ${i + 1} label`}
              value={c.label}
              onChange={(e) => setCase(c.id, { label: e.target.value })}
              className='h-6 px-1 text-xs'
            />
            <span className='text-muted-foreground'>=</span>
            <Input
              aria-label={`Case ${i + 1} matches`}
              aria-invalid={dupes.has(c.matches)}
              value={c.matches}
              onChange={(e) => setCase(c.id, { matches: e.target.value })}
              className={cn('h-6 px-1 font-mono text-xs', dupes.has(c.matches) && 'border-red-500')}
            />
            <Button
              variant='ghost'
              size='icon'
              aria-label={`Remove case ${c.label}`}
              className='h-6 w-6 shrink-0'
              onClick={() => removeSwitchCase(id, c.id)}
            >
              <X className='h-3 w-3' aria-hidden='true' />
            </Button>
            <Handle
              type='source'
              id={caseHandle(c.id)}
              position={Position.Right}
              isConnectable={isConnectable}
              className='!h-2 !w-2'
            />
          </div>
        ))}

        {dupes.size > 0 && (
          <div role='alert' className='text-[10px] text-red-600'>
            Two cases match the same value.
          </div>
        )}

        <Button
          variant='ghost'
          size='sm'
          aria-label='Add case'
          className='h-6 gap-1 px-1 text-xs'
          onClick={addCase}
        >
          <Plus className='h-3 w-3' aria-hidden='true' />
          Add case
        </Button>
      </div>

      <div className='relative flex justify-end px-2 pb-1.5 pr-4'>
        <span className='text-muted-foreground'>default</span>
        <Handle
          type='source'
          id={DEFAULT_HANDLE}
          position={Position.Right}
          isConnectable={isConnectable}
          className='!h-2 !w-2'
        />
      </div>
    </div>
  );
}
```

In `src/components/flow/FlowCanvas.tsx`, import both components and register them:

```ts
import { IfNode } from './nodes/IfNode';
import { SwitchNode } from './nodes/SwitchNode';

const nodeTypes = {
  Request: RequestNode,
  Input: InputNode,
  Output: OutputNode,
  If: IfNode,
  Switch: SwitchNode,
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: all flow tests PASS; `tsc` reports no errors; `yarn check` reports no Biome errors.

If Biome reports `useExhaustiveDependencies` for `caseKey`, keep the existing `biome-ignore` comment exactly where it is (directly above `useEffect`).

- [ ] **Step 5: Commit**

Stage the eight files and commit using the `dev-workflow-skills:1-git-commit` skill.
Suggested subject: `feat(flow): add inline-editable If and Switch nodes`

---

### Task 4: Palette entries and taken/not-taken edge styling

**Files:**
- Modify: `src/components/flow/NodePalette.tsx`
- Modify: `src/components/flow/flowExits.ts` (add `edgeRunState`)
- Modify: `src/components/flow/FlowCanvas.tsx` (`toRfEdges` and its `useMemo`)
- Test: `src/components/flow/__tests__/NodePalette.test.tsx` (create)
- Test: `src/components/flow/__tests__/flowExits.test.ts` (extend)
- Test: `src/components/flow/__tests__/FlowCanvas.routing.test.tsx` (extend)

**Interfaces:**
- Consumes:
  - `exitLabel` (Task 3), `RESULT_HANDLE` (plan 04).
  - The `FlowCanvas` props `nodeStatus` and `nodeDetail`. After plan 04, `nodeDetail` is typed `Record<string, FlowNodeDetail>`, where `FlowNodeDetail` includes `branch?: string`.
- Produces:
  - `type EdgeRunState = 'taken' | 'not-taken' | 'neutral'`.
  - `edgeRunState(edge: FlowEdge, source: FlowNode | undefined, sourceStatus: FlowNodeStatus | undefined, sourceBranch: string | undefined): EdgeRunState`.
  - Each React Flow edge gets a `label` (routing exits only). It also gets a `className` of `flow-edge-taken` or `flow-edge-not-taken` (routing exits after a completed route only).
  - Palette menu items "If" and "Switch".

- [ ] **Step 1: Write the failing tests**

Append to `src/components/flow/__tests__/flowExits.test.ts`, changing the import line to `import { edgeRunState, exitLabel } from '../flowExits';`, and adding `import type { FlowEdge, FlowNode } from '@/lib/tauri-api';`:

```ts
describe('edgeRunState', () => {
  const ifNode: FlowNode = { id: 'if1', position: { x: 0, y: 0 }, kind: ifKind };
  const plain: FlowNode = { id: 'r1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'O' } };
  const trueEdge: FlowEdge = { id: 'e1', sourceNodeId: 'if1', sourceHandle: 'true', targetNodeId: 'x', targetField: 'trigger', expression: '' };
  const falseEdge: FlowEdge = { ...trueEdge, id: 'e2', sourceHandle: 'false' };

  it('marks the chosen exit taken and the other not-taken', () => {
    expect(edgeRunState(trueEdge, ifNode, 'success', 'true')).toBe('taken');
    expect(edgeRunState(falseEdge, ifNode, 'success', 'true')).toBe('not-taken');
  });

  it('is neutral while the routing node is running or has no branch', () => {
    expect(edgeRunState(trueEdge, ifNode, 'running', 'true')).toBe('neutral');
    expect(edgeRunState(trueEdge, ifNode, 'success', undefined)).toBe('neutral');
    expect(edgeRunState(trueEdge, ifNode, undefined, undefined)).toBe('neutral');
  });

  it('is neutral when the routing node failed or was skipped', () => {
    expect(edgeRunState(trueEdge, ifNode, 'failed', undefined)).toBe('neutral');
    expect(edgeRunState(trueEdge, ifNode, 'skipped', undefined)).toBe('neutral');
  });

  it('is neutral for plain nodes and a missing source', () => {
    const plainEdge: FlowEdge = { ...trueEdge, sourceNodeId: 'r1', sourceHandle: undefined };
    expect(edgeRunState(plainEdge, plain, 'success', undefined)).toBe('neutral');
    expect(edgeRunState(trueEdge, undefined, 'success', 'true')).toBe('neutral');
  });
});
```

Create `src/components/flow/__tests__/NodePalette.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { NodePalette } from '../NodePalette';

describe('NodePalette routing entries', () => {
  it('adds an If node with the default condition', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'If' }));
    expect(onAddNode).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^if-/),
        position: { x: 100, y: 100 },
        kind: { kind: 'If', label: 'New If', condition: 'response.status === 200' },
      }),
    );
  });

  it('adds a Switch node with a default value and one empty case', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Switch' }));
    const node = onAddNode.mock.calls[0][0];
    expect(node.id).toMatch(/^switch-/);
    expect(node.kind).toMatchObject({ kind: 'Switch', label: 'New Switch', value: 'response.body.type' });
    expect(node.kind.cases).toHaveLength(1);
    expect(node.kind.cases[0]).toMatchObject({ label: 'Case 1', matches: 'case-1' });
  });
});
```

Append to `src/components/flow/__tests__/FlowCanvas.routing.test.tsx`. Add `afterEach` to the `vitest` import. Then add this block at the end of the outer `describe`. It uses the same `stubLayout` idea as `FlowCanvas.test.tsx:141-168`, copied verbatim, because edges only render once handles are measured.

```tsx
  describe('edges after a run', () => {
    function stubLayout() {
      vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(200);
      vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(80);
      vi.stubGlobal(
        'DOMMatrixReadOnly',
        class {
          m22 = 1;
        },
      );
      vi.stubGlobal(
        'ResizeObserver',
        class {
          constructor(private cb: ResizeObserverCallback) {}
          observe(target: Element) {
            if (!target.classList.contains('react-flow__node')) return;
            this.cb([{ target } as ResizeObserverEntry], this as unknown as ResizeObserver);
          }
          unobserve() {
            // Not needed by these tests.
          }
          disconnect() {
            // Not needed by these tests.
          }
        },
      );
    }

    afterEach(() => {
      vi.restoreAllMocks();
      vi.unstubAllGlobals();
    });

    const graph: FlowNode[] = [
      {
        id: 'if1',
        position: { x: 0, y: 0 },
        kind: { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' },
      },
      { id: 'yes', position: { x: 300, y: -100 }, kind: { kind: 'Output', label: 'Yes' } },
      { id: 'no', position: { x: 300, y: 100 }, kind: { kind: 'Output', label: 'No' } },
    ];
    const wires: FlowEdge[] = [
      { id: 'eT', sourceNodeId: 'if1', sourceHandle: 'true', targetNodeId: 'yes', targetField: 'trigger', expression: '' },
      { id: 'eF', sourceNodeId: 'if1', sourceHandle: 'false', targetNodeId: 'no', targetField: 'trigger', expression: '' },
    ];

    it('labels routing exits and styles taken vs not-taken edges', () => {
      stubLayout();
      render(
        <FlowCanvas
          nodes={graph}
          edges={wires}
          nodeStatus={{ if1: 'success', yes: 'success', no: 'skipped' }}
          nodeDetail={{ if1: { branch: 'true' }, no: { skipReason: 'branch_not_taken' } }}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
        />,
      );
      expect(screen.getByTestId('rf__edge-eT')).toHaveClass('flow-edge-taken');
      expect(screen.getByTestId('rf__edge-eF')).toHaveClass('flow-edge-not-taken');
      expect(screen.getByTestId('rf__edge-eT')).toHaveTextContent('true');
      expect(screen.getByTestId('rf__edge-eF')).toHaveTextContent('false');
    });

    it('renders both exits neutral before any run', () => {
      stubLayout();
      render(
        <FlowCanvas
          nodes={graph}
          edges={wires}
          nodeStatus={{}}
          onNodesChange={vi.fn()}
          onEdgesChange={vi.fn()}
          onConnect={vi.fn()}
        />,
      );
      expect(screen.getByTestId('rf__edge-eT')).not.toHaveClass('flow-edge-taken');
      expect(screen.getByTestId('rf__edge-eF')).not.toHaveClass('flow-edge-not-taken');
    });
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/flow`
Expected: FAIL. `edgeRunState` is not exported, the palette has no "If"/"Switch" menu items, and the edges lack the `flow-edge-*` classes.

- [ ] **Step 3: Write the implementation**

Append to `src/components/flow/flowExits.ts`, extending its imports to `caseIdFromHandle, DEFAULT_HANDLE, FALSE_HANDLE, RESULT_HANDLE, TRUE_HANDLE` and `FlowEdge, FlowNode, FlowNodeKind, FlowNodeStatus`:

```ts
export type EdgeRunState = 'taken' | 'not-taken' | 'neutral';

// Only a routing node that completed has a chosen exit. Every other case
// (running, failed, skipped, never run, plain node) renders neutral, so a
// new run never shows the previous run's branch.
export function edgeRunState(
  edge: FlowEdge,
  source: FlowNode | undefined,
  sourceStatus: FlowNodeStatus | undefined,
  sourceBranch: string | undefined,
): EdgeRunState {
  if (!source || (source.kind.kind !== 'If' && source.kind.kind !== 'Switch')) return 'neutral';
  if (sourceStatus !== 'success' || !sourceBranch) return 'neutral';
  return (edge.sourceHandle ?? RESULT_HANDLE) === sourceBranch ? 'taken' : 'not-taken';
}
```

In `src/components/flow/FlowCanvas.tsx`:
- Import `edgeRunState, exitLabel` from `./flowExits` and `RESULT_HANDLE` from `@/lib/flow-handles`, if plan 04 did not already import it.
- Replace the whole `toRfEdges` function (as plan 04 left it) with:

```ts
const CYCLE_EDGE_STYLE = { stroke: '#ef4444', strokeWidth: 2 };
const TAKEN_EDGE_STYLE = { stroke: '#22c55e', strokeWidth: 2 };
const NOT_TAKEN_EDGE_STYLE = { opacity: 0.35, strokeDasharray: '4 4' };

// The source handle is the edge's exit (absent means `result`). The target
// handle is the first segment of `targetField`, so
// "headers[Authorization].value" lands on the single `headers` handle.
// Routing exits get a label, and after a run their taken/not-taken state.
// A validation (cycle) highlight wins over run styling.
function toRfEdges(
  edges: FlowEdge[],
  nodes: FlowNode[],
  nodeStatus: Record<string, FlowNodeStatus>,
  selectedIds: ReadonlySet<string>,
  nodeDetail?: FlowCanvasProps['nodeDetail'],
  cycleEdgeIds?: string[],
): Edge[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  return edges.map((e) => {
    const source = byId.get(e.sourceNodeId);
    const handle = e.sourceHandle ?? RESULT_HANDLE;
    const run = edgeRunState(e, source, nodeStatus[e.sourceNodeId], nodeDetail?.[e.sourceNodeId]?.branch);
    const isCycle = cycleEdgeIds?.includes(e.id) ?? false;
    return {
      id: e.id,
      source: e.sourceNodeId,
      sourceHandle: handle,
      target: e.targetNodeId,
      targetHandle: e.targetField.split('[')[0],
      selected: selectedIds.has(e.id),
      label: source ? exitLabel(source.kind, handle) : undefined,
      className: run === 'neutral' ? undefined : `flow-edge-${run}`,
      style: isCycle
        ? CYCLE_EDGE_STYLE
        : run === 'taken'
          ? TAKEN_EDGE_STYLE
          : run === 'not-taken'
            ? NOT_TAKEN_EDGE_STYLE
            : undefined,
    };
  });
}
```

- Update the `rfEdges` memo:

```ts
  const rfEdges = useMemo(
    () => toRfEdges(edges, nodes, nodeStatus, selectedEdgeIds, nodeDetail, cycleEdgeIds),
    [edges, nodes, nodeStatus, selectedEdgeIds, nodeDetail, cycleEdgeIds],
  );
```

In `src/components/flow/NodePalette.tsx`:
- Change the icon import to `import { ArrowLeftFromLine, ArrowRightToLine, GitBranch, Globe, Plus, Split } from 'lucide-react';`.
- Add these two items after the "Inline Request" item, inside `DropdownMenuContent`:

```tsx
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('if'),
                kind: { kind: 'If', label: 'New If', condition: 'response.status === 200' },
                position: defaultPosition,
              })
            }
          >
            <GitBranch className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            If
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('switch'),
                kind: {
                  kind: 'Switch',
                  label: 'New Switch',
                  value: 'response.body.type',
                  cases: [{ id: crypto.randomUUID(), label: 'Case 1', matches: 'case-1' }],
                },
                position: defaultPosition,
              })
            }
          >
            <Split className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Switch
          </DropdownMenuItem>
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow && yarn tsc --noEmit && yarn check`
Expected: PASS for every flow test, including the existing `FlowCanvas.test.tsx` cycle-stroke test, which still gets `#ef4444`. `tsc` and Biome report no errors.

- [ ] **Step 5: Commit**

Stage the six files and commit using the `dev-workflow-skills:1-git-commit` skill.
Suggested subject: `feat(flow): add routing nodes to palette and show taken branches`

---

## Whole-series verification

After Task 4, run the full verification once for the whole Phase 2 series:

- [ ] `cargo check -j4`: no errors.
- [ ] `cargo test -j4 -p rocket-flow -p rocket-app -p rocket-infra -p rocket-shared`: all pass.
- [ ] The `src-tauri` crate's tests (`cargo test -j4 --manifest-path src-tauri/Cargo.toml`): all pass.
- [ ] `yarn tsc --noEmit`: no errors.
- [ ] `yarn check`: no Biome errors.
- [ ] `yarn test`: the full Vitest suite passes, not only `flow`.

Then walk the spec §13 acceptance criteria in the running app (`yarn tauri dev`). Record each result, pass or fail:

- [ ] Add If and Switch from the palette, edit the condition, value and cases inline, and wire an upstream Request into each `input`. Wire their exits into a data field and into a "Run when" input. Save, close the tab, reopen the flow: every node, case and wire is intact.
- [ ] Run a flow with an If: only the chosen exit's dependents run. The others show **Not taken** with a dashed border, distinct from **Skipped — upstream failed**.
- [ ] A Switch routes to the first matching case, or to `default` when none match. The badge shows the case label.
- [ ] if/else paths rejoining into one field of one node run that node once. A node that needs a value from a not-taken branch in a different field shows **Not taken**.
- [ ] Each routing node shows its chosen exit. Taken exit edges are green; not-taken exit edges are dimmed and dashed.
- [ ] Invalid graphs cannot be saved (for example a second wire into an If `input`, or two cases with the same match value), and the offending node or edge is highlighted in red.
- [ ] An existing Phase 1 flow loads, runs with the same per-node statuses, and re-saving produces no `git diff`.

## Next Plan

None. This is the last plan in the Phase 2 series. Return to [00-plan-index.md](00-plan-index.md) and close issue #31 once the checklist above is green.

## Post-Implementation Review

Before the series is considered done, dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review everything this plan added or modified:
- `src/components/flow/**`
- `src/lib/flow-graph-edits.ts`
- `src/stores/pane-store.ts` (`updateFlowGraph`)

The reviewer checks:
- Interface drift from the cross-plan contract in `00-plan-index.md`.
- shadcn-only and lucide-only compliance, and that no Monaco is used for single-line fields.
- Zustand selector hygiene.
- Duplicated status or label logic that should go through `nodeStatus.ts` or `flowExits.ts`.
- Handle names hard-coded instead of taken from `flow-handles.ts`.
- Missing `nodrag`/`nokey` on any interactive element inside a node.

The reviewer has explicit authority to fix what it finds directly, not only report it. It must run `yarn tsc --noEmit`, `yarn check` and `yarn test flow` after its fixes.
