# Flow Accessibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the flow canvas usable with a screen reader and without relying on colour: every node and wire gets an accessible name that includes its run status, run progress is announced through live regions, the canvas is named and described, and each node header shows a status icon.

**Architecture:** Pure helpers (`nodeStatusLabel`, `flowNodeAriaLabel`, `flowEdgeAriaLabel`) build the names. `toRfNodes` and `toRfEdges` in `FlowCanvas.tsx` set `ariaLabel` on every React Flow node and edge, and the canvas passes `ariaLabelConfig`, an `aria-label` and an `aria-describedby` to `<ReactFlow>`. A pure diff function plus a hook (`useFlowRunAnnouncer`) turn status changes into messages that a `FlowRunAnnouncer` component renders into two always-mounted live regions in `FlowPane`. A `NodeStatusIcon` shows the status next to the label in all eight node components. Frontend only.

**Tech Stack:** React, TypeScript, `@xyflow/react` 12.12.0, Vitest and Testing Library, lucide-react.

**Spec:** Roadmap item F-45 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P16) and the plan index (`00-plan-index.md`, plan P16).

**Independent of other plans.** It edits `FlowCanvas.tsx` (object literals in `toRfNodes` and `toRfEdges`, the `<ReactFlow>` props), `FlowPane.tsx` (one JSX element) and the eight node headers. If plan P12 (issue badges) merged first, `toRfNodes` has an `issues` line and every node header has `<NodeIssueBadge />` above `<NodeMenuButton`; this plan only adds lines and does not touch those. Insert the status icon directly above `<NodeMenuButton`, whatever else is there. If plan P2 merged first, the result strip is a labelled `<section>` region (not a live region) and does not duplicate the announcements here. If plan P11 merged first, pass the announcer the **live** `tab.nodeStatus`, not the viewed run's maps.

**Findings from checking the code (design notes were stale or wrong in places):**
- `<ReactFlow>` already renders its wrapper with `role="application"` (`@xyflow/react` `index.js`, `ReactFlow` function). The notes suggest adding `role="application"` to the canvas wrapper; that would nest two application roles. This plan names the existing wrapper instead: `aria-label` and `aria-describedby` go on `<ReactFlow>`, whose extra props land on that wrapper. The outer `div` (`tabIndex={-1}`, `data-testid='flow-canvas'`) keeps no role.
- In 12.12.0 the node description that is actually rendered when keyboard accessibility is on (our case, `disableKeyboardA11y` is false) is the config key `'node.a11yDescription.keyboardDisabled'`, not `'.default'` (the names are inverted in the library, see `A11yDescriptions`). This plan overrides both keys to the same text.
- The existing hint panel ("Drag to select · Ctrl+A ...") describes mouse use, so it is not used as the description. A separate screen-reader-only paragraph carries the keyboard instructions.
- Nodes are role `group` but `visibility: hidden` in jsdom until measured, so tests read `aria-label` attributes from `.react-flow__node[data-id]` instead of `getByRole`.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- Do not break `deleteKeyCode={['Backspace', 'Delete']}`, Ctrl+A / Cmd+A select-all, or the `nokey` / `nodrag` classes and the `EDITABLE_TARGET` guard in `FlowCanvas.tsx`. Do not change the outer wrapper's `tabIndex={-1}`. The existing tests `FlowCanvas.test.tsx` (`deletes a selected node ...`, `selects every node on Ctrl+A`, `does not select all when Ctrl+A comes from an input`, `... from inside a .nokey element`) must stay green.
- A node's accessible name must never contain changing progress text such as "attempt 3/30".
- Live regions are always in the DOM and only their text changes.
- No Rust changes. No new dependency.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check` (if it only reports import order or formatting, run `yarn lint` and `yarn format`, review the diff, and re-check), and the targeted `yarn test <pattern>` listed in the task.
- Only one implementer at a time touches `FlowCanvas.tsx`, `FlowPane.tsx` and the node components.
- Not in scope: a full keyboard-only way to draw wires, roving focus, high-contrast themes, translating strings.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. The labels must not break existing keyboard behaviour: delete, Ctrl+A (also from a focused node) and the `nokey` guard. Tests pinned in Task 1 (new Ctrl+A-from-a-node test plus the existing suite).
2. A node name that includes progress or a long error would be re-read on every update or flood the reader. The name uses only label, kind, status and the first line of the error cut at 80 characters. Tests pinned in Task 1.
3. Live regions that are mounted late, or re-created on each render, are not announced. They must exist before any text and keep the same element. Test pinned in Task 2 (`FlowPane`).
4. Announcing on mount or tab switch, announcing every `running` transition, or announcing 30 nodes at once when the final summary lands in one render. Tests pinned in Task 2 (baseline, no running, collapse over five).
5. Status shown by colour alone: the icon must exist for every non-idle status in all eight node kinds, be hidden from assistive tech (the name already says the status) and not add to card text queries. Test pinned in Task 3.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/components/flow/nodes/nodeStatus.ts` (modify) | `nodeStatusLabel(status, detail)`. |
| `src/components/flow/flowA11y.ts` (new) | `flowNodeName`, `shortError`, `flowNodeAriaLabel`, `flowEdgeAriaLabel`. |
| `src/components/flow/FlowCanvas.tsx` (modify) | `ariaLabel` on nodes and edges, `ariaLabelConfig`, canvas name and description. |
| `src/components/flow/flowAnnounce.ts` (new) | Pure `diffAnnouncements` and its types. |
| `src/components/flow/useFlowRunAnnouncer.ts` (new) | Hook that keeps the latest messages. |
| `src/components/flow/FlowRunAnnouncer.tsx` (new) | The two live regions. |
| `src/components/flow/FlowPane.tsx` (modify) | Mounts `FlowRunAnnouncer`. |
| `src/components/flow/nodes/NodeStatusIcon.tsx` (new) | Status icon shared by all node headers. |
| `src/components/flow/nodes/*Node.tsx` (modify, 8 files) | Render `NodeStatusIcon`. |

Existing tests to know: `src/components/flow/__tests__/FlowCanvas.test.tsx` (`Harness`, `SelectHarness`, key tests, `toRfEdges` tests), `src/components/flow/__tests__/FlowPane.properties.test.tsx` (the store-driven `Harness` and the jsdom polyfills), `src/components/flow/nodes/__tests__/nodeStatus.test.ts`.

---

### Task 1: Names for nodes, wires and the canvas

**Files:**
- Modify: `src/components/flow/nodes/nodeStatus.ts`
- Modify: `src/components/flow/nodes/__tests__/nodeStatus.test.ts`
- Create: `src/components/flow/flowA11y.ts`
- Create: `src/components/flow/__tests__/flowA11y.test.ts`
- Modify: `src/components/flow/FlowCanvas.tsx`
- Create: `src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`

**Interfaces:**
- Produces: `nodeStatusLabel(status: FlowNodeStatus, detail?: { skipReason?: FlowSkipReason }): string` returning `not run`, `running`, `succeeded`, `failed`, `skipped, branch not taken` or `skipped, upstream failed`.
- Produces: `flowNodeName(kind: FlowNodeKind): string` (the label, or the lowercase kind name when blank).
- Produces: `shortError(error: string): string` (first line, at most 80 characters, ending in `…` when cut).
- Produces: `flowNodeAriaLabel(kind: FlowNodeKind, status: FlowNodeStatus, detail?: FlowNodeDetail): string`, for example `Fetch, request node, failed: boom`.
- Produces: `flowEdgeAriaLabel(edge: FlowEdge, byId: ReadonlyMap<string, FlowNode>, run: EdgeRunState): string`, for example `Wire from Check (true exit) to Result, value, taken`.

- [ ] **Step 1: Write the failing tests for the helpers**

1. In `src/components/flow/nodes/__tests__/nodeStatus.test.ts`, change the import to `import { nodeStatusCaption, nodeStatusClassName, nodeStatusLabel } from '../nodeStatus';` and append:

```ts
describe('nodeStatusLabel', () => {
  it('names every status in words', () => {
    expect(nodeStatusLabel('idle')).toBe('not run');
    expect(nodeStatusLabel('running')).toBe('running');
    expect(nodeStatusLabel('success')).toBe('succeeded');
    expect(nodeStatusLabel('failed')).toBe('failed');
  });

  it('says why a node was skipped', () => {
    expect(nodeStatusLabel('skipped', { skipReason: 'branch_not_taken' })).toBe(
      'skipped, branch not taken',
    );
    expect(nodeStatusLabel('skipped', { skipReason: 'upstream_failed' })).toBe(
      'skipped, upstream failed',
    );
  });

  it('treats a skip with no reason as an upstream failure, like the caption does', () => {
    expect(nodeStatusLabel('skipped')).toBe('skipped, upstream failed');
  });
});
```

2. Create `src/components/flow/__tests__/flowA11y.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import {
  flowEdgeAriaLabel,
  flowNodeAriaLabel,
  flowNodeName,
  shortError,
} from '../flowA11y';

const request: FlowNodeKind = {
  kind: 'Request',
  label: 'Fetch',
  source: { type: 'Saved', requestPath: 'a.yml' },
};

describe('flowNodeName', () => {
  it('uses the label, and the kind name when the label is blank', () => {
    expect(flowNodeName(request)).toBe('Fetch');
    expect(flowNodeName({ ...request, label: '  ' })).toBe('request');
    expect(flowNodeName({ kind: 'WaitForCallback', label: '', name: 'cb', timeoutMs: 1000 })).toBe(
      'wait for callback',
    );
  });
});

describe('shortError', () => {
  it('keeps the first line only', () => {
    expect(shortError('first line\nsecond line')).toBe('first line');
  });

  it('cuts a long error at 80 characters with an ellipsis', () => {
    const cut = shortError('x'.repeat(200));
    expect(cut).toHaveLength(80);
    expect(cut.endsWith('…')).toBe(true);
  });

  it('leaves a short error alone', () => {
    expect(shortError('boom')).toBe('boom');
  });
});

describe('flowNodeAriaLabel', () => {
  it('names label, kind and status', () => {
    expect(flowNodeAriaLabel(request, 'idle')).toBe('Fetch, request node, not run');
    expect(flowNodeAriaLabel(request, 'success')).toBe('Fetch, request node, succeeded');
  });

  it('adds the short error of a failed node', () => {
    expect(flowNodeAriaLabel(request, 'failed', { error: 'boom\nstack' })).toBe(
      'Fetch, request node, failed: boom',
    );
    expect(flowNodeAriaLabel(request, 'failed')).toBe('Fetch, request node, failed');
  });

  it('says why a node was skipped', () => {
    expect(flowNodeAriaLabel(request, 'skipped', { skipReason: 'branch_not_taken' })).toBe(
      'Fetch, request node, skipped, branch not taken',
    );
  });

  it('never includes the progress text, the value or an error of a node that did not fail', () => {
    const label = flowNodeAriaLabel(request, 'running', {
      progress: 'attempt 3/30',
      value: 'secret-value',
      error: 'old error',
    });
    expect(label).toBe('Fetch, request node, running');
  });
});

describe('flowEdgeAriaLabel', () => {
  const nodes: FlowNode[] = [
    { id: 'in', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'a' } },
    { id: 'if', position: { x: 0, y: 0 }, kind: { kind: 'If', label: 'Check', condition: 'x' } },
    { id: 'out', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    {
      id: 'sw',
      position: { x: 0, y: 0 },
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [{ id: 'c1', label: 'Pro plan', matches: 'pro' }],
      },
    },
  ];
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const edge = (over: Partial<FlowEdge>): FlowEdge => ({
    id: 'e1',
    sourceNodeId: 'in',
    targetNodeId: 'out',
    targetField: 'value',
    expression: '',
    ...over,
  });

  it('names both ends and the target field', () => {
    expect(flowEdgeAriaLabel(edge({}), byId, 'neutral')).toBe('Wire from User to Result, value');
  });

  it('names a routing exit and whether the run took it', () => {
    const fromIf = edge({ sourceNodeId: 'if', sourceHandle: 'true' });
    expect(flowEdgeAriaLabel(fromIf, byId, 'taken')).toBe(
      'Wire from Check (true exit) to Result, value, taken',
    );
    expect(flowEdgeAriaLabel(fromIf, byId, 'not-taken')).toBe(
      'Wire from Check (true exit) to Result, value, not taken',
    );
    const fromSwitch = edge({ sourceNodeId: 'sw', sourceHandle: 'case:c1' });
    expect(flowEdgeAriaLabel(fromSwitch, byId, 'neutral')).toBe(
      'Wire from Route (Pro plan exit) to Result, value',
    );
  });

  it('calls a trigger wire "run when"', () => {
    expect(flowEdgeAriaLabel(edge({ targetField: 'trigger' }), byId, 'neutral')).toBe(
      'Wire from User to Result, run when',
    );
  });

  it('falls back to the ids when an end is missing', () => {
    expect(flowEdgeAriaLabel(edge({ sourceNodeId: 'gone' }), byId, 'neutral')).toBe(
      'Wire from gone to Result, value',
    );
  });
});
```

3. Create `src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { FlowCanvas, toRfEdges } from '../FlowCanvas';

// The real CodeMirror editor needs react-query and Tauri mocks.
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

const trio: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
  { id: 'c', kind: { kind: 'Output', label: 'Gamma' }, position: { x: 600, y: 0 } },
];

const nodeEl = (id: string) => document.querySelector<HTMLElement>(`.react-flow__node[data-id="${id}"]`);

function renderCanvas(props: Partial<React.ComponentProps<typeof FlowCanvas>> = {}) {
  return render(
    <FlowCanvas
      nodes={trio}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      {...props}
    />,
  );
}

describe('FlowCanvas accessible names', () => {
  it('gives each node a name with its kind and status', () => {
    renderCanvas({ nodeStatus: { b: 'success' } });
    expect(nodeEl('a')).toHaveAttribute('aria-label', 'Alpha, output node, not run');
    expect(nodeEl('b')).toHaveAttribute('aria-label', 'Beta, output node, succeeded');
  });

  it('adds the short error of a failed node', () => {
    renderCanvas({ nodeStatus: { a: 'failed' }, nodeDetail: { a: { error: 'boom\nmore' } } });
    expect(nodeEl('a')).toHaveAttribute('aria-label', 'Alpha, output node, failed: boom');
  });

  it('does not put progress text in the name', () => {
    renderCanvas({
      nodeStatus: { a: 'running' },
      nodeDetail: { a: { progress: 'attempt 3/30' } },
    });
    expect(nodeEl('a')).toHaveAttribute('aria-label', 'Alpha, output node, running');
  });

  it('names the canvas and describes the keyboard use', () => {
    renderCanvas();
    const wrapper = screen.getByTestId('rf__wrapper');
    expect(wrapper).toHaveAttribute('aria-label', 'Flow canvas');
    const describedBy = wrapper.getAttribute('aria-describedby');
    expect(describedBy).toBeTruthy();
    const description = document.getElementById(describedBy ?? '');
    expect(description).toHaveTextContent('Ctrl+A');
    expect(description).toHaveTextContent('Delete');
  });

  it('keeps the outer wrapper focusable by script only and without a role', () => {
    renderCanvas();
    const outer = screen.getByTestId('flow-canvas');
    expect(outer).toHaveAttribute('tabindex', '-1');
    expect(outer).not.toHaveAttribute('role');
  });

  it('describes wires in the keyboard help text with the flow wording', () => {
    renderCanvas();
    expect(document.body.textContent).toContain('Press Enter or Space to select this step');
    expect(document.body.textContent).toContain('Press Enter or Space to select this wire');
  });
});

describe('toRfEdges accessible names', () => {
  const edges: FlowEdge[] = [
    {
      id: 'e1',
      sourceNodeId: 'a',
      targetNodeId: 'b',
      targetField: 'value',
      expression: 'response.body',
    },
  ];

  it('sets an aria label that names both ends', () => {
    const rf = toRfEdges(edges, trio, {}, new Set());
    expect(rf[0].ariaLabel).toBe('Wire from Alpha to Beta, value');
  });
});

describe('FlowCanvas keyboard behaviour with names in place', () => {
  function SelectHarness({ onSelect }: { onSelect: (ids: ReadonlySet<string>) => void }) {
    const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
    return (
      <FlowCanvas
        nodes={trio}
        edges={[]}
        nodeStatus={{ a: 'success' }}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
        selectedNodeIds={selected}
        onSelectedNodeIdsChange={(ids) => {
          onSelect(ids);
          setSelected(ids);
        }}
      />
    );
  }

  it('still selects every node on Ctrl+A pressed on a focused node', async () => {
    const onSelect = vi.fn();
    render(<SelectHarness onSelect={onSelect} />);
    const node = nodeEl('a');
    expect(node).not.toBeNull();
    fireEvent.keyDown(node as HTMLElement, { key: 'a', ctrlKey: true });
    await waitFor(() => expect(onSelect).toHaveBeenLastCalledWith(new Set(['a', 'b', 'c'])));
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/nodes/__tests__/nodeStatus.test.ts src/components/flow/__tests__/flowA11y.test.ts src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`
Expected: FAIL (`nodeStatusLabel` is not exported, `../flowA11y` does not resolve, no `aria-label` on nodes).

- [ ] **Step 3: Add `nodeStatusLabel`**

At the end of `src/components/flow/nodes/nodeStatus.ts`, add:

```ts
// The status in words, for accessible names and announcements. A skip with no
// reason predates Phase 2 and can only mean an upstream failure, as in the caption.
export function nodeStatusLabel(
  status: FlowNodeStatus,
  detail?: { skipReason?: FlowSkipReason },
): string {
  switch (status) {
    case 'idle':
      return 'not run';
    case 'running':
      return 'running';
    case 'success':
      return 'succeeded';
    case 'failed':
      return 'failed';
    case 'skipped':
      return detail?.skipReason === 'branch_not_taken'
        ? 'skipped, branch not taken'
        : 'skipped, upstream failed';
  }
}
```

- [ ] **Step 4: Write `flowA11y.ts`**

Create `src/components/flow/flowA11y.ts`:

```ts
import { RESULT_HANDLE } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { type EdgeRunState, exitLabel } from './flowExits';
import { nodeStatusLabel } from './nodes/nodeStatus';

const KIND_NAMES: Record<FlowNodeKind['kind'], string> = {
  Request: 'request',
  Input: 'input',
  Output: 'output',
  If: 'if',
  Switch: 'switch',
  WaitForCallback: 'wait for callback',
  Transform: 'transform',
  Auth: 'auth',
};

const MAX_ERROR_CHARS = 80;

// What a screen reader calls a node: its label, or its kind when the label is blank.
export function flowNodeName(kind: FlowNodeKind): string {
  return kind.label.trim() || KIND_NAMES[kind.kind];
}

// First line of an error, cut so that one failure cannot flood a reader.
export function shortError(error: string): string {
  const first = (error.split('\n')[0] ?? '').trim();
  return first.length > MAX_ERROR_CHARS ? `${first.slice(0, MAX_ERROR_CHARS - 1)}…` : first;
}

// Label, kind and status only. Progress text, values and exchanges change too
// often or are too long, and the node's own content is read separately.
export function flowNodeAriaLabel(
  kind: FlowNodeKind,
  status: FlowNodeStatus,
  detail?: FlowNodeDetail,
): string {
  const base = `${flowNodeName(kind)}, ${KIND_NAMES[kind.kind]} node, ${nodeStatusLabel(status, detail)}`;
  return status === 'failed' && detail?.error ? `${base}: ${shortError(detail.error)}` : base;
}

const targetFieldLabel = (field: string) => (field === 'trigger' ? 'run when' : field);

export function flowEdgeAriaLabel(
  edge: FlowEdge,
  byId: ReadonlyMap<string, FlowNode>,
  run: EdgeRunState,
): string {
  const source = byId.get(edge.sourceNodeId);
  const target = byId.get(edge.targetNodeId);
  const exit = source ? exitLabel(source.kind, edge.sourceHandle ?? RESULT_HANDLE) : undefined;
  const from = source ? flowNodeName(source.kind) : edge.sourceNodeId;
  const to = target ? flowNodeName(target.kind) : edge.targetNodeId;
  const state = run === 'taken' ? ', taken' : run === 'not-taken' ? ', not taken' : '';
  return `Wire from ${from}${exit ? ` (${exit} exit)` : ''} to ${to}, ${targetFieldLabel(edge.targetField)}${state}`;
}
```

- [ ] **Step 5: Wire the names into `FlowCanvas.tsx`**

1. Imports. Change the React import to `import { useId, useMemo, useRef, useState } from 'react';`. Add `type AriaLabelConfig,` to the `@xyflow/react` import list (alphabetical position first). Add next to the `./flowExits` import:

```tsx
import { flowEdgeAriaLabel, flowNodeAriaLabel } from './flowA11y';
```

2. Above `toRfNodes`, add the config constant:

```tsx
// Wording for this canvas. In 12.12.0 the description shown while keyboard use is
// enabled is the key named `keyboardDisabled`, so both node keys get the same text.
const NODE_HELP =
  'Press Enter or Space to select this step. With it selected, use the arrow keys to move it, Delete or Backspace to remove it, and Escape to cancel.';
const ARIA_LABEL_CONFIG: Partial<AriaLabelConfig> = {
  'node.a11yDescription.default': NODE_HELP,
  'node.a11yDescription.keyboardDisabled': NODE_HELP,
  'edge.a11yDescription.default':
    'Press Enter or Space to select this wire. With it selected, press Delete or Backspace to remove it, or Escape to cancel.',
};
```

3. In `toRfNodes`, in the returned object, directly after `type: n.kind.kind,` add:

```tsx
      ariaLabel: flowNodeAriaLabel(n.kind, nodeStatus[n.id] ?? 'idle', nodeDetail?.[n.id]),
```

4. In `toRfEdges`, in the returned object, directly after `id: e.id,` add:

```tsx
      ariaLabel: flowEdgeAriaLabel(e, byId, run),
```

(`byId` and `run` already exist in that function.)

5. In `FlowCanvasInner`, after the `const paneRef = useRef<HTMLDivElement>(null);` line, add:

```tsx
  const helpId = useId();
```

6. In the JSX, inside the outer `<div ... data-testid='flow-canvas'>`, as the first child before `<FlowNodeActionsContext.Provider`, add:

```tsx
      <p id={helpId} className='sr-only'>
        Flow canvas. Press Tab to move between steps. Press Enter to select a step, Delete or
        Backspace to remove it, and Ctrl+A to select every step. Right-drag or scroll to pan.
      </p>
```

and on the `<ReactFlow` element add these three props (anywhere in the prop list, for example after `nodeTypes={nodeTypes}`):

```tsx
          aria-label='Flow canvas'
          aria-describedby={helpId}
          ariaLabelConfig={ARIA_LABEL_CONFIG}
```

Do not touch `deleteKeyCode`, `handleKeyDown`, `EDITABLE_TARGET`, the `tabIndex={-1}` on the outer `div` or the hint `Panel`.

- [ ] **Step 6: Run to verify the tests pass**

Run: `yarn test src/components/flow src/components/flow/nodes`
Expected: PASS, including the whole existing `FlowCanvas.test.tsx` and every `FlowPane.*` test. If a test that compares a node's accessible name or `getByLabelText` finds a second match, the new `aria-label` on the node wrapper (for example `Result, output node, not run`) overlaps an existing label query: narrow that query, do not remove the label.

- [ ] **Step 7: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/nodes/nodeStatus.ts src/components/flow/nodes/__tests__/nodeStatus.test.ts src/components/flow/flowA11y.ts src/components/flow/__tests__/flowA11y.test.ts src/components/flow/FlowCanvas.tsx src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`
Suggested subject: `feat(flow): name nodes, wires and the canvas for screen readers`.

---

### Task 2: Run announcements

**Files:**
- Create: `src/components/flow/flowAnnounce.ts`
- Create: `src/components/flow/__tests__/flowAnnounce.test.ts`
- Create: `src/components/flow/useFlowRunAnnouncer.ts`
- Create: `src/components/flow/FlowRunAnnouncer.tsx`
- Create: `src/components/flow/__tests__/useFlowRunAnnouncer.test.tsx`
- Modify: `src/components/flow/FlowPane.tsx`
- Create: `src/components/flow/__tests__/FlowPane.announcer.test.tsx`

**Interfaces:**
- Consumes: `flowNodeName`, `shortError` from Task 1; `nodeStatusLabel`.
- Produces:

```ts
type AnnounceRunState = 'idle' | 'running' | 'done';
interface AnnounceSnapshot { runState: AnnounceRunState; status: Record<string, FlowNodeStatus> }
interface Announcements { polite: string[]; alerts: string[]; runStarted: boolean }
diffAnnouncements(prev: AnnounceSnapshot, next: AnnounceSnapshot, nodes: FlowNode[], nodeDetail?: Record<string, FlowNodeDetail>): Announcements
MAX_NODE_MESSAGES = 5
useFlowRunAnnouncer(nodes, nodeStatus, nodeDetail, runState): { polite: string; alert: string }
<FlowRunAnnouncer nodes nodeStatus nodeDetail runState />
```

The two regions are `data-testid='flow-announcer-status'` (`role='status'`, `aria-live='polite'`, `aria-atomic='true'`) and `data-testid='flow-announcer-alert'` (`role='alert'`), both `sr-only`.

- [ ] **Step 1: Write the failing tests for the diff**

Create `src/components/flow/__tests__/flowAnnounce.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { type AnnounceSnapshot, diffAnnouncements, MAX_NODE_MESSAGES } from '../flowAnnounce';

const nodes: FlowNode[] = [
  { id: 'a', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Alpha' } },
  { id: 'b', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Beta' } },
  { id: 'c', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Gamma' } },
];

const snap = (
  runState: AnnounceSnapshot['runState'],
  status: Record<string, FlowNodeStatus> = {},
): AnnounceSnapshot => ({ runState, status });

describe('diffAnnouncements', () => {
  it('says nothing when nothing changed', () => {
    const s = snap('done', { a: 'success' });
    expect(diffAnnouncements(s, s, nodes)).toEqual({ polite: [], alerts: [], runStarted: false });
  });

  it('announces the start of a run once', () => {
    const out = diffAnnouncements(snap('idle'), snap('running'), nodes);
    expect(out.polite).toEqual(['Run started.']);
    expect(out.runStarted).toBe(true);
    expect(diffAnnouncements(snap('running'), snap('running'), nodes).polite).toEqual([]);
  });

  it('does not announce a node that is merely running', () => {
    const out = diffAnnouncements(snap('running'), snap('running', { a: 'running' }), nodes);
    expect(out.polite).toEqual([]);
    expect(out.alerts).toEqual([]);
  });

  it('announces a node that succeeded or was skipped, with the reason', () => {
    const out = diffAnnouncements(
      snap('running', { a: 'running' }),
      snap('running', { a: 'success', b: 'skipped' }),
      nodes,
      { b: { skipReason: 'branch_not_taken' } },
    );
    expect(out.polite).toEqual(['Alpha succeeded.', 'Beta skipped, branch not taken.']);
  });

  it('sends a failure to the alert list with the short error', () => {
    const out = diffAnnouncements(snap('running'), snap('running', { a: 'failed' }), nodes, {
      a: { error: 'boom\nstack trace' },
    });
    expect(out.alerts).toEqual(['Alpha failed: boom.']);
    expect(out.polite).toEqual([]);
  });

  it('announces each terminal change only once', () => {
    const before = snap('running', { a: 'success' });
    const after = snap('running', { a: 'success', b: 'success' });
    expect(diffAnnouncements(before, after, nodes).polite).toEqual(['Beta succeeded.']);
  });

  it('ignores nodes that are not on the canvas', () => {
    const out = diffAnnouncements(snap('running'), snap('running', { ghost: 'success' }), nodes);
    expect(out.polite).toEqual([]);
  });

  it('summarises a finished run', () => {
    const out = diffAnnouncements(
      snap('running', { a: 'success', b: 'failed', c: 'skipped' }),
      snap('done', { a: 'success', b: 'failed', c: 'skipped' }),
      nodes,
    );
    expect(out.polite).toEqual(['Run finished: 1 succeeded, 1 failed, 1 skipped.']);
  });

  it('collapses a burst of node results to the failures and the summary', () => {
    const many: FlowNode[] = Array.from({ length: MAX_NODE_MESSAGES + 3 }, (_, i) => ({
      id: `n${i}`,
      position: { x: 0, y: 0 },
      kind: { kind: 'Output' as const, label: `Node ${i}` },
    }));
    const finalStatus: Record<string, FlowNodeStatus> = Object.fromEntries(
      many.map((n, i) => [n.id, i === 0 ? 'failed' : 'success']),
    );
    const out = diffAnnouncements(snap('running'), snap('done', finalStatus), many, {
      n0: { error: 'boom' },
    });
    expect(out.polite).toEqual([
      `Run finished: ${many.length - 1} succeeded, 1 failed, 0 skipped.`,
    ]);
    expect(out.alerts).toEqual(['Node 0 failed: boom.']);
  });

  it('caps the failure list and counts the rest', () => {
    const many: FlowNode[] = Array.from({ length: MAX_NODE_MESSAGES + 2 }, (_, i) => ({
      id: `n${i}`,
      position: { x: 0, y: 0 },
      kind: { kind: 'Output' as const, label: `Node ${i}` },
    }));
    const failed: Record<string, FlowNodeStatus> = Object.fromEntries(
      many.map((n) => [n.id, 'failed']),
    );
    const out = diffAnnouncements(snap('running'), snap('running', failed), many);
    expect(out.alerts).toHaveLength(MAX_NODE_MESSAGES + 1);
    expect(out.alerts[MAX_NODE_MESSAGES]).toBe('and 2 more failed.');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/flowAnnounce.test.ts`
Expected: FAIL, cannot resolve `../flowAnnounce`.

- [ ] **Step 3: Write the diff**

Create `src/components/flow/flowAnnounce.ts`:

```ts
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { flowNodeName, shortError } from './flowA11y';
import { nodeStatusLabel } from './nodes/nodeStatus';

export type AnnounceRunState = 'idle' | 'running' | 'done';

export interface AnnounceSnapshot {
  runState: AnnounceRunState;
  status: Record<string, FlowNodeStatus>;
}

export interface Announcements {
  // Spoken politely, in order.
  polite: string[];
  // Failures, spoken at once.
  alerts: string[];
  // True when a new run began, so the owner can clear the last failures.
  runStarted: boolean;
}

// More node results than this in one update (for example the final summary
// landing at once) are replaced by the run summary.
export const MAX_NODE_MESSAGES = 5;

const TERMINAL: ReadonlySet<FlowNodeStatus> = new Set<FlowNodeStatus>([
  'success',
  'failed',
  'skipped',
]);

// Compares two snapshots and says what changed. A node that is only `running`
// is not announced. Failures go to `alerts`, never to `polite`.
export function diffAnnouncements(
  prev: AnnounceSnapshot,
  next: AnnounceSnapshot,
  nodes: FlowNode[],
  nodeDetail?: Record<string, FlowNodeDetail>,
): Announcements {
  const polite: string[] = [];
  const failures: string[] = [];
  const results: string[] = [];
  const runStarted = prev.runState !== 'running' && next.runState === 'running';
  if (runStarted) polite.push('Run started.');

  const byId = new Map(nodes.map((n) => [n.id, n]));
  for (const [id, status] of Object.entries(next.status)) {
    if (!TERMINAL.has(status) || prev.status[id] === status) continue;
    const node = byId.get(id);
    if (!node) continue;
    const name = flowNodeName(node.kind);
    const detail = nodeDetail?.[id];
    if (status === 'failed') {
      failures.push(`${name} failed${detail?.error ? `: ${shortError(detail.error)}` : ''}.`);
    } else {
      results.push(`${name} ${nodeStatusLabel(status, detail)}.`);
    }
  }
  if (results.length <= MAX_NODE_MESSAGES) polite.push(...results);

  if (prev.runState === 'running' && next.runState === 'done') {
    const statuses = nodes.map((n) => next.status[n.id]);
    const count = (s: FlowNodeStatus) => statuses.filter((x) => x === s).length;
    polite.push(
      `Run finished: ${count('success')} succeeded, ${count('failed')} failed, ${count('skipped')} skipped.`,
    );
  }

  const alerts = failures.slice(0, MAX_NODE_MESSAGES);
  if (failures.length > MAX_NODE_MESSAGES) {
    alerts.push(`and ${failures.length - MAX_NODE_MESSAGES} more failed.`);
  }
  return { polite, alerts, runStarted };
}
```

- [ ] **Step 4: Run to verify the diff tests pass**

Run: `yarn test src/components/flow/__tests__/flowAnnounce.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the failing hook test**

Create `src/components/flow/__tests__/useFlowRunAnnouncer.test.tsx`:

```tsx
import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { useFlowRunAnnouncer } from '../useFlowRunAnnouncer';

const nodes: FlowNode[] = [
  { id: 'a', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Alpha' } },
];

interface Props {
  status: Record<string, FlowNodeStatus>;
  detail?: Record<string, FlowNodeDetail>;
  runState: 'idle' | 'running' | 'done';
}

function setup(initial: Props) {
  return renderHook(
    (p: Props) => useFlowRunAnnouncer(nodes, p.status, p.detail, p.runState),
    { initialProps: initial },
  );
}

describe('useFlowRunAnnouncer', () => {
  it('is silent on mount, even when the tab already holds a finished run', () => {
    const { result } = setup({ status: { a: 'success' }, runState: 'done' });
    expect(result.current).toEqual({ polite: '', alert: '' });
  });

  it('is silent when mounted in the middle of a run', () => {
    const { result } = setup({ status: {}, runState: 'running' });
    expect(result.current).toEqual({ polite: '', alert: '' });
  });

  it('announces a run from start to finish', () => {
    const { result, rerender } = setup({ status: {}, runState: 'idle' });
    rerender({ status: {}, runState: 'running' });
    expect(result.current.polite).toBe('Run started.');
    rerender({ status: { a: 'success' }, runState: 'running' });
    expect(result.current.polite).toBe('Alpha succeeded.');
    rerender({ status: { a: 'success' }, runState: 'done' });
    expect(result.current.polite).toBe('Run finished: 1 succeeded, 0 failed, 0 skipped.');
  });

  it('says "Run started." again for a second run, with other text in between', () => {
    const { result, rerender } = setup({ status: {}, runState: 'idle' });
    rerender({ status: {}, runState: 'running' });
    const first = result.current.polite;
    rerender({ status: { a: 'success' }, runState: 'done' });
    expect(result.current.polite).not.toBe(first);
    rerender({ status: {}, runState: 'running' });
    expect(result.current.polite).toBe(first);
  });

  it('puts a failure in the alert text and clears it when the next run starts', () => {
    const { result, rerender } = setup({ status: {}, runState: 'running' });
    rerender({ status: { a: 'failed' }, detail: { a: { error: 'boom' } }, runState: 'running' });
    expect(result.current.alert).toBe('Alpha failed: boom.');
    rerender({ status: { a: 'failed' }, detail: { a: { error: 'boom' } }, runState: 'done' });
    rerender({ status: {}, runState: 'running' });
    expect(result.current.alert).toBe('');
  });

  it('keeps the last text when only progress details change', () => {
    const { result, rerender } = setup({ status: {}, runState: 'idle' });
    rerender({ status: {}, runState: 'running' });
    rerender({ status: {}, detail: { a: { progress: 'attempt 2/5' } }, runState: 'running' });
    expect(result.current.polite).toBe('Run started.');
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/useFlowRunAnnouncer.test.tsx`
Expected: FAIL, cannot resolve `../useFlowRunAnnouncer`.

- [ ] **Step 7: Write the hook and the component**

Create `src/components/flow/useFlowRunAnnouncer.ts`:

```ts
import { useEffect, useRef, useState } from 'react';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { type AnnounceRunState, type AnnounceSnapshot, diffAnnouncements } from './flowAnnounce';

// Turns status changes into the text of two live regions. The first render is
// the baseline, so opening a tab never announces an old run.
export function useFlowRunAnnouncer(
  nodes: FlowNode[],
  nodeStatus: Record<string, FlowNodeStatus>,
  nodeDetail: Record<string, FlowNodeDetail> | undefined,
  runState: AnnounceRunState,
): { polite: string; alert: string } {
  const previous = useRef<AnnounceSnapshot>({ runState, status: nodeStatus });
  const [messages, setMessages] = useState({ polite: '', alert: '' });

  useEffect(() => {
    const next: AnnounceSnapshot = { runState, status: nodeStatus };
    const out = diffAnnouncements(previous.current, next, nodes, nodeDetail);
    previous.current = next;
    if (out.polite.length === 0 && out.alerts.length === 0 && !out.runStarted) return;
    setMessages((current) => ({
      polite: out.polite.length > 0 ? out.polite.join(' ') : current.polite,
      alert: out.alerts.length > 0 ? out.alerts.join(' ') : out.runStarted ? '' : current.alert,
    }));
  }, [nodes, nodeStatus, nodeDetail, runState]);

  return messages;
}
```

Create `src/components/flow/FlowRunAnnouncer.tsx`:

```tsx
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import type { AnnounceRunState } from './flowAnnounce';
import { useFlowRunAnnouncer } from './useFlowRunAnnouncer';

interface FlowRunAnnouncerProps {
  nodes: FlowNode[];
  nodeStatus: Record<string, FlowNodeStatus>;
  nodeDetail?: Record<string, FlowNodeDetail>;
  runState: AnnounceRunState;
}

// Two screen-reader-only regions. They are always rendered, because a live
// region is only announced when it already exists before its text changes.
export function FlowRunAnnouncer({ nodes, nodeStatus, nodeDetail, runState }: FlowRunAnnouncerProps) {
  const { polite, alert } = useFlowRunAnnouncer(nodes, nodeStatus, nodeDetail, runState);
  return (
    <>
      <div
        data-testid='flow-announcer-status'
        role='status'
        aria-live='polite'
        aria-atomic='true'
        className='sr-only'
      >
        {polite}
      </div>
      <div data-testid='flow-announcer-alert' role='alert' className='sr-only'>
        {alert}
      </div>
    </>
  );
}
```

- [ ] **Step 8: Run to verify the hook tests pass**

Run: `yarn test src/components/flow/__tests__/useFlowRunAnnouncer.test.tsx src/components/flow/__tests__/flowAnnounce.test.ts`
Expected: PASS.

- [ ] **Step 9: Write the failing FlowPane test**

Create `src/components/flow/__tests__/FlowPane.announcer.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowRunFinished: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const tabId = 'flow-announce-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: announce',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'announce',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const polite = () => screen.getByTestId('flow-announcer-status');
const alert = () => screen.getByTestId('flow-announcer-alert');

describe('FlowPane run announcer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('has both live regions in the document, empty, before anything happens', () => {
    render(<Harness />);
    expect(polite()).toBeEmptyDOMElement();
    expect(alert()).toBeEmptyDOMElement();
    expect(polite()).toHaveAttribute('aria-live', 'polite');
    expect(polite()).toHaveAttribute('aria-atomic', 'true');
    expect(alert()).toHaveAttribute('role', 'alert');
  });

  it('announces a run and keeps the same region elements throughout', () => {
    render(<Harness />);
    const status = polite();
    const failure = alert();
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1'));
    expect(polite()).toHaveTextContent('Run started.');
    act(() => usePaneStore.getState().patchFlowNodeStatus(tabId, 'out1', 'success', { value: '"ok"' }));
    expect(polite()).toHaveTextContent('Result succeeded.');
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1'));
    expect(polite()).toHaveTextContent('Run finished: 1 succeeded, 0 failed, 0 skipped.');
    expect(polite()).toBe(status);
    expect(alert()).toBe(failure);
  });

  it('puts a failed node in the alert region and clears it on the next run', () => {
    render(<Harness />);
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1'));
    act(() =>
      usePaneStore.getState().patchFlowNodeStatus(tabId, 'out1', 'failed', { error: 'boom' }),
    );
    expect(alert()).toHaveTextContent('Result failed: boom.');
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1'));
    act(() => usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2'));
    expect(alert()).toBeEmptyDOMElement();
  });

  it('does not announce an old result when the tab is opened', () => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab({
      ...baseTab,
      runState: 'done',
      runId: 'run-0',
      nodeStatus: { out1: 'failed' },
      nodeDetail: { out1: { error: 'old' } },
    });
    render(<Harness />);
    expect(polite()).toBeEmptyDOMElement();
    expect(alert()).toBeEmptyDOMElement();
  });
});
```

- [ ] **Step 10: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowPane.announcer.test.tsx`
Expected: FAIL (`flow-announcer-status` not found).

- [ ] **Step 11: Mount the announcer in `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

1. Add the import: `import { FlowRunAnnouncer } from './FlowRunAnnouncer';`.
2. Inside `<div ref={canvasAreaRef} className='relative h-full'>`, directly above `<NodePalette`, add:

```tsx
          <FlowRunAnnouncer
            nodes={tab.nodes}
            nodeStatus={tab.nodeStatus}
            nodeDetail={tab.nodeDetail}
            runState={tab.runState}
          />
```

The announcer sits after the picker early return, so it only exists for an open flow, and it always reads the live `tab` maps.

- [ ] **Step 12: Run to verify the tests pass**

Run: `yarn test src/components/flow`
Expected: PASS, including every existing `FlowPane.*` test.

- [ ] **Step 13: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/flowAnnounce.ts src/components/flow/useFlowRunAnnouncer.ts src/components/flow/FlowRunAnnouncer.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/flowAnnounce.test.ts src/components/flow/__tests__/useFlowRunAnnouncer.test.tsx src/components/flow/__tests__/FlowPane.announcer.test.tsx`
Suggested subject: `feat(flow): announce run progress to screen readers`.

---

### Task 3: Status icons, so status is not colour alone

**Files:**
- Create: `src/components/flow/nodes/NodeStatusIcon.tsx`
- Create: `src/components/flow/nodes/__tests__/NodeStatusIcon.test.tsx`
- Modify: `src/components/flow/nodes/AuthNode.tsx`, `IfNode.tsx`, `InputNode.tsx`, `OutputNode.tsx`, `RequestNode.tsx`, `SwitchNode.tsx`, `TransformNode.tsx`, `WaitForCallbackNode.tsx`
- Modify: `src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`

**Interfaces:**
- Produces: `<NodeStatusIcon status={FlowNodeStatus} />`. Renders nothing for `idle`. Otherwise a `span` with `data-testid='node-status-icon'`, `data-status`, `aria-hidden='true'` holding a lucide icon: `Loader2` (running, spinning, not under reduced motion), `CheckCircle2` (success), `XCircle` (failed), `MinusCircle` (skipped).

- [ ] **Step 1: Write the failing tests**

1. Create `src/components/flow/nodes/__tests__/NodeStatusIcon.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowNodeStatus } from '@/lib/tauri-api';
import { NodeStatusIcon } from '../NodeStatusIcon';

describe('NodeStatusIcon', () => {
  it('renders nothing for an idle node', () => {
    const { container } = render(<NodeStatusIcon status='idle' />);
    expect(container).toBeEmptyDOMElement();
  });

  it.each<FlowNodeStatus>(['running', 'success', 'failed', 'skipped'])(
    'renders a hidden icon for %s',
    (status) => {
      render(<NodeStatusIcon status={status} />);
      const icon = screen.getByTestId('node-status-icon');
      expect(icon).toHaveAttribute('data-status', status);
      expect(icon).toHaveAttribute('aria-hidden', 'true');
      expect(icon.textContent).toBe('');
      expect(icon.querySelector('svg')).not.toBeNull();
    },
  );

  it('spins only while running, and not for users who prefer reduced motion', () => {
    const { rerender } = render(<NodeStatusIcon status='running' />);
    const cls = screen.getByTestId('node-status-icon').querySelector('svg')?.getAttribute('class');
    expect(cls).toContain('animate-spin');
    expect(cls).toContain('motion-reduce:animate-none');
    rerender(<NodeStatusIcon status='success' />);
    expect(screen.getByTestId('node-status-icon').querySelector('svg')?.getAttribute('class')).not.toContain(
      'animate-spin',
    );
  });
});
```

2. In `src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`, add at the end of the file:

```tsx
describe('FlowCanvas status icons', () => {
  const at = { x: 0, y: 0 };
  const everyKind: FlowNode[] = [
    {
      id: 'auth',
      position: at,
      kind: {
        kind: 'Auth',
        label: 'Sign in',
        auth: { authType: 'bearer', token: 't' },
        applyToInherit: false,
      },
    },
    {
      id: 'req',
      position: at,
      kind: {
        kind: 'Request',
        label: 'Fetch',
        source: { type: 'Inline', request: { method: 'GET', url: 'https://x.test', headers: [] } },
      },
    },
    { id: 'in', position: at, kind: { kind: 'Input', label: 'Key', value: 'k' } },
    { id: 'out', position: at, kind: { kind: 'Output', label: 'Shown' } },
    { id: 'if', position: at, kind: { kind: 'If', label: 'Check', condition: 'true' } },
    {
      id: 'sw',
      position: at,
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [{ id: 'c1', label: 'One', matches: '1' }],
      },
    },
    { id: 'tf', position: at, kind: { kind: 'Transform', label: 'Pick', script: 'return 1;' } },
    {
      id: 'wait',
      position: at,
      kind: { kind: 'WaitForCallback', label: 'Hook', name: 'cb', timeoutMs: 60000 },
    },
  ];
  const cardIds = [
    'auth-node-card',
    'request-node-card',
    'input-node-card',
    'output-node-card',
    'if-node-card',
    'switch-node-card',
    'transform-node-card',
    'wait-node-card',
  ];
  const card = (id: string) => document.querySelector<HTMLElement>(`[data-testid="${id}"]`);

  function renderKinds(status: 'idle' | 'success' | 'failed' | 'skipped' | 'running') {
    return render(
      <FlowCanvas
        nodes={everyKind}
        edges={[]}
        nodeStatus={Object.fromEntries(everyKind.map((n) => [n.id, status]))}
        onNodesChange={vi.fn()}
        onEdgesChange={vi.fn()}
        onConnect={vi.fn()}
      />,
    );
  }

  it('shows no icon on idle nodes', () => {
    renderKinds('idle');
    for (const id of cardIds) {
      expect(card(id)?.querySelector('[data-testid="node-status-icon"]'), id).toBeNull();
    }
  });

  it.each(['running', 'success', 'failed', 'skipped'] as const)(
    'shows a %s icon in the header of all eight node kinds',
    (status) => {
      renderKinds(status);
      for (const id of cardIds) {
        const icon = card(id)?.querySelector('[data-testid="node-status-icon"]');
        expect(icon, id).toHaveAttribute('data-status', status);
        expect(icon, id).toHaveAttribute('aria-hidden', 'true');
      }
    },
  );

  it('keeps the status in each node name, so the icon is not the only carrier', () => {
    renderKinds('failed');
    expect(nodeEl('req')).toHaveAttribute('aria-label', 'Fetch, request node, failed');
    expect(nodeEl('wait')).toHaveAttribute('aria-label', 'Hook, wait for callback node, failed');
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/nodes/__tests__/NodeStatusIcon.test.tsx src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`
Expected: FAIL (cannot resolve `../NodeStatusIcon`; no `node-status-icon` in the canvas).

- [ ] **Step 3: Create the icon**

Create `src/components/flow/nodes/NodeStatusIcon.tsx`:

```tsx
import { CheckCircle2, Loader2, MinusCircle, XCircle } from 'lucide-react';
import type { FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

// Shows the run status as a shape as well as a colour. It is hidden from
// assistive technology because the node's accessible name already says the status.
export function NodeStatusIcon({ status }: { status: FlowNodeStatus }) {
  if (status === 'idle') return null;
  const icon =
    status === 'running' ? (
      <Loader2
        className={cn('h-3.5 w-3.5 text-blue-500', 'animate-spin motion-reduce:animate-none')}
      />
    ) : status === 'success' ? (
      <CheckCircle2 className='h-3.5 w-3.5 text-green-600' />
    ) : status === 'failed' ? (
      <XCircle className='h-3.5 w-3.5 text-red-600' />
    ) : (
      <MinusCircle className='h-3.5 w-3.5 text-muted-foreground' />
    );
  return (
    <span
      data-testid='node-status-icon'
      data-status={status}
      aria-hidden='true'
      className='inline-flex shrink-0'
    >
      {icon}
    </span>
  );
}
```

- [ ] **Step 4: Add it to the eight node headers**

In each of `AuthNode.tsx`, `IfNode.tsx`, `InputNode.tsx`, `OutputNode.tsx`, `RequestNode.tsx`, `SwitchNode.tsx`, `TransformNode.tsx` and `WaitForCallbackNode.tsx` (all under `src/components/flow/nodes/`):

1. Add the import (keep Biome's order): `import { NodeStatusIcon } from './NodeStatusIcon';`
2. In the card header, add this line immediately above `<NodeMenuButton` (if plan P12 has merged there is a `<NodeIssueBadge ... />` line above it too; leave that line where it is):

```tsx
        <NodeStatusIcon status={data.status} />
```

`data.status` exists on every node's data type (`status: FlowNodeStatus`). Nothing else in the nodes changes.

- [ ] **Step 5: Run to verify the tests pass**

Run: `yarn test src/components/flow src/components/flow/nodes`
Expected: PASS, including the existing node tests. A test that does `getByText('✓ 200 ...')` or compares a card's `textContent` stays green because the icon is an svg with no text. If a node test counts `svg` elements in a header, update that count by one and say so in the commit body.

- [ ] **Step 6: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/nodes/NodeStatusIcon.tsx src/components/flow/nodes/AuthNode.tsx src/components/flow/nodes/IfNode.tsx src/components/flow/nodes/InputNode.tsx src/components/flow/nodes/OutputNode.tsx src/components/flow/nodes/RequestNode.tsx src/components/flow/nodes/SwitchNode.tsx src/components/flow/nodes/TransformNode.tsx src/components/flow/nodes/WaitForCallbackNode.tsx src/components/flow/nodes/__tests__/NodeStatusIcon.test.tsx src/components/flow/__tests__/FlowCanvas.a11y.test.tsx`
Suggested subject: `feat(flow): show node status as an icon as well as a colour`.

---

## Self-Review

- **Spec coverage (F-45):** `nodeStatusLabel` (Task 1); node `ariaLabel` of the form "label, kind node, status" with the short error and a kind-name fallback, edge `ariaLabel` with ends, field and taken state, `ariaLabelConfig`, canvas name and description (Task 1); `useFlowRunAnnouncer` with a baseline, "Run started", terminal node states only, a separate `role='alert'` region for failures, "Run finished: N succeeded, M failed, K skipped", and both regions always mounted in `FlowPane` (Task 2); lucide status icons `CheckCircle2`, `XCircle`, `Loader2`, `MinusCircle` with `aria-hidden` in all eight node headers (Task 3). The notes' "announce attempt n/m only occasionally" is met by never announcing progress at all.
- **Placeholders:** none. Every code step shows code. The eight node edits are one repeated two-line recipe because the headers differ only in markup that is not touched.
- **Type consistency:** `FlowNodeDetail` is the detail type in `flowNodeAriaLabel`, `diffAnnouncements`, the hook and the component. `AnnounceRunState` equals `FlowTab['runState']`. `EdgeRunState` comes from the existing `flowExits.ts`. The test ids `flow-announcer-status`, `flow-announcer-alert`, `node-status-icon` match between components and tests. The message texts ("Run started.", "Alpha succeeded.", "Run finished: 1 succeeded, 0 failed, 0 skipped.", "Alpha failed: boom.") match between `flowAnnounce.ts` and all three test files.
- **Review Focus coverage:** item 1 is the Ctrl+A-on-a-focused-node test plus the whole existing `FlowCanvas.test.tsx`; item 2 is the progress and error tests in `flowA11y.test.ts` and `FlowCanvas.a11y.test.tsx`; item 3 is the `FlowPane.announcer` tests that check the regions exist empty first and stay the same elements; item 4 is the baseline tests (hook and `FlowPane`), the no-`running` test and the over-five collapse test; item 5 is the `NodeStatusIcon` and all-eight-kinds canvas tests.
- **Deviations from the design notes:** no `role="application"` on the wrapper (React Flow already sets it), the description override targets both `node.a11yDescription` keys, and the description is a dedicated screen-reader paragraph instead of the mouse-oriented hint panel.
