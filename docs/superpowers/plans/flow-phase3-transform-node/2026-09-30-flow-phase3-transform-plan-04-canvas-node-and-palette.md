# Flow Phase 3 — Plan 04: Canvas Node and Palette — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Run this plan on its own. Do not start plan 05 in the same run.**

**Goal:** Let a user add a Transform node from the palette and see it on the canvas with one input handle, one result handle, a one-line script preview and the standard run-status caption.

**Architecture:** A new `TransformNode` component mirrors `IfNode`/`RequestNode`: a target `input` handle, a source `result` handle, the shared status caption and menu button. The script is shown as a one-line preview and is edited in the properties panel (plan 05), so the node holds no editor. `FlowCanvas` registers the node type, and `NodePalette` gets a menu entry.

**Tech Stack:** React, TypeScript, `@xyflow/react`, shadcn/ui, `lucide-react`, Vitest, Yarn.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (§9 Frontend). Plan index and cross-plan contract: `docs/superpowers/plans/flow-phase3-transform-node/00-plan-index.md`.

## Global Constraints

- Plan 03 is merged: `FlowNodeKind` has the `Transform` variant, `DEFAULT_TRANSFORM_SCRIPT` exists in `src/lib/flow-transform.ts`, and `takesSingleInput` exists in `src/lib/flow-handles.ts`. If not, stop and report.
- All UI uses shadcn/ui primitives, and icons come from `lucide-react` only (no inline SVGs, no raw `<button>`, `<input>`, `<select>`, `<form>` or `<dialog>`).
- The node holds no code editor. Multi-line editing belongs to the properties panel (Monaco), not the node body.
- Use Yarn. Never fully destructure a Zustand store at component top level.
- Commits: conventional-commit subjects, created through the `dev-workflow-skills:1-git-commit` skill. Stage only the task's own paths.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **An empty or whitespace-only script.** The node must show a clear "(empty)" placeholder, not a blank row that looks broken. Pinned in Task 1 (`shows a placeholder for an empty script`).
2. **A long or multi-line script.** The node must show only the first non-empty line, cut to fit, and never grow taller with the script. Pinned in Task 1 (`shows only the first non-empty line of the script`).
3. **A Transform skipped because its branch was not taken.** It must read "Not taken", not "Skipped — upstream failed". Pinned in Task 1 (`says Not taken for a skipped branch`).
4. **A failed Transform.** The script error must show on the node. Pinned in Task 1 (`shows the error of a failed run`).
5. **A Transform placed after an If.** Wires that leave a Transform must never get the taken/not-taken styling that If and Switch exits get. Pinned in Task 3 (`a Transform never gets taken or not-taken styling`).

---

### Task 1: The `TransformNode` component

**Files:**
- Create: `src/components/flow/nodes/TransformNode.tsx`
- Create: `src/components/flow/nodes/__tests__/TransformNode.test.tsx`

**Interfaces:**
- Consumes: `INPUT_HANDLE` and `RESULT_HANDLE` from `@/lib/flow-handles`, `scriptPreview` from `../properties/wireRows`, `NodeMenuButton`, `NodeStatusCaption`, `nodeStatusClassName`, and the `Transform` kind type.
- Produces: `TransformNode` and `TransformNodeData` (`{ kind: Extract<FlowNodeKind, { kind: 'Transform' }>; status: FlowNodeStatus; error?: string; progress?: string; skipReason?: FlowSkipReason; hasCycleError?: boolean }`). Task 2 registers the component.

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/nodes/__tests__/TransformNode.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import { FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { TransformNode, type TransformNodeData } from '../TransformNode';

const kind = {
  kind: 'Transform' as const,
  label: 'Pick token',
  script: 'return response.body.token;',
};

function renderTransform(data: TransformNodeData) {
  const actions = { updateNodeKind: vi.fn(), removeSwitchCase: vi.fn(), openProperties: vi.fn() };
  render(
    <ReactFlowProvider>
      <FlowNodeActionsContext.Provider value={actions}>
        <TransformNode
          id='tf1'
          type='Transform'
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

describe('TransformNode', () => {
  it('renders one input handle and one result handle', () => {
    renderTransform({ kind, status: 'idle' });
    const card = screen.getByTestId('transform-node-card');
    expect(screen.getByText('Pick token')).toBeInTheDocument();
    expect(card.querySelectorAll('.react-flow__handle.target')).toHaveLength(1);
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(1);
    expect(card.querySelector('[data-handleid="input"]')).toBeInTheDocument();
    expect(card.querySelector('[data-handleid="result"]')).toBeInTheDocument();
  });

  it('shows only the first non-empty line of the script', () => {
    renderTransform({
      kind: { ...kind, script: '\n\nconst token = response.body.token;\nreturn token;' },
      status: 'idle',
    });
    expect(screen.getByTestId('transform-script-preview')).toHaveTextContent(
      'const token = response.body.token;',
    );
    expect(screen.queryByText(/return token/)).not.toBeInTheDocument();
  });

  it('shows a placeholder for an empty script', () => {
    renderTransform({ kind: { ...kind, script: '  \n ' }, status: 'idle' });
    expect(screen.getByTestId('transform-script-preview')).toHaveTextContent('(empty)');
  });

  it('says Not taken for a skipped branch', () => {
    renderTransform({ kind, status: 'skipped', skipReason: 'branch_not_taken' });
    expect(screen.getByText(/not taken/i)).toBeInTheDocument();
  });

  it('says upstream failed for a failure skip', () => {
    renderTransform({ kind, status: 'skipped', skipReason: 'upstream_failed' });
    expect(screen.getByText(/upstream failed/i)).toBeInTheDocument();
  });

  it('shows the error of a failed run', () => {
    renderTransform({ kind, status: 'failed', error: 'script returned no value' });
    expect(screen.getByTestId('node-error')).toHaveTextContent('script returned no value');
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test TransformNode`
Expected: FAIL, because `../TransformNode` does not exist.

- [ ] **Step 3: Write the component**

Create `src/components/flow/nodes/TransformNode.tsx`:

```tsx
import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Code } from 'lucide-react';
import { INPUT_HANDLE, RESULT_HANDLE } from '@/lib/flow-handles';
import type { FlowNodeKind, FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { scriptPreview } from '../properties/wireRows';
import { NodeMenuButton } from './NodeMenuButton';
import { NodeStatusCaption } from './NodeStatusCaption';
import { nodeStatusClassName } from './nodeStatus';

export type TransformNodeData = {
  kind: Extract<FlowNodeKind, { kind: 'Transform' }>;
  status: FlowNodeStatus;
  error?: string;
  /** Progress text while running. */
  progress?: string;
  skipReason?: FlowSkipReason;
  /** Set when a save was rejected because of this node. */
  hasCycleError?: boolean;
};

export function TransformNode({
  id,
  data,
  isConnectable,
}: NodeProps & { data: TransformNodeData }) {
  const { kind, status } = data;
  // The script is edited in the properties panel, so the node shows one line.
  const preview = scriptPreview(kind.script);

  return (
    <div
      data-testid='transform-node-card'
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
        <Code className='h-3.5 w-3.5 shrink-0 text-muted-foreground' aria-hidden='true' />
        <span className='font-mono text-[10px] text-muted-foreground'>Transform</span>
        <span className='truncate font-medium'>{kind.label}</span>
        <NodeMenuButton nodeId={id} label={kind.label} />
      </div>

      <NodeStatusCaption
        status={status}
        skipReason={data.skipReason}
        error={data.error}
        progress={data.progress}
      />

      <div className='px-2 py-1.5'>
        <span className='text-muted-foreground'>script</span>
        <p data-testid='transform-script-preview' className='truncate font-mono'>
          {preview ?? '(empty)'}
        </p>
      </div>

      <div className='relative flex justify-end px-2 pb-1.5 pr-4'>
        <span className='text-muted-foreground'>result</span>
        <Handle
          type='source'
          id={RESULT_HANDLE}
          position={Position.Right}
          isConnectable={isConnectable}
          className='!h-2 !w-2'
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test TransformNode`
Expected: PASS for all six tests. If the two skip-caption tests fail on wording, open `src/components/flow/nodes/nodeStatus.ts`, read the exact caption strings, and change the test regexes to match them. Do not change `nodeStatus.ts`.

- [ ] **Step 5: Commit**

Stage the two new files, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): add Transform canvas node`.

---

### Task 2: Register the node and add the palette entry

**Files:**
- Modify: `src/components/flow/FlowCanvas.tsx` (imports and `nodeTypes`)
- Modify: `src/components/flow/NodePalette.tsx` (imports and a new menu item)
- Modify: `src/components/flow/__tests__/NodePalette.test.tsx`

**Interfaces:**
- Consumes: `TransformNode` from Task 1 and `DEFAULT_TRANSFORM_SCRIPT` from plan 03.
- Produces: a `Transform` entry in the Add node menu that calls `onAddNode` with `{ id: 'transform-…', kind: { kind: 'Transform', label: 'New Transform', script: DEFAULT_TRANSFORM_SCRIPT }, position }`, and a canvas that renders Transform nodes through `nodeTypes`.

- [ ] **Step 1: Write the failing test**

Append to `src/components/flow/__tests__/NodePalette.test.tsx`:

```tsx
describe('NodePalette Transform entry', () => {
  it('adds a Transform node with the default script', async () => {
    const onAddNode = vi.fn();
    render(<NodePalette onAddNode={onAddNode} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Add node/ }));
    await user.click(screen.getByRole('menuitem', { name: 'Transform' }));
    expect(onAddNode).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^transform-/),
        position: { x: 100, y: 100 },
        kind: { kind: 'Transform', label: 'New Transform', script: 'return response.body;' },
      }),
    );
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test NodePalette`
Expected: FAIL, because there is no `Transform` menu item.

- [ ] **Step 3: Add the palette entry**

In `src/components/flow/NodePalette.tsx`:
- Add `Code` to the `lucide-react` import (keep the list alphabetical).
- Add `import { DEFAULT_TRANSFORM_SCRIPT } from '@/lib/flow-transform';` after the `@/lib/flow-repeat` import.
- Add this item after the `Switch` item:

```tsx
          <DropdownMenuItem
            onSelect={() =>
              onAddNode({
                id: newNodeId('transform'),
                kind: {
                  kind: 'Transform',
                  label: 'New Transform',
                  script: DEFAULT_TRANSFORM_SCRIPT,
                },
                position: defaultPosition,
              })
            }
          >
            <Code className='mr-2 h-3.5 w-3.5' aria-hidden='true' />
            Transform
          </DropdownMenuItem>
```

- [ ] **Step 4: Register the node type**

In `src/components/flow/FlowCanvas.tsx`, add `import { TransformNode } from './nodes/TransformNode';` after the `SwitchNode` import, and add `Transform: TransformNode,` to `nodeTypes` after `Switch: SwitchNode,`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test NodePalette FlowCanvas`
Expected: PASS, including the existing palette, canvas and routing tests.

- [ ] **Step 6: Commit**

Stage the three files, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): add Transform to the node palette`.

---

### Task 3: Canvas integration and edge styling

**Files:**
- Create: `src/components/flow/__tests__/FlowCanvas.transform.test.tsx`
- Modify: `src/components/flow/__tests__/flowExits.test.ts`

**Interfaces:**
- Consumes: `FlowCanvas` and `toRfEdges` from `../FlowCanvas`, `edgeRunState` from `../flowExits`, and the `Transform` type.
- Produces: regression tests only. If a test fails, fix production code in `FlowCanvas.tsx` or `flowExits.ts`, not the test.

- [ ] **Step 1: Write the tests**

Create `src/components/flow/__tests__/FlowCanvas.transform.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas, toRfEdges } from '../FlowCanvas';

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it measures a node.
class FakeMatrix {
  m22 = 1;
}
vi.stubGlobal('DOMMatrixReadOnly', FakeMatrix);

const request: FlowNode = {
  id: 'req',
  position: { x: 0, y: 0 },
  kind: { kind: 'Request', label: 'Login', source: { type: 'Saved', requestPath: 'a.yml' } },
};
const transform: FlowNode = {
  id: 'tf',
  position: { x: 300, y: 0 },
  kind: { kind: 'Transform', label: 'Pick token', script: 'return response.body.token;' },
};
const output: FlowNode = {
  id: 'out',
  position: { x: 600, y: 0 },
  kind: { kind: 'Output', label: 'Out' },
};
const edges: FlowEdge[] = [
  { id: 'e1', sourceNodeId: 'req', targetNodeId: 'tf', targetField: 'input', expression: '' },
  {
    id: 'e2',
    sourceNodeId: 'tf',
    targetNodeId: 'out',
    targetField: 'value',
    expression: 'response.body',
  },
];

describe('FlowCanvas with a Transform node', () => {
  it('renders the Transform node through nodeTypes', () => {
    render(
      <FlowCanvas
        nodes={[request, transform, output]}
        edges={edges}
        nodeStatus={{}}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
    expect(screen.getByTestId('transform-node-card')).toBeInTheDocument();
    expect(screen.getByText('Pick token')).toBeInTheDocument();
  });

  it('maps the wires onto the input and result handles', () => {
    const rf = toRfEdges(edges, [request, transform, output], {}, new Set());
    const into = rf.find((e) => e.id === 'e1');
    const outOf = rf.find((e) => e.id === 'e2');
    expect(into).toMatchObject({ source: 'req', target: 'tf', targetHandle: 'input' });
    expect(outOf).toMatchObject({
      source: 'tf',
      sourceHandle: 'result',
      target: 'out',
      targetHandle: 'value',
    });
    expect(outOf?.label).toBeUndefined();
  });

  it('a Transform never gets taken or not-taken styling', () => {
    const rf = toRfEdges(
      edges,
      [request, transform, output],
      { tf: 'success' },
      new Set(),
      { tf: { branch: 'true' } },
    );
    const outOf = rf.find((e) => e.id === 'e2');
    expect(outOf?.className).not.toMatch(/flow-edge-(taken|not-taken)/);
  });
});
```

Append to `src/components/flow/__tests__/flowExits.test.ts`, reusing its existing imports (add `edgeRunState` and the `FlowNode`/`FlowEdge` types to them if they are missing):

```ts
describe('edgeRunState for a Transform source', () => {
  const transform: FlowNode = {
    id: 'tf',
    position: { x: 0, y: 0 },
    kind: { kind: 'Transform', label: 'T', script: 'return 1;' },
  };
  const edge: FlowEdge = {
    id: 'e1',
    sourceNodeId: 'tf',
    targetNodeId: 'out',
    targetField: 'value',
    expression: '',
  };

  it('is always neutral, even when a branch is reported', () => {
    expect(edgeRunState(edge, transform, 'success', 'true')).toBe('neutral');
    expect(edgeRunState(edge, transform, 'success', 'result')).toBe('neutral');
  });
});
```

- [ ] **Step 2: Run the tests**

Run: `yarn test FlowCanvas flowExits`
Expected: PASS. These pin behavior that already holds (`edgeRunState` only colors If and Switch sources). If `toRfEdges` returns a different `className` shape than the regex expects, log `outOf?.className`, correct the regex, and keep the intent: no taken or not-taken class.

- [ ] **Step 3: Run the full frontend checks**

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no lint or format errors. If Biome reports formatting problems in files this plan touched, run `yarn format` and re-run `yarn check`.

Run: `yarn test flow`
Expected: PASS for every flow test file.

- [ ] **Step 4: Commit**

Stage the two test files and any file `yarn format` changed in this plan, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `test(flow): cover Transform canvas wiring`.

---

## Next Plan

**Next plan to execute:** `docs/superpowers/plans/flow-phase3-transform-node/2026-09-30-flow-phase3-transform-plan-05-panel-last-run-and-verification.md`

Do not start it in this run. Finish the review below, report to the user, and wait for them to start plan 05.

## Post-Implementation Review

Before plan 05 starts, dispatch one Opus-model subagent (read and fix allowed) with this brief: "Review every change made by plan 04 of `docs/superpowers/plans/flow-phase3-transform-node/`, using `git log` for its three commits. Check: (a) `TransformNode` follows the guardrails in `.claude/rules/frontend-component-guardrails.md` (shadcn primitives, lucide icons, no inline editor); (b) the node shows only one line of the script and cannot grow with it; (c) the palette entry and `nodeTypes` registration are the only production changes outside the new component; (d) `yarn tsc --noEmit`, `yarn check` and `yarn test flow` are green. Fix any defect you find and report what changed. Do not start plan 05."
